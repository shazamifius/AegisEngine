use glam::{Quat, Vec3};

/// Structure de Splat Gaussien 3D (3D Gaussian Splatting).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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

    /// Convertit les coefficients SH d'ordre 0 en couleur RGB réelle.
    pub fn sh_to_rgb(&self) -> Vec3 {
        Vec3::from_slice(&self.sh_order_0) * Self::SH_C0
    }

    /// Extrait l'Albédo PBR découplé pour le relighting EAG-PT (Path Tracing sur Gaussiennes).
    pub fn extract_eag_albedo(&self) -> Vec3 {
        let rgb = self.sh_to_rgb();
        rgb.clamp(Vec3::ZERO, Vec3::ONE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sh_order_0_rgb_roundtrip() {
        let color_orig = Vec3::new(0.8, 0.4, 0.2);
        let splat = GaussianSplat::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY, 1.0, color_orig);

        let rgb_extracted = splat.sh_to_rgb();
        assert!((rgb_extracted.x - color_orig.x).abs() < 1e-4);
        assert!((rgb_extracted.y - color_orig.y).abs() < 1e-4);
        assert!((rgb_extracted.z - color_orig.z).abs() < 1e-4);
    }

    #[test]
    fn test_eag_albedo_clamping() {
        let overbright_color = Vec3::new(2.5, 1.2, 0.5);
        let splat = GaussianSplat::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY, 1.0, overbright_color);

        let albedo = splat.extract_eag_albedo();
        assert_eq!(albedo.x, 1.0); // Normalisé pour la réflectance PBR
        assert_eq!(albedo.y, 1.0);
        assert_eq!(albedo.z, 0.5);
    }
}
