use crate::chrono_gpu::ChronoGpu;
use crate::core::capacites;
use ash::vk;
#[cfg(feature = "fenetre")]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;
#[cfg(feature = "fenetre")]
use std::sync::Arc;

/// ⭐⭐ **CE QU'EST UNE FENÊTRE POUR LE MOTEUR — et ce qu'elle devient quand il n'y en a pas.**
///
/// Avec la fonction `fenetre`, c'est la fenêtre de winit, comme avant.
///
/// **Sans elle, c'est un type INHABITABLE** — une énumération sans variante, dont aucune valeur ne
/// peut exister. Ce n'est pas une astuce de compilation : c'est le compilateur qui **garantit**
/// qu'aucune fenêtre ne circule dans le moteur. Un `Option<&Fenetre>` ne peut alors valoir que
/// `None`, et il n'y a aucun chemin où l'oublier.
///
/// *C'est la doctrine du projet appliquée aux types : une garde posée là où le chemin se décide
/// ferme la classe entière ; une liste de « pense à vérifier » en oublie toujours un.*
#[cfg(feature = "fenetre")]
pub type Fenetre = winit::window::Window;

/// Sans système de fenêtrage, aucune fenêtre ne peut exister — et le compilateur le sait.
#[cfg(not(feature = "fenetre"))]
pub enum Fenetre {}

/// Struct regroupant la file de traitement et son index de famille.
pub struct QueueInfo {
    pub queue: vk::Queue,
    pub family_index: u32,
}

/// Contexte Vulkan pur, ecrit a la main (aucun intermediaire, aucun wgpu).
///
/// ⚠ La version exigee vit dans [`crate::core::capacites::VERSION_EXIGEE`] — **pas ici**. Ce
/// commentaire a annonce « 1.4 » pendant deux jours apres la descente en 1.3.
pub struct GpuContext {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub surface_loader: ash::khr::surface::Instance,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    /// Ce que la carte annonce d'elle-meme : ses limites, ses formats, son nom.
    ///
    /// ⚠ **Lire ces limites plutot que supposer** est une regle du projet payee cher : le moteur
    /// poussait 160 octets de constantes la ou Vulkan n'en garantit que 128, et fonctionnait ici
    /// parce que cette machine en offre 256. Une limite qu'on ne lit pas est une limite qu'on
    /// decouvre chez quelqu'un d'autre.
    pub proprietes: vk::PhysicalDeviceProperties,
    pub device: ash::Device,
    pub graphics_queue: QueueInfo,
    pub swapchain_loader: ash::khr::swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available_semaphore: vk::Semaphore,
    pub render_finished_semaphore: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
    /// Le chronometre GPU. `None` quand la file ne sait pas horodater — un cas reel sur certains
    /// pilotes, et qu'il vaut mieux voir que masquer par des zeros.
    pub chrono: Option<ChronoGpu>,
    /// Sans écran, c'est nous qui décidons quelle image sert : ce compteur tourne à la place de
    /// `acquire_next_image`. Inutilisé dès qu'une chaîne de présentation existe.
    image_suivante: usize,
    /// ⚠ **La mémoire des images que NOUS avons allouées** — vide dès qu'une chaîne de
    /// présentation existe, car ces images-là appartiennent au système de fenêtrage.
    ///
    /// *Elle manquait, et c'était une fuite silencieuse* : `sans_ecran` allouait une image et sa
    /// mémoire par tampon, les empilait dans un vecteur local, et ce vecteur mourait à la fin de la
    /// fonction en emportant les identifiants sans rien détruire. **Aucun warning ne pouvait le
    /// dire** — `push` est un usage. Sur un processus court, le pilote nettoie derrière ; dans une
    /// suite de tests qui ouvre un contexte par cas, ça s'accumule.
    memoires_des_images: Vec<vk::DeviceMemory>,
    /// L'index de l'image la plus récemment rendue.
    ///
    /// ⚠ **La capture et la mesure lisaient toujours `swapchain_images[0]`**, quelle que soit
    /// l'image réellement dessinée. Avec quatre images en rotation, on photographiait donc une
    /// image vieille de trois trames — ou jamais rendue. C'est le genre de défaut qu'aucun test ne
    /// voit et qu'aucun œil ne remarque, parce que deux images consécutives se ressemblent : il
    /// ne se révèle que lorsqu'on demande à la mesure d'être exacte.
    pub derniere_image: usize,
}

impl GpuContext {
    /// Initialise l'instance Vulkan, selectionne le GPU **apres avoir verifie qu'il convient**, et
    /// cree la chaine de presentation.
    #[cfg(feature = "fenetre")]
    pub fn new(window: Arc<Fenetre>) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation de Vulkan via ash, ecrit a la main...");

        // 1. Chargement de la bibliothèque dynamique Vulkan
        let entry = unsafe { ash::Entry::load()? };

        // 2. Extensions d'instance requises pour la fenêtre
        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();

        let instance_extensions = ash_window::enumerate_required_extensions(display_handle)?;

