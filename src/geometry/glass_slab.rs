use glam::{Vec2, Vec3, Vec4};
use crate::geometry::vertex::Vertex;

/// Générateur de Géométrie 3D pour Dalles de Verre avec Courbure Continuement Lisse (Smooth Stadium Slab).
pub struct GlassSlabGenerator;

impl GlassSlabGenerator {
    /// Génère un maillage 3D en forme de capsule (stadium) avec courbure 3D lissée continue.
    pub fn create_capsule_slab(
        length: f32,
        radius: f32,
        thickness: f32,
        bevel_radius: f32,
        radial_segments: u32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let half_len = length * 0.5;
        let half_thick = thickness * 0.5;
        let b_rad = bevel_radius.min(half_thick * 0.85);

        // 1. Contour 2D de la forme Stadium (Capsule)
        let num_arc_pts = radial_segments;
        let mut perimeter_pts_2d = Vec::new();
        let mut perimeter_normals_2d = Vec::new();

        // Arc Droite (Centre : +half_len, 0)
        for i in 0..=num_arc_pts {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32 / num_arc_pts as f32) * std::f32::consts::PI;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let pos = Vec2::new(half_len, 0.0) + dir * radius;
            perimeter_pts_2d.push(pos);
            perimeter_normals_2d.push(dir);
        }

        // Arc Gauche (Centre : -half_len, 0)
        for i in 0..=num_arc_pts {
            let angle = std::f32::consts::FRAC_PI_2 + (i as f32 / num_arc_pts as f32) * std::f32::consts::PI;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let pos = Vec2::new(-half_len, 0.0) + dir * radius;
            perimeter_pts_2d.push(pos);
            perimeter_normals_2d.push(dir);
        }

        let num_perim = perimeter_pts_2d.len();

        // -------------------------------------------------------------
        // Face Avant Lisse (Z = +half_thick)
        // -------------------------------------------------------------
        let center_front_idx = vertices.len() as u32;
        vertices.push(Vertex::new(
            Vec3::new(0.0, 0.0, half_thick),
            Vec3::Z,
            Vec4::new(1.0, 0.0, 0.0, 1.0),
            Vec2::new(0.5, 0.5),
            Vec2::ZERO,
        ));

        let front_ring_start = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let u = (pos2d.x / (length + 2.0 * radius)) + 0.5;
            let v = (pos2d.y / (2.0 * radius)) + 0.5;

            // Normale douce et continue vers les bords
            let face_norm = Vec3::new(norm2d.x * 0.25, norm2d.y * 0.25, 0.968).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, half_thick),
                face_norm,
                Vec4::new(1.0, 0.0, 0.0, 1.0),
                Vec2::new(u, v),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            indices.push(center_front_idx);
            indices.push(front_ring_start + i as u32);
            indices.push(front_ring_start + next_i as u32);
        }

        // -------------------------------------------------------------
        // Face Arrière Lisse (Z = -half_thick)
        // -------------------------------------------------------------
        let center_back_idx = vertices.len() as u32;
        vertices.push(Vertex::new(
            Vec3::new(0.0, 0.0, -half_thick),
            -Vec3::Z,
            Vec4::new(-1.0, 0.0, 0.0, 1.0),
            Vec2::new(0.5, 0.5),
            Vec2::ZERO,
        ));

        let back_ring_start = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let u = (pos2d.x / (length + 2.0 * radius)) + 0.5;
            let v = (pos2d.y / (2.0 * radius)) + 0.5;

            let face_norm = Vec3::new(norm2d.x * 0.25, norm2d.y * 0.25, -0.968).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, -half_thick),
                face_norm,
                Vec4::new(-1.0, 0.0, 0.0, 1.0),
                Vec2::new(u, v),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            indices.push(center_back_idx);
            indices.push(back_ring_start + next_i as u32);
            indices.push(back_ring_start + i as u32);
        }

        // -------------------------------------------------------------
        // Tranche Extérieure Continue (Side Wall)
        // -------------------------------------------------------------
        let side_wall_top = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let side_norm = Vec3::new(norm2d.x * 0.707, norm2d.y * 0.707, 0.707).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, half_thick),
                side_norm,
                Vec4::new(0.0, 0.0, 1.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 0.0),
                Vec2::ZERO,
            ));
        }

        let side_wall_bottom = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let side_norm = Vec3::new(norm2d.x * 0.707, norm2d.y * 0.707, -0.707).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, -half_thick),
                side_norm,
                Vec4::new(0.0, 0.0, 1.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 1.0),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            let top_curr = side_wall_top + i as u32;
            let top_next = side_wall_top + next_i as u32;
            let bot_curr = side_wall_bottom + i as u32;
            let bot_next = side_wall_bottom + next_i as u32;

            indices.push(top_curr);
            indices.push(bot_curr);
            indices.push(top_next);

            indices.push(top_next);
            indices.push(bot_curr);
            indices.push(bot_next);
        }

        (vertices, indices)
    }
}
