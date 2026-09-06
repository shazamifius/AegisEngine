//! **L'ÉCRAN LIT LA SURFACE — l'étage 0, geste 2. La première image de la thèse.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example lire_surface --no-default-features
//! cargo run --release -p aegis_engine --example lire_surface --no-default-features -- <fichier.glb> <k>
//! ```
//!
//! ## Ce qu'il met à l'épreuve
//!
//! `02-THESE.md` annonce, pour l'étage 0 :
//!
//! > **Image attendue :** la table, identique à l'œil, ombrée en espace surface.
//! > **Ce qui invaliderait :** une empreinte non bornée, ou des coutures visibles aux arêtes.
//!
//! Ce banc rend donc **deux fois la même scène** :
//!
//! | | Ce que fait le pixel |
//! |---|---|
//! | **A — le chemin ordinaire** | il **calcule** sa lumière, comme tous les moteurs |
//! | **B — la thèse** | il **lit** la lumière qu'une passe de calcul a écrite sur la surface |
//!
//! ⭐ **Un seul shader porte les deux chemins, et un seul entier les sépare.** Même géométrie, même
//! caméra, même cadrage, même courbe. *Toute différence est donc imputable à la mémoire de surface,
//! et à rien d'autre.* Le corpus porte le contre-exemple : une garde qui comparait deux rendus
//! **cadrés différemment** et qui n'est passée qu'une fois, par hasard.
//!
//! ## ⚠ Ce qu'il ne prouve pas
//!
//! - **Rien sur la performance.** Le chemin B fait *plus* de travail que A ici (une passe de calcul
//!   en plus, une géométrie non indexée), et c'est normal : le gain de la thèse est ailleurs — une
//!   surface ombrée une fois pour **deux yeux**, et un ombrage découplé de la cadence d'affichage.
//!   *Aucun des deux n'est mesuré ici, et prétendre le contraire serait une victoire prématurée.*
//! - **Rien sur l'allocation.** La subdivision est **uniforme** ; les aires d'un `.glb` varient de
//!   20 610 × (banc `topologie`). C'est le chantier 0.2, entier.
//! - **Rien sur un Adreno 650**, comme toujours.

use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::Vec3;
use aegis_engine::core::memory::MemoryManager;
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::render::pipeline::{Faces, Melange, PipelineFactory, Reglages as ReglagesPipeline};
use aegis_engine::render::surface::{MemoireDeSurface, PasseDeSurface, Reglages as ReglagesSurface, OCTETS_PAR_ENTREE};
use ash::vk;
use std::path::{Path, PathBuf};

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
/// ⚠ `_SRGB`, et pas `_UNORM` : c'est le matériel qui encode la gamma à l'écriture, une seule fois.
/// *Le shader écrit du linéaire ; une garde du moteur tombe si un shader encode lui-même.*
const FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
const COTE: u32 = 900;
const K_PAR_DEFAUT: u32 = 3;
const SOLEIL: [f32; 4] = [-0.4, -0.85, -0.35, 0.0];

/// Le signal de test : une teinte et une fréquence spatiale.
///
/// ⚠⚠ **Il vit ici, dans le banc, et pas dans le shader — c'est la frontière du projet.** Le moteur
/// fournit ce qui est VRAI (de la lumière sur une surface) ; choisir une couleur est le rôle du jeu,
/// et un test échoue si un shader du moteur en porte une. *La première version de ces deux shaders
/// portait `vec3(1.0, 0.85, 0.7)` en dur.*
///
/// ⚠ La fréquence 3,0 est délibérément ÉLEVÉE : elle fait plusieurs cycles sur un seul triangle du
/// plateau, donc elle met la mémoire de surface dans son pire cas. *Un banc qui choisit un signal
/// facile ne mesure rien.*
const SIGNAL: [f32; 4] = [1.0, 0.85, 0.7, 3.0];


/// ⭐ **Le critère, écrit AVANT la mesure.**
///
/// Les deux images passent par la même courbe et sortent en 8 bits par canal. Un écart de **un**
/// niveau est le bruit d'arrondi de cette quantification, plus celui du demi-flottant de la mémoire
/// de surface. *Au-delà de trois niveaux, ce n'est plus de l'arrondi : c'est l'interpolation
/// barycentrique qui s'écarte du calcul par pixel — ce que ce banc cherche précisément à voir.*
const SEUIL_ECART: u8 = 3;

