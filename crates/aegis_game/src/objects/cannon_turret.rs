use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};
use crate::traps::Direction;

pub struct CannonTurretObject {
    pub mesh: GpuMesh,
}

impl CannonTurretObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb("/home/shaza/Documents/asset/cannon_turret.glb")?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Tourelle Canon 3D (cannon_turret.glb) initialisée.");

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec2,
        dir: Direction,
    ) {
        let rot_z = match dir {
            Direction::Right => 0.0,
            Direction::Up => 90.0f32.to_radians(),
            Direction::Left => 180.0f32.to_radians(),
            Direction::Down => 270.0f32.to_radians(),
        };

        let model = Mat4::from_translation(Vec3::new(pos.x, pos.y, 0.1))
            * Mat4::from_rotation_z(rot_z);

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.3, 0.35, 0.4, 1.0), // Fonte Sombre
            params: Vec4::new(0.1, 1.5, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);
    }
}
