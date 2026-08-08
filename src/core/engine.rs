use ash::vk;
use std::sync::Arc;
use std::time::Instant;
use glam::{Mat4, Vec3, Vec4};
use crate::core::gpu_context::GpuContext;
use crate::core::memory::MemoryManager;
use crate::geometry::glass_slab::GlassSlabGenerator;
use crate::geometry::vertex::Vertex;
use crate::render::pipeline::PipelineFactory;
use crate::render::render_graph::{RenderPass, RenderGraph};
use winit::window::Window;

/// Payload de Push Constants transféré instantanément au shader de Verre (144 octets).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlassPushConstants {
    mvp_matrix: [f32; 16],
    model_matrix: [f32; 16],
    normal_matrix: [f32; 16],
    glass_tint: [f32; 4],
}

/// Dalle Capsule de Verre Dépoli dans la Scène 3D.
struct GlassSlabInstance {
    position: Vec3,
    rotation_z: f32,
    rotation_x: f32,
    tint: Vec4,
}

/// Passe de Rendu 3D Photoréaliste de Dalles Capsule en Verre Dépoli Satiné sous Vulkan 1.4 Native.
struct GlassSceneRenderPass {
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
        log::info!("Initialisation de la Scène de Verre Dépoli Satiné Photoréaliste Vulkan 1.4...");

        let (vertices, indices) = GlassSlabGenerator::create_capsule_slab(2.8, 0.62, 0.12, 0.05, 36);
        let index_count = indices.len() as u32;

        let vertex_bytes = bytemuck::cast_slice(&vertices);
        let index_bytes = bytemuck::cast_slice(&indices);

        let (vertex_buffer, vertex_memory) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            vertex_bytes.len() as vk::DeviceSize,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let (index_buffer, index_memory) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            index_bytes.len() as vk::DeviceSize,
            vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        unsafe {
            let data_ptr = gpu.device.map_memory(vertex_memory, 0, vertex_bytes.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(vertex_bytes.as_ptr(), data_ptr as *mut u8, vertex_bytes.len());
            gpu.device.unmap_memory(vertex_memory);

            let data_ptr = gpu.device.map_memory(index_memory, 0, index_bytes.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), data_ptr as *mut u8, index_bytes.len());
            gpu.device.unmap_memory(index_memory);
        }

        // 1. Image Z-Buffer (Profondeur)
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

        // 2. Offscreen Transmission Image Buffer (pour réfraction et flou dépoli)
        let trans_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(gpu.swapchain_format)
            .extent(vk::Extent3D {
                width: gpu.swapchain_extent.width,
                height: gpu.swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
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
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let transmission_image_view = unsafe { gpu.device.create_image_view(&trans_view_info, None)? };

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);

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

        let descriptor_image_info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: transmission_image_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];

