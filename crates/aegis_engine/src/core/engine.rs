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

    pub fn capture_screenshot(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let extent = self.gpu.swapchain_extent;
        let format = self.gpu.swapchain_format;
        let image = self.gpu.swapchain_images[0];

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

        let temp_ppm = format!("{}.ppm", path);
        let mut ppm_data = format!("P6\n{} {}\n255\n", extent.width, extent.height).into_bytes();
        for pixel in raw_pixels.chunks_exact(4) {
            ppm_data.push(pixel[0]);
            ppm_data.push(pixel[1]);
            ppm_data.push(pixel[2]);
        }

        std::fs::write(&temp_ppm, ppm_data)?;

        let py_script = format!(
            "import zlib, struct, binascii, sys\n\
            with open(sys.argv[1], 'rb') as f:\n\
            \tline1 = f.readline()\n\
            \tline2 = f.readline()\n\
            \twhile line2.startswith(b'#'): line2 = f.readline()\n\
            \tw, h = map(int, line2.split())\n\
            \tf.readline()\n\
            \trgb = f.read()\n\
            raw = bytearray()\n\
            for y in range(h):\n\
            \traw.append(0)\n\
            \traw.extend(rgb[y*w*3:(y+1)*w*3])\n\
            def chunk(t, d):\n\
            \treturn struct.pack('>I', len(d)) + t + d + struct.pack('>I', binascii.crc32(t + d) & 0xffffffff)\n\
            png = bytearray(b'\\x89PNG\\r\\n\\x1a\\n')\n\
            png += chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))\n\
            png += chunk(b'IDAT', zlib.compress(bytes(raw)))\n\
            png += chunk(b'IEND', b'')\n\
            with open(sys.argv[2], 'wb') as f: f.write(png)\n"
        );

        let temp_py = format!("{}.py", path);
        let _ = std::fs::write(&temp_py, py_script);

        let status = std::process::Command::new("python3")
            .arg(&temp_py)
            .arg(&temp_ppm)
            .arg(path)
            .status();

        let _ = std::fs::remove_file(&temp_ppm);
        let _ = std::fs::remove_file(&temp_py);

        if status.as_ref().map(|s| s.success()).unwrap_or(false) {
            log::info!("Screenshot PNG VRAM Vulkan 1.4 généré avec succès : {}", path);
        }

        Ok(())
    }
}
