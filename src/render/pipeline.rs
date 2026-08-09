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


