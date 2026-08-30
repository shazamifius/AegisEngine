//! Shaders SPIR-V pré-compilés natifs de l'AegisEngine.

pub const BACKGROUND_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/background.vert.spv"));
pub const BACKGROUND_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/background.frag.spv"));
pub const PARTY_2D5_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/party_2d5.vert.spv"));
pub const PARTY_2D5_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/party_2d5.frag.spv"));
// ⚠ `_glass_dispersive.wgsl` DORT depuis le 29 aout 2026 (prefixe `_`, retire de `build.rs`).
// Il portait de la vraie dispersion chromatique, une absorption de Beer-Lambert et des chanfreins
// eclaires — du travail juste, et aucun pipeline ne l'a jamais cree. Il etait compile a chaque
// build et embarque dans le binaire : 21,5 Ko de SPIR-V, le plus gros du projet, pour rien.
// C'est la garde des couleurs qui l'a trouve, en cherchant autre chose.
pub const OMBRE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ombre.vert.spv"));
pub const OMBRE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ombre.frag.spv"));
pub const COMPOSITION_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/composition.vert.spv"));
pub const COMPOSITION_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/composition.frag.spv"));

// ⚠ Les trois passes du halo n'ont qu'une constante chacune : `build.rs` ecrit le meme SPIR-V
// pour le sommet et le fragment (les deux points d'entree vivent dans le meme module), et
// `Ecran` monte donc le meme module aux deux etages. Une seconde constante identique ne
// tromperait que son lecteur.
pub const HALO_EXTRACTION_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/halo_extraction.vert.spv"));
pub const HALO_DESCENTE_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/halo_descente.vert.spv"));
pub const HALO_MONTEE_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/halo_montee.vert.spv"));
