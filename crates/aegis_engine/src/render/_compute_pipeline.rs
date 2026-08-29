use ash::vk;
use ash::Device;

/// Gestionnaire de Pipelines Compute Native Vulkan 1.4.
///
/// ### Utilité dans l'Architecture :
/// Les Compute Pipelines permettent d'exécuter des calculs massivement parallèles
/// sur GPU hors de la chaîne de rasterisation classique :
/// 1. Solveur hydrodynamique des fluides **MLS-MPM**.
/// 2. Tri et projection du **3D Gaussian Splatting (3DGS)**.
/// 3. Rasteriseur logiciel 64-bits Nanite-style sur **Visibility Buffer**.
pub struct ComputePipelineManager;

impl ComputePipelineManager {
    /// Crée un Pipeline Compute Vulkan 1.4 autonome.
    pub fn create_compute_pipeline(
        device: &Device,
        shader_module: vk::ShaderModule,
        pipeline_layout: vk::PipelineLayout,
        entry_point: &std::ffi::CStr,
    ) -> Result<vk::Pipeline, Box<dyn std::error::Error>> {
        let stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(entry_point);

        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(pipeline_layout);

        let pipelines = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
                .map_err(|(_, err)| err)?
        };

        log::info!("Pipeline Compute Vulkan 1.4 créé avec succès.");
        Ok(pipelines[0])
    }

    /// Calcule le nombre de groupes de threads (Workgroups) nécessaires pour couvrir une taille de problème N.
    ///
    /// Formula : `workgroups = (total_items + workgroup_size - 1) / workgroup_size`
    pub fn calculate_workgroup_count(total_items: u32, workgroup_size: u32) -> u32 {
        if workgroup_size == 0 {
            return 0;
        }
        (total_items + workgroup_size - 1) / workgroup_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workgroup_count_calculation() {
        // 1000 particules avec des workgroups de 64 threads -> 16 workgroups
        assert_eq!(ComputePipelineManager::calculate_workgroup_count(1000, 64), 16);
        assert_eq!(ComputePipelineManager::calculate_workgroup_count(64, 64), 1);
        assert_eq!(ComputePipelineManager::calculate_workgroup_count(65, 64), 2);
        assert_eq!(ComputePipelineManager::calculate_workgroup_count(0, 64), 0);
    }
}
