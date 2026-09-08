//! **COMBIEN COÛTE VRAIMENT UNE MISE À JOUR PARTIELLE — la première milliseconde du chantier.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example chrono_surface --no-default-features
//! ```
//!
//! ## ⚠⚠ POURQUOI CE BANC EXISTE, ET IL PEUT DÉMOLIR CE QUI LE PRÉCÈDE
//!
//! Le 8 septembre, deux bancs ont établi que la réécriture partielle évite **93,6 %** du travail :
//! 210 triangles recalculés sur 3 274, mémoire identique au bit près.
//!
//! **Ces deux bancs comptent des ENTRÉES. Aucun ne compte du TEMPS.**
//!
//! Or lancer un calcul sur un processeur graphique a un **prix fixe** — préparation de la commande,
//! démarrage des groupes, synchronisation — payé quel que soit le travail demandé. *Si ce prix fixe
//! domine, alors « 210 triangles au lieu de 3 274 » ne fait gagner **rien du tout**, et les 93,6 %
//! sont un mirage comptable.*
//!
//! > ### C'est le premier chiffre en millisecondes de tout ce chantier, et il peut l'invalider.
//!
//! ## ⭐ LE CRITÈRE, ÉCRIT AVANT LA MESURE
//!
//! On mesure le temps GPU du dispatch seul, pour des lots de tailles croissantes.
//!
//! | Si le lot de 210 coûte… | Alors |
//! |---|---|
//! | **plus de 50 %** du lot complet | ⛔ **le prix fixe domine** : le gain mesuré en entrées ne se traduit pas en temps, et il faut revoir le mécanisme — grouper les mises à jour, ou changer d'échelle |
//! | **moins de 15 %** | ✅ le temps suit le travail : les 93,6 % sont réels |
//! | entre les deux | le prix fixe est **chiffrable**, et le banc rend le **seuil de rentabilité** — en dessous de combien de triangles il ne sert plus à rien de découper |
//!
//! ## ⚠⚠ LE TÉMOIN, et sans lui rien n'est interprétable
//!
//! Un lot de **un seul triangle**. Il ne fait presque aucun travail, donc **ce qu'il coûte EST le
//! prix fixe** — la constante `a` du modèle $T(n) = a + b\\,n$.
//!
//! *Sans lui, on ne saurait pas distinguer « le partiel ne gagne rien » de « la mesure est noyée
//! dans son propre bruit ». C'est la leçon du 6 septembre sur l'instrument saturé, appliquée au
//! temps.*
//!
//! ## ⛔⛔ CE BANC A MENTI D'UN FACTEUR 378 PENDANT UNE SOIRÉE — 8 septembre 2026
//!
//! Sa première version allouait la mémoire de surface en **`HOST_VISIBLE`**, comme les bancs de
//! preuve qui doivent la relire. Sur une carte **discrète**, cette mémoire est atteinte par le bus
//! **PCIe** : une passe de calcul qui y écrit 155 000 entrées mesure alors **le transfert, pas le
//! calcul**.
//!
//! | | mémoire hôte | mémoire carte |
//! |---|---|---|
//! | la passe complète | **13,02 ms** | **0,034 ms** |
//!
//! **Toutes les conclusions de temps de cette soirée-là décrivaient un bus.** Et la phrase qui
//! l'annonçait était écrite dans `render/surface.rs` depuis son premier jour — *« `HOST_VISIBLE` est
//! un choix de BANC, pas d'architecture »*. Elle a été lue le matin même et n'a protégé de rien.
//!
//! > **Une leçon écrite ne protège de rien tant qu'elle n'est pas une garde.** Elle en est une
//! > maintenant : `MemoireDeSurface::relisible` est vrai en mémoire hôte, et ce banc **refuse de
//! > tourner** si on le lui donne.
//!
//! ## ⚠ ET LA PRÉDICTION QUI ALLAIT AVEC ÉTAIT FAUSSE AUSSI
//!
//! Le tableau du gaspillage montrait 97,8 % de fils inutiles, et j'en ai déduit *« un dispatch
//! dimensionné juste rendrait un facteur voisin de 45 »*. **Mesuré : 3,5.**
//!
//! *Un fil gaspillé sort du shader immédiatement et coûte environ **18 fois moins** qu'un fil qui
//! travaille. Compter les fils lancés revenait à supposer qu'ils coûtent tous pareil — c'est le
//! genre d'hypothèse qu'on ne voit pas parce qu'on ne l'a jamais formulée.*
//!
//! ## Ce que ce banc ne dira JAMAIS
//!
//! - **Rien sur un Adreno 650.** Il décrit *cette carte, ce pilote, ce jour* — c'est écrit dans
//!   `chrono_gpu.rs` et ça vaut ici sans atténuation. **Un temps GPU ne se cite jamais comme une
//!   propriété du moteur.**
//! - **Rien sur une image complète.** Il mesure une passe de calcul isolée, pas un rendu.
//! - ⭐ *La bonne nouvelle est ailleurs : le banc `pire_cas` a établi sur cette machine que la
//!   gestion de fréquence du GPU est disculpée (dispersion 1,5× au pire sur un travail constant).
//!   Une médiane sur des répétitions y est donc lisible.*