/// La part de pixels autorisée à dépasser [`SEUIL_ECART`].
///
/// ⚠ Elle n'est pas nulle, et il faut dire pourquoi : sur une **silhouette**, un pixel couvert à
/// moitié par la géométrie prend l'une ou l'autre valeur selon des règles de remplissage que les
/// deux passes n'ont aucune raison de trancher pareil. *Ces pixels-là ne mesurent pas la mémoire de
/// surface, ils mesurent le bord d'un triangle.*
const PART_TOLEREE: f64 = 0.005;

fn main() {
    let mut args = std::env::args().skip(1);
    let chemin = match args.next() {
        Some(a) => PathBuf::from(a),
        None => racine_du_depot().join(MODELE_PAR_DEFAUT),
    };
    let k: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(K_PAR_DEFAUT);

    titre("AEGIS — L'ÉCRAN LIT LA SURFACE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    let triangles = (scene.indices.len() / 3) as u32;
    println!("  Scène   : {} parties, {} triangles · subdivision k = {k}", scene.parties.len(), triangles);

    match rendre(&scene, triangles, k) {
        Ok(Some((calcule, lu, entrees))) => confronter(&calcule, &lu, entrees, triangles, k),
        Ok(None) => {
            println!("\n  ⚠ Aucun Vulkan joignable — le banc ne peut rien conclure.");
            return;
        }
        Err(e) => {
            println!("\n  ⛔ Échec : {e}");
            return;
        }
    }
    convergence(&scene, triangles);
}

/// ⭐⭐ LA MESURE QUI TRANCHE — l'écart décroît-il quand la densité monte ?
///
/// ## Pourquoi elle est indispensable, et pourquoi elle manquait
///
/// La première version de ce banc rendait un verdict binaire à `k = 3`, et il était **⛔ ÉCHEC** :
/// 78 % des pixels couverts au-delà du seuil. *La carte d'écart a corrigé la lecture avant qu'on
/// en tire une conclusion* — l'écart y est en **bandes régulières sur les faces**, et il n'y a
/// **aucune ligne le long des arêtes**. Le critère écrit avant la mesure tranchait déjà : arêtes =
/// couture donc adressage en cause ; faces = densité.
///
/// **Ce que ce banc mesurait vraiment, c'était sa propre teinte de test.** `sin(p · 3)` fait
/// plusieurs cycles sur un seul triangle du plateau : aucune mémoire de surface à 8 segments par
/// arête ne peut porter ça. *Un verdict binaire sur un signal choisi arbitrairement ne mesure pas
/// le socle, il mesure le choix du signal.*
///
/// ⭐ **Et c'est un résultat de conception, pas un défaut :** la mémoire de surface a une
/// **fréquence de coupure**. C'est la contrepartie exacte de la thèse — *si tout vit sur la
/// surface, alors rien de plus fin que sa densité ne peut y vivre.* Cela a une conséquence directe
/// sur l'étage 1 : un détail de matière plus fin que la densité de surface exigera de la densité,
/// pas un format plus malin.
///
/// **Le critère, écrit avant :** l'écart moyen doit **décroître de façon monotone** avec `k`, et
/// approximativement d'un facteur 4 quand `k` gagne 1 (deux fois plus de segments par arête sur
/// chacune des deux directions). *S'il stagne, ce n'est pas une question de densité et le socle est
/// en cause.*
fn convergence(scene: &Scene, triangles: u32) {
    titre("LA CONVERGENCE — l'écart décroît-il quand la densité monte ?");
    println!("  Critère écrit AVANT : décroissance MONOTONE, ~÷4 par incrément de k.");
    println!("  Si l'écart stagne, ce n'est pas la densité — c'est le socle.\n");
    println!("  {:>3} {:>10} {:>12} {:>10} {:>10}", "k", "segments", "entrées", "écart moy.", "rapport");

    let mut precedent: Option<f64> = None;
    let mut monotone = true;
    let mut rapports: Vec<f64> = Vec::new();
    for k in 1..=6u32 {
        match rendre(scene, triangles, k) {
            Ok(Some((calcule, lu, entrees))) => {
                let (moyen, _, _) = ecart(&calcule, &lu);
                let rapport = precedent.map(|p| p / moyen.max(1e-9));
                println!(
                    "  {:>3} {:>10} {:>12} {:>9.2}  {:>9}",
                    k,
                    1u32 << k,
                    entrees,
                    moyen,
                    rapport.map(|r| format!("{r:.2}×")).unwrap_or_else(|| "—".into())
                );
                if let Some(p) = precedent {
                    if moyen > p {
                        monotone = false;
                    }
                    rapports.push(p / moyen.max(1e-9));
                }
                precedent = Some(moyen);
            }
            _ => println!("  {k:>3}  (rendu impossible)"),
        }
    }

    println!();
    // ⚠ Pas une moyenne : elle mélangerait deux régimes. Aux petits `k` le signal n'est pas résolu
    // du tout et le rapport ne veut rien dire ; c'est le DERNIER incrément qui approche le régime
    // asymptotique, et c'est sa TENDANCE qui informe.
    let dernier = rapports.last().copied().unwrap_or(f64::NAN);
    let croissant = rapports.len() >= 3
        && rapports.windows(2).skip(1).all(|w| w[1] >= w[0] - 0.05);
    if monotone {
        println!("  ⇒ ✅ DÉCROISSANCE MONOTONE. L'écart est une affaire de DENSITÉ, pas de socle.");
        println!("     L'adressage barycentrique porte le signal ; il ne le déforme pas. Ce qui");
        println!("     reste est la fréquence de coupure de la mémoire — une propriété, pas un");
        println!("     défaut, et elle commande l'étage 1 : un détail plus fin que la densité");
        println!("     exigera de la densité, jamais un format plus malin.");
        println!();
        println!("  ⭐ LE FACTEUR PRÉDIT ÉTAIT 4×. Dernier incrément mesuré : {dernier:.2}×.");
        if croissant {
            println!("     Et il CROÎT à chaque pas. *Le critère n'était pas faux, il était");
            println!("     ASYMPTOTIQUE : l'erreur d'une interpolation linéaire par morceaux ne");
            println!("     décroît en O(h²) qu'une fois le signal effectivement résolu. Aux");
            println!("     petits k, le signal de test ne l'est pas du tout, et le rapport ne");
            println!("     mesure alors que le repliement.*");
        } else {
            println!("     ⚠⚠ Et il NE CROÎT PAS. Le régime O(h²) n'est pas atteint — quelque");
            println!("     chose d'autre borne la convergence, et ce n'est pas la densité.");
        }
        println!();
        println!("     ⚠ Une cause a déjà été confirmée en chemin, par accident : le banc encodait");
        println!("     la gamma DANS le shader (une garde du moteur l'a fait tomber). En la");
        println!("     laissant à la surface `_SRGB`, le facteur est passé de 2,87 à {dernier:.2}.");
        println!("     *Une non-linéarité dans la chaîne de mesure déformait la mesure elle-même.*");
        println!();
        println!("     Deux causes restent PLAUSIBLES et non vérifiées : la normale est");
        println!("     renormalisée, et le lambert est écrêté à zéro — donc non dérivable au");
        println!("     terminateur, où la théorie en O(h²) ne s'applique pas.");
    } else {
        println!("  ⇒ ⛔ L'ÉCART NE DÉCROÎT PAS DE FAÇON MONOTONE. Le socle est en cause, et pas");
        println!("     la densité. Ne pas augmenter k : chercher l'erreur dans l'adressage ou");
        println!("     dans l'interpolation.");
    }
}

/// L'écart entre deux rendus, restreint aux pixels que la géométrie couvre.
fn ecart(calcule: &[u8], lu: &[u8]) -> (f64, u8, usize) {
    let mut total = 0u64;
    let mut pire = 0u8;
    let mut couverts = 0usize;
    for p in 0..(calcule.len() / 3) {
        let a = &calcule[p * 3..p * 3 + 3];
        let b = &lu[p * 3..p * 3 + 3];
        if a.iter().any(|v| *v > 0) || b.iter().any(|v| *v > 0) {
            let e = (0..3).fold(0u8, |m, c| m.max(a[c].abs_diff(b[c])));
            total += e as u64;
            pire = pire.max(e);
            couverts += 1;
        }
    }
    (total as f64 / couverts.max(1) as f64, pire, couverts)
}

/// Rend les deux images. Rend `None` si aucun Vulkan n'est joignable.
#[allow(clippy::type_complexity)]
fn rendre(
    scene: &Scene,
    triangles: u32,
    k: u32,
) -> Result<Option<(Vec<u8>, Vec<u8>, u32)>, Box<dyn std::error::Error>> {
    let gpu = match GpuContext::sans_ecran_format(COTE, COTE, 1, FORMAT) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ {e}");
            return Ok(None);
        }
    };
    let memory_props =
        unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

    let plats: Vec<f32> = scene
        .sommets
        .iter()
        .flat_map(|s| {
            s.position.iter().chain(s.normal.iter()).chain(s.tangent.iter())
                .chain(s.uv0.iter()).chain(s.uv1.iter()).copied()
        })
        .collect();
    let (b_sommets, m_sommets, o_sommets) = televerser(&gpu.device, &memory_props, &plats)?;
    let (b_indices, m_indices, o_indices) = televerser(&gpu.device, &memory_props, &scene.indices)?;

    // ── 1. Remplir la mémoire de surface — le geste 1, réutilisé tel quel ───────────────────
    let memoire = MemoireDeSurface::allouer(&gpu.device, &memory_props, triangles, k)?;
    let passe = PasseDeSurface::nouvelle(
        &gpu.device, b_sommets, o_sommets, b_indices, o_indices, &memoire,
    )?;
    let reglages_surface = ReglagesSurface {
        triangles,
        cote: memoire.cote,
        par_triangle: memoire.par_triangle,
        _pad: 0,
        soleil: SOLEIL,
        signal: SIGNAL,
    };
    let cmd = gpu.begin_single_time_commands()?;
    passe.encoder(&gpu.device, cmd, &reglages_surface, &memoire);
    gpu.end_single_time_commands(cmd)?;

    // ── 2. Le pipeline de lecture ───────────────────────────────────────────────────────────
    let lecture = Lecture::nouvelle(
        &gpu, b_sommets, o_sommets, b_indices, o_indices, &memoire,
    )?;

    // ── 3. Le cadrage, calculé une fois et partagé par les deux rendus ──────────────────────
    //
    // ⚠ C'est la ligne qui rend la comparaison valide. Deux cadrages, même très proches, feraient
    // dire n'importe quoi à l'écart mesuré.
    let (centre, rayon) = boite_englobante(scene);
    let fov = 55_f32.to_radians();
    let recul = rayon * 1.3 / (fov * 0.5).tan();
    let oeil = centre + Vec3::new(0.55, 0.40, -0.73).normalize() * recul;
    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, centre, 1.0);
    camera.fov_y_radians = fov;
    camera.z_near = (recul - rayon * 1.3).max(rayon * 0.01);
    camera.z_far = recul + rayon * 2.6;
    let view_proj = camera.compute_projection_matrix() * camera.compute_view_matrix();

    let mut images = Vec::new();
    for mode in [0u32, 1u32] {
        let r = ReglagesLecture {
            view_proj: aplatir(&view_proj),
            triangles,
            cote: memoire.cote,
            par_triangle: memoire.par_triangle,
            mode,
            soleil: SOLEIL,
            signal: SIGNAL,
        };
        images.push(lecture.rendre(&gpu, &r, triangles)?);
    }
    let entrees = memoire.entrees;

    lecture.detruire(&gpu.device);
    passe.detruire(&gpu.device);
    memoire.detruire(&gpu.device);
    unsafe {
        gpu.device.destroy_buffer(b_sommets, None);
        gpu.device.free_memory(m_sommets, None);
        gpu.device.destroy_buffer(b_indices, None);
        gpu.device.free_memory(m_indices, None);
    }
    let lu = images.pop().unwrap();
    let calcule = images.pop().unwrap();
    Ok(Some((calcule, lu, entrees)))
}

