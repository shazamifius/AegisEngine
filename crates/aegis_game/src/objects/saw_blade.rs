use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::glb_loader::GlbLoader;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;

pub struct SawBladeObject {
    pub mesh: GpuMesh,
}

impl SawBladeObject {
    pub fn new(gpu: &GpuContext, memory_props: &vk::PhysicalDeviceMemoryProperties) -> Result<Self, Box<dyn std::error::Error>> {
        let (v, i) = GlbLoader::load_glb_bytes(include_bytes!("../../../../assets/modeles/saw_blade.glb"))?;

        // ── ⚠ LA SCIE N'EST PAS VOXELISÉE, ET C'EST UN VERDICT, PAS UN OUBLI ────────────────
        //
        // ## Ce qui a été tenté le 31 août 2026, et pourquoi c'est revenu en arrière
        //
        // Elle passait par `voxeliser_pour_le_monde` : la même trame que le décor, `1/SOUS_VOXELS`
        // de bloc. Le raisonnement tenait — un disque poli posé dans un monde de cubes alignés n'y
        // cohabite pas, il y est déposé — et la mesure aussi (771 sommets / 1402 triangles ont
        // donné 1392 sommets / **696** triangles, les faces internes n'étant jamais émises).
        //
        // **Le rendu, lui, a été rejeté à l'œil, et l'œil est le juge.** Ses mots : *« la
        // pixelisation de la scie est HORRIBLE (…) actuellement c'est vraiment trop moche »*.
        //
        // ## La cause exacte, parce qu'elle décide de la suite
        //
        // La scie mesure 1,5 unité dans sa plus grande dimension (`glb_loader` normalise), et
        // s'affiche à l'échelle 1 : **12 voxels de diamètre**. Ses dents, elles, mesurent une
        // fraction de ce diamètre — donc **moins d'un voxel**. La trame ne les arrondit pas : elle
        // les efface. Il ne restait qu'un disque cranté, et un piège mortel qui ne se reconnaît
        // plus n'est plus un piège, c'est une surprise.
        //
        // ⚠ **Le défaut d'origine est donc ROUVERT** (la scie est de nouveau lisse dans un monde de
        // cubes) — le dire plutôt que le taire : c'est un arbitrage entre deux défauts, pas une
        // correction.
        //
        // ⚠⚠ **ET CE COMMENTAIRE A MENTI PENDANT TROIS JOURS, ce qui vaut d'être gardé ici.** Il
        // affirmait, au présent, que `voxeliser_pour_le_monde` « reste juste et sert les autres
        // objets importés ». **Le commit qui écrivait cette phrase était celui qui supprimait cette
        // fonction** (`babe31c`, 31 août 2026), et plus aucun objet ne voxelisait quoi que ce soit.
        // Le module du moteur a dormi le 3 septembre — trouvé non pas par une relecture, mais par
        // une commande qui cherche ce que personne n'appelle (`--example etat`).
        //
        // *Un commentaire qui décrit une intention non réalisée est plus dangereux qu'une absence :
        // l'absence se remarque, la fausse assurance se fait confirmer par chaque relecture.*
        //
        // ## Ce qui la ferait entrer dans la trame POUR DE VRAI
        //
        // Rien ici. Ce qui manque est **en amont, dans le modèle** — des dents d'au moins deux voxels, donc
        // taillées pour une scie de 1,5 bloc, ou une scie plus grande. Affiner la trame pour un
        // seul objet est le piège à éviter : elle vaudrait alors deux chiffres au lieu d'un, et
        // l'objet cesserait de tomber sur la même grille que le décor — soit exactement la
        // propriété qu'on cherchait.
        let mesh = GpuMesh::upload(gpu, memory_props, &v, &i)?;
        log::info!(
            "Scie Rotative (forme lisse, non voxelisee) : {} sommets, {} triangles",
            v.len(),
            i.len() / 3
        );

        Ok(Self { mesh })
    }

    pub fn draw(
        &self,
        pose: &aegis_engine::render::instances::Pose,
        vp: Mat4,
        pos: Vec2,
        rotation: f32,
    ) {
        self.draw_at_3d(pose, vp, Vec3::new(pos.x, pos.y, 0.1), 1.0, rotation);
    }

    pub fn draw_at_3d(
        &self,
        pose: &aegis_engine::render::instances::Pose,
        vp: Mat4,
        pos: Vec3,
        scale: f32,
        rotation: f32,
    ) {
        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_z(rotation)
            * Mat4::from_rotation_x(90.0f32.to_radians())
            * Mat4::from_scale(Vec3::splat(scale));

        let push = PushConstants {
            model_matrix: model,
            color_tint: Vec4::new(0.85, 0.88, 0.92, 1.0), // Acier Métallique
            params: Vec4::new(0.1, 2.0, 0.0, 0.0),
        };

        pose.objet(&self.mesh, &push);
    }
}
