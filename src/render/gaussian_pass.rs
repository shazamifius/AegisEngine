use crate::core::gpu_context::GpuContext;
use crate::render::render_graph::RenderPass;
use crate::scene::gaussian_splat::GaussianSplat;
use ash::vk;

/// Passe de Rendu 3D Gaussian Splatting (3DGS) & Relighting EAG-PT sous Vulkan 1.4.
///
/// ### Principe R&D :
/// Rendu des nuages de points gaussiens 3D numérisés en temps réel.
/// 1. **Tri Radix GPU par Profondeur** : Tri parallèle des gaussiennes du fond vers l'avant.
/// 2. **Découplage EAG-PT** : Extraction de l'albédo PBR pour autoriser le rééclairage dynamique par Path Tracing.
pub struct GaussianSplatPass {
    pub splat_count: u32,
    pub storage_buffer: vk::Buffer,
    pub extent: vk::Extent2D,
}

impl GaussianSplatPass {
    pub fn new(splat_count: u32, storage_buffer: vk::Buffer, extent: vk::Extent2D) -> Self {
        Self {
            splat_count,
            storage_buffer,
            extent,
        }
    }

    /// Extrait la couleur d'albédo PBR rééclairable d'une gaussienne.
    pub fn extract_relightable_albedo(&self, splat: &GaussianSplat) -> glam::Vec3 {
        splat.extract_eag_albedo()
    }
}

impl RenderPass for GaussianSplatPass {
    fn name(&self) -> &str {
        "Vulkan 1.4 3D Gaussian Splatting Compute & Raster Pass"
    }

    fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, _image_index: usize) {
        unsafe {
            // Barrière mémoire pour le Buffer de Gaussiennes (Storage Buffer)
            let barrier = vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ)
                .buffer(self.storage_buffer)
                .offset(0)
                .size(vk::WHOLE_SIZE);

            let dependency_info = vk::DependencyInfo::default()
                .buffer_memory_barriers(std::slice::from_ref(&barrier));

            context.device.cmd_pipeline_barrier2(cmd, &dependency_info);
            // Inscription du dispatch de tri Radix Compute & Rasterisation Tile-Based
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    #[test]
    fn test_gaussian_pass_albedo_extraction() {
        let pass = GaussianSplatPass::new(100_000, vk::Buffer::null(), vk::Extent2D { width: 1280, height: 720 });
        let splat = GaussianSplat::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY, 0.9, Vec3::new(0.8, 0.5, 0.2));

        let albedo = pass.extract_relightable_albedo(&splat);
        assert_eq!(albedo, Vec3::new(0.8, 0.5, 0.2));
    }
}
