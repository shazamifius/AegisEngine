use aegis_engine::math::{Vec2, Vec4};
use crate::grid::{TileGrid, TileType};
use crate::player::{Player, InputState};
use crate::traps::TrapManager;
use crate::mystery_box::MysteryBox;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    Drafting,
    Placement,
    Running,
    RoundSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorBlockType {
    Grass,  // Touche 1 : Vert (Herbe)
    Dirt,   // Touche 2 : Marron (Terre)
    Stone,  // Touche 3 : Gris (Pierre)
    Start,  // Touche 4 : Point de Spawn
    Finish, // Touche 5 : Point d'Arrivée
}

impl EditorBlockType {
    pub fn to_tile_type(&self) -> TileType {
        match self {
            EditorBlockType::Grass => TileType::GrassBlock,
            EditorBlockType::Dirt => TileType::SolidBlock,
            EditorBlockType::Stone => TileType::MetalBlock,
            EditorBlockType::Start => TileType::StartPoint,
            EditorBlockType::Finish => TileType::FinishFlag,
        }
    }

    pub fn color(&self) -> Vec4 {
        match self {
            EditorBlockType::Grass => Vec4::new(0.25, 0.85, 0.30, 1.0),  // Herbe Verte Vive
            EditorBlockType::Dirt => Vec4::new(0.48, 0.32, 0.20, 1.0),   // Terre Marron Chaude
            EditorBlockType::Stone => Vec4::new(0.60, 0.63, 0.68, 1.0),  // Pierre Grise Élégante
            EditorBlockType::Start => Vec4::new(0.20, 0.50, 0.95, 1.0),  // Point de Spawn Bleu
            EditorBlockType::Finish => Vec4::new(0.98, 0.85, 0.10, 1.0), // Drapeau Arrivée Or
        }
    }
}

pub struct EditorState {
    pub cursor: (i32, i32),
    pub selected_block: EditorBlockType,
    pub move_cooldown: f32,
}

#[derive(Debug, Clone)]
pub struct PlayerSession {
    pub id: u32,
    pub name: String,
    pub player: Player,
    pub is_human: bool,
    pub trap_points: u32,
    pub victory_points: u32,
    pub total_score: u32,
}

impl PlayerSession {
    pub fn new(id: u32, name: impl Into<String>, start_pos: Vec2, is_human: bool) -> Self {
        Self {
            id,
            name: name.into(),
            player: Player::new(start_pos),
            is_human,
            trap_points: 0,
            victory_points: 0,
            total_score: 0,
        }
    }

    pub fn recalculate_score(&mut self) {
        self.total_score = self.trap_points * 100 + self.victory_points * 200;
    }
}

pub struct PartyGame {
    pub grid: TileGrid,
    pub players: Vec<PlayerSession>,
    pub traps: TrapManager,
    pub mystery_box: MysteryBox,
    pub particles: crate::particles::ParticleEffectManager,
    pub editor: EditorState,
    pub is_play_mode: bool, // true = Jouer directement sur la Map / false = Mode Éditeur
    pub phase: GamePhase,
    pub round_number: u32,
    pub round_timer: f32,
}

impl PartyGame {
    pub fn new(grid_width: usize, grid_height: usize) -> Self {
        let grid = TileGrid::new(grid_width, grid_height);
        let start_p = grid.start_pos;

        let mut players = vec![
            PlayerSession::new(0, "Joueur 1 (Toi)", start_p, true),
        ];
        players[0].player.position = start_p;

        let traps = TrapManager::new();
        let mystery_box = MysteryBox::new();
        let particles = crate::particles::ParticleEffectManager::new();

        let editor = EditorState {
            cursor: (start_p.x as i32, start_p.y as i32),
            selected_block: EditorBlockType::Grass,
            move_cooldown: 0.0,
        };

        Self {
            grid,
            players,
            traps,
            mystery_box,
            particles,
            editor,
            is_play_mode: true, // Démarrage direct en Mode JEU sur la Map créée !
            phase: GamePhase::Running,
            round_number: 1,
            round_timer: 0.0,
        }
    }

    pub fn human_player(&self) -> &Player {
        &self.players[0].player
    }

    pub fn toggle_mode(&mut self) {
        self.is_play_mode = !self.is_play_mode;
        if self.is_play_mode {
            self.reset_player_to_spawn();
            log::info!("🎮 Passation en MODE JEU DIRECT SUR LA MAP !");
        } else {
            log::info!("🛠️ Passation en MODE ÉDITEUR DE MAP !");
        }
    }

    pub fn reset_player_to_spawn(&mut self) {
        let spawn = self.grid.start_pos;
        self.players[0].player.reset(spawn);
    }

    pub fn place_selected_block(&mut self) {
        let (cx, cy) = self.editor.cursor;
        let tile = self.editor.selected_block.to_tile_type();
        self.grid.set_tile(cx as usize, cy as usize, tile);
        if self.editor.selected_block == EditorBlockType::Start {
            self.players[0].player.position = Vec2::new(cx as f32 + 0.5, cy as f32 + 1.0);
        }
        let _ = self.grid.save_to_file("custom_map.lvl");
        log::info!("Bloc posé à ({}, {}) : {:?} (Sauvegardé)", cx, cy, self.editor.selected_block);
    }

    pub fn delete_selected_block(&mut self) {
        let (cx, cy) = self.editor.cursor;
        self.grid.set_tile(cx as usize, cy as usize, TileType::Air);
        let _ = self.grid.save_to_file("custom_map.lvl");
        log::info!("Bloc supprimé à ({}, {}) (Sauvegardé)", cx, cy);
    }

    pub fn update(&mut self, dt: f32, input: &InputState) {
        self.round_timer += dt;

        if self.is_play_mode {
            // MODE JEU DIRECT SUR LA MAP : Contrôle physique du personnage joueur
            self.players[0].player.update(dt, input, &self.grid, &self.traps);
        } else {
            // MODE ÉDITEUR DE MAP : Contrôle du curseur d'édition ZQSD
            if self.editor.move_cooldown > 0.0 {
                self.editor.move_cooldown -= dt;
            }

            if self.editor.move_cooldown <= 0.0 {
                let mut dx = 0;
                let mut dy = 0;
                if input.left { dx -= 1; }
                if input.right { dx += 1; }
                if input.up { dy += 1; }
                if input.down { dy -= 1; }

                if dx != 0 || dy != 0 {
                    self.editor.cursor.0 = (self.editor.cursor.0 + dx).clamp(0, 300);
                    self.editor.cursor.1 = (self.editor.cursor.1 + dy).clamp(0, 150);
                    self.editor.move_cooldown = 0.12;
                }
            }

            if input.jump_pressed_this_frame {
                self.place_selected_block();
            }
        }

        self.particles.update(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_party_game_loop() {
        let game = PartyGame::new(32, 18);
        assert_eq!(game.players.len(), 1);
        assert_eq!(game.phase, GamePhase::Running);
    }
}
