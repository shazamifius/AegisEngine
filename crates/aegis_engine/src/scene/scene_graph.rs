use crate::core::math::{Mat4, Quat, Vec3};

/// Type d'élément contenu dans un nœud du Graphe de Scène.
#[derive(Debug, Clone)]
pub enum ScenePayload {
    MeshletGroup(u32),   // Groupe de Meshlets Opaques
    GlassObject(u32),     // Objet en Verre Dispersif
    GaussianSplat(u32),   // Nuage de Gaussiennes 3DGS
    Light(u32),           // Source Lumineuse
    Group,                // Nœud conteneur d'organisation
}

/// Nœud Hiérarchique du Graphe de Scène (Scene Graph Node).
///
/// Permet la transformation spatiale locale et le calcul de la Matrice de Monde globale (`WorldMatrix`)
/// par multiplication en chaîne parent-enfant : $\mathbf{M}_{\text{world}} = \mathbf{M}_{\text{parent}} \times \mathbf{M}_{\text{local}}$.
#[derive(Debug, Clone)]
pub struct SceneNode {
    pub name: String,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub payload: ScenePayload,
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    pub fn new(name: impl Into<String>, payload: ScenePayload) -> Self {
        Self {
            name: name.into(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            payload,
            children: Vec::new(),
        }
    }

    /// Calcule la Matrice de Transformation Locale du nœud.
    pub fn compute_local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Calcule la Matrice de Monde globale accumulée depuis la racine.
    pub fn compute_world_matrix(&self, parent_world_matrix: Mat4) -> Mat4 {
        parent_world_matrix * self.compute_local_matrix()
    }

    /// Ajoute un nœud enfant.
    pub fn add_child(&mut self, child: SceneNode) {
        self.children.push(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_node_hierarchy_transforms() {
        let mut root = SceneNode::new("Root", ScenePayload::Group);
        root.translation = Vec3::new(10.0, 0.0, 0.0);

        let mut child = SceneNode::new("Child", ScenePayload::MeshletGroup(1));
        child.translation = Vec3::new(5.0, 0.0, 0.0);

        let child_world = child.compute_world_matrix(root.compute_local_matrix());

        // La position globale de l'enfant doit être (15, 0, 0)
        let child_pos = child_world.transform_point3(Vec3::ZERO);
        assert_eq!(child_pos, Vec3::new(15.0, 0.0, 0.0));
    }
}
