//! **LE MOTEUR ÉCRIT ENFIN DANS UNE MÉMOIRE — l'étage 0, geste 1.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example surface --no-default-features
//! cargo run --release -p aegis_engine --example surface --no-default-features -- <fichier.glb> <k>
//! ```
//!
//! ## Le trou qu'il ferme
//!
//! Le 6 septembre 2026, une sonde a établi un fait qu'aucun document du projet ne portait :
//!
//! ```text
//! passes de calcul vivantes (cmd_dispatch) : AUCUNE
//! tampons de stockage vivants              : AUCUN
//! ```
//!
//! Le moteur ne savait que rastériser vers des attachements. **Or toute la thèse — *l'état vit sur
//! la surface, et un shader le fait évoluer* — repose entièrement sur le geste inverse : écrire
//! dans une mémoire persistante depuis un shader.** Le mécanisme primitif dont dépendait le plan à
//! cinq étages n'existait pas, et les trois documents qui l'annonçaient raisonnaient sur le papier.
//!
//! *C'est la famille de défauts n° 1 du projet sous une forme neuve : non pas du code mort, mais du
//! code jamais écrit, masqué par des plans qui le supposaient acquis.*
//!
//! ## Ce que ce banc prouve, et comment
//!
//! 1. **Une passe de calcul tourne** — le premier `cmd_dispatch` du moteur.
//! 2. **L'adresse barycentrique est juste** — chaque micro-sommet est comparé à un calcul refait
//!    indépendamment sur le processeur.
//! 3. **Le format tient le budget** — les octets sont comptés, pas estimés.
//!
//! ## ⚠ Ce qu'il ne prouve PAS, et qu'il ne faut pas lire entre les lignes
//!
//! - **Aucune image.** L'écran ne lit pas encore cette mémoire ; c'est le geste 2. *Montrer une
//!   jolie capture ici ne prouverait rien de ce qui précède.*
//! - **Aucune allocation.** La subdivision est **uniforme**, alors que le banc `topologie` mesure
//!   des aires de triangles qui varient de **20 610 ×** sur un `.glb` ordinaire. Le chantier 0.2
//!   n'est pas commencé.
//! - **Rien sur un Adreno 650.** Les pourcentages de budget sont `CALCULÉ`s depuis des specs de
//!   seconde main, et ils servent à **éliminer** un format, jamais à en valider un.
//! - ⚠ **La vérification est une ré-implémentation.** Le processeur refait le même calcul dans un
//!   autre langage, par un autre chemin — c'est ce qu'on peut faire de mieux sans machine tierce,
//!   mais une erreur de *conception* partagée par les deux passerait. *Le dire vaut mieux que de
//!   laisser croire à une preuve indépendante.*

use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::Vec3;
use aegis_engine::core::memory::MemoryManager;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::render::surface::{
    depuis_rang, micro_sommets, MemoireDeSurface, PasseDeSurface, Reglages, OCTETS_PAR_ENTREE,
};
use ash::vk;
use std::path::PathBuf;

const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";

/// La subdivision par défaut : $k = 3$, soit 8 segments par arête et 45 micro-sommets par triangle.
///
/// *Assez pour que l'adressage soit non trivial, assez petit pour que la vérification processeur
/// reste immédiate.*
const K_PAR_DEFAUT: u32 = 3;

/// La direction dans laquelle le soleil VOYAGE — de la lumière vers la surface.
const SOLEIL: [f32; 4] = [-0.4, -0.85, -0.35, 0.0];

