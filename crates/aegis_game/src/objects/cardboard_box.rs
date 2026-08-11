use ash::vk;
use aegis_engine::math::{Mat4, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};

pub struct CardboardBoxObject {
    pub mesh_closed: GpuMesh,
    pub mesh_open: GpuMesh,
    pub anim_timer: f32,
    pub is_opened: bool,
    pub burst_triggered: bool,
}

impl CardboardBoxObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (vc, ic) = GlbLoader::load_glb("/home/shaza/Documents/asset/boxfermer.glb")?;
        let mesh_closed = GpuMesh::upload(gpu, memory_props, &vc, &ic)?;

        let (vo, io) = GlbLoader::load_glb("/home/shaza/Documents/asset/box.glb")?;
        let mesh_open = GpuMesh::upload(gpu, memory_props, &vo, &io)?;

        log::info!("Carton Mystère 3D (boxfermer.glb & box.glb) initialisé.");

        Ok(Self {
            mesh_closed,
            mesh_open,
            anim_timer: 0.0,
            is_opened: false,
            burst_triggered: false,
        })
    }

    pub fn reset_animation(&mut self) {
        self.anim_timer = 0.0;
        self.is_opened = false;
        self.burst_triggered = false;
    }

    pub fn update(&mut self, dt: f32) {
        self.anim_timer += dt;
        if self.anim_timer >= 0.8 {
            self.is_opened = true;
        }
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec3,
    ) {
        let (shake_offset_x, shake_offset_y, shake_rot_z) = if !self.is_opened {
            // Secousse dynamique pendant 0.8 seconde (Shake phase)
            let intensity = (0.8 - self.anim_timer).max(0.0) / 0.8;
            let sx = (self.anim_timer * 42.0).sin() * 0.10 * intensity;
            let sy = (self.anim_timer * 55.0).cos().abs() * 0.12 * intensity;
            let rz = (self.anim_timer * 35.0).sin() * 0.15 * intensity;
            (sx, sy, rz)
        } else {
            (0.0, 0.0, 0.0)
        };

        let model = Mat4::from_translation(pos + Vec3::new(shake_offset_x, shake_offset_y, 0.0))
            * Mat4::from_rotation_z(shake_rot_z)
            * Mat4::from_scale(Vec3::new(3.5, 3.5, 3.5));

        // Couleur Kraft Carton Warm Naturelle
        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.74, 0.54, 0.34, 1.0), // Kraft Carton
            params: Vec4::new(0.3, 0.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }

        if !self.is_opened {
            self.mesh_closed.draw(device, cmd);
        } else {
            self.mesh_open.draw(device, cmd);
        }
    }
}
