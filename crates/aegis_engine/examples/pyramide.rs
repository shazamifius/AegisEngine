//! **LA DENSITÉ DOIT-ELLE SUIVRE LE REGARD ? — le calcul qui décide, sans GPU.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example pyramide --no-default-features
//! cargo run --release -p aegis_engine --example pyramide --no-default-features -- <fichier.glb>
//! ```
//!
//! ## ⚠⚠ CE BANC REMET EN CAUSE UN CHOIX JAMAIS EXAMINÉ, ET DU TRAVAIL DÉJÀ FAIT
//!
//! Le chantier 0.2 fait suivre la subdivision $k(T)$ à l'**aire à l'écran**. Ce choix vient de trois
//! sources qui convergent — FastAtlas, Split RC, OSC-GI — et il a été adopté parce que l'état de
//! l'art le fait.
//!
//! **Il a une conséquence que personne n'avait chiffrée** : la densité change dès que la caméra
//! bouge, donc il faut réallouer, donc il y a de la fragmentation, donc il faut compacter. *Tout
//! `render/placement.rs` — la free-list, les 9 classes, le compactage — n'existe que pour rattraper
//! ça.*
//!
//! ### Et ce choix contredit la thèse du projet
//!
//! **Une texture ne se réalloue pas quand on recule.** Sa résolution est décidée par l'auteur, une
//! fois ; ce qui varie avec la distance, c'est le **niveau qu'on lit** — le mipmap. Or la thèse
//! d'Aegis dit *« texture et shader sont la même chose »*, et son cadrage accepte explicitement le
//! coût d'auteur : *« même si pour les artistes c'est ignoble à utiliser, qu'il doive faire des
//! centaines de LOD manuels — et bien que ça se fasse comme ça »*.
//!
//! > ### 🔺 L'idée à chiffrer : découpler la densité ALLOUÉE (fixe, géométrique) de la densité
//! > ### CALCULÉE (variable, écran).
//!
//! La mémoire de surface devient une **pyramide** : l'adresse ne bouge plus jamais, et une pyramide
//! complète coûte $1 + \\frac14 + \\frac1{16} + \\cdots = \\frac43$ du niveau le plus fin. *Un nombre
//! qui se démontre, pas qui se règle.*
//!
//! ## ⭐ LE CRITÈRE, ÉCRIT AVANT LE CALCUL
//!
//! On compare **à qualité égale à l'écran** : la pyramide doit servir, au plus près, la même densité
//! que l'allocation adaptative sert.
//!
//! | Si la pyramide coûte… | Alors |
//! |---|---|
//! | **moins de 2×** l'adaptatif | ✅ elle tient telle quelle, et toute la plomberie d'allocation disparaît |
//! | **2× à 10×** | 🟡 elle tient, mais il faut de la **résidence par pages** — un mécanisme de plus, à taille fixe donc sans fragmentation |
//! | **plus de 10×** | ⛔ elle ne tient pas sur ce type de scène, et il faut le dire |
//!
//! ## ⚠ ET LA GRANDEUR QUI VA TOUT DÉCIDER
//!
//! Une pyramide géométrique donne aux objets **lointains** la densité qu'ils auraient **de près**.
//! Le surcoût est donc gouverné par **l'étendue en profondeur de la scène** — le rapport entre le
//! triangle le plus densément vu et le moins.
//!
//! *Sur une table vue de près, l'écart est faible. Sur un monde ouvert, il peut être énorme.* **Ce
//! banc mesure cet écart et le rend explicite, parce que c'est lui qui gouverne le verdict — pas la
//! scène de test qu'on lui donne.**

use aegis_engine::core::math::Vec3;
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::render::allocation::{aires_ecran, k_ideal, planifier, K_MAX};
use aegis_engine::render::surface::{micro_sommets, OCTETS_PAR_ENTREE};
use std::path::PathBuf;

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
const COTE: f32 = 900.0;
const BUDGET: u64 = 4_000_000;