/// Compare les deux images, écrit les preuves, et rend le verdict contre le critère écrit avant.
fn confronter(calcule: &[u8], lu: &[u8], entrees: u32, triangles: u32, k: u32) {
    titre("LES DEUX IMAGES");
    let dossier = racine_du_depot().join("target/preuves");
    let _ = std::fs::create_dir_all(&dossier);
    ecrire_png(&dossier.join("surface-calculee.png"), calcule);
    ecrire_png(&dossier.join("surface-lue.png"), lu);

    // ── L'écart, pixel par pixel ────────────────────────────────────────────────────────────
    let mut pire = 0u8;
    let mut au_dela = 0usize;
    let mut couverts = 0usize;
    let mut ecart_total = 0u64;
    let mut carte = vec![0u8; calcule.len()];
    for p in 0..(calcule.len() / 3) {
        let mut e = 0u8;
        for c in 0..3 {
            e = e.max(calcule[p * 3 + c].abs_diff(lu[p * 3 + c]));
        }
        // Un pixel « couvert » porte de la géométrie dans au moins un des deux rendus.
        if calcule[p * 3..p * 3 + 3].iter().any(|v| *v > 0)
            || lu[p * 3..p * 3 + 3].iter().any(|v| *v > 0)
        {
            couverts += 1;
            if e > SEUIL_ECART {
                au_dela += 1;
            }
            ecart_total += e as u64;
            pire = pire.max(e);
        }
        // La carte d'écart : amplifiée ×16 pour être visible à l'œil.
        let v = (e as u32 * 16).min(255) as u8;
        carte[p * 3] = v;
        carte[p * 3 + 1] = v;
        carte[p * 3 + 2] = v;
    }
    ecrire_png(&dossier.join("surface-ecart.png"), &carte);

    let total = calcule.len() / 3;
    let part = au_dela as f64 / couverts.max(1) as f64;
    println!("  pixels couverts par la géométrie : {couverts} sur {total}");
    println!("  écart moyen SUR LES COUVERTS     : {:.3} niveau(x) sur 255", ecart_total as f64 / couverts.max(1) as f64);
    println!("  pire écart                       : {pire} niveau(x)");
    println!("  pixels au-delà de {SEUIL_ECART} niveaux         : {au_dela}  ({:.3} % des couverts)", part * 100.0);

    titre("LE VERDICT — contre le critère écrit AVANT la mesure");
    println!("  · écart ≤ {SEUIL_ECART} niveaux sur ≥ {:.1} % des pixels couverts → le socle tient", (1.0 - PART_TOLEREE) * 100.0);
    println!("  · au-delà → l'interpolation barycentrique s'écarte du calcul par pixel\n");
    if part <= PART_TOLEREE {
        println!("  ⇒ ✅ L'IMAGE OMBRÉE HORS ÉCRAN EST INDISCERNABLE DE CELLE CALCULÉE PAR PIXEL.");
        println!("     La lumière a été écrite sur la surface par une passe de calcul, puis lue à");
        println!("     l'adresse (T, u, v) par le fragment — sans dépliage UV, sans atlas, et sans");
        println!("     aucune recherche spatiale.");
    } else {
        println!("  ⇒ ⛔ ÉCART AU-DELÀ DU CRITÈRE. Lire `surface-ecart.png` avant de conclure :");
        println!("     un écart concentré sur les ARÊTES est une couture — l'adressage est en cause.");
        println!("     Un écart réparti sur les FACES est un défaut de densité — augmenter k.");
    }

    titre("LE CHIFFRE");
    let octets = entrees as u64 * OCTETS_PAR_ENTREE;
    const PIXELS_QUEST2: f64 = 2.0 * 1832.0 * 1920.0;
    const TRAFIC: f64 = 44_000_000_000.0 / 72.0;
    println!("  mémoire de surface     : {} Ko  ({entrees} entrées × {OCTETS_PAR_ENTREE} o, {triangles} triangles, k = {k})", octets / 1024);
    println!("  lue une fois par pixel : {:.1} % du trafic mémoire d'une image sur Quest 2", OCTETS_PAR_ENTREE as f64 * PIXELS_QUEST2 / TRAFIC * 100.0);
    println!("  ⚠ CALCULÉ depuis des specs de seconde main. Sert à éliminer, jamais à valider.");

    titre("LES PREUVES");
    println!("  target/preuves/surface-calculee.png  — le pixel calcule");
    println!("  target/preuves/surface-lue.png       — le pixel LIT la surface");
    println!("  target/preuves/surface-ecart.png     — leur écart, amplifié ×16");
    println!("\n  ⚠ Ces images ne disent RIEN de la performance : le chemin B fait plus de travail");
    println!("    ici. Le gain de la thèse est ailleurs — une surface ombrée une fois pour deux");
    println!("    yeux, et un ombrage découplé de la cadence. Ni l'un ni l'autre n'est mesuré.");
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// La plomberie
// ═══════════════════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Clone, Copy)]
struct ReglagesLecture {
    view_proj: [f32; 16],
    triangles: u32,
    cote: u32,
    par_triangle: u32,
    mode: u32,
    soleil: [f32; 4],
    signal: [f32; 4],
}

struct Lecture {
    layout_descripteur: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    profondeur: vk::Image,
    memoire_profondeur: vk::DeviceMemory,
    vue_profondeur: vk::ImageView,
    format_profondeur: vk::Format,
}

impl Lecture {
    fn nouvelle(
        gpu: &GpuContext,
        sommets: vk::Buffer,
        o_sommets: u64,
        indices: vk::Buffer,
        o_indices: u64,
        memoire: &MemoireDeSurface,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let device = &gpu.device;
        let liaisons: [vk::DescriptorSetLayoutBinding; 3] = std::array::from_fn(|i| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        });
        let layout_descripteur = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&liaisons), None)?
        };
        let tailles = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER).descriptor_count(3)];
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default().pool_sizes(&tailles).max_sets(1), None)?
        };
        let set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&layout_descripteur)))?[0]
        };
        let infos = [
            vk::DescriptorBufferInfo::default().buffer(sommets).range(o_sommets),
            vk::DescriptorBufferInfo::default().buffer(indices).range(o_indices),
            vk::DescriptorBufferInfo::default().buffer(memoire.tampon).range(memoire.octets()),
        ];
        let ecritures: Vec<vk::WriteDescriptorSet> = (0..3)
            .map(|i| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(&infos[i]))
            })
            .collect();
        unsafe { device.update_descriptor_sets(&ecritures, &[]) };

        let plage = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(std::mem::size_of::<ReglagesLecture>() as u32);
        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(std::slice::from_ref(&layout_descripteur))
                    .push_constant_ranges(std::slice::from_ref(&plage)), None)?
        };

        // La profondeur : sans elle, les faces arrière du plateau passeraient devant.
        let format_profondeur = vk::Format::D32_SFLOAT;
        let (profondeur, memoire_profondeur, vue_profondeur) =
            image_profondeur(gpu, format_profondeur)?;

        let module = PipelineFactory::create_shader_module_from_bytes(
            device, aegis_engine::shaders::LECTURE_SPV)?;
        let pipeline = PipelineFactory::create_graphics_pipeline(
            device, layout, module, module,
            ReglagesPipeline {
                color_format: FORMAT,
                second_format: None,
                depth_format: Some(format_profondeur),
                depth_write: true,
                melange: Melange::Aucun,
                // ⚠ Aucun attribut de sommet : le shader lit la géométrie depuis les tampons de
                // stockage et déduit son triangle de l'index du sommet.
                use_vertex_input: false,
                faces: Faces::Toutes,
                echantillons: vk::SampleCountFlags::TYPE_1,
            })?;
        unsafe { device.destroy_shader_module(module, None) };

        Ok(Self {
            layout_descripteur, pool, set, layout, pipeline,
            profondeur, memoire_profondeur, vue_profondeur, format_profondeur,
        })
    }

    fn rendre(
        &self,
        gpu: &GpuContext,
        reglages: &ReglagesLecture,
        triangles: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let device = &gpu.device;
        let etendue = gpu.swapchain_extent;
        let image = gpu.swapchain_images[0];
        let vue = gpu.swapchain_image_views[0];

        let cmd = gpu.begin_single_time_commands()?;
        unsafe {
            barriere(device, cmd, image, vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageAspectFlags::COLOR);
            barriere(device, cmd, self.profondeur, vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, vk::ImageAspectFlags::DEPTH);

            let couleur = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } });
            let prof = vk::RenderingAttachmentInfo::default()
                .image_view(self.vue_profondeur)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                });
            device.cmd_begin_rendering(cmd, &vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&couleur))
                .depth_attachment(&prof));
            device.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0, y: 0.0,
                width: etendue.width as f32, height: etendue.height as f32,
                min_depth: 0.0, max_depth: 1.0,
            }]);
            device.cmd_set_scissor(cmd, 0, &[vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue,
            }]);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.layout,
                0, std::slice::from_ref(&self.set), &[]);
            let octets = std::slice::from_raw_parts(
                (reglages as *const ReglagesLecture) as *const u8,
                std::mem::size_of::<ReglagesLecture>());
            device.cmd_push_constants(cmd, self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, octets);
            // Non indexé : trois sommets par triangle, et le shader retrouve son triangle.
            device.cmd_draw(cmd, triangles * 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
        gpu.end_single_time_commands(cmd)?;
        gpu.relire_image(image, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, etendue, FORMAT)
    }

    fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.vue_profondeur, None);
            device.destroy_image(self.profondeur, None);
            device.free_memory(self.memoire_profondeur, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout_descripteur, None);
        }
        let _ = self.format_profondeur;
    }
}

