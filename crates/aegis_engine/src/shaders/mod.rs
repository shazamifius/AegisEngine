//! Shaders SPIR-V pré-compilés natifs de l'AegisEngine.

pub const BACKGROUND_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/background.vert.spv"));
pub const BACKGROUND_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/background.frag.spv"));
pub const PARTY_2D5_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/party_2d5.vert.spv"));
pub const PARTY_2D5_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/party_2d5.frag.spv"));
pub const GLASS_DISPERSIVE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glass_dispersive.vert.spv"));
pub const GLASS_DISPERSIVE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glass_dispersive.frag.spv"));
pub const OMBRE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ombre.vert.spv"));
pub const OMBRE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ombre.frag.spv"));
