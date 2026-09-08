//! **CE QUI SURVIT D'UNE IMAGE À LA SUIVANTE — la mesure qui décide si la persistance est possible.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example persistance --no-default-features
//! ```
//!
//! ## La question, et pourquoi elle n'avait jamais été posée
//!
//! `02-THESE.md` promet qu'une texture est *« l'état d'une surface »* et un shader *« la loi qui
//! fait évoluer cet état »* — **une mémoire et sa dérivée**. Cette ligne suppose qu'il existe un
//! état qui **survit** à l'image précédente. Or l'étage 0 réécrit toute la mémoire de surface à
//! chaque passe.
//!
//! On pourrait croire que c'est un simple choix de banc, et qu'il suffirait de ne réécrire que ce
//! qui a changé. **Ce banc met cette croyance à l'épreuve**, parce que le chantier 0.2 a introduit
//! une dépendance que rien n'a encore chiffrée :
//!
//! > `k(T) = clamp(k_idéal(**aire à l'écran**) + biais, …)` — **la subdivision d'un triangle dépend
//! > de la caméra.**
//!
//! Et l'adresse d'une entrée vaut `base(T) + rang`, où les bases sont **cumulatives** :
//! `base += micro_sommets(k)`. *Un seul triangle qui change de subdivision décale donc toutes les
//! bases qui le suivent.* Le corpus ne dit nulle part ce que ça coûte.
//!
//! ## ⭐ LE CRITÈRE, ÉCRIT AVANT LA MESURE
//!
//! On mesure la fraction de la mémoire de surface qui **cesse d'être valide** entre deux images
//! consécutives, à 72 Hz, pour des mouvements de caméra réalistes en casque.
//!
//! | Si la fraction invalidée par image est… | Alors |
//! |---|---|
//! | **< 1 %** | la persistance est un chantier LOCAL : on réécrit ce qui a bougé, et la thèse tient telle qu'elle est écrite |
//! | **1 % à 10 %** | la persistance exige d'abord une allocation STABILISÉE — c'est un chantier à part, à faire AVANT |
//! | **≥ 10 %** | l'allocation pilotée par l'écran est, en l'état, incompatible avec une mémoire persistante. *Il faudrait changer l'ALLOCATION, pas ajouter la persistance.* |
//!
//! ## ⚠⚠ LE TÉMOIN — et sans lui cette mesure ne vaudrait rien
//!
//! La même mesure est faite **caméra immobile**. Elle doit rendre **exactement zéro** sur les trois
//! grandeurs. *Si elle ne rend pas zéro, ce banc ne mesure pas le mouvement de la caméra mais le
//! bruit de la planification, et aucun de ses chiffres n'est interprétable.*
//!
//! C'est la leçon du 6 septembre 2026, apprise sur la mesure de couture : *un instrument saturé
//! répond sur la bonne grandeur, noyée dans autre chose — et il rend un chiffre stable et
//! plausible.*
//!
//! ## ⭐⭐ LE SECOND RÉGIME — l'adresse STABLE, et son critère écrit AVANT
//!
//! Depuis le 8 septembre, [`Placement`] donne à chaque triangle un bloc qui **ne bouge pas** tant
//! que sa subdivision ne bouge pas — une free-list par classe de taille, le patron que *Shading
//! Atlas Streaming* appelle *« superblock scheme inspired by memory management »*.
//!
//! Ce banc mesure donc **les deux régimes côte à côte**, sur exactement la même séquence.
//!
//! | | Le critère, écrit avant |
//! |---|---|
//! | **le travail réécrit** | doit tomber **sous 5 %** par image en mouvement ordinaire, contre ~97 % en cumulatif. *Sinon l'adresse stable ne sert à rien et il faut le dire.* |
//! | **la fragmentation** | ne doit pas dépasser **2× l'empreinte utile**. ⚠ C'est le critère d'invalidation de `02-THESE.md` — *« une empreinte non bornée »* — et sans coalescence, le risque est réel |
//! | **le témoin** | caméra immobile : **0 %** réécrit après la première image, et fragmentation exactement **1,00** |
//!
//! ## Ce que ce banc ne prouve PAS
//!
//! - **Rien sur le coût réel d'une réécriture partielle.** Il compte des entrées invalidées, pas des
//!   millisecondes. *Une fraction faible ne dit pas que le mécanisme sera bon marché : un dispatch
//!   creux coûte son lancement.*
//! - **Rien sur une vraie scène de jeu.** Il tourne sur la table de test, 3 274 triangles, une seule
//!   pièce. Une scène ouverte avec du relief lointain n'a aucune raison de se comporter pareil.
//! - **Rien sur un Adreno 650**, comme tout le reste de ce corpus.

