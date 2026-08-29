//! # LES CIBLES DE RENDU — là où l'image se fabrique avant d'être montrée
//!
//! Né le 29 août 2026, étape B du chantier du rendu. Il règle **deux choses d'un coup**, et la
//! seconde n'était pas cherchée.
//!
//! ## 1. Le problème cherché : les arêtes en escalier
//!
//! Le moteur rasterisait à **un seul échantillon par pixel**. Chaque arête de chaque cube était
//! donc un escalier dur, et sur du voxel — une image qui n'est faite que d'arêtes — c'est
//! probablement le plus gros écart visuel pour le moins de travail.
//!
//! ⭐ **Et le MSAA est le seul anti-crénelage qui serve les deux bouts du cadrage matériel.** Un
//! GPU à tuiles (tous les mobiles, le Meta Quest 2 qui est la machine de référence) garde les
//! échantillons en mémoire de tuile et les résout **sans jamais les écrire en mémoire centrale** :
//! le surcoût y est en calcul, pas en bande passante — la ressource rare sur ces machines. Sur une
//! RTX 4090 il est simplement payé. À l'inverse, un TAA a besoin d'un historique d'images, ce qui
//! produit des traînées dès qu'on tourne la tête — inacceptable en VR — et un FXAA floute le
//! détail au lieu de le résoudre, c'est-à-dire qu'il retire la clarté qu'on cherche.
//!
//! ## 2. Le problème trouvé en chemin
//!
//! La création de l'image de profondeur était **écrite deux fois** dans le fichier de rendu du
//! JEU : une fois à l'ouverture, une fois au redimensionnement, quarante lignes chacune. Ajouter
//! une image couleur multi-échantillonnée en aurait fait quatre copies.
//!
//! *La question n'était donc pas « qu'est-ce qui manque ici » mais « qu'est-ce qui n'aurait jamais
//! dû vivre là ».* Allouer une image Vulkan n'est pas une décision de jeu : aucune n'a de couleur,
//! de forme ni de goût. Ce fichier les reprend, et le jeu n'a plus qu'à demander une image.
//!
//! ## Ce qui n'est PAS garanti
//!
//! Le gain visuel est prouvé par l'œil, pas par un test — comme tout rendu perçu. Ce qui est
//! vérifié ici de façon déterministe est le **choix** du nombre d'échantillons : qu'il ne demande
//! jamais plus que ce que la carte annonce, et qu'il retombe proprement à un seul échantillon sur
//! une machine qui ne sait rien faire d'autre.

use crate::core::memory::MemoryManager;
use crate::GpuContext;
use ash::vk;

/// Combien d'échantillons par pixel on demande quand la carte le permet.
///
/// ⚠ **Ce n'est pas un réglage de qualité, c'est un plafond.** La valeur retenue est toujours le
/// plus grand niveau *supporté* qui ne dépasse pas celui-ci — donc monter ce chiffre ne force
/// rien, et une machine modeste retombe d'elle-même. Quatre est le point d'équilibre habituel : au
/// delà, l'œil distingue mal et le coût continue de monter.
pub const ECHANTILLONS_VOULUS: u32 = 4;

/// Choisit le nombre d'échantillons : le plus grand niveau supporté qui ne dépasse pas `voulus`.
///
/// ⚠ **Un niveau doit être supporté par la couleur ET par la profondeur.** Les deux limites sont
/// distinctes dans Vulkan, et une carte peut annoncer 8 pour l'une et 4 pour l'autre — attacher
/// deux images d'échantillonnages différents à la même passe est un défaut que le pilote n'est pas
/// tenu de signaler. L'appelant passe donc déjà l'intersection des deux.
///
/// Fonction **pure** : c'est elle qui décide, et elle se teste sans le moindre GPU.
pub fn echantillons_retenus(supportes: vk::SampleCountFlags, voulus: u32) -> vk::SampleCountFlags {
    // Du plus grand au plus petit : le premier qui tient les deux conditions gagne.
    const NIVEAUX: [(u32, vk::SampleCountFlags); 6] = [
        (64, vk::SampleCountFlags::TYPE_64),
        (32, vk::SampleCountFlags::TYPE_32),
        (16, vk::SampleCountFlags::TYPE_16),
        (8, vk::SampleCountFlags::TYPE_8),
        (4, vk::SampleCountFlags::TYPE_4),
        (2, vk::SampleCountFlags::TYPE_2),
    ];

    for (combien, drapeau) in NIVEAUX {
        if combien <= voulus && supportes.contains(drapeau) {
            return drapeau;
        }
    }
    // Un seul échantillon est toujours possible : c'est le rendu sans anti-crénelage, et c'est
    // aussi la sortie de secours d'un GPU qui n'annonce rien. Jamais d'échec ici.
    vk::SampleCountFlags::TYPE_1
}

