use ash::vk;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use crate::core::gpu_context::GpuContext;
use crate::core::memory::MemoryManager;

/// Une image échantillonnable par un shader — **plate ou volumique**.
///
/// # ⭐ Pourquoi la dimension n'est pas dans le nom (4 septembre 2026)
///
/// Cette structure s'appelait `Texture2D`, et le volume de matière de la sucette a demandé une
/// image à trois dimensions. Le geste court était d'écrire une `Texture3D` à côté : deux cents
/// lignes identiques au caractère près, sauf trois champs. **Ça aurait « marché » le soir même, et
/// créé deux textes à faire évoluer en parallèle** — donc, tôt ou tard, deux comportements.
///
/// *C'est la leçon du portage Linux du 7 août 2026, retrouvée ailleurs : quand une chose manque
/// dans un cas, la question n'est pas « que faut-il ajouter ici » mais « qu'est-ce qui n'aurait
/// jamais dû vivre là ».* Ici, ce qui n'aurait jamais dû vivre dans le nom, c'est la dimension.
///
/// # ⚠⚠ ET LA DIMENSION NE SE DÉDUIT PAS DE LA TAILLE — payé le jour même
///
/// La première version décidait toute seule : `si profondeur > 1, alors volume`. C'était joli, et
/// **faux**. Le volume neutre de la passe de verre mesure un seul texel : il était donc créé
/// comme une image **plate**, pendant que le shader réclamait une texture à trois dimensions.
/// Descripteur incompatible, lecture indéfinie — le volume rendait **zéro**, l'absorption
/// disparaissait, et *aucune erreur n'était signalée nulle part*.
///
/// **Un volume d'un seul texel reste un volume.** La dimension est une **intention de
/// l'appelant**, pas une propriété qu'on mesure sur les données. *Deviner à sa place produit
/// exactement ce que ce moteur redoute le plus : une image plausible et fausse.*
pub struct Texture {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub width: u32,
    pub height: u32,
    /// 1 pour une image plate — mais aussi pour un volume d'une seule couche : **ce champ ne dit
    /// pas la dimension**, il dit combien de couches. Voir l'avertissement ci-dessus.
    pub depth: u32,
}

impl Texture {
    /// Crée une texture 1x1 par défaut (couleur unie RGBA8) dans la memoire de la carte.
    pub fn create_solid_color(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        color: [u8; 4],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_from_rgba8(gpu, memory_props, 1, 1, &color)
    }

