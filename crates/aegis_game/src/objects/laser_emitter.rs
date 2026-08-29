use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;
use crate::traps::Direction;

pub struct LaserEmitterObject {
    pub mesh: GpuMesh,
}

impl LaserEmitterObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/laser_emitter.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Émetteur Laser 3D (laser_emitter.glb) initialisé.");

        Ok(Self { mesh })
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, pipeline_layout: vk::PipelineLayout, cube_mesh: &GpuMesh, vp: Mat4, pos: Vec2, dir: Direction, active: bool, beam_length: f32, time: f32) {
        self.draw_at_3d(device, cmd, pipeline_layout, cube_mesh, vp, Vec3::new(pos.x, pos.y, 0.1), dir, 1.0, active, beam_length, time);
    }

    pub fn draw_at_3d(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        cube_mesh: &GpuMesh,
        vp: Mat4,
        pos: Vec3,
        dir: Direction,
        scale: f32,
        active: bool,
        beam_length: f32,
        time: f32,
    ) {
        // En position verticale d'origine Blender, +Y est orienté vers le haut!
        let rot_z = match dir {
            Direction::Up => 0.0,
            Direction::Right => -90.0f32.to_radians(),
            Direction::Left => 90.0f32.to_radians(),
            Direction::Down => 180.0f32.to_radians(),
        };

        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_z(rot_z)
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.25, 0.75, 0.95, 1.0),
            params: Vec4::new(0.1, 2.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);

        // Faisceau Laser Cyan Continu Raycasté & Particules en Spirale
        if active {
            let half_len = beam_length * 0.5;
            let (beam_center, beam_scale) = match dir {
                Direction::Up => (pos + Vec3::new(0.0, half_len, 0.0), Vec3::new(0.18, beam_length, 0.18)),
                Direction::Down => (pos - Vec3::new(0.0, half_len, 0.0), Vec3::new(0.18, beam_length, 0.18)),
                Direction::Right => (pos + Vec3::new(half_len, 0.0, 0.0), Vec3::new(beam_length, 0.18, 0.18)),
                Direction::Left => (pos - Vec3::new(half_len, 0.0, 0.0), Vec3::new(beam_length, 0.18, 0.18)),
            };

            let beam_model = Mat4::from_translation(beam_center + Vec3::new(0.0, 0.0, 0.1))
                * Mat4::from_scale(beam_scale);

            let push_beam = PushConstants {
                mvp_matrix: vp * beam_model,
                model_matrix: beam_model,
                color_tint: Vec4::new(0.0, 0.95, 1.0, 1.0), // Laser Néon Cyan
                params: Vec4::new(0.0, 10.0, 0.0, 0.0),     // Émission maximale !
            };

            unsafe {
                device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_beam));
            }
            cube_mesh.draw(device, cmd);

            // Essaim d'Énergie Dense & Organique (9x plus de particules en vortex cyan/blanc)
            let particle_count = (beam_length * 18.0).clamp(60.0, 220.0) as usize;
            for i in 0..particle_count {
                let fi = i as f32;
                // Emplacement le long du faisceau avec micro-bruit organique
                let frac = (fi / particle_count as f32 + (fi * 12.9898).sin() * 0.015).clamp(0.0, 1.0);
                let dist_along = frac * beam_length;

                // Variations pseudo-aléatoires déterministes par particule
                let speed_mult = 3.5 + (i % 5) as f32 * 1.25;
                let phase_offset = fi * 0.85 + (i % 3) as f32 * 2.094;
                let angle = time * speed_mult + phase_offset;

                // Rayon et taille organiques variés
                let radius = 0.12 + ((i * 13) % 9) as f32 * 0.035;
                let p_scale = 0.05 + ((i * 17) % 7) as f32 * 0.018;

                let (offset_x, offset_y, offset_z) = match dir {
                    Direction::Up => (angle.cos() * radius, dist_along, angle.sin() * radius),
                    Direction::Down => (angle.cos() * radius, -dist_along, angle.sin() * radius),
                    Direction::Right => (dist_along, angle.cos() * radius, angle.sin() * radius),
                    Direction::Left => (-dist_along, angle.cos() * radius, angle.sin() * radius),
                };

                let p_pos = pos + Vec3::new(offset_x, offset_y, offset_z + 0.15);
                let p_model = Mat4::from_translation(p_pos) * Mat4::from_scale(Vec3::splat(p_scale));

                // Couleur Cyan Électrique Étincelante identique au Faisceau Laser
                let p_color = if i % 3 == 0 {
                    Vec4::new(0.00, 1.00, 0.92, 1.0) // Cyan Turquoise Néon Étincelant
                } else if i % 2 == 0 {
                    Vec4::new(0.00, 0.95, 1.00, 1.0) // Bleu Cyan Électrique Laser
                } else {
                    Vec4::new(0.10, 0.85, 1.00, 1.0) // Cyan Bleu Brillant
                };

                let push_particle = PushConstants {
                    mvp_matrix: vp * p_model,
                    model_matrix: p_model,
                    color_tint: p_color,
                    params: Vec4::new(0.0, 14.0, 0.0, 0.0), // Émission maximale !
                };

                unsafe {
                    device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_particle));
                }
                cube_mesh.draw(device, cmd);
            }
        }
    }
}
