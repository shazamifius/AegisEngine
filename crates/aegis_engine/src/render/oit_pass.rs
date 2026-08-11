use crate::core::math::Vec4;

/// Mode d'Order-Independent Transparency (OIT) sélectionné par le Render Graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OitMode {
    WeightedBlended, // WBOIT (Fast, Mobile Baseline)
    MomentBased,     // MBOIT (Précis, VR Baseline)
    LinkedList,      // A-Buffer exact (PC VR High-End)
}

/// Gestionnaire et fonctions mathématiques de l'Order-Independent Transparency.
pub struct OitManager {
    pub mode: OitMode,
    pub depth_range_far: f32,
}

impl OitManager {
    pub fn new(mode: OitMode, depth_range_far: f32) -> Self {
        Self {
            mode,
            depth_range_far: depth_range_far.max(1.0),
        }
    }

    /// Calcule le poids WBOIT (Weighted Blended OIT) de McGuire & Bavoil pour un fragment.
    pub fn compute_wboit_weight(&self, depth: f32, alpha: f32) -> f32 {
        let z_norm = (depth / self.depth_range_far).clamp(0.0, 1.0);
        let weight = alpha * (10.0f32.powf(-2.0).max(1000.0 * (1.0 - z_norm).powi(3)));
        weight.clamp(1e-2, 3e3)
    }

    /// Calcule le vecteur des 4 premiers moments de profondeur pour MBOIT.
    pub fn compute_mboit_moments(&self, depth: f32, alpha: f32) -> Vec4 {
        let z_norm = (depth / self.depth_range_far).clamp(0.0, 1.0);
        let z2 = z_norm * z_norm;
        let z3 = z2 * z_norm;
        let z4 = z3 * z_norm;

        Vec4::new(z_norm, z2, z3, z4) * alpha
    }

    /// Combine la couleur accumulée et l'opacité cumulée (WBOIT Composite Final).
    pub fn composite_wboit(accum_color_weight: Vec4, accum_alpha_product: f32) -> Vec4 {
        let alpha_total = 1.0 - accum_alpha_product;
        if alpha_total <= 1e-4 {
            return Vec4::ZERO;
        }

        let rgb_normalized = accum_color_weight.truncate() / accum_color_weight.w.max(1e-4);
        Vec4::new(rgb_normalized.x, rgb_normalized.y, rgb_normalized.z, alpha_total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wboit_weight_decay_with_depth() {
        let oit = OitManager::new(OitMode::WeightedBlended, 100.0);

        // Surface proche (z = 1m) -> Poids élevé
        let w_near = oit.compute_wboit_weight(1.0, 0.8);
        
        // Surface éloignée (z = 90m) -> Poids très faible
        let w_far = oit.compute_wboit_weight(90.0, 0.8);

        assert!(w_near > w_far);
        assert!(w_near > 10.0);
        assert!(w_far < 5.0);
    }

    #[test]
    fn test_mboit_moment_vector_scaling() {
        let oit = OitManager::new(OitMode::MomentBased, 100.0);
        let depth = 50.0; // z_norm = 0.5
        let alpha = 0.5;

        let moments = oit.compute_mboit_moments(depth, alpha);

        // b1 = 0.5 * 0.5 = 0.25
        assert_eq!(moments.x, 0.25);
        // b2 = 0.25 * 0.5 = 0.125
        assert_eq!(moments.y, 0.125);
        // b3 = 0.125 * 0.5 = 0.0625
        assert_eq!(moments.z, 0.0625);
        // b4 = 0.0625 * 0.5 = 0.03125
        assert_eq!(moments.w, 0.03125);
    }

    #[test]
    fn test_composite_wboit_normalization() {
        let accum_color_weight = Vec4::new(4.0, 2.0, 1.0, 2.0); // RGB ponderé = (2.0, 1.0, 0.5)
        let accum_alpha_product = 0.2; // Opacité totale = 1 - 0.2 = 0.8

        let final_color = OitManager::composite_wboit(accum_color_weight, accum_alpha_product);

        assert_eq!(final_color.x, 2.0);
        assert_eq!(final_color.y, 1.0);
        assert_eq!(final_color.z, 0.5);
        assert_eq!(final_color.w, 0.8);
    }
}
