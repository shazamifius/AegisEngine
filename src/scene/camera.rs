use glam::{Mat4, Vec3};

/// Caméra 3D avec support de la Stéréoscopie VR (Œil Gauche / Œil Droit) et de la Projection Perspective.
///
/// ### Théorie Graphique :
/// La caméra génère les matrices de Vue (View) et de Projection (Projection).
/// En VR, la matrice de vue est décalée latéralement d'une demi-distance inter-oculaire (IPD/2)
/// pour simuler l'écartement naturel de la rétine humaine.
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub aspect_ratio: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    /// Crée une nouvelle caméra avec des paramètres de perspective standard.
    pub fn new(position: Vec3, target: Vec3, aspect_ratio: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y_radians: 60.0f32.to_radians(),
            aspect_ratio,
            z_near: 0.1,
            z_far: 1000.0,
        }
    }

    /// Calcule la Matrice de Vue globale (View Matrix) pour un observateur unique.
    pub fn compute_view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Calcule la Matrice de Projection Perspective avec inversion de l'axe Y pour Vulkan.
    ///
    /// Note : Dans Vulkan, les coordonnées d'écran Y vont de -1 (haut) à +1 (bas),
    /// contrairement à OpenGL. La matrice de projection inverse donc l'axe Y.
    pub fn compute_projection_matrix(&self) -> Mat4 {
        let mut proj = Mat4::perspective_rh(self.fov_y_radians, self.aspect_ratio, self.z_near, self.z_far);
        // Inverse l'axe Y pour la convention de viewport Vulkan
        proj.y_axis.y *= -1.0;
        proj
    }

    /// Calcule les Matrices de Vue Stéréoscopiques séparées pour l'Œil Gauche (0) et l'Œil Droit (1).
    ///
    /// ### Effet Stéréoscopique :
    /// En décalant le centre optique de `-ipd/2` pour l'œil gauche et `+ipd/2` pour l'œil droit,
    /// on génère la disparité horizontale exacte ressentie par le système visuel humain.
    pub fn compute_stereo_views(&self, ipd_meters: f32) -> (Mat4, Mat4) {
        let half_ipd = ipd_meters * 0.5;
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(self.up).normalize();

        let left_pos = self.position - right * half_ipd;
        let right_pos = self.position + right * half_ipd;

        let left_view = Mat4::look_at_rh(left_pos, left_pos + forward, self.up);
        let right_view = Mat4::look_at_rh(right_pos, right_pos + forward, self.up);

        (left_view, right_view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_matrices_creation() {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 16.0 / 9.0);
        let view = cam.compute_view_matrix();
        let proj = cam.compute_projection_matrix();

        assert!(!view.is_nan());
        assert!(!proj.is_nan());

        // L'inversion de l'axe Y pour Vulkan doit être négative
        assert!(proj.y_axis.y < 0.0);
    }

    #[test]
    fn test_stereo_eye_separation() {
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0), 1.0);
        let (left_view, right_view) = cam.compute_stereo_views(0.064); // IPD de 64 mm

        // L'œil gauche et l'œil droit doivent produire des matrices de vue différentes
        assert_ne!(left_view, right_view);
    }
}