        let app_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"AegisEngine\0") };

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            // ⚠⚠ 1.3, ET SURTOUT PAS 1.4 — cette ligne annonçait « VULKAN 1.4 CORE ! » jusqu'au
            // 1er septembre 2026, et c'était une prétention **gratuite** : le moteur ne demande que
            // des fonctionnalités 1.3 (`dynamic_rendering`, `synchronization2` — voir plus bas) et
            // n'appelle **aucune** fonction propre à 1.4.
            //
            // **Le prix de cette ligne se paie ailleurs que sur cette machine.** Une RTX 4070
            // annonce 1.4, donc rien ne se voyait ici. Mais un GPU mobile n'y est pas :
            // l'IMG BXM-8-256 d'un Motorola G54 s'arrête à **Vulkan 1.3**, et le Snapdragon XR2
            // d'un Meta Quest 2 — la machine de référence du projet — est en dessous. *Declarer une version
            // qu'on n'utilise pas ne sert a rien — et ⚠ la justification ecrite ici le 1er septembre
            // etait TROP DRAMATIQUE : depuis Vulkan 1.1 le loader ne refuse plus une `apiVersion`
            // trop haute (c'est en 1.0 qu'il renvoyait `VK_ERROR_INCOMPATIBLE_DRIVER`). Ce qui se
            // paie vraiment, c'est le `vkCreateDevice` : les fonctionnalites d'une version que la
            // carte n'a pas ne peuvent pas etre activees. Voir `core::capacites`.*
            //
            // ⚠ Ce que ça ne prouve pas : que le moteur démarre sur mobile. Rien n'a jamais tourné
            // sur un téléphone. Ça retire **un** obstacle certain, pas tous les obstacles.
            .api_version(capacites::VERSION_EXIGEE);

        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(instance_extensions);

        log::info!("Création de l'instance Vulkan 1.3...");
        let instance = unsafe { entry.create_instance(&instance_create_info, None)? };

        // 3. Surface de fenêtrage
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let surface = unsafe {
            ash_window::create_surface(&entry, &instance, display_handle, window_handle, None)?
        };

        // 4. Sélection du Physical Device (GPU)
        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        if physical_devices.is_empty() {
            return Err("Aucun GPU compatible Vulkan trouvé.".into());
        }

        let mut selected_gpu = None;
        for &gpu in physical_devices.iter() {
            let props = unsafe { instance.get_physical_device_properties(gpu) };
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }.to_string_lossy();
            log::info!("GPU Détecté : {} (Type: {:?})", name, props.device_type);

            if props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU || selected_gpu.is_none() {
                selected_gpu = Some((gpu, props));
            }
        }

        let (physical_device, gpu_props) = selected_gpu.ok_or("Impossible de trouver un GPU approprié.")?;
        let gpu_name = unsafe { CStr::from_ptr(gpu_props.device_name.as_ptr()) }.to_string_lossy();
        log::info!("GPU Retenu pour AegisEngine : {}", gpu_name);

        // 5. Recherche de la famille de file d'attente (Graphics & Present)
        let queue_families = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let mut graphics_family_idx = None;

        for (idx, family) in queue_families.iter().enumerate() {
            let idx = idx as u32;
            let supports_graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let supports_present = unsafe {
                surface_loader.get_physical_device_surface_support(physical_device, idx, surface)?
            };

            if supports_graphics && supports_present {
                graphics_family_idx = Some(idx);
                break;
            }
        }

        let graphics_family_idx = graphics_family_idx.ok_or("Aucune famille de queue Graphics + Present trouvée.")?;

        // Les deux nombres dont depend toute mesure de temps GPU. Ils se lisent ICI, une fois :
        // la periode est une propriete du GPU, les bits utiles une propriete de la FAMILLE de file
        // — et les confondre donne des durees fausses sans que rien ne le signale.
        let periode_horodatage = gpu_props.limits.timestamp_period;
        let bits_horodatage = queue_families[graphics_family_idx as usize].timestamp_valid_bits;

        // 6. Device Virtuel & Extensions de Device
        let device_extension_names = [ash::khr::swapchain::NAME.as_ptr()];
        let queue_priorities = [1.0f32];

        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_family_idx)
            .queue_priorities(&queue_priorities);

        // ⭐ La carte est INTERROGÉE avant qu'on lui demande quoi que ce soit, et le refus NOMME ce
        // qui manque. Avant le 3 septembre 2026, `create_device` échouait ici sans un mot utile.
        unsafe { capacites::verifier(&instance, physical_device)? };

        // Ce que le moteur active — la liste vit dans `core::capacites`, à un seul endroit, et un
        // test échoue si elle contient une fonctionnalité dont aucun code vivant ne se sert.
        let mut features_13 = capacites::fonctionnalites_13();
        let mut features_core = vk::PhysicalDeviceFeatures2::default().push_next(&mut features_13);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info))
            .enabled_extension_names(&device_extension_names)
            .push_next(&mut features_core);

        log::info!("Création du Logical Device Vulkan 1.3...");
        let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };
        let graphics_queue = unsafe { device.get_device_queue(graphics_family_idx, 0) };

        // 32 jalons : largement au-dela du decoupage actuel, et le pool est minuscule. Un
        // depassement est journalise une fois plutot que d'etre ignore en silence.
        let chrono = match ChronoGpu::nouveau(&device, periode_horodatage, bits_horodatage, 32) {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("Chronometre GPU indisponible : {e}. Le rendu tourne, la mesure non.");
                None
            }
        };

        // 7. Initialisation du Swapchain
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let surface_caps = unsafe { surface_loader.get_physical_device_surface_capabilities(physical_device, surface)? };
        let surface_formats = unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };

        let format = surface_formats
            .iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_SRGB || f.format == vk::Format::R8G8B8A8_SRGB)
            .copied()
            .unwrap_or(surface_formats[0]);

        let extent = if surface_caps.current_extent.width != u32::MAX {
            surface_caps.current_extent
        } else {
            let inner_size = window.inner_size();
            vk::Extent2D {
                width: inner_size.width.clamp(surface_caps.min_image_extent.width, surface_caps.max_image_extent.width),
                height: inner_size.height.clamp(surface_caps.min_image_extent.height, surface_caps.max_image_extent.height),
            }
        };

        let image_count = (surface_caps.min_image_count + 1).min(if surface_caps.max_image_count > 0 { surface_caps.max_image_count } else { u32::MAX });

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO);

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };
        let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };

        let mut swapchain_image_views = Vec::new();
        for &img in swapchain_images.iter() {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let view = unsafe { device.create_image_view(&view_info, None)? };
            swapchain_image_views.push(view);
        }

        // 8. Command Pool & Buffers
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(graphics_family_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        let command_pool = unsafe { device.create_command_pool(&pool_info, None)? };

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);

        let command_buffers = unsafe { device.allocate_command_buffers(&alloc_info)? };

        // 9. Synchro Vulkan (Semaphores & Fences)
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let image_available_semaphore = unsafe { device.create_semaphore(&semaphore_info, None)? };
        let render_finished_semaphore = unsafe { device.create_semaphore(&semaphore_info, None)? };
        let in_flight_fence = unsafe { device.create_fence(&fence_info, None)? };

        log::info!("Chaine de presentation creee ({x}x{y}, Format: {fmt:?}, Images: {count})", x = extent.width, y = extent.height, fmt = format.format, count = swapchain_images.len());

        Ok(Self {
            entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            proprietes: gpu_props,
            device,
            graphics_queue: QueueInfo {
                queue: graphics_queue,
                family_index: graphics_family_idx,
            },
            swapchain_loader,
            swapchain,
            swapchain_format: format.format,
            swapchain_extent: extent,
            swapchain_images,
            swapchain_image_views,
            command_pool,
            command_buffers,
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
            chrono,
            image_suivante: 0,
            // Avec un écran, les images viennent de la chaîne de présentation : elles ne sont pas
            // à nous, et les détruire serait une faute.
            memoires_des_images: Vec::new(),
            derniere_image: 0,
        })
    }

    /// Ouvre un contexte **sans aucune fenêtre** : ni surface, ni chaîne de présentation.
    ///
    /// # ⭐⭐ Pourquoi ce chemin existe : une mesure prise à travers un compositeur ment
    ///
    /// Le chronomètre GPU est juste, mais **ce qu'il mesure dépend de qui regarde la fenêtre**.
    /// Sous un compositeur qui met en veille ce qui n'est pas visible — niri en est un — une
    /// fenêtre non regardée voit sa cadence s'effondrer, le GPU baisse ses fréquences, et *chaque
    /// image mesurée devient alors plus lente qu'elle ne l'est réellement*. Le relevé n'est pas
    /// bruité : il est **biaisé, et toujours dans le même sens**.
    ///
    /// Pire pour le travail quotidien : il fallait cliquer sur la bonne fenêtre au bon moment,
    /// donc chaque mesure dépendait d'un geste humain qu'on ne peut ni répéter ni vérifier.
    ///
    /// *C'est le quatrième cas de la règle : quand l'instrument est modifié par ce qui l'entoure,
    /// toutes les conclusions sont fausses, y compris les négatives.* Un banc ne se répare pas en
    /// faisant plus attention ; il se répare en retirant ce qui le perturbe.
    ///
    /// # Ce que ce chemin change, et ce qu'il ne change PAS
    ///
    /// Il ne change **rien au rendu** : mêmes pipelines, mêmes shaders, mêmes cibles, même format.
    /// Seule la destination change — des images que l'on s'alloue au lieu de celles que le système
    /// de fenêtrage prête. Le reste du moteur ne voit que des images et un format : il ne sait
    /// même pas qu'il n'y a pas d'écran.
    ///
    /// *C'est ce que la présentation aurait toujours dû être : une capacité, pas une fondation.*
    ///
    /// ⚠ Ce qu'il ne prouve pas : le comportement de la présentation elle-même (déchirure,
    /// intervalle de rafraîchissement, `OUT_OF_DATE`). Un banc sans écran mesure le coût du
    /// rendu, jamais le confort de l'affichage.
    ///
    /// # ⭐ Pourquoi le FORMAT est un paramètre depuis le 2 septembre 2026
    ///
    /// Il était fixé à `B8G8R8A8_SRGB` — « celui qu'un écran nous donnerait », et cette raison
    /// reste juste **pour mesurer une couleur** : un banc qui rendrait dans un autre espace ne
    /// mesurerait pas l'image qu'on regarde.
    ///
    /// **Mais tout ce qu'une image transporte n'est pas une couleur.** Une carte de directions,
    /// de normales ou de distances passée par une courbe sRGB est quantifiée de façon **non
    /// uniforme** : autour de 0,5 la pente vaut 0,66, donc un pas de 1/255 en sortie vaut 1/168 en
    /// valeur réelle — trois fois le bruit qu'on croit avoir. *Une sonde qui mesure une direction
    /// à travers une courbe de gamma mesure la courbe autant que la direction.*
    ///
    /// Le défaut reste `B8G8R8A8_SRGB` : la couleur passe par le même chemin qu'avant.
    pub fn sans_ecran_format(
        largeur: u32,
        hauteur: u32,
        combien_d_images: usize,
        format: vk::Format,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Contexte Vulkan SANS ECRAN — aucune fenetre, aucun compositeur dans la mesure");

        let entry = unsafe { ash::Entry::load()? };

        let app_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"AegisEngine Banc\0") };
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            // 1.3 pour la même raison que dans `new` — voir le commentaire là-haut. Les deux
            // chemins doivent déclarer la MÊME version : un banc qui tournerait sous une version
            // différente du jeu ne mesurerait pas le jeu.
            .api_version(capacites::VERSION_EXIGEE);

        // Aucune extension d'instance : c'est la différence de fond avec `new`. Rien ici ne sait
        // ce qu'est un écran.
        let instance = unsafe {
            entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app_info), None)?
        };

        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        if physical_devices.is_empty() {
            return Err("Aucun GPU compatible Vulkan trouvé.".into());
        }

        let mut retenu = None;
        for &gpu in physical_devices.iter() {
            let props = unsafe { instance.get_physical_device_properties(gpu) };
            let nom = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }.to_string_lossy();
            log::info!("GPU Détecté : {} (Type: {:?})", nom, props.device_type);
            if props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU || retenu.is_none() {
                retenu = Some((gpu, props));
            }
        }
        let (physical_device, gpu_props) = retenu.ok_or("Impossible de trouver un GPU approprié.")?;
        log::info!(
            "GPU Retenu : {}",
            unsafe { CStr::from_ptr(gpu_props.device_name.as_ptr()) }.to_string_lossy()
        );

        // ⚠ On ne demande QUE `GRAPHICS`. Exiger en plus le support de présentation, comme le fait
        // `new`, demanderait une surface — donc une fenêtre, donc le problème qu'on retire.
        let familles = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let famille = familles
            .iter()
            .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or("Aucune famille de queue Graphics trouvée.")? as u32;

        let periode_horodatage = gpu_props.limits.timestamp_period;
        let bits_horodatage = familles[famille as usize].timestamp_valid_bits;

        let priorites = [1.0f32];
        let file = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(famille)
            .queue_priorities(&priorites);

        // Mêmes exigences que le chemin avec écran, et par la même fonction — un banc qui
        // demanderait autre chose que le jeu ne mesurerait pas le jeu.
        unsafe { capacites::verifier(&instance, physical_device)? };

        let mut f13 = capacites::fonctionnalites_13();
        let mut fcore = vk::PhysicalDeviceFeatures2::default().push_next(&mut f13);

        // Aucune extension de device non plus : `VK_KHR_swapchain` ne sert qu'à présenter.
        let device = unsafe {
            instance.create_device(
                physical_device,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(std::slice::from_ref(&file))
                    .push_next(&mut fcore),
                None,
            )?
        };
        let graphics_queue = unsafe { device.get_device_queue(famille, 0) };

        let chrono = match ChronoGpu::nouveau(&device, periode_horodatage, bits_horodatage, 32) {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("Chronometre GPU indisponible : {e}. Le rendu tourne, la mesure non.");
                None
            }
        };

        let extent = vk::Extent2D { width: largeur, height: hauteur };

        let memory_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let mut swapchain_images = Vec::with_capacity(combien_d_images);
        let mut swapchain_image_views = Vec::with_capacity(combien_d_images);
        let mut memoires = Vec::with_capacity(combien_d_images);

        for _ in 0..combien_d_images.max(1) {
            let image = unsafe {
                device.create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(format)
                        .extent(vk::Extent3D { width: largeur, height: hauteur, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        // `TRANSFER_SRC` : c'est ce qui permet de relire l'image pour l'écrire en
                        // PNG. Un banc qui ne rend pas d'image ne prouverait que des durées.
                        .usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .sharing_mode(vk::SharingMode::EXCLUSIVE)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )?
            };
            let besoins = unsafe { device.get_image_memory_requirements(image) };
            let type_memoire = crate::core::memory::MemoryManager::find_memory_type(
                &memory_props,
                besoins.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .ok_or("aucune memoire pour une image de banc")?;
            let memoire = unsafe {
                device.allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(besoins.size)
                        .memory_type_index(type_memoire),
                    None,
                )?
            };
            unsafe { device.bind_image_memory(image, memoire, 0)? };

            let vue = unsafe {
                device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
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
            swapchain_images.push(image);
            swapchain_image_views.push(vue);
            memoires.push(memoire);
        }

        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(famille)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?
        };
        let command_buffers = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(swapchain_images.len() as u32),
            )?
        };

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        log::info!("Banc : {largeur}x{hauteur}, {} image(s), format {format:?}", swapchain_images.len());

        // ⚠ Ces deux chargeurs sont construits mais leurs pointeurs sont NULS : les extensions
        // correspondantes ne sont pas activées. Rien ne doit les appeler — c'est ce que
        // `presente()` garantit, et c'est pourquoi tout chemin de présentation passe par lui.
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

        Ok(Self {
            entry,
            surface_loader,
            surface: vk::SurfaceKHR::null(),
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            instance,
            physical_device,
            proprietes: gpu_props,
            graphics_queue: QueueInfo { queue: graphics_queue, family_index: famille },
            swapchain_format: format,
            swapchain_extent: extent,
            swapchain_images,
            swapchain_image_views,
            image_available_semaphore: unsafe { device.create_semaphore(&semaphore_info, None)? },
            render_finished_semaphore: unsafe { device.create_semaphore(&semaphore_info, None)? },
            in_flight_fence: unsafe { device.create_fence(&fence_info, None)? },
            command_pool,
            command_buffers,
            chrono,
            image_suivante: 0,
            memoires_des_images: memoires,
            derniere_image: 0,
            device,
        })
    }

    /// Ce contexte sait-il montrer ses images à quelqu'un ?
    ///
    /// ⚠ **Tout chemin de présentation doit passer par cette garde.** Sans écran, les chargeurs
    /// d'extension existent mais leurs pointeurs sont nuls : les appeler planterait sans message
    /// utile. *Une garde posée à l'endroit où le chemin se décide ferme la classe entière ; une
    /// liste de « pense à vérifier » en oublie toujours un.*
    pub fn presente(&self) -> bool {
        self.swapchain != vk::SwapchainKHR::null()
    }

    /// Referme l'etape courante du chronometre et lui donne son nom.
    ///
    /// Ne fait rien si le GPU ne sait pas horodater — l'appelant n'a donc jamais a s'en soucier,
    /// et le rendu ne depend en aucune facon de la presence de l'instrument.
    pub fn jalon(&self, cmd: vk::CommandBuffer, nom: &'static str) {
        if let Some(chrono) = self.chrono.as_ref() {
            chrono.jalon(&self.device, cmd, nom);
        }
    }

    /// Ouvre une image.
    ///
    /// ⚠ La fenêtre est **optionnelle**, et c'est ce qui rend le banc sans écran possible : sans
    /// elle, il n'y a ni image à demander au système de fenêtrage, ni redimensionnement possible.
    /// Le reste — la barrière, le carnet de commandes, le chronomètre — est identique, et ce n'est
    /// pas une commodité : *un banc qui suivrait un autre chemin que le jeu mesurerait autre chose
    /// que le jeu.*
    // Sans fenêtrage, `window` est un `Option` d'un type inhabitable : il ne peut valoir que
    // `None`, et aucun chemin ne le lit. Le paramètre reste pour que la signature soit la même
    // des deux côtés — c'est le prix d'une API unique, et il est bas.
    #[cfg_attr(not(feature = "fenetre"), allow(unused_variables))]
    pub fn begin_frame(&mut self, window: Option<&Fenetre>) -> Result<(vk::CommandBuffer, usize), Box<dyn std::error::Error>> {
        unsafe {
            self.device.wait_for_fences(&[self.in_flight_fence], true, u64::MAX)?;

            let image_index: u32 = if self.presente() {
                let result = self.swapchain_loader.acquire_next_image(
                    self.swapchain,
                    u64::MAX,
                    self.image_available_semaphore,
                    vk::Fence::null(),
                );

                match result {
                    Ok((idx, _sub)) => idx,
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        // Sans fenêtrage, `Fenetre` est inhabitable : ce bras ne peut pas exister,
                        // et le compilateur n'a même pas de quoi le compiler.
                        #[cfg(feature = "fenetre")]
                        if let Some(w) = window {
                            self.resize(w);
                        }
                        return Err("OUT_OF_DATE_KHR".into());
                    }
                    Err(err) => return Err(err.into()),
                }
            } else {
                // Sans écran, personne ne prête d'image : on tourne sur les nôtres, à la ronde.
                // Le compteur vient du chronomètre pour rester juste après un redémarrage.
                self.image_suivante = (self.image_suivante + 1) % self.swapchain_images.len();
                self.image_suivante as u32
            };

            self.derniere_image = image_index as usize;
            let cmd = self.command_buffers[image_index as usize];

            self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            self.device.begin_command_buffer(cmd, &begin_info)?;

            // Le releve de l'image PRECEDENTE se fait ici, et l'endroit n'est pas un detail : la
            // barriere vient d'etre attendue (`wait_for_fences` ci-dessus), donc cette image-la est
            // terminee et ses compteurs sont lisibles sans attendre une seule microseconde.
            if let Some(chrono) = self.chrono.as_mut() {
                chrono.ouvrir_image(&self.device, cmd);
            }

            Ok((cmd, image_index as usize))
        }
    }

    /// Ferme l'image, la soumet, et la présente **s'il y a quelqu'un pour la voir**.
    #[cfg_attr(not(feature = "fenetre"), allow(unused_variables))]
    pub fn end_frame(
        &mut self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        window: Option<&Fenetre>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.end_command_buffer(cmd)?;
            self.device.reset_fences(&[self.in_flight_fence])?;

            if !self.presente() {
                // ⚠ Aucun sémaphore : sans chaîne de présentation, personne ne signale l'arrivée
                // d'une image et personne n'attend la fin du rendu. Les réclamer bloquerait le
                // banc pour toujours, sur une attente que rien ne viendrait satisfaire.
                self.device.queue_submit(
                    self.graphics_queue.queue,
                    &[vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd))],
                    self.in_flight_fence,
                )?;
                return Ok(());
            }

            let wait_semaphores = [self.image_available_semaphore];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [self.render_finished_semaphore];

            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(std::slice::from_ref(&cmd))
                .signal_semaphores(&signal_semaphores);

            self.device.queue_submit(
                self.graphics_queue.queue,
                &[submit_info],
                self.in_flight_fence,
            )?;

            let swapchains = [self.swapchain];
            let image_indices = [image_index as u32];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            let result = self.swapchain_loader.queue_present(self.graphics_queue.queue, &present_info);

            if result == Ok(true) || result == Err(vk::Result::ERROR_OUT_OF_DATE_KHR) || result == Err(vk::Result::SUBOPTIMAL_KHR) {
                // Même raison qu'au-dessus : sans fenêtrage, ce bras ne peut pas exister — on n'est
                // d'ailleurs jamais arrivé ici, `presente()` a déjà rendu la main.
                #[cfg(feature = "fenetre")]
                if let Some(w) = window {
                    self.resize(w);
                }
            }
        }
        Ok(())
    }

    pub fn begin_single_time_commands(&self) -> Result<vk::CommandBuffer, Box<dyn std::error::Error>> {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(self.command_pool)
            .command_buffer_count(1);

        let command_buffer = unsafe { self.device.allocate_command_buffers(&alloc_info)?[0] };
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe { self.device.begin_command_buffer(command_buffer, &begin_info)? };
        Ok(command_buffer)
    }

    pub fn end_single_time_commands(&self, command_buffer: vk::CommandBuffer) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.end_command_buffer(command_buffer)?;

            let submit_info = vk::SubmitInfo::default()
                .command_buffers(std::slice::from_ref(&command_buffer));

            self.device.queue_submit(self.graphics_queue.queue, &[submit_info], vk::Fence::null())?;
            self.device.queue_wait_idle(self.graphics_queue.queue)?;

            self.device.free_command_buffers(self.command_pool, &[command_buffer]);
        }
        Ok(())
    }

    /// Un contexte sans écran dans le format d'un écran — le cas de loin le plus courant.
    ///
    /// *Séparé de `sans_ecran_format` pour que « je veux mesurer une couleur » n'ait pas à écrire
    /// le nom d'un format : un appel qui doit choisir finit toujours par choisir mal une fois.*
    pub fn sans_ecran(
        largeur: u32,
        hauteur: u32,
        combien_d_images: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::sans_ecran_format(largeur, hauteur, combien_d_images, vk::Format::B8G8R8A8_SRGB)
    }

    /// Les octets **bruts** de l'image, tels que la carte les range — quatre par pixel, sans
    /// réordonner les canaux et sans retirer l'alpha.
    ///
    /// ⚠ **À n'employer que quand l'image ne transporte pas une couleur** : une carte de
    /// directions, de normales, de distances. Pour une couleur, `relire_image` fait le travail
    /// (échange B↔R, retrait de l'alpha) et c'est ce qu'on veut.
    pub fn relire_image_brute(
        &self,
        image: vk::Image,
        layout_actuel: vk::ImageLayout,
        extent: vk::Extent2D,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // `UNDEFINED` n'est le format d'aucune image réelle : il ne peut donc déclencher aucun des
        // échanges de canaux, ce qui est exactement le sens de « brut ».
        self.transfert(image, layout_actuel, extent, vk::Format::UNDEFINED, true)
    }

    /// ⭐⭐ **Rapatrie une image de la carte vers la mémoire, en RVB (trois octets par pixel).**
    ///
    /// # Pourquoi cette fonction a déménagé ici, et c'est le fond du sujet
    ///
    /// Ce transfert vivait dans `Engine` — c'est-à-dire dans la structure qui **exige une
    /// fenêtre**. Il y supposait, en dur, que l'image venait d'une chaîne de présentation :
    /// `old_layout(PRESENT_SRC_KHR)` à l'aller, le même au retour. **Donc la seule façon de relire
    /// une image du moteur était d'avoir un écran devant soi.**
    ///
    /// *C'est exactement la raison structurelle pour laquelle « le rendu casse en silence » sur ce
    /// projet* : `GpuContext::sans_ecran` sait ouvrir un GPU sans fenêtre depuis le 1ᵉʳ septembre
    /// 2026, mais rien ne savait relire ce qu'il produisait — alors même que ses images sont
    /// allouées en `TRANSFER_SRC` **précisément pour ça**.
    ///
    /// ⚠ **Et c'est la deuxième fois que ce défaut se trouve au même endroit.** Le `Drop` de ce
    /// fichier détruisait lui aussi une chaîne de présentation absente (`SIGABRT`, corrigé le
    /// 1ᵉʳ septembre). *Quand un même fichier suppose deux fois la présentation, ce n'est plus un
    /// oubli : c'est que la présentation y a été traitée comme une fondation au lieu d'une
    /// capacité.*
    ///
    /// Le geste juste n'était donc pas d'écrire une seconde version pour le banc — ç'aurait été
    /// deux textes à faire évoluer en parallèle, donc tôt ou tard deux comportements. **C'est de
    /// remonter la fonction là où elle appartient, et de faire du layout un PARAMÈTRE.**
    ///
    /// # Les paramètres
    ///
    /// - `image` : l'image à lire. Avec un écran, `swapchain_images[derniere_image]` — *l'image
    ///   réellement rendue, jamais `[0]`, qui rendrait une photographie vieille de plusieurs
    ///   trames.* Sans écran, l'une des images allouées par `sans_ecran`.
    /// - `layout_actuel` : le layout dans lequel l'image se trouve **et dans lequel elle sera
    ///   remise** en repartant. `PRESENT_SRC_KHR` après une présentation, `COLOR_ATTACHMENT_OPTIMAL`
    ///   juste après un rendu.
    /// - `extent`, `format` : la taille et le format de l'image.
    ///
    /// # ⚠ Ce que ça ne fait pas
    ///
    /// Aucune synchronisation fine : la barrière est volontairement large (`ALL_COMMANDS`), et
    /// `end_single_time_commands` attend la file entière. *C'est un chemin de CAPTURE, hors du
    /// chemin critique — ici la correction prime sur la finesse, et une barrière étroite mal posée
    /// donnerait une image partielle sans que rien ne le dise.*
    pub fn relire_image(
        &self,
        image: vk::Image,
        layout_actuel: vk::ImageLayout,
        extent: vk::Extent2D,
        format: vk::Format,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.transfert(image, layout_actuel, extent, format, false)
    }

    /// Le transfert lui-même. `brut` garde les quatre canaux dans l'ordre de la carte.
    fn transfert(
        &self,
        image: vk::Image,
        layout_actuel: vk::ImageLayout,
        extent: vk::Extent2D,
        format: vk::Format,
        brut: bool,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let octets = (extent.width as vk::DeviceSize) * (extent.height as vk::DeviceSize) * 4;
        let memory_props =
            unsafe { self.instance.get_physical_device_memory_properties(self.physical_device) };

        let (tampon, memoire) = crate::core::memory::MemoryManager::create_buffer(
            &self.device,
            &memory_props,
            octets,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let plage = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let cmd = self.begin_single_time_commands()?;

        let vers_source = vk::ImageMemoryBarrier::default()
            .old_layout(layout_actuel)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .image(image)
            .subresource_range(plage);

        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vers_source],
            );
        }

        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            });

        unsafe {
            self.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                tampon,
                &[region],
            );
        }

        // On remet l'image comme on l'a trouvée : l'appelant continue son travail sans savoir
        // qu'on est passé.
        let retour = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(layout_actuel)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ)
            .image(image)
            .subresource_range(plage);

        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[retour],
            );
        }

        self.end_single_time_commands(cmd)?;

        let mut bruts = vec![0u8; octets as usize];
        unsafe {
            let source =
                self.device
                    .map_memory(memoire, 0, octets, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(source as *const u8, bruts.as_mut_ptr(), octets as usize);
            self.device.unmap_memory(memoire);
            self.device.destroy_buffer(tampon, None);
            self.device.free_memory(memoire, None);
        }

        if brut {
            return Ok(bruts);
        }

        // Les formats en `B8G8R8A8` rangent le bleu en premier : sans cet échange, une capture
        // sort avec le rouge et le bleu inversés — et ça ne se voit pas sur une image grise.
        if format == vk::Format::B8G8R8A8_SRGB || format == vk::Format::B8G8R8A8_UNORM {
            for pixel in bruts.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        let mut rvb = Vec::with_capacity(bruts.len() / 4 * 3);
        for pixel in bruts.chunks_exact(4) {
            rvb.extend_from_slice(&pixel[..3]);
        }
        Ok(rvb)
    }

    /// Redimensionne la taille du Swapchain Vulkan lors du redimensionnement de la fenêtre.
    #[cfg(feature = "fenetre")]
    pub fn resize(&mut self, window: &Fenetre) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        unsafe {
            let _ = self.device.device_wait_idle();
            for &view in self.swapchain_image_views.iter() {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_image_views.clear();

            let surface_caps = self.surface_loader.get_physical_device_surface_capabilities(self.physical_device, self.surface).unwrap();
            let extent = vk::Extent2D {
                width: size.width.clamp(surface_caps.min_image_extent.width, surface_caps.max_image_extent.width),
                height: size.height.clamp(surface_caps.min_image_extent.height, surface_caps.max_image_extent.height),
            };

            let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(self.swapchain_images.len() as u32)
                .image_format(self.swapchain_format)
                .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(surface_caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
                .old_swapchain(self.swapchain);

            let new_swapchain = self.swapchain_loader.create_swapchain(&swapchain_create_info, None).unwrap();
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);

            self.swapchain = new_swapchain;
            self.swapchain_extent = extent;
            self.swapchain_images = self.swapchain_loader.get_swapchain_images(self.swapchain).unwrap();

            for &img in self.swapchain_images.iter() {
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(img)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.swapchain_format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });
                let view = self.device.create_image_view(&view_info, None).unwrap();
                self.swapchain_image_views.push(view);
            }
        }
        log::debug!("Swapchain Vulkan redimensionné à {}x{}", size.width, size.height);
    }
}

