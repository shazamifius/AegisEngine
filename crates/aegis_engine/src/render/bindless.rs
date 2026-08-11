use ash::vk;
use ash::Device;

/// Gestionnaire de la mémoire Bindless Vulkan 1.4 (500 000+ Textures et Buffers sans rebinding).
pub struct BindlessManager {
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_set: vk::DescriptorSet,
    pub max_textures: u32,
    pub max_buffers: u32,
}

impl BindlessManager {
    pub const MAX_TEXTURE_SLOTS: u32 = 500_000;
    pub const MAX_BUFFER_SLOTS: u32 = 100_000;

    /// Initialise le pool et le layout Bindless avec les drapeaux UPDATE_AFTER_BIND et PARTIALLY_BOUND.
    pub fn new(device: &Device) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Initialisation du système Bindless Vulkan 1.4 (500K Textures / 100K Buffers)...");

        // 1. Définition des tailles de Pool
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: Self::MAX_TEXTURE_SLOTS,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: Self::MAX_BUFFER_SLOTS,
            },
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .max_sets(1)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None)? };

        // 2. Définition des Bindings de Layout
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(Self::MAX_TEXTURE_SLOTS)
                .stage_flags(vk::ShaderStageFlags::ALL),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(Self::MAX_BUFFER_SLOTS)
                .stage_flags(vk::ShaderStageFlags::ALL),
        ];

        let binding_flags = [
            vk::DescriptorBindingFlags::PARTIALLY_BOUND | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::PARTIALLY_BOUND | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
        ];

        let mut flags_info = vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
            .binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .bindings(&bindings)
            .push_next(&mut flags_info);

        let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None)? };

        // 3. Allocation du Descriptor Set unique
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let descriptor_sets = unsafe { device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        log::info!("Système Bindless Vulkan 1.4 alloué avec succès.");

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            max_textures: Self::MAX_TEXTURE_SLOTS,
            max_buffers: Self::MAX_BUFFER_SLOTS,
        })
    }

    /// Nettoie les ressources Bindless.
    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
        log::info!("Ressources Bindless Vulkan 1.4 nettoyées.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bindless_capacity_constants() {
        assert_eq!(BindlessManager::MAX_TEXTURE_SLOTS, 500_000);
        assert_eq!(BindlessManager::MAX_BUFFER_SLOTS, 100_000);
    }
}
