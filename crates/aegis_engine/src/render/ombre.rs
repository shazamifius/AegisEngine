//! # LA CARTE D'OMBRE — ce que la lumière voit, et ce qu'elle ne voit pas
//!
//! Née le 29 août 2026. Le principe tient en une phrase : **on dessine la scène depuis la lumière
//! en ne gardant que la profondeur**, puis, pour chaque pixel de l'écran, on regarde s'il est plus
//! loin que ce que la lumière avait vu dans cette direction. S'il l'est, quelque chose le cache :
//! il est dans l'ombre.
//!
//! ## Pourquoi une projection ORTHOGRAPHIQUE, et pas perspective
//!
//! Le soleil est si loin que ses rayons arrivent parallèles. Une projection perspective ferait
//! diverger les ombres en s'éloignant de la source — un défaut immédiatement visible, et qu'on
//! croirait longtemps venir du filtrage.
//!
//! ## ⚠ L'ACNÉ D'OMBRE, et pourquoi le décalage est ici plutôt qu'ailleurs
//!
//! Une surface éclairée se compare à *elle-même* dans la carte d'ombre, avec une profondeur qui a
//! été arrondie à la résolution de la carte. Un pixel sur deux se retrouve donc « derrière »
//! lui-même : la surface se raye de bandes sombres. C'est le défaut le plus classique du procédé.
//!
//! Le remède retenu est le **décalage de profondeur au moment de dessiner la carte**
//! (`depth_bias`), pas un epsilon ajouté à la comparaison. La différence compte : un epsilon est
//! une constante arbitraire à re-régler pour chaque scène et chaque distance, alors que le décalage
//! matériel s'adapte à l'inclinaison de la surface — c'est justement là que l'erreur est la plus
//! grande. *La constante ne rétrécit pas, elle disparaît.*
//!
//! ## Ce que ça coûte, et pourquoi ça compte plus qu'ailleurs
//!
//! Cette passe **redessine la scène une seconde fois**. Sur la machine de référence du projet — un
//! Meta Quest 2, 13,9 ms pour deux yeux — c'est le genre de dépense qui se paie cher. D'où deux
//! choix inscrits dans le code plutôt que dans une intention : le shader d'ombre **ne calcule
//! aucune couleur**, et seuls les objets marqués porteurs d'ombre entrent dans la passe.

use crate::core::math::{Mat4, Vec3};
use crate::core::memory::MemoryManager;
use crate::render::file::File;
use crate::render::pipeline::PipelineFactory;
use crate::GpuContext;
use ash::vk;

/// La matrice qui place le monde du point de vue d'une lumière directionnelle.
///
/// **Partie purement calculatoire, séparée exprès du Vulkan** : c'est ici que vivent les erreurs
/// possibles (l'orientation, les bornes, la profondeur), et cette fonction se teste sans GPU.
///
/// - `vers_la_lumiere` : la direction qui **pointe vers** la lumière, la même convention que
///   [`crate::scene::light::GpuLight::new_directional`]. L'inverser retournerait les ombres.
/// - `centre` et `rayon` : la sphère du monde qu'on veut voir ombrée. Tout ce qui est dehors ne
///   projette rien — c'est le compromis de toute carte d'ombre unique, et il vaut mieux l'écrire
///   que le laisser découvrir.
pub fn matrice_lumiere(vers_la_lumiere: Vec3, centre: Vec3, rayon: f32) -> Mat4 {
    let direction = vers_la_lumiere.normalize();
    let oeil = centre + direction * rayon;

    // ⚠ Un `look_at` dégénère quand la direction est colinéaire au vecteur « haut » : le produit
    // vectoriel s'annule et toute la matrice devient invalide. Une lumière exactement zénithale
    // n'a rien d'exotique — c'est même le réglage le plus naturel pour un soleil de midi.
    let haut = if direction.y.abs() > 0.99 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };

    let vue = Mat4::look_at_rh(oeil, centre, haut);
    // Le volume s'arrête juste avant l'œil et va jusqu'au bout de la sphère : plus serré, on
    // couperait des objets ; plus large, on gaspillerait la précision de la carte.
    let projection = Mat4::orthographic_rh(-rayon, rayon, -rayon, rayon, 0.0, 2.0 * rayon);
    projection * vue
}

/// La carte d'ombre : une image de profondeur, et de quoi la remplir puis la lire.
pub struct Ombre {
    image: vk::Image,
    memoire: vk::DeviceMemory,
    pub vue: vk::ImageView,
    pub echantillonneur: vk::Sampler,
    pub resolution: u32,
    pub format: vk::Format,
    pipeline: vk::Pipeline,
}