impl Drop for GpuContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for &view in self.swapchain_image_views.iter() {
                self.device.destroy_image_view(view, None);
            }
            self.device.destroy_semaphore(self.image_available_semaphore, None);
            self.device.destroy_semaphore(self.render_finished_semaphore, None);
            self.device.destroy_fence(self.in_flight_fence, None);
            if let Some(chrono) = self.chrono.as_mut() {
                chrono.detruire(&self.device);
            }
            self.device.destroy_command_pool(self.command_pool, None);

            // ⚠⚠ DEUX FAUTES CORRIGÉES ICI, ET LES DEUX N'ONT ÉTÉ VUES QUE LE JOUR OÙ
            // `sans_ecran` A ENFIN ÉTÉ APPELÉ.
            //
            // **① Ce `Drop` détruisait la chaîne de présentation SANS CONDITION.** Sans écran, le
            // chargeur de l'extension existe mais ses pointeurs sont **nuls** — l'extension n'est
            // pas activée. Le programme s'arrêtait donc net sur `Unable to load
            // destroy_swapchain_khr`, dans un `drop`, c'est-à-dire là où une panique ne peut même
            // pas se dérouler proprement (`SIGABRT`).
            //
            // *Et le plus instructif : le commentaire de `sans_ecran` promettait déjà que « rien ne
            // doit les appeler ». Ce `Drop` les appelait.* **Un commentaire qui décrit une
            // garantie que le code ne tient pas est plus dangereux qu'une absence de commentaire**
            // — il fait passer le lecteur, moi compris.
            //
            // **② Les images d'un contexte sans écran nous appartiennent, et personne ne les
            // libérait.** Ni elles, ni leur mémoire. Aucun warning ne pouvait le dire.
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            }
            if self.surface != vk::SurfaceKHR::null() {
                self.surface_loader.destroy_surface(self.surface, None);
            }
            if !self.memoires_des_images.is_empty() {
                for &image in self.swapchain_images.iter() {
                    self.device.destroy_image(image, None);
                }
                for &memoire in self.memoires_des_images.iter() {
                    self.device.free_memory(memoire, None);
                }
            }

            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
        log::info!("Ressources Vulkan liberees proprement.");
    }
}

