//! **LA RÉÉCRITURE PARTIELLE — le geste qui rend « une mémoire et sa dérivée » littéral.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example reecriture --no-default-features
//! ```
//!
//! ## Ce qu'il met à l'épreuve
//!
//! Le 8 septembre au matin, deux choses ont été établies :
//!
//! - **le banc `persistance`** : quand la caméra bouge, 0,52 % du contenu cesse d'être vrai, mais
//!   96,3 % de la mémoire devait être déplacée — à cause de la forme de l'adresse ;
//! - **`render/placement.rs`** : l'adresse cesse de bouger, et le travail *épargnABLE* tombe à
//!   3,47 % par image, compactages compris.
//!
//! > ### ⚠ Mais « épargnable » n'est pas « épargné ». La passe de calcul réécrivait encore TOUT.
//!
//! Ce banc mesure le geste qui manquait : **ne recalculer que les triangles qui en ont besoin, et
//! prouver que la mémoire obtenue est la même.**
//!
//! ## ⭐ LE CRITÈRE, ÉCRIT AVANT LA MESURE
//!
//! On rend la même image de deux façons, depuis le même état de départ :
//!
//! | | |
//! |---|---|
//! | **la référence** | tout est recalculé, comme avant ce chantier |
//! | **la partielle** | on part de la mémoire de l'image précédente et on ne recalcule que la liste |
//!
//! **Pour chaque triangle, ses entrées doivent être IDENTIQUES AU BIT PRÈS entre les deux.** Aucune
//! tolérance : ce n'est pas une approximation qu'on introduit, c'est du travail qu'on évite. *Un
//! seuil ici masquerait exactement le défaut qu'on cherche — un triangle oublié dans la liste.*
//!
//! ## ⚠⚠ CE QUI REND CE BANC HONNÊTE, ET SANS QUOI IL NE PROUVERAIT RIEN
//!
//! 1. **Il faut avoir SAUTÉ des triangles.** Si la liste contient tout le maillage, les deux chemins
//!    sont le même chemin et la comparaison est vide. *Le banc le vérifie et refuse de conclure.*
//! 2. **On ne compare que les entrées OCCUPÉES.** L'arène a des trous — de la mémoire qu'aucun
//!    triangle ne revendique — et personne ne les écrit. *Ils diffèrent forcément, et les compter
//!    ferait échouer un code juste.*
//! 3. **La liste ne se réduit pas aux triangles relogés.** Un triangle peut garder sa place ET sa
//!    subdivision, et devoir quand même être refait parce qu'un **voisin** a changé : le raccord
//!    aligne le niveau d'une arête sur le minimum des deux triangles, donc son bord change.
//!    *L'oublier donnerait un banc vert et des coutures à l'écran.*
//!
//! ## Ce que ce banc ne prouve PAS
//!
//! - **Rien en millisecondes.** Il compare des octets, pas du temps.
//! - **Rien si la LOI change.** La réécriture partielle suppose que ce qu'on n'a pas recalculé est
//!   encore vrai. *Le jour où le soleil bouge, où un matériau change, ou où la géométrie se déforme,
//!   tout le monde est périmé — et rien ici ne le détecte.* C'est la limite conceptuelle du
//!   mécanisme, et elle n'est pas fermée.

use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::Vec3;
use aegis_engine::core::memory::MemoryManager;
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::render::allocation::{aires_ecran, encoder_pour_gpu, planifier, raccorder, Plan};
use aegis_engine::render::placement::{a_refaire, Placement};
use aegis_engine::render::surface::{
    micro_sommets, EntreesGeometrie, ListeDeTravail, MemoireDeSurface, PasseDeSurface, Reglages,
};
use ash::vk;
use std::path::PathBuf;

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
const COTE: f32 = 900.0;
const BUDGET: u64 = 4_000_000;
/// La direction dans laquelle le soleil voyage — de la lumière vers la surface.
const SOLEIL: [f32; 4] = [-0.45, -0.80, 0.40, 0.0];
/// La teinte et la fréquence du signal de test. *Elles vivent ici, pas dans le shader : le moteur
/// fournit ce qui est VRAI, le jeu ce qui est BEAU.*
const SIGNAL: [f32; 4] = [0.90, 0.80, 0.70, 3.0];

