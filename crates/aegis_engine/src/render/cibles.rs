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
//! ## 3. Ce qui a changé le 30 août 2026 : la scène ne se dessine plus dans l'écran
//!
//! Elle se dessine dans une image **HDR**, et une passe de composition la porte à l'écran. La
//! raison est le halo (`ecran.rs`), mais elle vaut bien au-delà de lui.
//!
//! ⭐ **Une image à 8 bits par canal ne peut pas dire ce qui est LUMINEUX, seulement ce qui est
//! CLAIR.** Elle s'arrête à 1,0, qui est « le blanc de l'écran » — un mur blanc et le soleil y
//! sont la même valeur. Tout effet qui doit distinguer les deux (un halo, une adaptation de
//! l'œil, une exposition automatique) est donc *impossible* après coup, quelle que soit
//! l'ingéniosité qu'on y mette. Ce n'était pas un manque de finition : c'était un mur.
//!
//! Le format retenu — `B10G11R11_UFLOAT` — pèse **exactement autant que le précédent**, quatre
//! octets par pixel, et monte jusqu'à 65 000 au lieu de 1. C'est le format des moteurs mobiles,
//! et pour cette raison précise : sur un GPU à tuiles, la bande passante est la ressource rare,
//! et doubler la taille de l'image de scène aurait coûté plus cher que tout le halo.
//!
//! ⚠ Il n'a **pas de canal alpha**. Aucun mélange du moteur n'en lit un (`SRC_ALPHA` lit celui de
//! la source, pas celui de la destination), mais un futur effet qui voudrait de l'alpha dans la
//! scène devra changer de format — c'est écrit ici pour qu'on le sache avant de le chercher.
//!
//! ## Ce qui n'est PAS garanti
//!
//! Le gain visuel est prouvé par l'œil, pas par un test — comme tout rendu perçu. Ce qui est
//! vérifié ici de façon déterministe est le **choix** du nombre d'échantillons : qu'il ne demande
//! jamais plus que ce que la carte annonce, et qu'il retombe proprement à un seul échantillon sur
//! une machine qui ne sait rien faire d'autre.
//!
//! ⚠ **Et une limite connue de l'anti-crénelage, qui naît le jour même :** la moyenne des
//! échantillons se fait désormais sur des valeurs HDR, *avant* la courbe de tonalité. Sur une
//! arête entre un pixel très lumineux et un pixel sombre, moyenner puis courber ne donne pas la
//! même chose que courber puis moyenner — l'arête ressort un peu plus claire qu'elle ne devrait.
//! L'écart mesuré sur les valeurs de cette scène est de l'ordre de 7 % de luminance, sur les
//! seuls pixels d'arête. C'est le prix connu du HDR, il se paie partout, et le remède (résoudre
//! les échantillons en shader avec une courbe par échantillon) coûte bien plus que ce qu'il rend.

use crate::core::memory::MemoryManager;
use crate::GpuContext;
use ash::vk;

/// Le format dans lequel la scène se dessine, quand la carte l'accepte.
///
/// Trois canaux flottants tenant dans **32 bits** (11+11+10, exposant partagé par canal). Voir la
/// note en tête de fichier : c'est le même poids que l'image d'écran, pour une plage 65 000 fois
/// plus large.
const HDR_COMPACT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;

/// Le repli, quand `HDR_COMPACT` n'est pas utilisable. Deux fois plus lourd, et universel.
const HDR_REPLI: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Ce qu'une cible de scène doit savoir faire.
///
/// ⚠ Les quatre sont nécessaires, et en oublier un donne un défaut *silencieux* : sans
/// `SAMPLED_IMAGE_FILTER_LINEAR` par exemple, la carte a le droit de rendre un échantillonnage
/// au plus proche voisin — le halo devient alors une mosaïque, sans qu'aucune erreur ne soit levée.
const CAPACITES_REQUISES: vk::FormatFeatureFlags = vk::FormatFeatureFlags::from_raw(
    vk::FormatFeatureFlags::COLOR_ATTACHMENT.as_raw()
        | vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND.as_raw()
        | vk::FormatFeatureFlags::SAMPLED_IMAGE.as_raw()
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR.as_raw(),
);

