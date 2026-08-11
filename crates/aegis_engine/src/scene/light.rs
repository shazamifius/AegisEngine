use crate::core::math::Vec3;

/// Type de Source Lumineuse (Light Type).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightType {
    Directional = 0, // Lumière du Soleil (infinie)
    Point = 1,       // Omnidirectionnelle (Ponctuelle)
    Spot = 2,        // Projecteur conique
}

/// Structure de Données de Lumière GPU PBR (GPU Light Data Structure).
///
/// Alignée à 16 octets pour correspondre aux exigences des Uniform / Storage Buffers Vulkan (std140).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpuLight {
    pub position_type: [f32; 4],   // (x, y, z, LightType as f32)
    pub color_intensity: [f32; 4], // (r, g, b, Intensity in Lux / Candela)
    pub direction_cutoff: [f32; 4], // (dx, dy, dz, Spot Cutoff Cosine)
}

impl GpuLight {
    pub fn new_directional(direction: Vec3, color: Vec3, intensity_lux: f32) -> Self {
        let dir_norm = direction.normalize();
        Self {
            position_type: [0.0, 0.0, 0.0, LightType::Directional as u32 as f32],
            color_intensity: [color.x, color.y, color.z, intensity_lux],
            direction_cutoff: [dir_norm.x, dir_norm.y, dir_norm.z, 0.0],
        }
    }

    pub fn new_point(position: Vec3, color: Vec3, intensity_candela: f32) -> Self {
        Self {
            position_type: [position.x, position.y, position.z, LightType::Point as u32 as f32],
            color_intensity: [color.x, color.y, color.z, intensity_candela],
            direction_cutoff: [0.0, 0.0, 0.0, 0.0],
        }
    }
}
