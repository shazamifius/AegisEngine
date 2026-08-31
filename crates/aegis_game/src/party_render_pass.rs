use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::GpuContext;
use aegis_engine::geometry::primitives::Primitives;
use aegis_engine::render::pipeline::PipelineFactory;
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::push_constants::PushConstants;
use crate::party_game::PartyGame;
use crate::grid::TileType;
use crate::objects::{
    map::MapObject,
    saw_blade::SawBladeObject,
    cannon_turret::CannonTurretObject,
    spike_trap::SpikeTrapObject,
    laser_emitter::LaserEmitterObject,
    flamethrower::FlamethrowerObject,
    plants::PlantsObject,
    rock::RockObject,
    cardboard_box::CardboardBoxObject,
};

fn tile_hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(seed);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

/// En combien de sous-voxels un bloc se divise.
///
/// ## ⭐ Le seul chiffre de toute la decoration, et pourquoi il remplace huit constantes
///
/// Les details poses sur les blocs — eclats de pierre, cailloux de terre, brins d'herbe — avaient
/// chacun leurs constantes : `0.11 + (h % 10) * 0.012` pour la taille, `(h % 65) / 100.0 - 0.32`
/// pour la position. Des valeurs CONTINUES, donc jamais alignees sur quoi que ce soit.
///
/// **C'est ce qui produisait du grain la ou l'on voulait du voxel.** Un observateur exterieur l'a
/// decrit comme « un motif de bruit 2D avec des pixels disperses sans logique de volume, creant
/// un effet sale » — et il a cru voir une texture photographique. Il n'y en a aucune : ce sont de
/// vrais cubes, simplement trop petits et poses n'importe ou.
///
/// Mesure : ces details faisaient 0,05 a 0,12 unite, soit **2 a 6 pixels a l'ecran**. Sous le
/// seuil ou l'oeil peut lire un volume, un cube ne devient pas un petit cube — il devient une
/// salissure.
///
/// Tout se pose desormais sur une grille de 1/8e de bloc. Les tailles et les positions sont des
/// ENTIERS de sous-voxels, donc alignees par construction. *Huit constantes arbitraires n'ont pas
/// retreci : elles ont cesse d'exister, remplacees par un seul entier qui a un sens.*
///
/// ## ⚠ Ce chiffre vit dans le JEU, et pourquoi le moteur n'y touchera jamais
///
/// Le moteur sait voxeliser ([`aegis_engine::geometry::voxel`]) : c'est de la geometrie, sans
/// gout. Mais **la finesse de la trame est une decision de direction artistique**, au meme titre
/// que la couleur du ciel. Un moteur qui graverait `1/8` deciderait du grain de tous les jeux
/// qu'il portera.
///
/// ## ⚠⚠ Et ce chiffre a une PORTEE, mesuree le 31 aout 2026
///
/// Une fonction passait ici les maillages importes dans cette meme trame, pour qu'un objet et un
/// detail de decor tombent sur la meme grille. Elle n'a jamais eu qu'un seul appelant — la scie —
/// et **son rendu a ete rejete a l'oeil** : voir `objects/saw_blade.rs`, qui porte la mesure et la
/// cause. Elle a donc ete retiree plutot que gardee morte.
///
/// Ce qu'il faut en retenir avant de la reecrire : un objet importe mesure 1,5 unite dans sa plus
/// grande dimension (`glb_loader` normalise) et s'affiche a l'echelle 1, donc **12 voxels**. Tout
/// detail plus fin que 1/12e de l'objet n'est pas arrondi par la trame : il est efface. La trame
/// convient aux formes taillees pour elle, pas aux modeles fins — et l'affiner pour un seul objet
/// ferait exactement ce que ce commentaire dit d'eviter : deux chiffres au lieu d'un.
const SOUS_VOXELS: u32 = 8;

/// Place un detail de `cotes` sous-voxels sur la face d'un bloc, aligne sur la sous-grille.
///
/// ⚠ `h` decide de la position, mais **seulement parmi les emplacements ou le detail tient
/// entier**. Un detail qui deborderait du bloc se verrait immediatement sur une carte reguliere.
fn detail_voxel(bloc_x: i32, bloc_y: i32, h: u32, cotes: u32, avancee: f32) -> Mat4 {
    let pas = 1.0 / SOUS_VOXELS as f32;
    // Les emplacements possibles : de 0 a (8 - cotes), inclus.
    let libres = (SOUS_VOXELS - cotes + 1).max(1);
    let cx = h % libres;
    let cy = (h / libres) % libres;

    Mat4::from_translation(Vec3::new(
        bloc_x as f32 + (cx as f32 + cotes as f32 * 0.5) * pas,
        bloc_y as f32 + (cy as f32 + cotes as f32 * 0.5) * pas,
        avancee,
    )) * Mat4::from_scale(Vec3::new(cotes as f32 * pas, cotes as f32 * pas, 0.92))
}





/// Ce que le monde **extérieur** apporte au rendu, en plus du jeu lui-même.
///
/// Les trois voyagent ensemble et grandiront ensemble. Les regrouper n'est pas du rangement :
/// sans ça, la liste d'arguments de `render_party_scene` s'allonge d'un cran chaque fois que le
/// jeu apprend quelque chose du dehors — et elle en était déjà à huit.
pub struct Exterieur<'a> {
    /// L'état du pont réseau, pour le témoin du HUD.
    pub pont: &'a crate::hud::EtatPont,
    /// Les joueurs distants, déjà validés par le cœur.
    pub distants: &'a [crate::sidecar_client::Avatar],
    /// Ce que le solveur pense de la franchissabilité de la carte.
    pub carte: crate::tas::EtatCarte,
    /// Le bloc à soumettre au vote quand la carte est bouchée — vide sinon.
    pub bouchon: &'a crate::tas::Bouchon,
    /// Le vote en cours, s'il y en a un.
    pub vote: Option<&'a crate::vote::Vote>,
    /// Où en est la démonstration du parcours, quand personne n'a réussi la manche.
    pub demonstration: Option<Vec2>,
    /// LE LOBBY, quand il est ouvert. Dessiné PAR-DESSUS tout le reste : choisir sa partie n'est
    /// pas une information de plus posée sur le jeu, c'est un moment à part.
    pub lobby: &'a crate::lobby::Lobby,
}

/// L'index du cube dans la table de maillages passee a la file. Une constante nommee plutot
/// qu'un `0` : le jour ou un second maillage entre dans la file, un chiffre nu serait illisible.
const MAILLAGE_CUBE: u16 = 0;

pub struct PartyRenderPass {
    bg_pipeline: vk::Pipeline,
    bg_pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    particle_pipeline: vk::Pipeline,
    /// Le même shader que la scène, monté pour l'image de la FENÊTRE. Le HUD se dessine après la
    /// composition — voir le commentaire à sa création.
    hud_pipeline: vk::Pipeline,
    /// Ce qui porte la lumière au pixel : la courbe de tonalité, appliquée une seule fois.
    ecran: aegis_engine::render::ecran::Ecran,
    /// Ce que le ciel ne voit pas — retiré de l'ambiante, et d'elle seule.
    occlusion: aegis_engine::render::occlusion::Occlusion,
    pipeline_layout: vk::PipelineLayout,
    /// Ce qui est vrai pour toute l'image : la vue-projection, la caméra, les lumières.
    cadre: aegis_engine::render::cadre::Cadre,
    /// Ce qu'il y a à dessiner cette image. ⚠ Elle existe pour les OMBRES : une ombre se
    /// calcule en rejouant la scène depuis la lumière, ce qui est impossible quand le dessin
    /// est impératif et entrelacé avec la logique de jeu.
    file: aegis_engine::render::file::File,
    /// La carte d'ombre du soleil. ⚠ Une seule : la deuxieme lumiere eclaire sans ombrer.
    ombre: aegis_engine::render::ombre::Ombre,

    pub cube_mesh: GpuMesh,
    pub char_mesh: GpuMesh,

    // Modules d'Objets 3D Blender (Disponibles dans le moteur)
    pub map_obj: MapObject,
    pub plant_obj: PlantsObject,
    pub rock_obj: RockObject,
    pub saw_obj: SawBladeObject,
    pub cannon_obj: CannonTurretObject,
    pub spike_obj: SpikeTrapObject,
    pub laser_obj: LaserEmitterObject,
    pub flame_obj: FlamethrowerObject,
    pub box_obj: CardboardBoxObject,

    /// Les images dans lesquelles l'image se fabrique : la profondeur, et — quand la carte sait
    /// anti-creneler — une couleur multi-echantillonnee resolue vers l'ecran a la fin de la passe.
    cibles: aegis_engine::render::cibles::Cibles,
    /// Le tampon d'instances : tout ce qui se dessine y passe, la foule de cubes comme le HUD.
    ///
    /// ⚠ 65 536 emplacements, soit 6 Mo. La carte la plus dense mesuree en pose ~3 500 ; le reste
    /// est de la marge pour les particules et l'interface. Un depassement ne casse rien — les
    /// objets en trop ne sont pas dessines et le compte est JOURNALISE, plutot que de disparaitre
    /// en silence.
    instances: aegis_engine::render::instances::Instances,
    /// La direction artistique de CE jeu : couleur du ciel, du sol, exposition, courbe.
    ///
    /// ⚠ Elle vit ici, jamais dans le moteur, et elle est MUTABLE parce que son juge est un œil :
    /// tant qu'un changement de couleur demandait une recompilation, l'aller-retour etait trop
    /// long pour comparer deux reglages, donc le choix se faisait de memoire. La console la regle
    /// en direct (« ambiance ciel 0.1 0.2 0.4 »), et ce qui plait devient le defaut ci-dessous.
    ambiance: aegis_engine::render::cadre::Ambiance,

    pub camera_pos: Vec3,
    pub camera_target: Vec3,
    pub zoom_level: f32,
}

