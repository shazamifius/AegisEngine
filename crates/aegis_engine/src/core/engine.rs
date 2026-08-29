use ash::vk;
use crate::core::math::{Mat4, Vec3, Vec4};
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::core::gpu_context::GpuContext;
use crate::core::memory::MemoryManager;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GlassPushConstants {
    pub mvp_matrix: Mat4,
    pub model_matrix: Mat4,
    pub normal_matrix: Mat4,
    pub glass_tint: Vec4,
    pub params: Vec4,
}

#[derive(Clone, Debug)]
pub struct GlassSlabInstance {
    pub position: Vec3,
    pub rotation_z: f32,
    pub rotation_x: f32,
    pub tint: Vec4,
    pub rugosite: f32,
}

/// Moteur de Rendu 3D Principal AegisEngine (Pure Vulkan 1.4 Native).
pub struct Engine {
    pub gpu: GpuContext,
    pub frame_count: u64,
    pub last_update: Instant,
}

impl Engine {
    pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let gpu = GpuContext::new(window)?;

        Ok(Self {
            gpu,
            frame_count: 0,
            last_update: Instant::now(),
        })
    }

    pub fn delta_time(&mut self) -> f32 {
        let dt = self.last_update.elapsed().as_secs_f32().min(0.033);
        self.last_update = Instant::now();
        dt
    }

    pub fn on_resize(&mut self, window: &Window) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.gpu.resize(window);
    }

    /// Rapatrie l'image affichée depuis la carte, en RVB (trois octets par pixel).
    ///
    /// ⚠ Extraite de la capture le 30 août 2026 parce que **deux choses en ont besoin** : écrire
    /// un PNG, et mesurer ce que l'image fait à l'œil. Deux copies de ce transfert auraient
    /// divergé — et la mesure aurait alors porté sur une image qui n'est pas celle qu'on regarde.
    pub fn lire_image(&self) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
        let extent = self.gpu.swapchain_extent;
        let mut rvb = Vec::new();
        self.transferer_image(&mut rvb)?;
        Ok((extent.width, extent.height, rvb))
    }

    /// Mesure la tonalité de ce qui est actuellement affiché.
    ///
    /// ⚠ Ce que ça rend est un INDICATEUR, jamais un verdict : le juge du rendu perçu reste un
    /// œil humain. L'instrument sert à rendre décidable ce qui ne l'était pas — comparer deux
    /// réglages autrement que de mémoire, et nommer ce qui manque.
    pub fn mesurer_tonalite(
        &self,
    ) -> Result<crate::image::tonalite::Analyse, Box<dyn std::error::Error>> {
        let (_, _, rvb) = self.lire_image()?;
        Ok(crate::image::tonalite::analyser(&rvb))
    }

    /// Le transfert Vulkan lui-meme : image de la carte vers un tampon lisible, puis RVB.
    fn transferer_image(&self, sortie: &mut Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let extent = self.gpu.swapchain_extent;
        let format = self.gpu.swapchain_format;
        // ⚠ L'image RÉELLEMENT rendue, pas la première de la liste. Lire `[0]` marchait par
        // accident tant qu'on ne regardait pas de près ; dès qu'on demande une mesure exacte, ça
        // devient une photographie d'une image vieille de plusieurs trames.
        let image = self.gpu.swapchain_images[self.gpu.derniere_image];

        let buffer_size = (extent.width * extent.height * 4) as vk::DeviceSize;
        let memory_props = unsafe { self.gpu.instance.get_physical_device_memory_properties(self.gpu.physical_device) };

        let (staging_buffer, staging_memory) = MemoryManager::create_buffer(
            &self.gpu.device,
            &memory_props,
            buffer_size,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let cmd = self.gpu.begin_single_time_commands()?;

        let barrier_to_src = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::NONE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            self.gpu.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_src],
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
            self.gpu.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[region],
            );
        }

        let barrier_back = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::NONE)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            self.gpu.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_back],
            );
        }

        self.gpu.end_single_time_commands(cmd)?;

        let mut raw_pixels = vec![0u8; buffer_size as usize];
        unsafe {
            let data_ptr = self.gpu.device.map_memory(staging_memory, 0, buffer_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(data_ptr as *const u8, raw_pixels.as_mut_ptr(), buffer_size as usize);
            self.gpu.device.unmap_memory(staging_memory);
            self.gpu.device.destroy_buffer(staging_buffer, None);
            self.gpu.device.free_memory(staging_memory, None);
        }

        if format == vk::Format::B8G8R8A8_SRGB || format == vk::Format::B8G8R8A8_UNORM {
            for pixel in raw_pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        sortie.clear();
        sortie.reserve(raw_pixels.len() / 4 * 3);
        for pixel in raw_pixels.chunks_exact(4) {
            sortie.extend_from_slice(&pixel[..3]);
        }
        Ok(())
    }

    pub fn capture_screenshot(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let extent = self.gpu.swapchain_extent;
        let mut rvb = Vec::new();
        self.transferer_image(&mut rvb)?;

        // ⚠⚠ ICI VIVAIT UN SCRIPT PYTHON, ÉCRIT SUR LE DISQUE PUIS EXÉCUTÉ (30 août 2026).
        //
        // Trois fautes en une : la règle la plus ferme du projet est « QUE du Rust, aucun autre
        // langage » ; la capture échouait chez quiconque n'a pas `python3` ; et **cet échec était
        // avalé** — le code ne journalisait que le succès, sans branche `else`. Le mécanisme
        // serait donc mort chez quelqu'un d'autre, en silence.
        //
        // L'encodeur est maintenant dans le moteur, en Rust, et prouvé par aller-retour (il porte
        // son propre décodeur de test : ce qu'il écrit doit se relire octet pour octet).
        let png = crate::image::png::encoder(extent.width, extent.height, &rvb)
            .map_err(|e| format!("encodage PNG impossible : {e}"))?;
        std::fs::write(path, &png)?;

        log::info!(
            "Capture ecrite : {path} ({}x{}, {} Ko)",
            extent.width,
            extent.height,
            png.len() / 1024
        );

        Ok(())
    }
}
