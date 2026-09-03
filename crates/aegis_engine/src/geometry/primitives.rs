use crate::geometry::vertex::Vertex;
use crate::core::math::{Vec2, Vec3, Vec4};

/// Générateur de Géométries Procédurales 3D (Procedural Primitive Generator).
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

    /// Génère un Cube 3D (2.5D Voxel) centré à l'origine avec 24 sommets (4 par face) et 36 indices.
    pub fn create_cube(sx: f32, sy: f32, sz: f32) -> (Vec<Vertex>, Vec<u32>) {
        let hx = sx * 0.5;
        let hy = sy * 0.5;
        let hz = sz * 0.5;

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let faces = [
            (Vec3::Z, Vec4::new(1.0, 0.0, 0.0, 1.0), [Vec3::new(-hx, -hy, hz), Vec3::new(hx, -hy, hz), Vec3::new(hx, hy, hz), Vec3::new(-hx, hy, hz)]),
            (Vec3::NEG_Z, Vec4::new(-1.0, 0.0, 0.0, 1.0), [Vec3::new(hx, -hy, -hz), Vec3::new(-hx, -hy, -hz), Vec3::new(-hx, hy, -hz), Vec3::new(hx, hy, -hz)]),
            (Vec3::Y, Vec4::new(1.0, 0.0, 0.0, 1.0), [Vec3::new(-hx, hy, hz), Vec3::new(hx, hy, hz), Vec3::new(hx, hy, -hz), Vec3::new(-hx, hy, -hz)]),
            (Vec3::NEG_Y, Vec4::new(1.0, 0.0, 0.0, 1.0), [Vec3::new(-hx, -hy, -hz), Vec3::new(hx, -hy, -hz), Vec3::new(hx, -hy, hz), Vec3::new(-hx, -hy, hz)]),
            (Vec3::X, Vec4::new(0.0, 0.0, -1.0, 1.0), [Vec3::new(hx, -hy, hz), Vec3::new(hx, -hy, -hz), Vec3::new(hx, hy, -hz), Vec3::new(hx, hy, hz)]),
            (Vec3::NEG_X, Vec4::new(0.0, 0.0, 1.0, 1.0), [Vec3::new(-hx, -hy, -hz), Vec3::new(-hx, -hy, hz), Vec3::new(-hx, hy, hz), Vec3::new(-hx, hy, -hz)]),
        ];

        for (normal, tangent, corners) in faces {
            let base_idx = vertices.len() as u32;

            vertices.push(Vertex::new(corners[0], normal, tangent, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)));
            vertices.push(Vertex::new(corners[1], normal, tangent, Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.0)));
            vertices.push(Vertex::new(corners[2], normal, tangent, Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0)));
            vertices.push(Vertex::new(corners[3], normal, tangent, Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)));

            indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx + 2, base_idx + 3, base_idx]);
        }

        (vertices, indices)
    }

    /// Génère un maillage 3D détaillé pour un petit bonhomme mignon (Mime/Aventurier Voxel) de 1.75 blocs de haut.
    pub fn create_character_mesh() -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let add_sub_cube = |verts: &mut Vec<Vertex>, inds: &mut Vec<u32>, offset: Vec3, size: Vec3| {
            let (sub_verts, sub_inds) = Self::create_cube(size.x, size.y, size.z);
            let base_idx = verts.len() as u32;
            for mut v in sub_verts {
                v.position[0] += offset.x;
                v.position[1] += offset.y;
                v.position[2] += offset.z;
                verts.push(v);
            }
            for i in sub_inds {
                inds.push(base_idx + i);
            }
        };

        // 1. Jambes & Bottes (y = 0.0 à 0.5)
        add_sub_cube(&mut vertices, &mut indices, Vec3::new(0.0, 0.25, 0.0), Vec3::new(0.5, 0.5, 0.45));

        // 2. Torso / Pull Cozy (y = 0.5 à 1.15)
        add_sub_cube(&mut vertices, &mut indices, Vec3::new(0.0, 0.825, 0.0), Vec3::new(0.65, 0.65, 0.5));

        // 3. Tête (y = 1.15 à 1.6)
        add_sub_cube(&mut vertices, &mut indices, Vec3::new(0.0, 1.375, 0.0), Vec3::new(0.55, 0.45, 0.5));

        // 4. Bonnet Cozy (y = 1.6 à 1.75)
        add_sub_cube(&mut vertices, &mut indices, Vec3::new(0.0, 1.675, 0.0), Vec3::new(0.6, 0.15, 0.55));

        (vertices, indices)
    }

    /// Génère une Sphère 3D.
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

                // ⚠⚠ L'ORDRE A ÉTÉ INVERSÉ LE 3 SEPTEMBRE 2026, et il était faux depuis toujours.
                //
                // 992 triangles sur 1024 tournaient dans le sens HORAIRE vu de l'extérieur de la
                // bille, c'est-à-dire à l'envers de la convention du moteur
                // (`FrontFace::COUNTER_CLOCKWISE`). *Le défaut était invisible parce que tout le
                // moteur dessinait avec `cull_mode: NONE` : personne n'avait jamais demandé à
                // Vulkan de distinguer l'avant de l'arrière.* La première passe qui l'a demandé —
                // les deux cartes de la matière — a capturé la face opposée sur 100 % des pixels.
                //
                // ⚠ Et il s'en est fallu de peu que je corrige le PIPELINE : une carte inversée a
                // exactement deux causes possibles (le maillage tourne à l'envers, ou la
                // convention d'écran est inversée), et l'image ne les distingue pas. C'est une
                // sonde qui calcule le sens sur le PROCESSEUR, sans GPU, qui a tranché —
                // `le_maillage_tourne_dans_le_sens_direct_vu_du_dehors`.
                indices.push(first);
                indices.push(first + 1);
                indices.push(second);

                indices.push(second);
                indices.push(first + 1);
                indices.push(second + 1);
            }
        }

        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_mesh_generation() {
        let (vertices, indices) = Primitives::create_character_mesh();
        assert!(vertices.len() > 0);
        assert!(indices.len() > 0);
    }
}
