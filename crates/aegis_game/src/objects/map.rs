use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct MapObject {
    pub mesh: GpuMesh,
    pub position: Vec3,
    pub scale: Vec3,
}

impl MapObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_raw_bytes(include_bytes!("../../../../assets/modeles/map.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Map 3D (map.glb) initialisée : {} sommets, {} indices.", v.len(), i.len());

        Ok(Self {
            mesh,
            position: Vec3::new(24.0, 1.5, -5.0),
            scale: Vec3::new(0.3, 0.3, 0.3),
        })
    }

    pub fn get_model_matrix(&self) -> Mat4 {
        // Rotation de 90 degrés sur l'axe Z pour orienter la carte horizontalement !
        Mat4::from_translation(self.position)
            * Mat4::from_rotation_z(90.0f32.to_radians())
            * Mat4::from_rotation_x(-90.0f32.to_radians())
            * Mat4::from_scale(self.scale)
    }

    pub fn check_collision(&self, pos: Vec2, size: Vec2) -> bool {
        let half_w = size.x / 2.0;
        let left = pos.x - half_w;
        let right = pos.x + half_w;
        let bottom = pos.y;
        let top = pos.y + size.y;

        // Bords et frontières physiques de la carte 3D (sol, plafond, mur gauche, mur droit)
        if bottom <= 1.5 {
            return true; // Sol de la carte 3D
        }
        if top >= 24.0 {
            return true; // Plafond
        }
        if left <= 0.5 {
            return true; // Mur Gauche
        }
        if right >= 47.5 {
            return true; // Mur Droit
        }

        false
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, pipeline_layout: vk::PipelineLayout, vp: Mat4) {
        let model = self.get_model_matrix();

        let push = PushConstants {
            mvp_matrix: vp * model,
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.88, 0.92, 1.0), // Gris studio clair
            params: Vec4::new(0.2, 1.0, 0.0, 0.0),
        };

        unsafe {
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
        }
        self.mesh.draw(device, cmd);
    }
}