/// Une image allouée avec sa mémoire et sa vue — les trois vont toujours ensemble.
struct ImageAllouee {
    image: vk::Image,
    memoire: vk::DeviceMemory,
    vue: vk::ImageView,
}

impl ImageAllouee {
    fn creer(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        echantillons: vk::SampleCountFlags,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: gpu.swapchain_extent.width,
                height: gpu.swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(echantillons)
            .tiling(vk::ImageTiling::OPTIMAL)
            // ⚠ `TRANSIENT_ATTACHMENT` est ce qui rend le MSAA presque gratuit sur mobile : il
            // annonce au pilote que cette image ne quitte jamais la passe. Un GPU à tuiles peut
            // alors la garder en mémoire de tuile et ne jamais l'écrire en mémoire centrale.
            // Sans lui, on paierait la bande passante de quatre images entières — exactement ce
            // que la machine de référence ne peut pas se permettre.
            .usage(usage | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { gpu.device.create_image(&info, None)? };
        let besoins = unsafe { gpu.device.get_image_memory_requirements(image) };

        // `LAZILY_ALLOCATED` va avec `TRANSIENT_ATTACHMENT` : sur les cartes qui le proposent
        // (les mobiles), la mémoire n'est jamais réellement engagée. Ailleurs elle n'existe pas,
        // et on retombe sur de la mémoire ordinaire — la même image, simplement payée.
        let type_memoire = MemoryManager::find_memory_type(
            memory_props,
            besoins.memory_type_bits,
            vk::MemoryPropertyFlags::LAZILY_ALLOCATED,
        )
        .or_else(|| {
            MemoryManager::find_memory_type(
                memory_props,
                besoins.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
        })
        .ok_or("aucune memoire ne convient pour une cible de rendu")?;

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
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?
        };

        Ok(Self { image, memoire, vue })
    }

    fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.vue, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memoire, None);
        }
    }
}

/// Ce dans quoi une image se dessine avant d'être montrée.
pub struct Cibles {
    /// Combien d'échantillons par pixel, réellement retenus par cette carte.
    pub echantillons: vk::SampleCountFlags,
    pub format_profondeur: vk::Format,
    /// L'image couleur multi-échantillonnée — **absente quand un seul échantillon suffit**, car on
    /// dessine alors directement dans l'image présentée. Le chemin sans anti-crénelage ne coûte
    /// donc pas une image de plus : il n'en a simplement pas.
    couleur: Option<ImageAllouee>,
    profondeur: ImageAllouee,
}