use aegis_engine::core::math::Vec3;
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::render::allocation::{aires_ecran, planifier, raccorder, Plan};
use aegis_engine::render::placement::Placement;
use aegis_engine::render::surface::micro_sommets;
use std::path::PathBuf;

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
/// Le côté de l'écran simulé, en pixels — le même que le banc `lire_surface`.
const COTE: f32 = 900.0;
/// La cadence de référence du Quest 2.
const HZ: f32 = 72.0;
/// Le nombre d'images consécutives simulées par mouvement.
const IMAGES: usize = 24;

/// Un mouvement de caméra : sa vitesse de rotation en degrés par seconde, et sa vitesse
/// d'avancement en mètres par seconde.
struct Mouvement {
    nom: &'static str,
    degres_par_seconde: f32,
    metres_par_seconde: f32,
    /// Ce que ce mouvement représente, pour qu'un lecteur puisse juger s'il est réaliste.
    justification: &'static str,
    /// ⚠⚠ **Ce drapeau est DÉCLARÉ, il ne se déduit pas des vitesses — et c'est une mutation qui
    /// l'a exigé.**
    ///
    /// La première version reconnaissait le témoin à `vitesse == 0`. En le faisant tourner exprès
    /// pour vérifier que la garde savait tomber, il a cessé d'être reconnu **comme témoin** : la
    /// vérification ne s'appliquait alors à rien, et le banc annonçait « témoin nul » sur une ligne
    /// qui déplaçait 19,9 % de la mémoire. *Une garde qui se désarme précisément quand la condition
    /// qu'elle surveille change est une garde décorative — et elle passe toujours.*
    temoin: bool,
}

/// Ce qui a changé entre deux images.
#[derive(Default, Clone, Copy)]
struct Delta {
    /// Le biais global a changé — dans ce cas **tout** le plan bouge d'un coup.
    biais_change: bool,
    /// Entrées dont le CONTENU doit être recalculé : leur triangle a changé de subdivision, donc
    /// leurs coordonnées barycentriques ne sont plus les mêmes.
    recalcul: u64,
    /// Entrées de BORD à recalculer : la subdivision du triangle n'a pas bougé, mais le niveau
    /// effectif d'une de ses arêtes si — le raccord décime alors un bord différent.
    bord: u64,
    /// Entrées dont la valeur reste juste mais dont l'ADRESSE a bougé, parce que les bases sont
    /// cumulatives et qu'un triangle en amont a changé de taille.
    deplacees: u64,
    /// Le total des entrées de l'image.
    total: u64,
    /// Combien de TRIANGLES ont changé de subdivision.
    ///
    /// ⭐ C'est ce compteur qui rend la cascade visible au lieu de la laisser déduire : si trois
    /// triangles sur trois mille suffisent à déplacer 97 % de la mémoire, alors ce n'est pas le
    /// mouvement qui coûte — c'est le fait que les bases soient **cumulatives**.
    triangles_k: u32,
    /// Le rang du premier triangle qui change. *Tout ce qui le suit voit sa base décalée.*
    premier: Option<u32>,
}

impl Delta {
    /// La fraction de la mémoire qui cesse d'être valide — ce que le critère juge.
    fn invalide(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.recalcul + self.bord) as f64 / self.total as f64
    }

    /// La fraction qui reste juste mais doit être déplacée.
    fn a_deplacer(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.deplacees as f64 / self.total as f64
    }
}