fn main() {
    let chemin = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| racine_du_depot().join(MODELE_PAR_DEFAUT));

    titre("AEGIS — LA RÉÉCRITURE PARTIELLE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    println!("  Scène   : {} triangles · budget {} Mo\n", scene.indices.len() / 3, BUDGET / 1_000_000);
    println!("  ⭐ CRITÈRE ÉCRIT AVANT : pour chaque triangle, ses entrées doivent être IDENTIQUES");
    println!("     AU BIT PRÈS entre le rendu partiel et le rendu total. Aucune tolérance — ce");
    println!("     n'est pas une approximation qu'on introduit, c'est du travail qu'on évite.");

    match mesurer(&scene) {
        Ok(Some(())) => {}
        Ok(None) => println!("\n  ⚠ Aucun Vulkan joignable — le banc ne peut rien conclure."),
        Err(e) => println!("\n  ⛔ Échec : {e}"),
    }
}

/// Les deux plans successifs et ce qui les sépare.
struct Etape {
    plan: Plan,
    /// Les triangles qu'il faut recalculer pour passer de l'étape précédente à celle-ci.
    a_refaire: Vec<u32>,
}

fn mesurer(scene: &Scene) -> Result<Option<()>, Box<dyn std::error::Error>> {
    let triangles = (scene.indices.len() / 3) as u32;
    let positions: Vec<[f32; 3]> = scene.sommets.iter().map(|s| s.position).collect();

    // ── Les deux étapes, calculées sur le processeur avant de toucher au GPU ─────────────────
    let mut placement = Placement::nouveau(triangles as usize);
    let etape0 = etape(scene, &positions, &mut placement, 0.0, None);
    // Une rotation franche : assez pour reloger des triangles, pas assez pour tout changer.
    let etape1 = etape(scene, &positions, &mut placement, 12.0, Some(&etape0.plan));

    println!("\n  Étape 0 : cadrage initial · {} triangles à écrire", etape0.a_refaire.len());
    println!("  Étape 1 : caméra tournée de 12° · {} triangles à refaire ({:.2} %)",
        etape1.a_refaire.len(),
        etape1.a_refaire.len() as f64 / triangles as f64 * 100.0);

    // ── ⚠ La garde anti-test-creux, avant toute conclusion ───────────────────────────────────
    if etape1.a_refaire.len() as u32 >= triangles {
        println!("\n  ⛔ LA LISTE CONTIENT TOUT LE MAILLAGE : les deux chemins sont le MÊME");
        println!("     chemin, et cette comparaison ne prouverait rien. Ne rien conclure.");
        return Ok(Some(()));
    }
    if etape1.a_refaire.is_empty() {
        println!("\n  ⛔ LA LISTE EST VIDE : rien n'a bougé, donc rien n'est mis à l'épreuve.");
        return Ok(Some(()));
    }

    // ── Le rendu PARTIEL : étape 0 complète, puis seulement la liste ─────────────────────────
    let partielle = match rendre(scene, triangles, &[(&etape0, true), (&etape1, false)])? {
        Some(m) => m,
        None => return Ok(None),
    };
    // ── Le rendu de RÉFÉRENCE : l'étape 1 recalculée entièrement, depuis rien ────────────────
    let reference = match rendre(scene, triangles, &[(&etape1, true)])? {
        Some(m) => m,
        None => return Ok(None),
    };

    // ── La comparaison, sur les entrées OCCUPÉES seulement ───────────────────────────────────
    let mut comparees = 0u64;
    let mut differentes = 0u64;
    let mut triangles_fautifs = Vec::new();
    for t in 0..triangles {
        let (base, cote) = etape1.plan.par_triangle[t as usize];
        let n = micro_sommets(cote.trailing_zeros());
        let mut faux = false;
        for e in base..base + n {
            comparees += 1;
            if partielle[e as usize] != reference[e as usize] {
                differentes += 1;
                faux = true;
            }
        }
        if faux && triangles_fautifs.len() < 8 {
            triangles_fautifs.push(t);
        }
    }

    println!("\n  Entrées comparées (occupées) : {comparees}");
    println!("  Entrées différentes          : {differentes}");
    println!("  ⚠ Les TROUS de l'arène ne sont pas comparés : personne ne les écrit, donc ils");
    println!("    diffèrent forcément — les compter ferait échouer un code juste.");

    titre("LE VERDICT — contre le critère écrit AVANT la mesure");
    if differentes == 0 {
        let epargne = 1.0 - etape1.a_refaire.len() as f64 / triangles as f64;
        println!("  ⇒ ✅ IDENTIQUES AU BIT PRÈS, en ne recalculant que {} triangles sur {triangles}.",
            etape1.a_refaire.len());
        println!("     **{:.1} % du travail n'a pas été fait, et l'image ne s'en aperçoit pas.**", epargne * 100.0);
        println!();
        println!("     *La mémoire de surface est désormais un ÉTAT qui persiste, et le shader la loi");
        println!("     qui le fait évoluer. « Texture = shader » cesse d'être une intention.*");
    } else {
        println!("  ⇒ ⛔ {differentes} ENTRÉES DIFFÈRENT sur {comparees}. La liste de travail oublie");
        println!("     du monde — premiers triangles fautifs : {triangles_fautifs:?}");
        println!("     *Une entrée périmée ne lève aucune erreur : elle rend une image presque juste.*");
    }
    println!();
    println!("  ⚠ Ce banc ne dit RIEN en millisecondes, et RIEN si la LOI change : le jour où le");
    println!("    soleil bouge ou qu'un matériau change, tout est périmé et rien ici ne le détecte.");
    Ok(Some(()))
}