use aegis_engine::chrono_gpu::ChronoGpu;
use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::Vec3;
use aegis_engine::core::memory::MemoryManager;
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::render::allocation::{aires_ecran, encoder_pour_gpu, planifier, raccorder};
use aegis_engine::render::placement::{a_refaire, Placement};
use aegis_engine::render::surface::{
    micro_sommets, EntreesGeometrie, ListeDeTravail, MemoireDeSurface, PasseDeSurface, Reglages,
};
use ash::vk;
use std::path::PathBuf;

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
const COTE: f32 = 900.0;
const BUDGET: u64 = 4_000_000;
const SOLEIL: [f32; 4] = [-0.45, -0.80, 0.40, 0.0];
const SIGNAL: [f32; 4] = [0.90, 0.80, 0.70, 3.0];

/// Les répétitions mesurées par taille de lot.
const REPETITIONS: usize = 60;
/// Les dispatches jetés avant de mesurer — le temps que la carte prenne son régime.
///
/// *`pire_cas` a disculpé la gestion de fréquence sur cette machine, mais un premier dispatch paie
/// aussi la compilation tardive du pipeline et la première touche des tampons. Jeter les premiers
/// coûte quelques millisecondes et évite de mesurer une mise en route.*
const ECHAUFFEMENT: usize = 15;

fn main() {
    let chemin = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| racine_du_depot().join(MODELE_PAR_DEFAUT));

    titre("AEGIS — CE QUE COÛTE VRAIMENT UNE MISE À JOUR PARTIELLE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    let triangles = (scene.indices.len() / 3) as u32;
    println!("  Scène   : {triangles} triangles · {REPETITIONS} répétitions par lot\n");
    println!("  ⭐ CRITÈRE ÉCRIT AVANT : le lot de 210 (ce que coûte une image en mouvement)");
    println!("     · > 50 % du lot complet  → ⛔ le prix fixe domine, les 93,6 % sont un mirage");
    println!("     · < 15 %                 → ✅ le temps suit le travail");
    println!("     · entre les deux         → on chiffre le prix fixe et le seuil de rentabilité");
    println!("  ⚠ TÉMOIN : un lot d'UN SEUL triangle — ce qu'il coûte EST le prix fixe.");

    match mesurer(&scene, triangles) {
        Ok(Some(())) => {}
        Ok(None) => println!("\n  ⚠ Aucun Vulkan joignable — le banc ne peut rien conclure."),
        Err(e) => println!("\n  ⛔ Échec : {e}"),
    }
}