fn main() {
    let chemin = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| racine_du_depot().join(MODELE_PAR_DEFAUT));

    titre("AEGIS — CE QUI SURVIT D'UNE IMAGE À LA SUIVANTE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    let triangles = scene.indices.len() / 3;
    println!("  Scène   : {triangles} triangles · écran simulé {COTE} × {COTE} · {HZ} Hz");
    println!("  Budget  : 4 Mo et 16 Mo — les deux du banc `lire_surface` au-dessus du plancher.\n");

    println!("  ⭐ CRITÈRE ÉCRIT AVANT : < 1 % invalidé par image = chantier local ·");
    println!("     1–10 % = il faut stabiliser l'allocation d'abord · ≥ 10 % = l'allocation");
    println!("     pilotée par l'écran est incompatible avec une mémoire persistante.");
    println!("  ⚠ TÉMOIN : caméra immobile, qui DOIT rendre exactement zéro.");

    let mouvements = [
        Mouvement {
            nom: "immobile (TÉMOIN)",
            degres_par_seconde: 0.0,
            metres_par_seconde: 0.0,
            justification: "rien ne bouge — doit rendre zéro, sinon la mesure ne mesure rien",
            temoin: true,
        },
        Mouvement {
            nom: "tête, lente",
            degres_par_seconde: 30.0,
            metres_par_seconde: 0.0,
            justification: "un regard qui suit quelqu'un dans une pièce",
            temoin: false,
        },
        Mouvement {
            nom: "tête, vive",
            degres_par_seconde: 120.0,
            metres_par_seconde: 0.0,
            justification: "un coup d'œil par-dessus l'épaule",
            temoin: false,
        },
        Mouvement {
            nom: "marche",
            degres_par_seconde: 0.0,
            metres_par_seconde: 1.4,
            justification: "la vitesse de marche d'un adulte",
            temoin: false,
        },
        Mouvement {
            nom: "marche + tête",
            degres_par_seconde: 30.0,
            metres_par_seconde: 1.4,
            justification: "le cas ordinaire — on marche en regardant autour de soi",
            temoin: false,
        },
    ];

    let mut temoin_propre = true;
    for budget in [4_000_000u64, 16_000_000] {
        titre(&format!("BUDGET {} Mo", budget / 1_000_000));
        println!(
            "  {:>20} {:>8} {:>9} {:>12} {:>12} {:>10}",
            "mouvement", "biais≠", "tri. k≠", "invalidé/img", "à déplacer", "pire img"
        );
        println!("  {:>20} {:>8} {:>9} {:>12} {:>12} {:>10}", "", "", "", "", "▼ STABLE", "frag.");

        for m in &mouvements {
            let deltas = simuler(&scene, budget, m);
            let n = deltas.len().max(1) as f64;
            let moyen = deltas.iter().map(|d| d.invalide()).sum::<f64>() / n;
            let deplace = deltas.iter().map(|d| d.a_deplacer()).sum::<f64>() / n;
            let pire = deltas.iter().map(|d| d.invalide()).fold(0.0f64, f64::max);
            let biais = deltas.iter().filter(|d| d.biais_change).count();
            let tri_k = deltas.iter().map(|d| d.triangles_k as f64).sum::<f64>() / n;

            println!(
                "  {:>20} {:>8} {:>9.1} {:>11.2}% {:>11.2}% {:>9.2}%",
                m.nom,
                biais,
                tri_k,
                moyen * 100.0,
                deplace * 100.0,
                pire * 100.0
            );

            // ⭐ Le même mouvement, adresse stable.
            let (stable_moyen, stable_pire, frag) = simuler_stable(&scene, budget, m);
            println!(
                "  {:>20} {:>8} {:>9} {:>11.2}% {:>11.2}% {:>9.2}×",
                "└─ adresse stable", "", "", stable_moyen * 100.0, stable_pire * 100.0, frag
            );

            if m.temoin {
                if deltas.iter().any(|d| d.recalcul + d.bord + d.deplacees > 0) {
                    temoin_propre = false;
                }
                if stable_moyen > 0.0 || (frag - 1.0).abs() > 1e-9 {
                    temoin_propre = false;
                }
            }
        }
        println!();
        for m in &mouvements {
            println!("  · {:<20} — {}", m.nom, m.justification);
        }
    }

    titre("CE QUE ÇA DIT");
    if !temoin_propre {
        println!("  ⛔ LE TÉMOIN N'EST PAS NUL. Une caméra immobile produit un plan qui change,");
        println!("     donc ce banc ne mesure pas le mouvement — il mesure son propre bruit.");
        println!("     ⇒ Ne rien conclure des chiffres ci-dessus tant que ce n'est pas fermé.");
        return;
    }
    println!("  ✅ Le témoin est nul : caméra immobile, plan identique au bit près.");
    println!("     L'instrument sait donc distinguer une absence de changement d'un changement.");

    // ⭐ LA CASCADE, montrée plutôt que déduite — sur le mouvement le plus ordinaire.
    let ordinaire = Mouvement {
        nom: "marche + tête",
        degres_par_seconde: 30.0,
        metres_par_seconde: 1.4,
        justification: "",
        temoin: false,
    };
    let deltas = simuler(&scene, 4_000_000, &ordinaire);
    let n = deltas.len().max(1) as f64;
    let tri_k = deltas.iter().map(|d| d.triangles_k as f64).sum::<f64>() / n;
    let deplace = deltas.iter().map(|d| d.a_deplacer()).sum::<f64>() / n;
    let premier = deltas.iter().filter_map(|d| d.premier).min().unwrap_or(0);

    println!();
    println!("  ⭐⭐ LA CASCADE — et c'est le vrai résultat de ce banc.");
    println!("     En marchant et en tournant la tête, **{tri_k:.1} triangles sur {triangles}** changent");
    println!("     de subdivision par image — soit {:.3} % du maillage. Et pourtant", tri_k / triangles as f64 * 100.0);
    println!("     **{:.1} % de la mémoire doit être déplacée**, parce que `base += micro_sommets(k)`", deplace * 100.0);
    println!("     est CUMULATIF : le premier triangle qui change (rang {premier}) décale l'adresse de");
    println!("     tous ceux qui le suivent.");
    println!();
    println!("     ⇒ Ce qui empêche la persistance n'est donc PAS le mouvement de la caméra :");
    println!("       c'est la forme de l'adresse. *Le contenu qui cesse d'être vrai tient dans");
    println!("       {:.2} % ; le reste est de la copie que rien n'oblige à faire.*", deltas.iter().map(|d| d.invalide()).sum::<f64>() / n * 100.0);

    println!();
    fragmentation_longue(&scene, 4_000_000);

    titre("CE QUE ÇA NE DIT PAS");
    println!("  ⚠ Ce banc ne dit RIEN du coût en millisecondes d'une réécriture partielle, ni");
    println!("    d'une vraie scène ouverte. Il dit ce qui CESSE D'ÊTRE VALIDE, et rien de plus.");
}