#[cfg(test)]
mod tests_sans_ecran {
    use super::*;

    /// ⭐⭐ **LE PREMIER TEST QUI OUVRE LA PORTE SANS ÉCRAN** — et elle n'avait jamais été poussée.
    ///
    /// `sans_ecran` était écrit, complet, soigneusement documenté — et **appelé par rien**, depuis
    /// sa naissance. C'est la famille de défauts n° 1 du projet dans sa forme la plus discrète :
    /// *le remède est déjà écrit, et branché à rien.*
    ///
    /// **Ce que ça débloque dépasse largement ce test :** tant que le contexte Vulkan exigeait une
    /// fenêtre, aucune passe de rendu ne pouvait être vérifiée autrement qu'à l'œil, sur une
    /// capture, à la main. *C'est la raison structurelle pour laquelle « le rendu casse en
    /// silence » dans ce projet — pas une négligence, une architecture.*
    ///
    /// ⚠ **Ignoré si aucun Vulkan n'est joignable.** Une machine sans pilote — une CI nue — n'a pas
    /// à faire échouer la suite ; mais l'absence est **dite**, jamais avalée en silence.
    #[test]
    fn le_contexte_sans_ecran_s_ouvre_vraiment() {
        let contexte = match GpuContext::sans_ecran(64, 64, 2) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠ aucun Vulkan joignable sur cette machine : {e}");
                println!("  (le test est neutralise, PAS reussi — il n'a rien prouve)");
                return;
            }
        };

        let nom = unsafe { CStr::from_ptr(contexte.proprietes.device_name.as_ptr()) };
        println!("  contexte sans ecran ouvert sur : {}", nom.to_string_lossy());
        println!("  format des images : {:?}", contexte.swapchain_format);
        println!("  images allouees   : {}", contexte.swapchain_images.len());

        // Les trois choses sans lesquelles aucune passe ne peut tourner.
        assert!(!contexte.swapchain_images.is_empty(), "aucune image de destination");
        assert_eq!(contexte.swapchain_extent.width, 64);
        assert_eq!(contexte.swapchain_extent.height, 64);
        // ⚠ Et celle qu'on oublie : sans file de commandes, on a un GPU qu'on ne peut pas faire
        // travailler.
        assert!(
            !contexte.command_buffers.is_empty(),
            "aucun tampon de commandes — le contexte est inerte"
        );
        // Un contexte sans écran ne présente rien, par construction. Si ce drapeau disait le
        // contraire, la boucle de rendu essaierait de présenter dans le vide.
        assert!(!contexte.presente(), "un contexte sans ecran ne doit rien presenter");
    }

    /// Rend une image d'une seule couleur, sans écran, et la relit.
    ///
    /// Renvoie `None` si aucun Vulkan n'est joignable — l'absence est **dite** par l'appelant,
    /// jamais avalée.
    fn rendre_un_aplat(couleur: [f32; 4]) -> Option<(Vec<u8>, usize)> {
        let ctx = match GpuContext::sans_ecran(16, 16, 1) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠ aucun Vulkan joignable : {e}");
                return None;
            }
        };
        let image = ctx.swapchain_images[0];
        let vue = ctx.swapchain_image_views[0];
        let etendue = ctx.swapchain_extent;

        let plage = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let cmd = ctx.begin_single_time_commands().ok()?;
        unsafe {
            // Une image fraîchement allouée est en `UNDEFINED` : on ne peut rien y écrire avant de
            // l'avoir amenée dans le layout d'un attachement de couleur.
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

            // ⭐ `CLEAR` écrit À TRAVERS l'attachement de couleur — donc la conversion sRGB du
            // format s'applique, exactement comme pour un pixel dessiné. C'est ce qui rend ce
            // test représentatif du vrai rendu, et pas seulement d'un remplissage mémoire.
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: couleur },
                });
            ctx.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            ctx.device.cmd_end_rendering(cmd);
        }
        ctx.end_single_time_commands(cmd).ok()?;

        let rvb = ctx
            .relire_image(
                image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                etendue,
                ctx.swapchain_format,
            )
            .ok()?;
        let pixels = (etendue.width * etendue.height) as usize;
        Some((rvb, pixels))
    }

    /// ⭐⭐⭐ **LE PREMIER TEST D'IMAGE GPU DE CE PROJET** (2 septembre 2026).
    ///
    /// # Ce qui n'existait pas avant lui, et pourquoi ça compte plus que ce qu'il vérifie
    ///
    /// Le moteur portait **105 tests, tous verts** — et **aucun** ne regardait un pixel produit
    /// par la carte graphique. Ils vérifiaient des conventions et des calculs faits par le
    /// processeur ; les quinze images de preuve du dépôt sortent d'un rastériseur écrit à la main,
    /// **pas de Vulkan**.
    ///
    /// *C'est la raison pour laquelle « le rendu casse en silence » sur ce projet — et ce n'est
    /// pas une négligence, c'est une architecture.* Le HUD est sorti à l'envers avec onze tests au
    /// vert ; le chargeur de géométrie a éclairé tous les modèles importés de travers pendant des
    /// mois, à 88° de la vérité, sous 221 tests verts. **Aucun test ne pouvait les voir, parce
    /// qu'aucun test ne voyait d'image.**
    ///
    /// # Le critère, écrit avant le code
    ///
    /// Trois propriétés, chacune choisie pour ne dépendre d'aucune convention fragile :
    ///
    /// 1. **Le rendu s'exécute vraiment sans écran.** Une passe de rendu dynamique (`1.3`) ouverte
    ///    et fermée sur une image que personne ne présente. *Rien ne prouvait jusqu'ici que
    ///    `dynamic_rendering` — la seule fonctionnalité 1.3 que le moteur demande — fonctionne.*
    /// 2. **⭐ L'ordre des canaux est le bon.** Le format est `B8G8R8A8` : le bleu vient en
    ///    premier en mémoire. Un rouge pur doit ressortir en rouge. *Ce piège ne se voit sur
    ///    aucune image grise — et une capture aux canaux inversés a l'air parfaitement normale
    ///    tant qu'on ne connaît pas la couleur attendue.*
    /// 3. **⭐⭐ La chaîne écrit bien dans un espace sRGB.** Un gris à 0,5 **linéaire** doit
    ///    ressortir nettement **au-dessus** de 128 (≈188). S'il sortait à 128, le moteur écrirait
    ///    en linéaire tout en croyant faire du sRGB — et toute la courbe de tonalité porterait sur
    ///    une hypothèse fausse.
    ///
    /// ⚠ **Aucune empreinte d'image n'est gravée ici, et c'est délibéré.** La règle est née la
    /// veille en mesurant que deux architectures donnent des images identiques *au bit près* sauf
    /// accumulation : *un test qui grave une empreinte passerait sur cette machine et tomberait
    /// chez quelqu'un d'autre, en accusant un code parfaitement juste.* On teste des
    /// **propriétés**, jamais une valeur exacte.
    #[test]
    fn le_gpu_rend_une_image_sans_ecran_et_on_la_relit() {
        // ── 1 & 2 : le rouge pur, qui prouve le rendu ET l'ordre des canaux ──────────────
        let Some((rvb, pixels)) = rendre_un_aplat([1.0, 0.0, 0.0, 1.0]) else {
            println!("  (le test est neutralise, PAS reussi — il n'a rien prouve)");
            return;
        };
        assert_eq!(rvb.len(), pixels * 3, "il manque des pixels a la relecture");
        println!("  rouge pur relu   : R={} V={} B={}", rvb[0], rvb[1], rvb[2]);
        for (i, pixel) in rvb.chunks_exact(3).enumerate() {
            assert_eq!(
                pixel,
                [255u8, 0, 0],
                "pixel {i} : un rouge pur est ressorti {pixel:?} — canaux inverses ou rendu muet"
            );
        }

        // Le vert pur : sans lui, un code qui rendrait toujours le premier canal passerait.
        let (vert, _) = rendre_un_aplat([0.0, 1.0, 0.0, 1.0]).expect("Vulkan etait la a l'instant");
        println!("  vert pur relu    : R={} V={} B={}", vert[0], vert[1], vert[2]);
        assert_eq!(&vert[..3], [0u8, 255, 0], "un vert pur n'est pas ressorti vert");

        // ── 3 : la preuve que la chaîne est bien en sRGB ─────────────────────────────────
        let (gris, _) = rendre_un_aplat([0.5, 0.5, 0.5, 1.0]).expect("Vulkan etait la a l'instant");
        let clair = gris[0];
        println!("  gris 0,5 lineaire ressort a {clair}/255 (sRGB predit ~188, lineaire dirait 128)");
        assert!(
            clair > 160,
            "un gris 0,5 lineaire est ressorti a {clair} : la chaine n'applique PAS la courbe sRGB, \
             alors que tout le reste du moteur le suppose"
        );
        assert_eq!(gris[0], gris[1], "un gris doit rester gris");
        assert_eq!(gris[1], gris[2], "un gris doit rester gris");
    }
}