impl Cibles {
    pub fn nouvelles(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let format_profondeur = vk::Format::D32_SFLOAT;

        // L'intersection des deux limites : un niveau que la couleur seule supporte ne sert à
        // rien si la profondeur ne le suit pas.
        let supportes = gpu.proprietes.limits.framebuffer_color_sample_counts
            & gpu.proprietes.limits.framebuffer_depth_sample_counts;

        // ⚠ `AEGIS_ECHANTILLONS=1` abaisse le plafond. C'est un interrupteur de BANC — il sert a
        // mesurer ce que l'anti-crenelage coute reellement, en comparant deux executions du meme
        // binaire — et une sortie de secours si un pilote se comporte mal. Ce n'est pas un reglage
        // de qualite offert au joueur : un reglage se pose dans le jeu, pas dans l'environnement.
        //
        // Une valeur illisible est ignoree plutot que fatale, et le journal le DIT : une variable
        // mal tapee qui bride silencieusement le rendu ferait chercher un defaut ailleurs pendant
        // des heures.
        let plafond = match std::env::var("AEGIS_ECHANTILLONS") {
            Err(_) => ECHANTILLONS_VOULUS,
            Ok(texte) => match texte.trim().parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    log::warn!(
                        "AEGIS_ECHANTILLONS={texte:?} n'est pas un nombre — on garde {ECHANTILLONS_VOULUS}"
                    );
                    ECHANTILLONS_VOULUS
                }
            },
        };

        let echantillons = echantillons_retenus(supportes, plafond);

        Self::batir(gpu, memory_props, echantillons, format_profondeur)
    }

    fn batir(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        echantillons: vk::SampleCountFlags,
        format_profondeur: vk::Format,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let couleur = if echantillons == vk::SampleCountFlags::TYPE_1 {
            None
        } else {
            Some(ImageAllouee::creer(
                gpu,
                memory_props,
                gpu.swapchain_format,
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                echantillons,
            )?)
        };

        let profondeur = ImageAllouee::creer(
            gpu,
            memory_props,
            format_profondeur,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
            echantillons,
        )?;

        let combien = 1u32 << (echantillons.as_raw().trailing_zeros());
        log::info!(
            "Cibles de rendu : {}x{}, {combien} echantillon(s) par pixel",
            gpu.swapchain_extent.width,
            gpu.swapchain_extent.height
        );

        Ok(Self { echantillons, format_profondeur, couleur, profondeur })
    }

    /// La vue dans laquelle la passe écrit ses couleurs.
    ///
    /// C'est l'image multi-échantillonnée s'il y en a une, et **l'image présentée sinon** — d'où
    /// le paramètre. Un seul chemin de rendu sert les deux cas, sans condition ailleurs.
    pub fn vue_couleur(&self, image_presentee: vk::ImageView) -> vk::ImageView {
        match &self.couleur {
            Some(c) => c.vue,
            None => image_presentee,
        }
    }

    /// L'image multi-échantillonnée, quand elle existe — la barrière de disposition en a besoin.
    pub fn image_couleur(&self) -> Option<vk::Image> {
        self.couleur.as_ref().map(|c| c.image)
    }

    pub fn vue_profondeur(&self) -> vk::ImageView {
        self.profondeur.vue
    }

    pub fn image_profondeur(&self) -> vk::Image {
        self.profondeur.image
    }

    /// Vrai quand la passe doit résoudre les échantillons vers l'image présentée.
    pub fn resout(&self) -> bool {
        self.couleur.is_some()
    }

    /// Refait les images à la taille courante de la fenêtre.
    ///
    /// ⚠ L'appelant doit avoir attendu que la carte soit au repos : détruire une image encore
    /// utilisée par une image en vol est un défaut que rien ne signale tout de suite.
    pub fn recreer(
        &mut self,
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let echantillons = self.echantillons;
        let format = self.format_profondeur;
        self.detruire(&gpu.device);
        *self = Self::batir(gpu, memory_props, echantillons, format)?;
        Ok(())
    }

    pub fn detruire(&self, device: &ash::Device) {
        if let Some(c) = &self.couleur {
            c.detruire(device);
        }
        self.profondeur.detruire(device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On ne demande jamais plus que ce que la carte annonce.
    #[test]
    fn on_ne_demande_jamais_plus_que_ce_qui_est_supporte() {
        // Une carte qui ne sait faire que 1x et 2x, alors qu'on voudrait 4x.
        let supportes = vk::SampleCountFlags::TYPE_1 | vk::SampleCountFlags::TYPE_2;
        assert_eq!(
            echantillons_retenus(supportes, 4),
            vk::SampleCountFlags::TYPE_2,
            "il faut retomber sur le meilleur niveau REELLEMENT disponible"
        );
    }

    /// Le plafond est un plafond : une carte capable de 8x n'en donne pas plus que demandé.
    #[test]
    fn le_plafond_demande_n_est_jamais_depasse() {
        let supportes = vk::SampleCountFlags::TYPE_1
            | vk::SampleCountFlags::TYPE_2
            | vk::SampleCountFlags::TYPE_4
            | vk::SampleCountFlags::TYPE_8;
        assert_eq!(echantillons_retenus(supportes, 4), vk::SampleCountFlags::TYPE_4);
        assert_eq!(echantillons_retenus(supportes, 8), vk::SampleCountFlags::TYPE_8);
        assert_eq!(echantillons_retenus(supportes, 2), vk::SampleCountFlags::TYPE_2);
    }

    /// ⚠ Le cas qui compte le plus : une carte qui n'annonce rien ne doit pas faire échouer le
    /// moteur, elle doit le faire dessiner sans anti-crénelage. *Un moteur qui refuse de démarrer
    /// sur une machine modeste a raté sa cible bien plus gravement qu'un escalier sur une arête.*
    #[test]
    fn une_carte_qui_n_annonce_rien_retombe_sur_un_seul_echantillon() {
        assert_eq!(
            echantillons_retenus(vk::SampleCountFlags::empty(), 4),
            vk::SampleCountFlags::TYPE_1
        );
        assert_eq!(
            echantillons_retenus(vk::SampleCountFlags::TYPE_1, 4),
            vk::SampleCountFlags::TYPE_1
        );
    }

    /// Demander zéro échantillon n'a pas de sens et ne doit pas produire de niveau invalide.
    #[test]
    fn demander_moins_d_un_echantillon_reste_un_echantillon() {
        let tout = vk::SampleCountFlags::TYPE_1
            | vk::SampleCountFlags::TYPE_2
            | vk::SampleCountFlags::TYPE_4;
        assert_eq!(echantillons_retenus(tout, 0), vk::SampleCountFlags::TYPE_1);
        assert_eq!(echantillons_retenus(tout, 1), vk::SampleCountFlags::TYPE_1);
    }
}