impl Ombre {
    /// `resolution` est un compromis mémoire contre finesse : 2048 pèse 8 Mo en 32 bits et suffit
    /// largement à une scène de plateforme ; 1024 conviendrait à un téléphone.
    pub fn nouvelle(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        layout: vk::PipelineLayout,
        resolution: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let format = vk::Format::D32_SFLOAT;

        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: resolution, height: resolution, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { gpu.device.create_image(&info, None)? };
        let besoins = unsafe { gpu.device.get_image_memory_requirements(image) };
        let type_memoire = MemoryManager::find_memory_type(
            memory_props,
            besoins.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .ok_or("aucune memoire GPU compatible pour la carte d'ombre")?;

        let memoire = unsafe {
            gpu.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(besoins.size)
                    .memory_type_index(type_memoire),
                None,
            )?
        };
        unsafe { gpu.device.bind_image_memory(image, memoire, 0)? };

        let vue = unsafe {
            gpu.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?
        };

        // ⚠ Un échantillonneur de COMPARAISON, et c'est ce qui rend le filtrage possible.
        // Un échantillonnage ordinaire moyennerait des PROFONDEURS, ce qui n'a aucun sens — la
        // moyenne de « 3 m » et « 10 m » ne dit rien sur l'ombre. Avec la comparaison, le matériel
        // teste d'abord puis moyenne les RÉSULTATS (0 ou 1), et un seul appel rend un bord adouci.
        let echantillonneur = unsafe {
            gpu.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    // Hors de la carte = pleinement éclairé. Le contraire plongerait dans le noir
                    // tout ce que la lumière ne couvre pas, ce qui est bien plus visible qu'une
                    // ombre manquante.
                    .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                    .compare_enable(true)
                    .compare_op(vk::CompareOp::LESS_OR_EQUAL),
                None,
            )?
        };

        let vert = PipelineFactory::create_shader_module_from_bytes(
            &gpu.device,
            crate::shaders::OMBRE_VERT_SPV,
        )?;
        let frag = PipelineFactory::create_shader_module_from_bytes(
            &gpu.device,
            crate::shaders::OMBRE_FRAG_SPV,
        )?;

