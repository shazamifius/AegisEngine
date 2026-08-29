use ash::vk;
use aegis_engine::math::{Mat4, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::bytes::as_bytes;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct CardboardBoxObject {
    pub mesh_closed: GpuMesh,
    pub mesh_open: GpuMesh,
    pub offset_closed: Vec3,
    pub offset_open: Vec3,
    pub scale_closed: Vec3,
    pub scale_open: Vec3,
}

/// Le carton mystère du début de manche.
///
/// **Cet objet n'a aucun état.** C'est délibéré, et c'est la réparation d'un vrai défaut : il
/// retenait auparavant son propre `anim_timer`, un `is_opened` et un `burst_triggered`, qu'une
/// méthode `reset_animation()` savait remettre à zéro — sauf que **personne ne l'appelait
/// jamais**. Le carton s'ouvrait donc une fois au lancement de la partie et restait ouvert pour
/// le reste de la soirée : l'animation ne rejouait à aucune manche suivante.
///
/// Rien ne pouvait le signaler : la méthode étant publique, le compilateur n'avait même pas de
/// « jamais utilisé » à donner.
///
/// La correction n'ajoute pas l'appel manquant, elle **supprime ce qu'il fallait remettre à
/// zéro** : l'avancement de l'animation se lit maintenant sur le minuteur de la phase de choix,
/// qui repart de lui-même à chaque manche. Un état qui n'existe plus ne peut plus rester en
/// arrière. En prime, l'animation ne dépend plus du nombre d'images par seconde — elle avançait
/// d'un `dt` figé à 16 ms, donc deux fois trop vite sur un écran à 120 Hz.
impl CardboardBoxObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (vc, ic) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/boxfermer.glb"))?;
        let mesh_closed = GpuMesh::upload(gpu, memory_props, &vc, &ic)?;

        let (vo, io) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/box.glb"))?;
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
        })
    }

    /// Combien de temps le carton se secoue avant de s'ouvrir.
    pub const DUREE_SECOUSSE: f32 = 2.0;

    /// Où le carton flotte, pour une carte de cette taille : au centre, et douze unités vers la
    /// caméra pour qu'il remplisse le champ.
    ///
    /// Le rendu et la gerbe de particules de l'ouverture doivent viser **le même point** : le
    /// décalage vivait auparavant à l'intérieur du dessin, donc personne d'autre ne pouvait le
    /// connaître.
    pub fn position(largeur_carte: f32, hauteur_carte: f32) -> Vec3 {
        Vec3::new(largeur_carte * 0.5, hauteur_carte * 0.5 - 0.5, 12.0)
    }

    /// Le carton est-il ouvert, à cet instant de la phase de choix ?
    pub fn est_ouvert(avancement: f32) -> bool {
        avancement >= Self::DUREE_SECOUSSE
    }

    pub fn draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        vp: Mat4,
        pos: Vec3,
        avancement: f32,
    ) {
        let (shake_offset_x, shake_offset_y, shake_rot_z) = if !Self::est_ouvert(avancement) {
            // Secousse, d'autant plus forte qu'on est loin de l'ouverture.
            let intensite = (Self::DUREE_SECOUSSE - avancement).max(0.0) / Self::DUREE_SECOUSSE;
            let sx = (avancement * 42.0).sin() * 0.12 * intensite;
            let sy = (avancement * 55.0).cos().abs() * 0.14 * intensite;
            let rz = (avancement * 35.0).sin() * 0.16 * intensite;
            (sx, sy, rz)
        } else {
            (0.0, 0.0, 0.0)
        };

        // `pos` est déjà le point final (voir `position`) : ici on n'ajoute que la secousse.
        let box_depth_pos = Vec3::new(pos.x + shake_offset_x, pos.y + shake_offset_y, pos.z);

        let (mesh_to_draw, offset, scale) = if !Self::est_ouvert(avancement) {
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
        let push = PushConstants {
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