/// Le second régime : le même mouvement, mais avec un placement à adresse stable.
///
/// ## ⚠ Ce qui est compté comme réécrit, et pourquoi les deux termes comptent
///
/// 1. **Les triangles relogés** — leur subdivision a changé, donc leur bloc aussi : tout leur
///    contenu est à recalculer. *C'est irréductible : ça ne dépend pas de l'adressage.*
/// 2. **Les bords dont le raccord a changé** — un triangle peut garder sa place ET sa subdivision,
///    et voir le niveau effectif d'une de ses arêtes bouger parce qu'un **voisin** a changé.
///    *L'oublier ferait annoncer un gain trop beau, et l'image montrerait une couture.*
fn simuler_stable(scene: &Scene, budget: u64, m: &Mouvement) -> (f64, f64, f64) {
    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();
    let triangles = scene.indices.len() / 3;
    let mut placement = Placement::nouveau(triangles);
    let mut aretes_avant: Option<Vec<[u32; 3]>> = None;
    let (mut somme, mut pire, mut frag) = (0.0f64, 0.0f64, 1.0f64);
    let mut mesures = 0usize;

    for image in 0..IMAGES {
        let t = image as f32 / HZ;
        let vp = camera_a(scene, m.degres_par_seconde * t, m.metres_par_seconde * t);
        let aires = aires_ecran(&positions, &scene.indices, &vp, COTE, COTE);
        let modele = planifier(&aires, budget);
        let k_voulu: Vec<u32> = modele.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();

        let deplacement = placement.mettre_a_jour(&k_voulu, budget);
        let mut plan = placement.appliquer(&modele);
        raccorder(&mut plan, &positions, &scene.indices);

        // La première image ne mesure rien : tout y est neuf par construction.
        if let Some(avant) = &aretes_avant {
            let relogés: std::collections::HashSet<u32> = deplacement.relogés.iter().copied().collect();
            let mut bord = 0u64;
            for (t, aretes_avant) in avant.iter().enumerate() {
                if !relogés.contains(&(t as u32)) && *aretes_avant != plan.aretes[t] {
                    bord += (3 * plan.par_triangle[t].1) as u64;
                }
            }
            let part = (deplacement.entrees_relogées + bord) as f64
                / deplacement.entrees_totales.max(1) as f64;
            somme += part;
            pire = pire.max(part);
            mesures += 1;
        }
        frag = frag.max(placement.fragmentation());
        aretes_avant = Some(plan.aretes.clone());
    }
    (somme / mesures.max(1) as f64, pire, frag)
}