/// Le budget du Quest 2, en octets de trafic mémoire par image. `CALCULÉ`, jamais mesuré.
const TRAFIC_PAR_IMAGE: f64 = 44_000_000_000.0 / 72.0;
const PIXELS_QUEST2: f64 = 2.0 * 1832.0 * 1920.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let chemin = match args.next() {
        Some(a) => PathBuf::from(a),
        None => racine_du_depot().join(MODELE_PAR_DEFAUT),
    };
    let k: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(K_PAR_DEFAUT);

    titre("AEGIS — LE MOTEUR ÉCRIT DANS UNE MÉMOIRE DE SURFACE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    let triangles = (scene.indices.len() / 3) as u32;
    println!(
        "  Scène   : {} parties, {} sommets, {} triangles",
        scene.parties.len(),
        scene.sommets.len(),
        triangles
    );
    println!("  Subdivision : k = {k}  →  {} segments par arête, {} micro-sommets par triangle",
        1u32 << k, micro_sommets(k));

    match remplir(&scene, triangles, k) {
        Ok(Some((mesure, entrees))) => rendre_compte(mesure, entrees, triangles, k),
        Ok(None) => println!("\n  ⚠ Aucun Vulkan joignable — le banc ne peut rien conclure."),
        Err(e) => println!("\n  ⛔ Échec : {e}"),
    }
}

struct Mesure {
    /// Le nombre de micro-sommets dont la valeur relue s'écarte du calcul processeur.
    faux: usize,
    /// Le plus grand écart observé, en valeur absolue sur un canal.
    pire_ecart: f32,
    /// Le nombre d'entrées restées à zéro — le symptôme d'une écriture qui n'a pas eu lieu.
    nulles: usize,
}

/// Alloue, remplit et relit la mémoire de surface. Rend `None` si aucun Vulkan n'est joignable.
fn remplir(
    scene: &aegis_engine::geometry::glb_loader::Scene,
    triangles: u32,
    k: u32,
) -> Result<Option<(Mesure, u32)>, Box<dyn std::error::Error>> {
    // Un contexte sans écran : ce banc n'affiche rien, et une fenêtre fausserait les durées.
    let gpu = match GpuContext::sans_ecran(64, 64, 1) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ {e}");
            return Ok(None);
        }
    };
    let memory_props =
        unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

    // ── Les deux entrées du shader, en tampons de stockage ──────────────────────────────────
    //
    // ⚠ `GpuMesh::upload` crée des tampons de SOMMETS et d'INDICES, que le shader de calcul ne
    // peut pas lire : un tampon doit porter `STORAGE_BUFFER` dans son usage pour être lié à un
    // descripteur de stockage. *La distinction est invisible à la lecture du code appelant, et
    // Vulkan ne la signale qu'à la validation.*
    let plats: Vec<f32> = scene
        .sommets
        .iter()
        .flat_map(|s| {
            s.position
                .iter()
                .chain(s.normal.iter())
                .chain(s.tangent.iter())
                .chain(s.uv0.iter())
                .chain(s.uv1.iter())
                .copied()
        })
        .collect();
    let (tampon_sommets, mem_sommets, octets_sommets) =
        televerser(&gpu.device, &memory_props, &plats)?;
    let (tampon_indices, mem_indices, octets_indices) =
        televerser(&gpu.device, &memory_props, &scene.indices)?;

    let memoire = MemoireDeSurface::allouer(&gpu.device, &memory_props, triangles, k)?;
    let passe = PasseDeSurface::nouvelle(
        &gpu.device,
        tampon_sommets,
        octets_sommets,
        tampon_indices,
        octets_indices,
        &memoire,
    )?;

    let reglages = Reglages {
        triangles,
        cote: memoire.cote,
        par_triangle: memoire.par_triangle,
        _pad: 0,
        soleil: SOLEIL,
    };

    let cmd = gpu.begin_single_time_commands()?;
    passe.encoder(&gpu.device, cmd, &reglages, &memoire);
    gpu.end_single_time_commands(cmd)?;

    let relu = memoire.relire(&gpu.device)?;
    let mesure = confronter(scene, &relu, triangles, memoire.cote, memoire.par_triangle);
    let entrees = memoire.entrees;

    passe.detruire(&gpu.device);
    memoire.detruire(&gpu.device);
    unsafe {
        gpu.device.destroy_buffer(tampon_sommets, None);
        gpu.device.free_memory(mem_sommets, None);
        gpu.device.destroy_buffer(tampon_indices, None);
        gpu.device.free_memory(mem_indices, None);
    }
    Ok(Some((mesure, entrees)))
}

