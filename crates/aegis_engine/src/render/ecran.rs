//! # L'ÉCRAN — ce qui va de la lumière au pixel
//!
//! Né le 30 août 2026. La scène ne se dessine plus dans l'image montrée : elle se dessine dans
//! une image **HDR** (`cibles.rs`), et ce module la porte à l'écran.
//!
//! ## Pourquoi une passe de plus, alors qu'on venait d'en supprimer 3 400
//!
//! Parce que ce n'est pas une passe *par objet* mais une passe *par image* : son coût ne dépend
//! ni du nombre de cubes ni de la complexité de la scène, seulement du nombre de pixels. C'est la
//! différence entre un coût qui grandit avec le monde et un coût fixe — et c'est ce qui la rend
//! payable là où 3 458 appels de dessin ne l'étaient pas.
//!
//! ## ⭐ Ce qu'elle referme, et qui n'était pas le sujet
//!
//! La courbe de tonalité vivait dans **deux** shaders, avec un test pour surveiller qu'un
//! troisième ne l'oublie pas. Elle vit maintenant à un seul endroit, traversé par tout ce qui va
//! à l'écran. *Une garde qui surveille une duplication est l'aveu que la duplication existe ; ici
//! elle n'a plus rien à surveiller.*
//!
//! ## Ce qui n'est PAS ici
//!
//! Le HUD. Il se dessine **après** la composition, dans la même passe, mais sans passer par la
//! courbe : une interface n'est pas dans la scène, ne reçoit aucune lumière, et un texte blanc
//! courbé par une exposition sortirait gris. C'est le jeu qui l'émet — ce module ne connaît que
//! le passage de la lumière au pixel.

use crate::core::memory::MemoryManager;
use crate::render::pipeline::{Melange, PipelineFactory, Reglages};
use crate::GpuContext;
use ash::vk;

/// La plus petite image qui porte encore de l'information spatiale.
///
/// C'est ce qui décide de la profondeur de la chaîne : on descend jusqu'à passer sous ce seuil.
/// Le rayon du halo est donc une **fraction fixe de l'écran** — le même effet à 1080p et sur un
/// casque — au lieu d'un nombre de pixels qui rétrécirait quand la fenêtre grandit.
const PLUS_PETIT_NIVEAU: u32 = 8;

/// Plafond du nombre d'octaves.
///
/// ⚠ C'est le seul chiffre choisi de cette chaîne, et il se justifie : les poids valant 1/2^k, un
/// septième niveau pèserait 1/128. Sur un écran à 8 bits par canal, sa contribution passe sous le
/// pas de quantification (1/255) pour tout débordement en dessous de deux — c'est-à-dire qu'elle
/// serait calculée et *invisible*. Chaque niveau coûte par ailleurs deux passes, dont le coût
/// fixe cesse d'être amorti quand l'image ne fait plus que quelques pixels.
const OCTAVES_MAX: usize = 6;

/// Combien de niveaux la chaîne compte pour une image de cette taille.
///
/// Fonction **pure** : c'est elle qui décide, et elle se teste sans le moindre GPU.
pub fn octaves(largeur: u32, hauteur: u32) -> usize {
    let mut compte = 0;
    let mut cote = largeur.min(hauteur);
    while cote / 2 >= PLUS_PETIT_NIVEAU && compte < OCTAVES_MAX {
        cote /= 2;
        compte += 1;
    }
    compte
}