/// Rejoue `IMAGES` images consécutives d'un mouvement et rend le delta de chaque paire.
fn simuler(scene: &Scene, budget: u64, m: &Mouvement) -> Vec<Delta> {
    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();
    let mut plans: Vec<Plan> = Vec::with_capacity(IMAGES);

    for image in 0..IMAGES {
        let t = image as f32 / HZ;
        let vp = camera_a(scene, m.degres_par_seconde * t, m.metres_par_seconde * t);
        let aires = aires_ecran(&positions, &scene.indices, &vp, COTE, COTE);
        let mut plan = planifier(&aires, budget);
        // ⚠ Le raccord fait partie du plan livré : le mesurer sans lui décrirait une configuration
        // que le rendu n'emploie pas. *Une mesure faite sur autre chose que ce qu'on livre rassure
        // sans rien prouver.*
        raccorder(&mut plan, &positions, &scene.indices);
        plans.push(plan);
    }

    plans.windows(2).map(|p| comparer(&p[0], &p[1])).collect()
}

/// Ce qui sépare deux plans consécutifs, entrée par entrée.
///
/// ## Les trois cas, et ils ne coûtent pas la même chose
///
/// 1. **La subdivision du triangle a changé** — ses micro-sommets ne sont plus aux mêmes
///    coordonnées barycentriques. Tout son contenu est à recalculer, il n'y a rien à sauver.
/// 2. **La subdivision est la même, mais le niveau effectif d'une arête a changé** — le raccord
///    décime un bord différent. Seuls les micro-sommets du BORD sont faux ; l'intérieur reste juste.
/// 3. **Tout est identique sauf la base** — le contenu reste exact, mais il n'est plus à la bonne
///    adresse, parce qu'un triangle en amont a changé de taille et que les bases sont cumulatives.
///    *C'est une copie, pas un recalcul — mais ce n'est pas gratuit.*
fn comparer(avant: &Plan, apres: &Plan) -> Delta {
    let mut d = Delta { biais_change: avant.biais != apres.biais, total: apres.entrees as u64, ..Default::default() };

    for t in 0..apres.par_triangle.len().min(avant.par_triangle.len()) {
        let (base_a, cote_a) = avant.par_triangle[t];
        let (base_b, cote_b) = apres.par_triangle[t];
        let entrees = micro_sommets(cote_b.trailing_zeros()) as u64;

        if cote_a != cote_b {
            d.recalcul += entrees;
            d.triangles_k += 1;
            d.premier = d.premier.or(Some(t as u32));
        } else if avant.aretes[t] != apres.aretes[t] {
            // Les micro-sommets du bord : 3n pour n segments par arête (les trois coins sont
            // partagés, d'où 3n et non 3(n+1)).
            d.bord += (3 * cote_b) as u64;
            if base_a != base_b {
                d.deplacees += entrees - (3 * cote_b) as u64;
            }
        } else if base_a != base_b {
            d.deplacees += entrees;
        }
    }
    d
}

/// La caméra du banc `lire_surface`, tournée de `angle` degrés et avancée de `avance` mètres.
///
/// ⚠ **La rotation fait tourner le REGARD autour de l'œil**, pas l'œil autour de la scène : c'est
/// ce que fait une tête dans un casque. *Faire orbiter l'œil mesurerait un travelling, qui n'est
/// pas le mouvement dominant en VR.*
fn camera_a(scene: &Scene, angle_degres: f32, avance_metres: f32) -> [f32; 16] {
    let (centre, rayon) = boite_englobante(scene);
    let fov = 55_f32.to_radians();
    let recul = rayon * 1.3 / (fov * 0.5).tan();
    let direction = Vec3::new(0.55, 0.40, -0.73).normalize();
    let oeil_base = centre + direction * recul;
    // On avance vers la scène, donc dans le sens opposé au recul.
    let oeil = oeil_base - direction * avance_metres;

    // La direction du regard, tournée autour de l'axe vertical.
    let vue = (centre - oeil_base).normalize();
    let a = angle_degres.to_radians();
    let (s, c) = a.sin_cos();
    let vue_tournee = Vec3::new(vue.x * c + vue.z * s, vue.y, -vue.x * s + vue.z * c);

    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, oeil + vue_tournee * recul, 1.0);
    camera.fov_y_radians = fov;
    camera.z_near = (recul - rayon * 1.3).max(rayon * 0.01);
    camera.z_far = recul + rayon * 2.6;
    aplatir(&(camera.compute_projection_matrix() * camera.compute_view_matrix()))
}

