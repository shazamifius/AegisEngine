use ash::vk;
use aegis_engine::math::{Mat4, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};

pub struct PlantsObject {
    pub mesh: GpuMesh,
}

impl PlantsObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/plantedecendente.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Plantes Suspendues 3D (plantedecendente.glb) initialisées.");

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
            * Mat4::from_scale(Vec3::new(2.5, 4.0, 2.5));

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.15, 0.65, 0.25, 1.0), // Vert Feuillage
            params: Vec4::new(0.1, 1.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);
    }
}