impl PartyRenderPass {
    pub fn new(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation du Render Pass Mode Éditeur de Map...");

        let (c_v, c_i) = Primitives::create_cube(1.0, 1.0, 1.0);
        let cube_mesh = GpuMesh::upload(gpu, memory_props, &c_v, &c_i)?;

        let (ch_v, ch_i) = Primitives::create_character_mesh();
        let char_mesh = GpuMesh::upload(gpu, memory_props, &ch_v, &ch_i)?;

        let map_obj = MapObject::new(gpu, memory_props)?;
        let plant_obj = PlantsObject::new(gpu, memory_props)?;
        let rock_obj = RockObject::new(gpu, memory_props)?;
        let saw_obj = SawBladeObject::new(gpu, memory_props)?;
        let cannon_obj = CannonTurretObject::new(gpu, memory_props)?;
        let spike_obj = SpikeTrapObject::new(gpu, memory_props)?;
        let laser_obj = LaserEmitterObject::new(gpu, memory_props)?;
        let flame_obj = FlamethrowerObject::new(gpu, memory_props)?;
        let box_obj = CardboardBoxObject::new(gpu, memory_props)?;

        // Les cibles de rendu appartiennent au moteur : ce fichier decrit ce qu'il y a a voir,
        // pas comment on alloue une image Vulkan. Ces quarante lignes vivaient ici en DEUX
        // exemplaires (ouverture et redimensionnement) — c'etait la duplication qui allait
        // quadrupler en ajoutant l'anti-crenelage.
        let cibles = aegis_engine::render::cibles::Cibles::nouvelles(gpu, memory_props)?;
        let depth_format = cibles.format_profondeur;

        let bg_vert_spv = aegis_engine::shaders::BACKGROUND_VERT_SPV;
        let bg_frag_spv = aegis_engine::shaders::BACKGROUND_FRAG_SPV;
        let p_vert_spv = aegis_engine::shaders::PARTY_2D5_VERT_SPV;
        let p_frag_spv = aegis_engine::shaders::PARTY_2D5_FRAG_SPV;

        let bg_vert_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, bg_vert_spv)?;
        let bg_frag_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, bg_frag_spv)?;
        let p_vert_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, p_vert_spv)?;
        let p_frag_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, p_frag_spv)?;

        // Le cadre naît avant les layouts : c'est lui qui fournit la description du descripteur.
        let cadre = aegis_engine::render::cadre::Cadre::nouveau(gpu, memory_props)?;

        // ⚠ Le fond reçoit le MÊME descripteur que la scène, et c'est tout le correctif du
        // 29 août : il ne recevait rien du tout, donc il peignait un blanc écrit en dur pendant
        // que les objets étaient éclairés à 0,17. Il ne pousse en revanche aucune constante — un
        // fond n'a pas d'objet.
        let bg_pipeline_layout = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            std::slice::from_ref(&cadre.layout_descripteur),
            &[],
        )?;

        // ⭐ PLUS AUCUNE CONSTANTE POUSSEE, et c'est le vrai denouement d'un chantier entier.
        //
        // Le moteur en poussait 160 octets la ou Vulkan n'en garantit que 128 : il fonctionnait
        // ici (256 sur cette carte) et aurait tres probablement refuse de creer son pipeline sur
        // un Quest 2. On les avait ramenees a 96 ; elles ont maintenant DISPARU, remplacees par
        // un tampon d'instances qui n'a pas de plafond.
        //
        // *La constante arbitraire n'a pas retreci, elle a cesse d'exister* — et avec elle toute
        // une classe de pannes qui ne se seraient manifestees que chez quelqu'un d'autre.
        let pipeline_layout = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            std::slice::from_ref(&cadre.layout_descripteur),
            &[],
        )?;

        // ⚠ L'ordre est FORCE par une dependance : l'ombre a besoin du layout de pipeline, qui a
        // besoin du layout de descripteur du cadre. On DECLARE le descripteur a la creation du
        // cadre, et on le REMPLIT seulement maintenant — c'est ce qui casse le cercle.
        let ombre = aegis_engine::render::ombre::Ombre::nouvelle(
            gpu,
            memory_props,
            pipeline_layout,
            2048,
        )?;
        cadre.brancher_la_carte_d_ombre(&gpu.device, ombre.vue, ombre.echantillonneur);

        let bg_pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            bg_pipeline_layout,
            bg_vert_mod,
            bg_frag_mod,
            aegis_engine::render::pipeline::Reglages {
                // ⚠⚠ LE FORMAT DE LA SCENE, PAS CELUI DE LA FENETRE — et ils ne sont plus les
                // memes depuis le 30 aout 2026. Tout ce qui dessine de la LUMIERE ecrit dans la
                // cible HDR ; seules la composition et l'interface touchent l'image presentee.
                // Se tromper ici donne un pipeline que la carte refuse de creer, avec un message
                // sur les formats d'attachement — c'est la bonne nouvelle, il ne passe pas.
                color_format: cibles.format_hdr,
                // ⚠ La seconde sortie porte l'AMBIANTE seule. Un shader qui declare `@location(1)`
                // sans que ce format soit renseigne — ou l'inverse — donne un pipeline que la carte
                // refuse : les deux se decident ensemble.
                second_format: Some(cibles.format_hdr),
                // Le fond n'a ni profondeur ni sommets : son shader fabrique ses trois points.
                depth_format: None,
                depth_write: false,
                melange: aegis_engine::render::pipeline::Melange::Aucun,
                use_vertex_input: false,
                echantillons: cibles.echantillons,
            },
        )?;

        let pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            pipeline_layout,
            p_vert_mod,
            p_frag_mod,
            aegis_engine::render::pipeline::Reglages {
                color_format: cibles.format_hdr,
                second_format: Some(cibles.format_hdr),
                depth_format: Some(depth_format),
                depth_write: true,
                melange: aegis_engine::render::pipeline::Melange::Aucun,
                use_vertex_input: true,
                echantillons: cibles.echantillons,
            },
        )?;

        let particle_pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            pipeline_layout,
            p_vert_mod,
            p_frag_mod,
            aegis_engine::render::pipeline::Reglages {
                color_format: cibles.format_hdr,
                second_format: Some(cibles.format_hdr),
                // Les particules se melangent et n'ecrivent pas la profondeur : sinon la premiere
                // dessinee masquerait toutes celles qui sont derriere elle.
                depth_format: Some(depth_format),
                depth_write: false,
                melange: aegis_engine::render::pipeline::Melange::Transparence,
                use_vertex_input: true,
                echantillons: cibles.echantillons,
            },
        )?;

        // ⭐ LA CHAINE QUI PORTE LA LUMIERE AU PIXEL. Elle naît ici parce qu'elle a besoin du
        // descripteur du cadre : c'est lui qui porte l'exposition et le point blanc, et la courbe
        // de tonalite est la seule chose que la composition fait.
        let ecran = aegis_engine::render::ecran::Ecran::nouveau(
            gpu,
            memory_props,
            &cibles,
            cadre.layout_descripteur,
        )?;

        // ⭐ L'occlusion reutilise le contrat de l'ecran plutot que d'en definir un second : une
        // passe plein ecran lit une image, quelle qu'elle soit, et deux descriptions du meme
        // contrat finiraient par diverger sans que rien ne le signale.
        let occlusion = aegis_engine::render::occlusion::Occlusion::nouvelle(
            gpu,
            &cibles,
            ecran.layout_descripteur(),
            ecran.layout_pipeline(),
        )?;

        // ── LE MEME SHADER, MONTE UNE SECONDE FOIS POUR L'INTERFACE ──────────────────────────
        //
        // ⚠ Ce n'est pas une duplication : c'est le MEME code monte pour une autre cible. Le HUD
        // se dessine apres la composition, donc dans l'image de la FENETRE et non dans la scene
        // HDR — un pipeline Vulkan porte le format de sa cible, il en faut donc un par cible.
        //
        // Trois differences avec celui de la scene, chacune pour une raison :
        //  • le format de l'ecran, forcement ;
        //  • **aucune profondeur** : le HUD se trie desormais lui-meme (voir `ui::Pinceau::
        //    terminer`), ce qui epargne une image de profondeur pleine resolution ;
        //  • un seul echantillon : l'interface n'a pas d'arete geometrique a lisser.
        //
        // ⚠ Il paie encore tout l'eclairage PBR pour rien — chaque lettre d'un score traverse la
        // boucle des lumieres et quatre lectures de la carte d'ombre avant que `params.w` ne jette
        // le resultat. C'est une dette CONNUE, laissee ouverte exprès : la fermer demande un
        // shader d'interface a part, et l'ajouter dans le meme chantier rendrait impossible de
        // savoir lequel des deux changements a bouge l'image.
        let hud_pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            ecran.layout_pipeline(),
            p_vert_mod,
            p_frag_mod,
            aegis_engine::render::pipeline::Reglages {
                color_format: gpu.swapchain_format,
                second_format: None,
                depth_format: None,
                depth_write: false,
                melange: aegis_engine::render::pipeline::Melange::Aucun,
                use_vertex_input: true,
                echantillons: vk::SampleCountFlags::TYPE_1,
            },
        )?;

        unsafe {
            gpu.device.destroy_shader_module(bg_vert_mod, None);
            gpu.device.destroy_shader_module(bg_frag_mod, None);
            gpu.device.destroy_shader_module(p_vert_mod, None);
            gpu.device.destroy_shader_module(p_frag_mod, None);
        }

        Ok(Self {
            bg_pipeline,
            bg_pipeline_layout,
            pipeline,
            particle_pipeline,
            hud_pipeline,
            ecran,
            occlusion,
            pipeline_layout,
            cadre,
            file: aegis_engine::render::file::File::nouvelle(),
            ombre,

            cube_mesh,
            char_mesh,

            map_obj,
            plant_obj,
            rock_obj,
            saw_obj,
            cannon_obj,
            spike_obj,
            laser_obj,
            flame_obj,
            box_obj,

            cibles,
            instances: aegis_engine::render::instances::Instances::nouveau(gpu, memory_props, 65_536)?,
            // ── LE POINT DE DEPART DE LA DIRECTION ARTISTIQUE — a regler a l'oeil ───────────
            //
            // ⚠ Ce n'est PAS un choix esthetique arrete : c'est un point de depart, et il attend
            // d'etre remplace par ce que son oeil retiendra du laboratoire (`ambiance ...` dans
            // la console). Le juge du rendu percu n'est ni une metrique ni ce commentaire.
            //
            // Il n'est pas neutre pour autant, et c'est deliberé : le defaut du MOTEUR pose
            // ciel = sol, donc une ambiante plate — la capacite existe alors sans que personne
            // s'en serve, et *un mecanisme jamais exerce est mort.* Ces deux teintes ont la meme
            // luminosite que le gris qu'elles remplacent : seule leur TEMPERATURE change, ce qui
            // donne du volume sans decider du reste.
            ambiance: aegis_engine::render::cadre::Ambiance {
                // ── LE RAPPORT DIRECT / AMBIANT, choisi sur une MESURE ─────────────────────
                //
                // Ce ne sont pas des couleurs choisies a l'oeil : c'est un rapport, et son effet
                // se chiffre. Balayage de quatre reglages sur la scene reelle, etendue tonale
                // mesuree (du 5e au 95e centile de clarte, sur 100) :
                //
                //     ambiante forte, soleil 2,45  →  etendue 20, 92 % percu gris
                //     ambiante /3,    soleil 5,0   →  etendue  1, 99 % percu gris
                //     ambiante basse, soleil 7,5   →  etendue 50, 66 % percu gris   ← retenu
                //     ciel clair,     soleil 6,0   →  etendue 29, 82 % percu gris
                //
                // ⚠ Ces quatre releves ne sont PAS rigoureusement comparables : la fenetre a
                // change de taille et la phase de jeu a bouge pendant le balayage. C'est
                // exactement pourquoi un banc SANS FENETRE est necessaire, et pourquoi ce
                // reglage est un point de depart et non une conclusion.
                //
                // ⚠⚠ Et la TEINTE, elle, reste entierement a decider a l'oeil. Ce qui est etabli
                // ici est la STRUCTURE — une ambiante faible et un soleil dur, c'est-a-dire une
                // scene ensoleillee plutot qu'un temps couvert. La couleur du ciel se regle en
                // direct : `ambiance ciel 0.03 0.04 0.07` dans la console.
                ciel: [0.03, 0.04, 0.07],
                sol: [0.05, 0.04, 0.03],
                intensite_soleil: 7.5,
                ..aegis_engine::render::cadre::Ambiance::default()
            },

            camera_pos: Vec3::new(5.0, 3.0, 16.0),
            camera_target: Vec3::new(5.0, 3.0, 0.0),
            zoom_level: 1.0,
        })
    }

    /// La caméra de la partie — **la seule**, lue par le rendu comme par la détection de clic.
    ///
    /// Ses réglages ne vivent qu'ici : un clic qui recalculerait les siens finirait par viser un
    /// monde légèrement différent de celui qu'on voit, et ce décalage-là ne se remarque pas, il
    /// s'endure.
    pub fn camera(&self, aspect: f32) -> aegis_engine::scene::camera::Camera {
        aegis_engine::scene::camera::Camera {
            position: self.camera_pos,
            target: self.camera_target,
            up: Vec3::Y,
            fov_y_radians: 38.0f32.to_radians(),
            aspect_ratio: aspect,
            z_near: 0.1,
            z_far: 500.0,
        }
    }

    pub fn render_party_scene(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, image_index: usize, game: &PartyGame, exterieur: &Exterieur) {
        let view = context.swapchain_image_views[image_index];
        let image = context.swapchain_images[image_index];

        // Suivi de caméra fluide (Drafting = vue d'ensemble dézoommée de la map, Placement = suit le curseur d'édition avec Zoom, Running = suit le joueur)
        let (target_x, target_y, target_dist) = if game.phase == crate::party_game::GamePhase::Drafting {
            (game.grid.width as f32 / 2.0, game.grid.height as f32 / 2.0, 42.0)
        } else if game.phase == crate::party_game::GamePhase::Placement || !game.is_play_mode {
            let (cx, cy) = game.editor.cursor;
            (cx as f32 + 0.5, cy as f32 + 0.5, 18.0 * self.zoom_level)
        } else if let Some(fantome) = exterieur.demonstration {
            // Une démonstration est en cours : c'est ELLE qu'il faut regarder. Laisser la caméra
            // sur le cadavre du joueur reviendrait à montrer la solution à personne — c'est
            // exactement l'erreur qui rendait le tableau des scores invisible.
            (fantome.x, fantome.y + 0.85, 20.0 * self.zoom_level)
        } else {
            let p_pos = game.human_player().position;
            (p_pos.x, p_pos.y + 0.85, 18.0 * self.zoom_level)
        };


        self.camera_target = self.camera_target.lerp(Vec3::new(target_x, target_y, 0.0), 0.18);
        self.camera_pos = self.camera_target + Vec3::new(0.0, 0.5, target_dist);

        // ⚠ UNE SEULE CAMÉRA POUR LE RENDU ET POUR LES CLICS (29 août 2026).
        // Ces quatre valeurs — 38°, l'aspect, 0,1 et 500 — étaient écrites TROIS fois : ici, et
        // deux fois dans `main.rs` pour deviner ce qu'on venait de cliquer. Changer le champ de
        // vision du rendu faisait donc viser les clics à côté, sans qu'aucun test ni aucun écran
        // ne le signale. On construit la caméra du moteur, et les clics lisent LA MÊME.
        let aspect = context.swapchain_extent.width as f32 / context.swapchain_extent.height as f32;
        let camera = self.camera(aspect);
        let vp = camera.compute_projection_matrix() * camera.compute_view_matrix();

        // ── LES LUMIÈRES DE LA SCÈNE ────────────────────────────────────────────────────────
        //
        // ⚠ **DEUX lumières, et la seconde n'est pas décorative : elle EXERCE le mécanisme.**
        // Un éclairage multi-lumières qui ne porte qu'une lumière est un mécanisme jamais exercé,
        // donc mort sans que rien ne le dise — la famille de défauts n° 1 de ce projet. La
        // ponctuelle prouve en conditions réelles le second type, l'atténuation en carré inverse,
        // et le fait que la boucle du shader parcourt bien ce qu'on lui annonce.
        //
        // ⚠⚠ Les valeurs ci-dessous sont un POINT DE DÉPART TECHNIQUE, pas une direction
        // artistique : le juge du rendu perçu est son œil, et lui seul. Elles sont ici pour être
        // corrigées, pas pour être défendues.
        let soleil = aegis_engine::scene::light::GpuLight::new_directional(
            // Pointe VERS la lumière — c'est la convention que le shader attend.
            Vec3::new(0.4, 0.9, 0.7),
            Vec3::new(1.0, 0.96, 0.88),
            // ⚠ Vient de l'ambiance, et c'est ce qui rend le RAPPORT direct/ambiant reglable en
            // direct. La valeur d'origine — 2,45 — n'etait pas tatonnee : le diffus vaut
            // albedo x (1-F) x I x NdotL / π, donc pour qu'une face en plein soleil rende ~0,75
            // avant l'ambiante il faut I ≈ 0,75 x π / 0,96. Elle reste le defaut du moteur.
            self.ambiance.intensite_soleil,
        );
        // Une lampe chaude au-dessus du joueur. Sa portée n'est bornée par aucune constante : le
        // carré inverse l'éteint tout seul, ce qui évite un rayon arbitraire à justifier.
        let ici = game.human_player().position;
        let lampe = aegis_engine::scene::light::GpuLight::new_point(
            Vec3::new(ici.x, ici.y + 2.5, 2.0),
            Vec3::new(1.0, 0.75, 0.45),
            // A 2,5 unites, le carre inverse rend 8 / 6,25 ≈ 1,3 — du meme ordre que le soleil,
            // donc visible sans l'ecraser.
            8.0,
        );
        // ── L'AMBIANCE : c'est LE JEU qui la décide, le moteur ne fait que l'appliquer ──────
        // Elle se règle en direct depuis la console (`ambiance ciel 0.1 0.2 0.4`) : voir le champ
        // du même nom. Le moteur, lui, ne saura jamais de quelle couleur est ce ciel.
        let ambiance = self.ambiance;

        // La zone que la carte d'ombre couvre : centree sur la camera, assez large pour porter
        // ce qu'on voit. ⚠ Ce qui sort de cette sphere ne projette rien — compromis de toute carte
        // d'ombre unique, et il vaut mieux l'ecrire que le laisser decouvrir.
        let centre_ombre = Vec3::new(self.camera_target.x, self.camera_target.y, 0.0);
        let matrice_lumiere = aegis_engine::render::ombre::matrice_lumiere(
            Vec3::new(0.4, 0.9, 0.7),
            centre_ombre,
            28.0,
        );

        self.cadre.ecrire(&aegis_engine::render::cadre::DonneesImage::nouvelle(
            vp,
            matrice_lumiere,
            [self.camera_pos.x, self.camera_pos.y, self.camera_pos.z],
            ambiance,
            &[soleil, lampe],
        ));

        unsafe {
            // ── LA DESCRIPTION D'ABORD, LE DESSIN ENSUITE ────────────────────────────────
            // La file se remplit ici, AVANT toute passe de rendu, parce que la carte d'ombre a
            // besoin de la connaitre pour rejouer la scene depuis la lumiere. Ce bloc ne fait
            // aucun appel Vulkan : il decrit, il ne dessine pas.
            // Une image neuve : le tampon d'instances repart de zero. ⚠ L'oublier ferait
            // deborder au bout de quelques images, et les objets disparaitraient un par un.
            self.instances.recommencer();
            self.file.vider();
                        for y in 0..game.grid.height {
                for x in 0..game.grid.width {
                    let xi = x as i32;
                    let yi = y as i32;
                    let tile = game.grid.get_tile(xi, yi);
                    if tile != TileType::Air && tile != TileType::StartPoint {
                        let model = Mat4::from_translation(Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0))
                            * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0));

                        let col = match tile {
                            TileType::GrassBlock => Vec4::new(0.32, 0.82, 0.36, 1.0), // Herbe Vert Vif Pur
                            TileType::SolidBlock => Vec4::new(0.48, 0.32, 0.20, 1.0), // Terre Marron
                            TileType::MetalBlock => Vec4::new(0.60, 0.63, 0.68, 1.0), // Pierre Grise
                            _ => tile.color(),
                        };

                        self.file.ajouter(aegis_engine::render::file::Dessin {
                            maillage: MAILLAGE_CUBE,
                            modele: model,
                            teinte: col,
                            params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                            porte_une_ombre: true,
                        });

                        // 1. Décoration Procédurale de la Roche (Pierre) : Petits cubes gris plus sombres encastrés
                        if tile == TileType::MetalBlock {
                            for k in 0..3 {
                                let h = tile_hash(xi, yi, k);
                                // Deux sous-voxels de cote, soit un quart de bloc : c'est la plus
                                // petite taille qui se lise encore comme un volume a distance de
                                // jeu. En dessous, on ajoute du bruit, pas du detail.
                                let sub_m = detail_voxel(xi, yi, h, 2, 0.05);
                                self.file.ajouter(aegis_engine::render::file::Dessin {
                                    maillage: MAILLAGE_CUBE,
                                    modele: sub_m,
                                    teinte: Vec4::new(0.40, 0.43, 0.48, 1.0),
                                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                    porte_une_ombre: true,
                                });
                            }
                        }

                        // 2. Décoration Procédurale de la Terre (Dirt) : Très léger et très petit (1 à 2 micro-tavelures subtiles)
                        if tile == TileType::SolidBlock {
                            for k in 0..2 {
                                let h = tile_hash(xi, yi, k + 10);
                                // La terre porte des cailloux, pas de la poussiere : ils faisaient
                                // 0,05 unite, c'est-a-dire deux pixels a l'ecran — invisibles
                                // individuellement, et sales collectivement.
                                let sub_m = detail_voxel(xi, yi, h, 2, 0.05);
                                self.file.ajouter(aegis_engine::render::file::Dessin {
                                    maillage: MAILLAGE_CUBE,
                                    modele: sub_m,
                                    teinte: Vec4::new(0.42, 0.27, 0.16, 1.0),
                                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                    porte_une_ombre: true,
                                });
                            }
                        }

                        // 3. Décoration Procédurale de l'Herbe (Grass) : Brins d'Herbe 3D Voxels au sommet
                        if tile == TileType::GrassBlock {
                            // Brins d'Herbe Voxels Procéduraux si le bloc au-dessus est vide (Air)
                            if game.grid.get_tile(xi, yi + 1) == TileType::Air {
                                // ⚠ TROIS brins et non cinq, mais visibles. Cinq brins de deux
                                // pixels de large forment une frange indistincte ; trois brins
                                // d'un sous-voxel se lisent comme de l'herbe.
                                for k in 0..3 {
                                    let h = tile_hash(xi, yi, k + 50);
                                    let pas = 1.0 / SOUS_VOXELS as f32;
                                    // Le brin occupe une colonne de la sous-grille, et monte d'un
                                    // a trois sous-voxels : des entiers, comme le reste.
                                    let colonne = h % SOUS_VOXELS;
                                    let hauteur = 1 + (h / SOUS_VOXELS) % 3;
                                    let blade_w = pas;
                                    let blade_h = hauteur as f32 * pas;

                                    let green_color = match k % 3 {
                                        0 => Vec4::new(0.32, 0.90, 0.35, 1.0), // Vert Vif
                                        1 => Vec4::new(0.20, 0.78, 0.26, 1.0), // Émeraude
                                        _ => Vec4::new(0.14, 0.65, 0.20, 1.0), // Forêt
                                    };

                                    // Aucune rotation : un brin incline sur une grille de voxels
                                    // se lit comme une erreur d'alignement, pas comme du vent.
                                    let blade_m = Mat4::from_translation(Vec3::new(
                                        x as f32 + (colonne as f32 + 0.5) * pas,
                                        y as f32 + 1.0 + blade_h * 0.5,
                                        0.02,
                                    )) * Mat4::from_scale(Vec3::new(blade_w, blade_h, 0.18));
                                    self.file.ajouter(aegis_engine::render::file::Dessin {
                                        maillage: MAILLAGE_CUBE,
                                        modele: blade_m,
                                        teinte: green_color,
                                        params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                        porte_une_ombre: true,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // ── LA CARTE D'OMBRE ─────────────────────────────────────────────────────────────
            // Hors de toute passe de rendu, et avant la passe principale qui la lira.
            if std::env::var("AEGIS_SANS_OMBRE").is_err() {
                self.ombre.dessiner(
                    &context.device,
                    cmd,
                    self.pipeline_layout,
                    &self.file,
                    &self.instances,
                    &[&self.cube_mesh],
                );
            }
            context.jalon(cmd, "ombre");

            // ⚠ La scene resolue passe en attachement : c'est elle qu'on ecrit maintenant, et
            // l'image de la fenetre attendra la composition. Elle est `UNDEFINED` a chaque image
            // parce que la passe l'efface entierement — reclamer son ancien contenu couterait une
            // lecture de plus pour des pixels qu'on va tous recouvrir.
            let barrier_scene = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .image(self.cibles.image_resolue())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let barrier_depth = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .image(self.cibles.image_profondeur())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            // ⚠ L'image multi-echantillonnee a besoin de sa PROPRE barriere : elle est neuve a
            // chaque image (`UNDEFINED`) et rien d'autre ne la fait entrer dans la disposition
            // d'attachement. L'oublier ne produit aucune erreur — juste un rendu indefini, ce que
            // le pilote a le droit de faire ressembler a n'importe quoi.
            // ⚠ Chaque image ECRITE par la passe a besoin d'entrer dans sa disposition, y compris
            // celles qui ne sont que des cibles de reduction : la carte y ecrit a la fin de la
            // passe, et une image restee `UNDEFINED` y recoit un contenu que rien ne definit.
            let en_attachement = |image, aspect, acces| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(if aspect == vk::ImageAspectFlags::DEPTH {
                        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                    } else {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    })
                    .src_access_mask(vk::AccessFlags::NONE)
                    .dst_access_mask(acces)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
            };

            let mut barrieres = vec![
                barrier_scene,
                barrier_depth,
                en_attachement(
                    self.cibles.image_ambiante_resolue(),
                    vk::ImageAspectFlags::COLOR,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                ),
                en_attachement(
                    self.cibles.image_profondeur_lisible(),
                    vk::ImageAspectFlags::DEPTH,
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                ),
            ];
            if let Some(image_ambiante) = self.cibles.image_ambiante() {
                barrieres.push(en_attachement(
                    image_ambiante,
                    vk::ImageAspectFlags::COLOR,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                ));
            }
            if let Some(image_msaa) = self.cibles.image_couleur() {
                barrieres.push(
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::NONE)
                        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .image(image_msaa)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                );
            }

            context.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS, vk::DependencyFlags::empty(), &[], &[], &barrieres);

            // ⭐ LA RESOLUTION DES ECHANTILLONS, et c'est elle qui rend l'anti-crenelage abordable.
            //
            // On dessine dans l'image multi-echantillonnee, et la carte fait la moyenne des
            // echantillons vers l'image presentee A LA FIN de la passe, en une fois. Le
            // `DONT_CARE` sur l'attachement multi-echantillonne n'est pas une negligence : il dit
            // que les echantillons eux-memes ne servent a rien apres la moyenne. Un GPU a tuiles
            // peut alors ne JAMAIS les ecrire en memoire centrale — c'est tout l'ecart entre un
            // anti-crenelage gratuit et un anti-crenelage hors budget sur la machine de reference.
            //
            // Quand la carte ne sait pas anti-creneler, `vue_couleur` rend l'image presentee
            // elle-meme et rien de tout cela n'existe : le meme code sert les deux chemins.
            let mut color_attach = vk::RenderingAttachmentInfo::default()
                .image_view(self.cibles.vue_couleur())
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(if self.cibles.resout() {
                    vk::AttachmentStoreOp::DONT_CARE
                } else {
                    vk::AttachmentStoreOp::STORE
                })
                // ⚠ Zero, et non plus un blanc studio a 0,92. Cette valeur est desormais de la
                // LUMIERE, pas une couleur d'ecran : un 0,92 lineaire y serait une source
                // eclatante. De toute facon le fond recouvre chaque pixel — cet effacement n'est
                // qu'un point de depart, et le noir est le seul qui ne mente pas sur ce qu'il est.
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] } });

            if self.cibles.resout() {
                color_attach = color_attach
                    .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                    .resolve_image_view(self.cibles.vue_resolue())
                    .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            }

            // ⚠ La SECONDE cible : l'ambiante seule. Elle suit exactement le sort de la couleur —
            // multi-echantillonnee et jetee apres la moyenne, ou ecrite directement s'il n'y a pas
            // d'anti-crenelage. C'est elle qui rend l'occlusion ambiante juste plutot qu'a peu pres.
            let mut ambiante_attach = vk::RenderingAttachmentInfo::default()
                .image_view(self.cibles.vue_ambiante())
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(if self.cibles.resout() {
                    vk::AttachmentStoreOp::DONT_CARE
                } else {
                    vk::AttachmentStoreOp::STORE
                })
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] },
                });
            if self.cibles.resout() {
                ambiante_attach = ambiante_attach
                    .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                    .resolve_image_view(self.cibles.vue_ambiante_resolue())
                    .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            }

            let depth_attach = vk::RenderingAttachmentInfo::default()
                .image_view(self.cibles.vue_profondeur())
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } });

            // ⚠ La profondeur est REDUITE elle aussi, en `SAMPLE_ZERO` : on garde le premier
            // echantillon plutot que d'en faire une moyenne. Moyenner des profondeurs n'a aucun
            // sens — la moyenne de « 3 m » et « 10 m » designe une distance ou il n'y a rien, et
            // l'occlusion se calculerait sur une surface fantome le long de chaque arete.
            let depth_attach = match self.cibles.resolution_profondeur() {
                None => depth_attach,
                Some(vers) => depth_attach
                    .resolve_mode(vk::ResolveModeFlags::SAMPLE_ZERO)
                    .resolve_image_view(vers)
                    .resolve_image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
            };

            let attaches = [color_attach, ambiante_attach];
            let rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent })
                .layer_count(1)
                .color_attachments(&attaches)
                .depth_attachment(&depth_attach);

            context.device.cmd_begin_rendering(cmd, &rendering_info);

            // 1. Fond Studio
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.bg_pipeline);
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: context.swapchain_extent.width as f32, height: context.swapchain_extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
            let scissor = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent };
            context.device.cmd_set_viewport(cmd, 0, &[viewport]);
            context.device.cmd_set_scissor(cmd, 0, &[scissor]);
            // Sans cette ligne, le fond ne connaîtrait pas la lumière de la scène et retomberait
            // dans le défaut qu'on vient de fermer. Il lit la même caméra et le même ciel que les
            // objets — et c'est ce qui les met enfin dans la même image.
            self.cadre.lier(&context.device, cmd, self.bg_pipeline_layout);
            // Le fond ne passe pas par un maillage (le shader fabrique ses trois sommets) :
            // c'est le seul dessin du projet qui doit s'annoncer lui-même.
            aegis_engine::mesure::noter_dessin(1);
            context.device.cmd_draw(cmd, 3, 1, 0, 0);

            context.jalon(cmd, "fond");

            // 2. Pipeline Opaque : Rendu des Blocs de la Grille Posés par le Joueur
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            // Le descripteur est lié une fois pour toute la passe : tous les dessins qui suivent
            // partagent la même caméra et les mêmes lumières.
            self.cadre.lier(&context.device, cmd, self.pipeline_layout);



            // La file est jouee ICI, apres avoir ete remplie. Le meme contenu servira
            // a la passe d'ombre, sans que le jeu ait a redire ce qu'il y a a dessiner.
            // Deja dans le bloc unsafe de la passe : pas de second niveau.
            // ⚠ Le tampon d'instances est lie ICI, une fois pour toute la passe : tous les
            // dessins qui suivent y puisent, la grille comme les objets uniques comme le HUD.
            self.instances.lier(&context.device, cmd);
            // Les quatre valeurs qui accompagnent chaque objet, reunies une fois : c'est le
            // « ou je dessine » que chaque piege, chaque decor et chaque particule recevait
            // jusqu'ici en quatre morceaux.
            let pose = aegis_engine::render::instances::Pose {
                device: &context.device,
                cmd,
                layout: self.pipeline_layout,
                instances: &self.instances,
            };
            let bilan =
                self.file.dessiner(&context.device, cmd, &self.instances, &[&self.cube_mesh]);
            if bilan.ignores > 0 {
                log::warn!(
                    "file de rendu : {} dessins ignores (maillage inconnu ou tampon plein)",
                    bilan.ignores
                );
            }
            // Rendu de l'Immense Bande Noire du Vide (Void Kill Line - 5 blocs en dessous du bloc le plus bas)
            let void_y = game.grid.get_void_kill_y();
            let void_model = Mat4::from_translation(Vec3::new(game.grid.width as f32 / 2.0, void_y + 0.5, -0.1))
                * Mat4::from_scale(Vec3::new(500.0, 1.0, 2.0)); // Bande noire infinie sur les côtés X, 1 bloc de hauteur Y

            let push_void = PushConstants {
                model_matrix: void_model,
                color_tint: Vec4::new(0.02, 0.02, 0.05, 1.0), // Noir Abyssal Profond
                params: Vec4::new(0.8, 0.0, 0.0, 0.0),
            };

            self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_void);

            // ─── Rendu des Pièges et Objets 3D Posés            // ─── Rendu des Pièges et Objets 3D Posés sur la Carte ─────────────────────
            let trap_t = game.round_timer;
            for trap in &game.traps.traps {
                match &trap.kind {
                    crate::traps::TrapKind::SawBlade { rotation, .. } => {
                        self.saw_obj.draw(&pose, vp, trap.position, *rotation);
                    }
                    crate::traps::TrapKind::CannonTurret { dir, .. } => {
                        self.cannon_obj.draw(&pose, Some(&self.cube_mesh), vp, trap.position, *dir, false);
                    }
                    crate::traps::TrapKind::SpikeTrap => {
                        self.spike_obj.draw(&pose, vp, trap.position);
                    }
                    crate::traps::TrapKind::LaserEmitter { dir, active, .. } => {
                        let beam_len = crate::traps::compute_laser_beam_length(trap.position, *dir, &game.grid);
                        let is_active = *active && game.phase == crate::party_game::GamePhase::Running;
                        self.laser_obj.draw(&pose, &self.cube_mesh, vp, trap.position, *dir, is_active, beam_len, trap_t);
                    }
                    crate::traps::TrapKind::Flamethrower { dir, active, .. } => {
                        let is_active = *active && game.phase == crate::party_game::GamePhase::Running;
                        self.flame_obj.draw(&pose, &self.cube_mesh, vp, trap.position, *dir, is_active, trap_t);
                    }
                    _ => {}
                }
            }

            // ─── Rendu des Projectiles Cinétiques (Balles & Traînée Voxel Lumineuse) ────
            for proj in &game.traps.projectiles {
                let dir_norm = proj.velocity.normalize_or_zero();

                // 1. Tête de Balle Cinétique Compacte et Brillante (Or Incandescent)
                let bullet_m = Mat4::from_translation(Vec3::new(proj.position.x, proj.position.y, 0.2))
                    * Mat4::from_scale(Vec3::splat(0.22)); // Compacte et légère !
                let push_bullet = PushConstants {
                    model_matrix: bullet_m,
                    color_tint: Vec4::new(1.00, 0.85, 0.20, 1.0), // Or Incandescent Vif
                    params: Vec4::new(0.0, 14.0, 0.0, 0.0),        // Lueur émissive
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_bullet);

                // 2. Traînée de Balle Voxel Décroissante (Trail)
                let trail_steps = 7;
                for step in 1..=trail_steps {
                    let step_f = step as f32;
                    let trail_pos = proj.position - dir_norm * (step_f * 0.12);
                    let trail_scale = 0.18 * (1.0 - step_f / (trail_steps as f32 + 1.0));

                    let trail_m = Mat4::from_translation(Vec3::new(trail_pos.x, trail_pos.y, 0.18))
                        * Mat4::from_scale(Vec3::splat(trail_scale));

                    let alpha = 0.95 - (step_f / trail_steps as f32) * 0.75;
                    let trail_color = if step <= 2 {
                        Vec4::new(1.00, 0.55, 0.05, alpha) // Orange Feu
                    } else if step <= 4 {
                        Vec4::new(0.90, 0.25, 0.05, alpha) // Rouge Incandescent
                    } else {
                        Vec4::new(0.50, 0.50, 0.55, alpha * 0.5) // Fumée Grise
                    };

                    let push_trail = PushConstants {
                        model_matrix: trail_m,
                        color_tint: trail_color,
                        params: Vec4::new(0.0, 8.0, 0.0, 0.0),
                    };
                    self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_trail);
                }
            }

            // ─── Rendu du Carton Mystère 3D (Phase Drafting) ───────────────────────────
            if game.phase == crate::party_game::GamePhase::Drafting {
                // L'avancement de l'animation se LIT sur le minuteur de la phase : il repart donc
                // de lui-même à chaque manche, et ne dépend plus du nombre d'images par seconde
                // (il avançait d'un `dt` figé à 16 ms, deux fois trop vite sur un écran 120 Hz).
                //
                // La gerbe d'ouverture, elle, n'est plus émise ici : le rendu ne peut pas modifier
                // le jeu, si bien que l'ancien appel travaillait sur un `clone()` jeté à la ligne
                // suivante — les particules ne sont jamais arrivées nulle part. Elle est désormais
                // déclenchée dans `PartyGame::update`, là où l'état est réellement modifiable.
                let avancement = crate::party_game::DUREE_DRAFT - game.draft_timer;
                let box_pos = CardboardBoxObject::position(
                    game.grid.width as f32,
                    game.grid.height as f32,
                );
                self.box_obj.draw(&pose, vp, box_pos, avancement);

                // Objets 3D Disponibles éparpillés DANS l'intérieur du carton ouvert sans piédestaux
                if CardboardBoxObject::est_ouvert(avancement) {
                    let items = &game.mystery_box.available_items;

                    for (i, item) in items.iter().enumerate() {
                        let (offset_vec, scale_factor) = crate::mystery_box::compute_box_item_offset(i, items.len());
                        let item_pos = box_pos + offset_vec;

                        let is_selected = game.mystery_box.selected_index == Some(i);

                        // Rendu Distinct de l'Objet 3D ou du Bloc Coloré avec échelle adaptative
                        let scale_mult = if is_selected { scale_factor * 1.25 } else { scale_factor };

                        match item {
                            crate::mystery_box::ItemType::SawBlade => self.saw_obj.draw_at_3d(&pose, vp, item_pos, scale_mult, 0.0),
                            crate::mystery_box::ItemType::CannonTurret => self.cannon_obj.draw_at_3d(&pose, None, vp, item_pos, crate::traps::Direction::Right, scale_mult, false),
                            crate::mystery_box::ItemType::SpikeTrap => self.spike_obj.draw_at_3d(&pose, vp, item_pos, scale_mult),
                            crate::mystery_box::ItemType::LaserEmitter => self.laser_obj.draw_at_3d(&pose, &self.cube_mesh, vp, item_pos, crate::traps::Direction::Up, scale_mult, false, 0.0, 0.0),
                            crate::mystery_box::ItemType::Flamethrower => self.flame_obj.draw_at_3d(&pose, &self.cube_mesh, vp, item_pos, crate::traps::Direction::Right, scale_mult, false, 0.0),
                            _ => {
                                let block_color = match item {
                                    crate::mystery_box::ItemType::SolidBlock => Vec4::new(0.55, 0.35, 0.20, 1.0), // Terre / Marron
                                    crate::mystery_box::ItemType::GrassBlock => Vec4::new(0.30, 0.75, 0.25, 1.0), // Herbe / Vert
                                    crate::mystery_box::ItemType::MetalBlock => Vec4::new(0.70, 0.75, 0.80, 1.0), // Métal / Gris acier
                                    crate::mystery_box::ItemType::IceBlock => Vec4::new(0.40, 0.85, 0.98, 1.0),   // Glace / Cyan translucent
                                    crate::mystery_box::ItemType::LavaBlock => Vec4::new(0.95, 0.35, 0.10, 1.0),  // Lave / Rouge-Orange
                                    _ => Vec4::new(0.60, 0.60, 0.60, 1.0),
                                };

                                let item_m = Mat4::from_translation(item_pos) * Mat4::from_scale(Vec3::splat(0.70 * scale_mult));
                                let push_item = PushConstants {
                                    model_matrix: item_m,
                                    color_tint: block_color,
                                    params: Vec4::new(0.3, 1.0, 0.0, 0.0),
                                };
                                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_item);
                            }
                        }

                        // 3. Petite Gemme d'Or au-dessus de l'objet sélectionné
                        if is_selected {
                            let gem_m = Mat4::from_translation(item_pos + Vec3::new(0.0, 0.65, 0.1))
                                * Mat4::from_rotation_z(trap_t * 3.0)
                                * Mat4::from_scale(Vec3::splat(0.18));
                            let push_gem = PushConstants {
                                model_matrix: gem_m,
                                color_tint: Vec4::new(0.98, 0.85, 0.15, 1.0),
                                params: Vec4::new(0.0, 8.0, 0.0, 0.0),
                            };
                            self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_gem);
                        }
                    }
                }
            }

            // ─── Rendu Phase de Placement : Aperçu Semi-Transparent Fixe sur la Grille ────
            if game.phase == crate::party_game::GamePhase::Placement {
                let (gx, gy) = (game.editor.cursor.0 as usize, game.editor.cursor.1 as usize);
                let item_pos_2d = Vec2::new(gx as f32 + 0.5, gy as f32 + 0.5);
                let t = game.round_timer;
                let dir = game.placement_dir;

                if let Some(idx) = game.mystery_box.selected_index {
                    if let Some(item) = game.mystery_box.available_items.get(idx) {
                        match item {
                            crate::mystery_box::ItemType::SawBlade =>
                                self.saw_obj.draw(&pose, vp, item_pos_2d, t * 8.0),
                            crate::mystery_box::ItemType::CannonTurret =>
                                self.cannon_obj.draw(&pose, Some(&self.cube_mesh), vp, item_pos_2d, dir, true),
                            crate::mystery_box::ItemType::SpikeTrap =>
                                self.spike_obj.draw(&pose, vp, item_pos_2d),
                            crate::mystery_box::ItemType::LaserEmitter =>
                                self.laser_obj.draw(&pose, &self.cube_mesh, vp, item_pos_2d, dir, false, 0.0, 0.0),
                            crate::mystery_box::ItemType::Flamethrower =>
                                self.flame_obj.draw(&pose, &self.cube_mesh, vp, item_pos_2d, dir, false, 0.0),
                            _ => {
                                let item_m = Mat4::from_translation(Vec3::new(gx as f32 + 0.5, gy as f32 + 0.5, 0.3))
                                    * Mat4::from_scale(Vec3::splat(0.90));
                                let push_item = PushConstants {
                                    model_matrix: item_m,
                                    color_tint: Vec4::new(0.35, 0.75, 0.95, 0.5), // Semi-transparence 50%
                                    params: Vec4::new(0.0, 2.0, 0.0, 0.0),
                                };
                                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_item);
                            }
                        }
                    }
                }
            }


            // ─── Rendu du Personnage Joueur (VISIBLE DANS TOUTES LES PHASES !) ────────
            for session in &game.players {
                let player = &session.player;
                let p_pos = player.position;
                let facing_sign = if player.facing_right { 1.0 } else { -1.0 };
                let t = game.round_timer;


                // 1. Rendu du Ragdoll de Mort 3D si le joueur est mort
                if player.state == crate::player::PlayerState::Dead && player.ragdoll.active {
                    for limb in &player.ragdoll.limbs {
                        let m = Mat4::from_translation(limb.pos)
                            * Mat4::from_rotation_z(limb.rotation.z)
                            * Mat4::from_rotation_y(limb.rotation.y)
                            * Mat4::from_rotation_x(limb.rotation.x)
                            * Mat4::from_scale(limb.scale);

                        let push = PushConstants {
                            model_matrix: m,
                            color_tint: limb.color,
                            params: Vec4::new(0.4, 0.0, 0.0, 0.0),
                        };

                        self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push);
                    }
                    continue;
                }

                // 2. Calculs de Squash & Stretch et Impact d'Atterrissage Procédural
                let dt: f32 = 0.016; // Delta time pour interpolation exponentielle ultra-douce
                let is_wall_sliding = matches!(player.state, crate::player::PlayerState::WallSliding { .. });
                let is_running = player.state == crate::player::PlayerState::OnGround && player.velocity.x.abs() > 0.3;
                let is_in_air = player.state == crate::player::PlayerState::InAir;
                let vy = player.velocity.y;
                let is_rising = is_in_air && vy > 0.1;
                let is_falling = is_in_air && vy <= 0.1;

                // Absorption d'impact dynamique à l'atterrissage (Squash Recoil proportionnel à la hauteur de chute)
                let landing_squash = if player.landing_timer > 0.0 && player.landing_duration > 0.0 {
                    let progress = (1.0 - player.landing_timer / player.landing_duration).clamp(0.0, 1.0);
                    (progress * std::f32::consts::PI).sin() * (1.0 - progress * 0.5) * player.landing_intensity
                } else {
                    0.0
                };

                let (target_scale_x, target_scale_y) = if is_wall_sliding {
                    (0.85, 1.15) // Posture tendue d'escalade
                } else if is_rising {
                    (0.82, 1.28) // Étirement athlétique à l'impulsion du saut
                } else if is_falling {
                    let fall_factor = (-vy / 22.0).clamp(0.0, 0.22);
                    (1.0 + fall_factor, 1.0 - fall_factor) // Compression progressive en chute
                } else if landing_squash > 0.005 {
                    // Grande déformation dynamique proportionnelle à la hauteur de chute (jusqu'à 50% de compression !)
                    (1.0 + landing_squash * 0.85, (1.0 - landing_squash * 0.65).max(0.48))
                } else if is_running {
                    (1.0 + 0.04 * (t * 16.0).sin(), 1.0 - 0.04 * (t * 16.0).sin())
                } else {
                    (1.0, 1.0 + 0.02 * (t * 3.5).sin()) // Respiration Choupi
                };

                let body_base_matrix = Mat4::from_translation(Vec3::new(p_pos.x, p_pos.y, 0.2))
                    * Mat4::from_rotation_z(player.tilt_angle)
                    * Mat4::from_scale(Vec3::new(target_scale_x * facing_sign, target_scale_y, 1.0));

                // Effet d'Étincelles de Frottement sur le Mur
                if let crate::player::PlayerState::WallSliding { left_wall } = player.state {
                    let wall_dir_world = if left_wall { -1.0 } else { 1.0 };
                    for i in 0..3 {
                        let spark_y = p_pos.y + 0.3 + (t * 14.0 + i as f32 * 2.1).sin() * 0.5;
                        let spark_x = p_pos.x + wall_dir_world * 0.42;
                        let spark_m = Mat4::from_translation(Vec3::new(spark_x, spark_y, 0.25))
                            * Mat4::from_scale(Vec3::new(0.08, 0.08, 0.08));
                        let push_spark = PushConstants {
                            model_matrix: spark_m,
                            color_tint: Vec4::new(0.98, 0.75, 0.20, 1.0), // Étincelles Or
                            params: Vec4::new(0.0, 6.0, 0.0, 0.0), // Lueur intense
                        };
                        self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_spark);
                    }
                }

                // 3. Pose d'Articulation Lissée (Consommation de l'animation organique de Player)
                let leg_angle_front = player.anim_leg_front;
                let leg_angle_back = player.anim_leg_back;
                let arm_angle_front = player.anim_arm_front;
                let arm_angle_back = player.anim_arm_back;

                // Jambe Avant (Z = +0.12)
                let left_leg_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.35, 0.12))
                    * Mat4::from_rotation_z(leg_angle_front)
                    * Mat4::from_translation(Vec3::new(0.0, -0.175, 0.0))
                    * Mat4::from_scale(Vec3::new(0.22, 0.35, 0.20));
                let push_leg = PushConstants {
                    model_matrix: left_leg_m,
                    color_tint: Vec4::new(0.12, 0.15, 0.28, 1.0), // Pantalon Indigo
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_leg);

                // Jambe Arrière (Z = -0.12)
                let right_leg_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.35, -0.12))
                    * Mat4::from_rotation_z(leg_angle_back)
                    * Mat4::from_translation(Vec3::new(0.0, -0.175, 0.0))
                    * Mat4::from_scale(Vec3::new(0.22, 0.35, 0.20));
                let push_leg_r = PushConstants {
                    model_matrix: right_leg_m,
                    color_tint: Vec4::new(0.10, 0.12, 0.22, 1.0), // Jambe arrière plus sombre
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_leg_r);

                // 4. Torso / Veste de Héros (Centré à y=0.65)
                let torso_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.65, 0.0))
                    * Mat4::from_scale(Vec3::new(0.48, 0.65, 0.40));
                let push_torso = PushConstants {
                    model_matrix: torso_m,
                    color_tint: Vec4::new(0.20, 0.65, 0.95, 1.0), // Veste Cyan
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_torso);

                // 5. Animation Dynamique des Bras
                // Bras Avant (Devant la veste Z = +0.23)
                let arm_front_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.88, 0.23))
                    * Mat4::from_rotation_z(arm_angle_front)
                    * Mat4::from_translation(Vec3::new(0.0, -0.18, 0.0))
                    * Mat4::from_scale(Vec3::new(0.20, 0.38, 0.18));
                let push_arm_f = PushConstants {
                    model_matrix: arm_front_m,
                    color_tint: Vec4::new(0.15, 0.18, 0.25, 1.0),
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_arm_f);

                // Bras Arrière (Derrière la veste Z = -0.23)
                let arm_back_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.88, -0.23))
                    * Mat4::from_rotation_z(arm_angle_back)
                    * Mat4::from_translation(Vec3::new(0.0, -0.18, 0.0))
                    * Mat4::from_scale(Vec3::new(0.20, 0.38, 0.18));
                let push_arm_b = PushConstants {
                    model_matrix: arm_back_m,
                    color_tint: Vec4::new(0.12, 0.14, 0.20, 1.0),
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_arm_b);

                // 6. Tête du Héros Choupi & Animation "BONK !" au Choc de Plafond
                let is_ceiling_bumping = player.ceiling_bump_timer > 0.0;
                let bump_squash = if is_ceiling_bumping {
                    let progress = (1.0 - player.ceiling_bump_timer / 0.28).clamp(0.0, 1.0);
                    (progress * std::f32::consts::PI).sin() * player.ceiling_bump_intensity
                } else {
                    0.0
                };

                let head_choupi_tilt = (t * 3.2).sin() * 0.03 - player.velocity.x * 0.02 - bump_squash * 0.40;
                let head_y = 1.22 - landing_squash * 0.12 - bump_squash * 0.18;
                let head_scale_y = 0.42 * (1.0 - bump_squash * 0.45);

                let head_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, head_y, 0.0))
                    * Mat4::from_rotation_z(head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.46 * (1.0 + bump_squash * 0.3), head_scale_y, 0.42));
                let push_head = PushConstants {
                    model_matrix: head_m,
                    color_tint: Vec4::new(0.96, 0.96, 0.98, 1.0), // Tête Blanche Pur
                    params: Vec4::new(0.1, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_head);

                // 7. Yeux / Visière Luminescente Robot Choupi (Pulsation d'Énergie Respiratoire)
                let visor_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.20, head_y + 0.01, 0.0))
                    * Mat4::from_rotation_z(head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.16, 0.10 * (1.0 - bump_squash * 0.3), 0.32));
                let visor_glow = 3.5 + 1.2 * (t * 4.0).sin();
                let push_visor = PushConstants {
                    model_matrix: visor_m,
                    color_tint: Vec4::new(0.98, 0.82, 0.10, 1.0), // Visière Or
                    params: Vec4::new(0.0, visor_glow, 0.0, 0.0), // Lueur Émissive Respiratoire
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_visor);

                // 8. Bonnet Rouge Ajusté (Posé sur la tête à y=1.45, aplati au choc du plafond)
                let trail_angle = if is_running {
                    -(player.velocity.x * 0.04)
                } else {
                    0.0
                };
                let cap_scale_y = 0.12 * (1.0 - bump_squash * 0.50);
                let cap_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(-0.02, head_y + 0.23, 0.0))
                    * Mat4::from_rotation_z(trail_angle + head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.48 * (1.0 + bump_squash * 0.35), cap_scale_y, 0.44));
                let push_cap = PushConstants {
                    model_matrix: cap_m,
                    color_tint: Vec4::new(0.92, 0.20, 0.25, 1.0), // Bonnet Rouge Vif
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_cap);

                // 9. Étoiles de Choc Pop-up "BONK !" au Plafond
                if is_ceiling_bumping {
                    for i in 0..4 {
                        let star_angle = i as f32 * std::f32::consts::PI / 2.0 + t * 14.0;
                        let star_x = p_pos.x + star_angle.cos() * (0.35 + bump_squash * 0.25);
                        let star_y = p_pos.y + 1.58 + star_angle.sin() * 0.14;
                        let star_m = Mat4::from_translation(Vec3::new(star_x, star_y, 0.3))
                            * Mat4::from_rotation_z(t * 22.0 + i as f32)
                            * Mat4::from_scale(Vec3::new(0.09, 0.09, 0.09));
                        let push_star = PushConstants {
                            model_matrix: star_m,
                            color_tint: Vec4::new(0.98, 0.90, 0.15, 1.0), // Étoile "BONK !" Or Brillant
                            params: Vec4::new(0.0, 8.0, 0.0, 0.0),
                        };
                        self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_star);
                    }
                }

                // 10. Les particules de ce joueur ne sont PLUS dessinées ici — voir le bloc
                //     « TOUTES LES PARTICULES, EN DERNIER » à la fin de la passe du monde. Elles
                //     étaient émises au milieu des objets opaques, donc sous le pipeline opaque.
            }

            // Le tableau des scores ne se dessine plus ici : il vivait dans le MONDE, posé au
            // centre de la carte, et la caméra ne s'y rendait jamais — elle reste sur le joueur
            // qui vient de mourir. Il est passé sur le calque d'écran (`leaderboard_hud`), le
            // seul endroit d'où une interface est visible sans dépendre d'où l'on est tombé.

            // ─── LE FANTÔME du solveur, quand on montre comment franchir la carte ───────
            if let Some(pos) = exterieur.demonstration {
                let base = Vec3::new(pos.x, pos.y, 0.0);
                // Volontairement translucide de ton et sans détail : ce n'est pas un joueur, et
                // il ne faut pas une seconde le confondre avec quelqu'un de la partie.
                for (hauteur, taille, teinte) in [
                    (0.45, Vec3::new(0.62, 0.90, 0.62), Vec4::new(0.55, 0.75, 0.95, 1.0)),
                    (1.12, Vec3::new(0.52, 0.46, 0.52), Vec4::new(0.80, 0.90, 1.00, 1.0)),
                ] {
                    let m = Mat4::from_translation(base + Vec3::new(0.0, hauteur, 0.0))
                        * Mat4::from_scale(taille);
                    let push = PushConstants {
                        model_matrix: m,
                        color_tint: teinte,
                        params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                    };
                    self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push);
                }
            }

            // ─── LES JOUEURS DISTANTS, tels que le cœur nous les donne ──────────────────
            //
            // Ce ne sont pas des joueurs simulés : chaque pose est arrivée par le vrai protocole
            // P2P, signée et déjà validée par le cœur. Le jeu ne peut pas en inventer un — il ne
            // fait que dessiner ce qu'on lui remet.
            //
            // Volontairement plus sobre que le personnage local (un corps, une tête, pas de
            // bonnet ni de visière) : ce qu'il faut lire d'un adversaire à distance, c'est OÙ il
            // est et QUI il est, pas le détail de sa tenue.
            for distant in exterieur.distants {
                let base = Vec3::new(distant.x, distant.y, 0.0);
                let teinte = Vec4::new(distant.r, distant.g, distant.b, 1.0);

                let corps = Mat4::from_translation(base + Vec3::new(0.0, 0.45, 0.0))
                    * Mat4::from_rotation_z(distant.yaw)
                    * Mat4::from_scale(Vec3::new(0.62, 0.90, 0.62));
                let push_corps = PushConstants {
                    model_matrix: corps,
                    color_tint: teinte,
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_corps);

                let tete = Mat4::from_translation(base + Vec3::new(0.0, 1.12, 0.0))
                    * Mat4::from_scale(Vec3::new(0.52, 0.46, 0.52));
                let push_tete = PushConstants {
                    model_matrix: tete,
                    // Un ton plus clair que le corps : la tête se détache sans changer la
                    // couleur qui identifie le joueur.
                    color_tint: Vec4::new(
                        (distant.r + 0.35).min(1.0),
                        (distant.g + 0.35).min(1.0),
                        (distant.b + 0.35).min(1.0),
                        1.0,
                    ),
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_tete);
            }

            // ═══ TOUTES LES PARTICULES, EN DERNIER ═══════════════════════════════════════════
            //
            // ## Ce qui n'allait pas jusqu'au 31 août 2026, et son verdict à l'œil
            //
            // *« les particules se fondent mal, pas vraiment transparentes avec le décor — je
            // parle principalement de celles quand on marche et qu'on fait des traces ; le feu,
            // le laser, ça ne pose pas de problème. »*
            //
            // **Elles étaient dessinées OPAQUES.** `particle_pipeline` — mélange alpha, sans
            // écriture de profondeur — existait, était correct, et n'était lié **que pour
            // l'aperçu de l'éditeur**, plus bas. Les particules, elles, étaient émises au milieu
            // des objets du monde, donc sous le pipeline opaque. *Un mécanisme branché d'un seul
            // côté, avec un commentaire qui promettait le contraire.*
            //
            // Deux autres causes tombaient au même endroit, et **chacune suffisait à annuler les
            // deux autres** : le shader forçait l'opacité à 1,0 (`party_2d5.wgsl`), et l'alpha ne
            // décroissait pas avec l'âge (`Particle::couleur_maintenant`). Réparer une seule
            // n'aurait rien produit de visible — d'où la conclusion naturelle et fausse « ce
            // n'était pas ça ».
            //
            // ## Pourquoi elles sont rassemblées ICI, et pas remises chacune à sa place
            //
            // Trois raisons qui vont dans le même sens, et c'est ce qui en fait une conception
            // plutôt qu'un déplacement :
            //
            // 1. **L'ORDRE.** Un mélange alpha n'est juste que si ce qui est derrière est déjà
            //    dessiné. Émises au milieu du monde, les particules se mélangeaient avec un décor
            //    incomplet — et le résultat dépendait de l'ordre des joueurs.
            // 2. **UN SEUL CHANGEMENT DE PIPELINE** au lieu d'un par joueur. C'est l'état le plus
            //    coûteux à changer sur un GPU à tuiles, la machine de référence du projet.
            // 3. **UNE SEULE DÉFINITION.** Les deux boucles étaient un copier-coller ; le fondu de
            //    fin aurait dû être ajouté aux deux, et une seule aurait fini par diverger.
            //
            // ⚠ **Rien d'opaque ne doit être dessiné après ce bloc** — le pipeline lié ici n'écrit
            // pas la profondeur. Le seul dessin qui suit est l'aperçu de l'éditeur, qui lie son
            // propre pipeline.
            //
            // ⚠ Les particules ne sont **pas triées** entre elles : le résultat dépend donc encore
            // de leur ordre d'émission. Pour de la poussière et des étincelles, l'œil ne le voit
            // pas. La brique qui règle ça pour de bon (`_oit_pass.rs`, transparence pondérée sans
            // tri) **dort dans le moteur** et n'est pas réveillée ici : ce serait sur-dimensionné,
            // et une brique rallumée doit être exercée dans le même commit.
            //   *Dette écrite, pas faite — `prive/aegis/BRIQUES-EN-SOMMEIL.md`.*
            context.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.particle_pipeline,
            );

            // Celles de chaque joueur (course, dérapage, glissade, impact), puis celles du JEU.
            // `PartyGame::particles` était avancé à chaque image depuis toujours et n'était
            // affiché nulle part : tout ce qu'on y émettait disparaissait en silence.
            let particules = game
                .players
                .iter()
                .flat_map(|session| session.player.particles.particles.iter())
                .chain(game.particles.particles.iter());

            for particule in particules {
                let m = Mat4::from_translation(particule.pos) * Mat4::from_scale(particule.size);
                let push = PushConstants {
                    model_matrix: m,
                    // ⚠ Et non `particule.color` : c'est ici que le fondu de fin entre dans
                    // l'image. Lire le champ brut redonnerait une particule qui disparaît d'un
                    // coup, sans que rien ne le signale.
                    color_tint: particule.couleur_maintenant(),
                    params: Vec4::new(0.0, particule.emissive, 0.0, 0.0),
                };
                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push);
            }

            context.jalon(cmd, "monde");

            // 3. Pipeline Particules / Transparence : Bloc Preview Wireframe en Mode Éditeur uniquement
            if !game.is_play_mode {
                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.particle_pipeline);

                let (cx, cy) = game.editor.cursor;
                let preview_col = game.editor.selected_block.color();
                let preview_model = Mat4::from_translation(Vec3::new(cx as f32 + 0.5, cy as f32 + 0.5, 0.3))
                    * Mat4::from_scale(Vec3::new(1.08, 1.08, 1.08));

                let push_preview = PushConstants {
                    model_matrix: preview_model,
                    color_tint: preview_col,
                    params: Vec4::new(0.0, 5.0, 0.0, 0.0), // Lueur Émissive Wireframe Preview
                };

                self.instances.dessiner_avec(&context.device, cmd, &self.cube_mesh, &push_preview);
            }

            context.jalon(cmd, "apercu");

            // ═══ FIN DE LA SCÈNE, DÉBUT DE L'ÉCRAN ═══════════════════════════════════════════
            //
            // Tout ce qui précède a écrit de la LUMIÈRE, dans une image HDR. Rien n'a encore été
            // porté à l'échelle de l'écran, et c'est ce qui rend le halo possible : la valeur
            // d'un pixel dit encore combien il est lumineux, pas seulement combien il est clair.
            context.device.cmd_end_rendering(cmd);

            // ⭐ L'OCCLUSION AMBIANTE : ce que le ciel ne voit pas, retiré de l'ambiante SEULE.
            //
            // ⚠ Elle vient AVANT le halo, et l'ordre compte : une zone occluse ne doit pas
            // déborder du blanc à cause d'une lumière qu'elle ne reçoit pas. Le halo doit donc
            // voir la scène déjà corrigée.
            //
            // La profondeur et l'ambiante réduites passent d'abord en lecture — elles sortent de
            // la passe en disposition d'attachement.
            let en_lecture = |image, aspect, vers| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(if aspect == vk::ImageAspectFlags::DEPTH {
                        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                    } else {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    })
                    .new_layout(vers)
                    .src_access_mask(if aspect == vk::ImageAspectFlags::DEPTH {
                        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    } else {
                        vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    })
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
            };
            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[en_lecture(
                    self.cibles.image_profondeur_lisible(),
                    vk::ImageAspectFlags::DEPTH,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                )],
            );

            self.occlusion.appliquer(
                &context.device,
                cmd,
                &self.cibles,
                context.swapchain_extent,
            );

            context.jalon(cmd, "occlusion");

            // ⚠ DEUX barrières, et aucune n'est facultative :
            //  • la scène passe d'attachement à texture — sans cette barrière, la composition
            //    lirait des pixels que la carte n'a pas fini d'écrire ;
            //  • l'image de la fenêtre entre en attachement pour la première fois de l'image.
            let scene_en_lecture = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(self.cibles.image_resolue())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let fenetre_en_attachement = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[scene_en_lecture, fenetre_en_attachement],
            );


            // ⭐ LE HALO : ce que l'écran ne peut pas montrer, rendu visible autour de sa source.
            //
            // Il s'ajoute à la scène AVANT la courbe de tonalité, et c'est la seule place possible :
            // après elle, il n'y aurait plus de différence entre un mur blanc et une lampe. Toute
            // la conception — pourquoi il n'a ni seuil, ni intensité, ni rayon à régler — vit en
            // tête de `halo.wgsl`.
            self.ecran.diffuser(
                &context.device,
                cmd,
                &self.cibles,
                context.swapchain_extent,
            );

            context.jalon(cmd, "halo");

            // ⚠ Aucune profondeur ici, et c'est voulu : la composition couvre l'écran entier et
            // le HUD se trie lui-même. Une image de profondeur pleine résolution en moins.
            let attache_ecran = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                // `DONT_CARE` : la composition recouvre chaque pixel. Demander à la carte de
                // charger l'ancien contenu serait lire une image entière pour l'écraser.
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE);
            context.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: context.swapchain_extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache_ecran)),
            );
            context.device.cmd_set_viewport(cmd, 0, &[viewport]);
            context.device.cmd_set_scissor(cmd, 0, &[scissor]);

            // La courbe de tonalité, une seule fois, pour toute l'image. Le cadre porte
            // l'exposition et le point blanc : sans lui, la composition n'aurait aucune courbe.
            self.cadre.lier(&context.device, cmd, self.ecran.layout_pipeline());
            self.ecran.composer(&context.device, cmd);

            context.jalon(cmd, "composition");

            // ─── LE HUD : ce qui se dessine sur l'ÉCRAN, et non dans le monde ────────────────
            //
            // ⚠ Il est ICI, après la courbe de tonalité, et c'est la seule place juste : une
            // interface ne reçoit aucune lumière et ne doit subir aucune exposition. Passée dans
            // la scène, un texte demandé en blanc pur sortirait gris à 62 %.
            //
            // Le tampon d'instances est relié : c'est le même que celui de la scène, avec les
            // mêmes objets dedans — seuls les indices émis ici diffèrent.
            self.instances.lier(&context.device, cmd);

            // Tout ce qui suit ignore la caméra. C'est le point entier : le tableau des scores
            // était jusqu'ici posé au centre de la CARTE, pendant que la caméra restait sur le
            // joueur qui venait de mourir — il ne s'affichait donc pour personne.
            if game.is_play_mode {
                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.hud_pipeline);
                // ⚠ Le pinceau est NOMME pour pouvoir etre referme. Chaque lettre du HUD est un
                // quad ; les emettre un par un, c'etait des centaines d'appels de dessin pour
                // afficher un score. `terminer` les envoie tous en un seul.
                let pinceau = crate::hud::Pinceau {
                    lot: std::cell::RefCell::new(Vec::new()),
                    device: &context.device,
                    cmd,
                    layout: self.ecran.layout_pipeline(),
                    cube: &self.cube_mesh,
                    aspect,
                };
                crate::hud::dessiner(
                    &pinceau,
                    game,
                    exterieur.pont,
                    exterieur.carte,
                    exterieur.bouchon,
                    exterieur.vote,
                    exterieur.demonstration.is_some(),
                );
                pinceau.terminer(&self.instances);
            }
            // LE LOBBY EN DERNIER, et hors du `is_play_mode` : on doit pouvoir choisir sa partie
            // même quand aucune n'a commencé — c'est justement le moment où l'on en a besoin.
            if exterieur.lobby.ouvert() {
                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.hud_pipeline);
                let pinceau = crate::hud::Pinceau {
                    lot: std::cell::RefCell::new(Vec::new()),
                    device: &context.device,
                    cmd,
                    layout: self.ecran.layout_pipeline(),
                    cube: &self.cube_mesh,
                    aspect,
                };
                exterieur.lobby.dessiner(&pinceau);
                pinceau.terminer(&self.instances);
            }

            context.jalon(cmd, "interface");

            context.device.cmd_end_rendering(cmd);

            // Transition Swapchain -> PRESENT_SRC
            let barrier_present_back = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::NONE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            context.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, vk::PipelineStageFlags::BOTTOM_OF_PIPE, vk::DependencyFlags::empty(), &[], &[], &[barrier_present_back]);

            context.jalon(cmd, "presentation");
        }
    }

    /// Règle un champ de l'ambiance, en direct.
    ///
    /// ⚠ Rend l'erreur du moteur telle quelle plutôt que de la traduire : c'est lui qui connaît
    /// les champs et leurs bornes, et une reformulation ici serait une seconde vérité à tenir.
    pub fn regler_ambiance(&mut self, champ: &str, valeurs: &[f32]) -> Result<(), String> {
        self.ambiance.regler(champ, valeurs)
    }

    /// Allume ou éteint le halo. Rend son nouvel état, pour que l'appelant journalise ce qui EST
    /// plutôt que ce qu'il a demandé.
    pub fn regler_halo(&mut self, allume: bool) -> bool {
        self.ecran.allumer(allume);
        self.ecran.halo_allume()
    }

    /// Allume ou éteint l'occlusion ambiante. Rend son nouvel état.
    pub fn regler_occlusion(&mut self, allume: bool) -> bool {
        self.occlusion.allumer(allume);
        self.occlusion.active()
    }

    /// L'ambiance courante, écrite telle qu'elle se recolle dans ce fichier.
    pub fn ambiance_decrite(&self) -> String {
        self.ambiance.decrire()
    }

    /// Refait les cibles quand la fenetre change de taille.
    ///
    /// ⚠ Quarante lignes d'allocation Vulkan vivaient ici, copie conforme de celles de
    /// l'ouverture. *Deux textes identiques a maintenir, c'est un texte qui finira par etre le
    /// seul corrige.* Elles vivent maintenant dans le moteur, en un seul exemplaire.
    pub fn recreate_framebuffer_resources(
        &mut self,
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) {
        // L'attente est ce qui rend la destruction sure : une image encore lue par une image en
        // vol se detruit sans erreur immediate, et le defaut apparait ailleurs, plus tard.
        unsafe {
            let _ = gpu.device.device_wait_idle();
        }
        if let Err(e) = self.cibles.recreer(gpu, memory_props) {
            log::error!("cibles de rendu non recreees apres redimensionnement : {e}");
            // ⚠ On ne rebranche PAS sur des cibles qu'on vient d'echouer a refaire : le
            // descripteur pointerait alors sur une vue detruite, ce que la carte n'est pas tenue
            // de signaler. Mieux vaut garder l'ancien pointage, faux mais valide, et le dire.
            return;
        }
        // ⚠⚠ SANS CETTE LIGNE, LE DESCRIPTEUR RESTE SUR L'IMAGE DETRUITE. Aucune erreur ne serait
        // levee : selon le pilote, l'ecran resterait fige sur la derniere image d'avant, ou
        // deviendrait noir, ou afficherait n'importe quoi — et seulement APRES un redimensionnement,
        // c'est-a-dire dans le cas qu'on teste le moins.
        //
        // Les octaves du halo suivent la meme taille : elles sont refaites en meme temps.
        if let Err(e) = self.ecran.redimensionner(gpu, memory_props, &self.cibles) {
            log::error!("chaine de l'ecran non recreee apres redimensionnement : {e}");
        }
        // ⚠ L'occlusion pointe sur la profondeur et l'ambiante, qui viennent d'etre refaites.
        self.occlusion.brancher(&gpu.device, &self.cibles, self.ecran.layout_descripteur());
    }
}
