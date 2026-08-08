use glam::{Vec2, Vec3, Vec4};
use crate::geometry::vertex::Vertex;

/// Générateur de Géométrie 3D pour Dalles de Verre avec Tranches Sèches à 90° et Chanfreins Nets à 45°.
pub struct GlassSlabGenerator;

impl GlassSlabGenerator {
    /// Génère un maillage 3D en forme de capsule (stadium) avec des tranches sèches verticales à 90°
    /// et des chanfreins biseautés franches à 45° (séparation nette de la lumière).
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
        let b_rad = bevel_radius.min(half_thick * 0.45);

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
        // BAND 1: Face Avant Plane (Z = +half_thick, Normale = (0, 0, 1))
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
            let inner_pos2d = pos2d - norm2d * b_rad;
            let u = (pos2d.x / (length + 2.0 * radius)) + 0.5;
            let v = (pos2d.y / (2.0 * radius)) + 0.5;

            vertices.push(Vertex::new(
                Vec3::new(inner_pos2d.x, inner_pos2d.y, half_thick),
                Vec3::Z,
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
        // BAND 2: Face Arrière Plane (Z = -half_thick, Normale = (0, 0, -1))
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
            let inner_pos2d = pos2d - norm2d * b_rad;
            let u = (pos2d.x / (length + 2.0 * radius)) + 0.5;
            let v = (pos2d.y / (2.0 * radius)) + 0.5;

            vertices.push(Vertex::new(
                Vec3::new(inner_pos2d.x, inner_pos2d.y, -half_thick),
                -Vec3::Z,
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
        // BAND 3: Chanfrein Avant Nette à 45° (Normale de Chanfrein)
        // -------------------------------------------------------------
        let chamfer_front_inner = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let inner_pos2d = pos2d - norm2d * b_rad;
            let chamfer_norm = Vec3::new(norm2d.x, norm2d.y, 1.0).normalize();

            vertices.push(Vertex::new(
                Vec3::new(inner_pos2d.x, inner_pos2d.y, half_thick),
                chamfer_norm,
                Vec4::new(0.0, 1.0, 0.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 0.0),
                Vec2::ZERO,
            ));
        }

        let chamfer_front_outer = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let chamfer_norm = Vec3::new(norm2d.x, norm2d.y, 1.0).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, half_thick - b_rad),
                chamfer_norm,
                Vec4::new(0.0, 1.0, 0.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 1.0),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            let in_curr = chamfer_front_inner + i as u32;
            let in_next = chamfer_front_inner + next_i as u32;
            let out_curr = chamfer_front_outer + i as u32;
            let out_next = chamfer_front_outer + next_i as u32;

            indices.push(in_curr);
            indices.push(out_curr);
            indices.push(in_next);

            indices.push(in_next);
            indices.push(out_curr);
            indices.push(out_next);
        }

        // -------------------------------------------------------------
        // BAND 4: Tranche Verticale Sèche à 90° (Normale Pure Horizontale)
        // -------------------------------------------------------------
        let side_wall_top = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let side_norm = Vec3::new(norm2d.x, norm2d.y, 0.0);

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, half_thick - b_rad),
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
            let side_norm = Vec3::new(norm2d.x, norm2d.y, 0.0);

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, -half_thick + b_rad),
                side_norm,
                Vec4::new(0.0, 0.0, 1.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 1.0),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            let t_curr = side_wall_top + i as u32;
            let t_next = side_wall_top + next_i as u32;
            let b_curr = side_wall_bottom + i as u32;
            let b_next = side_wall_bottom + next_i as u32;

            indices.push(t_curr);
            indices.push(b_curr);
            indices.push(t_next);

            indices.push(t_next);
            indices.push(b_curr);
            indices.push(b_next);
        }

        // -------------------------------------------------------------
        // BAND 5: Chanfrein Arrière Nette à 45° (Normale de Chanfrein Arrière)
        // -------------------------------------------------------------
        let chamfer_back_outer = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let chamfer_norm = Vec3::new(norm2d.x, norm2d.y, -1.0).normalize();

            vertices.push(Vertex::new(
                Vec3::new(pos2d.x, pos2d.y, -half_thick + b_rad),
                chamfer_norm,
                Vec4::new(0.0, 1.0, 0.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 0.0),
                Vec2::ZERO,
            ));
        }

        let chamfer_back_inner = vertices.len() as u32;
        for i in 0..num_perim {
            let pos2d = perimeter_pts_2d[i];
            let norm2d = perimeter_normals_2d[i];
            let inner_pos2d = pos2d - norm2d * b_rad;
            let chamfer_norm = Vec3::new(norm2d.x, norm2d.y, -1.0).normalize();

            vertices.push(Vertex::new(
                Vec3::new(inner_pos2d.x, inner_pos2d.y, -half_thick),
                chamfer_norm,
                Vec4::new(0.0, 1.0, 0.0, 1.0),
                Vec2::new(i as f32 / num_perim as f32, 1.0),
                Vec2::ZERO,
            ));
        }

        for i in 0..num_perim {
            let next_i = (i + 1) % num_perim;
            let out_curr = chamfer_back_outer + i as u32;
            let out_next = chamfer_back_outer + next_i as u32;
            let in_curr = chamfer_back_inner + i as u32;
            let in_next = chamfer_back_inner + next_i as u32;

            indices.push(out_curr);
            indices.push(in_curr);
            indices.push(out_next);

            indices.push(out_next);
            indices.push(in_curr);
            indices.push(in_next);
        }

        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capsule_slab_generation() {
        let (vertices, indices) = GlassSlabGenerator::create_capsule_slab(1.8, 0.6, 0.15, 0.04, 16);
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0);
    }
}
