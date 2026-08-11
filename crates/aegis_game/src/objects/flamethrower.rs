use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};
use crate::traps::Direction;

pub struct FlamethrowerObject {
    pub mesh: GpuMesh,
}

impl FlamethrowerObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb("/home/shaza/Documents/asset/flamethrower.glb")?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Lance-flammes 3D (flamethrower.glb) initialisé.");

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
        // En position verticale d'origine Blender, la buse pointe vers le haut.
        // Une rotation de -90° autour de Z et 90° autour de X la couche horizontalement pointant vers la DROITE!
        let rot_z = match dir {
            Direction::Right => -90.0f32.to_radians(),
            Direction::Up => 0.0,
            Direction::Left => 90.0f32.to_radians(),
            Direction::Down => 180.0f32.to_radians(),
        };

        let model = Mat4::from_translation(Vec3::new(pos.x, pos.y, 0.1))
            * Mat4::from_rotation_z(rot_z)
            * Mat4::from_rotation_x(90.0f32.to_radians())
            * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0));

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.45, 0.1, 1.0),
            params: Vec4::new(0.1, 1.5, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);

        // Jet de flammes horizontal incandescent (projeté vers la droite!)
        if active {
            let (flame_center, flame_scale) = match dir {
                Direction::Right => (pos + Vec2::new(2.5, 0.0), Vec3::new(5.0, 0.7, 0.7)),
                Direction::Left => (pos - Vec2::new(2.5, 0.0), Vec3::new(5.0, 0.7, 0.7)),
                Direction::Up => (pos + Vec2::new(0.0, 2.5), Vec3::new(0.7, 5.0, 0.7)),
                Direction::Down => (pos - Vec2::new(0.0, 2.5), Vec3::new(0.7, 5.0, 0.7)),
            };

            let flame_model = Mat4::from_translation(Vec3::new(flame_center.x, flame_center.y, 0.2))
                * Mat4::from_scale(flame_scale);

            let push_flame = PartyPushConstants {
                mvp_matrix: vp * flame_model,
                model_matrix: flame_model,
                color_tint: Vec4::new(1.0, 0.45, 0.05, 0.95), // Orange Feu Vif
                params: Vec4::new(0.0, 6.0, 0.0, 0.0),         // Effet de lueur émissive
            };

            unsafe {
                device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_flame));
            }
            cube_mesh.draw(device, cmd);
        }
    }
}
