use glam::Vec3;

/// Structure de Réservoir stochastique GRIS / ReSTIR PT.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reservoir {
    pub candidate_id: u32,  // Chemin lumineux candidat Y
    pub weight_sum: f32,    // Poids cumulé w_i
    pub sample_count: u32,  // Nombre de candidats vus M
    pub weight_ris: f32,    // Poids d'échantillonnage RIS W_Y
}

impl Reservoir {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tente d'insérer un nouveau candidat d'échantillonnage stochastique.
    pub fn update(&mut self, candidate_id: u32, weight: f32, random_val: f32) -> bool {
        self.weight_sum += weight;
        self.sample_count += 1;

        if random_val * self.weight_sum < weight {
            self.candidate_id = candidate_id;
            true
        } else {
            false
        }
    }

    /// Calcule le facteur d'ajustement Jacobien J_A->B pour le Shift Mapping.
    pub fn compute_jacobian(
        pos_a: Vec3,
        pos_b: Vec3,
        target_pos: Vec3,
        normal_a: Vec3,
        normal_b: Vec3,
    ) -> f32 {
        let dir_a = (target_pos - pos_a).normalize();
        let dir_b = (target_pos - pos_b).normalize();

        let dist_a_sq = pos_a.distance_squared(target_pos).max(1e-4);
        let dist_b_sq = pos_b.distance_squared(target_pos).max(1e-4);

        let cos_a = normal_a.dot(dir_a).max(1e-3);
        let cos_b = normal_b.dot(dir_b).max(1e-3);

        (cos_b / cos_a) * (dist_a_sq / dist_b_sq)
    }

    /// Teste si le Shift Mapping doit être rejeté (Footprint Reconnection pour casser les Boiling Artifacts).
    pub fn test_footprint_reconnection(jacobian: f32) -> bool {
        jacobian >= 0.1 && jacobian <= 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reservoir_update_stochastic_selection() {
        let mut res = Reservoir::new();
        assert_eq!(res.sample_count, 0);

        // Premier candidat (weight = 10.0, random = 0.5) -> Sélectionné
        let sel1 = res.update(101, 10.0, 0.5);
        assert!(sel1);
        assert_eq!(res.candidate_id, 101);

        // Second candidat (weight = 1.0, random = 0.99) -> Rejeté
        let sel2 = res.update(102, 1.0, 0.99);
        assert!(!sel2);
        assert_eq!(res.candidate_id, 101);
        assert_eq!(res.sample_count, 2);
    }

    #[test]
    fn test_jacobian_shift_mapping_calculation() {
        let pos_a = Vec3::new(0.0, 0.0, 0.0);
        let pos_b = Vec3::new(0.1, 0.0, 0.0);
        let target = Vec3::new(0.0, 5.0, 0.0);
        let normal_a = Vec3::Y;
        let normal_b = Vec3::Y;

        let j = Reservoir::compute_jacobian(pos_a, pos_b, target, normal_a, normal_b);
        assert!((j - 1.0).abs() < 0.1); // Décalage minime -> Jacobien proche de 1
    }

    #[test]
    fn test_footprint_reconnection_rejection() {
        assert!(Reservoir::test_footprint_reconnection(1.5));
        assert!(Reservoir::test_footprint_reconnection(0.5));

        // Jacobien instable -> Rejeté pour éviter les taches d'ébullition (Boiling Artifacts)
        assert!(!Reservoir::test_footprint_reconnection(15.0));
        assert!(!Reservoir::test_footprint_reconnection(0.05));
    }
}
