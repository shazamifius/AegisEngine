use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;
use crate::traps::Direction;

pub struct FlamethrowerObject {
    pub mesh: GpuMesh,
}

impl FlamethrowerObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/flamethrower.glb"))?;
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!("Lance-flammes 3D (flamethrower.glb) initialisé.");

        Ok(Self { mesh })
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer, pipeline_layout: vk::PipelineLayout,
        instances: &aegis_engine::render::instances::Instances, cube_mesh: &GpuMesh, vp: Mat4, pos: Vec2, dir: Direction, active: bool, time: f32) {
        self.draw_at_3d(device, cmd, pipeline_layout, instances, cube_mesh, vp, Vec3::new(pos.x, pos.y, 0.1), dir, 1.0, active, time);
    }

    pub fn draw_at_3d(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        instances: &aegis_engine::render::instances::Instances,
        cube_mesh: &GpuMesh,
        vp: Mat4,
        pos: Vec3,
        dir: Direction,
        scale: f32,
        active: bool,
        time: f32,
    ) {
        // Le modèle 3D flamethrower.glb est naturellement orienté verticalement le long de Y dans Blender (buse vers +Y).
        // Une simple rotation autour de Z positionne le canon à plat sur la carte dans les 4 directions !
        let rot_z = match dir {
            Direction::Up    => 0.0,
            Direction::Right => -std::f32::consts::FRAC_PI_2,
            Direction::Down  => std::f32::consts::PI,
            Direction::Left  => std::f32::consts::FRAC_PI_2,
        };

        let rot_matrix = Mat4::from_rotation_z(rot_z);

        let model = Mat4::from_translation(pos)
            * rot_matrix
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PushConstants {
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.45, 0.1, 1.0),
            params: Vec4::new(0.1, 1.5, 0.0, 0.0),
        };

        instances.dessiner_avec(device, cmd, &self.mesh, &push);

        // Particules Carrées Voxel Rétro-Style (Strictement max 5 blocs long, 1 bloc haut)
        if active {
            let nozzle_offset = match dir {
                Direction::Up    => Vec3::new(0.0, 0.75, 0.0),
                Direction::Down  => Vec3::new(0.0, -0.75, 0.0),
                Direction::Right => Vec3::new(0.75, 0.0, 0.0),
                Direction::Left  => Vec3::new(-0.75, 0.0, 0.0),
            };

            let nozzle_pos = pos + nozzle_offset;
            let max_flame_length = 5.0f32; // Stricte limite 5.0 blocs !
            let particle_count = 42;       // 42 particules carrées nettes et lisibles

            for i in 0..particle_count {
                let fi = i as f32;

                // Écoulement continu fluide du tir de feu
                let cycle = ((time * 3.5 + fi * 0.127) % 1.0).abs();
                let dist_along = cycle * max_flame_length; // 0.0 -> 5.0 max !

                // Élévation thermique & oscillation latérale
                let float_up = (dist_along / max_flame_length) * 0.18;
                let wobble = (time * 6.5 + fi * 0.9).sin() * 0.10;

                let (offset_x, offset_y, offset_z) = match dir {
                    Direction::Right => (dist_along, wobble + float_up, 0.0),
                    Direction::Left  => (-dist_along, wobble + float_up, 0.0),
                    Direction::Up    => (wobble, dist_along, 0.0),
                    Direction::Down  => (wobble, -dist_along, 0.0),
                };

                let p_pos = nozzle_pos + Vec3::new(offset_x, offset_y, offset_z + 0.15);

                // Taille des carrés de feu voxel
                let p_scale = if dist_along < 0.7 {
                    0.07 + dist_along * 0.10
                } else if dist_along < 3.0 {
                    0.15 - (dist_along - 0.7) * 0.02
                } else {
                    (0.10 - (dist_along - 3.0) * 0.03).max(0.03)
                };

                let p_model = Mat4::from_translation(p_pos) * Mat4::from_scale(Vec3::splat(p_scale));

                // Couleur rétro-voxel : Jaune Solaire -> Orange Feu -> Rouge Braise
                let p_color = if dist_along < 0.9 {
                    Vec4::new(1.00, 0.95, 0.35, 1.0) // Jaune Or Solaire
                } else if dist_along < 3.0 {
                    Vec4::new(1.00, 0.50, 0.05, 1.0) // Orange Feu Net
                } else {
                    Vec4::new(0.88, 0.12, 0.04, 1.0) // Rouge Braise
                };

                let push_particle = PushConstants {
                    model_matrix: p_model,
                    color_tint: p_color,
                    params: Vec4::new(0.0, 10.0, 0.0, 0.0),
                };

                instances.dessiner_avec(device, cmd, &cube_mesh, &push_particle);
            }
        }
    }
}