/// Construit une étape : le plan à cette position de caméra, et ce qu'il faut refaire pour y arriver.
fn etape(
    scene: &Scene,
    positions: &[[f32; 3]],
    placement: &mut Placement,
    angle: f32,
    precedent: Option<&Plan>,
) -> Etape {
    let vp = camera_a(scene, angle);
    let aires = aires_ecran(positions, &scene.indices, &vp, COTE, COTE);
    let mut modele = planifier(&aires, BUDGET);
    raccorder(&mut modele, positions, &scene.indices);
    let k: Vec<u32> = modele.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();
    let deplacement = placement.mettre_a_jour(&k, BUDGET);
    let plan = placement.appliquer(&modele);

    let a_refaire = match precedent {
        // La première étape écrit tout : il n'y a rien à conserver.
        None => (0..plan.par_triangle.len() as u32).collect(),
        // ⭐ La logique vit dans le MOTEUR, pas ici — et elle y est gardée par un test.
        //
        // *Elle a d'abord été écrite dans ce banc, et une mutation a montré qu'en oubliant son
        // second terme on passait de 210 à 99 triangles avec 209 entrées fausses. Une logique aussi
        // facile à casser n'a rien à faire dans un exemple, où aucune garde ne la protège.*
        Some(avant) => a_refaire(&deplacement, &avant.aretes, &plan.aretes),
    };
    Etape { plan, a_refaire }
}

