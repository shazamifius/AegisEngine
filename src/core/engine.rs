use ash::vk;
use glam::{Mat4, Vec3, Vec4};
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::core::gpu_context::GpuContext;
use crate::core::memory::MemoryManager;
use crate::geometry::glass_slab::GlassSlabGenerator;
use crate::geometry::vertex::Vertex;
use crate::render::pipeline::PipelineFactory;
use crate::render::render_graph::RenderPass;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GlassPushConstants {
    pub mvp_matrix: Mat4,
    pub model_matrix: Mat4,
    pub normal_matrix: Mat4,
    pub glass_tint: Vec4,
}

unsafe impl bytemuck::Pod for GlassPushConstants {}
unsafe impl bytemuck::Zeroable for GlassPushConstants {}

#[derive(Clone, Debug)]
pub struct GlassSlabInstance {
    pub position: Vec3,
    pub rotation_z: f32,
    pub rotation_x: f32,
    pub scale: Vec3,
    pub tint: Vec4,
}

/// Dynamic Glass Scene Render Pass (Native Vulkan 1.4 + GLSL + Hardware Mipmap Chain Frosted Blur)
pub struct GlassSceneRenderPass {
    bg_pipeline: vk::Pipeline,
    bg_pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,

    transmission_image: vk::Image,
    _transmission_memory: vk::DeviceMemory,
    transmission_image_view: vk::ImageView,
    transmission_sampler: vk::Sampler,
    transmission_layout: vk::ImageLayout,

    vertex_buffer: vk::Buffer,
    _vertex_memory: vk::DeviceMemory,
    index_buffer: vk::Buffer,
    _index_memory: vk::DeviceMemory,
    index_count: u32,

    depth_image: vk::Image,
    _depth_memory: vk::DeviceMemory,
    depth_image_view: vk::ImageView,

    instances: Vec<GlassSlabInstance>,
    start_time: Instant,
}

