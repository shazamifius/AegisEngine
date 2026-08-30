//! # L'OCCLUSION AMBIANTE — ce que le ciel ne voit pas
//!
//! Né le 31 août 2026. `ambiance_hemispherique` donne à chaque surface la lumière du ciel selon
//! son **orientation**, et rien d'autre : une face tournée vers le haut reçoit donc autant de ciel
//! au fond d'un trou qu'en plein air. Ça se voit partout — les objets ne se posent pas, ils
//! flottent ; deux cubes qui se touchent n'ont aucun creux entre eux.
//!
//! ## ⭐ Ce qui a décidé de l'architecture : un facteur deux, en plein soleil
//!
//! L'occlusion ne doit multiplier **que** l'ambiante. Le geste facile — l'appliquer à l'image
//! finie — a été écarté sur un calcul, pas sur un principe. Au pied d'un mur ensoleillé, avec les
//! valeurs réelles de cette scène (direct 1,2 · ambiante 0,02 · occlusion 0,5) :
//!
//! | | résultat |
//! |---|---|
//! | juste : `direct + ambiante × occlusion` | **1,21** |
//! | occulter la lumière totale | **0,61** |
//!
//! Des taches sombres en plein soleil, et d'autant plus visibles que le rapport direct/ambiant
//! venait justement d'être monté. *La séparation n'est pas un raffinement : sans elle l'effet est
//! faux.* La passe de scène émet donc l'ambiante sur une seconde sortie.
//!
//! ## ⭐⭐ Deux passes, et AUCUNE image intermédiaire
//!
//! Le montage habituel écrit l'occlusion dans une image, puis la combine dans un shader qui lit
//! deux textures — donc un second agencement de descripteurs, une image de plus, et une passe de
//! plus. Ici la **carte** fait les deux combinaisons :
//!
//! 1. l'occlusion s'écrit dans l'image d'ambiante en **multipliant** : l'ambiante y devient « ce
//!    qu'il faut retirer » ;
//! 2. une simple copie la **soustrait** de la scène.
//!
//! Aucune passe ne lit jamais plus d'une image, le même agencement sert tout, et rien n'est
//! alloué. *Le mécanisme n'a pas été optimisé — la moitié n'a jamais eu à exister.*
//!
//! ## Ce qui n'est PAS fait
//!
//! Pas de passe de flou. L'occlusion est calculée à pleine résolution, avec un angle de départ
//! qui varie d'un pixel à l'autre : le grain qui en résulte est fin et se lit comme du bruit de
//! surface, pas comme des bandes. Un flou bilatéral serait plus propre et coûterait une passe
//! entière — *à ajouter si ça granule à l'œil, pas avant.*

use crate::render::pipeline::{Melange, PipelineFactory, Reglages};
use crate::GpuContext;
use ash::vk;

/// L'occlusion est-elle allumée ?
///
/// ⚠ `AEGIS_OCCLUSION=0` l'éteint. Interrupteur de **banc**, comme `AEGIS_HALO` : il sert à
/// mesurer ce que l'effet coûte en comparant deux exécutions du même binaire, et à prouver qu'il
/// fait bien quelque chose. Ce n'est pas un réglage offert au joueur.
fn occlusion_allumee() -> bool {
    match std::env::var("AEGIS_OCCLUSION") {
        Err(_) => true,
        Ok(texte) => match texte.trim() {
            "0" => {
                log::info!("AEGIS_OCCLUSION=0 — l'occlusion est eteinte (interrupteur de banc)");
                false
            }
            "1" => true,
            autre => {
                log::warn!("AEGIS_OCCLUSION={autre:?} n'est ni 0 ni 1 — elle reste allumee");
                true
            }
        },
    }
}

pub struct Occlusion {
    /// ⚠ Un échantillonneur au **plus proche voisin**, et c'est obligatoire : il lit des
    /// PROFONDEURS. Les interpoler donnerait, entre deux surfaces, une distance intermédiaire où
    /// il n'y a rien — une surface fantôme le long de chaque silhouette, qui occulterait le vide.
    /// C'est la même raison qui fait résoudre les échantillons de profondeur en `SAMPLE_ZERO`.
    proche: vk::Sampler,
    pool: vk::DescriptorPool,
    /// Pointe sur la profondeur : ce que la passe de calcul lit.
    depuis_profondeur: vk::DescriptorSet,
    /// Pointe sur l'ambiante : ce que la passe de correction lit.
    depuis_ambiante: vk::DescriptorSet,
    layout_pipeline: vk::PipelineLayout,
    calcul: vk::Pipeline,
    correction: vk::Pipeline,
    actif: bool,
}

impl Occlusion {
    const ENSEMBLES: u32 = 2;

