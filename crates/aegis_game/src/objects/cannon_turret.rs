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
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/cannon_turret.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Tourelle Canon 3D (cannon_turret.glb) initialisée.");

        Ok(Self { mesh })
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, pipeline_layout: vk::PipelineLayout, cube_mesh: Option<&GpuMesh>, vp: Mat4, pos: Vec2, dir: Direction, is_placement: bool) {
        self.draw_at_3d(device, cmd, pipeline_layout, cube_mesh, vp, Vec3::new(pos.x, pos.y, 0.1), dir, 1.0, is_placement);
    }

    pub fn draw_at_3d(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        cube_mesh: Option<&GpuMesh>,
        vp: Mat4,
        pos: Vec3,
        dir: Direction,
        scale: f32,
        is_placement: bool,
    ) {
        // Modèle de la tourelle Portal debout sur ses 3 pieds
        let rot_z = match dir {
            Direction::Right => 0.0,
            Direction::Up    => 90.0f32.to_radians(),
            Direction::Left  => 180.0f32.to_radians(),
            Direction::Down  => 270.0f32.to_radians(),
        };

        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_z(rot_z)
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.92, 0.92, 0.95, 1.0), // Blanc Coque Portal
            params: Vec4::new(0.1, 1.5, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);

        // Viseur Laser Rouge sortant directement de l'Œil Central de la tourelle Portal !
        if let Some(cube) = cube_mesh {
            let line_len = if is_placement { 3.5 } else { 1.0 };
            let (eye_offset, line_m) = match dir {
                Direction::Right => (Vec3::new(0.35, 0.0, 0.15), Mat4::from_translation(pos + Vec3::new(0.35 + line_len * 0.5, 0.0, 0.15)) * Mat4::from_scale(Vec3::new(line_len, 0.05, 0.05))),
                Direction::Left  => (Vec3::new(-0.35, 0.0, 0.15), Mat4::from_translation(pos - Vec3::new(0.35 + line_len * 0.5, 0.0, 0.15)) * Mat4::from_scale(Vec3::new(line_len, 0.05, 0.05))),
                Direction::Up    => (Vec3::new(0.0, 0.35, 0.15), Mat4::from_translation(pos + Vec3::new(0.0, 0.35 + line_len * 0.5, 0.15)) * Mat4::from_scale(Vec3::new(0.05, line_len, 0.05))),
                Direction::Down  => (Vec3::new(0.0, -0.35, 0.15), Mat4::from_translation(pos - Vec3::new(0.0, 0.35 + line_len * 0.5, 0.15)) * Mat4::from_scale(Vec3::new(0.05, line_len, 0.05))),
            };

            // 1. Œil Rouge Lumineux au centre de la tourelle
            let eye_m = Mat4::from_translation(pos + eye_offset) * Mat4::from_scale(Vec3::splat(0.12));
            let push_eye = PartyPushConstants {
                mvp_matrix: vp * eye_m,
                model_matrix: eye_m,
                color_tint: Vec4::new(1.0, 0.0, 0.05, 1.0), // Œil Rouge Portal Émissif
                params: Vec4::new(0.0, 16.0, 0.0, 0.0),
            };

            unsafe {
                device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_eye));
            }
            cube.draw(device, cmd);

            // 2. Rayon Viseur Laser Rouge sortant de l'œil
            let sight_color = if is_placement {
                Vec4::new(1.0, 0.15, 0.1, 1.0) // Ligne Laser Écarlate (Placement)
            } else {
                Vec4::new(1.0, 0.35, 0.1, 0.5) // Discret en jeu
            };

            let push_sight = PartyPushConstants {
                mvp_matrix: vp * line_m,
                model_matrix: line_m,
                color_tint: sight_color,
                params: Vec4::new(0.0, 12.0, 0.0, 0.0),
            };

            unsafe {
                device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_sight));
            }
            cube.draw(device, cmd);
        }
    }
}