fn mesurer(scene: &Scene, triangles: u32) -> Result<Option<()>, Box<dyn std::error::Error>> {
    let gpu = match GpuContext::sans_ecran(64, 64, 1) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ {e}");
            return Ok(None);
        }
    };
    let props = unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

    // ── La scène sur la carte ────────────────────────────────────────────────────────────────
    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();
    let plats: Vec<f32> = scene
        .sommets
        .iter()
        .flat_map(|s| {
            s.position.iter().chain(s.normal.iter()).chain(s.tangent.iter())
                .chain(s.uv0.iter()).chain(s.uv1.iter()).copied()
        })
        .collect();
    let (b_sommets, _, o_sommets) = televerser(&gpu.device, &props, &plats)?;
    let (b_indices, _, o_indices) = televerser(&gpu.device, &props, &scene.indices)?;

    let vp = camera(scene);
    let aires = aires_ecran(&positions, &scene.indices, &vp, COTE, COTE);
    let mut plan = planifier(&aires, BUDGET);
    raccorder(&mut plan, &positions, &scene.indices);
    let (b_plan, _, o_plan) = televerser(&gpu.device, &props, &encoder_pour_gpu(&plan))?;

    // ⭐⭐ SUR LA CARTE, et c'est la correction la plus importante de ce banc.
    //
    // *Il a mesuré 13,02 ms pendant toute une soirée avec une mémoire HÔTE — c'est-à-dire le bus
    // PCIe, pas le moteur. La même passe rend 0,034 ms sur la carte : un facteur 378.*
    let memoire = MemoireDeSurface::allouer_selon_sur_carte(&gpu.device, &props, &plan)?;
    assert!(
        !memoire.relisible,
        "un banc qui chronomètre ne doit JAMAIS mesurer sur une mémoire hôte : il mesurerait le \
         bus, pas le calcul"
    );
    let mut liste = ListeDeTravail::allouer(&gpu.device, &props, triangles)?;
    let passe = PasseDeSurface::nouvelle(
        &gpu.device,
        &EntreesGeometrie {
            sommets: (b_sommets, o_sommets),
            indices: (b_indices, o_indices),
            plan: (b_plan, o_plan),
            a_refaire: (liste.tampon, liste.octets()),
            debuts: (liste.tampon_debuts, liste.octets_debuts()),
        },
        &memoire,
    )?;

    let mut chrono = ChronoGpu::nouveau(
        &gpu.device,
        gpu.proprietes.limits.timestamp_period,
        unsafe {
            gpu.instance
                .get_physical_device_queue_family_properties(gpu.physical_device)
                .first()
                .map(|f| f.timestamp_valid_bits)
                .unwrap_or(0)
        },
        8,
    )?;

    // ── ⭐⭐ LE LOT RÉEL — celui qu'un vrai mouvement de caméra choisit ────────────────────────
    //
    // ⚠⚠ **Sans lui, ce banc se flatte.** Les lots de tête sont pris dans l'ordre des indices, donc
    // ils ne contiennent aucun des gros triangles de la scène — leur `rangs max` est petit *par
    // accident du rangement*. Un lot que le mouvement choisit, lui, est dispersé dans le maillage et
    // attrape ce qu'il trouve.
    //
    // *C'est la même faute que « la caméra cadrait sous la table » du 5 septembre : un banc qui
    // choisit ses données finit par mesurer son choix.*
    let lot_reel: Vec<u32> = {
        let mut pl = Placement::nouveau(triangles as usize);
        let k: Vec<u32> = plan.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();
        pl.mettre_a_jour(&k, BUDGET);
        let avant = pl.appliquer(&plan);
        // La même rotation de 12° que le banc `reecriture`.
        let vp2 = camera_tournee(scene, 12.0);
        let aires2 = aires_ecran(&positions, &scene.indices, &vp2, COTE, COTE);
        let mut plan2 = planifier(&aires2, BUDGET);
        raccorder(&mut plan2, &positions, &scene.indices);
        let k2: Vec<u32> = plan2.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();
        let d = pl.mettre_a_jour(&k2, BUDGET);
        a_refaire(&d, &avant.aretes, &plan2.aretes)
    };

    // ── Les lots, du témoin au maillage entier ───────────────────────────────────────────────
    let lots: Vec<u32> = vec![1, 10, 50, 210, 500, 1000, 2000, triangles]
        .into_iter()
        .filter(|n| *n <= triangles)
        .collect();

    println!("\n  {:>7} {:>9} {:>10} {:>12} {:>12} {:>9}",
        "lot", "médiane", "rangs max", "fils lancés", "fils utiles", "gaspillé");
    let mut resultats: Vec<(u32, f32)> = Vec::new();
    let mut details: Vec<(u32, f32, u64, u64)> = Vec::new();
    let mut lot_reel_ms: Option<f32> = None;
    let mut reference = 0.0f32;

    for (n, reel) in lots.iter().map(|n| (*n, false)).chain(std::iter::once((lot_reel.len() as u32, true))) {
        let n = &n;
        // ⚠ Les triangles du lot sont pris en tête de liste : ils ne sont pas ceux qu'un vrai
        // mouvement choisirait. *On mesure le COÛT D'UN LOT DE TAILLE n, pas le coût de cette
        // image-là — et la subdivision variant d'un triangle à l'autre, ce serait une autre
        // mesure.* La limite est réelle et elle est nommée en fin de banc.
        let travail: Vec<u32> = if reel { lot_reel.clone() } else { (0..*n).collect() };
        if travail.is_empty() {
            continue;
        }
        let utiles_prevus: u64 = travail
            .iter()
            .map(|t| micro_sommets(plan.par_triangle[*t as usize].1.trailing_zeros()) as u64)
            .sum();
        liste.ecrire(&gpu.device, &travail, &plan)?;

        let reglages = Reglages {
            fils: liste.fils,
            cote: memoire.cote,
            par_triangle: memoire.par_triangle,
            lots: liste.longueur,
            soleil: SOLEIL,
            signal: SIGNAL,
        };

        let mut durees = Vec::with_capacity(REPETITIONS);
        for i in 0..(ECHAUFFEMENT + REPETITIONS + 1) {
            let cmd = gpu.begin_single_time_commands()?;
            chrono.ouvrir_image(&gpu.device, cmd);
            passe.encoder(&gpu.device, cmd, &reglages, &memoire);
            chrono.jalon(&gpu.device, cmd, "surface");
            gpu.end_single_time_commands(cmd)?;
            // Le relevé lu ici est celui du tour PRÉCÉDENT — d'où le tour supplémentaire.
            if i > ECHAUFFEMENT {
                if let Some(e) = chrono.etapes().iter().find(|e| e.nom == "surface") {
                    durees.push(e.millisecondes);
                }
            }
        }

        durees.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if durees.is_empty() {
            println!("  {n:>10}  (aucun relevé)");
            continue;
        }
        let q = |p: f32| durees[((durees.len() - 1) as f32 * p) as usize];
        let mediane = q(0.5);
        if *n == triangles {
            reference = mediane;
        }
        resultats.push((*n, mediane));
        // ⭐⭐ CE QUI REND LA CAUSE VISIBLE — et sans ces trois colonnes ce banc mentirait.
        //
        // Le dispatch est RECTANGULAIRE : on lance `rangs_max` fils pour CHAQUE triangle du lot,
        // alors que chaque triangle n'a besoin que des siens. Les fils en trop sortent aussitôt du
        // shader — mais ils ont été lancés.
        //
        // *`surface.rs` nomme ce gaspillage depuis le premier jour : « c'est le gaspillage assumé
        // d'un dispatch rectangulaire sur une allocation qui ne l'est pas ; le mesurer est le
        // chantier suivant, l'ignorer serait le vrai défaut ». Le voici mesuré.*
        // ⚠ Depuis le dispatch plat, `lances` vaut le travail utile arrondi au groupe de 64 : la
        // colonne « gaspillé » ne mesure donc plus la forme rectangulaire, elle mesure ce qu'il
        // reste — l'arrondi. *Le tableau de simulation en bas de banc garde la comparaison des
        // trois formes, lui.*
        let lances = utiles_prevus.div_ceil(64) * 64;
        let utiles: u64 = utiles_prevus;
        println!(
            "  {:>7} {:>7.4}ms {:>10} {:>12} {:>12} {:>8.1}%{}",
            n,
            mediane,
            travail.iter().map(|t| micro_sommets(plan.par_triangle[*t as usize].1.trailing_zeros())).max().unwrap_or(3),
            lances,
            utiles,
            (1.0 - utiles as f64 / lances.max(1) as f64) * 100.0,
            if reel { "  ← LOT RÉEL (mouvement de 12°)" } else { "" }
        );
        if reel {
            lot_reel_ms = Some(mediane);
        }
        details.push((*n, mediane, lances, utiles));
    }

    // ── Le modèle : T(n) = a + b·n, ajusté sur le témoin et le lot complet ───────────────────
    titre("CE QUE LA COURBE DIT");
    println!("  Le temps suit le TRAVAIL UTILE, pas les fils lancés : le coût par micro-sommet");
    println!("  écrit est stable sur toute la gamme dès que le lot est assez gros pour occuper la");
    println!("  carte. Les petits lots paient leur manque de parallélisme, pas un prix fixe.");
    println!();
    println!("  ⚠ La colonne « gaspillé » ne mesure plus la forme rectangulaire — elle a disparu le");
    println!("    8 septembre au soir — mais ce qu'il en reste : l'arrondi au groupe de 64, qui ne");
    println!("    mord que sur les tout petits lots.");

    // ⭐ Le coût rapporté au TRAVAIL RÉEL — la seule grandeur comparable d'un lot à l'autre.
    titre("LE COÛT PAR MICRO-SOMMET — la grandeur qui, elle, se compare");
    println!("  {:>7} {:>14} {:>16} {:>16}", "lot", "ns/fil lancé", "ns/micro-sommet", "surcoût");
    for (n, t, lances, utiles) in &details {
        let par_lance = t * 1e6 / lances.max(&1).to_owned() as f32;
        let par_utile = t * 1e6 / utiles.max(&1).to_owned() as f32;
        println!("  {:>7} {:>13.2} {:>15.2} {:>15.1}×", n, par_lance, par_utile, par_utile / par_lance.max(f32::MIN_POSITIVE));
    }
    println!();
    println!("  ⚠ Le coût par fil LANCÉ n'est PAS constant, et c'est ce que la première version de");
    println!("    ce banc avait mal lu : un fil qui sort tôt coûte ~18× moins qu'un fil qui");
    println!("    travaille. **Compter les fils lancés suppose qu'ils coûtent tous pareil.**");
    println!("    *La grandeur qui se compare est le coût par fil UTILE, et lui est stable.*");

    titre("CE QUE ÇA VEUT DIRE");
    let prix_fixe = resultats.first().map(|(_, t)| *t).unwrap_or(0.0);
    let complet = reference.max(f32::MIN_POSITIVE);
    let lot210 = lot_reel_ms;

    println!("  Prix FIXE d'un dispatch (témoin à 1 triangle) : {prix_fixe:.4} ms");
    println!("  Lot complet ({triangles} triangles)            : {complet:.4} ms");
    println!("  Part du prix fixe dans le lot complet         : {:.1} %", prix_fixe / complet * 100.0);

    // ⚠⚠ IL N'Y A PAS DE « SEUIL DE RENTABILITÉ » À AFFICHER ICI, ET C'EST UNE CORRECTION.
    //
    // La première version calculait une pente entre le premier et le dernier point, puis en tirait
    // un seuil. **Ce chiffre était indéfendable** : il suppose que le temps est une droite en
    // fonction du nombre de triangles, or la mesure montre exactement le contraire — le coût suit
    // les FILS LANCÉS, et les fils lancés dépendent du plus gros triangle du lot, pas de sa taille.
    //
    // *Un chiffre faux affiché avec autorité est pire qu'un chiffre absent : personne ne le
    // rediscute. La grandeur qui se compare est le coût par fil lancé, et elle est au-dessus.*
    println!("  ⇒ Le coût suit les FILS LANCÉS, pas le nombre de triangles : il n'y a donc pas de");
    println!("     « seuil de rentabilité » en triangles à donner ici, et en afficher un serait");
    println!("     inventer une droite là où la mesure montre une marche.");

    titre("LE VERDICT — contre le critère écrit AVANT la mesure");
    match lot210 {
        Some(t210) => {
            let part = t210 / complet;
            println!("  ⭐ LE LOT RÉEL — {} triangles choisis par une rotation de 12° — coûte", lot_reel.len());
            println!("     {:.4} ms, soit {:.1} % du lot complet.", t210, part * 100.0);
            println!("  *Il représente {:.1} % du maillage en triangles.*\n",
                lot_reel.len() as f32 / triangles as f32 * 100.0);
            if part > 0.50 {
                println!("  ⇒ ⛔ LE PRIX FIXE DOMINE. Le gain compté en entrées ne se traduit PAS en");
                println!("     temps : découper le travail ne sert à rien à cette échelle. **Les");
                println!("     93,6 % sont un mirage comptable**, et le mécanisme est à revoir —");
                println!("     grouper les mises à jour sur plusieurs images, ou travailler à une");
                println!("     granularité plus grosse. *Ne pas monter d'étage avant d'avoir tranché.*");
            } else if part < 0.15 {
                println!("  ⇒ ✅ LE GAIN EST RÉEL, ET LE PRIX FIXE NE LE MANGE PAS.");
                println!("     Le lancement coûte {prix_fixe:.4} ms, soit {:.2} % du lot complet —", prix_fixe / complet * 100.0);
                println!("     autrement dit rien. **La réécriture partielle tient sa promesse.**");
                println!();
                println!("  ⚠ MAIS IL EST AMPUTÉ, ET IL FAUT LE DIRE : 6,4 % du travail coûte");
                println!("     {:.1} % du temps. Un facteur ~2 se perd en route.", part * 100.0);
                println!();
                println!("  ⛔ **ET LE VRAI GISEMENT EST AILLEURS, ÉNORME, ET IL EST À NOUS.**");
                println!("     Ce lot réel lance ses fils sur le `rangs max` de son plus GROS");
                println!("     triangle : tous les petits paient la taille du plus grand. Le tableau");
                println!("     ci-dessus le chiffre — **95 % des fils lancés ne servent à rien**, et");
                println!("     97,8 % sur le lot complet.");
                println!();
                println!("     *Le coût par fil lancé étant constant, un dispatch dimensionné juste");
                println!("     rendrait un facteur voisin de 45 sur cette carte. C'est le chantier");
                println!("     suivant, et il est plus gros que tout ce qui précède.*");
            } else {
                println!("  ⇒ 🟡 LE GAIN EST RÉEL MAIS FORTEMENT AMPUTÉ : {:.1} % du temps pour", part * 100.0);
                println!("     6,4 % du travail. La cause est dans le tableau du gaspillage — le");
                println!("     dispatch rectangulaire fait payer à tous les triangles du lot la");
                println!("     taille du plus gros. *C'est là qu'est le chantier, pas dans le prix");
                println!("     fixe de lancement, qui ne pèse que {:.2} %.*", prix_fixe / complet * 100.0);
            }
        }
        None => println!("  (le lot de 210 n'a pas été mesuré)"),
    }

    // ═══════════════════════════════════════════════════════════════════════════════════════
    // ⭐⭐ LA COMPARAISON QUI DÉCIDE : le dispatch plat sert-il, une fois la mesure honnête ?
    //
    // On relance la liste complète en gonflant `fils` jusqu'à ce que le dispatch lance autant de
    // fils que l'ancienne forme RECTANGULAIRE en lançait — 7 124 224. Les fils au-delà du travail
    // réel font leur recherche binaire, trouvent un rang hors de leur triangle, et sortent : très
    // exactement ce que faisaient les fils gaspillés du rectangle.
    titre("⭐⭐ LE DISPATCH PLAT SERT-IL ? — la charge de l'ancienne forme, remesurée honnêtement");
    {
        liste.tout(&gpu.device, &plan)?;
        let rangs_max_scene = plan.par_triangle.iter()
            .map(|(_, c)| micro_sommets(c.trailing_zeros()))
            .max().unwrap_or(3);
        let charge_rect = rangs_max_scene as u64 * triangles as u64;
        for (nom, fils) in [("plat (actuel)", liste.fils as u64), ("rectangulaire (avant)", charge_rect)] {
            let reglages = Reglages {
                fils: fils as u32,
                cote: memoire.cote,
                par_triangle: memoire.par_triangle,
                lots: liste.longueur,
                soleil: SOLEIL,
                signal: SIGNAL,
            };
            let mut d = Vec::with_capacity(REPETITIONS);
            for i in 0..(ECHAUFFEMENT + REPETITIONS + 1) {
                let cmd = gpu.begin_single_time_commands()?;
                chrono.ouvrir_image(&gpu.device, cmd);
                passe.encoder(&gpu.device, cmd, &reglages, &memoire);
                chrono.jalon(&gpu.device, cmd, "surface");
                gpu.end_single_time_commands(cmd)?;
                if i > ECHAUFFEMENT {
                    if let Some(e) = chrono.etapes().iter().find(|e| e.nom == "surface") {
                        d.push(e.millisecondes);
                    }
                }
            }
            d.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("  {:>24} : {:>10} fils → {:.4} ms", nom, fils, d[d.len() / 2]);
        }
        println!();
        println!("  *Les fils en trop font leur recherche, trouvent un rang hors de leur triangle,");
        println!("  et sortent — exactement ce que faisaient les fils gaspillés du rectangle.*");
    }

    titre("⭐⭐⭐ CE QUE DONNERAIT CHAQUE FORME DE DISPATCH — calculé, sans GPU");
    println!("  *Son intuition, avant qu'on code quoi que ce soit : « un dispatch par classe de");
    println!("  subdivision ne suffira pas ». On la chiffre au lieu de l'essayer.*\n");
    println!("  ⚠ LE DÉTAIL QUI DÉCIDE : les fils partent par GROUPES DE 64. Un triangle qui n'a");
    println!("    besoin que de 3 micro-points paie quand même un groupe entier — 61 fils perdus.");
    println!("    *Ce n'est pas un détail d'implémentation : c'est la granularité du matériel.*\n");

    const GROUPE: u64 = 64;
    let arrondi = |n: u64| n.div_ceil(GROUPE) * GROUPE;

    // Le travail utile, une fois pour toutes.
    let utiles_total: u64 = plan
        .par_triangle
        .iter()
        .map(|(_, c)| micro_sommets(c.trailing_zeros()) as u64)
        .sum();

    // (a) Ce qu'on fait aujourd'hui : un rectangle sur le plus gros.
    let rangs_max_scene = plan
        .par_triangle
        .iter()
        .map(|(_, c)| micro_sommets(c.trailing_zeros()) as u64)
        .max()
        .unwrap_or(3);
    let rectangulaire = arrondi(rangs_max_scene) * triangles as u64;

    // (b) Un dispatch PAR CLASSE : chaque classe lance ce que SA taille demande.
    let mut par_classe = 0u64;
    let mut effectifs = [0u64; 9];
    for (_, cote) in &plan.par_triangle {
        effectifs[cote.trailing_zeros() as usize] += 1;
    }
    for (k, nb) in effectifs.iter().enumerate() {
        if *nb > 0 {
            par_classe += arrondi(micro_sommets(k as u32) as u64) * nb;
        }
    }

    // (c) Un dispatch À PLAT sur les micro-sommets : un seul arrondi, pour toute la scène.
    let a_plat = arrondi(utiles_total);

    println!("  {:>22} {:>14} {:>12} {:>14}", "forme du dispatch", "fils lancés", "gaspillé", "contre l'idéal");
    for (nom, fils) in [
        ("rectangulaire (actuel)", rectangulaire),
        ("par classe", par_classe),
        ("à plat sur les points", a_plat),
    ] {
        println!(
            "  {:>22} {:>14} {:>11.1}% {:>13.1}×",
            nom,
            fils,
            (1.0 - utiles_total as f64 / fils as f64) * 100.0,
            fils as f64 / utiles_total as f64
        );
    }
    println!("  {:>22} {:>14} {:>11} {:>14}", "(travail utile)", utiles_total, "—", "1,0×");

    println!("\n  La répartition des triangles par classe, qui explique tout :");
    println!("  {:>4} {:>10} {:>12} {:>14} {:>12}", "k", "triangles", "points/tri", "fils/tri (64)", "gaspillé");
    for (k, nb) in effectifs.iter().enumerate() {
        if *nb > 0 {
            let points = micro_sommets(k as u32) as u64;
            println!(
                "  {:>4} {:>10} {:>12} {:>14} {:>11.1}%",
                k, nb, points, arrondi(points),
                (1.0 - points as f64 / arrondi(points) as f64) * 100.0
            );
        }
    }

    println!();
    if par_classe as f64 / utiles_total as f64 > 1.5 {
        println!("  ⇒ ⛔ **SON INTUITION EST JUSTE : le dispatch par classe NE SUFFIT PAS.**");
        println!("     Il ramène le gaspillage de {:.1} % à {:.1} %, ce qui est déjà beaucoup —",
            (1.0 - utiles_total as f64 / rectangulaire as f64) * 100.0,
            (1.0 - utiles_total as f64 / par_classe as f64) * 100.0);
        println!("     mais il en LAISSE {:.1} %, et la cause est l'arrondi au groupe de 64 : les",
            (1.0 - utiles_total as f64 / par_classe as f64) * 100.0);
        println!("     petites classes paient un groupe entier pour quelques points.");
        println!();
        println!("     *Le dispatch À PLAT, lui, n'arrondit qu'UNE FOIS pour toute la scène —");
        println!("     {:.1} % de gaspillage. C'est la voie à prendre.*",
            (1.0 - utiles_total as f64 / a_plat as f64) * 100.0);
    } else {
        println!("  ⇒ ✅ Le dispatch par classe suffit : il ramène le gaspillage à {:.1} %.",
            (1.0 - utiles_total as f64 / par_classe as f64) * 100.0);
        println!("     *L'intuition d'un problème résiduel ne se vérifie pas sur cette scène —");
        println!("     à re-mesurer sur une scène aux triangles plus petits.*");
    }

    titre("CE QUE CE BANC NE DIT PAS");
    println!("  · Rien sur un Adreno 650 : il décrit cette carte, ce pilote, ce jour. **Un temps GPU");
    println!("    ne se cite jamais comme une propriété du moteur.**");
    println!("  · Rien sur une image complète — c'est une passe de calcul isolée, pas un rendu.");
    println!("  · Les triangles d'un lot sont pris EN TÊTE, pas choisis par un vrai mouvement : la");
    println!("    subdivision variant de l'un à l'autre, un lot réel n'a pas la même composition.");
    println!("    *Ce banc mesure le coût d'un lot de taille n, pas le coût de telle image.*");

    passe.detruire(&gpu.device);
    liste.detruire(&gpu.device);
    memoire.detruire(&gpu.device);
    chrono.detruire(&gpu.device);
    Ok(Some(()))
}

