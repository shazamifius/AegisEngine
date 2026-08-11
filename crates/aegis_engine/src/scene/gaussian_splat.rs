use crate::core::math::{Quat, Vec3};

/// Structure de Splat Gaussien 3D (3D Gaussian Splatting).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GaussianSplat {
    pub position: [f32; 3],
    pub opacity: f32,
    pub scale: [f32; 3],
    pub reserved: f32,
    pub rotation: [f32; 4], // Quaternion (x, y, z, w)
    pub sh_order_0: [f32; 3], // Coefficients Harmoniques Sphériques de base
    pub _padding: f32,
}

impl GaussianSplat {
    /// Constante de normalisation des Harmoniques Sphériques d'ordre 0 (1 / (2 * sqrt(pi)))
    pub const SH_C0: f32 = 0.28209479;

    pub fn new(position: Vec3, scale: Vec3, rotation: Quat, opacity: f32, color_rgb: Vec3) -> Self {
        let sh_0 = color_rgb / Self::SH_C0;
        Self {
            position: position.to_array(),
            opacity: opacity.clamp(0.0, 1.0),
            scale: scale.to_array(),
            reserved: 0.0,
            rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
            sh_order_0: sh_0.to_array(),
            _padding: 0.0,
        }
    }

    /// Découplage EAG-PT : Extrait l'albédo rééclairable pour le Path Tracing.
    pub fn extract_eag_albedo(&self) -> Vec3 {
        Vec3::from_array(self.sh_order_0) * Self::SH_C0
    }
}
