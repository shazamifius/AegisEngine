//! **LA PASSE QUI FAIT ENTRER UNE VRAIE GÉOMÉTRIE DANS LES DEUX CARTES.**
//!
//! C'est le chaînon qui manquait entre un maillage et la physique de la matière. Jusqu'au
//! 3 septembre 2026, `render::verre` était exercé par ses seuls tests, et les cartes qu'il lisait
//! étaient calculées analytiquement — une sphère, écrite à la main, dans le test. *Le verre savait
//! réfracter, mais rien ne pouvait lui donner un objet.*
//!
//! ## Deux passes, et une seule différence entre elles
//!
//! | | ce qu'elle capture |
//! |---|---|
//! | [`Faces::Entree`] | la face par où le rayon **entre** — faces avant, la plus proche |
//! | [`Faces::Sortie`] | la face par où il **ressort** — faces arrière, la plus lointaine |
//!
//! Le shader est le **même**, le maillage est le **même**, seul le réglage `Faces` change. C'est
//! toute la conception : deux cartes ne sont pas deux algorithmes, ce sont deux points de vue sur
//! la même surface. *Le jour où elles divergeraient en code, l'une des deux ne serait plus testée.*
//!
//! ## ⚠ La chose à ne pas croire : ce n'est pas de la transparence
//!
//! Cette passe ne trie rien, ne mélange rien, ne suppose **qu'une seule couche de matière par
//! pixel** — une face avant, une face arrière. Un objet creux, ou deux verres l'un derrière
//! l'autre, demanderaient plus de deux cartes. *C'est une limite du modèle, pas un défaut
//! d'implémentation, et elle était déjà écrite avant que cette passe existe.*

use crate::core::gpu_context::GpuContext;
use crate::geometry::gpu_mesh::GpuMesh;
use crate::render::cadre::Cadre;
use crate::render::pipeline::{Faces, Melange, PipelineFactory, Reglages};
use ash::vk;

/// Le format des deux cartes.
///
/// ⭐ **Trente-deux bits par canal, et c'est un choix d'INSTRUMENT assumé, pas une décision
/// d'architecture.** Le commentaire qui le justifie vit dans `verre.rs` : *« sur seize, la précision
/// des distances entrerait dans la mesure et on ne saurait plus ce qu'on mesure »*. C'est un
/// argument de banc.
///
/// ⚠ **Il ne le restera pas.** Sur un GPU mobile la bande passante est la ressource rare — 87 octets
/// par pixel pour tout — et deux cartes à 16 octets en consomment 32 à l'écriture, autant à la
/// lecture. Le format de production se **dérivera** de la précision réellement nécessaire à une
/// normale et à une distance, il ne se recopiera pas depuis l'instrument. *La constante est ici,
/// seule et nommée, pour que ce jour-là il n'y ait qu'un endroit à changer.*
///
/// *Vérifié dans la spécification Vulkan et non de seconde main : `COLOR_ATTACHMENT` est
/// **obligatoire** pour ce format. En revanche `SAMPLED_IMAGE_FILTER_LINEAR` ne l'est pas — ce qui
/// tombe bien, `refraction.wgsl` lit ses cartes au `textureLoad`, sans échantillonneur.*
pub const FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;

/// Le format de profondeur qui départage les faces d'une même carte.
pub const FORMAT_PROFONDEUR: vk::Format = vk::Format::D32_SFLOAT;

/// Ce qu'on dessine dans une carte : un maillage, ses instances, et où il se trouve.
///
/// *Ces trois-là voyagent toujours ensemble et n'ont aucun sens séparément — les tenir groupés
/// évite une liste d'arguments qui s'allonge, et un ordre d'appel à retenir.*
pub struct Objet<'a> {
    pub maillage: &'a GpuMesh,
    pub instances: &'a crate::render::instances::Instances,
    pub modele: crate::core::math::Mat4,
}

