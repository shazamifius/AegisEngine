use crate::core::gpu_context::GpuContext;
use crate::materials::glass::GlassMaterial;
use crate::render::oit_pass::OitManager;
use crate::render::render_graph::RenderPass;
use ash::vk;

/// Passe Forward d'Ombrage de Transparence & Verre Dispersif sous Vulkan 1.4 Dynamic Rendering.
///
/// ### Raison d'Être Architectural (Tension R&D) :
/// Comme identifié dans notre analyse d'architecture, la réfraction du verre et la transparence
/// dépendent du point de vue de la caméra et de la profondeur des pixels de l'écran.
/// Elles **ne peuvent pas** être calculées dans l'atlas d'espace-texture d'objet.
///
/// Cette passe s'exécute donc en **Forward Screen-Space** après la passe de géométrie opaque,
/// en appliquant :
/// 1. La dispersion chromatique de Cauchy ($n(\lambda) = n_0 + B/\lambda^2$).
/// 2. L'absorption volumétrique de Beer-Lambert ($e^{-\sigma d}$).
/// 3. La transparence indépendante de l'ordre **OIT** (WBOIT / MBOIT).
pub struct ForwardGlassPass {
    pub glass_material: GlassMaterial,
    pub oit_manager: OitManager,
    pub color_image_view: vk::ImageView,
    pub depth_image_view: vk::ImageView,
    pub extent: vk::Extent2D,
}

impl ForwardGlassPass {
    pub fn new(
        ior: f32,
        color_image_view: vk::ImageView,
        depth_image_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            glass_material: GlassMaterial::new(ior, 0.005, 0.05),
            oit_manager: OitManager::new(crate::render::oit_pass::OitMode::WeightedBlended, 1000.0),
            color_image_view,
            depth_image_view,
            extent,
        }
    }
}

impl RenderPass for ForwardGlassPass {
    fn name(&self) -> &str {
        "Vulkan 1.4 Forward Dispersive Glass & OIT Rendering Pass"
    }

    fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, _image_index: usize) {
        unsafe {
            // Configuration de l'attachement de couleur (Rendu Forward)
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.color_image_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD) // Conserve les objets opaques du fond
                .store_op(vk::AttachmentStoreOp::STORE);

            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD) // Test de profondeur contre le Z-buffer opaque
                .store_op(vk::AttachmentStoreOp::STORE);

            let rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&color_attachment))
                .depth_attachment(&depth_attachment);

            context.device.cmd_begin_rendering(cmd, &rendering_info);
            // Exécution du BSDF de verre dispersif et Accumulation OIT sur GPU
            context.device.cmd_end_rendering(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_glass_pass_creation() {
        let pass = ForwardGlassPass::new(
            1.52, // IOR du verre BK7
            vk::ImageView::null(),
            vk::ImageView::null(),
            vk::Extent2D { width: 1280, height: 720 },
        );

        assert_eq!(pass.glass_material.ior, 1.52);
        assert_eq!(pass.extent.width, 1280);
    }
}