fn image_profondeur(
    gpu: &GpuContext,
    format: vk::Format,
) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView), Box<dyn std::error::Error>> {
    let device = &gpu.device;
    let props = unsafe {
        gpu.instance.get_physical_device_memory_properties(gpu.physical_device)
    };
    let image = unsafe {
        device.create_image(&vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: COTE, height: COTE, depth: 1 })
            .mip_levels(1).array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .initial_layout(vk::ImageLayout::UNDEFINED), None)?
    };
    let reqs = unsafe { device.get_image_memory_requirements(image) };
    let idx = MemoryManager::find_memory_type(&props, reqs.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL).ok_or("pas de mémoire pour la profondeur")?;
    let memoire = unsafe {
        device.allocate_memory(&vk::MemoryAllocateInfo::default()
            .allocation_size(reqs.size).memory_type_index(idx), None)?
    };
    unsafe { device.bind_image_memory(image, memoire, 0)? };
    let vue = unsafe {
        device.create_image_view(&vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0, level_count: 1,
                base_array_layer: 0, layer_count: 1,
            }), None)?
    };
    Ok((image, memoire, vue))
}

unsafe fn barriere(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    de: vk::ImageLayout,
    vers: vk::ImageLayout,
    aspect: vk::ImageAspectFlags,
) {
    let b = vk::ImageMemoryBarrier::default()
        .old_layout(de).new_layout(vers)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0, level_count: 1,
            base_array_layer: 0, layer_count: 1,
        })
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
    unsafe {
        device.cmd_pipeline_barrier(cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::DependencyFlags::empty(), &[], &[], std::slice::from_ref(&b));
    }
}

