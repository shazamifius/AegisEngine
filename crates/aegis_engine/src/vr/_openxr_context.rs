use crate::core::math::{Mat4, Vec3};

/// Mode de Fovéation VR (Variable Rate Shading - VRS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrFoveationMode {
    Disabled,
    Fixed,      // Fixed Foveated Rendering (FFR - Meta Quest)
    EyeTracked, // Eye-Tracked Foveated Rendering (ETFR - Meta Quest Pro / Tobii)
}

/// Paramètres du Foveated Rendering.
pub struct VrFoveationSettings {
    pub mode: VrFoveationMode,
    pub inner_radius: f32, // Zone 1x1
    pub mid_radius: f32,   // Zone 2x2
    pub outer_radius: f32, // Zone 4x4
}

impl Default for VrFoveationSettings {
    fn default() -> Self {
        Self {
            mode: VrFoveationMode::Fixed,
            inner_radius: 0.3, // 30% du rayon central à 1x1
            mid_radius: 0.6,   // 60% du rayon moyen à 2x2
            outer_radius: 1.0, // Périphérie à 4x4
        }
    }
}

/// Gestionnaire de la pose du casque VR (Late Latching & Stéréoscopie Multiview).
pub struct VrContext {
    pub ipd_distance: f32, // Interpupillary Distance (distance inter-oculaire, ex: 63mm = 0.063m)
    pub foveation: VrFoveationSettings,
}

impl VrContext {
    pub fn new(ipd_distance: f32) -> Self {
        Self {
            ipd_distance: ipd_distance.clamp(0.05, 0.08),
            foveation: VrFoveationSettings::default(),
        }
    }

    /// Calcule les matrices de vue pour l'œil gauche et l'œil droit (Single-Pass Multiview).
    pub fn compute_stereo_view_matrices(&self, head_pose: Mat4) -> (Mat4, Mat4) {
        let half_ipd = self.ipd_distance * 0.5;
        let left_eye_offset = Mat4::from_translation(Vec3::new(-half_ipd, 0.0, 0.0));
        let right_eye_offset = Mat4::from_translation(Vec3::new(half_ipd, 0.0, 0.0));

        let view_left = (head_pose * left_eye_offset).inverse();
        let view_right = (head_pose * right_eye_offset).inverse();

        (view_left, view_right)
    }

    /// Détermine le taux d'ombrage VRS (Shading Rate) pour une distance au centre optique.
    pub fn get_shading_rate_for_radius(&self, radius_norm: f32) -> (u32, u32) {
        if !matches!(self.foveation.mode, VrFoveationMode::Fixed | VrFoveationMode::EyeTracked) {
            return (1, 1);
        }

        if radius_norm <= self.foveation.inner_radius {
            (1, 1) // Centre optique : 1x1
        } else if radius_norm <= self.foveation.mid_radius {
            (2, 2) // Région intermédiaire : 2x2 (75% économie)
        } else {
            (4, 4) // Périphérie : 4x4 (93.75% économie)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_ipd_offset_calculation() {
        let vr = VrContext::new(0.064); // IPD de 64 mm
        let head_pose = Mat4::IDENTITY;

        let (v_left, v_right) = vr.compute_stereo_view_matrices(head_pose);

        // La position de l'œil gauche doit être à +0.032m (matrice de vue inversée)
        assert!((v_left.cols[3].x - 0.032).abs() < 1e-4);
        // La position de l'œil droit doit être à -0.032m
        assert!((v_right.cols[3].x + 0.032).abs() < 1e-4);
    }

    #[test]
    fn test_foveated_shading_rates() {
        let vr = VrContext::new(0.063);

        // Centre (r = 0.1) -> 1x1
        assert_eq!(vr.get_shading_rate_for_radius(0.1), (1, 1));

        // Milieu (r = 0.5) -> 2x2
        assert_eq!(vr.get_shading_rate_for_radius(0.5), (2, 2));

        // Périphérie (r = 0.9) -> 4x4
        assert_eq!(vr.get_shading_rate_for_radius(0.9), (4, 4));
    }
}