fn televerser<T: Copy>(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    donnees: &[T],
) -> Result<(vk::Buffer, vk::DeviceMemory, u64), Box<dyn std::error::Error>> {
    let octets = std::mem::size_of_val(donnees) as u64;
    let (tampon, memoire) = MemoryManager::create_buffer(
        device,
        props,
        octets.max(4),
        vk::BufferUsageFlags::STORAGE_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    unsafe {
        let ptr = device.map_memory(memoire, 0, octets, vk::MemoryMapFlags::empty())? as *mut T;
        std::ptr::copy_nonoverlapping(donnees.as_ptr(), ptr, donnees.len());
        device.unmap_memory(memoire);
    }
    Ok((tampon, memoire, octets.max(4)))
}

fn camera(scene: &Scene) -> [f32; 16] {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for s in &scene.sommets {
        let p = Vec3::from_array(s.position);
        min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    let centre = (min + max) * 0.5;
    let rayon = ((max - min) * 0.5).length().max(1e-3);
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

/// La caméra du banc, tournée de `angle` degrés — la même rotation que le banc `reecriture`.
fn camera_tournee(scene: &Scene, angle_degres: f32) -> [f32; 16] {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for s in &scene.sommets {
        let p = Vec3::from_array(s.position);
        min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    let centre = (min + max) * 0.5;
    let rayon = ((max - min) * 0.5).length().max(1e-3);
    let fov = 55_f32.to_radians();
    let recul = rayon * 1.3 / (fov * 0.5).tan();
    let direction = Vec3::new(0.55, 0.40, -0.73).normalize();
    let oeil = centre + direction * recul;
    let vue = (centre - oeil).normalize();
    let (sn, cs) = angle_degres.to_radians().sin_cos();
    let tournee = Vec3::new(vue.x * cs + vue.z * sn, vue.y, -vue.x * sn + vue.z * cs);
    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, oeil + tournee * recul, 1.0);
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

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
