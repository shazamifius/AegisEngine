use aegis_engine::math::{Vec2, Vec4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Air,
    SolidBlock,
    GrassBlock,
    MetalBlock,
    WoodPlank,
    CloudPlatform,
    Lava,
    Ice,
    StickyHoney,
    Portal,
    AntiGravityBubble,
    BlackHole,
    SpikesUp,
    SpikesDown,
    SpikesLeft,
    SpikesRight,
    StartPoint,
    FinishFlag,
}

impl TileType {
    pub fn is_solid(&self) -> bool {
        matches!(
            self,
            TileType::SolidBlock
                | TileType::GrassBlock
                | TileType::MetalBlock
                | TileType::WoodPlank
                | TileType::CloudPlatform
                | TileType::Ice
                | TileType::StickyHoney
        )
    }

    pub fn is_hazard(&self) -> bool {
        matches!(
            self,
            TileType::Lava
                | TileType::SpikesUp
                | TileType::SpikesDown
                | TileType::SpikesLeft
                | TileType::SpikesRight
        )
    }

    pub fn is_ice(&self) -> bool {
        matches!(self, TileType::Ice)
    }

    pub fn is_honey(&self) -> bool {
        matches!(self, TileType::StickyHoney)
    }

    pub fn color(&self) -> Vec4 {
        match self {
            TileType::Air => Vec4::new(0.0, 0.0, 0.0, 0.0),
            TileType::SolidBlock => Vec4::new(0.42, 0.28, 0.18, 1.0),     // Earth dirt brown
            TileType::GrassBlock => Vec4::new(0.25, 0.75, 0.25, 1.0),     // Grass green
            TileType::MetalBlock => Vec4::new(0.55, 0.58, 0.65, 1.0),     // Metal grey
            TileType::WoodPlank => Vec4::new(0.68, 0.45, 0.22, 1.0),      // Oak wood plank
            TileType::CloudPlatform => Vec4::new(0.90, 0.95, 1.0, 0.85),   // Fluffy white cloud
            TileType::Lava => Vec4::new(0.98, 0.35, 0.05, 1.0),           // Glowing lava
            TileType::Ice => Vec4::new(0.45, 0.88, 0.98, 0.85),           // Ice cyan
            TileType::StickyHoney => Vec4::new(0.98, 0.75, 0.10, 0.92),   // Golden honey
            TileType::Portal => Vec4::new(0.65, 0.20, 0.95, 1.0),         // Cosmic portal purple
            TileType::AntiGravityBubble => Vec4::new(0.20, 0.90, 0.95, 0.75), // Anti-gravity bubble
            TileType::BlackHole => Vec4::new(0.08, 0.05, 0.15, 1.0),       // Black hole singularity
            TileType::SpikesUp | TileType::SpikesDown | TileType::SpikesLeft | TileType::SpikesRight => {
                Vec4::new(0.85, 0.15, 0.15, 1.0)                          // Red spikes
            }
            TileType::StartPoint => Vec4::new(0.2, 0.45, 0.95, 1.0),       // Start point blue
            TileType::FinishFlag => Vec4::new(0.98, 0.85, 0.1, 1.0),      // Finish flag gold
        }
    }
}

impl TileType {
    pub fn to_u8(&self) -> u8 {
        match self {
            TileType::Air => 0,
            TileType::SolidBlock => 1,
            TileType::GrassBlock => 2,
            TileType::MetalBlock => 3,
            TileType::StartPoint => 4,
            TileType::FinishFlag => 5,
            _ => 0,
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => TileType::SolidBlock,
            2 => TileType::GrassBlock,
            3 => TileType::MetalBlock,
            4 => TileType::StartPoint,
            5 => TileType::FinishFlag,
            _ => TileType::Air,
        }
    }
}

pub struct TileGrid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<TileType>,
    pub start_pos: Vec2,
    pub finish_pos: Vec2,
}

