use crate::core::gpu_context::GpuContext;
use crate::render::render_graph::RenderPass;
use ash::vk;

/// Passe de Visibilité Stéréoscopique (Visibility Buffer / Depth Prepass) sous Vulkan 1.4.
///
/// ### Principe R&D :
/// Au lieu de calculer l'ombrage complexe (BRDF, Lumières, Ombres) directement lors du dessin de la géométrie,
/// la passe de visibilité se contente d'écrire la profondeur (Z-Buffer) et les identifiants de primitives (`PrimitiveID`, `InstanceID`)
/// dans un écran intermédiaire léger pour l'œil gauche et l'œil droit.
///
/// C'est le fondement de la géométrie virtuelle (Nanite-style) et du découplage d'ombrage (Object-Space Shading).
pub struct VisibilityPass {
    pub depth_image_view: vk::ImageView,
    pub extent: vk::Extent2D,
}

impl VisibilityPass {
    pub fn new(depth_image_view: vk::ImageView, extent: vk::Extent2D) -> Self {
        Self {
            depth_image_view,
            extent,
        }
    }
}

impl RenderPass for VisibilityPass {
    fn name(&self) -> &str {
        "Vulkan 1.4 Multiview Visibility & Depth Prepass"
    }

    fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, _image_index: usize) {
        unsafe {
            // Configuration de l'attachement de profondeur (Depth Attachment Info)
            let depth_clear_value = vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0, // Profondeur maximale initiale (Z-buffer)
                    stencil: 0,
                },
            };

            let depth_attachment_info = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(depth_clear_value);

            // Dynamic Rendering Vulkan 1.4 (sans RenderPass objet)
            let rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                })
                .layer_count(1) // Multiview ou Layer 0
                .depth_attachment(&depth_attachment_info);

            context.device.cmd_begin_rendering(cmd, &rendering_info);
            // Les commandes de dessin de meshlets (vkCmdDrawMeshTasks / vkCmdDrawIndexed) s'insèrent ici
            context.device.cmd_end_rendering(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_pass_creation() {
        let pass = VisibilityPass::new(vk::ImageView::null(), vk::Extent2D { width: 1920, height: 1080 });
        assert_eq!(pass.extent.width, 1920);
        assert_eq!(pass.extent.height, 1080);
    }
}
