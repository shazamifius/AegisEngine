use ash::vk;
use ash::Device;

/// Usine de création de Pipelines Graphiques et Compute Native Vulkan 1.4.
pub struct PipelineFactory;

impl PipelineFactory {
    /// Crée un Shader Module Vulkan à partir d'un tranche d'octets SPIR-V précompilée (ex: via include_bytes!).
    pub fn create_shader_module_from_bytes(
        device: &Device,
        spirv_bytes: &[u8],
    ) -> Result<vk::ShaderModule, Box<dyn std::error::Error>> {
        let mut words = vec![0u32; spirv_bytes.len() / 4];
        unsafe {
            std::ptr::copy_nonoverlapping(
                spirv_bytes.as_ptr(),
                words.as_mut_ptr() as *mut u8,
                spirv_bytes.len(),
            );
        }
        let create_info = vk::ShaderModuleCreateInfo::default().code(&words);
        let module = unsafe { device.create_shader_module(&create_info, None)? };
        Ok(module)
    }

    /// Crée un Shader Module Vulkan à partir d'un bytecode SPIR-V (mots 32-bits).
    pub fn create_shader_module(
        device: &Device,
        spirv_words: &[u32],
    ) -> Result<vk::ShaderModule, Box<dyn std::error::Error>> {
        let create_info = vk::ShaderModuleCreateInfo::default().code(spirv_words);
        let module = unsafe { device.create_shader_module(&create_info, None)? };
        Ok(module)
    }

    /// Crée un Pipeline Layout Vulkan avec support des Push Constants.
    pub fn create_pipeline_layout(
        device: &Device,
        descriptor_set_layouts: &[vk::DescriptorSetLayout],
        push_constant_ranges: &[vk::PushConstantRange],
    ) -> Result<vk::PipelineLayout, Box<dyn std::error::Error>> {
        let create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(descriptor_set_layouts)
            .push_constant_ranges(push_constant_ranges);

        let layout = unsafe { device.create_pipeline_layout(&create_info, None)? };
        Ok(layout)
    }

    /// Crée une plage de Push Constants pour passer des données instantanées (ex: Matrices, MaterialID) au GPU.
    pub fn create_push_constant_range(
        stage_flags: vk::ShaderStageFlags,
        offset: u32,
        size: u32,
    ) -> vk::PushConstantRange {
        vk::PushConstantRange {
            stage_flags,
            offset,
            size,
        }
    }