impl TileGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let mut grid = Self {
            width,
            height,
            tiles: vec![TileType::Air; width * height],
            start_pos: Vec2::new(3.5, 1.0),
            finish_pos: Vec2::new((width - 4) as f32 + 0.5, 2.0),
        };
        if grid.load_from_file("custom_map.lvl").is_err() {
            grid.load_default_stage();
        }
        grid
    }

    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        writeln!(f, "{} {} {} {} {} {}", self.width, self.height, self.start_pos.x, self.start_pos.y, self.finish_pos.x, self.finish_pos.y)?;
        for tile in &self.tiles {
            write!(f, "{} ", tile.to_u8())?;
        }
        writeln!(f)?;
        log::info!("Carte enregistrée avec succès sur le disque !");
        Ok(())
    }

    pub fn load_from_file(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::BufRead;
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut line1 = String::new();
        reader.read_line(&mut line1)?;
        let parts: Vec<&str> = line1.split_whitespace().collect();
        if parts.len() >= 6 {
            self.width = parts[0].parse()?;
            self.height = parts[1].parse()?;
            self.start_pos.x = parts[2].parse()?;
            self.start_pos.y = parts[3].parse()?;
            self.finish_pos.x = parts[4].parse()?;
            self.finish_pos.y = parts[5].parse()?;
        }

        let mut line2 = String::new();
        reader.read_line(&mut line2)?;
        let tiles_str: Vec<&str> = line2.split_whitespace().collect();
        self.tiles = vec![TileType::Air; self.width * self.height];
        for (idx, val_str) in tiles_str.iter().enumerate() {
            if idx < self.tiles.len() {
                if let Ok(val) = val_str.parse::<u8>() {
                    self.tiles[idx] = TileType::from_u8(val);
                }
            }
        }
        log::info!("Carte chargée avec succès depuis le disque !");
        Ok(())
    }

    pub fn ensure_capacity(&mut self, required_w: usize, required_h: usize) {
        if required_w <= self.width && required_h <= self.height {
            return;
        }
        let new_w = self.width.max(required_w);
        let new_h = self.height.max(required_h);
        let mut new_tiles = vec![TileType::Air; new_w * new_h];

        for y in 0..self.height {
            for x in 0..self.width {
                new_tiles[y * new_w + x] = self.tiles[y * self.width + x];
            }
        }

        self.width = new_w;
        self.height = new_h;
        self.tiles = new_tiles;
    }

    pub fn set_tile(&mut self, x: usize, y: usize, tile: TileType) {
        self.ensure_capacity(x + 1, y + 1);
        self.tiles[y * self.width + x] = tile;
        if tile == TileType::StartPoint {
            self.start_pos = Vec2::new(x as f32 + 0.5, y as f32 + 1.0);
        } else if tile == TileType::FinishFlag {
            self.finish_pos = Vec2::new(x as f32 + 0.5, y as f32 + 1.0);
        }
    }

    pub fn get_tile(&self, x: i32, y: i32) -> TileType {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return TileType::Air; // Air en dehors des limites pour permettre la chute dans le vide !
        }
        self.tiles[y as usize * self.width + x as usize]
    }

    pub fn load_default_stage(&mut self) {
        for t in self.tiles.iter_mut() {
            *t = TileType::Air;
        }

        // Sol de base propre en bas de la carte (row y=0)
        for x in 0..self.width {
            self.tiles[0 * self.width + x] = TileType::GrassBlock;
        }

        self.start_pos = Vec2::new(3.5, 1.0);
        self.finish_pos = Vec2::new((self.width - 4) as f32 + 0.5, 2.0);
    }

    pub fn get_lowest_block_y(&self) -> f32 {
        let mut min_y = f32::MAX;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.tiles[y * self.width + x] != TileType::Air {
                    min_y = min_y.min(y as f32);
                }
            }
        }
        if min_y == f32::MAX {
            0.0
        } else {
            min_y
        }
    }

    pub fn get_void_kill_y(&self) -> f32 {
        // Toujours exactement 5 blocs en dessous du bloc le plus bas de la carte
        self.get_lowest_block_y() - 5.0
    }

    pub fn check_solid_collision(&self, pos: Vec2, size: Vec2) -> bool {
        let half_w = size.x / 2.0;
        let left = pos.x - half_w;
        let right = pos.x + half_w;
        let bottom = pos.y;
        let top = pos.y + size.y;

        // Grille de blocs uniquement (pas de barrières invisibles autour de la map!)
        let min_x = left.floor() as i32;
        let max_x = right.floor() as i32;
        let min_y = bottom.floor() as i32;
        let max_y = top.floor() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.get_tile(x, y).is_solid() {
                    return true;
                }
            }
        }
        false
    }

    pub fn check_hazard_collision(&self, pos: Vec2, size: Vec2) -> bool {
        // Mort immédiate si le joueur tombe au niveau de la ligne du vide (5 blocs sous le bloc le plus bas)
        if pos.y <= self.get_void_kill_y() {
            return true;
        }

        let half_w = size.x / 2.0;
        let min_x = (pos.x - half_w).floor() as i32;
        let max_x = (pos.x + half_w).floor() as i32;
        let min_y = pos.y.floor() as i32;
        let max_y = (pos.y + size.y).floor() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.get_tile(x, y).is_hazard() {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_initialization() {
        let grid = TileGrid::new(32, 18);
        assert_eq!(grid.width, 32);
        assert_eq!(grid.height, 18);
    }

    #[test]
    fn test_map_saving_and_loading() {
        let mut grid = TileGrid::new(32, 18);
        grid.set_tile(5, 5, TileType::GrassBlock);
        grid.set_tile(6, 5, TileType::SolidBlock);
        grid.set_tile(7, 5, TileType::MetalBlock);
        grid.set_tile(8, 5, TileType::StartPoint);
        grid.set_tile(9, 5, TileType::FinishFlag);

        let test_path = "test_custom_map.lvl";
        assert!(grid.save_to_file(test_path).is_ok());

        let mut loaded_grid = TileGrid::new(32, 18);
        assert!(loaded_grid.load_from_file(test_path).is_ok());

        assert_eq!(loaded_grid.get_tile(5, 5), TileType::GrassBlock);
        assert_eq!(loaded_grid.get_tile(6, 5), TileType::SolidBlock);
        assert_eq!(loaded_grid.get_tile(7, 5), TileType::MetalBlock);
        assert_eq!(loaded_grid.get_tile(8, 5), TileType::StartPoint);
        assert_eq!(loaded_grid.get_tile(9, 5), TileType::FinishFlag);

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_dynamic_void_kill_plane() {
        let mut grid = TileGrid::new(32, 18);
        for t in grid.tiles.iter_mut() { *t = TileType::Air; }

        grid.set_tile(10, 4, TileType::GrassBlock);
        assert_eq!(grid.get_lowest_block_y(), 4.0);
        assert_eq!(grid.get_void_kill_y(), -1.0); // 4 - 5 = -1.0

        grid.set_tile(12, 2, TileType::SolidBlock);
        assert_eq!(grid.get_lowest_block_y(), 2.0);
        assert_eq!(grid.get_void_kill_y(), -3.0); // 2 - 5 = -3.0

        // Mort instantanée si le joueur tombe au niveau de la ligne noire du vide
        assert!(grid.check_hazard_collision(Vec2::new(12.0, -3.5), Vec2::new(0.6, 1.2)));
        assert!(!grid.check_hazard_collision(Vec2::new(12.0, 3.0), Vec2::new(0.6, 1.2)));
    }
}