/// La passe des deux cartes. Un shader, deux pipelines, aucune autre différence.
pub struct Cartes {
    entree: vk::Pipeline,
    sortie: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl Cartes {
    pub fn nouvelle(
        gpu: &GpuContext,
        layout_cadre: vk::DescriptorSetLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let layout = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            std::slice::from_ref(&layout_cadre),
            &[],
        )?;

        let module = PipelineFactory::create_shader_module_from_bytes(
            &gpu.device,
            crate::shaders::CARTES_SPV,
        )?;

        // ⚠ Les deux pipelines partagent TOUT sauf `faces`. Écrire deux littéraux complets aurait
        // laissé la porte ouverte à une divergence silencieuse — un format ici, un échantillonnage
        // là — dont l'effet serait une carte juste et l'autre subtilement fausse.
        let reglages = |faces| Reglages {
            color_format: FORMAT,
            second_format: None,
            depth_format: Some(FORMAT_PROFONDEUR),
            depth_write: true,
            melange: Melange::Aucun,
            use_vertex_input: true,
            faces,
            echantillons: vk::SampleCountFlags::TYPE_1,
        };

        let entree = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            layout,
            module,
            module,
            reglages(Faces::Entree),
        )?;
        let sortie = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            layout,
            module,
            module,
            reglages(Faces::Sortie),
        )?;

        unsafe { gpu.device.destroy_shader_module(module, None) };
        Ok(Self { entree, sortie, layout })
    }

    /// Dessine le maillage dans la carte de la face demandée.
    ///
    /// ⚠ **`cmd` doit être à l'intérieur d'un `cmd_begin_rendering`** dont les attachements sont la
    /// carte et sa profondeur — c'est l'appelant qui ouvre le rendu, comme pour `verre::dessiner`.
    /// *Le nettoyage de la profondeur doit valoir [`Faces::profondeur_initiale`] : sinon le premier
    /// fragment ne gagne jamais, et la carte reste vide sans qu'aucune erreur ne soit levée.*
    pub fn dessiner(
        &self,
        gpu: &GpuContext,
        cmd: vk::CommandBuffer,
        face: Faces,
        cadre: &Cadre,
        objet: &Objet<'_>,
    ) {
        let pipeline = match face {
            Faces::Sortie => self.sortie,
            // `Toutes` n'a pas de sens ici — une carte capture une face, ou elle ne veut rien dire.
            // On rend l'entrée plutôt que de paniquer : une carte fausse se voit, un plantage dans
            // une passe de rendu ne dit rien de plus et coûte l'image.
            _ => self.entree,
        };
        unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            cadre.lier(&gpu.device, cmd, self.layout);
        }
        objet.instances.lier(&gpu.device, cmd);
        // ⚠ Teinte et paramètres sont à zéro, et c'est le sujet : ce shader n'en lit AUCUN.
        // Une carte porte une géométrie, jamais une apparence — c'est la frontière du 29 août.
        objet.instances.dessiner_un(
            &gpu.device,
            cmd,
            objet.maillage,
            objet.modele,
            crate::core::math::Vec4::new(0.0, 0.0, 0.0, 0.0),
            crate::core::math::Vec4::new(0.0, 0.0, 0.0, 0.0),
        );
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.entree, None);
            device.destroy_pipeline(self.sortie, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::math::{Mat4, Vec3};
    use crate::geometry::primitives::Primitives;
    use crate::render::cadre::DonneesImage;

    // ── LE BANC, et ses constantes sont CELLES DE `verre.rs`, délibérément ──────────────────
    //
    // Une bille de rayon 1 à l'origine, l'œil à 4 unités en arrière, un demi-champ dont la
    // tangente vaut 0,57735 (soit 30°). *Les mêmes que le banc du verre : c'est ce qui permettra
    // de comparer directement l'écart d'une carte RASTÉRISÉE à celui d'une carte EXACTE, déjà
    // mesuré à 2,132° en 256². Changer un seul de ces chiffres rendrait les deux mesures
    // incomparables sans que rien ne le signale.*
    const RAYON: f32 = 1.0;
    const RECUL: f32 = 4.0;
    const TANGENTE: f32 = 0.57735;

    /// La direction du rayon d'un pixel, **écrite à la main plutôt que dérivée de la caméra** —
    /// c'est ce qui en fait une vérité indépendante et non un second exemplaire du même calcul.
    ///
    /// ## ⚠⚠ LE SIGNE DE `sx` EST INVERSE DE CELUI DU BANC DE `verre.rs`, ET CE N'EST PAS UN CHOIX
    ///
    /// Le banc du verre pose sa caméra à la main : l'œil regarde vers **+Z** et son axe droit est
    /// **+X**. C'est un repère **main gauche**. Le moteur, lui, construit sa caméra avec
    /// `perspective_rh` et un `look_at` : en main **droite**, un œil qui regarde vers +Z a son axe
    /// droit en **−X**. *Les deux conventions sont cohérentes chacune de son côté ; elles ne sont
    /// simplement pas la même.*
    ///
    /// **Une bille ne pouvait pas révéler ce désaccord** : sa silhouette et ses distances sont
    /// rigoureusement identiques dans un miroir horizontal. Seules les normales diffèrent — et
    /// l'écart valait **44° constant**, indifférent à la finesse du maillage. *Un écart qui ne
    /// bouge pas quand on subdivise n'est jamais une erreur de maillage : c'est un repère.*
    ///
    /// ⭐ **Et ça vaut bien plus que ce test.** Le jour où `verre` lira ces cartes rastérisées pour
    /// de vrai, ses constantes poussées `droite` / `haut` / `avant` devront décrire **la caméra du
    /// moteur**, pas celle de son banc. Le même désaccord y produirait une image plausible et
    /// fausse — un verre qui dévie la lumière du mauvais côté, sans qu'aucun test ne tombe.
    fn direction_du_pixel(x: u32, y: u32, cote: u32) -> Vec3 {
        let sx = ((x as f32 + 0.5) / cote as f32) * 2.0 - 1.0;
        let sy = 1.0 - ((y as f32 + 0.5) / cote as f32) * 2.0;
        Vec3::new(-sx * TANGENTE, sy * TANGENTE, 1.0).normalize_or_zero()
    }

    /// ⭐ **LA VÉRITÉ** — où le rayon entre et ressort d'une bille, analytiquement.
    ///
    /// Renvoie `(t_entrée, t_sortie)`, ou `None` si le rayon manque la bille. *On ne compare jamais
    /// la rastérisation à une autre implémentation : deux calculs issus du même raisonnement
    /// peuvent être faux de la même façon, et leur accord ne prouverait que leur parenté.*
    fn verite(direction: Vec3) -> Option<(f32, f32)> {
        let origine = Vec3::new(0.0, 0.0, -RECUL);
        let b = origine.dot(direction);
        let c = origine.dot(origine) - RAYON * RAYON;
        let disc = b * b - c;
        if disc < 0.0 {
            return None;
        }
        let r = disc.sqrt();
        Some((-b - r, -b + r))
    }

    /// Ce qu'une carte rastérisée a coûté d'écart à la vérité.
    struct Ecart {
        /// Écart moyen de DISTANCE, en unités de monde.
        distance: f32,
        /// Écart moyen d'ANGLE de la normale, en degrés.
        angle: f32,
        /// Pixels où la carte ET la vérité voient de la matière.
        compte: usize,
        /// Pixels où l'une voit de la matière et l'autre non — la silhouette.
        desaccords: usize,
    }

    /// Rend les deux cartes d'une bille MAILLÉE, et rapporte leurs octets bruts.
    ///
    /// `None` si aucun Vulkan n'est joignable : sur une machine sans carte, ce test s'abstient au
    /// lieu d'échouer — il ne mesure alors rien, et il le dit.
    fn rendre_les_cartes(cote: u32, subdivisions: u32) -> Option<(Vec<u8>, Vec<u8>)> {
        let ctx = match GpuContext::sans_ecran_format(cote, cote, 1, FORMAT) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠ aucun Vulkan joignable : {e}");
                return None;
            }
        };
        // ⚠⚠ À PARTIR D'ICI, PLUS RIEN N'A LE DROIT D'ÊTRE AVALÉ.
        //
        // La première version de ce test posait `.ok()?` partout : chaque échec renvoyait `None`,
        // le test sortait par son `else { return }` et **passait sans avoir rien mesuré**. Il est
        // resté vert deux fois de suite en ne dessinant rien du tout.
        //
        // *Seule l'absence de Vulkan est une abstention légitime — et elle vient d'être écartée,
        // au-dessus, en le disant. Tout le reste est un défaut, et un défaut se voit.*
        let memory_props =
            unsafe { ctx.instance.get_physical_device_memory_properties(ctx.physical_device) };

        // ── La bille, en triangles ──
        let (sommets, indices) =
            Primitives::create_uv_sphere(RAYON, subdivisions, subdivisions * 2);
        let maillage = GpuMesh::upload(&ctx, &memory_props, &sommets, &indices)
            .expect("le maillage de la bille n'a pas pu monter sur la carte");

        // ── Le cadre : la caméra du banc, et rien d'autre ──
        //
        // ⚠ La projection doit rendre EXACTEMENT la `direction_du_pixel` ci-dessus, sans quoi on
        // mesurerait un désaccord de conventions plutôt qu'un coût de rastérisation. Le champ
        // vertical vaut donc `2·atan(TANGENTE)`, et l'image est carrée.
        let mut cadre = crate::render::cadre::Cadre::nouveau(&ctx, &memory_props)
            .expect("le cadre uniforme n'a pas pu etre cree");
        let oeil = Vec3::new(0.0, 0.0, -RECUL);
        let mut camera = crate::scene::camera::Camera::new(oeil, Vec3::new(0.0, 0.0, 0.0), 1.0);
        camera.fov_y_radians = 2.0 * TANGENTE.atan();
        let view_proj = camera.compute_projection_matrix() * camera.compute_view_matrix();
        // ⚠ L'ambiante et les lumières sont sans objet ici : ce shader ne lit que `view_proj` et
        // la position de l'œil. On passe les valeurs par défaut plutôt qu'inventer une scène — si
        // un jour ce shader lisait une couleur, la garde des couleurs tomberait avant ce test.
        let donnees = DonneesImage::nouvelle(
            view_proj,
            Mat4::IDENTITY,
            [oeil.x, oeil.y, oeil.z],
            crate::render::cadre::Ambiance::default(),
            &[],
        );
        cadre.ecrire(&donnees);

        // ── Une seule instance, à l'identité : la bille est déjà à l'origine ──
        let instances =
            crate::render::instances::Instances::nouveau(&ctx, &memory_props, 1)
                .expect("le tampon d'instances n'a pas pu etre cree");

        let passe = Cartes::nouvelle(&ctx, cadre.layout_descripteur)
            .expect("les deux pipelines de cartes n'ont pas pu etre crees");
        let profondeur = image_de_profondeur(&ctx, &memory_props, cote)
            .expect("l'image de profondeur n'a pas pu etre creee");

        let mut sorties = Vec::new();
        for face in [Faces::Entree, Faces::Sortie] {
            instances.recommencer();
            let octets = rendre_une_carte(
                &ctx, &passe, face, &cadre, &maillage, &instances, profondeur.1, cote,
            )
            .unwrap_or_else(|| panic!("le rendu de la carte {face:?} a echoue"));
            sorties.push(octets);
        }

        unsafe {
            ctx.device.device_wait_idle().ok();
            ctx.device.destroy_image_view(profondeur.1, None);
            ctx.device.destroy_image(profondeur.0, None);
            ctx.device.free_memory(profondeur.2, None);
        }
        passe.detruire(&ctx.device);
        cadre.detruire(&ctx.device);

        let arriere = sorties.pop().expect("carte de sortie manquante");
        let avant = sorties.pop().expect("carte d'entree manquante");
        Some((avant, arriere))
    }

    /// Ouvre le rendu, dessine la face demandée dans la carte, et rapporte ses octets bruts.
    ///
    /// ⚠ **Le nettoyage de la profondeur vient de [`Faces::profondeur_initiale`]**, jamais d'une
    /// constante écrite ici : pour la face de sortie, la comparaison est `GREATER_OR_EQUAL` et un
    /// nettoyage à 1,0 ferait perdre *tous* les fragments. La carte serait vide, sans qu'aucune
    /// erreur ne soit levée — le genre de défaut qui se diagnostique une soirée.
    #[allow(clippy::too_many_arguments)]
    fn rendre_une_carte(
        ctx: &GpuContext,
        passe: &Cartes,
        face: Faces,
        cadre: &crate::render::cadre::Cadre,
        maillage: &GpuMesh,
        instances: &crate::render::instances::Instances,
        vue_profondeur: vk::ImageView,
        cote: u32,
    ) -> Option<Vec<u8>> {
        let image = ctx.swapchain_images[0];
        let vue = ctx.swapchain_image_views[0];
        let etendue = vk::Extent2D { width: cote, height: cote };
        let plage = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let cmd = ctx.begin_single_time_commands().expect("ouverture du tampon de commandes");
        unsafe {
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::NONE)
                    .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .image(image)
                    .subresource_range(plage)],
            );

            // ⭐ `w = 0` au nettoyage : c'est ainsi que `refraction.wgsl` reconnaît « pas de
            // matière ici » (`w <= 0`). La convention n'est écrite ni ici ni là-bas seulement —
            // elle est la même des deux côtés parce qu'un seul chiffre la porte.
            let couleur = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0; 4] },
                });
            let profondeur = vk::RenderingAttachmentInfo::default()
                .image_view(vue_profondeur)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: face.profondeur_initiale(),
                        stencil: 0,
                    },
                });

            ctx.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&couleur))
                    .depth_attachment(&profondeur),
            );
            ctx.device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: cote as f32,
                    height: cote as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            ctx.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue }],
            );
        }

        passe.dessiner(
            ctx,
            cmd,
            face,
            cadre,
            &Objet { maillage, instances, modele: Mat4::IDENTITY },
        );
        unsafe { ctx.device.cmd_end_rendering(cmd) };
        ctx.end_single_time_commands(cmd).expect("soumission du tampon de commandes");

        Some(
            ctx.relire_image_brute(image, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, etendue, FORMAT)
                .expect("relecture de la carte"),
        )
    }

    /// Une image de carte : cible de rendu **et** lisible par un shader ensuite.
    fn image_de_carte(
        ctx: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cote: u32,
    ) -> Option<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
        image(
            ctx,
            memory_props,
            cote,
            FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
        )
    }

    /// L'image de profondeur qui départage les faces d'une même carte.
    fn image_de_profondeur(
        ctx: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cote: u32,
    ) -> Option<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
        image(
            ctx,
            memory_props,
            cote,
            FORMAT_PROFONDEUR,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )
    }

    /// Une image carrée, son allocation et sa vue — **une seule fois, pour les deux usages**.
    fn image(
        ctx: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cote: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Option<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
        let image = unsafe {
            ctx.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D { width: cote, height: cote, depth: 1 })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
        }
        .ok()?;
        let besoins = unsafe { ctx.device.get_image_memory_requirements(image) };
        let type_memoire = crate::core::memory::MemoryManager::find_memory_type(
            memory_props,
            besoins.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let memoire = unsafe {
            ctx.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(besoins.size)
                    .memory_type_index(type_memoire),
                None,
            )
        }
        .ok()?;
        unsafe { ctx.device.bind_image_memory(image, memoire, 0) }.ok()?;
        let vue = unsafe {
            ctx.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )
        }
        .ok()?;
        Some((image, vue, memoire))
    }

    /// Lit un pixel d'une carte : `(normale, distance)`. `w <= 0` = pas de matière.
    fn pixel(octets: &[u8], x: u32, y: u32, cote: u32) -> (Vec3, f32) {
        let base = ((y * cote + x) * 16) as usize;
        let f = |n: usize| {
            f32::from_le_bytes([
                octets[base + n * 4],
                octets[base + n * 4 + 1],
                octets[base + n * 4 + 2],
                octets[base + n * 4 + 3],
            ])
        };
        (Vec3::new(f(0), f(1), f(2)), f(3))
    }

    /// ⭐⭐ **LA SONDE QUI TRANCHE LE SENS DU CULLING — et rien d'autre ne pouvait le trancher.**
    ///
    /// L'orientation apparente d'un triangle dépend de la convention de la projection, et sur ce
    /// moteur `naga` **retourne l'axe Y** à la compilation des shaders. *C'est ce qui a fait sortir
    /// le HUD à l'envers sous onze tests verts le 19 août 2026, et rien dans le code ne le disait.*
    ///
    /// Un raisonnement sur `FrontFace::COUNTER_CLOCKWISE` aurait donc valu ce que valait la
    /// convention supposée. La mesure, elle, est sans ambiguïté : **par où le rayon entre est plus
    /// près de l'œil que par où il ressort**, pour tout pixel d'un objet fermé. Si les deux
    /// pipelines capturaient la mauvaise face, ce test s'inverserait d'un bloc.
    #[test]
    fn l_entree_est_toujours_plus_proche_que_la_sortie() {
        let cote = 128;
        let Some((avant, arriere)) = rendre_les_cartes(cote, 64) else { return };

        let mut compares = 0usize;
        let mut fautifs = 0usize;
        for y in 0..cote {
            for x in 0..cote {
                let (_, t_avant) = pixel(&avant, x, y, cote);
                let (_, t_arriere) = pixel(&arriere, x, y, cote);
                if t_avant <= 0.0 || t_arriere <= 0.0 {
                    continue;
                }
                compares += 1;
                if t_avant > t_arriere {
                    fautifs += 1;
                }
            }
        }

        // ⚠ Garde anti-test-creux : sans elle, « aucun fautif » serait vrai parce que rien n'a
        // été dessiné. *Une absence n'est jamais une preuve tant que l'instrument n'a pas montré
        // qu'il sait produire une présence.*
        assert!(
            compares > 1000,
            "seulement {compares} pixels portent de la matiere dans les DEUX cartes : \
             la bille n'a pas ete rasterisee, ce test ne mesure rien"
        );
        assert_eq!(
            fautifs, 0,
            "{fautifs} pixels sur {compares} ont leur face d'ENTREE plus loin que leur face de \
             SORTIE. Les deux pipelines capturent la mauvaise face : le sens du culling est \
             inverse (voir `Faces`, et le piege de l'axe Y retourne par naga)."
        );
    }

    /// Compare une carte rastérisée à la vérité analytique de la bille.
    fn confronter(octets: &[u8], cote: u32, sortie: bool) -> Ecart {
        let (mut distance, mut angle) = (0.0f64, 0.0f64);
        let (mut compte, mut desaccords) = (0usize, 0usize);

        for y in 0..cote {
            for x in 0..cote {
                let direction = direction_du_pixel(x, y, cote);
                let (normale_lue, t_lu) = pixel(octets, x, y, cote);
                let attendu = verite(direction);

                match (attendu, t_lu > 0.0) {
                    (None, false) => {}
                    (Some(_), false) | (None, true) => desaccords += 1,
                    (Some((t0, t1)), true) => {
                        let t = if sortie { t1 } else { t0 };
                        let point = Vec3::new(0.0, 0.0, -RECUL) + direction * t;
                        let normale_vraie = point * (1.0 / RAYON);

                        distance += (t_lu - t).abs() as f64;
                        let cos = normale_lue
                            .normalize_or_zero()
                            .dot(normale_vraie)
                            .clamp(-1.0, 1.0);
                        angle += cos.acos().to_degrees() as f64;
                        compte += 1;
                    }
                }
            }
        }

        Ecart {
            distance: (distance / compte.max(1) as f64) as f32,
            angle: (angle / compte.max(1) as f64) as f32,
            compte,
            desaccords,
        }
    }

    /// ⚠⚠ **LA SONDE QUI TESTE UN RETOURNEMENT DE L'IMAGE — et une sphère ne peut pas le voir.**
    ///
    /// L'écart d'angle des normales restait bloqué à ~44° quel que soit le maillage, pendant que
    /// la distance convergeait parfaitement. *Un écart qui ne bouge pas quand on subdivise n'est
    /// pas une erreur de maillage : c'est une erreur de repère.*
    ///
    /// Et une bille centrée est **symétrique par retournement vertical** : sa silhouette et ses
    /// distances sont rigoureusement identiques à l'endroit et à l'envers. **Seules les normales
    /// changent.** C'est exactement le piège du HUD sorti à l'envers sous onze tests verts — un
    /// objet trop symétrique ne peut pas témoigner de son propre retournement.
    ///
    /// Cette sonde compare donc chaque normale à la vérité du pixel **miroir**. Si l'écart
    /// s'effondre, l'image est retournée ; s'il reste, la cause est ailleurs.
    #[test]
    fn diagnostic_l_image_est_elle_retournee_verticalement() {
        let cote = 128;
        let Some((avant, _)) = rendre_les_cartes(cote, 64) else { return };

        let mesurer = |(mx, my): (bool, bool)| {
            let (mut angle, mut compte) = (0.0f64, 0usize);
            for y in 0..cote {
                for x in 0..cote {
                    let (normale_lue, t_lu) = pixel(&avant, x, y, cote);
                    if t_lu <= 0.0 {
                        continue;
                    }
                    let xv = if mx { cote - 1 - x } else { x };
                    let yv = if my { cote - 1 - y } else { y };
                    let direction = direction_du_pixel(xv, yv, cote);
                    let Some((t0, _)) = verite(direction) else { continue };
                    let point = Vec3::new(0.0, 0.0, -RECUL) + direction * t0;
                    let cos = normale_lue
                        .normalize_or_zero()
                        .dot(point * (1.0 / RAYON))
                        .clamp(-1.0, 1.0);
                    angle += cos.acos().to_degrees() as f64;
                    compte += 1;
                }
            }
            angle / compte.max(1) as f64
        };

        // Les quatre orientations possibles. *Une bille est symétrique dans les deux axes : ni sa
        // silhouette ni ses distances ne peuvent départager. Seules les normales le peuvent.*
        let essais = [
            ("a l'endroit    ", (false, false)),
            ("miroir X       ", (true, false)),
            ("miroir Y       ", (false, true)),
            ("miroir X et Y  ", (true, true)),
        ];
        let mut resultats: Vec<(&str, f64)> =
            essais.iter().map(|(nom, m)| (*nom, mesurer(*m))).collect();
        for (nom, e) in &resultats {
            println!("  {nom} : {e:.3}°");
        }
        resultats.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let (meilleur, valeur) = resultats[0];
        assert!(
            meilleur.trim() == "a l'endroit",
            "les normales collent MIEUX en « {} » ({valeur:.3}°) qu'a l'endroit : \
             l'image produite ne suit pas la convention du banc",
            meilleur.trim()
        );
    }

    /// ⭐⭐ **CE QUE LA RASTÉRISATION COÛTE, ET IL SE DÉMONTRE AU LIEU DE SE DÉCRÉTER.**
    ///
    /// Une bille maillée n'est pas une bille : c'est un polyèdre. L'écart à la sphère vraie est
    /// donc **réel et attendu** — ce qui doit être vérifié n'est pas un chiffre, c'est qu'il
    /// **rétrécit quand on subdivise**. *Un seuil absolu se périmerait à la première carte
    /// graphique différente ; une tendance, non.*
    ///
    /// C'est exactement la méthode qui a démontré, le 2 septembre, que le surcoût des cartes
    /// venait bien de la discrétisation en pixels et non d'une faute d'indexation.
    #[test]
    fn l_ecart_a_la_bille_vraie_retrecit_quand_on_subdivise() {
        let cote = 128;
        let mut mesures = Vec::new();
        for subdivisions in [8u32, 16, 32, 64] {
            let Some((avant, _)) = rendre_les_cartes(cote, subdivisions) else { return };
            let e = confronter(&avant, cote, false);
            println!(
                "  {subdivisions:>3} subdivisions : distance {:.5}  angle {:.3}°  \
                 {} pixels  {} desaccords de silhouette",
                e.distance, e.angle, e.compte, e.desaccords
            );
            mesures.push(e);
        }

        assert!(
            mesures[0].compte > 1000,
            "rien n'a ete rasterise : ce test ne mesure rien"
        );
        for f in mesures.windows(2) {
            assert!(
                f[1].angle < f[0].angle,
                "l'ecart d'angle ne retrecit pas en subdivisant ({:.3}° puis {:.3}°) : \
                 ce qu'on mesure n'est donc pas la finesse du maillage",
                f[0].angle,
                f[1].angle
            );
        }
    }

    /// ⚠ **La face de SORTIE se vérifie séparément, et ce n'est pas une redite.**
    ///
    /// Elle a son propre pipeline, son propre sens de culling et sa propre comparaison de
    /// profondeur. *Une garde posée sur un seul chemin n'est pas une garde — et ici les deux
    /// chemins sont littéralement deux pipelines.*
    #[test]
    fn la_face_de_sortie_retrouve_elle_aussi_la_bille() {
        let cote = 128;
        let Some((_, arriere)) = rendre_les_cartes(cote, 64) else { return };
        let e = confronter(&arriere, cote, true);
        println!(
            "  face de sortie : distance {:.5}  angle {:.3}°  {} pixels",
            e.distance, e.angle, e.compte
        );
        assert!(e.compte > 1000, "la carte de sortie est vide : rien n'a ete rasterise");
        assert!(
            e.angle < 10.0,
            "la face de sortie s'ecarte de {:.3}° de la bille vraie — au-dela de ce qu'un \
             maillage a 64 subdivisions explique",
            e.angle
        );
    }
    /// ⭐ **LA SONDE QUI SÉPARE DEUX CAUSES QUI DONNENT LE MÊME SYMPTÔME.**
    ///
    /// Une carte d'entrée qui capture la mauvaise face a exactement deux explications possibles :
    /// **(a)** le maillage tourne ses triangles à l'envers, **(b)** la convention d'écran est
    /// inversée. Le rendu ne peut pas les distinguer — les deux donnent la même image fausse.
    ///
    /// Celle-ci les sépare **sans GPU** : elle calcule sur le processeur, pour le premier triangle
    /// venu, si l'ordre de ses indices tourne dans le sens direct **vu depuis l'extérieur de la
    /// bille**. C'est une propriété du MAILLAGE seul, qu'aucune convention d'affichage ne touche.
    ///
    /// *Si le maillage est correct et que la carte est quand même inversée, la cause est
    /// nécessairement l'autre — et c'est alors le pipeline qu'il faut corriger, pas la géométrie.*
    #[test]
    fn le_maillage_de_la_bille_tourne_dans_le_sens_direct_vu_du_dehors() {
        let (sommets, indices) = Primitives::create_uv_sphere(RAYON, 16, 32);
        let mut directs = 0usize;
        let mut inverses = 0usize;
        let mut degeneres = 0usize;

        for t in indices.chunks_exact(3) {
            let a = sommets[t[0] as usize].position;
            let b = sommets[t[1] as usize].position;
            let c = sommets[t[2] as usize].position;
            let (a, b, c) = (Vec3::new(a[0], a[1], a[2]), Vec3::new(b[0], b[1], b[2]), Vec3::new(c[0], c[1], c[2]));

            // La normale géométrique donnée par l'ordre des indices (règle de la main droite).
            let geometrique = (b - a).cross(c - a);
            // Sur une bille centrée à l'origine, la normale SORTANTE est la position elle-même.
            let sortante = (a + b + c) * (1.0 / 3.0);

            // ⚠⚠ LES TRIANGLES DÉGÉNÉRÉS SONT ÉCARTÉS, ET ILS SONT COMPTÉS.
            //
            // Aux deux pôles d'une sphère UV, deux sommets coïncident : le triangle n'a **aucun**
            // sens de rotation. Les ranger avec les fautifs faisait accuser un maillage correct de
            // 64 erreurs — exactement 2 × le nombre de tranches, ce qui était le seul indice.
            //
            // ⚠ **Et la première version testait l'AIRE, ce qui n'en écartait que la moitié.** Au
            // pôle sud, `sin(π)` ne vaut pas zéro en virgule flottante mais **−8,7·10⁻⁸** : les
            // sommets du pôle diffèrent d'un cheveu, l'aire est minuscule mais non nulle, et 32
            // triangles parfaitement dégénérés passaient pour des fautes d'orientation. *Un seuil
            // sur une aire est un chiffre à régler ; une COÏNCIDENCE DE SOMMETS est la définition
            // même d'un triangle dégénéré, et elle ne se règle pas.*
            //
            // *Le juge qui a tranché entre les deux sondes est le VOLUME SIGNÉ, plus bas : il
            // donnait déjà un maillage sain pendant que celle-ci criait au loup.*
            let coincident = |p: Vec3, q: Vec3| (p - q).length() < 1e-5 * RAYON;
            if coincident(a, b) || coincident(b, c) || coincident(a, c) {
                degeneres += 1;
                continue;
            }

            if geometrique.dot(sortante) > 0.0 {
                directs += 1;
            } else {
                inverses += 1;
            }
        }

        println!(
            "  sens direct vu du dehors : {directs}, inverses : {inverses}, \
             degeneres (poles, aire nulle) : {degeneres}"
        );
        assert!(directs + inverses > 100, "maillage trop pauvre pour conclure");
        assert_eq!(
            inverses, 0,
            "{inverses} triangles sur {} tournent a l'envers dans le maillage lui-meme. \
             La cause de la carte inversee serait alors la GEOMETRIE, pas le pipeline.",
            directs + inverses
        );
    }

    /// ⭐⭐ **LA SONDE INDÉPENDANTE : le VOLUME SIGNÉ du maillage.**
    ///
    /// La sonde triangle-par-triangle dit *combien* tournent mal ; celle-ci dit si le maillage est
    /// **globalement cohérent**, et elle le dit par un nombre qu'on peut vérifier à la main.
    ///
    /// Le volume signé d'un maillage fermé est la somme des volumes des tétraèdres formés par
    /// l'origine et chaque triangle. Pour une surface correctement orientée vers l'extérieur, il
    /// vaut **exactement le volume enfermé** — ici `4/3·π·r³ ≈ 4,18879`. Un triangle retourné y
    /// contribue en négatif : le total s'effondre, ou change de signe.
    ///
    /// *Deux sondes indépendantes valent bien mieux qu'une sonde répétée : celle-ci ne partage
    /// aucune hypothèse avec l'autre, et c'est ce qui lui donne le droit de la contredire.*
    #[test]
    fn le_volume_signe_de_la_bille_vaut_celui_d_une_vraie_sphere() {
        for subdivisions in [16u32, 32, 64] {
            let (sommets, indices) =
                Primitives::create_uv_sphere(RAYON, subdivisions, subdivisions * 2);
            let mut volume = 0.0f64;
            for t in indices.chunks_exact(3) {
                let p = |n: usize| {
                    let v = sommets[t[n] as usize].position;
                    Vec3::new(v[0], v[1], v[2])
                };
                let (a, b, c) = (p(0), p(1), p(2));
                volume += a.dot(b.cross(c)) as f64 / 6.0;
            }

            let attendu = 4.0 / 3.0 * std::f64::consts::PI * (RAYON as f64).powi(3);
            let ecart = (volume - attendu).abs() / attendu;
            println!(
                "  {subdivisions:>3} subdivisions : volume signe {volume:.5} \
                 (une vraie sphere : {attendu:.5}, ecart {:.2} %)",
                ecart * 100.0
            );

            assert!(
                volume > 0.0,
                "le volume signe est NEGATIF ({volume:.5}) : le maillage est retourne dans son \
                 ensemble"
            );
            // Un polyèdre inscrit est toujours un peu plus petit que la sphère — l'écart doit donc
            // être positif et RÉTRÉCIR en subdivisant. 15 % laisse la place au maillage le plus
            // grossier des trois sans laisser passer une bande de triangles retournés, qui coûte
            // bien davantage.
            assert!(
                ecart < 0.15,
                "le volume s'ecarte de {:.1} % de celui d'une vraie sphere : des triangles \
                 sont retournes et se soustraient au total",
                ecart * 100.0
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════════════════════════════
    //  LA CHAÎNE COMPLÈTE : un maillage → deux cartes → la réfraction → une image
    // ══════════════════════════════════════════════════════════════════════════════════════════

    const ETA: f32 = 1.0 / 1.5;

    /// Snell, sous forme vectorielle. **Écrite à part du shader, exprès.**
    ///
    /// `None` = réflexion totale interne : ce n'est pas un cas à coder, c'est ce qui reste quand la
    /// racine n'existe pas.
    fn refracter(incident: Vec3, normale: Vec3, eta: f32) -> Option<Vec3> {
        let cos_i = -normale.dot(incident);
        let reste = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
        (reste >= 0.0).then(|| incident * eta + normale * (eta * cos_i - reste.sqrt()))
    }

    /// ⭐ **LA VÉRITÉ** — la direction qui ressort d'une bille de verre, analytiquement.
    ///
    /// Deux interfaces, la sphère résolue exactement aux deux, aucune carte et aucun maillage.
    /// *C'est contre ce vecteur que l'image produite par toute la chaîne est jugée.*
    fn direction_de_sortie(direction: Vec3) -> Option<Vec3> {
        let origine = Vec3::new(0.0, 0.0, -RECUL);
        let (t0, _) = verite(direction)?;
        let entree = origine + direction * t0;
        let dedans = refracter(direction, entree * (1.0 / RAYON), ETA)?;

        // Où ce rayon interne ressort : la seconde racine, depuis un point DÉJÀ sur la sphère.
        let b = entree.dot(dedans);
        let sortie = entree + dedans * (-2.0 * b);
        // ⚠ La normale pointe vers l'extérieur ; en sortant on la retourne, et le rapport
        // d'indices s'inverse avec elle.
        refracter(dedans, sortie * (-1.0 / RAYON), 1.0 / ETA)
    }

    /// ⭐⭐⭐ **LE TEST QUI BOUCLE LA CHAÎNE — et la première image de matière du moteur.**
    ///
    /// Jusqu'ici les deux moitiés existaient sans se parler : `verre` savait réfracter mais lisait
    /// des cartes écrites à la main dans son propre test, et `cartes` savait rastériser mais
    /// personne ne lisait ce qu'elle produisait. **Ceci les branche.**
    ///
    /// ⚠⚠ **ET LE REPÈRE EST LE PIÈGE DE CE BRANCHEMENT.** Le banc de `verre` décrit sa caméra
    /// avec `droite = +X` en regardant vers `+Z` — un repère **main gauche**. La caméra du moteur
    /// est en main droite : son axe droit est `−X`. *Donner au shader les constantes du banc
    /// produirait une image parfaitement plausible et fausse, avec un verre qui dévie la lumière
    /// du mauvais côté — et une bille est trop symétrique pour qu'une silhouette le révèle.*
    #[test]
    fn le_verre_refracte_une_bille_rasterisee_contre_la_verite_analytique() {
        let cote = 256u32;
        let Some(bruts) = chaine_complete(cote, 64) else { return };

        let (mut somme, mut compte, mut pire) = (0.0f64, 0usize, 0.0f32);
        for y in 0..cote {
            for x in 0..cote {
                let d = direction_du_pixel(x, y, cote);
                let Some(attendue) = direction_de_sortie(d) else { continue };
                let i = ((y * cote + x) * 4) as usize;
                // Format B8G8R8A8 : le bleu vient en premier. Les directions sont encodées en
                // `v * 0,5 + 0,5`, donc lues en `v * 2 − 1`.
                let lu = Vec3::new(
                    bruts[i + 2] as f32 / 255.0 * 2.0 - 1.0,
                    bruts[i + 1] as f32 / 255.0 * 2.0 - 1.0,
                    bruts[i] as f32 / 255.0 * 2.0 - 1.0,
                )
                .normalize_or_zero();
                if lu.length() < 0.5 {
                    continue;
                }
                let angle = attendue.dot(lu).clamp(-1.0, 1.0).acos().to_degrees();
                somme += angle as f64;
                pire = pire.max(angle);
                compte += 1;
            }
        }

        let moyenne = somme / compte.max(1) as f64;
        println!(
            "  bille RASTERISEE -> refraction : ecart moyen {moyenne:.3}° (pire {pire:.3}°) sur \
             {compte} pixels"
        );
        println!("  pour memoire : cartes EXACTES 2,132° · physique seule 1,789° · sans Newton 16,773°");

        assert!(compte > 5000, "trop peu de pixels compares : la chaine n'a rien produit");
        // ⚠ Le seuil est LARGE et c'est voulu : il ne mesure pas une qualité, il refuse une chaîne
        // débranchée ou un repère inversé. *Le repère faux donnait 44° sur les normales seules ;
        // ici il donnerait bien davantage.* La qualité, elle, se lit dans le chiffre affiché et se
        // compare aux trois repères de la ligne au-dessus.
        assert!(
            moyenne < 6.0,
            "la chaine complete s'ecarte de {moyenne:.3}° de la verite — au-dela des 2,132° des \
             cartes exactes plus le cout de la rasterisation. Verifier le REPERE en premier."
        );
    }

    /// Rastérise la bille dans deux cartes, branche le verre dessus, et rend l'image des
    /// directions de sortie.
    fn chaine_complete(cote: u32, subdivisions: u32) -> Option<Vec<u8>> {
        chaine(cote, subdivisions, 1.0, [0.0, 0.0, 0.0], vk::Format::B8G8R8A8_UNORM)
    }

    /// La même chaîne, mais qui rend une COULEUR — donc à travers la chaîne du vrai rendu, sRGB
    /// comprise. *Une direction se lit en `UNORM`, une couleur en `SRGB` : à travers une courbe de
    /// gamma, la quantification est trois fois plus grossière autour de 0,5.*
    fn chaine_complete_couleur(cote: u32, subdivisions: u32, sigma: [f32; 3]) -> Option<Vec<u8>> {
        chaine(cote, subdivisions, 0.0, sigma, vk::Format::B8G8R8A8_SRGB)
    }

    fn chaine(
        cote: u32,
        subdivisions: u32,
        mode: f32,
        sigma: [f32; 3],
        format_cible: vk::Format,
    ) -> Option<Vec<u8>> {
        // La cible finale est une image d'écran ordinaire ; les deux cartes vivent à part, en
        // flottants.
        let ctx = match GpuContext::sans_ecran_format(cote, cote, 1, format_cible) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠ aucun Vulkan joignable : {e}");
                return None;
            }
        };
        let memory_props =
            unsafe { ctx.instance.get_physical_device_memory_properties(ctx.physical_device) };

        let (sommets, indices) =
            Primitives::create_uv_sphere(RAYON, subdivisions, subdivisions * 2);
        let maillage = GpuMesh::upload(&ctx, &memory_props, &sommets, &indices)
            .expect("le maillage de la bille n'a pas pu monter sur la carte");

        let mut cadre = crate::render::cadre::Cadre::nouveau(&ctx, &memory_props)
            .expect("le cadre uniforme n'a pas pu etre cree");
        let oeil = Vec3::new(0.0, 0.0, -RECUL);
        let mut camera = crate::scene::camera::Camera::new(oeil, Vec3::new(0.0, 0.0, 0.0), 1.0);
        camera.fov_y_radians = 2.0 * TANGENTE.atan();
        let view_proj = camera.compute_projection_matrix() * camera.compute_view_matrix();
        cadre.ecrire(&crate::render::cadre::DonneesImage::nouvelle(
            view_proj,
            Mat4::IDENTITY,
            [oeil.x, oeil.y, oeil.z],
            crate::render::cadre::Ambiance::default(),
            &[],
        ));

        let instances = crate::render::instances::Instances::nouveau(&ctx, &memory_props, 1)
            .expect("le tampon d'instances n'a pas pu etre cree");
        let passe = Cartes::nouvelle(&ctx, cadre.layout_descripteur)
            .expect("les pipelines de cartes n'ont pas pu etre crees");
        let profondeur = image_de_profondeur(&ctx, &memory_props, cote)
            .expect("l'image de profondeur n'a pas pu etre creee");

        let mut cartes = Vec::new();
        for face in [Faces::Entree, Faces::Sortie] {
            let cible = image_de_carte(&ctx, &memory_props, cote)
                .expect("une image de carte n'a pas pu etre creee");
            instances.recommencer();
            rendre_dans(
                &ctx, &passe, face, &cadre, &maillage, &instances, cible.0, cible.1, profondeur.1,
                cote,
            );
            cartes.push(cible);
        }

        // ── La réfraction lit les deux cartes et écrit l'image ──
        let verre = crate::render::verre::Verre::nouvelle(&ctx, ctx.swapchain_format)
            .expect("la passe de verre n'a pas pu etre creee");
        verre.brancher(&ctx, cartes[0].1, cartes[1].1);

        let k = crate::render::verre::ConstantesVerre {
            position: [oeil.x, oeil.y, oeil.z, 0.0],
            // ⚠⚠ `−X` À DROITE, et c'est tout le sujet de ce test. Voir sa documentation.
            droite: [-1.0, 0.0, 0.0, TANGENTE],
            haut: [0.0, 1.0, 0.0, TANGENTE],
            avant: [0.0, 0.0, 1.0, 0.0],
            // Pas d'absorption : on mesure une DIRECTION, et une couleur absorbée n'en dit rien.
            matiere: [sigma[0], sigma[1], sigma[2], ETA],
            // `z = 1` : le mode « direction » (le vecteur de sortie) ; `z = 0` : une vraie image.
            reglages: [cote as f32, cote as f32, mode, 8.0],
        };

        let image = ctx.swapchain_images[0];
        let vue = ctx.swapchain_image_views[0];
        let etendue = vk::Extent2D { width: cote, height: cote };
        let cmd = ctx.begin_single_time_commands().expect("tampon de commandes");
        unsafe {
            barriere(
                &ctx, cmd, image, vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } });
            ctx.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            regler_la_vue(&ctx, cmd, cote);
        }
        verre.dessiner(&ctx, cmd, &k);
        unsafe { ctx.device.cmd_end_rendering(cmd) };
        ctx.end_single_time_commands(cmd).expect("soumission");

        let bruts = ctx
            .relire_image_brute(
                image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                etendue,
                ctx.swapchain_format,
            )
            .expect("relecture de l'image finale");

        unsafe {
            ctx.device.device_wait_idle().ok();
            for (img, vue, mem) in cartes.iter().chain(std::iter::once(&profondeur)) {
                ctx.device.destroy_image_view(*vue, None);
                ctx.device.destroy_image(*img, None);
                ctx.device.free_memory(*mem, None);
            }
        }
        verre.detruire(&ctx.device);
        passe.detruire(&ctx.device);
        cadre.detruire(&ctx.device);
        Some(bruts)
    }

    /// Une transition de layout, écrite une fois.
    ///
    /// ⚠ Les étages et les accès sont volontairement LARGES (`ALL_COMMANDS`) : c'est un chemin de
    /// banc, hors du chemin critique du rendu. *Un banc qui optimise ses barrières mesure ses
    /// barrières.*
    unsafe fn barriere(
        ctx: &GpuContext,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        avant: vk::ImageLayout,
        apres: vk::ImageLayout,
    ) {
        unsafe {
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .old_layout(avant)
                    .new_layout(apres)
                    .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })],
            );
        }
    }

    /// Le cadrage, identique pour toutes les passes de ce banc.
    unsafe fn regler_la_vue(ctx: &GpuContext, cmd: vk::CommandBuffer, cote: u32) {
        unsafe {
            ctx.device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: cote as f32,
                    height: cote as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            ctx.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D { width: cote, height: cote },
                }],
            );
        }
    }

    /// Rastérise une face dans l'image donnée, et la laisse **lisible par un shader**.
    ///
    /// *C'est la seule différence avec `rendre_une_carte`, qui rapatrie l'image au lieu de la
    /// laisser sur la carte — les deux partagent tout le reste.*
    #[allow(clippy::too_many_arguments)]
    fn rendre_dans(
        ctx: &GpuContext,
        passe: &Cartes,
        face: Faces,
        cadre: &crate::render::cadre::Cadre,
        maillage: &GpuMesh,
        instances: &crate::render::instances::Instances,
        image: vk::Image,
        vue: vk::ImageView,
        vue_profondeur: vk::ImageView,
        cote: u32,
    ) {
        let cmd = ctx.begin_single_time_commands().expect("tampon de commandes");
        unsafe {
            barriere(
                ctx, cmd, image, vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );
            let couleur = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } });
            let profondeur = vk::RenderingAttachmentInfo::default()
                .image_view(vue_profondeur)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: face.profondeur_initiale(),
                        stencil: 0,
                    },
                });
            ctx.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: vk::Extent2D { width: cote, height: cote },
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&couleur))
                    .depth_attachment(&profondeur),
            );
            regler_la_vue(ctx, cmd, cote);
        }

        passe.dessiner(
            ctx,
            cmd,
            face,
            cadre,
            &Objet { maillage, instances, modele: Mat4::IDENTITY },
        );
        unsafe {
            ctx.device.cmd_end_rendering(cmd);
            // ⭐ La carte devient une TEXTURE : c'est cette transition qui la fait passer du
            // statut d'image qu'on écrit à celui d'image qu'un shader lit.
            barriere(
                ctx, cmd, image, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        ctx.end_single_time_commands(cmd).expect("soumission");
    }

    /// ⭐⭐⭐ **LA PREMIÈRE IMAGE DE MATIÈRE DU MOTEUR ISSUE D'UNE VRAIE GÉOMÉTRIE.**
    ///
    /// Les trois images du verre existaient déjà — mais leur bille était une **équation**, résolue
    /// analytiquement dans le test. Celles-ci viennent d'un **maillage rastérisé par la carte
    /// graphique**, exactement comme viendra la géométrie de Blender.
    ///
    /// ⚠ **Un test ne peut pas juger ces images.** Il vérifie qu'elles sont écrites et qu'elles ne
    /// sont pas vides ; *le juge du rendu perçu est son œil, et rien d'autre.*
    #[test]
    fn les_images_de_la_bille_rasterisee() {
        let Some(directions) = chaine_complete(256, 64) else {
            println!("  (aucune image ecrite — pas de Vulkan)");
            return;
        };
        let Some(couleur) = chaine_complete_couleur(256, 64, [0.0, 0.0, 0.0]) else { return };
        let Some(coloree) = chaine_complete_couleur(256, 64, [0.35, 0.9, 1.6]) else { return };

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        for (nom, bruts) in [
            ("cartes-bille-directions.png", &directions),
            ("cartes-bille-rasterisee.png", &couleur),
            ("cartes-bille-absorbante.png", &coloree),
        ] {
            let mut rvb = Vec::with_capacity(bruts.len() / 4 * 3);
            for p in bruts.chunks_exact(4) {
                // B8G8R8A8 : le bleu vient en premier.
                rvb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            let png = crate::image::png::encoder(256, 256, &rvb).expect("png");
            std::fs::write(dossier.join(nom), png).expect("ecriture");
            println!("  ecrit : target/preuves/{nom}");
        }

        // Garde anti-image-vide : une image entièrement noire passerait pour un rendu.
        let vivants = couleur.chunks_exact(4).filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
        assert!(
            vivants > 10_000,
            "seulement {vivants} pixels non noirs : l'image est vide, la chaine n'a rien rendu"
        );
    }
}