/// Le halo est-il allumé ?
///
/// ⚠ `AEGIS_HALO=0` l'éteint. C'est un **interrupteur de BANC**, comme `AEGIS_ECHANTILLONS` pour
/// l'anti-crénelage : il sert à mesurer ce que l'effet coûte réellement en comparant deux
/// exécutions du *même* binaire, et à donner la preuve visuelle qu'il fait bien quelque chose.
/// Ce n'est pas un réglage de qualité offert au joueur — un réglage se pose dans le jeu, pas dans
/// l'environnement.
///
/// Une valeur illisible allume plutôt que d'être fatale, et **le journal le DIT** : une variable
/// mal tapée qui éteint silencieusement un effet ferait chercher un défaut ailleurs pendant des
/// heures.
fn halo_allume() -> bool {
    match std::env::var("AEGIS_HALO") {
        Err(_) => true,
        Ok(texte) => match texte.trim() {
            "0" => {
                log::info!("AEGIS_HALO=0 — le halo est eteint (interrupteur de banc)");
                false
            }
            "1" => true,
            autre => {
                log::warn!("AEGIS_HALO={autre:?} n'est ni 0 ni 1 — le halo reste allume");
                true
            }
        },
    }
}

/// Ce qu'une passe plein écran a besoin de savoir.
///
/// ⚠ Un type nommé plutôt que cinq arguments de suite, et pour la raison exacte qui a fait naître
/// `Reglages` : une vue d'image, un descripteur et une étendue se ressemblent tous à l'appel.
/// Intervertir la cible et la source donne un rendu parfaitement valide qui lit ce qu'il écrit,
/// et **rien ne le signale** — la carte n'a aucun moyen de savoir que ce n'était pas voulu.
struct Passe {
    pipeline: vk::Pipeline,
    /// L'image que le shader LIT.
    lecture: vk::DescriptorSet,
    /// L'image que la passe ÉCRIT.
    cible: vk::ImageView,
    etendue: vk::Extent2D,
    /// `LOAD` quand le mélange a besoin de ce qui est déjà là, `DONT_CARE` quand on recouvre tout.
    charger: vk::AttachmentLoadOp,
}

/// Une image intermédiaire du halo, avec sa taille.
struct Niveau {
    image: vk::Image,
    memoire: vk::DeviceMemory,
    vue: vk::ImageView,
    etendue: vk::Extent2D,
    /// Le descripteur qui pointe sur CE niveau, pour que la passe suivante le lise.
    lecture: vk::DescriptorSet,
}

/// La chaîne qui porte la scène à l'écran.
pub struct Ecran {
    /// ⚠ Un échantillonneur **linéaire** et **borné aux bords**. Le filtrage linéaire n'est pas
    /// un luxe : c'est lui qui fait qu'une lecture entre deux texels moyenne quatre voisins
    /// gratuitement. Le bornage évite qu'un pixel de bord aille chercher de l'autre côté de
    /// l'image — ce qui se voit comme une bavure lumineuse le long du cadre.
    echantillonneur: vk::Sampler,
    /// La description d'« une image à lire ». Le même pour toutes les passes d'écran.
    layout_descripteur: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    /// L'ensemble qui pointe sur la scène résolue. ⚠ Réécrit à chaque redimensionnement : la vue
    /// change avec la taille, et un descripteur qui pointe sur une image détruite est un défaut
    /// que la carte n'est pas tenue de signaler.
    scene: vk::DescriptorSet,
    layout_pipeline: vk::PipelineLayout,
    composition: vk::Pipeline,
    /// Les octaves du halo, du plus grand (moitié de l'écran) au plus petit.
    niveaux: Vec<Niveau>,
    extraction: vk::Pipeline,
    descente: vk::Pipeline,
    /// La remontée ordinaire : moitié-moitié avec l'octave déjà présente.
    montee: vk::Pipeline,
    /// La dernière remontée, vers la scène : additive, au format de la scène.
    montee_finale: vk::Pipeline,
    /// Lu une seule fois au démarrage — voir `halo_allume`.
    halo: bool,
}

impl Ecran {
    /// Combien d'ensembles de descripteurs la réserve doit contenir : la scène, plus un par
    /// octave possible. Dimensionnée pour le maximum — une réserve ne se réalloue pas.
    const ENSEMBLES: u32 = 1 + OCTAVES_MAX as u32;