    /// `layout_descripteur` et `layout_pipeline` viennent de l'écran : **un seul contrat pour
    /// toutes les passes plein écran du moteur.** En refaire un ici donnerait deux définitions à
    /// tenir d'accord, pour décrire exactement la même chose.
    pub fn nouvelle(
        gpu: &GpuContext,
        cibles: &crate::render::cibles::Cibles,
        layout_descripteur: vk::DescriptorSetLayout,
        layout_pipeline: vk::PipelineLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let proche = unsafe {
            gpu.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::NEAREST)
                    .min_filter(vk::Filter::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        let tailles = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(Self::ENSEMBLES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(Self::ENSEMBLES),
        ];
        let pool = unsafe {
            gpu.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&tailles)
                    .max_sets(Self::ENSEMBLES),
                None,
            )?
        };
        let layouts = [layout_descripteur, layout_descripteur];
        let ensembles = unsafe {
            gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )?
        };

        let monter = |source: &[u8], reglages: Reglages| -> Result<vk::Pipeline, Box<dyn std::error::Error>> {
            let m = PipelineFactory::create_shader_module_from_bytes(&gpu.device, source)?;
            let p = PipelineFactory::create_graphics_pipeline(
                &gpu.device, layout_pipeline, m, m, reglages,
            )?;
            unsafe { gpu.device.destroy_shader_module(m, None) };
            Ok(p)
        };
        let plein_ecran = |melange| Reglages {
            color_format: cibles.format_hdr,
            second_format: None,
            depth_format: None,
            depth_write: false,
            melange,
            use_vertex_input: false,
            echantillons: vk::SampleCountFlags::TYPE_1,
        };

        // ⚠ Le calcul écrit DANS l'ambiante, en multipliant : après lui, l'image d'ambiante ne
        // contient plus l'ambiante mais la part que l'occlusion lui retire.
        let calcul = monter(
            crate::shaders::OCCLUSION_SPV,
            plein_ecran(Melange::Multiplicatif),
        )?;
        let correction = monter(crate::shaders::COPIE_SPV, plein_ecran(Melange::Soustractif))?;

        let occlusion = Self {
            proche,
            pool,
            depuis_profondeur: ensembles[0],
            depuis_ambiante: ensembles[1],
            layout_pipeline,
            calcul,
            correction,
            actif: occlusion_allumee(),
        };
        occlusion.brancher(&gpu.device, cibles, layout_descripteur);
        Ok(occlusion)
    }

    /// Fait pointer les deux descripteurs sur les images courantes.
    ///
    /// ⚠ **À rappeler après chaque redimensionnement** : les vues changent avec la taille, et un
    /// descripteur resté sur une image détruite ne produit aucune erreur — seulement une image
    /// dont on ne sait pas dire ce qu'elle montre.
    pub fn brancher(
        &self,
        device: &ash::Device,
        cibles: &crate::render::cibles::Cibles,
        _layout: vk::DescriptorSetLayout,
    ) {
        // ⚠ La profondeur a sa PROPRE disposition de lecture, différente de celle d'une couleur.
        // L'annoncer comme une couleur est un défaut que les couches de validation attrapent, et
        // que le pilote, lui, peut très bien laisser passer en lisant n'importe quoi. La
        // disposition voyage donc AVEC la vue, plutôt que d'être devinée à partir de l'ensemble.
        for (ensemble, vue, disposition) in [
            (
                self.depuis_profondeur,
                cibles.vue_profondeur_lisible(),
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            ),
            (
                self.depuis_ambiante,
                cibles.vue_ambiante_resolue(),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ),
        ] {
            let image = vk::DescriptorImageInfo::default()
                .image_view(vue)
                .image_layout(disposition);
            let sampler = vk::DescriptorImageInfo::default().sampler(self.proche);
            let ecritures = [
                vk::WriteDescriptorSet::default()
                    .dst_set(ensemble)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&image)),
                vk::WriteDescriptorSet::default()
                    .dst_set(ensemble)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(std::slice::from_ref(&sampler)),
            ];
            unsafe { device.update_descriptor_sets(&ecritures, &[]) };
        }
    }

    /// Allume ou éteint l'occlusion en direct. Même état que `AEGIS_OCCLUSION`, pas un second.
    pub fn allumer(&mut self, actif: bool) {
        self.actif = actif;
    }

    pub fn active(&self) -> bool {
        self.actif
    }

    /// Retire de la scène ce que le ciel ne peut pas y apporter.
    ///
    /// ⚠ **À appeler entre la passe de scène et le halo.** Le halo doit voir la scène corrigée :
    /// une zone occluse ne doit pas déborder du blanc à cause d'une lumière qu'elle ne reçoit pas.
    ///
    /// # Safety
    /// `cmd` doit être en cours d'enregistrement, hors de toute passe de rendu. La profondeur et
    /// l'ambiante doivent être lisibles, la scène en attachement.
    pub unsafe fn appliquer(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        cibles: &crate::render::cibles::Cibles,
        etendue: vk::Extent2D,
    ) {
        if !self.actif {
            return;
        }
        unsafe {
            // ── 1. L'ambiante devient « ce qu'il faut retirer » ──────────────────────────────
            self.passe(
                device,
                cmd,
                self.calcul,
                self.depuis_profondeur,
                cibles.vue_ambiante_resolue(),
                etendue,
            );

            // ⚠ L'ambiante vient d'être écrite et va être lue : sans cette barrière, la seconde
            // passe lirait des pixels que la première n'a pas fini d'écrire.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(cibles.image_ambiante_resolue())
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })],
            );

            // ── 2. On le retire de la scène ─────────────────────────────────────────────────
            self.passe(
                device,
                cmd,
                self.correction,
                self.depuis_ambiante,
                cibles.vue_resolue(),
                etendue,
            );
        }
    }

    /// Une passe plein écran qui MÉLANGE avec sa cible — d'où le `LOAD` obligatoire.
    unsafe fn passe(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pipeline: vk::Pipeline,
        lecture: vk::DescriptorSet,
        cible: vk::ImageView,
        etendue: vk::Extent2D,
    ) {
        unsafe {
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(cible)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                // ⚠ `LOAD`, jamais `DONT_CARE` : les deux passes MULTIPLIENT ou SOUSTRAIENT ce qui
                // est déjà là. Avec `DONT_CARE` la carte a le droit de leur donner du vide, et le
                // résultat serait une image noire — sans le moindre message.
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: etendue.width as f32,
                    height: etendue.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue }],
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout_pipeline,
                1,
                &[lecture],
                &[],
            );
            crate::mesure::noter_dessin(1);
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.calcul, None);
            device.destroy_pipeline(self.correction, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_sampler(self.proche, None);
        }
    }
}
