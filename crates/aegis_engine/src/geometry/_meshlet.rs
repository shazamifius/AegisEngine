use crate::core::math::Vec3;

/// Structure représentant un Meshlet (cluster géométrique de 64 sommets / 126 triangles maximum).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Meshlet {
    pub vertex_offset: u32,
    pub triangle_offset: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
}

/// Enveloppes englobantes pour le Culling Hiérarchique GPU (Frustum, Backface & Hi-Z).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeshletBounds {
    pub center_radius: [f32; 4], // x, y, z = Center, w = Radius
    pub aabb_min: [f32; 4],       // x, y, z = Min, w = Unused
    pub aabb_max: [f32; 4],       // x, y, z = Max, w = Unused
    pub cone_axis_cutoff: [f32; 4], // x, y, z = Cone Axis, w = Cone Cutoff (cosinus angle)
}

impl MeshletBounds {
    /// Calcule les enveloppes englobantes pour un groupe de positions 3D.
    pub fn from_positions(positions: &[Vec3], normals: &[Vec3]) -> Self {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);

        for &pos in positions {
            min.x = min.x.min(pos.x);
            min.y = min.y.min(pos.y);
            min.z = min.z.min(pos.z);

            max.x = max.x.max(pos.x);
            max.y = max.y.max(pos.y);
            max.z = max.z.max(pos.z);
        }

        let center = (min + max) * 0.5;
        let mut radius: f32 = 0.0;
        for &pos in positions {
            radius = radius.max((pos - center).length());
        }

        // Cône de normales moyen pour le Backface Culling
        let mut avg_normal = Vec3::ZERO;
        for &norm in normals {
            avg_normal += norm;
        }
        let cone_axis = avg_normal.normalize();

        let mut cone_cutoff: f32 = 1.0;
        for &norm in normals {
            let dot = norm.normalize().dot(cone_axis);
            cone_cutoff = cone_cutoff.min(dot);
        }

        Self {
            center_radius: [center.x, center.y, center.z, radius],
            aabb_min: [min.x, min.y, min.z, 0.0],
            aabb_max: [max.x, max.y, max.z, 0.0],
            cone_axis_cutoff: [cone_axis.x, cone_axis.y, cone_axis.z, cone_cutoff],
        }
    }
}
