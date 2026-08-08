use glam::{Vec3, Vec4};

/// Structure représentant un Meshlet (cluster géométrique de 64 sommets / 126 triangles maximum).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Meshlet {
    pub vertex_offset: u32,
    pub triangle_offset: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
}

/// Enveloppes englobantes pour le Culling Hiérarchique GPU (Frustum, Backface & Hi-Z).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshletBounds {
    pub center_radius: [f32; 4], // x, y, z = Center, w = Radius
    pub aabb_min: [f32; 4],       // x, y, z = Min, w = Unused
    pub aabb_max: [f32; 4],       // x, y, z = Max, w = Unused
    pub cone_axis_cutoff: [f32; 4], // x, y, z = Cone Axis, w = Cone Cutoff (cosinus angle)
}

impl MeshletBounds {
    /// Calcule les enveloppes englobantes pour un groupe de positions 3D.
    pub fn from_positions(positions: &[Vec3], normals: &[Vec3]) -> Self {
        if positions.is_empty() {
            return Self {
                center_radius: [0.0; 4],
                aabb_min: [0.0; 4],
                aabb_max: [0.0; 4],
                cone_axis_cutoff: [0.0, 1.0, 0.0, -1.0],
            };
        }

        let mut min = positions[0];
        let mut max = positions[0];

        for &p in positions.iter() {
            min = min.min(p);
            max = max.max(p);
        }

        let center = (min + max) * 0.5;
        let mut radius: f32 = 0.0;
        for &p in positions.iter() {
            radius = radius.max(center.distance(p));
        }

        // Calcul du cône de normales moyen pour le Backface Culling
        let mut avg_normal = Vec3::ZERO;
        for &n in normals.iter() {
            avg_normal += n;
        }
        let cone_axis = if avg_normal.length_squared() > 1e-6 {
            avg_normal.normalize()
        } else {
            Vec3::Y
        };

        let mut max_dot: f32 = 1.0;
        for &n in normals.iter() {
            if n.length_squared() > 1e-6 {
                let dot = cone_axis.dot(n.normalize());
                max_dot = max_dot.min(dot);
            }
        }

        Self {
            center_radius: [center.x, center.y, center.z, radius],
            aabb_min: [min.x, min.y, min.z, 0.0],
            aabb_max: [max.x, max.y, max.z, 0.0],
            cone_axis_cutoff: [cone_axis.x, cone_axis.y, cone_axis.z, max_dot],
        }
    }

    /// Teste si la sphère englobante percute un plan de Frustum.
    pub fn is_visible_in_frustum_plane(&self, plane: Vec4) -> bool {
        let center = Vec3::from_slice(&self.center_radius[0..3]);
        let radius = self.center_radius[3];
        let dist = plane.truncate().dot(center) + plane.w;
        dist >= -radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meshlet_bounds_computation() {
        let positions = vec![
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(0.0, 0.0, 0.0),
        ];
        let normals = vec![
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];

        let bounds = MeshletBounds::from_positions(&positions, &normals);

        assert_eq!(bounds.aabb_min[0..3], [-1.0, -1.0, -1.0]);
        assert_eq!(bounds.aabb_max[0..3], [1.0, 1.0, 1.0]);
        assert_eq!(bounds.center_radius[0..3], [0.0, 0.0, 0.0]);
        assert!((bounds.center_radius[3] - 1.73205).abs() < 1e-3);
    }

    #[test]
    fn test_meshlet_frustum_visibility() {
        let positions = vec![Vec3::new(0.0, 0.0, -5.0)];
        let normals = vec![Vec3::Z];
        let bounds = MeshletBounds::from_positions(&positions, &normals);

        // Plan pointant vers Z positif
        let plane_front = Vec4::new(0.0, 0.0, 1.0, 10.0);
        assert!(bounds.is_visible_in_frustum_plane(plane_front));

        // Plan trop loin derrière
        let plane_behind = Vec4::new(0.0, 0.0, 1.0, 2.0);
        assert!(!bounds.is_visible_in_frustum_plane(plane_behind));
    }
}
