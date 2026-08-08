use crate::geometry::vertex::Vertex;
use glam::{Vec2, Vec3, Vec4};

/// Générateur de Géométries Procédurales 3D (Procedural Primitive Generator).
///
/// Permet de générer des maillages de test 3D (Sphère, Cube, Quad) avec coordonnées de normales,
/// tangentes et double UV (UV0 pour les textures, UV1 pour l'Atlas d'Espace-Objet).
pub struct Primitives;

impl Primitives {
    /// Génère un Quad 2D (Rectangle plan) centré à l'origine avec 4 sommets et 6 indices.
    pub fn create_quad(width: f32, height: f32) -> (Vec<Vertex>, Vec<u32>) {
        let hw = width * 0.5;
        let hh = height * 0.5;

        let vertices = vec![
            Vertex::new(Vec3::new(-hw, -hh, 0.0), Vec3::Z, Vec4::new(1.0, 0.0, 0.0, 1.0), Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
            Vertex::new(Vec3::new(hw, -hh, 0.0), Vec3::Z, Vec4::new(1.0, 0.0, 0.0, 1.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.0)),
            Vertex::new(Vec3::new(hw, hh, 0.0), Vec3::Z, Vec4::new(1.0, 0.0, 0.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0)),
            Vertex::new(Vec3::new(-hw, hh, 0.0), Vec3::Z, Vec4::new(1.0, 0.0, 0.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)),
        ];

        let indices = vec![0, 1, 2, 2, 3, 0];

        (vertices, indices)
    }

    /// Génère une Sphère 3D paramétrique UV avec `stacks` parallèles et `slices` méridiens.
    pub fn create_uv_sphere(radius: f32, stacks: u32, slices: u32) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for i in 0..=stacks {
            let v = i as f32 / stacks as f32;
            let phi = v * std::f32::consts::PI;

            for j in 0..=slices {
                let u = j as f32 / slices as f32;
                let theta = u * std::f32::consts::TAU;

                let x = theta.cos() * phi.sin();
                let y = phi.cos();
                let z = theta.sin() * phi.sin();

                let normal = Vec3::new(x, y, z).normalize();
                let position = normal * radius;
                let tangent = Vec4::new(-theta.sin(), 0.0, theta.cos(), 1.0);
                let uv = Vec2::new(u, v);

                vertices.push(Vertex::new(position, normal, tangent, uv, uv));
            }
        }

        for i in 0..stacks {
            for j in 0..slices {
                let first = i * (slices + 1) + j;
                let second = first + slices + 1;

                indices.push(first);
                indices.push(second);
                indices.push(first + 1);

                indices.push(second);
                indices.push(second + 1);
                indices.push(first + 1);
            }
        }

        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quad_generation() {
        let (vertices, indices) = Primitives::create_quad(2.0, 2.0);
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);
        assert_eq!(vertices[0].position, [-1.0, -1.0, 0.0]);
    }

    #[test]
    fn test_uv_sphere_generation() {
        let (vertices, indices) = Primitives::create_uv_sphere(1.0, 16, 32);
        assert!(vertices.len() > 0);
        assert!(indices.len() > 0);
        assert_eq!(indices.len() % 3, 0); // Triangles complets
    }
}
