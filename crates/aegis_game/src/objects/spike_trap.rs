use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct SpikeTrapObject {
    pub mesh: GpuMesh,
}

impl SpikeTrapObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/spike_trap.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Dalle à Pics 3D (spike_trap.glb) initialisée.");

        Ok(Self { mesh })
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, pipeline_layout: vk::PipelineLayout,
        instances: &aegis_engine::render::instances::Instances, vp: Mat4, pos: Vec2) {
        self.draw_at_3d(device, cmd, pipeline_layout, instances, vp, Vec3::new(pos.x, pos.y, 0.1), 1.0);
    }

    pub fn draw_at_3d(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        instances: &aegis_engine::render::instances::Instances,
        vp: Mat4,
        pos: Vec3,
        scale: f32,
    ) {
        // Échelle adaptative appliquée
        let model = Mat4::from_translation(pos)
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PushConstants {
            model_matrix: model,
            color_tint: Vec4::new(0.9, 0.25, 0.25, 1.0), // Rouge Métal Acéré
            params: Vec4::new(0.2, 1.5, 0.0, 0.0),
        };

        instances.dessiner_avec(device, cmd, &self.mesh, &push);
    }
}
