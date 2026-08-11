use aegis_engine::math::Vec2;
use crate::grid::{TileGrid, TileType};
use crate::traps::{TrapManager, TrapKind, Direction};

#[derive(Debug, Clone)]
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
        box_obj.generate_round_draft(6);
        box_obj
    }

    pub fn generate_round_draft(&mut self, count: usize) {
        let pool = [
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
        ];

        self.available_items.clear();
        for i in 0..count {
            self.available_items.push(pool[i % pool.len()].clone());
        }
        self.selected_index = if count > 0 { Some(0) } else { None };
    }

    pub fn select_item(&mut self, index: usize) {
        if index < self.available_items.len() {
            self.selected_index = Some(index);
        }
    }

    pub fn place_selected_item(&mut self, grid: &mut TileGrid, traps: &mut TrapManager, owner_id: u32) -> bool {
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
            ItemType::CannonTurret => traps.add_trap(pos, TrapKind::CannonTurret { dir: Direction::Right, fire_rate: 2.5, timer: 0.0 }, owner_id),
            ItemType::SpikeTrap => traps.add_trap(pos, TrapKind::SpikeTrap, owner_id),
            ItemType::LaserEmitter => traps.add_trap(pos, TrapKind::LaserEmitter { dir: Direction::Up, active: true, timer: 0.0 }, owner_id),
            ItemType::Flamethrower => traps.add_trap(pos, TrapKind::Flamethrower { dir: Direction::Right, active: true, timer: 0.0 }, owner_id),
        }

        // Remove item after placing
        self.available_items.remove(idx);
        self.selected_index = if !self.available_items.is_empty() { Some(0) } else { None };
        true
    }
}
