use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};
use crate::traps::Direction;

pub struct LaserEmitterObject {
    pub mesh: GpuMesh,
}

impl LaserEmitterObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb("/home/shaza/Documents/asset/laser_emitter.glb")?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Émetteur Laser 3D (laser_emitter.glb) initialisé.");

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        cube_mesh: &GpuMesh,
        vp: Mat4,
        pos: Vec2,
        dir: Direction,
        active: bool,
    ) {
        // En position verticale d'origine Blender, +Y est orienté vers le haut!
        let rot_z = match dir {
            Direction::Up => 0.0,
            Direction::Right => -90.0f32.to_radians(),
            Direction::Left => 90.0f32.to_radians(),
            Direction::Down => 180.0f32.to_radians(),
        };

        let model = Mat4::from_translation(Vec3::new(pos.x, pos.y, 0.1))
            * Mat4::from_rotation_z(rot_z)
            * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0));

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.25, 0.75, 0.95, 1.0),
            params: Vec4::new(0.1, 2.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);

        // Faisceau Laser Cyan Vertical Éclatant (projeté vers le haut!)
        if active {
            let (beam_center, beam_scale) = match dir {
                Direction::Up => (pos + Vec2::new(0.0, 5.0), Vec3::new(0.18, 10.0, 0.18)),
                Direction::Down => (pos - Vec2::new(0.0, 5.0), Vec3::new(0.18, 10.0, 0.18)),
                Direction::Right => (pos + Vec2::new(5.0, 0.0), Vec3::new(10.0, 0.18, 0.18)),
                Direction::Left => (pos - Vec2::new(5.0, 0.0), Vec3::new(10.0, 0.18, 0.18)),
            };

            let beam_model = Mat4::from_translation(Vec3::new(beam_center.x, beam_center.y, 0.2))
                * Mat4::from_scale(beam_scale);

            let push_beam = PartyPushConstants {
                mvp_matrix: vp * beam_model,
                model_matrix: beam_model,
                color_tint: Vec4::new(0.0, 0.95, 1.0, 1.0), // Laser Néon Cyan
                params: Vec4::new(0.0, 10.0, 0.0, 0.0),     // Émission maximale !
            };

            unsafe {
                device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_beam));
            }
            cube_mesh.draw(device, cmd);
        }
    }
}