impl GlassSceneRenderPass {
    pub fn new(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation de la Scène de Verre Dépoli Vulkan 1.4 Native (GLSL + Mipmap Chain)...");

        let (vertices, indices) = GlassSlabGenerator::create_capsule_slab(2.8, 0.62, 0.12, 0.05, 36);
        let index_count = indices.len() as u32;

        let vertex_bytes = bytemuck::cast_slice(&vertices);
        let (vertex_buffer, vertex_memory) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            vertex_bytes.len() as vk::DeviceSize,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            let data_ptr = gpu.device.map_memory(vertex_memory, 0, vertex_bytes.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(vertex_bytes.as_ptr(), data_ptr as *mut u8, vertex_bytes.len());
            gpu.device.unmap_memory(vertex_memory);
        }

        let index_bytes = bytemuck::cast_slice(&indices);
        let (index_buffer, index_memory) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            index_bytes.len() as vk::DeviceSize,
            vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            let data_ptr = gpu.device.map_memory(index_memory, 0, index_bytes.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), data_ptr as *mut u8, index_bytes.len());
            gpu.device.unmap_memory(index_memory);
        }

        // 1. Z-Buffer Depth Attachment
        let depth_format = vk::Format::D32_SFLOAT;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(depth_format)
            .extent(vk::Extent3D {
                width: gpu.swapchain_extent.width,
                height: gpu.swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let depth_image = unsafe { gpu.device.create_image(&image_info, None)? };
        let mem_reqs = unsafe { gpu.device.get_image_memory_requirements(depth_image) };

        let mem_type = MemoryManager::find_memory_type(memory_props, mem_reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
            .ok_or("Mémoire VRAM non disponible pour le Z-Buffer.")?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type);

        let depth_memory = unsafe { gpu.device.allocate_memory(&alloc_info, None)? };
        unsafe { gpu.device.bind_image_memory(depth_image, depth_memory, 0)? };

        let view_info = vk::ImageViewCreateInfo::default()
            .image(depth_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(depth_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let depth_image_view = unsafe { gpu.device.create_image_view(&view_info, None)? };

        // 2. Offscreen Transmission Image Buffer avec Chaîne de Mipmaps Vulkan Hardware (5 Niveaux de Mipmap)
        let mip_levels = 5u32;
        let trans_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(gpu.swapchain_format)
            .extent(vk::Extent3D {
                width: gpu.swapchain_extent.width,
                height: gpu.swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let transmission_image = unsafe { gpu.device.create_image(&trans_image_info, None)? };
        let trans_mem_reqs = unsafe { gpu.device.get_image_memory_requirements(transmission_image) };
        let trans_mem_type = MemoryManager::find_memory_type(memory_props, trans_mem_reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
            .ok_or("Mémoire VRAM non disponible pour le Transmission Buffer.")?;

        let trans_alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(trans_mem_reqs.size)
            .memory_type_index(trans_mem_type);

        let transmission_memory = unsafe { gpu.device.allocate_memory(&trans_alloc_info, None)? };
        unsafe { gpu.device.bind_image_memory(transmission_image, transmission_memory, 0)? };

        let trans_view_info = vk::ImageViewCreateInfo::default()
            .image(transmission_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(gpu.swapchain_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            });

        let transmission_image_view = unsafe { gpu.device.create_image_view(&trans_view_info, None)? };

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(mip_levels as f32);

        let transmission_sampler = unsafe { gpu.device.create_sampler(&sampler_info, None)? };

        // 3. Layout des Descriptors (Binding 0: Sampled Image, Binding 1: Sampler)
        let layout_bindings = [
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

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&layout_bindings);
        let descriptor_set_layout = unsafe { gpu.device.create_descriptor_set_layout(&layout_info, None)? };

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: 1,
            },
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe { gpu.device.create_descriptor_pool(&pool_info, None)? };

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let descriptor_set = unsafe { gpu.device.allocate_descriptor_sets(&alloc_info)?[0] };

        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(transmission_image_view);

        let sampler_descriptor_info = vk::DescriptorImageInfo::default()
            .sampler(transmission_sampler);

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image_info)),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&sampler_descriptor_info)),
        ];

        unsafe { gpu.device.update_descriptor_sets(&descriptor_writes, &[]) };

        // 4. Compilation des Shaders GLSL Native vers Vulkan SPIR-V
        let bg_vert_spv = PipelineFactory::compile_glsl_to_spirv(
            include_str!("../shaders/background.vert"),
            naga::ShaderStage::Vertex,
        )?;
        let bg_frag_spv = PipelineFactory::compile_glsl_to_spirv(
            include_str!("../shaders/background.frag"),
            naga::ShaderStage::Fragment,
        )?;

        let bg_vert_module = PipelineFactory::create_shader_module(&gpu.device, &bg_vert_spv)?;
        let bg_frag_module = PipelineFactory::create_shader_module(&gpu.device, &bg_frag_spv)?;

        let bg_pipeline_layout = PipelineFactory::create_pipeline_layout(&gpu.device, &[], &[])?;

        let entry_name = std::ffi::CStr::from_bytes_with_nul(b"main\0").unwrap();
        let bg_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(bg_vert_module)
                .name(entry_name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(bg_frag_module)
                .name(entry_name),
        ];

        let bg_vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let bg_rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);

        let bg_depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(false)
            .depth_write_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let bg_color_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false);

        let bg_color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&bg_color_attachment));

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state_info = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&dynamic_states);

        let color_formats = [gpu.swapchain_format];
        let mut bg_rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_formats);

        let bg_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&bg_stages)
            .vertex_input_state(&bg_vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&bg_rasterizer)
            .depth_stencil_state(&bg_depth_stencil)
            .multisample_state(&multisampling)
            .color_blend_state(&bg_color_blending)
            .dynamic_state(&dynamic_state_info)
            .layout(bg_pipeline_layout)
            .push_next(&mut bg_rendering_create_info);

        let bg_pipelines = unsafe {
            gpu.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[bg_pipeline_info], None)
                .map_err(|(_, err)| err)?
        };

        unsafe {
            gpu.device.destroy_shader_module(bg_vert_module, None);
            gpu.device.destroy_shader_module(bg_frag_module, None);
        };
        let bg_pipeline = bg_pipelines[0];

        // 5. Pipeline Verre Dépoli GLSL Native
        let glass_vert_spv = PipelineFactory::compile_glsl_to_spirv(
            include_str!("../shaders/glass_dispersive.vert"),
            naga::ShaderStage::Vertex,
        )?;
        let glass_frag_spv = PipelineFactory::compile_glsl_to_spirv(
            include_str!("../shaders/glass_dispersive.frag"),
            naga::ShaderStage::Fragment,
        )?;

        let glass_vert_module = PipelineFactory::create_shader_module(&gpu.device, &glass_vert_spv)?;
        let glass_frag_module = PipelineFactory::create_shader_module(&gpu.device, &glass_frag_spv)?;

        let push_constant_range = PipelineFactory::create_push_constant_range(
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            std::mem::size_of::<GlassPushConstants>() as u32,
        );

        let pipeline_layout = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            &[descriptor_set_layout],
            &[push_constant_range],
        )?;

        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(glass_vert_module)
                .name(entry_name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(glass_frag_module)
                .name(entry_name),
        ];

        let binding_desc = [Vertex::binding_description()];
        let attribute_descs = Vertex::attribute_descriptions();

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_desc)
            .vertex_attribute_descriptions(&attribute_descs);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);

        let color_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false);

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&color_attachment));

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_formats)
            .depth_attachment_format(depth_format);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .depth_stencil_state(&depth_stencil)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state_info)
            .layout(pipeline_layout)
            .push_next(&mut rendering_create_info);

        let pipelines = unsafe {
            gpu.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, err)| err)?
        };

        unsafe {
            gpu.device.destroy_shader_module(glass_vert_module, None);
            gpu.device.destroy_shader_module(glass_frag_module, None);
        };

        let instances = vec![
            // 1. Dalle Capsule Diagonale Haut-Gauche (BLEU SAPHIR PROFOND AU FOND)
            GlassSlabInstance {
                position: Vec3::new(-0.35, 0.45, -0.30),
                rotation_z: -36.0f32.to_radians(),
                rotation_x: -4.0f32.to_radians(),
                scale: Vec3::new(1.85, 0.58, 0.22),
                tint: Vec4::new(0.04, 0.22, 0.78, 0.85),
            },
            // 2. Dalle Capsule Premier Plan (Diagonale Haut-Droite, VERRE CLAIR CYAN AU-DESSUS)
            GlassSlabInstance {
                position: Vec3::new(0.05, -0.05, 0.35),
                rotation_z: 36.0f32.to_radians(),
                rotation_x: 4.0f32.to_radians(),
                scale: Vec3::new(2.10, 0.65, 0.25),
                tint: Vec4::new(0.85, 0.96, 1.00, 0.15),
            },
            // 3. Dalle Capsule Arrière (Bas-Droite)
            GlassSlabInstance {
                position: Vec3::new(0.35, -0.65, -0.50),
                rotation_z: -36.0f32.to_radians(),
                rotation_x: -4.0f32.to_radians(),
                scale: Vec3::new(1.50, 0.52, 0.18),
                tint: Vec4::new(0.55, 0.78, 0.95, 0.35),
            },
        ];

        log::info!("Pipeline Graphique GLSL Native & Hardware Mipmap Chain Vulkan 1.4 prêt !");

        Ok(Self {
            bg_pipeline,
            bg_pipeline_layout,
            pipeline: pipelines[0],
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            transmission_image,
            _transmission_memory: transmission_memory,
            transmission_image_view,
            transmission_sampler,
            transmission_layout: vk::ImageLayout::UNDEFINED,
            vertex_buffer,
            _vertex_memory: vertex_memory,
            index_buffer,
            _index_memory: index_memory,
            index_count,
            depth_image,
            _depth_memory: depth_memory,
            depth_image_view,
            instances,
            start_time: Instant::now(),
        })
    }
}