fn main() {
    let chemin = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| racine_du_depot().join(MODELE_PAR_DEFAUT));

    titre("AEGIS — LA DENSITÉ DOIT-ELLE SUIVRE LE REGARD ?");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();
    let triangles = scene.indices.len() / 3;
    println!("  Scène   : {triangles} triangles\n");
    println!("  ⭐ CRITÈRE ÉCRIT AVANT, à qualité égale à l'écran :");
    println!("     · < 2×   → ✅ la pyramide tient, et toute la plomberie d'allocation disparaît");
    println!("     · 2–10×  → 🟡 elle tient avec de la résidence par pages (taille fixe, donc");
    println!("                   sans fragmentation)");
    println!("     · > 10×  → ⛔ elle ne tient pas sur ce type de scène");

    // ── Les deux aires de chaque triangle : dans le monde, et à l'écran ──────────────────────
    let vp = camera(&scene);
    let ecran = aires_ecran(&positions, &scene.indices, &vp, COTE, COTE);
    let monde: Vec<f32> = scene
        .indices
        .chunks_exact(3)
        .map(|t| {
            let a = Vec3::from_array(positions[t[0] as usize]);
            let b = Vec3::from_array(positions[t[1] as usize]);
            let c = Vec3::from_array(positions[t[2] as usize]);
            (b - a).cross(c - a).length() * 0.5
        })
        .collect();

    // ── ⭐⭐ L'ÉTENDUE EN PROFONDEUR — la grandeur qui gouverne tout le verdict ────────────────
    //
    // Le rapport « pixels par mètre carré » dit combien l'écran demande de densité à cet endroit.
    // Son ÉTENDUE sur la scène dit de combien la pyramide devra sur-servir les triangles lointains.
    let mut ratios: Vec<f32> = monde
        .iter()
        .zip(&ecran)
        .filter(|(m, e)| **m > 1e-9 && **e > 0.5)
        .map(|(m, e)| e / m)
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if ratios.is_empty() {
        println!("\n  ⛔ Aucun triangle visible : ce banc ne peut rien conclure sur cette scène.");
        return;
    }
    let q = |p: f32| ratios[((ratios.len() - 1) as f32 * p) as usize];
    let etendue = q(1.0) / q(0.0).max(f32::MIN_POSITIVE);

    titre("L'ÉTENDUE EN PROFONDEUR — ce qui gouverne le verdict");
    println!("  Densité demandée par l'écran, en pixels par m² de surface :");
    println!("  {:>12} {:>14} {:>14} {:>14}", "minimum", "médiane", "maximum", "étendue");
    println!("  {:>12.0} {:>14.0} {:>14.0} {:>13.0}×", q(0.0), q(0.5), q(1.0), etendue);
    println!();
    println!("  *C'est le rapport entre le triangle le plus densément vu et le moins. Une pyramide");
    println!("  géométrique sert à TOUS la densité du plus exigeant : c'est ce facteur qu'elle paie.*");

    // ── Régime A — l'allocation adaptative actuelle, pilotée par l'écran ─────────────────────
    let plan_ecran = planifier(&ecran, BUDGET);
    let octets_ecran = plan_ecran.octets();

    // ── Régime B — la pyramide géométrique ───────────────────────────────────────────────────
    //
    // ⚠ La densité cible se DÉRIVE, elle ne se règle pas : c'est celle qui sert, dans le monde, ce
    // que l'écran demande au triangle le plus exigeant. *Autrement la comparaison porterait sur un
    // réglage et pas sur une architecture.*
    let densite_cible = q(1.0);
    let k_pyramide: Vec<u32> = monde
        .iter()
        .map(|m| k_ideal(densite_cible * m).min(K_MAX))
        .collect();
    let fin: u64 = k_pyramide.iter().map(|k| micro_sommets(*k) as u64).sum();
    // La pyramide complète : chaque triangle porte tous ses niveaux, de k jusqu'à 0.
    let pyramide: u64 = k_pyramide
        .iter()
        .map(|k| (0..=*k).map(|n| micro_sommets(n) as u64).sum::<u64>())
        .sum();

    titre("LES TROIS RÉGIMES, À QUALITÉ ÉGALE À L'ÉCRAN");
    println!("  {:>34} {:>12} {:>10}", "régime", "mémoire", "contre A");
    for (nom, octets) in [
        ("A — adaptatif écran (l'actuel)", octets_ecran),
        ("B — pyramide géométrique, niveau fin", fin * OCTETS_PAR_ENTREE),
        ("B — pyramide complète (tous niveaux)", pyramide * OCTETS_PAR_ENTREE),
    ] {
        println!(
            "  {:>34} {:>10} Ko {:>9.2}×",
            nom,
            octets / 1024,
            octets as f64 / octets_ecran.max(1) as f64
        );
    }
    println!();
    println!("  ⭐ Le rapport pyramide/fin doit valoir 4/3 = 1,333… — la somme 1 + ¼ + 1/16 + …");
    println!("     Mesuré : {:.4}×. *Un nombre qui se démontre, pas qui se règle.*",
        pyramide as f64 / fin.max(1) as f64);

    titre("LE VERDICT — contre le critère écrit AVANT le calcul");
    let facteur = pyramide as f64 * OCTETS_PAR_ENTREE as f64 / octets_ecran.max(1) as f64;
    println!("  La pyramide complète coûte {facteur:.2}× l'allocation adaptative.\n");
    if facteur < 2.0 {
        println!("  ⇒ ✅ ELLE TIENT TELLE QUELLE. L'adresse cesse de bouger pour toujours, et avec");
        println!("     elle disparaissent : l'allocateur et ses 9 classes, le compactage et son");
        println!("     seuil, la fragmentation, et la moitié de la liste de travail.");
        println!("     *Une constante qui DISPARAÎT au lieu de rétrécir.*");
    } else if facteur < 10.0 {
        println!("  ⇒ 🟡 ELLE TIENT, MAIS PAS RÉSIDENTE EN ENTIER. Il faut une résidence par");
        println!("     PAGES — de taille fixe, donc sans fragmentation, contrairement à");
        println!("     l'allocateur actuel. *L'adresse reste stable dans un espace virtuel ;");
        println!("     seule la présence en mémoire varie.*");
    } else {
        println!("  ⇒ ⛔ ELLE NE TIENT PAS sur ce type de scène : {facteur:.1}× est hors de portée.");
        println!("     *La densité géométrique sur-sert massivement les objets lointains, et");
        println!("     l'étendue en profondeur mesurée ci-dessus dit pourquoi.*");
    }

    titre("⚠ CE QUE CE CALCUL NE DIT PAS");
    println!("  · **Rien sur un monde OUVERT.** Cette scène tient dans une pièce ; son étendue en");
    println!("    profondeur est de {etendue:.0}×. Un paysage en aurait des milliers, et le verdict");
    println!("    ci-dessus basculerait. *C'est la limite la plus sérieuse de ce banc, et elle ne");
    println!("    se lève qu'avec une scène étendue — que le projet n'a pas encore.*");
    println!("  · Rien sur le COÛT EN TEMPS : une pyramide se met à jour par niveaux, et ce que ça");
    println!("    coûte n'est pas ici.");
    println!("  · Rien sur la QUALITÉ perçue : servir la densité du plus proche à tout le monde");
    println!("    change le filtrage, et un œil tranche ça mieux qu'un rapport d'octets.");
}

fn camera(scene: &Scene) -> [f32; 16] {
    let (centre, rayon) = boite(scene);
    let fov = 55_f32.to_radians();
    let recul = rayon * 1.3 / (fov * 0.5).tan();
    let oeil = centre + Vec3::new(0.55, 0.40, -0.73).normalize() * recul;
    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, centre, 1.0);
    camera.fov_y_radians = fov;
    camera.z_near = (recul - rayon * 1.3).max(rayon * 0.01);
    camera.z_far = recul + rayon * 2.6;
    let m = camera.compute_projection_matrix() * camera.compute_view_matrix();
    let c = m.to_cols_array_2d();
    let mut sortie = [0.0f32; 16];
    for (i, col) in c.iter().enumerate() {
        sortie[i * 4..i * 4 + 4].copy_from_slice(col);
    }
    sortie
}

fn boite(scene: &Scene) -> (Vec3, f32) {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for s in &scene.sommets {
        let p = Vec3::from_array(s.position);
        min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    ((min + max) * 0.5, ((max - min) * 0.5).length().max(1e-3))
}

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
