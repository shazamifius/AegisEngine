use ash::vk;
use glam::{Vec2, Vec3, Vec4};

/// Structure de Sommet PBR Standard (Standard PBR Vertex) pour Vulkan 1.4.
///
/// ### Attributs de Sommet :
/// 1. `position` ([f32; 3]) : Coordonnées 3D spatiales (x, y, z).
/// 2. `normal` ([f32; 3]) : Vecteur normal unitaire pour l'éclairage.
/// 3. `tangent` ([f32; 4]) : Vecteur tangente 4D (avec signe W de la bitangente) pour le Normal Mapping.
/// 4. `uv0` ([f32; 2]) : Coordonnées de texture principales (Albedo, Normal, Roughness/Metal).
/// 5. `uv1` ([f32; 2]) : Coordonnées de texture pour l'Atlas d'Espace-Objet (Object-Space Shading).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
}

impl Vertex {
    pub fn new(position: Vec3, normal: Vec3, tangent: Vec4, uv0: Vec2, uv1: Vec2) -> Self {
        Self {
            position: position.to_array(),
            normal: normal.to_array(),
            tangent: tangent.to_array(),
            uv0: uv0.to_array(),
            uv1: uv1.to_array(),
        }
    }

    /// Génère la description du binding de sommet pour la chaîne de rasterisation Vulkan.
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    /// Génère les descriptions d'attributs de sommets pour le Pipeline Layout Vulkan.
    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 5] {
        [
            // Location 0 : Position (Vec3 -> R32G32B32_SFLOAT)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(bytemuck::offset_of!(Self, position) as u32),
            // Location 1 : Normal (Vec3 -> R32G32B32_SFLOAT)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(bytemuck::offset_of!(Self, normal) as u32),
            // Location 2 : Tangent (Vec4 -> R32G32B32A32_SFLOAT)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(bytemuck::offset_of!(Self, tangent) as u32),
            // Location 3 : UV0 (Vec2 -> R32G32_SFLOAT)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(3)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(bytemuck::offset_of!(Self, uv0) as u32),
            // Location 4 : UV1 (Vec2 -> R32G32_SFLOAT)
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(4)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(bytemuck::offset_of!(Self, uv1) as u32),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_layout_alignment() {
        assert_eq!(std::mem::size_of::<Vertex>(), 56); // 3*4 + 3*4 + 4*4 + 2*4 + 2*4 = 56 octets
        let binding = Vertex::binding_description();
        let attrs = Vertex::attribute_descriptions();

        assert_eq!(binding.stride, 56);
        assert_eq!(attrs.len(), 5);
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[1].offset, 12);
        assert_eq!(attrs[2].offset, 24);
        assert_eq!(attrs[3].offset, 40);
        assert_eq!(attrs[4].offset, 48);
    }
}