/// Rend une suite d'étapes sur le GPU et relit la mémoire de surface obtenue.
///
/// Chaque étape porte un drapeau `complet` : quand il est vrai, on écrit tout le maillage ; sinon on
/// n'écrit que sa liste. *La mémoire, elle, n'est allouée qu'une fois — c'est tout l'objet.*
fn rendre(
    scene: &Scene,
    triangles: u32,
    etapes: &[(&Etape, bool)],
) -> Result<Option<Vec<[f32; 3]>>, Box<dyn std::error::Error>> {
    let gpu = match GpuContext::sans_ecran(64, 64, 1) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ {e}");
            return Ok(None);
        }
    };
    let props = unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

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

    // ⚠ Le plan change d'une étape à l'autre : son tampon est alloué à la taille du maillage et
    // RÉÉCRIT, jamais recréé. *Recréer un tampon lié à un descripteur exigerait de relier le
    // descripteur, et c'est exactement le genre de dépendance implicite que ce projet paie cher.*
    let mots_max = vec![0u32; triangles as usize * 2];
    let (b_plan, m_plan, o_plan) = televerser(&gpu.device, &props, &mots_max)?;

    // La mémoire de surface : allouée UNE fois, pour la plus grande arène de la suite.
    let entrees_max = etapes.iter().map(|(e, _)| e.plan.entrees).max().unwrap_or(3);
    let memoire = MemoireDeSurface::allouer_selon(
        &gpu.device,
        &props,
        &Plan { entrees: entrees_max, ..etapes[0].0.plan.clone() },
    )?;
    let mut liste = ListeDeTravail::allouer(&gpu.device, &props, triangles)?;

    let passe = PasseDeSurface::nouvelle(
        &gpu.device,
        &EntreesGeometrie {
            sommets: (b_sommets, o_sommets),
            indices: (b_indices, o_indices),
            plan: (b_plan, o_plan),
            a_refaire: (liste.tampon, liste.octets()),
        },
        &memoire,
    )?;

    for (etape, complet) in etapes {
        // Le plan de cette étape.
        ecrire_tampon(&gpu.device, m_plan, &encoder_pour_gpu(&etape.plan))?;
        // La liste de travail de cette étape.
        if *complet {
            liste.tout(&gpu.device)?;
        } else {
            liste.ecrire(&gpu.device, &etape.a_refaire)?;
        }
        // ⚠ `rangs_max` se calcule sur la LISTE, pas sur le maillage : dispatcher en X sur le plus
        // subdivisé de toute la scène lancerait des fils que le shader ferait sortir aussitôt.
        let rangs_max = etape
            .a_refaire
            .iter()
            .map(|t| micro_sommets(etape.plan.par_triangle[*t as usize].1.trailing_zeros()))
            .max()
            .unwrap_or(3);
        let rangs_max = if *complet {
            etape.plan.par_triangle.iter()
                .map(|(_, c)| micro_sommets(c.trailing_zeros()))
                .max().unwrap_or(3)
        } else {
            rangs_max
        };

        let reglages = Reglages {
            a_refaire: liste.longueur,
            cote: memoire.cote,
            par_triangle: memoire.par_triangle,
            _pad: 0,
            soleil: SOLEIL,
            signal: SIGNAL,
        };
        let cmd = gpu.begin_single_time_commands()?;
        passe.encoder(&gpu.device, cmd, &reglages, &memoire, rangs_max);
        gpu.end_single_time_commands(cmd)?;
    }

    let sortie = memoire.relire(&gpu.device)?;
    passe.detruire(&gpu.device);
    liste.detruire(&gpu.device);
    memoire.detruire(&gpu.device);
    Ok(Some(sortie))
}

fn ecrire_tampon<T: Copy>(
    device: &ash::Device,
    memoire: vk::DeviceMemory,
    donnees: &[T],
) -> Result<(), Box<dyn std::error::Error>> {
    let octets = std::mem::size_of_val(donnees) as u64;
    unsafe {
        let ptr = device.map_memory(memoire, 0, octets, vk::MemoryMapFlags::empty())? as *mut T;
        std::ptr::copy_nonoverlapping(donnees.as_ptr(), ptr, donnees.len());
        device.unmap_memory(memoire);
    }
    Ok(())
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

/// La caméra du banc, tournée de `angle` degrés autour de l'axe vertical.
fn camera_a(scene: &Scene, angle_degres: f32) -> [f32; 16] {
    let (centre, rayon) = boite_englobante(scene);
    let fov = 55_f32.to_radians();
    let recul = rayon * 1.3 / (fov * 0.5).tan();
    let direction = Vec3::new(0.55, 0.40, -0.73).normalize();
    let oeil = centre + direction * recul;
    let vue = (centre - oeil).normalize();
    let (s, c) = angle_degres.to_radians().sin_cos();
    let tournee = Vec3::new(vue.x * c + vue.z * s, vue.y, -vue.x * s + vue.z * c);

    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, oeil + tournee * recul, 1.0);
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

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
