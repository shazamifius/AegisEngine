use ash::vk;
use aegis_engine::math::{Mat4, Vec2, Vec3, Vec4};
use aegis_engine::bytes::{as_bytes, cast_slice};
use aegis_engine::GpuContext;
use aegis_engine::core::memory::MemoryManager;
use aegis_engine::geometry::primitives::Primitives;
use aegis_engine::geometry::vertex::Vertex;
use aegis_engine::render::pipeline::PipelineFactory;
use crate::party_game::PartyGame;
use crate::grid::TileType;
use crate::objects::{
    map::MapObject,
    saw_blade::SawBladeObject,
    cannon_turret::CannonTurretObject,
    spike_trap::SpikeTrapObject,
    laser_emitter::LaserEmitterObject,
    flamethrower::FlamethrowerObject,
    plants::PlantsObject,
    rock::RockObject,
    cardboard_box::CardboardBoxObject,
};

fn tile_hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(seed);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PartyPushConstants {
    pub mvp_matrix: Mat4,
    pub model_matrix: Mat4,
    pub color_tint: Vec4,
    pub params: Vec4,
}

pub struct GpuMesh {
    pub vertex_buffer: vk::Buffer,
    pub vertex_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer,
    pub index_memory: vk::DeviceMemory,
    pub index_count: u32,
}

impl GpuMesh {
    pub fn upload(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let vertex_bytes = cast_slice(vertices);
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

        let index_bytes = cast_slice(indices);
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

        Ok(Self {
            vertex_buffer,
            vertex_memory,
            index_buffer,
            index_memory,
            index_count: indices.len() as u32,
        })
    }

    pub fn draw(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
            device.cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            device.cmd_draw_indexed(cmd, self.index_count, 1, 0, 0, 0);
        }
    }
}

pub struct PartyRenderPass {
    bg_pipeline: vk::Pipeline,
    bg_pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    particle_pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,

    pub cube_mesh: GpuMesh,
    pub char_mesh: GpuMesh,

    // Modules d'Objets 3D Blender (Disponibles dans le moteur)
    pub map_obj: MapObject,
    pub plant_obj: PlantsObject,
    pub rock_obj: RockObject,
    pub saw_obj: SawBladeObject,
    pub cannon_obj: CannonTurretObject,
    pub spike_obj: SpikeTrapObject,
    pub laser_obj: LaserEmitterObject,
    pub flame_obj: FlamethrowerObject,
    pub box_obj: CardboardBoxObject,

    depth_image: vk::Image,
    depth_memory: vk::DeviceMemory,
    depth_image_view: vk::ImageView,

    pub camera_pos: Vec3,
    pub camera_target: Vec3,
    pub zoom_level: f32,
}

