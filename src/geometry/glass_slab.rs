use glam::{Vec2, Vec3, Vec4};
use crate::geometry::vertex::Vertex;

/// Générateur de Géométrie 3D pour Dalles de Verre avec Courbure et Chanfreins Continuement Lisses (Smooth Beveled Stadium Slab).
pub struct GlassSlabGenerator;

impl GlassSlabGenerator {
    /// Génère un maillage 3D en forme de capsule (stadium) avec profil biseauté tridimensionnel (chanfrein).
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
        let b_rad = bevel_radius.min(half_thick * 0.90).min(radius * 0.90);
        let r_inner = radius - b_rad;
        let z_inner = half_thick - b_rad;

        // 1. Points directeurs du périmètre 2D de la capsule
        let num_arc_pts = radial_segments;
        let mut centers_2d = Vec::new();
        let mut dirs_2d = Vec::new();

        // Arc Droite (Centre : +half_len, 0)
        for i in 0..=num_arc_pts {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32 / num_arc_pts as f32) * std::f32::consts::PI;
            let dir = Vec2::new(angle.cos(), angle.sin());
            centers_2d.push(Vec2::new(half_len, 0.0));
            dirs_2d.push(dir);
        }

        // Arc Gauche (Centre : -half_len, 0)
        for i in 0..=num_arc_pts {
            let angle = std::f32::consts::FRAC_PI_2 + (i as f32 / num_arc_pts as f32) * std::f32::consts::PI;
            let dir = Vec2::new(angle.cos(), angle.sin());
            centers_2d.push(Vec2::new(-half_len, 0.0));
            dirs_2d.push(dir);
        }

        let num_perim = centers_2d.len();

        // Helper pour calculer UVs
        let calc_uv = |pos: Vec3| -> Vec2 {
            let u = (pos.x / (length + 2.0 * radius)) + 0.5;
            let v = (pos.y / (2.0 * radius)) + 0.5;
            Vec2::new(u, v)
        };

        // -------------------------------------------------------------
        // A. Face Avant Plat (Z = +half_thick)
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
            let pos = centers_2d[i] + dirs_2d[i] * r_inner;
            let pos3d = Vec3::new(pos.x, pos.y, half_thick);
            vertices.push(Vertex::new(
                pos3d,
                Vec3::Z,
                Vec4::new(1.0, 0.0, 0.0, 1.0),
                calc_uv(pos3d),
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
        // B. Biseau Avant (Quarter-Circle Arc de Z = +half_thick vers Side Wall Z = +z_inner)
        // -------------------------------------------------------------
        let bevel_steps = 12u32;
        let mut prev_ring_start = front_ring_start;

        for step in 1..=bevel_steps {
            let t = step as f32 / bevel_steps as f32;
            let angle = t * std::f32::consts::FRAC_PI_2;
            let sin_a = angle.sin();
            let cos_a = angle.cos();

            let r_curr = r_inner + b_rad * sin_a;
            let z_curr = z_inner + b_rad * cos_a;

            let curr_ring_start = vertices.len() as u32;
            for i in 0..num_perim {
                let dir = dirs_2d[i];
                let pos2d = centers_2d[i] + dir * r_curr;
                let pos3d = Vec3::new(pos2d.x, pos2d.y, z_curr);
                let norm3d = Vec3::new(dir.x * sin_a, dir.y * sin_a, cos_a).normalize();

                vertices.push(Vertex::new(
                    pos3d,
                    norm3d,
                    Vec4::new(0.0, 1.0, 0.0, 1.0),
                    calc_uv(pos3d),
                    Vec2::new(i as f32 / num_perim as f32, t),
                ));
            }

            // Quadrilatères reliant prev_ring et curr_ring
            for i in 0..num_perim {
                let next_i = (i + 1) % num_perim;
                let p0 = prev_ring_start + i as u32;
                let p1 = prev_ring_start + next_i as u32;
                let c0 = curr_ring_start + i as u32;
                let c1 = curr_ring_start + next_i as u32;

                indices.push(p0);
                indices.push(c0);
                indices.push(p1);

                indices.push(p1);
                indices.push(c0);
                indices.push(c1);
            }

            prev_ring_start = curr_ring_start;
        }

        let front_bevel_end_ring = prev_ring_start;

        // -------------------------------------------------------------
        // C. Tranche Extérieure Principale (Side Wall de Z = +z_inner à Z = -z_inner)
        // -------------------------------------------------------------
        let side_wall_bottom_ring = vertices.len() as u32;
        for i in 0..num_perim {
            let dir = dirs_2d[i];
            let pos2d = centers_2d[i] + dir * radius;
            let pos3d = Vec3::new(pos2d.x, pos2d.y, -z_inner);
            let norm3d = Vec3::new(dir.x, dir.y, 0.0).normalize();

            vertices.push(Vertex::new(
                pos3d,
                norm3d,
                Vec4::new(0.0, 0.0, 1.0, 1.0),
                calc_uv(pos3d),
                Vec2::new(i as f32 / num_perim as f32, 1.0),
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            let top_curr = front_bevel_end_ring + i as u32;
            let top_next = front_bevel_end_ring + next_i as u32;
            let bot_curr = side_wall_bottom_ring + i as u32;
            let bot_next = side_wall_bottom_ring + next_i as u32;

            indices.push(top_curr);
            indices.push(bot_curr);
            indices.push(top_next);

            indices.push(top_next);
            indices.push(bot_curr);
            indices.push(bot_next);
        }

        // -------------------------------------------------------------
        // D. Biseau Arrière (Quarter-Circle Arc de Z = -z_inner vers Z = -half_thick)
        // -------------------------------------------------------------
        prev_ring_start = side_wall_bottom_ring;

        for step in (0..bevel_steps).rev() {
            let t = step as f32 / bevel_steps as f32;
            let angle = t * std::f32::consts::FRAC_PI_2;
            let sin_a = angle.sin();
            let cos_a = angle.cos();

            let r_curr = r_inner + b_rad * sin_a;
            let z_curr = -(z_inner + b_rad * cos_a);

            let curr_ring_start = vertices.len() as u32;
            for i in 0..num_perim {
                let dir = dirs_2d[i];
                let pos2d = centers_2d[i] + dir * r_curr;
                let pos3d = Vec3::new(pos2d.x, pos2d.y, z_curr);
                let norm3d = Vec3::new(dir.x * sin_a, dir.y * sin_a, -cos_a).normalize();

                vertices.push(Vertex::new(
                    pos3d,
                    norm3d,
                    Vec4::new(0.0, 1.0, 0.0, 1.0),
                    calc_uv(pos3d),
                    Vec2::new(i as f32 / num_perim as f32, 1.0 - t),
                ));
            }

            for i in 0..num_perim {
                let next_i = (i + 1) % num_perim;
                let p0 = prev_ring_start + i as u32;
                let p1 = prev_ring_start + next_i as u32;
                let c0 = curr_ring_start + i as u32;
                let c1 = curr_ring_start + next_i as u32;

                indices.push(p0);
                indices.push(c0);
                indices.push(p1);

                indices.push(p1);
                indices.push(c0);
                indices.push(c1);
            }

            prev_ring_start = curr_ring_start;
        }

        let back_ring_start = prev_ring_start;

        // -------------------------------------------------------------
        // E. Face Arrière Plat (Z = -half_thick)
        // -------------------------------------------------------------
        let center_back_idx = vertices.len() as u32;
        vertices.push(Vertex::new(
            Vec3::new(0.0, 0.0, -half_thick),
            -Vec3::Z,
            Vec4::new(-1.0, 0.0, 0.0, 1.0),
            Vec2::new(0.5, 0.5),
            Vec2::ZERO,
        ));

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            indices.push(center_back_idx);
            indices.push(back_ring_start + next_i as u32);
            indices.push(back_ring_start + i as u32);
        }

        (vertices, indices)
    }
}