impl RenderPass for GlassSceneRenderPass {
    fn name(&self) -> &str {
        "Vulkan 1.4 Native GLSL Glass Scene Render Pass"
    }

    fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, image_index: usize) {
        let view = context.swapchain_image_views[image_index];
        let image = context.swapchain_images[image_index];

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let osc = (elapsed * 0.35).sin() * 0.035;

        let view_matrix = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 4.4), Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
        let aspect_ratio = context.swapchain_extent.width as f32 / context.swapchain_extent.height as f32;
        let mut proj = Mat4::perspective_rh(42.0f32.to_radians(), aspect_ratio, 0.1, 100.0);
        proj.y_axis.y *= -1.0;

        unsafe {
            // A. Transition de l'image Swapchain -> COLOR_ATTACHMENT_OPTIMAL
            let barrier_present_to_color = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let barrier_depth = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::NONE)
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .image(self.depth_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let init_barriers = [barrier_present_to_color, barrier_depth];
            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &init_barriers,
            );

            // B. Rendu du Studio Background
            let color_attachment_info = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.94, 0.96, 0.98, 1.0],
                    },
                });

            let bg_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: context.swapchain_extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&color_attachment_info));

            context.device.cmd_begin_rendering(cmd, &bg_rendering_info);
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.bg_pipeline);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: context.swapchain_extent.width as f32,
                height: context.swapchain_extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: context.swapchain_extent,
            };
            context.device.cmd_set_viewport(cmd, 0, &[viewport]);
            context.device.cmd_set_scissor(cmd, 0, &[scissor]);

            context.device.cmd_draw(cmd, 3, 1, 0, 0);
            context.device.cmd_end_rendering(cmd);

            // C. Effacement du Z-Buffer avant le rendu des dalles
            let depth_clear_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                });

            let dummy_color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            let depth_clear_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: context.swapchain_extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&dummy_color_attachment))
                .depth_attachment(&depth_clear_attachment);

            context.device.cmd_begin_rendering(cmd, &depth_clear_rendering_info);
            context.device.cmd_end_rendering(cmd);

            // D. Boucle Multi-Passes pour chaque dalle avec Génération de Mipmaps Hardware Vulkan (vkCmdBlitImage)
            let mip_levels = 5u32;

            for (idx, instance) in self.instances.iter().enumerate() {
                // 1. Transition Swapchain -> TRANSFER_SRC_OPTIMAL
                let barrier_swapchain_src = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                // 2. Transition Transmission Level 0 -> TRANSFER_DST_OPTIMAL
                let barrier_trans_dst = vk::ImageMemoryBarrier::default()
                    .old_layout(self.transmission_layout)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::NONE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .image(self.transmission_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let barriers_copy = [barrier_swapchain_src, barrier_trans_dst];
                context.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers_copy,
                );

                // Copie Swapchain -> Transmission Image Level 0
                let copy_region = vk::ImageCopy::default()
                    .src_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .dst_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .extent(vk::Extent3D {
                        width: context.swapchain_extent.width,
                        height: context.swapchain_extent.height,
                        depth: 1,
                    });

                context.device.cmd_copy_image(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    self.transmission_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy_region],
                );

                // 3. Génération de Mipmaps Matérielle Vulkan (Blit Hardware Mip 0 -> Mip 1 -> Mip 2 -> Mip 3 -> Mip 4)
                let mut mip_w = context.swapchain_extent.width as i32;
                let mut mip_h = context.swapchain_extent.height as i32;

                for i in 1..mip_levels {
                    let prev_barrier = vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                        .image(self.transmission_image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: i - 1,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        });

                    let next_barrier = vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::NONE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .image(self.transmission_image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: i,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        });

                    let mip_barriers = [prev_barrier, next_barrier];
                    context.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &mip_barriers,
                    );

                    let next_w = if mip_w > 1 { mip_w / 2 } else { 1 };
                    let next_h = if mip_h > 1 { mip_h / 2 } else { 1 };

                    let blit = vk::ImageBlit::default()
                        .src_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: i - 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .src_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D { x: mip_w, y: mip_h, z: 1 },
                        ])
                        .dst_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: i,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .dst_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D { x: next_w, y: next_h, z: 1 },
                        ]);

                    context.device.cmd_blit_image(
                        cmd,
                        self.transmission_image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        self.transmission_image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[blit],
                        vk::Filter::LINEAR,
                    );

                    mip_w = next_w;
                    mip_h = next_h;
                }

                // 4. Transition Mip 0..3 et Mip 4 -> SHADER_READ_ONLY_OPTIMAL
                let barrier_mip_0_to_3 = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(self.transmission_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: mip_levels - 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let barrier_mip_4 = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(self.transmission_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: mip_levels - 1,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let barrier_swapchain_back = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let barriers_post_copy = [barrier_mip_0_to_3, barrier_mip_4, barrier_swapchain_back];
                context.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers_post_copy,
                );

                self.transmission_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                // 5. Rendu de la dalle avec Shading GLSL & Mipmaps Vulkan Hardware
                let color_attachment_slab = vk::RenderingAttachmentInfo::default()
                    .image_view(view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE);

                let depth_attachment_slab = vk::RenderingAttachmentInfo::default()
                    .image_view(self.depth_image_view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE);

                let rendering_info_slab = vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: context.swapchain_extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment_slab))
                    .depth_attachment(&depth_attachment_slab);

                context.device.cmd_begin_rendering(cmd, &rendering_info_slab);
                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

                context.device.cmd_set_viewport(cmd, 0, &[viewport]);
                context.device.cmd_set_scissor(cmd, 0, &[scissor]);

                let rot_y = if idx == 1 { osc } else { -osc };
                let model_matrix = Mat4::from_translation(instance.position)
                    * Mat4::from_rotation_z(instance.rotation_z)
                    * Mat4::from_rotation_y(rot_y)
                    * Mat4::from_rotation_x(instance.rotation_x);

                let mvp_matrix = proj * view_matrix * model_matrix;
                let normal_matrix = model_matrix.inverse().transpose();

                let push_constants = GlassPushConstants {
                    mvp_matrix,
                    model_matrix,
                    normal_matrix,
                    glass_tint: instance.tint,
                };

                let push_bytes = bytemuck::bytes_of(&push_constants);
                context.device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    push_bytes,
                );

                context.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[self.descriptor_set],
                    &[],
                );

                context.device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
                context.device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);

                context.device.cmd_draw_indexed(cmd, self.index_count, 1, 0, 0, 0);

                context.device.cmd_end_rendering(cmd);
            }

            // E. Transition Finale Swapchain -> PRESENT_SRC_KHR
            let barrier_color_to_present = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::NONE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_color_to_present],
            );
        }
    }
}

