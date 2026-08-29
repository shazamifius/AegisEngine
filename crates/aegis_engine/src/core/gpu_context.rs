use crate::chrono_gpu::ChronoGpu;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;
use std::sync::Arc;
use winit::window::Window;

/// Struct regroupant la file de traitement et son index de famille.
pub struct QueueInfo {
    pub queue: vk::Queue,
    pub family_index: u32,
}

/// Contexte Pure Vulkan 1.4 From Scratch (Zero Middleware / Zero wgpu).
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
}

impl GpuContext {
    /// Initialise une instance Vulkan 1.4 native pure, sélectionne le GPU et crée le Swapchain.
    pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation de l'API Vulkan 1.4 Native via ash (Pure From Scratch)...");

        // 1. Chargement de la bibliothèque dynamique Vulkan
        let entry = unsafe { ash::Entry::load()? };

        // 2. Extensions d'instance requises pour la fenêtre
        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();

        let instance_extensions = ash_window::enumerate_required_extensions(display_handle)?;

        let app_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"AegisEngine Pure Vulkan 1.4\0") };

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::make_api_version(0, 1, 4, 0)); // VULKAN 1.4 CORE !

        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(instance_extensions);

        log::info!("Création de l'instance Vulkan 1.4 Core...");
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

        // Activation des fonctionnalités Vulkan 1.3 & 1.4 en Core
        let mut features_13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);

        let mut features_12 = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .descriptor_indexing(true);

        let mut features_core = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut features_13)
            .push_next(&mut features_12);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info))
            .enabled_extension_names(&device_extension_names)
            .push_next(&mut features_core);

        log::info!("Création du Logical Device Vulkan 1.4...");
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

        log::info!("Swapchain Vulkan 1.4 créé ({x}x{y}, Format: {fmt:?}, Images: {count})", x = extent.width, y = extent.height, fmt = format.format, count = swapchain_images.len());

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
    pub fn sans_ecran(
        largeur: u32,
        hauteur: u32,
        combien_d_images: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Contexte Vulkan SANS ECRAN — aucune fenetre, aucun compositeur dans la mesure");

        let entry = unsafe { ash::Entry::load()? };

        let app_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"AegisEngine Banc\0") };
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::make_api_version(0, 1, 4, 0));

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

        let mut f13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        let mut f12 = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .descriptor_indexing(true);
        let mut fcore = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut f13)
            .push_next(&mut f12);

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

        // Le format est celui qu'un écran nous donnerait : la chaîne de couleur doit être la même,
        // sinon le banc mesurerait un autre rendu que celui qu'on regarde.
        let format = vk::Format::B8G8R8A8_SRGB;
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
    pub fn begin_frame(&mut self, window: Option<&Window>) -> Result<(vk::CommandBuffer, usize), Box<dyn std::error::Error>> {
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
    pub fn end_frame(
        &mut self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        window: Option<&Window>,
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

    /// Redimensionne la taille du Swapchain Vulkan lors du redimensionnement de la fenêtre.
    pub fn resize(&mut self, window: &Window) {
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
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
        log::info!("Ressources Vulkan 1.4 libérées proprement.");
    }
}