        let descriptor_sampler_info = [vk::DescriptorImageInfo {
            sampler: transmission_sampler,
            image_view: vk::ImageView::null(),
            image_layout: vk::ImageLayout::UNDEFINED,
        }];

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&descriptor_image_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&descriptor_sampler_info),
        ];

        unsafe { gpu.device.update_descriptor_sets(&descriptor_writes, &[]) };

        // 4. Pipeline Fond Studio
        let bg_wgsl_source = include_str!("../shaders/background.wgsl");
        let bg_spirv = PipelineFactory::compile_wgsl_to_spirv(bg_wgsl_source)?;
        let bg_shader_module = PipelineFactory::create_shader_module(&gpu.device, &bg_spirv)?;

        let bg_pipeline_layout = PipelineFactory::create_pipeline_layout(&gpu.device, &[], &[])?;

        let entry_name = unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(b"vs_main\0") };
        let fs_entry_name = unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(b"fs_main\0") };

        let bg_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(bg_shader_module)
                .name(entry_name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(bg_shader_module)
                .name(fs_entry_name),
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

        unsafe { gpu.device.destroy_shader_module(bg_shader_module, None) };
        let bg_pipeline = bg_pipelines[0];

        // 5. Pipeline Verre Dépoli
        let wgsl_source = include_str!("../shaders/glass_dispersive.wgsl");
        let spirv_code = PipelineFactory::compile_wgsl_to_spirv(wgsl_source)?;
        let shader_module = PipelineFactory::create_shader_module(&gpu.device, &spirv_code)?;

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
                .module(shader_module)
                .name(entry_name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_module)
                .name(fs_entry_name),
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

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&color_blend_attachment));

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

        unsafe { gpu.device.destroy_shader_module(shader_module, None) };

        let instances = vec![
            // 1. Dalle Capsule Arrière (Fond Bleu Glacial Dépoli)
            GlassSlabInstance {
                position: Vec3::new(-0.20, -0.65, -0.45),
                rotation_z: -38.0f32.to_radians(),
                rotation_x: -6.0f32.to_radians(),
                tint: Vec4::new(0.60, 0.85, 0.98, 0.32),
            },
            // 2. Dalle Capsule Intermédiaire (Bleu Saphir Profond)
            GlassSlabInstance {
                position: Vec3::new(0.12, 0.05, 0.00),
                rotation_z: 38.0f32.to_radians(),
                rotation_x: 4.0f32.to_radians(),
                tint: Vec4::new(0.05, 0.38, 0.95, 0.70),
            },
            // 3. Dalle Capsule Premier Plan (Verre Clair avec Liseré Cyan Électrique #00C2FF)
            GlassSlabInstance {
                position: Vec3::new(-0.15, 0.65, 0.45),
                rotation_z: -38.0f32.to_radians(),
                rotation_x: -6.0f32.to_radians(),
                tint: Vec4::new(0.82, 0.94, 1.00, 0.16),
            },
        ];

        log::info!("Pipeline Graphique de Verre Dépoli Satiné & Buffer Offscreen Vulkan 1.4 prêt !");

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
        "Vulkan 1.4 Photoreal Dispersive Glass Render Pass"
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

            // --- PASSE 1 : Fond Studio Texturé Photoréaliste ---
            let barrier_color_bg = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let dep_bg = vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier_color_bg));
            context.device.cmd_pipeline_barrier2(cmd, &dep_bg);

            let bg_color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] },
                });

            let bg_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&bg_color_attachment));

            context.device.cmd_begin_rendering(cmd, &bg_rendering_info);
            context.device.cmd_set_viewport(cmd, 0, &[viewport]);
            context.device.cmd_set_scissor(cmd, 0, &[scissor]);
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.bg_pipeline);
            context.device.cmd_draw(cmd, 3, 1, 0, 0);
            context.device.cmd_end_rendering(cmd);

            // --- Initialisation du Z-Buffer ---
            let barrier_depth = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS)
                .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .image(self.depth_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let dep_depth = vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier_depth));
            context.device.cmd_pipeline_barrier2(cmd, &dep_depth);

            let depth_clear_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } });
            let depth_clear_rendering = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent })
                .layer_count(1)
                .depth_attachment(&depth_clear_attachment);
            context.device.cmd_begin_rendering(cmd, &depth_clear_rendering);
            context.device.cmd_end_rendering(cmd);

            // --- PASSE 2 : Rendu Multi-Passes des Dalles Capsule avec Offscreen Transmission Buffer ---
            for instance in &self.instances {
                // 1. Transition swapchain image -> TRANSFER_SRC_OPTIMAL
                let b1 = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                    .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                // 2. Transition transmission_image -> TRANSFER_DST_OPTIMAL
                let b2 = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER | vk::PipelineStageFlags2::TOP_OF_PIPE)
                    .src_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::NONE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .old_layout(self.transmission_layout)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .image(self.transmission_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let trans_barriers = [b1, b2];
                let dep_trans = vk::DependencyInfo::default().image_memory_barriers(&trans_barriers);
                context.device.cmd_pipeline_barrier2(cmd, &dep_trans);

                // 3. Copy swapchain color attachment -> transmission_image
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

                // 4. Transition swapchain image -> COLOR_ATTACHMENT_OPTIMAL
                let b3 = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
                    .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                    .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                // 5. Transition transmission_image -> SHADER_READ_ONLY_OPTIMAL
                let b4 = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(self.transmission_image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                self.transmission_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                let back_barriers = [b3, b4];
                let dep_trans_back = vk::DependencyInfo::default().image_memory_barriers(&back_barriers);
                context.device.cmd_pipeline_barrier2(cmd, &dep_trans_back);

                // 6. Dessin de la Dalle Translucide
                let slab_color_attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE);

                let slab_depth_attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(self.depth_image_view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE);

                let slab_rendering_info = vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&slab_color_attachment))
                    .depth_attachment(&slab_depth_attachment);

                context.device.cmd_begin_rendering(cmd, &slab_rendering_info);
                context.device.cmd_set_viewport(cmd, 0, &[viewport]);
                context.device.cmd_set_scissor(cmd, 0, &[scissor]);

                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
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

                let model = Mat4::from_translation(instance.position + Vec3::new(0.0, osc, 0.0))
                    * Mat4::from_rotation_z(instance.rotation_z)
                    * Mat4::from_rotation_x(instance.rotation_x);

                let mvp = proj * view_matrix * model;
                let normal_matrix = model.inverse().transpose();

                let push_payload = GlassPushConstants {
                    mvp_matrix: mvp.to_cols_array(),
                    model_matrix: model.to_cols_array(),
                    normal_matrix: normal_matrix.to_cols_array(),
                    glass_tint: instance.tint.to_array(),
                };

                let push_bytes = bytemuck::bytes_of(&push_payload);
                context.device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    push_bytes,
                );

                context.device.cmd_draw_indexed(cmd, self.index_count, 1, 0, 0, 0);

                context.device.cmd_end_rendering(cmd);
            }

            // --- Transition finale pour Présentation à l'écran ---
            let barrier_to_present = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let dependency_info_present = vk::DependencyInfo::default()
                .image_memory_barriers(std::slice::from_ref(&barrier_to_present));

            context.device.cmd_pipeline_barrier2(cmd, &dependency_info_present);
        }
    }
}

