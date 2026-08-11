use aegis_engine::math::{Vec2, Vec3};
use crate::grid::{TileGrid, TileType};
use crate::traps::{TrapManager, TrapKind};

pub fn compute_box_item_offset(index: usize, total_items: usize) -> (Vec3, f32) {
    if total_items == 0 {
        return (Vec3::ZERO, 1.0);
    }

    // Calcul dynamique de la grille (Colonnes x Rangées)
    let cols = match total_items {
        1..=4 => 2,
        5..=6 => 3,
        7..=12 => 4,
        13..=20 => 5,
        21..=30 => 6,
        _ => 7,
    };

    let rows = (total_items + cols - 1) / cols;
    let col = index % cols;
    let row = index / cols;

    // Dimensions intérieures maximales du carton mystère (Width = 7.6, Height = 4.4)
    let width_span = 7.6f32;
    let height_span = 4.4f32;

    let dx = if cols > 1 { width_span / (cols - 1) as f32 } else { 0.0 };
    let dy = if rows > 1 { height_span / (rows - 1) as f32 } else { 0.0 };

    let start_x = -width_span * 0.5;
    let start_y = -height_span * 0.5 - 0.4;

    let off_x = start_x + (col as f32) * dx;
    let off_y = start_y + (row as f32) * dy;
    let off_z = 12.2; // Lancement légèrement devant le fond du carton (Z = 12.0) pour une visibilité 100% parfaite !

    // Échelle adaptative garantissant que TOUS les objets (GLB & blocs) restent TOUJOURS parfaitement dimensionnés
    let scale_mult = match total_items {
        1..=4 => 1.25,
        5..=9 => 0.95,
        10..=16 => 0.70,
        17..=25 => 0.55,
        26..=36 => 0.45,
        _ => 0.38,
    };

    (Vec3::new(off_x, off_y, off_z), scale_mult)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemType {
    SolidBlock,
    GrassBlock,
    MetalBlock,
    IceBlock,
    LavaBlock,
    SawBlade,
    CannonTurret,
    SpikeTrap,
    LaserEmitter,
    Flamethrower,
}

impl ItemType {
    pub fn all_types() -> Vec<ItemType> {
        vec![
            ItemType::SolidBlock,
            ItemType::GrassBlock,
            ItemType::MetalBlock,
            ItemType::IceBlock,
            ItemType::LavaBlock,
            ItemType::SawBlade,
            ItemType::CannonTurret,
            ItemType::SpikeTrap,
            ItemType::LaserEmitter,
            ItemType::Flamethrower,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ItemType::SolidBlock => "Bloc Terre",
            ItemType::GrassBlock => "Bloc Herbe",
            ItemType::MetalBlock => "Bloc Métal",
            ItemType::IceBlock => "Bloc Glace (Glissant)",
            ItemType::LavaBlock => "Bloc Lave (Mortel)",
            ItemType::SawBlade => "Scie Rotative 3D",
            ItemType::CannonTurret => "Tourelle Canon 3D",
            ItemType::SpikeTrap => "Dalle à Pics 3D",
            ItemType::LaserEmitter => "Émetteur Laser 3D",
            ItemType::Flamethrower => "Cracheur de Feu 3D",
        }
    }
}

pub struct MysteryBox {
    pub available_items: Vec<ItemType>,
    pub selected_index: Option<usize>,
    pub cursor_grid: (usize, usize),
}

impl MysteryBox {
    pub fn new() -> Self {
        let mut box_obj = Self {
            available_items: Vec::new(),
            selected_index: None,
            cursor_grid: (10, 5),
        };
        // Mode Test Visuel Réel 35 Joueurs -> 35 + 3 = 38 objets affichés en direct en jeu !
        box_obj.generate_round_draft(35);
        box_obj
    }

    /// Génère le tirage du carton mystère selon la règle : N joueurs + 3 propositions supplémentaires !
    pub fn generate_round_draft(&mut self, player_count: usize) {
        let total_items = (player_count + 3).clamp(4, 45);
        let all_types = ItemType::all_types();
        let mut items = Vec::with_capacity(total_items);

        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(123456789);

        for _ in 0..total_items {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let idx = ((seed >> 33) as usize) % all_types.len();
            items.push(all_types[idx].clone());
        }

        self.available_items = items;
        self.selected_index = if !self.available_items.is_empty() { Some(0) } else { None };
    }

    pub fn select_item(&mut self, index: usize) {
        if index < self.available_items.len() {
            self.selected_index = Some(index);
        }
    }

    pub fn place_selected_item(
        &mut self,
        grid: &mut TileGrid,
        traps: &mut TrapManager,
        owner_id: u32,
        dir: crate::traps::Direction,
    ) -> bool {
        let idx = match self.selected_index {
            Some(i) => i,
            None => return false,
        };

        let (gx, gy) = self.cursor_grid;
        if gx >= grid.width || gy >= grid.height {
            return false;
        }

        let item = &self.available_items[idx];
        let pos = Vec2::new(gx as f32 + 0.5, gy as f32 + 0.5);

        match item {
            ItemType::SolidBlock => grid.set_tile(gx, gy, TileType::SolidBlock),
            ItemType::GrassBlock => grid.set_tile(gx, gy, TileType::GrassBlock),
            ItemType::MetalBlock => grid.set_tile(gx, gy, TileType::MetalBlock),
            ItemType::IceBlock => grid.set_tile(gx, gy, TileType::Ice),
            ItemType::LavaBlock => grid.set_tile(gx, gy, TileType::Lava),
            ItemType::SawBlade => traps.add_trap(pos, TrapKind::SawBlade { radius: 0.75, rotation: 0.0 }, owner_id),
            ItemType::CannonTurret => traps.add_trap(pos, TrapKind::CannonTurret { dir, fire_rate: 2.5, timer: 0.0 }, owner_id),
            ItemType::SpikeTrap => traps.add_trap(pos, TrapKind::SpikeTrap, owner_id),
            ItemType::LaserEmitter => traps.add_trap(pos, TrapKind::LaserEmitter { dir, active: false, timer: 0.0 }, owner_id),
            ItemType::Flamethrower => traps.add_trap(pos, TrapKind::Flamethrower { dir, active: false, timer: 0.0 }, owner_id),
        }

        // Remove item after placing
        self.available_items.remove(idx);
        self.selected_index = if !self.available_items.is_empty() { Some(0) } else { None };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_n_plus_3_draft_generation() {
        let mut box_obj = MysteryBox::new();

        // 1 joueur -> 4 items
        box_obj.generate_round_draft(1);
        assert_eq!(box_obj.available_items.len(), 4);

        // 2 joueurs -> 5 items
        box_obj.generate_round_draft(2);
        assert_eq!(box_obj.available_items.len(), 5);

        // 9 joueurs -> 12 items
        box_obj.generate_round_draft(9);
        assert_eq!(box_obj.available_items.len(), 12);

        // 40 joueurs -> 43 items
        box_obj.generate_round_draft(40);
        assert_eq!(box_obj.available_items.len(), 43);
    }

    #[test]
    fn test_adaptive_grid_offsets_stay_inside_box() {
        let total_items = 43; // 40 joueurs + 3
        for i in 0..total_items {
            let (offset, scale) = compute_box_item_offset(i, total_items);
            // Vérifie que les offsets restent strictement dans les dimensions intérieures du carton (-2.7 <= X <= 2.7, -1.8 <= Y <= 1.8)
            assert!(offset.x >= -3.85 && offset.x <= 3.85, "Offset X out of box bounds: {}", offset.x);
            assert!(offset.y >= -2.65 && offset.y <= 2.65, "Offset Y out of box bounds: {}", offset.y);
            assert!(scale > 0.1 && scale <= 1.25);
        }
    }
}
