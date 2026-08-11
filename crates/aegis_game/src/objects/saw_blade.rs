use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};

pub struct SawBladeObject {
    pub mesh: GpuMesh,
}

impl SawBladeObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb("/home/shaza/Documents/asset/saw_blade.glb")?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Scie Rotative 3D (saw_blade.glb) initialisée.");

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec2,
        rotation: f32,
    ) {
        self.draw_at_3d(device, cmd, pipeline_layout, vp, Vec3::new(pos.x, pos.y, 0.1), 1.0, rotation);
    }

    pub fn draw_at_3d(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec3,
        scale: f32,
        rotation: f32,
    ) {
        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_z(rotation)
            * Mat4::from_rotation_x(90.0f32.to_radians())
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.88, 0.92, 1.0), // Acier Métallique
            params: Vec4::new(0.1, 2.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);
    }
}
