use glam::{Mat3, Vec3};

/// Particule hydrodynamique pour le solveur MLS-MPM (Moving Least Squares Material Point Method).
#[derive(Debug, Clone, Copy)]
pub struct MlsMpmParticle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub affine_matrix: Mat3, // Matrice de gradient de vitesse affine C
    pub mass: f32,
    pub volume_ratio: f32,   // Ratio de déformation volumétrique J = det(F)
}

impl MlsMpmParticle {
    pub fn new(position: Vec3, velocity: Vec3, mass: f32) -> Self {
        Self {
            position,
            velocity,
            affine_matrix: Mat3::ZERO,
            mass: mass.max(1e-4),
            volume_ratio: 1.0,
        }
    }

    /// Calcule la pression de Cauchy pour un fluide incompressible (Eau, K = bulk modulus).
    pub fn compute_cauchy_pressure(&self, bulk_modulus: f32) -> f32 {
        bulk_modulus * (self.volume_ratio - 1.0)
    }

    /// Calcule le poids du filtre bilatéral étroit SSFR (Screen-Space Fluid Rendering) pour lisser la profondeur de surface.
    pub fn ssfr_bilateral_weight(spatial_dist_sq: f32, depth_diff: f32, spatial_sigma: f32, range_sigma: f32) -> f32 {
        let w_spatial = (-spatial_dist_sq / (2.0 * spatial_sigma * spatial_sigma)).exp();
        let w_range = (-(depth_diff * depth_diff) / (2.0 * range_sigma * range_sigma)).exp();
        w_spatial * w_range
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cauchy_pressure_incompressible_water() {
        let mut particle = MlsMpmParticle::new(Vec3::ZERO, Vec3::ZERO, 1.0);
        let k = 1000.0; // Bulk modulus de l'eau

        // Volume initial -> Pression zéro
        assert_eq!(particle.compute_cauchy_pressure(k), 0.0);

        // Compression du fluide (J = 0.95) -> Pression négative / forte poussée de répulsion
        particle.volume_ratio = 0.95;
        let p_comp = particle.compute_cauchy_pressure(k);
        assert!((p_comp - (-50.0)).abs() < 1e-3);

        // Expansion du fluide (J = 1.05) -> Pression positive de rappel
        particle.volume_ratio = 1.05;
        let p_exp = particle.compute_cauchy_pressure(k);
        assert!((p_exp - 50.0).abs() < 1e-3);
    }

    #[test]
    fn test_ssfr_narrow_range_bilateral_filter() {
        // Pixel identique (dist = 0, depth_diff = 0) -> Poids maximal 1.0
        let w_center = MlsMpmParticle::ssfr_bilateral_weight(0.0, 0.0, 2.0, 0.1);
        assert_eq!(w_center, 1.0);

        // Pixel sur bordure nette (forte discontinuité de profondeur depth_diff = 1.0m) -> Poids zéro (préserve les bordures d'éclaboussures !)
        let w_edge = MlsMpmParticle::ssfr_bilateral_weight(1.0, 1.0, 2.0, 0.1);
        assert!(w_edge < 1e-4);
    }
}