    /// Rend à la carte tout ce que cette texture lui a pris.
    ///
    /// ⚠⚠ **Cette fonction n'existait pas avant le 2 septembre 2026, et personne ne libérait une
    /// une texture — ni `Drop`, ni geste explicite.** Sur un programme qui charge ses textures une
    /// fois au démarrage, ça ne se voit jamais : le pilote nettoie à la sortie du processus. Ça se
    /// voit dès qu'une texture est créée **par image ou par test**, et c'est exactement ce que fait
    /// la passe de verre.
    ///
    /// *Aucun warning ne pouvait le dire : construire un objet et l'oublier est un usage valide.*
    /// C'est la même famille que la fuite des images du contexte sans écran, trouvée la veille au
    /// même endroit — **une ressource Vulkan qui n'a pas de geste de libération n'en aura jamais**.
    ///
    /// Un `Drop` aurait été plus sûr encore, et il n'est **pas** posé volontairement : l'ordre de
    /// destruction compte en Vulkan, et une texture libérée après son `Device` planterait. Le
    /// moteur libère explicitement partout ailleurs ; une exception ici serait un piège.
    ///
    /// # Sûreté
    ///
    /// L'appelant garantit que plus aucune commande en vol ne lit cette texture.
    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }

    /// Charge une texture VRAM à partir de données RGBA8 brutes.
    pub fn create_from_rgba8(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_from_bytes(
            gpu,
            memory_props,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            4,
            pixels,
        )
    }

    /// Charge une texture VRAM à partir d'octets bruts, dans le format qu'on lui donne.
    ///
    /// ⚠ **Le format était en dur (`R8G8B8A8_SRGB`) jusqu'au 2 septembre 2026**, ce qui rendait
    /// cette fonction incapable de porter autre chose qu'une couleur. Or une carte de géométrie —
    /// une normale et une distance par pixel — a besoin de **flottants** : sur huit bits, des
    /// distances qui vont de 3 à 5 dans une scène deviennent des marches d'escalier, et la mesure
    /// porterait sur les marches plutôt que sur la géométrie.
    ///
    /// *Le sampler reste en `NEAREST`, et c'est essentiel ici : interpoler deux normales de part
    /// et d'autre d'une silhouette fabrique une normale qui n'existe sur aucune surface.*
    pub fn create_from_bytes(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
        format: vk::Format,
        octets_par_pixel: u32,
        pixels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::creer(
            gpu,
            memory_props,
            width,
            height,
            1,
            false,
            format,
            octets_par_pixel,
            vk::Filter::NEAREST,
            vk::SamplerAddressMode::REPEAT,
            pixels,
        )
    }

    /// ⭐⭐ **UN VOLUME DE MATIÈRE** — ce que le rayon traverse, et non plus seulement la forme
    /// qu'il rencontre.
    ///
    /// # Pourquoi il existe (4 septembre 2026)
    ///
    /// Le shader de réfraction savait absorber selon Beer-Lambert avec **un seul `σ` pour tout le
    /// trajet** : un milieu homogène, donc un verre teinté et rien d'autre. Une sucette de sucre
    /// bleu a un feuillet de colorant mal mélangé et des bulles — *la matière change le long du
    /// rayon*, et aucune formule fermée ne dit ça.
    ///
    /// **La géométrie entre dans le shader par deux cartes plates ; la matière y entre par ce
    /// volume, et par lui seul.** Le shader ne connaît donc aucune sucette : il échantillonne ce
    /// qu'on lui donne. *Écrire les bulles dans le shader aurait gravé une décision d'artiste
    /// dans le moteur — exactement la faute du voxel du 31 août, qu'un raccourci qui fonctionne
    /// rend si tentante.*
    ///
    /// # Deux réglages qui ne sont pas ceux d'une carte, et pourquoi
    ///
    /// - **`LINEAR`** : ici, l'interpolation est ce qu'on veut. *Pour une carte de normales elle
    ///   est un défaut — mélanger deux normales de part et d'autre d'une silhouette fabrique une
    ///   normale qui n'existe nulle part. Pour de la matière, deux échantillons voisins décrivent
    ///   le même milieu, et l'interpolation dit la vérité entre eux.*
    /// - **`CLAMP_TO_EDGE`** : un rayon qui sort du volume doit trouver le bord, jamais l'autre
    ///   côté. *Avec `REPEAT`, la matière d'une face reviendrait par la face opposée — une image
    ///   parfaitement plausible et fausse.*
    pub fn create_volume(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        taille: [u32; 3],
        format: vk::Format,
        octets_par_texel: u32,
        texels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::creer(
            gpu,
            memory_props,
            taille[0],
            taille[1],
            taille[2],
            true,
            format,
            octets_par_texel,
            vk::Filter::LINEAR,
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
            texels,
        )
    }

    /// ⭐ **UNE CARTE D'ENVIRONNEMENT** — plate comme une image, mais bouclée comme un horizon.
    ///
    /// La seule différence avec [`Self::create_from_bytes`] est le comportement aux bords, et elle
    /// est décisive :
    ///
    /// - **`u` boucle (`REPEAT`)** : l'azimut fait le tour. *Avec `CLAMP`, la couture derrière la
    ///   caméra fige une bande de pixels — un défaut qu'on attribuerait ensuite à la géométrie.*
    /// - **`v` se fige (`CLAMP_TO_EDGE`)** : l'élévation ne boucle pas, le zénith est un pôle.
    /// - **`LINEAR`** : deux directions voisines regardent le même monde ; interpoler entre elles
    ///   dit la vérité, contrairement à deux normales de part et d'autre d'une silhouette.
    pub fn create_environnement(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        largeur: u32,
        hauteur: u32,
        format: vk::Format,
        octets_par_pixel: u32,
        pixels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::creer_complet(
            gpu,
            memory_props,
            largeur,
            hauteur,
            1,
            false,
            format,
            octets_par_pixel,
            vk::Filter::LINEAR,
            vk::SamplerAddressMode::REPEAT,
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
            pixels,
        )
    }

    /// Le corps commun aux deux. **`volumique` est dit, jamais deviné** — voir l'avertissement
    /// sur [`Texture`].
    #[allow(clippy::too_many_arguments)]
    fn creer(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
        depth: u32,
        volumique: bool,
        format: vk::Format,
        octets_par_pixel: u32,
        filtre: vk::Filter,
        bordure: vk::SamplerAddressMode,
        pixels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::creer_complet(
            gpu,
            memory_props,
            width,
            height,
            depth,
            volumique,
            format,
            octets_par_pixel,
            filtre,
            bordure,
            bordure,
            pixels,
        )
    }

    /// Le corps réel — le seul endroit où une image Vulkan est construite dans ce moteur.
    ///
    /// `bordure_u` et `bordure_v` sont distinctes parce qu'un horizon **boucle en azimut et pas en
    /// élévation** : c'est la seule chose qui sépare une carte d'environnement d'une image plate.
    #[allow(clippy::too_many_arguments)]
    fn creer_complet(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
        depth: u32,
        volumique: bool,
        format: vk::Format,
        octets_par_pixel: u32,
        filtre: vk::Filter,
        bordure_u: vk::SamplerAddressMode,
        bordure_v: vk::SamplerAddressMode,
        pixels: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let image_size = (width * height * depth * octets_par_pixel) as vk::DeviceSize;

        // 1. Staging Buffer
        let (staging_buffer, staging_memory) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        unsafe {
            let data_ptr = gpu.device.map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), data_ptr as *mut u8, pixels.len());
            gpu.device.unmap_memory(staging_memory);
        }

        // 2. Image en pavage optimal
        // ⚠ La dimension vient de l'appelant, JAMAIS de la profondeur — un volume d'un seul
        // texel est un volume.
        let image_info = vk::ImageCreateInfo::default()
            .image_type(if volumique { vk::ImageType::TYPE_3D } else { vk::ImageType::TYPE_2D })
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { gpu.device.create_image(&image_info, None)? };
        let mem_reqs = unsafe { gpu.device.get_image_memory_requirements(image) };
        let mem_type = MemoryManager::find_memory_type(memory_props, mem_reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
            .ok_or("Impossible de trouver de la VRAM pour la Texture")?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type);

        let memory = unsafe { gpu.device.allocate_memory(&alloc_info, None)? };
        unsafe { gpu.device.bind_image_memory(image, memory, 0)? };

        // 3. Transfert Buffer Staging -> Image GPU via Command Buffer
        unsafe {
            let cmd_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(gpu.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd = gpu.device.allocate_command_buffers(&cmd_info)?[0];

            let begin_info = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            gpu.device.begin_command_buffer(cmd, &begin_info)?;

            // Transition UNDEFINED -> TRANSFER_DST
            let barrier_to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            gpu.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_transfer],
            );

            // Copy Staging Buffer -> Image
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
                    width,
                    height,
                    depth,
                });

            gpu.device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            // Transition TRANSFER_DST -> SHADER_READ_ONLY
            let barrier_to_shader = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            gpu.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_shader],
            );

            gpu.device.end_command_buffer(cmd)?;

            let cmds = [cmd];
            let submit_info = vk::SubmitInfo::default().command_buffers(&cmds);
            let submits = [submit_info];
            gpu.device.queue_submit(gpu.graphics_queue.queue, &submits, vk::Fence::null())?;
            gpu.device.queue_wait_idle(gpu.graphics_queue.queue)?;

            gpu.device.free_command_buffers(gpu.command_pool, &[cmd]);
            gpu.device.destroy_buffer(staging_buffer, None);
            gpu.device.free_memory(staging_memory, None);
        }

        // 4. ImageView Vulkan
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(if volumique { vk::ImageViewType::TYPE_3D } else { vk::ImageViewType::TYPE_2D })
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let view = unsafe { gpu.device.create_image_view(&view_info, None)? };

        // 5. Sampler VRAM Vulkan
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(filtre)
            .min_filter(filtre)
            .address_mode_u(bordure_u)
            .address_mode_v(bordure_v)
            .address_mode_w(bordure_v)
            .anisotropy_enable(false)
            .unnormalized_coordinates(false);

        let sampler = unsafe { gpu.device.create_sampler(&sampler_info, None)? };

        log::info!("Texture VRAM creee ({width}x{height}x{depth}).");

        Ok(Self {
            image,
            memory,
            view,
            sampler,
            width,
            height,
            depth,
        })
    }

    /// Tente de charger un fichier d'image PNG/PPM/BMP natif ou retourne une texture fallback par défaut.
    pub fn load_file_or_fallback(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        path: impl AsRef<Path>,
        fallback_color: [u8; 4],
    ) -> Self {
        let p = path.as_ref();
        if p.exists() {
            if let Ok(mut f) = File::open(p) {
                let mut bytes = Vec::new();
                if f.read_to_end(&mut bytes).is_ok() {
                    // Try parsing uncompressed PPM or raw binary grid
                    if let Ok(tex) = Self::parse_raw_or_ppm(gpu, memory_props, &bytes) {
                        return tex;
                    }
                }
            }
        }
        Self::create_solid_color(gpu, memory_props, fallback_color).unwrap()
    }

    fn parse_raw_or_ppm(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        bytes: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Minimal PPM (P6) binary format parser (Zero external dependencies)
        if bytes.len() > 10 && &bytes[0..2] == b"P6" {
            let mut pos = 3;
            while pos < bytes.len() && (bytes[pos] == b'#' || bytes[pos].is_ascii_whitespace()) {
                if bytes[pos] == b'#' {
                    while pos < bytes.len() && bytes[pos] != b'\n' { pos += 1; }
                }
                pos += 1;
            }
            let end_idx = (pos + 30).min(bytes.len());
            let header_str = std::str::from_utf8(&bytes[pos..end_idx]).unwrap_or("");
            let parts: Vec<&str> = header_str.split_whitespace().collect();
            if parts.len() >= 3 {
                let w: u32 = parts[0].parse().unwrap_or(0);
                let h: u32 = parts[1].parse().unwrap_or(0);
                let _max_val: u32 = parts[2].parse().unwrap_or(255);

                if w > 0 && h > 0 {
                    // Find start of pixel data (after max_val + 1 whitespace byte)
                    let data_start = bytes.len() - (w * h * 3) as usize;
                    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                    for i in (data_start..bytes.len()).step_by(3) {
                        if i + 2 < bytes.len() {
                            rgba.push(bytes[i]);
                            rgba.push(bytes[i+1]);
                            rgba.push(bytes[i+2]);
                            rgba.push(255);
                        }
                    }
                    if rgba.len() == (w * h * 4) as usize {
                        return Self::create_from_rgba8(gpu, memory_props, w, h, &rgba);
                    }
                }
            }
        }
        Err("Format non supporté".into())
    }
}
