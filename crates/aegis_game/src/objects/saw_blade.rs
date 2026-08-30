use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct SawBladeObject {
    pub mesh: GpuMesh,
}

impl SawBladeObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/saw_blade.glb"))?;

        // ── LA SCIE ENTRE DANS LA TRAME DU MONDE ────────────────────────────────────────────
        //
        // Elle arrivait d'un modeleur : un disque rond, poli, avec des dents lisses. Posée dans un
        // décor entièrement fait de cubes alignés, elle n'y cohabitait pas — elle y était déposée.
        // *C'est le genre d'écart que l'œil voit tout de suite sans savoir le nommer.*
        //
        // ⚠ Le côté du voxel n'est PAS une résolution choisie : c'est celui de la trame du décor,
        // `1 / SOUS_VOXELS` de bloc. La scie tombe donc exactement sur la même grille que les
        // détails de la carte, et un objet plus grand aura simplement plus de voxels — pas des
        // voxels plus gros. La question « en combien de subdivisions ? » ne se pose pas.
        //
        // Le maillage normalisé mesure 1,5 unité dans sa plus grande dimension (`glb_loader`), et
        // la scie s'affiche à l'échelle 1 : une unité de maillage EST un bloc du monde.
        let (v, i) = match crate::party_render_pass::voxeliser_pour_le_monde(&v, &i) {
            Some(voxelise) => voxelise,
            // ⚠ Une voxelisation vide veut dire que quelque chose ne va pas dans le maillage. On
            // garde alors la forme lisse plutôt que de faire disparaître la scie : un piège
            // mortel invisible est bien pire qu'un piège au mauvais style.
            None => {
                log::warn!("Scie : voxelisation vide, la forme lisse est conservee");
                (v, i)
            }
        };

        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!(
            "Scie Rotative voxelisee : {} sommets, {} triangles",
            v.len(),
            i.len() / 3
        );

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        pose: &aegis_engine::render::instances::Pose,
        vp: Mat4,
        pos: Vec2,
        rotation: f32,
    ) {
        self.draw_at_3d(pose, vp, Vec3::new(pos.x, pos.y, 0.1), 1.0, rotation);
    }

    pub fn draw_at_3d(
        &self,
        pose: &aegis_engine::render::instances::Pose,
        vp: Mat4,
        pos: Vec3,
        scale: f32,
        rotation: f32,
    ) {
        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_z(rotation)
            * Mat4::from_rotation_x(90.0f32.to_radians())
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PushConstants {
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.88, 0.92, 1.0), // Acier Métallique
            params: Vec4::new(0.1, 2.0, 0.0, 0.0),
        };

        pose.objet(&self.mesh, &push);
    }
}
