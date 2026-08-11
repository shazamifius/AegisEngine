use ash::vk;
use aegis_engine::math::{Mat4, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};

pub struct CardboardBoxObject {
    pub mesh_closed: GpuMesh,
    pub mesh_open: GpuMesh,
    pub offset_closed: Vec3,
    pub offset_open: Vec3,
    pub scale_closed: Vec3,
    pub scale_open: Vec3,
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

        let compute_bounds = |verts: &[aegis_engine::geometry::vertex::Vertex]| -> (Vec3, Vec3, Vec3) {
            let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
            let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
            for v in verts {
                min.x = min.x.min(v.position[0]);
                min.y = min.y.min(v.position[1]);
                min.z = min.z.min(v.position[2]);
                max.x = max.x.max(v.position[0]);
                max.y = max.y.max(v.position[1]);
                max.z = max.z.max(v.position[2]);
            }
            let center = (min + max) * 0.5;
            (min, max, center)
        };

        let (_min_c, _max_c, center_c) = compute_bounds(&vc);
        let (min_o, max_o, _center_o) = compute_bounds(&vo);

        // Center base cube of both boxes at origin (0,0,0)
        let offset_closed = -center_c;
        let offset_open = -Vec3::new(0.0, center_c.y, (min_o.z + max_o.z) * 0.5);

        // Échelle fermée (9.0) vs Échelle ouverte parfaitement recadrée (11.5)
        let scale_closed = Vec3::splat(9.0);
        let scale_open = Vec3::splat(11.5);

        log::info!("Carton Mystère initialisé - Scale fermé: {:?}, Scale ouvert: {:?}", scale_closed, scale_open);

        Ok(Self {
            mesh_closed,
            mesh_open,
            offset_closed,
            offset_open,
            scale_closed,
            scale_open,
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
        // Animation de secousse pendant 2.0 secondes exactes, puis la boîte s'ouvre !
        if self.anim_timer >= 2.0 {
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
            // Secousse dynamique pendant 2 secondes (Shake phase)
            let intensity = (2.0 - self.anim_timer).max(0.0) / 2.0;
            let sx = (self.anim_timer * 42.0).sin() * 0.12 * intensity;
            let sy = (self.anim_timer * 55.0).cos().abs() * 0.14 * intensity;
            let rz = (self.anim_timer * 35.0).sin() * 0.16 * intensity;
            (sx, sy, rz)
        } else {
            (0.0, 0.0, 0.0)
        };

        // Position du carton au centre parfait du champ de vision (Y offset -0.5, Z +12.0)
        let box_depth_pos = Vec3::new(pos.x + shake_offset_x, pos.y + shake_offset_y - 0.5, pos.z + 12.0);

        let (mesh_to_draw, offset, scale) = if !self.is_opened {
            (&self.mesh_closed, self.offset_closed, self.scale_closed)
        } else {
            (&self.mesh_open, self.offset_open, self.scale_open)
        };

        // Rotation X (90°) puis Z (90°) pour orienter l'ouverture vers la caméra et les rabats vers GAUCHE/DROITE !
        let rot_x = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let rot_z = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);

        let model = Mat4::from_translation(box_depth_pos)
            * rot_z
            * rot_x
            * Mat4::from_rotation_z(shake_rot_z)
            * Mat4::from_scale(scale)
            * Mat4::from_translation(offset);

        // Couleur Kraft Carton Warm Naturelle
        let push = PartyPushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.78, 0.58, 0.36, 1.0), // Kraft Carton Chaud
            params: Vec4::new(0.3, 0.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }

        mesh_to_draw.draw(device, cmd);
    }
}