    /// Crée un Pipeline Graphique Vulkan 1.4 complet avec support Dynamic Rendering.
    pub fn create_graphics_pipeline(
        device: &Device,
        layout: vk::PipelineLayout,
        vert_shader: vk::ShaderModule,
        frag_shader: vk::ShaderModule,
        color_format: vk::Format,
        depth_format: Option<vk::Format>,
        depth_write: bool,
        blend_enable: bool,
        use_vertex_input: bool,
        // ⚠ Doit valoir EXACTEMENT l'échantillonnage des images attachées à la passe. Un pipeline
        // qui rasterise à 4 échantillons dans une cible qui n'en a qu'un est un défaut que le
        // pilote n'est pas tenu de signaler — il peut dessiner n'importe quoi, ou rien.
        echantillons: vk::SampleCountFlags,
    ) -> Result<vk::Pipeline, Box<dyn std::error::Error>> {
        let vs_entry = std::ffi::CStr::from_bytes_with_nul(b"vs_main\0")?;
        let fs_entry = std::ffi::CStr::from_bytes_with_nul(b"fs_main\0")?;
        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_shader)
                .name(vs_entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_shader)
                .name(fs_entry),
        ];

        // Deux points de liaison : le maillage (lu par sommet) et les instances (lues par objet).
        // ⚠ Les intervertir donne une geometrie explosee sans qu'aucune erreur ne soit levee.
        let vertex_binding_descriptions = [
            vk::VertexInputBindingDescription::default()
                .binding(0)
                .stride(std::mem::size_of::<crate::geometry::vertex::Vertex>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX),
            crate::render::instances::liaison(),
        ];

        let vertex_attribute_descriptions = [
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::offset_of!(crate::geometry::vertex::Vertex, position) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::offset_of!(crate::geometry::vertex::Vertex, normal) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(std::mem::offset_of!(crate::geometry::vertex::Vertex, tangent) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(3)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(std::mem::offset_of!(crate::geometry::vertex::Vertex, uv0) as u32),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(4)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(std::mem::offset_of!(crate::geometry::vertex::Vertex, uv1) as u32),
        ];
        // Les six attributs d'instance viennent du moteur, pas d'ici : leur agencement doit
        // suivre `Instance` a l'octet pres, et une seconde description a tenir divergerait.
        let vertex_attribute_descriptions: Vec<vk::VertexInputAttributeDescription> =
            vertex_attribute_descriptions
                .into_iter()
                .chain(crate::render::instances::attributs())
                .collect();

        let vertex_input_info = if use_vertex_input {
            vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&vertex_binding_descriptions)
                .vertex_attribute_descriptions(&vertex_attribute_descriptions)
        } else {
            vk::PipelineVertexInputStateCreateInfo::default()
        };

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        // ⚠ UN FORMAT DE COULEUR `UNDEFINED` VEUT DIRE « PASSE DE PROFONDEUR PURE ».
        //
        // C'est le cas de la carte d'ombre : elle n'écrit aucune couleur. Deux conséquences, et
        // les avoir manquées a coûté une chute de 165 images par seconde à UNE — un défaut qui ne
        // produisait aucun message d'erreur, et que le chronomètre GPU ne voyait pas non plus
        // (il mesurait 2 ms pendant que l'image en prenait 1000).
        //
        //  1. Déclarer un attachement de couleur au format `UNDEFINED` est un contrat que le
        //     pilote honore comme il peut. Il ne faut en déclarer AUCUN.
        //  2. Le décalage de profondeur doit être ACTIVÉ ici et rendu dynamique plus bas, sinon
        //     `cmd_set_depth_bias` s'applique à un pipeline qui ne l'attend pas.
        let passe_de_profondeur_seule = color_format == vk::Format::UNDEFINED;

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(passe_de_profondeur_seule);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            // Le crénelage traité est celui des ARÊTES (la couverture géométrique), pas celui de
            // l'intérieur des faces : le fragment n'est calculé qu'une fois par pixel, quel que
            // soit le nombre d'échantillons. C'est ce qui rend le MSAA abordable — activer
            // `sample_shading` multiplierait le coût de l'éclairage par quatre pour un gain
            // invisible sur des aplats de couleur.
            .sample_shading_enable(false)
            .rasterization_samples(echantillons);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(depth_format.is_some())
            .depth_write_enable(depth_write)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let color_blend_attachment = if blend_enable {
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
        } else {
            vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false)
        };

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(std::slice::from_ref(&color_blend_attachment));

        let dynamic_states: &[vk::DynamicState] = if passe_de_profondeur_seule {
            &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::DEPTH_BIAS]
        } else {
            &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]
        };
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(dynamic_states);

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default();
        if !passe_de_profondeur_seule {
            rendering_create_info =
                rendering_create_info.color_attachment_formats(std::slice::from_ref(&color_format));
        }
        if let Some(df) = depth_format {
            rendering_create_info = rendering_create_info.depth_attachment_format(df);
        }

        let mut pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .push_next(&mut rendering_create_info)
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .subpass(0);

        if depth_format.is_some() {
            pipeline_info = pipeline_info.depth_stencil_state(&depth_stencil);
        }

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| e)?[0]
        };

        Ok(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_constant_range_creation() {
        let range = PipelineFactory::create_push_constant_range(
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            128,
        );

        assert_eq!(range.stage_flags, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);
        assert_eq!(range.offset, 0);
        assert_eq!(range.size, 128);
    }
}
