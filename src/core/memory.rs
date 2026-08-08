use ash::vk;
use ash::Device;

/// Gestionnaire de la mémoire VRAM et des allocations transitoires (Pure Vulkan 1.4).
pub struct MemoryManager;

impl MemoryManager {
    /// Trouve le type de mémoire compatible sur le GPU (Host Visible vs Device Local).
    pub fn find_memory_type(
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        for i in 0..memory_props.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && (memory_props.memory_types[i as usize].property_flags & properties) == properties
            {
                return Some(i);
            }
        }
        None
    }

    /// Crée un tampon Vulkan (`vk::Buffer`) et lui alloue de la mémoire GPU.
    pub fn create_buffer(
        device: &Device,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<(vk::Buffer, vk::DeviceMemory), Box<dyn std::error::Error>> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None)? };
        let mem_reqs = unsafe { device.get_buffer_memory_requirements(buffer) };

        let memory_type = Self::find_memory_type(memory_props, mem_reqs.memory_type_bits, properties)
            .ok_or("Impossible de trouver un type de mémoire VRAM compatible.")?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(memory_type);

        let memory = unsafe { device.allocate_memory(&alloc_info, None)? };
        unsafe { device.bind_buffer_memory(buffer, memory, 0)? };

        Ok((buffer, memory))
    }

    /// Alignement bitwise de taille mémoire.
    pub fn align_to(size: u64, alignment: u64) -> u64 {
        if alignment == 0 {
            return size;
        }
        (size + alignment - 1) & !(alignment - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_alignment() {
        assert_eq!(MemoryManager::align_to(100, 256), 256);
        assert_eq!(MemoryManager::align_to(256, 256), 256);
        assert_eq!(MemoryManager::align_to(257, 256), 512);
        assert_eq!(MemoryManager::align_to(0, 256), 0);
    }
}
