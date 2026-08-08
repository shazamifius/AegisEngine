use ash::vk;
use ash::Device;

/// Usine de création de Pipelines Graphiques et Compute Native Vulkan 1.4.
pub struct PipelineFactory;

impl PipelineFactory {
    /// Compile du code shader WGSL vers du bytecode Vulkan SPIR-V à chaud en Pure Rust via Naga.
    pub fn compile_wgsl_to_spirv(
        wgsl_source: &str,
    ) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let module = naga::front::wgsl::parse_str(wgsl_source)?;
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        let module_info = validator.validate(&module)?;

        let mut options = naga::back::spv::Options::default();
        options.lang_version = (1, 5);

        let spirv_words = naga::back::spv::write_vec(&module, &module_info, &options, None)?;
        Ok(spirv_words)
    }

    /// Compile du code GLSL Native (Vertex ou Fragment) vers du bytecode Vulkan SPIR-V à chaud via Naga GLSL parser.
    pub fn compile_glsl_to_spirv(
        glsl_source: &str,
        stage: naga::ShaderStage,
    ) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let mut parser = naga::front::glsl::Frontend::default();
        let options = naga::front::glsl::Options {
            stage,
            defines: Default::default(),
        };
        let module = parser.parse(&options, glsl_source)?;

        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        let module_info = validator.validate(&module)?;

        let mut spv_options = naga::back::spv::Options::default();
        spv_options.lang_version = (1, 5);

        let spirv_words = naga::back::spv::write_vec(&module, &module_info, &spv_options, None)?;
        Ok(spirv_words)
    }

    /// Crée un Shader Module Vulkan à partir d'un bytecode SPIR-V.
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

    #[test]
    fn test_wgsl_to_spirv_compilation() {
        let wgsl = r#"
        @vertex
        fn vs_main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
            return vec4<f32>(pos, 1.0);
        }
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
        "#;

        let spirv = PipelineFactory::compile_wgsl_to_spirv(wgsl).unwrap();
        assert!(!spirv.is_empty());
    }

    #[test]
    fn test_glsl_to_spirv_compilation() {
        let glsl_vert = r#"#version 450
        layout(location = 0) in vec3 inPos;
        void main() {
            gl_Position = vec4(inPos, 1.0);
        }
        "#;

        let spirv = PipelineFactory::compile_glsl_to_spirv(glsl_vert, naga::ShaderStage::Vertex).unwrap();
        assert!(!spirv.is_empty());
    }
}
