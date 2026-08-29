use ash::vk;
use aegis_engine::math::{Mat4, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct RockObject {
    pub mesh: GpuMesh,
}

impl RockObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/rockbasdroit.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Rocher Bas Droit 3D (rockbasdroit.glb) initialisé.");

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec3,
    ) {
        let model = Mat4::from_translation(pos)
            * Mat4::from_scale(Vec3::new(4.0, 4.0, 4.0));

        let push = PushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.45, 0.42, 0.40, 1.0), // Roche Naturelle
            params: Vec4::new(0.3, 1.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);
    }
}