/// Refait, sur le processeur, le calcul que le shader a fait sur la carte, et compare.
///
/// ⚠ **La tolérance n'est pas une commodité, elle est imposée par le format.** Une entrée est
/// stockée en demi-flottants : sa précision relative est de ~2⁻¹¹, soit ~5·10⁻⁴ sur une valeur
/// proche de 1. Exiger l'égalité au bit près accuserait un code juste — *et le corpus porte déjà
/// cette leçon : ne jamais graver une empreinte d'image dans une assertion.*
fn confronter(
    scene: &aegis_engine::geometry::glb_loader::Scene,
    relu: &[[f32; 3]],
    triangles: u32,
    n: u32,
    par_triangle: u32,
) -> Mesure {
    const TOLERANCE: f32 = 2e-3;
    let soleil = Vec3::new(SOLEIL[0], SOLEIL[1], SOLEIL[2]).normalize();
    let mut m = Mesure { faux: 0, pire_ecart: 0.0, nulles: 0 };

    for t in 0..triangles {
        let ia = scene.indices[(t * 3) as usize] as usize;
        let ib = scene.indices[(t * 3 + 1) as usize] as usize;
        let ic = scene.indices[(t * 3 + 2) as usize] as usize;
        let (pa, pb, pc) = (
            Vec3::from_array(scene.sommets[ia].position),
            Vec3::from_array(scene.sommets[ib].position),
            Vec3::from_array(scene.sommets[ic].position),
        );
        let (na, nb, nc) = (
            Vec3::from_array(scene.sommets[ia].normal),
            Vec3::from_array(scene.sommets[ib].normal),
            Vec3::from_array(scene.sommets[ic].normal),
        );

        for r in 0..par_triangle {
            let (i, j) = depuis_rang(r, n);
            let u = i as f32 / n as f32;
            let v = j as f32 / n as f32;
            let w = 1.0 - u - v;

            let p = pa * w + pb * u + pc * v;
            let nrm = (na * w + nb * u + nc * v).normalize();
            let lambert = nrm.dot(soleil * -1.0).max(0.0);
            let teinte = [
                1.0 * (0.5 + 0.5 * (p.x * 3.0).sin()),
                0.85 * (0.5 + 0.5 * (p.y * 3.0).sin()),
                0.7 * (0.5 + 0.5 * (p.z * 3.0).sin()),
            ];
            let attendu = [teinte[0] * lambert, teinte[1] * lambert, teinte[2] * lambert];

            let obtenu = relu[(t * par_triangle + r) as usize];
            if obtenu == [0.0, 0.0, 0.0] && attendu.iter().any(|c| *c > TOLERANCE) {
                m.nulles += 1;
            }
            let ecart = (0..3).fold(0.0f32, |a, c| a.max((obtenu[c] - attendu[c]).abs()));
            m.pire_ecart = m.pire_ecart.max(ecart);
            if ecart > TOLERANCE {
                m.faux += 1;
            }
        }
    }
    m
}

