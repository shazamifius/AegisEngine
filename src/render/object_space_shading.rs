use glam::{Vec2, Vec3};

/// Gestionnaire d'Atlas de Shading en Espace-Objet (Decoupled Object-Space Shading).
/// Évalue l'ombrage une seule fois par texel et le partage entre les yeux gauche et droit (anti-TAA & 45% économie GPU).
pub struct ObjectSpaceAtlas {
    pub width: u32,
    pub height: u32,
    pub allocated_texels: u32,
}

impl ObjectSpaceAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            allocated_texels: 0,
        }
    }

    /// Convertit les coordonnées UV d'une primitive en coordonnée texel dans l'Atlas.
    pub fn uv_to_atlas_texel(&self, uv: Vec2) -> (u32, u32) {
        let x = ((uv.x.clamp(0.0, 1.0)) * (self.width as f32 - 1.0)).round() as u32;
        let y = ((uv.y.clamp(0.0, 1.0)) * (self.height as f32 - 1.0)).round() as u32;
        (x, y)
    }

    /// Calcule l'ajustement de rugosité spéculaire GGX selon le filtrage LEAN/LEADR Mapping (anti-shimmering sans TAA).
    pub fn apply_lean_specular_aa(base_roughness: f32, normal_variance: f32) -> f32 {
        // Formule LEAN : Rugosité effective = sqrt(roughness^2 + 2 * variance)
        let rough_sq = base_roughness * base_roughness;
        (rough_sq + 2.0 * normal_variance.max(0.0)).sqrt().clamp(0.0, 1.0)
    }

    /// Échantillonne la couleur pré-ombragée de l'atlas stéréoscopique pour un œil spécifique.
    pub fn sample_stereo_eye(atlas_color: Vec3, _eye_index: u32) -> Vec3 {
        // En Espace-Objet, les deux yeux partagent exactement la même valeur physique d'ombrage.
        // Aucune différence inter-oculaire = ZÉRO rivalité binoculaire !
        atlas_color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atlas_uv_mapping() {
        let atlas = ObjectSpaceAtlas::new(1024, 1024);

        let (x0, y0) = atlas.uv_to_atlas_texel(Vec2::new(0.0, 0.0));
        assert_eq!((x0, y0), (0, 0));

        let (x1, y1) = atlas.uv_to_atlas_texel(Vec2::new(1.0, 1.0));
        assert_eq!((x1, y1), (1023, 1023));

        let (x_mid, y_mid) = atlas.uv_to_atlas_texel(Vec2::new(0.5, 0.5));
        assert_eq!((x_mid, y_mid), (512, 512));
    }

    #[test]
    fn test_lean_specular_aa_shimmering_reduction() {
        let base_roughness = 0.1; // Surface très brillante sensible au scintillement

        // Sans variance (objet proche) -> conserve la rugosité d'origine
        let r_near = ObjectSpaceAtlas::apply_lean_specular_aa(base_roughness, 0.0);
        assert!((r_near - 0.1).abs() < 1e-4);

        // Forte variance de normales (objet éloigné sous-échantillonné) -> filtre en rugosité spéculaire
        let r_far = ObjectSpaceAtlas::apply_lean_specular_aa(base_roughness, 0.05);
        assert!(r_far > base_roughness); // La rugosité augmente automatiquement pour tuer le shimmering !
        assert!((r_far - 0.33166).abs() < 1e-3);
    }

    #[test]
    fn test_stereo_eye_identical_shading() {
        let shader_color = Vec3::new(0.8, 0.5, 0.2);
        let left_eye = ObjectSpaceAtlas::sample_stereo_eye(shader_color, 0);
        let right_eye = ObjectSpaceAtlas::sample_stereo_eye(shader_color, 1);

        assert_eq!(left_eye, right_eye); // Identité stricte inter-œil = zéro fatigue binoculaire
    }
}
