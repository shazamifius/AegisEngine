use glam::Vec3;

/// Matériau Verre Dispersif avec absorption de Beer-Lambert et réfraction de Snell-Descartes.
#[derive(Debug, Clone)]
pub struct GlassMaterial {
    pub ior: f32,            // Indice de réfraction de base (ex: 1.52 pour le verre BK7)
    pub dispersion_coeff: f32, // Coefficient de dispersion chromatique de Cauchy (B)
    pub roughness: f32,      // Rugosité micro-facette GGX
}

impl GlassMaterial {
    pub fn new(ior: f32, dispersion_coeff: f32, roughness: f32) -> Self {
        Self {
            ior,
            dispersion_coeff,
            roughness,
        }
    }

    /// Calcule l'indice de réfraction $n(\lambda)$ pour la longueur d'onde de la lumière (Équation de Cauchy).
    ///
    /// $n(\lambda) = n_0 + \frac{B}{\lambda^2}$
    pub fn compute_cauchy_ior(&self, wavelength_nm: f32) -> f32 {
        let lambda_microns = wavelength_nm / 1000.0;
        self.ior + (self.dispersion_coeff / (lambda_microns * lambda_microns))
    }

    /// Évalue les indices de réfraction séparés pour le Rouge (650nm), Vert (550nm) et Bleu (450nm).
    pub fn compute_rgb_iors(&self) -> Vec3 {
        Vec3::new(
            self.compute_cauchy_ior(650.0), // Rouge
            self.compute_cauchy_ior(550.0), // Vert
            self.compute_cauchy_ior(450.0), // Bleu
        )
    }

    /// Calcule l'atténuation lumineuse volumétrique par la Loi de Beer-Lambert ($e^{-\sigma d}$).
    pub fn compute_beer_lambert_absorption(absorption_coeff: Vec3, distance: f32) -> Vec3 {
        Vec3::new(
            (-absorption_coeff.x * distance).exp(),
            (-absorption_coeff.y * distance).exp(),
            (-absorption_coeff.z * distance).exp(),
        )
    }

    /// Calcule le coefficient de réflexion de Fresnel via l'approximation de Schlick.
    pub fn compute_fresnel_schlick(cos_theta: f32, ior: f32) -> f32 {
        let r0 = ((1.0 - ior) / (1.0 + ior)).powi(2);
        r0 + (1.0 - r0) * (1.0 - cos_theta.clamp(0.0, 1.0)).powi(5)
    }

    /// Calcule le vecteur de réfraction selon la Loi de Snell-Descartes.
    ///
    /// Renvoie `None` en cas de Réflexion Totale Interne (TIR).
    pub fn compute_snell_refraction(incident: Vec3, normal: Vec3, ior_src: f32, ior_dst: f32) -> Option<Vec3> {
        let eta = ior_src / ior_dst;
        let cos_i = (-incident.dot(normal)).clamp(-1.0, 1.0);
        let sin2_t = eta * eta * (1.0 - cos_i * cos_i);

        if sin2_t > 1.0 {
            None // Réflexion totale interne
        } else {
            let cos_t = (1.0 - sin2_t).sqrt();
            Some(eta * incident + (eta * cos_i - cos_t) * normal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cauchy_dispersion_rgb() {
        let glass = GlassMaterial::new(1.5, 0.005, 0.0);
        let iors = glass.compute_rgb_iors();

        // Le bleu (450nm) doit être plus réfracté que le rouge (650nm)
        assert!(iors.z > iors.x);
    }

    #[test]
    fn test_beer_lambert_decay() {
        let sigma = Vec3::new(0.5, 1.0, 2.0);
        let decay_0m = GlassMaterial::compute_beer_lambert_absorption(sigma, 0.0);
        let decay_1m = GlassMaterial::compute_beer_lambert_absorption(sigma, 1.0);

        assert_eq!(decay_0m, Vec3::ONE);
        assert!(decay_1m.x > decay_1m.y);
        assert!(decay_1m.y > decay_1m.z);
    }

    #[test]
    fn test_snell_refraction_and_total_internal_reflection() {
        let n1 = 1.5; // Verre
        let n2 = 1.0; // Air
        let incident_steep = Vec3::new(0.9, -0.1, 0.0).normalize();
        let normal = Vec3::Y;

        // Angle rasant -> Réflexion totale interne (TIR)
        let refract_dir = GlassMaterial::compute_snell_refraction(incident_steep, normal, n1, n2);
        assert!(refract_dir.is_none());
    }

    #[test]
    fn test_fresnel_schlick_limits() {
        let f_normal = GlassMaterial::compute_fresnel_schlick(1.0, 1.5);
        let f_grazing = GlassMaterial::compute_fresnel_schlick(0.0, 1.5);

        assert!((f_normal - 0.04).abs() < 1e-3);
        assert_eq!(f_grazing, 1.0);
    }
}
