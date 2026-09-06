use ash::vk;
use ash::Device;

/// Crée les pipelines de CALCUL — le seul moyen, pour un shader, d'écrire dans une mémoire.
///
/// ## ⭐ Pourquoi cette brique compte plus que sa taille ne le laisse croire
///
/// Toute la thèse du moteur repose sur un geste : *l'état vit sur la surface, et un shader le fait
/// évoluer*. Ce geste exige d'écrire dans un tampon persistant depuis un shader — ce que la chaîne
/// de rastérisation ne sait pas faire, puisqu'elle n'écrit que dans des attachements.
///
/// **Le 6 septembre 2026, une sonde l'a établi : le moteur n'avait AUCUNE passe de calcul vivante
/// et AUCUN tampon de stockage vivant.** Le mécanisme primitif dont dépendait le plan à cinq étages
/// n'existait pas, et aucun document ne le disait — ils raisonnaient tous sur le papier.
///
/// ⚠ **Ce fichier a dormi sous un préfixe `_` du 29 août au 6 septembre 2026**, et il annonçait
/// « Vulkan 1.4 » à trois endroits. **C'était faux** : un pipeline de calcul existe depuis
/// **Vulkan 1.0**, et le moteur ne demande que la 1.3. *Un commentaire qui exige une version qu'on
/// n'a pas fait renoncer un lecteur à s'en servir — la famille de défauts n° 1 du projet, dans sa
/// forme la plus discrète.*
pub struct ComputePipelineManager;

impl ComputePipelineManager {
    /// Crée un pipeline de calcul autonome.
    ///
    /// *Aucune capacité à demander au périphérique : c'est du Vulkan 1.0.*
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

        Ok(pipelines[0])
    }

    /// Calcule le nombre de groupes de threads (Workgroups) nécessaires pour couvrir une taille de problème N.
    ///
    /// *Le cas `workgroup_size == 0` est écarté avant l'appel à `div_ceil`, qui
    /// paniquerait.*
    pub fn calculate_workgroup_count(total_items: u32, workgroup_size: u32) -> u32 {
        if workgroup_size == 0 {
            return 0;
        }
        total_items.div_ceil(workgroup_size)
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