        let pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            layout,
            vert,
            frag,
            crate::render::pipeline::Reglages {
                // Aucune cible de couleur : cette passe n'écrit que de la profondeur.
                color_format: vk::Format::UNDEFINED,
                depth_format: Some(format),
                depth_write: true,
                melange: crate::render::pipeline::Melange::Aucun,
                use_vertex_input: true,
                // ⚠ La carte d'ombre reste a UN echantillon, et ce n'est pas un oubli : elle ne
                // s'affiche jamais, on y lit des profondeurs. Multi-echantillonner une profondeur
                // quadruplerait une carte de 2048x2048 (16 Mo -> 64 Mo) pour un bord d'ombre que
                // le filtrage PCF adoucit deja. *Jamais d'excedent.*
                echantillons: vk::SampleCountFlags::TYPE_1,
            },
        )?;

        unsafe {
            gpu.device.destroy_shader_module(vert, None);
            gpu.device.destroy_shader_module(frag, None);
        }

        log::info!("Carte d'ombre : {resolution}x{resolution}, format {format:?}");

        Ok(Self { image, memoire, vue, echantillonneur, resolution, format, pipeline })
    }

    /// Dessine la carte : la file, vue depuis la lumière, profondeur seule.
    ///
    /// # Safety
    /// Le tampon de commandes doit être en enregistrement, hors de toute passe de rendu.
    pub unsafe fn dessiner(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        // ⚠ Inutilise depuis l'instanciation — plus rien ne pousse de constantes. Conserve parce
        // que c'est par lui que le descripteur du cadre se lierait si cette passe en avait
        // besoin ; le retirer obligerait a le remettre au premier reglage qui le demande.
        _layout: vk::PipelineLayout,
        file: &File,
        instances: &crate::render::instances::Instances,
        maillages: &[&crate::geometry::gpu_mesh::GpuMesh],
    ) {
        unsafe {
            let vers_ecriture = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .image(self.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                );
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[], &[], &[vers_ecriture],
            );

            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(self.vue)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                });

            let zone = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width: self.resolution, height: self.resolution },
            };
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(zone)
                    .layer_count(1)
                    .depth_attachment(&attache),
            );

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0, y: 0.0,
                width: self.resolution as f32,
                height: self.resolution as f32,
                min_depth: 0.0, max_depth: 1.0,
            }]);
            device.cmd_set_scissor(cmd, 0, &[zone]);
            // Le décalage qui empêche une surface de s'ombrer elle-même. La pente compte plus que
            // la constante : c'est sur les surfaces inclinées que l'erreur d'arrondi est la pire.
            device.cmd_set_depth_bias(cmd, 1.5, 0.0, 2.5);

            // ⚠ La passe d'ombre instancie comme la passe principale, et pour la meme raison :
            // elle rejoue la MEME scene. Un chemin qui dessinerait objet par objet ici aurait
            // rendu tout le gain a la premiere ombre — et l'ombre coute deja une seconde passe.
            instances.lier(device, cmd);
            let mut lot: Vec<crate::render::instances::Instance> = Vec::new();
            let mut courant: Option<u16> = None;

            let vider = |lot: &mut Vec<crate::render::instances::Instance>,
                             id: Option<u16>| {
                let (Some(id), false) = (id, lot.is_empty()) else {
                    lot.clear();
                    return;
                };
                if let Some(maillage) = maillages.get(id as usize) {
                    if let Some(premiere) = instances.poser(lot) {
                        maillage.dessiner_instances(device, cmd, premiere, lot.len() as u32);
                    }
                }
                lot.clear();
            };

            for d in file.porteurs_d_ombre() {
                if courant != Some(d.maillage) {
                    vider(&mut lot, courant);
                    courant = Some(d.maillage);
                }
                lot.push(crate::render::instances::Instance {
                    modele: d.modele,
                    teinte: d.teinte,
                    params: d.params,
                });
            }
            vider(&mut lot, courant);

            device.cmd_end_rendering(cmd);

            let vers_lecture = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(self.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                );
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[], &[], &[vers_lecture],
            );
        }
    }

    pub fn detruire(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_sampler(self.echantillonneur, None);
            device.destroy_image_view(self.vue, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memoire, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::math::Vec4;

    fn projete(m: Mat4, p: Vec3) -> Vec3 {
        let v = m * Vec4::new(p.x, p.y, p.z, 1.0);
        Vec3::new(v.x / v.w, v.y / v.w, v.z / v.w)
    }

    /// Le centre de la zone ombrée doit tomber au milieu de la carte, à mi-profondeur.
    #[test]
    fn le_centre_du_monde_tombe_au_milieu_de_la_carte() {
        let m = matrice_lumiere(Vec3::new(0.4, 0.9, 0.7), Vec3::new(10.0, 5.0, 0.0), 20.0);
        let c = projete(m, Vec3::new(10.0, 5.0, 0.0));
        assert!(c.x.abs() < 1e-4 && c.y.abs() < 1e-4, "le centre doit etre au milieu : {c:?}");
        assert!(
            (c.z - 0.5).abs() < 1e-4,
            "le centre est a mi-chemin du volume, donc a 0,5 de profondeur : {c:?}"
        );
    }

    /// ⚠ LE TEST QUI MORD VRAIMENT : ce qui est PLUS PRÈS de la lumière doit avoir une profondeur
    /// PLUS PETITE. Sans ça, la comparaison d'ombre est inversée — et une scène entièrement dans
    /// l'ombre ou entièrement éclairée ressemble à un problème de filtrage, pas à un signe retourné.
    #[test]
    fn ce_qui_est_plus_pres_de_la_lumiere_a_une_profondeur_plus_petite() {
        let vers_la_lumiere = Vec3::new(0.0, 1.0, 0.0);
        let centre = Vec3::ZERO;
        let m = matrice_lumiere(vers_la_lumiere, centre, 10.0);

        let haut = projete(m, Vec3::new(0.0, 5.0, 0.0)); // plus pres du soleil
        let bas = projete(m, Vec3::new(0.0, -5.0, 0.0)); // plus loin
        assert!(
            haut.z < bas.z,
            "profondeur inversee : haut={:.4} devrait etre < bas={:.4}",
            haut.z, bas.z
        );
    }

    /// Une lumière au zénith ne doit pas produire une matrice invalide — le `look_at` dégénère si
    /// la direction est colinéaire au vecteur « haut », et un soleil de midi n'a rien d'exotique.
    #[test]
    fn une_lumiere_au_zenith_ne_degenere_pas() {
        let m = matrice_lumiere(Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, 10.0);
        for colonne in m.cols.iter() {
            for v in [colonne.x, colonne.y, colonne.z, colonne.w] {
                assert!(v.is_finite(), "la matrice contient une valeur non finie : {m:?}");
            }
        }
        let c = projete(m, Vec3::ZERO);
        assert!(c.x.abs() < 1e-4 && c.y.abs() < 1e-4, "{c:?}");
    }

    /// Tout ce qui tient dans la sphère demandée doit tenir dans la carte : sinon des objets
    /// cesseraient de projeter leur ombre sans qu'aucun message ne le dise.
    #[test]
    fn toute_la_sphere_demandee_tient_dans_la_carte() {
        let rayon = 12.0;
        let centre = Vec3::new(-3.0, 7.0, 2.0);
        let m = matrice_lumiere(Vec3::new(0.3, 0.8, 0.5), centre, rayon);

        for (dx, dy, dz) in [
            (rayon, 0.0, 0.0), (-rayon, 0.0, 0.0),
            (0.0, rayon, 0.0), (0.0, -rayon, 0.0),
            (0.0, 0.0, rayon), (0.0, 0.0, -rayon),
        ] {
            let p = projete(m, centre + Vec3::new(dx, dy, dz));
            assert!(
                p.x.abs() <= 1.0001 && p.y.abs() <= 1.0001 && (-1e-4..=1.0001).contains(&p.z),
                "le point ({dx}, {dy}, {dz}) sort de la carte : {p:?}"
            );
        }
    }
}