fn rendre_compte(m: Mesure, entrees: u32, triangles: u32, k: u32) {
    titre("LA VÉRIFICATION — chaque micro-sommet, refait sur le processeur");
    println!("  Critères écrits AVANT la mesure :");
    println!("   · toute entrée écarte de plus de 2·10⁻³ du calcul processeur → l'adresse ou le");
    println!("     shader est faux");
    println!("   · toute entrée restée à zéro là où de la lumière était attendue → l'écriture n'a");
    println!("     pas eu lieu, ou la barrière manque\n");

    println!("  entrées vérifiées      : {entrees}");
    println!("  entrées fausses        : {}", m.faux);
    println!("  entrées restées nulles : {}", m.nulles);
    println!("  pire écart observé     : {:.2e}  (tolérance 2,00e-3, imposée par le demi-flottant)", m.pire_ecart);

    if m.faux == 0 && m.nulles == 0 {
        println!("\n  ⇒ ✅ Le shader a écrit, à la bonne adresse, la bonne valeur.");
        println!("     La passe de calcul du moteur est vivante, et l'adressage barycentrique");
        println!("     (T, u, v) se calcule sans aucune recherche spatiale.");
    } else {
        println!("\n  ⇒ ⛔ ÉCHEC. Ne pas chercher l'explication ici : lire l'écart et les nulles.");
        println!("     Beaucoup de nulles et peu de fausses = l'écriture n'arrive pas (barrière,");
        println!("     usage du tampon). L'inverse = l'adresse ou le calcul est faux.");
    }

    titre("LE CHIFFRE — ce que ce format coûterait sur le Quest 2");
    let octets = entrees as u64 * OCTETS_PAR_ENTREE;
    println!("  format d'une entrée    : {OCTETS_PAR_ENTREE} octets (4 demi-flottants : R, G, B, réservé)");
    println!("  empreinte de la scène  : {} Ko pour {triangles} triangles à k = {k}", octets / 1024);
    let part = OCTETS_PAR_ENTREE as f64 * PIXELS_QUEST2 / TRAFIC_PAR_IMAGE;
    println!("  lue une fois par pixel : {:.1} % du trafic mémoire d'une image", part * 100.0);
    println!("                           (7,03 M pixels, 611 Mo par image à 72 Hz)");
    println!();
    println!("  Pour comparaison, `On-Surface Caches` (HPG 2024) paie 1 134 octets par entrée :");
    println!("   · hémisphère directionnel 8×8 en double : 1 024 o — 90,3 %");
    println!("   · harmoniques sphériques L2            :    54 o —  4,8 %");
    println!("   · pointeurs vers les entrées voisines  :    32 o —  2,8 %");
    println!("   · position + normale                   :    24 o —  2,1 %");
    println!("  ⚠ L'adresse barycentrique supprime les 56 derniers octets — soit 4,9 %, et pas");
    println!("     davantage. Les 90 % restants sont l'hémisphère, qu'elle paierait au même prix.");
    println!("     *L'adressage est une innovation de qualité, pas de budget. Le budget vient de");
    println!("     ce que l'entrée doit SAVOIR FAIRE, et celle-ci ne sait que se faire lire.*");

    titre("CE QUE CE BANC NE DIT PAS");
    println!("  · Aucune image : l'écran ne lit pas encore cette mémoire. C'est le geste suivant.");
    println!("  · Aucune allocation : k est UNIFORME, alors que les aires d'un `.glb` varient de");
    println!("    20 610 × (banc `topologie`). Le chantier 0.2 n'est pas commencé.");
    println!("  · Rien sur un Adreno 650 : les pourcentages sont CALCULÉS depuis des specs de");
    println!("    seconde main. Ils servent à éliminer un format, jamais à en valider un.");
    println!("  · La vérification est une ré-implémentation : une erreur de conception partagée");
    println!("    par les deux chemins passerait.");
}

/// Téléverse une tranche dans un tampon de stockage visible par le processeur.
fn televerser<T: Copy>(
    device: &ash::Device,
    memory_props: &vk::PhysicalDeviceMemoryProperties,
    donnees: &[T],
) -> Result<(vk::Buffer, vk::DeviceMemory, u64), Box<dyn std::error::Error>> {
    let octets = std::mem::size_of_val(donnees) as u64;
    let (tampon, memoire) = MemoryManager::create_buffer(
        device,
        memory_props,
        octets,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    unsafe {
        let ptr = device.map_memory(memoire, 0, octets, vk::MemoryMapFlags::empty())? as *mut T;
        std::ptr::copy_nonoverlapping(donnees.as_ptr(), ptr, donnees.len());
        device.unmap_memory(memoire);
    }
    Ok((tampon, memoire, octets))
}

fn racine_du_depot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titre(texte: &str) {
    println!("\n\x1b[1m{texte}\x1b[0m");
    println!("{}", "─".repeat(texte.chars().count()));
}