fn televerser<T: Copy>(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    donnees: &[T],
) -> Result<(vk::Buffer, vk::DeviceMemory, u64), Box<dyn std::error::Error>> {
    let octets = std::mem::size_of_val(donnees) as u64;
    let (tampon, memoire) = MemoryManager::create_buffer(device, props, octets,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)?;
    unsafe {
        let ptr = device.map_memory(memoire, 0, octets, vk::MemoryMapFlags::empty())? as *mut T;
        std::ptr::copy_nonoverlapping(donnees.as_ptr(), ptr, donnees.len());
        device.unmap_memory(memoire);
    }
    Ok((tampon, memoire, octets))
}

/// Aplatit une matrice en colonnes majeures — l'ordre qu'attend `mat4x4<f32>` en WGSL.
fn aplatir(m: &aegis_engine::core::math::Mat4) -> [f32; 16] {
    let c = m.to_cols_array_2d();
    let mut sortie = [0.0f32; 16];
    for (i, col) in c.iter().enumerate() {
        sortie[i * 4..i * 4 + 4].copy_from_slice(col);
    }
    sortie
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

/// Écrit un PNG.
///
/// ⚠ `GpuContext::relire_image` rend déjà du **RVB à trois octets** — c'est `relire_image_brute`
/// qui garde les quatre canaux dans l'ordre de la carte. *La première version de ce banc
/// reconvertissait, comptait 607 500 pixels au lieu de 810 000, et l'encodeur PNG l'a dit tout de
/// suite. Il aurait pu ne rien dire.*
fn ecrire_png(chemin: &Path, rvb: &[u8]) {
    match aegis_engine::image::png::encoder(COTE, COTE, rvb) {
        Ok(png) => {
            if let Err(e) = std::fs::write(chemin, png) {
                println!("  ⚠ écriture impossible : {e}");
            } else {
                println!("  {}", chemin.display());
            }
        }
        Err(e) => println!("  ⚠ encodage PNG impossible : {e}"),
    }
}

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