fn boite_englobante(scene: &Scene) -> (Vec3, f32) {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for s in &scene.sommets {
        let p = Vec3::from_array(s.position);
        min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    let centre = (min + max) * 0.5;
    let rayon = ((max - min) * 0.5).length().max(1e-3);
    (centre, rayon)
}

fn aplatir(m: &aegis_engine::core::math::Mat4) -> [f32; 16] {
    let c = m.to_cols_array_2d();
    let mut sortie = [0.0f32; 16];
    for (i, col) in c.iter().enumerate() {
        sortie[i * 4..i * 4 + 4].copy_from_slice(col);
    }
    sortie
}

/// ⚠⚠ **LA FRAGMENTATION SUR LA DURÉE — le seul point qui peut invalider l'adresse stable.**
///
/// `02-THESE.md` écrit noir sur blanc : *« ce qui invaliderait : une empreinte NON BORNÉE »*.
///
/// Sans coalescence, un bloc rendu par un triangle de classe $k$ ne resservira **qu'à** un futur
/// triangle de classe $k$. L'empreinte réservée est donc le cumul des **pics par classe**, et elle
/// dépasse l'empreinte utile. *Le brassage du banc `lire_surface` l'a mesurée à **1,82×** — bien
/// au-delà des 1,05× que rendait une séquence douce, et tout près du plafond de 2×.*
///
/// **La question n'est donc pas « y a-t-il de la fragmentation » — il y en a — mais : PLAFONNE-T-ELLE ?**
///
/// ## ⚠⚠ LE CRITÈRE A DÛ ÊTRE REMPLACÉ, ET IL FAUT DIRE POURQUOI
///
/// **Ce qu'il disait d'abord :** *« la fragmentation doit PLAFONNER — les 100 dernières images à
/// moins de 5 % au-dessus des 100 du milieu »*. Il était juste pour une free-list seule, et il a
/// rendu son verdict : **⛔ 2,65× au milieu, 3,90× à la fin, 5,26× au pire, +47 %.** C'est ce refus
/// qui a fait naître le compactage.
///
/// **Pourquoi il ne vaut plus rien depuis :** avec un compactage, la fragmentation monte, retombe à
/// exactement 1,00, remonte — *une dent de scie n'a pas de tendance.* Comparer deux fenêtres y
/// mesure la **phase** de l'oscillation, pas une dérive. Le critère ne serait pas trop sévère : il
/// répondrait à côté.
///
/// *Ce n'est donc pas un seuil assoupli parce qu'il gênait — c'est une grandeur qui a cessé d'être
/// la bonne quand le mécanisme a changé. Le distinguer est tout ce qui sépare une correction d'un
/// arrangement.*
///
/// ## Le critère qui le remplace
///
/// Sur 600 images d'une caméra qui balaie dans les deux sens, avance et recule :
///
/// - **l'empreinte ne dépasse jamais le budget** tant que la densité, elle, y tient. *C'est ça,
///   « bornée », et c'est vrai par construction : c'est le dépassement qui déclenche sa remise à
///   plat.*
/// - **le coût amorti**, compactages compris, doit rester **sous 5 %** par image — contre 96,25 %
///   pour l'adressage cumulatif. *Si le compactage se déclenche si souvent qu'il mange le gain,
///   l'adresse stable ne sert à rien et il faut le dire.*
///
/// ⚠ **La borne théorique existe mais elle est inutilisable** : le nombre de triangles est fini,
/// donc l'empreinte l'est aussi — à $\sum_k N \cdot \mathrm{taille}(k)$, soit ~145 M entrées ici.
/// *Une borne qu'on ne peut pas payer n'est pas une borne ; seul le comportement réel décide.*
fn fragmentation_longue(scene: &Scene, budget: u64) {
    titre("LA FRAGMENTATION SUR LA DURÉE — l'empreinte est-elle bornée ?");
    println!("  Critère écrit AVANT : l'empreinte ne dépasse jamais le budget, ET le coût amorti");
    println!("  (compactages compris) reste sous 5 % par image — contre 96,25 % en cumulatif.");
    println!("  ⚠ Le critère précédent portait sur la fragmentation ; il a rendu ⛔ (5,26× et");
    println!("    croissante), c'est ce qui a fait naître le compactage — et il a cessé d'être");
    println!("    interprétable avec lui : une dent de scie n'a pas de tendance.\n");

    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();
    let triangles = scene.indices.len() / 3;
    let mut placement = Placement::nouveau(triangles);
    let mut suite = Vec::with_capacity(600);
    let mut travail = 0.0f64;
    let mut depassements = 0usize;

    for image in 0..600usize {
        let t = image as f32 / HZ;
        // Un mouvement volontairement AGITÉ : deux rotations de périodes incommensurables plus un
        // va-et-vient. *Une caméra qui tourne toujours dans le même sens finirait par revenir sur
        // ses pas et flatterait la mesure.*
        let angle = 60.0 * (t * 0.7).sin() + 25.0 * (t * 1.9).sin();
        let avance = rayon_scene(scene) * 0.4 * (t * 0.5).sin();
        let vp = camera_a(scene, angle, avance);
        let aires = aires_ecran(&positions, &scene.indices, &vp, COTE, COTE);
        let modele = planifier(&aires, budget);
        let k: Vec<u32> = modele.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();
        let d = placement.mettre_a_jour(&k, budget);
        suite.push(placement.fragmentation());
        travail += d.entrees_relogées as f64 / d.entrees_totales.max(1) as f64;
        // ⚠ Un dépassement ne compte QUE si la densité, elle, tenait dans le budget : sinon c'est
        // le biais qui n'a pas su descendre, et ce n'est pas le procès du placement.
        if placement.empreinte() > budget && placement.utile() <= budget {
            depassements += 1;
        }
    }

    let moyenne = |t: &[f64]| t.iter().sum::<f64>() / t.len() as f64;
    let milieu = moyenne(&suite[250..350]);
    let fin = moyenne(&suite[500..600]);
    let pire = suite.iter().cloned().fold(0.0f64, f64::max);

    println!("  {:>14} {:>12} {:>12} {:>10}", "images 250-350", "images 500-600", "pire", "croissance");
    println!("  {:>14.3}× {:>11.3}× {:>11.3}× {:>9.1} %", milieu, fin, pire, (fin / milieu - 1.0) * 100.0);
    println!();
    println!("  ⭐ LE COÛT AMORTI, compactages compris — c'est LUI qui décide :");
    println!("     compactages : {} sur 600 images (1 toutes les {:.0} images)",
        placement.compactages, 600.0 / placement.compactages.max(1) as f64);
    println!("     travail moyen réécrit par image : {:.2} %", travail / 600.0 * 100.0);
    println!("     à comparer aux 96,25 % de l'adressage cumulatif.");
    println!();

    let amorti = travail / 600.0;
    let tient = depassements == 0;
    if tient && amorti < 0.05 {
        println!("  ⇒ ✅ L'EMPREINTE EST BORNÉE, et le coût amorti tient : {:.2} % par image contre", amorti * 100.0);
        println!("     96,25 % en cumulatif — un facteur {:.0}, compactages compris.", 0.9625 / amorti.max(1e-9));
        println!("     *Le critère d'invalidation de `02-THESE.md` tient, et il se paie en");
        println!("     compactages rares plutôt qu'en copie permanente.*");
    } else if !tient {
        println!("  ⇒ ⛔ L'EMPREINTE DÉPASSE LE BUDGET sur {depassements} image(s) alors que la");
        println!("     densité y tenait. Le compactage ne fait pas son travail. *Ne rien conclure.*");
    } else {
        println!("  ⇒ ⛔ LE COÛT AMORTI EST DE {:.2} %, au-dessus des 5 % du critère : le", amorti * 100.0);
        println!("     compactage se déclenche trop souvent et mange le gain. *L'adresse stable");
        println!("     ne se justifie pas dans cet état.*");
    }
}

/// Le rayon de la scène — extrait pour que le mouvement long soit à l'échelle du modèle chargé.
fn rayon_scene(scene: &Scene) -> f32 {
    boite_englobante(scene).1
}

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