impl PartyRenderPass {
    pub fn new(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation du Render Pass Mode Éditeur de Map...");

        let (c_v, c_i) = Primitives::create_cube(1.0, 1.0, 1.0);
        let cube_mesh = GpuMesh::upload(gpu, memory_props, &c_v, &c_i)?;

        let (ch_v, ch_i) = Primitives::create_character_mesh();
        let char_mesh = GpuMesh::upload(gpu, memory_props, &ch_v, &ch_i)?;

        let map_obj = MapObject::new(gpu, memory_props)?;
        let plant_obj = PlantsObject::new(gpu, memory_props)?;
        let rock_obj = RockObject::new(gpu, memory_props)?;
        let saw_obj = SawBladeObject::new(gpu, memory_props)?;
        let cannon_obj = CannonTurretObject::new(gpu, memory_props)?;
        let spike_obj = SpikeTrapObject::new(gpu, memory_props)?;
        let laser_obj = LaserEmitterObject::new(gpu, memory_props)?;
        let flame_obj = FlamethrowerObject::new(gpu, memory_props)?;
        let box_obj = CardboardBoxObject::new(gpu, memory_props)?;

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
        let mem_type = MemoryManager::find_memory_type(memory_props, mem_reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL).ok_or("Mémoire non trouvée pour la profondeur")?;

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

        let bg_vert_spv = aegis_engine::shaders::BACKGROUND_VERT_SPV;
        let bg_frag_spv = aegis_engine::shaders::BACKGROUND_FRAG_SPV;
        let p_vert_spv = aegis_engine::shaders::PARTY_2D5_VERT_SPV;
        let p_frag_spv = aegis_engine::shaders::PARTY_2D5_FRAG_SPV;

        let bg_vert_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, bg_vert_spv)?;
        let bg_frag_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, bg_frag_spv)?;
        let p_vert_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, p_vert_spv)?;
        let p_frag_mod = PipelineFactory::create_shader_module_from_bytes(&gpu.device, p_frag_spv)?;

        let bg_pipeline_layout = PipelineFactory::create_pipeline_layout(&gpu.device, &[], &[])?;

        let push_constant_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<PartyPushConstants>() as u32);

        let pipeline_layout = PipelineFactory::create_pipeline_layout(&gpu.device, &[], &[push_constant_range])?;

        let bg_pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            bg_pipeline_layout,
            bg_vert_mod,
            bg_frag_mod,
            gpu.swapchain_format,
            None,
            false,
            false,
            false,
        )?;

        let pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            pipeline_layout,
            p_vert_mod,
            p_frag_mod,
            gpu.swapchain_format,
            Some(depth_format),
            true,
            false,
            true,
        )?;

        let particle_pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            pipeline_layout,
            p_vert_mod,
            p_frag_mod,
            gpu.swapchain_format,
            Some(depth_format),
            false,
            true,
            true,
        )?;

        unsafe {
            gpu.device.destroy_shader_module(bg_vert_mod, None);
            gpu.device.destroy_shader_module(bg_frag_mod, None);
            gpu.device.destroy_shader_module(p_vert_mod, None);
            gpu.device.destroy_shader_module(p_frag_mod, None);
        }

        Ok(Self {
            bg_pipeline,
            bg_pipeline_layout,
            pipeline,
            particle_pipeline,
            pipeline_layout,

            cube_mesh,
            char_mesh,

            map_obj,
            plant_obj,
            rock_obj,
            saw_obj,
            cannon_obj,
            spike_obj,
            laser_obj,
            flame_obj,
            box_obj,

            depth_image,
            depth_memory,
            depth_image_view,

            camera_pos: Vec3::new(5.0, 3.0, 16.0),
            camera_target: Vec3::new(5.0, 3.0, 0.0),
            zoom_level: 1.0,
        })
    }

    pub fn render_party_scene(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, image_index: usize, game: &PartyGame) {
        let view = context.swapchain_image_views[image_index];
        let image = context.swapchain_images[image_index];

        // Suivi de caméra fluide (Full Dézoomée centrée sur la Map en GamePhase::Drafting)
        let (target_x, target_y, target_dist) = if game.phase == crate::party_game::GamePhase::Drafting {
            (game.grid.width as f32 / 2.0, game.grid.height as f32 / 2.0, 26.0)
        } else if game.is_play_mode {
            let p_pos = game.human_player().position;
            (p_pos.x, p_pos.y + 0.85, 18.0 * self.zoom_level)
        } else {
            let (cx, cy) = game.editor.cursor;
            (cx as f32 + 0.5, cy as f32 + 0.5, 18.0 * self.zoom_level)
        };

        self.camera_target = self.camera_target.lerp(Vec3::new(target_x, target_y, 0.0), 0.18);
        self.camera_pos = self.camera_target + Vec3::new(0.0, 0.5, target_dist);

        let view_matrix = Mat4::look_at_rh(self.camera_pos, self.camera_target, Vec3::Y);
        let aspect = context.swapchain_extent.width as f32 / context.swapchain_extent.height as f32;
        let proj_matrix = Mat4::perspective_rh(38.0f32.to_radians(), aspect, 0.1, 500.0);

        let vp = proj_matrix * view_matrix;

        unsafe {
            let barrier_present = vk::ImageMemoryBarrier::default()
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

            context.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS, vk::DependencyFlags::empty(), &[], &[], &[barrier_present, barrier_depth]);

            let color_attach = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.92, 0.95, 0.98, 1.0] } });

            let depth_attach = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } });

            let rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&color_attach))
                .depth_attachment(&depth_attach);

            context.device.cmd_begin_rendering(cmd, &rendering_info);

            // 1. Fond Studio
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.bg_pipeline);
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: context.swapchain_extent.width as f32, height: context.swapchain_extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
            let scissor = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: context.swapchain_extent };
            context.device.cmd_set_viewport(cmd, 0, &[viewport]);
            context.device.cmd_set_scissor(cmd, 0, &[scissor]);
            context.device.cmd_draw(cmd, 3, 1, 0, 0);

            // 2. Pipeline Opaque : Rendu des Blocs de la Grille Posés par le Joueur
            context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            for y in 0..game.grid.height {
                for x in 0..game.grid.width {
                    let xi = x as i32;
                    let yi = y as i32;
                    let tile = game.grid.get_tile(xi, yi);
                    if tile != TileType::Air {
                        let model = Mat4::from_translation(Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0))
                            * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0));

                        let col = match tile {
                            TileType::GrassBlock => Vec4::new(0.32, 0.82, 0.36, 1.0), // Herbe Vert Vif Pur
                            TileType::SolidBlock => Vec4::new(0.48, 0.32, 0.20, 1.0), // Terre Marron
                            TileType::MetalBlock => Vec4::new(0.60, 0.63, 0.68, 1.0), // Pierre Grise
                            _ => tile.color(),
                        };

                        let push = PartyPushConstants {
                            mvp_matrix: vp * model,
                            model_matrix: model,
                            color_tint: col,
                            params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                        };

                        context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
                        self.cube_mesh.draw(&context.device, cmd);

                        // 1. Décoration Procédurale de la Roche (Pierre) : Petits cubes gris plus sombres encastrés
                        if tile == TileType::MetalBlock {
                            for k in 0..4 {
                                let h = tile_hash(xi, yi, k);
                                let off_x = (h % 65) as f32 / 100.0 - 0.32;
                                let off_y = ((h / 65) % 65) as f32 / 100.0 - 0.32;
                                let sz = 0.11 + (h % 10) as f32 * 0.012;

                                let sub_m = Mat4::from_translation(Vec3::new(x as f32 + 0.5 + off_x, y as f32 + 0.5 + off_y, 0.05))
                                    * Mat4::from_scale(Vec3::new(sz, sz, 0.92));
                                let sub_push = PartyPushConstants {
                                    mvp_matrix: vp * sub_m,
                                    model_matrix: sub_m,
                                    color_tint: Vec4::new(0.40, 0.43, 0.48, 1.0), // Gris Roche Plus Sombre
                                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                };
                                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&sub_push));
                                self.cube_mesh.draw(&context.device, cmd);
                            }
                        }

                        // 2. Décoration Procédurale de la Terre (Dirt) : Très léger et très petit (1 à 2 micro-tavelures subtiles)
                        if tile == TileType::SolidBlock {
                            for k in 0..2 {
                                let h = tile_hash(xi, yi, k + 10);
                                let off_x = (h % 60) as f32 / 100.0 - 0.30;
                                let off_y = ((h / 60) % 60) as f32 / 100.0 - 0.30;
                                let sz = 0.05 + (h % 5) as f32 * 0.008; // Très petit !

                                let sub_m = Mat4::from_translation(Vec3::new(x as f32 + 0.5 + off_x, y as f32 + 0.5 + off_y, 0.05))
                                    * Mat4::from_scale(Vec3::new(sz, sz, 0.92));
                                let sub_push = PartyPushConstants {
                                    mvp_matrix: vp * sub_m,
                                    model_matrix: sub_m,
                                    color_tint: Vec4::new(0.42, 0.27, 0.16, 1.0), // Très légèrement plus sombre, ultra-subtil
                                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                };
                                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&sub_push));
                                self.cube_mesh.draw(&context.device, cmd);
                            }
                        }

                        // 3. Décoration Procédurale de l'Herbe (Grass) : Brins d'Herbe 3D Voxels au sommet
                        if tile == TileType::GrassBlock {
                            // Brins d'Herbe Voxels Procéduraux si le bloc au-dessus est vide (Air)
                            if game.grid.get_tile(xi, yi + 1) == TileType::Air {
                                for k in 0..5 {
                                    let h = tile_hash(xi, yi, k + 50);
                                    let blade_x = x as f32 + 0.12 + (h % 76) as f32 / 100.0;
                                    let blade_h = 0.14 + (h % 14) as f32 * 0.012;
                                    let blade_w = 0.07 + (h % 5) as f32 * 0.01;

                                    let green_color = match k % 3 {
                                        0 => Vec4::new(0.32, 0.90, 0.35, 1.0), // Vert Vif
                                        1 => Vec4::new(0.20, 0.78, 0.26, 1.0), // Émeraude
                                        _ => Vec4::new(0.14, 0.65, 0.20, 1.0), // Forêt
                                    };

                                    let blade_m = Mat4::from_translation(Vec3::new(blade_x, y as f32 + 1.0 + blade_h * 0.45, 0.02))
                                        * Mat4::from_rotation_z(((h % 20) as f32 - 10.0) * 0.02)
                                        * Mat4::from_scale(Vec3::new(blade_w, blade_h, 0.18));
                                    let blade_push = PartyPushConstants {
                                        mvp_matrix: vp * blade_m,
                                        model_matrix: blade_m,
                                        color_tint: green_color,
                                        params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                    };
                                    context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&blade_push));
                                    self.cube_mesh.draw(&context.device, cmd);
                                }
                            }
                        }
                    }
                }
            }
            // Rendu de l'Immense Bande Noire du Vide (Void Kill Line - 5 blocs en dessous du bloc le plus bas)
            let void_y = game.grid.get_void_kill_y();
            let void_model = Mat4::from_translation(Vec3::new(game.grid.width as f32 / 2.0, void_y + 0.5, -0.1))
                * Mat4::from_scale(Vec3::new(500.0, 1.0, 2.0)); // Bande noire infinie sur les côtés X, 1 bloc de hauteur Y

            let push_void = PartyPushConstants {
                mvp_matrix: vp * void_model,
                model_matrix: void_model,
                color_tint: Vec4::new(0.02, 0.02, 0.05, 1.0), // Noir Abyssal Profond
                params: Vec4::new(0.8, 0.0, 0.0, 0.0),
            };

            context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_void));
            self.cube_mesh.draw(&context.device, cmd);

            // Rendu du Carton Mystère 3D Animé au Centre de l'Écran (boxfermer.glb -> box.glb avec Secousse 0.8s)
            if game.phase == crate::party_game::GamePhase::Drafting {
                let box_pos = Vec3::new(game.grid.width as f32 / 2.0, game.grid.height as f32 / 2.0, 0.0);
                self.box_obj.update(0.016);
                if self.box_obj.is_opened && !self.box_obj.burst_triggered {
                    self.box_obj.burst_triggered = true;
                    if let Some(player_mut) = game.players.get(0) {
                        let mut p_mgr = player_mut.player.particles.clone();
                        p_mgr.spawn_box_open_burst(box_pos);
                    }
                }
                self.box_obj.draw(&context.device, cmd, self.pipeline_layout, vp, box_pos);

                // Objets 3D Disponibles DANS le Carton Ouvert (Surface grillmax)
                if self.box_obj.is_opened {
                    let items = &game.mystery_box.available_items;
                    let total = items.len();
                    let t = game.round_timer;

                    for (i, item) in items.iter().enumerate() {
                        let spacing = 0.45;
                        let x_offset = box_pos.x + (i as f32 - (total as f32 - 1.0) / 2.0) * spacing;
                        let item_pos = Vec3::new(x_offset, box_pos.y + 0.35 + (t * 3.0 + i as f32).sin() * 0.05, 0.1);

                        let is_selected = game.mystery_box.selected_index == Some(i);

                        let item_p2 = Vec2::new(item_pos.x, item_pos.y);

                        // Rendu des Objets 3D par Type
                        match item {
                            crate::mystery_box::ItemType::SawBlade => self.saw_obj.draw(&context.device, cmd, self.pipeline_layout, vp, item_p2, t * 8.0),
                            crate::mystery_box::ItemType::CannonTurret => self.cannon_obj.draw(&context.device, cmd, self.pipeline_layout, vp, item_p2, crate::traps::Direction::Right),
                            crate::mystery_box::ItemType::SpikeTrap => self.spike_obj.draw(&context.device, cmd, self.pipeline_layout, vp, item_p2),
                            crate::mystery_box::ItemType::LaserEmitter => self.laser_obj.draw(&context.device, cmd, self.pipeline_layout, &self.cube_mesh, vp, item_p2, crate::traps::Direction::Up, true),
                            crate::mystery_box::ItemType::Flamethrower => self.flame_obj.draw(&context.device, cmd, self.pipeline_layout, &self.cube_mesh, vp, item_p2, crate::traps::Direction::Right, true),
                            _ => {
                                let item_m = Mat4::from_translation(item_pos) * Mat4::from_scale(Vec3::splat(0.40));
                                let push_item = PartyPushConstants {
                                    mvp_matrix: vp * item_m,
                                    model_matrix: item_m,
                                    color_tint: Vec4::new(0.35, 0.75, 0.95, 1.0),
                                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                                };
                                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_item));
                                self.cube_mesh.draw(&context.device, cmd);
                            }
                        }

                        // Anneau d'Or de Sélection pour l'Objet Choisi !
                        if is_selected {
                            let ring_m = Mat4::from_translation(item_pos + Vec3::new(0.0, 0.38, 0.05))
                                * Mat4::from_rotation_z(t * 5.0)
                                * Mat4::from_scale(Vec3::new(0.12, 0.12, 0.12));
                            let push_ring = PartyPushConstants {
                                mvp_matrix: vp * ring_m,
                                model_matrix: ring_m,
                                color_tint: Vec4::new(0.98, 0.85, 0.15, 1.0), // Or Émissif
                                params: Vec4::new(0.0, 8.0, 0.0, 0.0),
                            };
                            context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_ring));
                            self.cube_mesh.draw(&context.device, cmd);
                        }
                    }
                }
            }

            // Rendu du Personnage Joueur (Héros Aventurier 3D - Activé & Spawné uniquement en GamePhase::Running)
            if game.phase == crate::party_game::GamePhase::Running {
                for session in &game.players {
                let player = &session.player;
                let p_pos = player.position;
                let facing_sign = if player.facing_right { 1.0 } else { -1.0 };
                let t = game.round_timer;

                // 1. Rendu du Ragdoll de Mort 3D si le joueur est mort
                if player.state == crate::player::PlayerState::Dead && player.ragdoll.active {
                    for limb in &player.ragdoll.limbs {
                        let m = Mat4::from_translation(limb.pos)
                            * Mat4::from_rotation_z(limb.rotation.z)
                            * Mat4::from_rotation_y(limb.rotation.y)
                            * Mat4::from_rotation_x(limb.rotation.x)
                            * Mat4::from_scale(limb.scale);

                        let push = PartyPushConstants {
                            mvp_matrix: vp * m,
                            model_matrix: m,
                            color_tint: limb.color,
                            params: Vec4::new(0.4, 0.0, 0.0, 0.0),
                        };

                        context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push));
                        self.cube_mesh.draw(&context.device, cmd);
                    }
                    continue;
                }

                // 2. Calculs de Squash & Stretch et Impact d'Atterrissage Procédural
                let dt: f32 = 0.016; // Delta time pour interpolation exponentielle ultra-douce
                let is_wall_sliding = matches!(player.state, crate::player::PlayerState::WallSliding { .. });
                let is_running = player.state == crate::player::PlayerState::OnGround && player.velocity.x.abs() > 0.3;
                let is_in_air = player.state == crate::player::PlayerState::InAir;
                let vy = player.velocity.y;
                let is_rising = is_in_air && vy > 0.1;
                let is_falling = is_in_air && vy <= 0.1;

                // Absorption d'impact dynamique à l'atterrissage (Squash Recoil proportionnel à la hauteur de chute)
                let landing_squash = if player.landing_timer > 0.0 && player.landing_duration > 0.0 {
                    let progress = (1.0 - player.landing_timer / player.landing_duration).clamp(0.0, 1.0);
                    (progress * std::f32::consts::PI).sin() * (1.0 - progress * 0.5) * player.landing_intensity
                } else {
                    0.0
                };

                let (target_scale_x, target_scale_y) = if is_wall_sliding {
                    (0.85, 1.15) // Posture tendue d'escalade
                } else if is_rising {
                    (0.82, 1.28) // Étirement athlétique à l'impulsion du saut
                } else if is_falling {
                    let fall_factor = (-vy / 22.0).clamp(0.0, 0.22);
                    (1.0 + fall_factor, 1.0 - fall_factor) // Compression progressive en chute
                } else if landing_squash > 0.005 {
                    // Grande déformation dynamique proportionnelle à la hauteur de chute (jusqu'à 50% de compression !)
                    (1.0 + landing_squash * 0.85, (1.0 - landing_squash * 0.65).max(0.48))
                } else if is_running {
                    (1.0 + 0.04 * (t * 16.0).sin(), 1.0 - 0.04 * (t * 16.0).sin())
                } else {
                    (1.0, 1.0 + 0.02 * (t * 3.5).sin()) // Respiration Choupi
                };

                let body_base_matrix = Mat4::from_translation(Vec3::new(p_pos.x, p_pos.y, 0.2))
                    * Mat4::from_rotation_z(player.tilt_angle)
                    * Mat4::from_scale(Vec3::new(target_scale_x * facing_sign, target_scale_y, 1.0));

                // Effet d'Étincelles de Frottement sur le Mur
                if let crate::player::PlayerState::WallSliding { left_wall } = player.state {
                    let wall_dir_world = if left_wall { -1.0 } else { 1.0 };
                    for i in 0..3 {
                        let spark_y = p_pos.y + 0.3 + (t * 14.0 + i as f32 * 2.1).sin() * 0.5;
                        let spark_x = p_pos.x + wall_dir_world * 0.42;
                        let spark_m = Mat4::from_translation(Vec3::new(spark_x, spark_y, 0.25))
                            * Mat4::from_scale(Vec3::new(0.08, 0.08, 0.08));
                        let push_spark = PartyPushConstants {
                            mvp_matrix: vp * spark_m,
                            model_matrix: spark_m,
                            color_tint: Vec4::new(0.98, 0.75, 0.20, 1.0), // Étincelles Or
                            params: Vec4::new(0.0, 6.0, 0.0, 0.0), // Lueur intense
                        };
                        context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_spark));
                        self.cube_mesh.draw(&context.device, cmd);
                    }
                }

                // 3. Pose d'Articulation Lissée (Consommation de l'animation organique de Player)
                let leg_angle_front = player.anim_leg_front;
                let leg_angle_back = player.anim_leg_back;
                let arm_angle_front = player.anim_arm_front;
                let arm_angle_back = player.anim_arm_back;

                // Jambe Avant (Z = +0.12)
                let left_leg_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.35, 0.12))
                    * Mat4::from_rotation_z(leg_angle_front)
                    * Mat4::from_translation(Vec3::new(0.0, -0.175, 0.0))
                    * Mat4::from_scale(Vec3::new(0.22, 0.35, 0.20));
                let push_leg = PartyPushConstants {
                    mvp_matrix: vp * left_leg_m,
                    model_matrix: left_leg_m,
                    color_tint: Vec4::new(0.12, 0.15, 0.28, 1.0), // Pantalon Indigo
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_leg));
                self.cube_mesh.draw(&context.device, cmd);

                // Jambe Arrière (Z = -0.12)
                let right_leg_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.35, -0.12))
                    * Mat4::from_rotation_z(leg_angle_back)
                    * Mat4::from_translation(Vec3::new(0.0, -0.175, 0.0))
                    * Mat4::from_scale(Vec3::new(0.22, 0.35, 0.20));
                let push_leg_r = PartyPushConstants {
                    mvp_matrix: vp * right_leg_m,
                    model_matrix: right_leg_m,
                    color_tint: Vec4::new(0.10, 0.12, 0.22, 1.0), // Jambe arrière plus sombre
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_leg_r));
                self.cube_mesh.draw(&context.device, cmd);

                // 4. Torso / Veste de Héros (Centré à y=0.65)
                let torso_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.65, 0.0))
                    * Mat4::from_scale(Vec3::new(0.48, 0.65, 0.40));
                let push_torso = PartyPushConstants {
                    mvp_matrix: vp * torso_m,
                    model_matrix: torso_m,
                    color_tint: Vec4::new(0.20, 0.65, 0.95, 1.0), // Veste Cyan
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_torso));
                self.cube_mesh.draw(&context.device, cmd);

                // 5. Animation Dynamique des Bras
                // Bras Avant (Devant la veste Z = +0.23)
                let arm_front_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.88, 0.23))
                    * Mat4::from_rotation_z(arm_angle_front)
                    * Mat4::from_translation(Vec3::new(0.0, -0.18, 0.0))
                    * Mat4::from_scale(Vec3::new(0.20, 0.38, 0.18));
                let push_arm_f = PartyPushConstants {
                    mvp_matrix: vp * arm_front_m,
                    model_matrix: arm_front_m,
                    color_tint: Vec4::new(0.15, 0.18, 0.25, 1.0),
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_arm_f));
                self.cube_mesh.draw(&context.device, cmd);

                // Bras Arrière (Derrière la veste Z = -0.23)
                let arm_back_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, 0.88, -0.23))
                    * Mat4::from_rotation_z(arm_angle_back)
                    * Mat4::from_translation(Vec3::new(0.0, -0.18, 0.0))
                    * Mat4::from_scale(Vec3::new(0.20, 0.38, 0.18));
                let push_arm_b = PartyPushConstants {
                    mvp_matrix: vp * arm_back_m,
                    model_matrix: arm_back_m,
                    color_tint: Vec4::new(0.12, 0.14, 0.20, 1.0),
                    params: Vec4::new(0.3, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_arm_b));
                self.cube_mesh.draw(&context.device, cmd);

                // 6. Tête du Héros Choupi & Animation "BONK !" au Choc de Plafond
                let is_ceiling_bumping = player.ceiling_bump_timer > 0.0;
                let bump_squash = if is_ceiling_bumping {
                    let progress = (1.0 - player.ceiling_bump_timer / 0.28).clamp(0.0, 1.0);
                    (progress * std::f32::consts::PI).sin() * player.ceiling_bump_intensity
                } else {
                    0.0
                };

                let head_choupi_tilt = (t * 3.2).sin() * 0.03 - player.velocity.x * 0.02 - bump_squash * 0.40;
                let head_y = 1.22 - landing_squash * 0.12 - bump_squash * 0.18;
                let head_scale_y = 0.42 * (1.0 - bump_squash * 0.45);

                let head_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.0, head_y, 0.0))
                    * Mat4::from_rotation_z(head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.46 * (1.0 + bump_squash * 0.3), head_scale_y, 0.42));
                let push_head = PartyPushConstants {
                    mvp_matrix: vp * head_m,
                    model_matrix: head_m,
                    color_tint: Vec4::new(0.96, 0.96, 0.98, 1.0), // Tête Blanche Pur
                    params: Vec4::new(0.1, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_head));
                self.cube_mesh.draw(&context.device, cmd);

                // 7. Yeux / Visière Luminescente Robot Choupi (Pulsation d'Énergie Respiratoire)
                let visor_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(0.20, head_y + 0.01, 0.0))
                    * Mat4::from_rotation_z(head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.16, 0.10 * (1.0 - bump_squash * 0.3), 0.32));
                let visor_glow = 3.5 + 1.2 * (t * 4.0).sin();
                let push_visor = PartyPushConstants {
                    mvp_matrix: vp * visor_m,
                    model_matrix: visor_m,
                    color_tint: Vec4::new(0.98, 0.82, 0.10, 1.0), // Visière Or
                    params: Vec4::new(0.0, visor_glow, 0.0, 0.0), // Lueur Émissive Respiratoire
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_visor));
                self.cube_mesh.draw(&context.device, cmd);

                // 8. Bonnet Rouge Ajusté (Posé sur la tête à y=1.45, aplati au choc du plafond)
                let trail_angle = if is_running {
                    -(player.velocity.x * 0.04)
                } else {
                    0.0
                };
                let cap_scale_y = 0.12 * (1.0 - bump_squash * 0.50);
                let cap_m = body_base_matrix
                    * Mat4::from_translation(Vec3::new(-0.02, head_y + 0.23, 0.0))
                    * Mat4::from_rotation_z(trail_angle + head_choupi_tilt)
                    * Mat4::from_scale(Vec3::new(0.48 * (1.0 + bump_squash * 0.35), cap_scale_y, 0.44));
                let push_cap = PartyPushConstants {
                    mvp_matrix: vp * cap_m,
                    model_matrix: cap_m,
                    color_tint: Vec4::new(0.92, 0.20, 0.25, 1.0), // Bonnet Rouge Vif
                    params: Vec4::new(0.2, 0.0, 0.0, 0.0),
                };
                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_cap));
                self.cube_mesh.draw(&context.device, cmd);

                // 9. Étoiles de Choc Pop-up "BONK !" au Plafond
                if is_ceiling_bumping {
                    for i in 0..4 {
                        let star_angle = i as f32 * std::f32::consts::PI / 2.0 + t * 14.0;
                        let star_x = p_pos.x + star_angle.cos() * (0.35 + bump_squash * 0.25);
                        let star_y = p_pos.y + 1.58 + star_angle.sin() * 0.14;
                        let star_m = Mat4::from_translation(Vec3::new(star_x, star_y, 0.3))
                            * Mat4::from_rotation_z(t * 22.0 + i as f32)
                            * Mat4::from_scale(Vec3::new(0.09, 0.09, 0.09));
                        let push_star = PartyPushConstants {
                            mvp_matrix: vp * star_m,
                            model_matrix: star_m,
                            color_tint: Vec4::new(0.98, 0.90, 0.15, 1.0), // Étoile "BONK !" Or Brillant
                            params: Vec4::new(0.0, 8.0, 0.0, 0.0),
                        };
                        context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_star));
                        self.cube_mesh.draw(&context.device, cmd);
                    }
                }

                // 10. Rendu du Système de Particules Procédurales (Course, Dérapage, Glissade sur Tous Blocs, Impact)
                for particle in &player.particles.particles {
                    let p_m = Mat4::from_translation(particle.pos)
                        * Mat4::from_scale(particle.size);
                    let push_p = PartyPushConstants {
                        mvp_matrix: vp * p_m,
                        model_matrix: p_m,
                        color_tint: particle.color,
                        params: Vec4::new(0.0, particle.emissive, 0.0, 0.0),
                    };
                    context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_p));
                    self.cube_mesh.draw(&context.device, cmd);
                }
            }
        }

        // 3. Pipeline Particules / Transparence : Bloc Preview Wireframe en Mode Éditeur uniquement
            if !game.is_play_mode {
                context.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.particle_pipeline);

                let (cx, cy) = game.editor.cursor;
                let preview_col = game.editor.selected_block.color();
                let preview_model = Mat4::from_translation(Vec3::new(cx as f32 + 0.5, cy as f32 + 0.5, 0.3))
                    * Mat4::from_scale(Vec3::new(1.08, 1.08, 1.08));

                let push_preview = PartyPushConstants {
                    mvp_matrix: vp * preview_model,
                    model_matrix: preview_model,
                    color_tint: preview_col,
                    params: Vec4::new(0.0, 5.0, 0.0, 0.0), // Lueur Émissive Wireframe Preview
                };

                context.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, as_bytes(&push_preview));
                self.cube_mesh.draw(&context.device, cmd);
            }

            context.device.cmd_end_rendering(cmd);

            // Transition Swapchain -> PRESENT_SRC
            let barrier_present_back = vk::ImageMemoryBarrier::default()
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

            context.device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, vk::PipelineStageFlags::BOTTOM_OF_PIPE, vk::DependencyFlags::empty(), &[], &[], &[barrier_present_back]);
        }
    }

    pub fn recreate_framebuffer_resources(
        &mut self,
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) {
        unsafe {
            let _ = gpu.device.device_wait_idle();
            gpu.device.destroy_image_view(self.depth_image_view, None);
            gpu.device.destroy_image(self.depth_image, None);
            gpu.device.free_memory(self.depth_memory, None);

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

            let depth_image = gpu.device.create_image(&image_info, None).unwrap();
            let mem_reqs = gpu.device.get_image_memory_requirements(depth_image);
            let mem_type = MemoryManager::find_memory_type(memory_props, mem_reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL).unwrap();

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type);

            let depth_memory = gpu.device.allocate_memory(&alloc_info, None).unwrap();
            gpu.device.bind_image_memory(depth_image, depth_memory, 0).unwrap();

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

            let depth_image_view = gpu.device.create_image_view(&view_info, None).unwrap();

            self.depth_image = depth_image;
            self.depth_memory = depth_memory;
            self.depth_image_view = depth_image_view;
        }
    }
}