/// Choisit le format de la scène : le compact s'il est pleinement utilisable, le repli sinon.
///
/// Fonction **pure** — elle ne reçoit que ce que la carte annonce, donc elle se teste sans GPU.
pub fn format_hdr(
    compact: vk::FormatFeatureFlags,
    _repli: vk::FormatFeatureFlags,
) -> vk::Format {
    if compact.contains(CAPACITES_REQUISES) {
        HDR_COMPACT
    } else {
        // Aucun test sur le repli : `R16G16B16A16_SFLOAT` est requis par la spécification Vulkan
        // pour ces quatre usages. Le refuser aussi voudrait dire qu'aucun rendu n'est possible —
        // il vaut mieux échouer plus tard, à la création de l'image, avec un message de la carte.
        HDR_REPLI
    }
}

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
    /// `passagere` dit si l'image quitte la passe qui l'écrit. Une image passagère peut ne jamais
    /// toucher la mémoire centrale sur un GPU à tuiles ; une image qu'on relit ensuite (la scène
    /// résolue, que le halo et la composition échantillonnent) ne peut évidemment pas l'être.
    fn creer(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        echantillons: vk::SampleCountFlags,
        passagere: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let usage = if passagere {
            usage | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
        } else {
            usage
        };
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
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { gpu.device.create_image(&info, None)? };
        let besoins = unsafe { gpu.device.get_image_memory_requirements(image) };

        // `LAZILY_ALLOCATED` va avec `TRANSIENT_ATTACHMENT` : sur les cartes qui le proposent
        // (les mobiles), la mémoire n'est jamais réellement engagée. Ailleurs elle n'existe pas,
        // et on retombe sur de la mémoire ordinaire — la même image, simplement payée.
        //
        // ⚠ On ne la demande QUE pour une image passagère : une image qu'on relit doit exister
        // pour de bon. La demander pour tout serait une faute que rien ne signalerait ici — elle
        // se verrait à l'écran, plus tard, sous la forme d'un halo lu dans du vide.
        let type_memoire = Some(())
            .filter(|_| passagere)
            .and_then(|_| {
                MemoryManager::find_memory_type(
                    memory_props,
                    besoins.memory_type_bits,
                    vk::MemoryPropertyFlags::LAZILY_ALLOCATED,
                )
            })
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
    /// Le format de la scène. ⚠ Tout pipeline qui dessine DANS la scène doit le déclarer, et non
    /// celui de l'écran : les deux ne sont plus le même depuis le 30 août 2026.
    pub format_hdr: vk::Format,
    /// L'image couleur multi-échantillonnée — **absente quand un seul échantillon suffit**, car on
    /// dessine alors directement dans la scène résolue. Le chemin sans anti-crénelage ne coûte
    /// donc pas une image de plus : il n'en a simplement pas.
    couleur: Option<ImageAllouee>,
    /// La scène en lumière, après moyenne des échantillons. C'est **elle** que le halo lit et que
    /// la composition porte à l'écran — l'image présentée n'est plus jamais dessinée directement.
    resolue: ImageAllouee,
    /// L'AMBIANTE seule, écrite par la passe de scène en même temps que la lumière totale.
    ///
    /// ## ⭐ Pourquoi une seconde image plutôt qu'un calcul en post-traitement
    ///
    /// L'occlusion ambiante ne doit multiplier **que** l'ambiante : une surface en plein soleil au
    /// fond d'un coin reste éclairée par le soleil. Or une fois la scène écrite, direct et ambiante
    /// sont additionnés et plus rien ne les sépare.
    ///
    /// Mesuré au pied d'un mur ensoleillé — direct 1,2, ambiante 0,02, occlusion 0,5 : la valeur
    /// juste est **1,21**, celle qu'on obtient en occultant la lumière totale est **0,61**. *Un
    /// facteur deux, en plein soleil, sur une image dont on vient justement de monter le rapport
    /// direct/ambiant.* La séparation n'est pas un raffinement, c'est la condition pour que l'effet
    /// ne soit pas faux.
    ///
    /// Elle suit exactement le sort de la couleur : multi-échantillonnée et passagère pendant la
    /// passe, réduite en fin de passe vers une image lisible.
    ambiante: Option<ImageAllouee>,
    ambiante_resolue: ImageAllouee,
    profondeur: ImageAllouee,
    /// La profondeur à **un seul échantillon**, lisible par un shader.
    ///
    /// ## ⚠ Pourquoi elle existe en plus de l'autre, et ce qu'elle coûte
    ///
    /// La profondeur de la passe est multi-échantillonnée et **passagère** : sur un GPU à tuiles
    /// elle ne touche jamais la mémoire centrale, et c'est ce qui rend l'anti-crénelage abordable.
    /// Une texture, elle, doit exister pour de bon et n'avoir qu'un échantillon par pixel.
    ///
    /// Vulkan sait faire la réduction en fin de passe, exactement comme pour la couleur. Le mode
    /// retenu est `SAMPLE_ZERO` — on garde le premier échantillon plutôt que d'en faire une
    /// moyenne : **moyenner des profondeurs n'a aucun sens.** La moyenne de « 3 m » et « 10 m »
    /// donne 6,5 m, une distance où il n'y a rien, et l'occlusion se calculerait sur une surface
    /// fantôme le long de chaque arête.
    ///
    /// Absente quand il n'y a pas d'anti-crénelage : la profondeur ordinaire fait alors l'affaire,
    /// et allouer une copie identique serait de l'excédent.
    profondeur_lisible: Option<ImageAllouee>,
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

        let proprietes = |f: vk::Format| unsafe {
            gpu.instance
                .get_physical_device_format_properties(gpu.physical_device, f)
                .optimal_tiling_features
        };
        let format_hdr = format_hdr(proprietes(HDR_COMPACT), proprietes(HDR_REPLI));

        Self::batir(gpu, memory_props, echantillons, format_profondeur, format_hdr)
    }

    fn batir(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        echantillons: vk::SampleCountFlags,
        format_profondeur: vk::Format,
        format_hdr: vk::Format,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let couleur = if echantillons == vk::SampleCountFlags::TYPE_1 {
            None
        } else {
            Some(ImageAllouee::creer(
                gpu,
                memory_props,
                format_hdr,
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                echantillons,
                // Passagère : les échantillons servent à la moyenne, et à rien après elle.
                true,
            )?)
        };

        let resolue = ImageAllouee::creer(
            gpu,
            memory_props,
            format_hdr,
            // Écrite par la scène (ou par la moyenne des échantillons), relue par le halo et par
            // la composition. `SAMPLED` est ce qui autorise cette relecture.
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
            vk::SampleCountFlags::TYPE_1,
            false,
        )?;

        let multi = echantillons != vk::SampleCountFlags::TYPE_1;

        // L'ambiante suit la couleur : passagère et multi-échantillonnée s'il y a lieu, réduite
        // vers une image que l'occlusion pourra lire.
        let ambiante = if multi {
            Some(ImageAllouee::creer(
                gpu,
                memory_props,
                format_hdr,
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                echantillons,
                true,
            )?)
        } else {
            None
        };

        let ambiante_resolue = ImageAllouee::creer(
            gpu,
            memory_props,
            format_hdr,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
            vk::SampleCountFlags::TYPE_1,
            false,
        )?;

        let profondeur = ImageAllouee::creer(
            gpu,
            memory_props,
            format_profondeur,
            // ⚠ `SAMPLED` seulement quand elle est LA profondeur lisible (pas d'anti-crenelage) :
            // une image multi-echantillonnee passagere ne doit surtout pas devenir une texture,
            // ce serait renoncer a tout ce que `TRANSIENT_ATTACHMENT` fait gagner.
            if multi {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
            } else {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
            },
            vk::ImageAspectFlags::DEPTH,
            echantillons,
            multi,
        )?;

        let profondeur_lisible = if multi {
            Some(ImageAllouee::creer(
                gpu,
                memory_props,
                format_profondeur,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::DEPTH,
                vk::SampleCountFlags::TYPE_1,
                false,
            )?)
        } else {
            None
        };

        let combien = 1u32 << (echantillons.as_raw().trailing_zeros());
        log::info!(
            "Cibles de rendu : {}x{}, {combien} echantillon(s) par pixel, scene en {format_hdr:?}",
            gpu.swapchain_extent.width,
            gpu.swapchain_extent.height
        );

        Ok(Self {
            echantillons,
            format_profondeur,
            format_hdr,
            couleur,
            resolue,
            ambiante,
            ambiante_resolue,
            profondeur,
            profondeur_lisible,
        })
    }

    /// La vue dans laquelle la passe de scène écrit ses couleurs.
    ///
    /// C'est l'image multi-échantillonnée s'il y en a une, et **la scène résolue sinon**. Un seul
    /// chemin de rendu sert les deux cas, sans condition ailleurs.
    pub fn vue_couleur(&self) -> vk::ImageView {
        match &self.couleur {
            Some(c) => c.vue,
            None => self.resolue.vue,
        }
    }

    /// L'image multi-échantillonnée, quand elle existe — la barrière de disposition en a besoin.
    pub fn image_couleur(&self) -> Option<vk::Image> {
        self.couleur.as_ref().map(|c| c.image)
    }

    /// La scène en lumière, moyennée : ce que le halo lit et ce que la composition présente.
    pub fn vue_resolue(&self) -> vk::ImageView {
        self.resolue.vue
    }

    pub fn image_resolue(&self) -> vk::Image {
        self.resolue.image
    }

    /// La vue dans laquelle la passe de scène écrit l'AMBIANTE.
    pub fn vue_ambiante(&self) -> vk::ImageView {
        match &self.ambiante {
            Some(a) => a.vue,
            None => self.ambiante_resolue.vue,
        }
    }

    pub fn image_ambiante(&self) -> Option<vk::Image> {
        self.ambiante.as_ref().map(|a| a.image)
    }

    /// L'ambiante réduite : ce que la correction d'occlusion lit.
    pub fn vue_ambiante_resolue(&self) -> vk::ImageView {
        self.ambiante_resolue.vue
    }

    pub fn image_ambiante_resolue(&self) -> vk::Image {
        self.ambiante_resolue.image
    }

    pub fn vue_profondeur(&self) -> vk::ImageView {
        self.profondeur.vue
    }

    pub fn image_profondeur(&self) -> vk::Image {
        self.profondeur.image
    }

    /// La profondeur qu'un shader peut lire — la résolue s'il y a de l'anti-crénelage, l'unique
    /// sinon. **Un seul appelant, un seul concept** : rien en aval n'a à savoir laquelle c'est.
    pub fn vue_profondeur_lisible(&self) -> vk::ImageView {
        match &self.profondeur_lisible {
            Some(p) => p.vue,
            None => self.profondeur.vue,
        }
    }

    pub fn image_profondeur_lisible(&self) -> vk::Image {
        match &self.profondeur_lisible {
            Some(p) => p.image,
            None => self.profondeur.image,
        }
    }

    /// La vue vers laquelle réduire les échantillons de profondeur, quand il y en a plusieurs.
    ///
    /// ⚠ Rend `None` sans anti-crénelage, et c'est ce qui doit décider : demander une réduction
    /// sur une passe à un seul échantillon est refusé par la carte.
    pub fn resolution_profondeur(&self) -> Option<vk::ImageView> {
        self.profondeur_lisible.as_ref().map(|p| p.vue)
    }

    /// Vrai quand la passe doit moyenner ses échantillons vers la scène résolue.
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
        let hdr = self.format_hdr;
        self.detruire(&gpu.device);
        *self = Self::batir(gpu, memory_props, echantillons, format, hdr)?;
        Ok(())
    }

    pub fn detruire(&self, device: &ash::Device) {
        if let Some(c) = &self.couleur {
            c.detruire(device);
        }
        self.resolue.detruire(device);
        if let Some(a) = &self.ambiante {
            a.detruire(device);
        }
        self.ambiante_resolue.detruire(device);
        self.profondeur.detruire(device);
        if let Some(p) = &self.profondeur_lisible {
            p.detruire(device);
        }
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

    /// Le format compact est retenu quand la carte le supporte **pleinement**.
    #[test]
    fn le_format_compact_est_retenu_quand_il_est_pleinement_utilisable() {
        assert_eq!(format_hdr(CAPACITES_REQUISES, CAPACITES_REQUISES), HDR_COMPACT);
    }

    /// ⚠ Le cas qui compte : une seule capacité manquante suffit à le disqualifier.
    ///
    /// C'est le filtrage linéaire qui est le plus insidieux — une carte peut accepter d'écrire
    /// dans le format et de le lire, mais pas de l'interpoler. Le halo deviendrait alors une
    /// mosaïque, **sans aucun message**, et on chercherait le défaut dans le filtre.
    #[test]
    fn une_seule_capacite_manquante_fait_basculer_sur_le_repli() {
        for manquante in [
            vk::FormatFeatureFlags::COLOR_ATTACHMENT,
            vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND,
            vk::FormatFeatureFlags::SAMPLED_IMAGE,
            vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        ] {
            let ampute = vk::FormatFeatureFlags::from_raw(
                CAPACITES_REQUISES.as_raw() & !manquante.as_raw(),
            );
            assert_eq!(
                format_hdr(ampute, CAPACITES_REQUISES),
                HDR_REPLI,
                "sans {manquante:?}, le format compact n'est pas utilisable"
            );
        }
    }

    /// Une carte qui n'annonce rien ne fait pas échouer le choix : elle prend le repli.
    #[test]
    fn une_carte_qui_n_annonce_rien_prend_le_repli() {
        assert_eq!(
            format_hdr(vk::FormatFeatureFlags::empty(), vk::FormatFeatureFlags::empty()),
            HDR_REPLI
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