    pub fn nouveau(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cibles: &crate::render::cibles::Cibles,
        layout_cadre: vk::DescriptorSetLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let echantillonneur = unsafe {
            gpu.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        let liaisons = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let layout_descripteur = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&liaisons),
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

        let scene = unsafe {
            gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&layout_descripteur)),
            )?[0]
        };

        // ⚠ DEUX ensembles, dans cet ordre : le cadre (l'exposition, le point blanc) puis l'image
        // à lire. Les intervertir donne un pipeline qui se crée sans erreur et lit n'importe quoi.
        let layout_pipeline = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            &[layout_cadre, layout_descripteur],
            &[],
        )?;

        let vert = PipelineFactory::create_shader_module_from_bytes(
            &gpu.device,
            crate::shaders::COMPOSITION_VERT_SPV,
        )?;
        let frag = PipelineFactory::create_shader_module_from_bytes(
            &gpu.device,
            crate::shaders::COMPOSITION_FRAG_SPV,
        )?;

        let composition = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            layout_pipeline,
            vert,
            frag,
            Reglages {
                // ⚠ Le format de l'ÉCRAN, pas celui de la scène : c'est la seule passe du moteur
                // qui écrit dans l'image présentée.
                color_format: gpu.swapchain_format,
                second_format: None,
                depth_format: None,
                depth_write: false,
                melange: Melange::Aucun,
                use_vertex_input: false,
                // ⚠ Un seul échantillon : la moyenne a déjà eu lieu, à la fin de la passe de
                // scène. Anti-créneler une image plein écran qui n'a aucune arête géométrique
                // serait payer quatre fois pour rien.
                echantillons: vk::SampleCountFlags::TYPE_1,
            },
        )?;

        unsafe {
            gpu.device.destroy_shader_module(vert, None);
            gpu.device.destroy_shader_module(frag, None);
        }

        // ── LES TROIS PIPELINES DU HALO ─────────────────────────────────────────────────────
        //
        // Quatre en réalité : la remontée existe en deux exemplaires, même shader, mélanges
        // différents. Les intermédiaires se mêlent moitié-moitié à l'octave déjà là ; la dernière
        // s'AJOUTE à la scène — on ne mélange pas la lumière du monde à moitié avec son halo.
        let monter = |source: &[u8], reglages: Reglages| -> Result<vk::Pipeline, Box<dyn std::error::Error>> {
            let m = PipelineFactory::create_shader_module_from_bytes(&gpu.device, source)?;
            let p = PipelineFactory::create_graphics_pipeline(
                &gpu.device, layout_pipeline, m, m, reglages,
            )?;
            unsafe { gpu.device.destroy_shader_module(m, None) };
            Ok(p)
        };
        // Tout le halo vit dans le format de la scène, à un seul échantillon et sans profondeur.
        let dans_la_scene = |melange| Reglages {
            color_format: cibles.format_hdr,
            second_format: None,
            depth_format: None,
            depth_write: false,
            melange,
            use_vertex_input: false,
            echantillons: vk::SampleCountFlags::TYPE_1,
        };

        let extraction = monter(crate::shaders::HALO_EXTRACTION_SPV, dans_la_scene(Melange::Aucun))?;
        let descente = monter(crate::shaders::HALO_DESCENTE_SPV, dans_la_scene(Melange::Aucun))?;
        let montee = monter(crate::shaders::HALO_MONTEE_SPV, dans_la_scene(Melange::Moitie))?;
        let montee_finale =
            monter(crate::shaders::HALO_MONTEE_SPV, dans_la_scene(Melange::Additif))?;

        let mut ecran = Self {
            echantillonneur,
            layout_descripteur,
            pool,
            scene,
            layout_pipeline,
            composition,
            niveaux: Vec::new(),
            extraction,
            descente,
            montee,
            montee_finale,
            halo: halo_allume(),
        };
        ecran.batir_les_niveaux(gpu, memory_props, cibles)?;
        ecran.brancher(&gpu.device, cibles);
        Ok(ecran)
    }

    /// Alloue les octaves du halo à la taille courante, chacune moitié de la précédente.
    ///
    /// ⚠ Elles sont allouées **même quand le halo est éteint**, et c'est délibéré : `halo on` doit
    /// pouvoir le rallumer en cours de partie, ce qui est toute la raison d'être de cette bascule.
    /// Les allouer paresseusement ferait de la première image après l'allumage un pic de plusieurs
    /// millisecondes, au moment précis où l'on cherche à mesurer un écart.
    fn batir_les_niveaux(
        &mut self,
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cibles: &crate::render::cibles::Cibles,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (largeur, hauteur) = (gpu.swapchain_extent.width, gpu.swapchain_extent.height);
        let combien = octaves(largeur, hauteur);

        for i in 0..combien {
            let etendue = vk::Extent2D {
                width: (largeur >> (i + 1)).max(1),
                height: (hauteur >> (i + 1)).max(1),
            };

            let image = unsafe {
                gpu.device.create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(cibles.format_hdr)
                        .extent(vk::Extent3D {
                            width: etendue.width,
                            height: etendue.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        // Écrite par une passe, relue par la suivante : ni passagère, ni
                        // paresseusement allouée. Une octave doit exister pour de bon.
                        .usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                        )
                        .sharing_mode(vk::SharingMode::EXCLUSIVE)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )?
            };
            let besoins = unsafe { gpu.device.get_image_memory_requirements(image) };
            let type_memoire = MemoryManager::find_memory_type(
                memory_props,
                besoins.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .ok_or("aucune memoire ne convient pour une octave du halo")?;
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
                        .format(cibles.format_hdr)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?
            };

            let lecture = unsafe {
                gpu.device.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.pool)
                        .set_layouts(std::slice::from_ref(&self.layout_descripteur)),
                )?[0]
            };
            self.pointer(&gpu.device, lecture, vue);

            self.niveaux.push(Niveau { image, memoire, vue, etendue, lecture });
        }

        log::info!(
            "Halo : {combien} octave(s), de {}x{} a {}x{}",
            largeur / 2,
            hauteur / 2,
            self.niveaux.last().map(|n| n.etendue.width).unwrap_or(0),
            self.niveaux.last().map(|n| n.etendue.height).unwrap_or(0),
        );
        Ok(())
    }

    /// Fait pointer un ensemble de descripteurs sur une vue d'image.
    fn pointer(&self, device: &ash::Device, ensemble: vk::DescriptorSet, vue: vk::ImageView) {
        let image = vk::DescriptorImageInfo::default()
            .image_view(vue)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let sampler = vk::DescriptorImageInfo::default().sampler(self.echantillonneur);
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

    /// Fait pointer le descripteur sur la scène courante.
    ///
    /// ⚠ **À rappeler après chaque redimensionnement.** L'oublier laisse le descripteur sur une
    /// vue détruite : la carte n'est pas tenue de s'en plaindre, et ce qui s'affiche alors va du
    /// résultat correct à l'écran noir selon le pilote — le pire genre de défaut.
    pub fn brancher(&self, device: &ash::Device, cibles: &crate::render::cibles::Cibles) {
        self.pointer(device, self.scene, cibles.vue_resolue());
    }

    /// Refait les octaves à la taille courante. À appeler avec les cibles déjà recréées.
    pub fn redimensionner(
        &mut self,
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        cibles: &crate::render::cibles::Cibles,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // ⚠ Les ensembles de descripteurs sont rendus à la réserve avec `reset` : les libérer un
        // par un demanderait `FREE_DESCRIPTOR_SET` à la création de la réserve, et une réserve
        // qui autorise la libération individuelle se fragmente. Ici tout repart de zéro à chaque
        // redimensionnement, ce qui est exactement l'usage pour lequel `reset` existe.
        for niveau in self.niveaux.drain(..) {
            unsafe {
                gpu.device.destroy_image_view(niveau.vue, None);
                gpu.device.destroy_image(niveau.image, None);
                gpu.device.free_memory(niveau.memoire, None);
            }
        }
        unsafe {
            gpu.device
                .reset_descriptor_pool(self.pool, vk::DescriptorPoolResetFlags::empty())?;
            self.scene = gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(std::slice::from_ref(&self.layout_descripteur)),
            )?[0];
        }
        self.batir_les_niveaux(gpu, memory_props, cibles)?;
        self.brancher(&gpu.device, cibles);
        Ok(())
    }

    /// Une passe plein écran : ouvre le rendu sur `cible`, lie `lecture`, dessine trois sommets.
    ///
    /// # Safety
    /// `cmd` doit être en cours d'enregistrement, **hors** de tout `cmd_begin_rendering`.
    unsafe fn passe(&self, device: &ash::Device, cmd: vk::CommandBuffer, passe: Passe) {
        let Passe { pipeline, lecture, cible, etendue, charger } = passe;
        unsafe {
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(cible)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(charger)
                .store_op(vk::AttachmentStoreOp::STORE);
            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            // ⚠ Le viewport suit la taille de CETTE octave, pas celle de l'écran. L'oublier
            // dessinerait le triangle hors du cadre : les petits niveaux resteraient vides, et le
            // halo s'éteindrait par ses grandes échelles sans qu'aucune erreur ne le dise.
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

    /// Fait passer une image d'attachement à texture, ou l'inverse.
    ///
    /// ⚠ Sans ces barrières, une passe lirait des pixels que la précédente n'a pas fini
    /// d'écrire. Le résultat n'est pas une erreur mais un halo qui traîne d'une image sur
    /// l'autre — le genre de défaut qu'on attribue au filtre pendant des heures.
    unsafe fn transiter(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        de: vk::ImageLayout,
        vers: vk::ImageLayout,
    ) {
        let (src_acces, dst_acces, src_etape, dst_etape) = if vers
            == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        {
            (
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            )
        } else {
            (
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            )
        };
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                src_etape,
                dst_etape,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .old_layout(de)
                    .new_layout(vers)
                    .src_access_mask(src_acces)
                    .dst_access_mask(dst_acces)
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

    /// Ajoute son halo à la scène.
    ///
    /// La descente retient ce qui déborde du blanc affichable et le réduit d'octave en octave ; la
    /// remontée superpose les échelles ; la dernière marche ajoute le tout à la scène. Toute la
    /// conception (le seuil, les poids, le rayon — et pourquoi aucun n'est un réglage) vit en tête
    /// de `halo.wgsl`.
    ///
    /// ⚠ **À appeler entre la passe de scène et la composition**, la scène étant déjà en lecture.
    /// Elle en ressort en lecture également, prête pour la composition.
    ///
    /// # Safety
    /// `cmd` doit être en cours d'enregistrement, hors de toute passe de rendu.
    pub unsafe fn diffuser(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        cibles: &crate::render::cibles::Cibles,
        etendue: vk::Extent2D,
    ) {
        if !self.halo {
            return;
        }
        // Une image trop petite pour porter la moindre octave : il n'y a rien à diffuser, et
        // c'est un cas normal (une fenêtre réduite à une bande). Le rendu reste juste, sans halo.
        let Some((premier, suite)) = self.niveaux.split_first() else { return };

        unsafe {
            // ── LA DESCENTE ──────────────────────────────────────────────────────────────────
            Self::transiter(
                device, cmd, premier.image,
                vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );
            self.passe(device, cmd, Passe {
                pipeline: self.extraction,
                lecture: self.scene,
                cible: premier.vue,
                etendue: premier.etendue,
                // Rien à charger : la passe recouvre chaque pixel de sa cible.
                charger: vk::AttachmentLoadOp::DONT_CARE,
            });
            Self::transiter(
                device, cmd, premier.image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            for (i, niveau) in suite.iter().enumerate() {
                let source = &self.niveaux[i];
                Self::transiter(
                    device, cmd, niveau.image,
                    vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                );
                self.passe(device, cmd, Passe {
                    pipeline: self.descente,
                    lecture: source.lecture,
                    cible: niveau.vue,
                    etendue: niveau.etendue,
                    charger: vk::AttachmentLoadOp::DONT_CARE,
                });
                Self::transiter(
                    device, cmd, niveau.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                );
            }

            // ── LA REMONTÉE ──────────────────────────────────────────────────────────────────
            //
            // ⚠ `LOAD` et non `DONT_CARE` : le mélange moitié-moitié a besoin de ce qui est déjà
            // dans l'octave — c'est la moitié du résultat. `DONT_CARE` la ferait mélanger avec du
            // vide, et le halo perdrait ses petites échelles, donc son cœur serré.
            for i in (0..self.niveaux.len() - 1).rev() {
                let cible = &self.niveaux[i];
                let source = &self.niveaux[i + 1];
                Self::transiter(
                    device, cmd, cible.image,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                );
                self.passe(device, cmd, Passe {
                    pipeline: self.montee,
                    lecture: source.lecture,
                    cible: cible.vue,
                    etendue: cible.etendue,
                    charger: vk::AttachmentLoadOp::LOAD,
                });
                Self::transiter(
                    device, cmd, cible.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                );
            }

            // ── LA DERNIÈRE MARCHE : LE HALO REJOINT LA SCÈNE ────────────────────────────────
            //
            // Additive, et non moitié-moitié : on n'atténue pas le monde de moitié pour y poser
            // sa propre lueur. La scène repasse en attachement le temps de la recevoir.
            Self::transiter(
                device, cmd, cibles.image_resolue(),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );
            // ⚠ L'étendue de la SCÈNE, pas le double de la première octave : une résolution
            // impaire perd un pixel à chaque division, et la colonne de droite resterait alors
            // sans halo. Un liseré d'un pixel, qui ne se verrait qu'à certaines tailles de
            // fenêtre — donc jamais pendant qu'on le cherche.
            self.passe(device, cmd, Passe {
                pipeline: self.montee_finale,
                lecture: premier.lecture,
                cible: cibles.vue_resolue(),
                etendue,
                charger: vk::AttachmentLoadOp::LOAD,
            });
            Self::transiter(
                device, cmd, cibles.image_resolue(),
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
    }

    /// Allume ou éteint le halo, en direct.
    ///
    /// ⚠ **C'est le MÊME état que `AEGIS_HALO`, pas un second.** La variable d'environnement pose
    /// la valeur de départ, celle-ci la change en cours de route. Deux drapeaux séparés auraient
    /// fini par se contredire, et il aurait fallu deviner lequel gagne.
    ///
    /// Son intérêt tient à ce qu'une seule mesure ne prouve rien ici : deux exécutions du jeu ne
    /// montrent PAS la même image, parce que la simulation avance à l'horloge murale et qu'une
    /// image de numéro identique tombe à un instant différent. *Comparer avec et sans, dans la
    /// même exécution, est le seul moyen d'attribuer un écart au halo plutôt qu'au temps.*
    pub fn allumer(&mut self, allume: bool) {
        self.halo = allume;
    }

    /// Le halo est-il allumé ?
    pub fn halo_allume(&self) -> bool {
        self.halo
    }

    /// Le contrat « une image à lire », partagé par TOUTES les passes plein écran du moteur.
    ///
    /// ⚠ L'occlusion le réutilise plutôt que d'en définir un second : deux descriptions du même
    /// contrat finiraient par diverger, et rien ne le signalerait avant que la carte lise à côté.
    pub fn layout_descripteur(&self) -> vk::DescriptorSetLayout {
        self.layout_descripteur
    }

    /// Le layout que le jeu doit employer pour dessiner son interface dans la passe de l'écran.
    ///
    /// ⚠ Le HUD ne peut PAS réutiliser celui de la scène : il n'a pas les mêmes ensembles. Rendre
    /// celui-ci évite au jeu d'en fabriquer un troisième qui devrait rester d'accord avec les deux.
    pub fn layout_pipeline(&self) -> vk::PipelineLayout {
        self.layout_pipeline
    }

    /// Porte la scène à l'écran. **À appeler dans une passe déjà ouverte sur l'image présentée.**
    ///
    /// Le cadre doit avoir été lié à l'ensemble 0 par l'appelant : c'est lui qui porte l'exposition
    /// et le point blanc, et il appartient au jeu, pas à ce module.
    ///
    /// # Safety
    /// `cmd` doit être en cours d'enregistrement, à l'intérieur d'un `cmd_begin_rendering`.
    pub unsafe fn composer(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.composition);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout_pipeline,
                1,
                &[self.scene],
                &[],
            );
            crate::mesure::noter_dessin(1);
            device.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            for niveau in &self.niveaux {
                device.destroy_image_view(niveau.vue, None);
                device.destroy_image(niveau.image, None);
                device.free_memory(niveau.memoire, None);
            }
            for pipeline in [
                self.composition,
                self.extraction,
                self.descente,
                self.montee,
                self.montee_finale,
            ] {
                device.destroy_pipeline(pipeline, None);
            }
            device.destroy_pipeline_layout(self.layout_pipeline, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout_descripteur, None);
            device.destroy_sampler(self.echantillonneur, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⭐ Le rayon du halo est une FRACTION de l'écran, pas un nombre de pixels.
    ///
    /// Doubler la résolution ajoute exactement une octave — donc la plus grande échelle de
    /// diffusion couvre toujours la même part de l'image. C'est ce qui fait qu'un halo réglé sur
    /// un écran reste le même sur un autre, sans qu'aucun chiffre ne soit à reprendre.
    #[test]
    fn doubler_la_resolution_ajoute_une_octave() {
        assert_eq!(octaves(256, 256) + 1, octaves(512, 512));
        assert_eq!(octaves(64, 64) + 1, octaves(128, 128));
    }

    /// La descente s'arrête avant de produire une image sous le seuil de lisibilité.
    #[test]
    fn aucune_octave_ne_descend_sous_le_plus_petit_niveau() {
        for (largeur, hauteur) in [(1920, 1080), (800, 600), (320, 240), (100, 37)] {
            let combien = octaves(largeur, hauteur);
            let plus_petit = largeur.min(hauteur) >> combien as u32;
            assert!(
                combien == 0 || plus_petit >= PLUS_PETIT_NIVEAU,
                "{largeur}x{hauteur} : {combien} octaves donnent un cote de {plus_petit}"
            );
        }
    }

    /// ⚠ Le cas qui compte le plus : une fenêtre minuscule ne doit produire AUCUNE octave, et
    /// surtout pas une image de zéro pixel — que Vulkan refuse d'allouer.
    ///
    /// *Le rendu reste alors parfaitement juste, simplement sans halo.* Un moteur qui refuse de
    /// dessiner parce que la fenêtre est étroite a raté sa cible bien plus gravement qu'un effet
    /// manquant.
    #[test]
    fn une_fenetre_minuscule_ne_produit_aucune_octave() {
        for (largeur, hauteur) in [(1, 1), (8, 8), (15, 400), (1920, 3)] {
            assert_eq!(
                octaves(largeur, hauteur),
                0,
                "{largeur}x{hauteur} ne peut porter aucune octave"
            );
        }
    }

    /// Le plafond est un plafond, quelle que soit la taille de l'écran.
    #[test]
    fn le_nombre_d_octaves_reste_borne() {
        assert!(octaves(u32::MAX, u32::MAX) <= OCTAVES_MAX);
        assert_eq!(octaves(7680, 4320), OCTAVES_MAX);
    }
}