/// Moteur principal orchestrant le cycle de trame Vulkan 1.4 Native.
pub struct Engine {
    pub gpu: GpuContext,
    pub render_graph: RenderGraph,
    pub command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
    pub image_available_semaphore: vk::Semaphore,
    pub render_finished_semaphore: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
    pub frame_count: u64,
    glass_pass: GlassSceneRenderPass,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
}

impl Engine {
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let gpu = GpuContext::new(window).await?;
        let memory_properties = unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };
        let render_graph = RenderGraph::new();

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(gpu.graphics_queue.family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        let command_pool = unsafe { gpu.device.create_command_pool(&pool_info, None)? };

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let command_buffers = unsafe { gpu.device.allocate_command_buffers(&alloc_info)? };
        let command_buffer = command_buffers[0];

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let image_available_semaphore = unsafe { gpu.device.create_semaphore(&semaphore_info, None)? };
        let render_finished_semaphore = unsafe { gpu.device.create_semaphore(&semaphore_info, None)? };
        let in_flight_fence = unsafe { gpu.device.create_fence(&fence_info, None)? };

        let glass_pass = GlassSceneRenderPass::new(&gpu, &memory_properties)?;

        Ok(Self {
            gpu,
            render_graph,
            command_pool,
            command_buffer,
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
            frame_count: 0,
            glass_pass,
            memory_properties,
        })
    }

    pub fn capture_screenshot(&mut self, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let width = self.gpu.swapchain_extent.width;
        let height = self.gpu.swapchain_extent.height;
        let size = (width * height * 4) as vk::DeviceSize;

        let (staging_buffer, staging_memory) = MemoryManager::create_buffer(
            &self.gpu.device,
            &self.memory_properties,
            size,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        unsafe {
            let cmd_alloc = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            let cmd = self.gpu.device.allocate_command_buffers(&cmd_alloc)?[0];

            let begin_info = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.gpu.device.begin_command_buffer(cmd, &begin_info)?;

            let image = self.gpu.swapchain_images[0];

            let barrier_src = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let dep1 = vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier_src));
            self.gpu.device.cmd_pipeline_barrier2(cmd, &dep1);

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
                .image_extent(vk::Extent3D { width, height, depth: 1 });

            self.gpu.device.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, staging_buffer, &[region]);

            let barrier_dst = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let dep2 = vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier_dst));
            self.gpu.device.cmd_pipeline_barrier2(cmd, &dep2);

            self.gpu.device.end_command_buffer(cmd)?;

            let command_buffers = [cmd];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
            self.gpu.device.queue_submit(self.gpu.graphics_queue.queue, &[submit_info], vk::Fence::null())?;
            self.gpu.device.queue_wait_idle(self.gpu.graphics_queue.queue)?;

            self.gpu.device.free_command_buffers(self.command_pool, &[cmd]);

            let ptr = self.gpu.device.map_memory(staging_memory, 0, size, vk::MemoryMapFlags::empty())? as *const u8;
            let bgra_slice = std::slice::from_raw_parts(ptr, size as usize);

            let mut rgb_bytes = Vec::with_capacity((width * height * 3) as usize);
            for pixel in bgra_slice.chunks_exact(4) {
                rgb_bytes.push(pixel[2]); // R
                rgb_bytes.push(pixel[1]); // G
                rgb_bytes.push(pixel[0]); // B
            }

            self.gpu.device.unmap_memory(staging_memory);
            self.gpu.device.destroy_buffer(staging_buffer, None);
            self.gpu.device.free_memory(staging_memory, None);

            use std::io::Write;
            let is_png = output_path.ends_with(".png");
            let ppm_path = if is_png {
                format!("{}.ppm", output_path)
            } else {
                output_path.to_string()
            };

            let mut file = std::fs::File::create(&ppm_path)?;
            let header = format!("P6\n{} {}\n255\n", width, height);
            file.write_all(header.as_bytes())?;
            file.write_all(&rgb_bytes)?;

            if is_png {
                let status = std::process::Command::new("convert")
                    .arg(&ppm_path)
                    .arg(output_path)
                    .status();
                let _ = std::fs::remove_file(&ppm_path);
                if let Ok(s) = status {
                    if s.success() {
                        log::info!("Screenshot PNG VRAM Vulkan 1.4 généré avec succès : {}", output_path);
                    } else {
                        log::warn!("Échec de conversion PNG via convert, fichier PPM conservé.");
                    }
                }
            } else {
                log::info!("Screenshot VRAM Vulkan 1.4 exporté avec succès : {}", output_path);
            }
        }

        Ok(())
    }

    pub fn render_frame(&mut self, window: &Window) {
        self.frame_count += 1;
        unsafe {
            let _ = self.gpu.device.wait_for_fences(&[self.in_flight_fence], true, u64::MAX);

            let (image_index, _is_suboptimal) = match self.gpu.swapchain_loader.acquire_next_image(
                self.gpu.swapchain,
                u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            ) {
                Ok(res) => res,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.gpu.resize(window);
                    return;
                }
                Err(e) => {
                    log::error!("Erreur d'acquisition de l'image swapchain: {:?}", e);
                    return;
                }
            };

            let _ = self.gpu.device.reset_fences(&[self.in_flight_fence]);
            let _ = self.gpu.device.reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty());

            let begin_info = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            let _ = self.gpu.device.begin_command_buffer(self.command_buffer, &begin_info);

            self.glass_pass.execute(&self.gpu, self.command_buffer, image_index as usize);

            let _ = self.gpu.device.end_command_buffer(self.command_buffer);

            let wait_semaphores = [self.image_available_semaphore];
            let signal_semaphores = [self.render_finished_semaphore];
            let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let command_buffers = [self.command_buffer];

            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_dst_stage_mask)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);

            let _ = self.gpu.device.queue_submit(self.gpu.graphics_queue.queue, &[submit_info], self.in_flight_fence);

            let swapchains = [self.gpu.swapchain];
            let image_indices = [image_index];

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            let result = self.gpu.swapchain_loader.queue_present(self.gpu.graphics_queue.queue, &present_info);

            if result == Err(vk::Result::ERROR_OUT_OF_DATE_KHR) || result == Ok(true) {
                self.gpu.resize(window);
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            let _ = self.gpu.device.device_wait_idle();
            self.gpu.device.destroy_pipeline(self.glass_pass.bg_pipeline, None);
            self.gpu.device.destroy_pipeline_layout(self.glass_pass.bg_pipeline_layout, None);
            self.gpu.device.destroy_descriptor_pool(self.glass_pass.descriptor_pool, None);
            self.gpu.device.destroy_descriptor_set_layout(self.glass_pass.descriptor_set_layout, None);
            self.gpu.device.destroy_sampler(self.glass_pass.transmission_sampler, None);
            self.gpu.device.destroy_image_view(self.glass_pass.transmission_image_view, None);
            self.gpu.device.destroy_image(self.glass_pass.transmission_image, None);
            self.gpu.device.free_memory(self.glass_pass._transmission_memory, None);
            self.gpu.device.destroy_image_view(self.glass_pass.depth_image_view, None);
            self.gpu.device.destroy_image(self.glass_pass.depth_image, None);
            self.gpu.device.free_memory(self.glass_pass._depth_memory, None);
            self.gpu.device.destroy_pipeline(self.glass_pass.pipeline, None);
            self.gpu.device.destroy_pipeline_layout(self.glass_pass.pipeline_layout, None);
            self.gpu.device.destroy_buffer(self.glass_pass.vertex_buffer, None);
            self.gpu.device.free_memory(self.glass_pass._vertex_memory, None);
            self.gpu.device.destroy_buffer(self.glass_pass.index_buffer, None);
            self.gpu.device.free_memory(self.glass_pass._index_memory, None);
            self.gpu.device.destroy_semaphore(self.image_available_semaphore, None);
            self.gpu.device.destroy_semaphore(self.render_finished_semaphore, None);
            self.gpu.device.destroy_fence(self.in_flight_fence, None);
            self.gpu.device.destroy_command_pool(self.command_pool, None);
        }
        log::info!("Ressources 3D et synchronisations Vulkan 1.4 libérées.");
    }
}
