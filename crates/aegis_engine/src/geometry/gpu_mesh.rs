//! # UN MAILLAGE SUR LA CARTE GRAPHIQUE — sommets, indices, et de quoi le dessiner
//!
//! **Remonté du jeu vers le moteur le 29 août 2026**, sur sa décision de faire d'Aegis un moteur
//! complet plutôt qu'un décor autour d'un seul jeu. Le critère est le même que pour l'interface
//! 2D : rien ici ne connaît le party platformer. Un tampon de sommets, un tampon d'indices et un
//! appel de dessin indexé servent à **n'importe quel** jeu — ils vivaient du mauvais côté.
//!
//! ⚠ La mémoire est `HOST_VISIBLE | HOST_COHERENT` : simple et suffisant pour des maillages qu'on
//! écrit une fois, mais ce n'est pas la mémoire la plus rapide de la carte. Le jour où un banc de
//! mesure le montrera, c'est ici que le transfert par tampon intermédiaire viendra.

use ash::vk;
use crate::bytes::cast_slice;
use crate::core::memory::MemoryManager;
use crate::geometry::vertex::Vertex;
use crate::GpuContext;

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
