use crate::core::gpu_context::GpuContext;
use ash::vk;

/// Trait représentant une passe de rendu individuelle au sein du Render Graph Pure Vulkan 1.4.
pub trait RenderPass {
    fn name(&self) -> &str;
    fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, image_index: usize);
}

/// Système de Render Graph (FrameGraph) gérant l'enchaînement et la synchronisation des passes Vulkan.
pub struct RenderGraph {
    passes: Vec<Box<dyn RenderPass>>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Enregistre une nouvelle passe dans le graphe d'exécution.
    pub fn add_pass(&mut self, pass: Box<dyn RenderPass>) {
        log::debug!("Render Graph : Passe Vulkan ajoutée -> {}", pass.name());
        self.passes.push(pass);
    }

    /// Nombre de passes enregistrées dans le graphe.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Exécute l'intégralité des passes enregistrées en encodant les commandes Vulkan 1.4.
    pub fn execute(&mut self, context: &GpuContext, cmd: vk::CommandBuffer, image_index: usize) {
        for pass in self.passes.iter_mut() {
            pass.execute(context, cmd, image_index);
        }
    }

    /// Réinitialise le graphe pour la trame suivante.
    pub fn clear(&mut self) {
        self.passes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPass {
        name: String,
    }

    impl RenderPass for MockPass {
        fn name(&self) -> &str {
            &self.name
        }

        fn execute(&mut self, _context: &GpuContext, _cmd: vk::CommandBuffer, _image_index: usize) {}
    }

    #[test]
    fn test_render_graph_pass_management() {
        let mut graph = RenderGraph::new();
        assert_eq!(graph.pass_count(), 0);

        graph.add_pass(Box::new(MockPass {
            name: "TestPass1".to_string(),
        }));
        graph.add_pass(Box::new(MockPass {
            name: "TestPass2".to_string(),
        }));

        assert_eq!(graph.pass_count(), 2);
        graph.clear();
        assert_eq!(graph.pass_count(), 0);
    }
}