/// Moteur de Rendu 3D Principal AegisEngine (Pure Vulkan 1.4 Native).
pub struct Engine {
    pub gpu: GpuContext,
    pub render_pass: GlassSceneRenderPass,
    pub frame_count: u64,
}

impl Engine {
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let gpu = GpuContext::new(window).await?;
        let memory_props = unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };
        let render_pass = GlassSceneRenderPass::new(&gpu, &memory_props)?;

        Ok(Self {
            gpu,
            render_pass,
            frame_count: 0,
        })
    }

    pub fn render_frame(&mut self, window: &Window) {
        if let Ok((cmd, image_index)) = self.gpu.begin_frame() {
            self.render_pass.execute(&self.gpu, cmd, image_index);
            let _ = self.gpu.end_frame(cmd, image_index, window);
            self.frame_count += 1;
        }
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
        let convert_status = std::process::Command::new("magick")
            .arg("convert")
            .arg(&temp_ppm)
            .arg(path)
            .status()
            .or_else(|_| std::process::Command::new("convert").arg(&temp_ppm).arg(path).status())?;

        let _ = std::fs::remove_file(&temp_ppm);

        if convert_status.success() {
            log::info!("Screenshot PNG VRAM Vulkan 1.4 généré avec succès : {}", path);
        }

        Ok(())
    }
}
