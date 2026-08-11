use aegis_engine::math::{Vec2, Vec4};
use crate::grid::{TileGrid, TileType};
use crate::player::{Player, InputState};
use crate::traps::TrapManager;
use crate::mystery_box::MysteryBox;
use crate::web3_integration::Web3Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    Drafting,    // 10s max (N+3 items draft)
    Placement,   // 30s max (Positionnement de l'objet)
    Running,     // 150s max (2m30s de course & pièges)
    Leaderboard, // 10s max (Tableau des scores & classement)
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
            EditorBlockType::Grass  => TileType::GrassBlock,
            EditorBlockType::Dirt   => TileType::SolidBlock,
            EditorBlockType::Stone  => TileType::MetalBlock,
            EditorBlockType::Start  => TileType::StartPoint,
            EditorBlockType::Finish => TileType::FinishFlag,
        }
    }

    pub fn color(&self) -> Vec4 {
        match self {
            EditorBlockType::Grass  => Vec4::new(0.25, 0.85, 0.30, 1.0),  // Herbe Verte Vive
            EditorBlockType::Dirt   => Vec4::new(0.48, 0.32, 0.20, 1.0),  // Terre Marron Chaude
            EditorBlockType::Stone  => Vec4::new(0.60, 0.63, 0.68, 1.0),  // Pierre Grise Élégante
            EditorBlockType::Start  => Vec4::new(0.20, 0.50, 0.95, 1.0),  // Point de Spawn Bleu
            EditorBlockType::Finish => Vec4::new(0.98, 0.85, 0.10, 1.0),  // Drapeau Arrivée Or
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

    pub win_points: f32,       // +3 pour 1er, +1 si fini (0 si > 50% ont fini)
    pub trap_points: f32,      // +1 par kill d'adversaire (0 si 0 arrivé), -1 si autokill
    pub negative_points: f32,  // -0.5 si en vie mais pas arrivé à la fin des 2m30
    pub total_score: f32,

    pub placement_done: bool,
    pub has_finished: bool,
    pub finish_rank: Option<usize>,
    pub is_dead: bool,
    pub killed_by_owner_id: Option<u32>,
}

impl PlayerSession {
    pub fn new(id: u32, name: impl Into<String>, start_pos: Vec2, is_human: bool) -> Self {
        Self {
            id,
            name: name.into(),
            player: Player::new(start_pos),
            is_human,
            win_points: 0.0,
            trap_points: 0.0,
            negative_points: 0.0,
            total_score: 0.0,
            placement_done: false,
            has_finished: false,
            finish_rank: None,
            is_dead: false,
            killed_by_owner_id: None,
        }
    }

    pub fn recalculate_score(&mut self) {
        self.total_score = self.win_points + self.trap_points - self.negative_points;
    }
}

pub struct PartyGame {
    pub grid: TileGrid,
    pub players: Vec<PlayerSession>,
    pub traps: TrapManager,
    pub mystery_box: MysteryBox,
    pub particles: crate::particles::ParticleEffectManager,
    pub editor: EditorState,
    pub is_play_mode: bool,
    pub phase: GamePhase,
    pub round_number: u32,
    pub round_timer: f32,
    pub draft_timer: f32,
    pub draft_cooldown: f32,

    pub placement_timer: f32,
    pub placement_place_request: bool,
    pub placement_dir: crate::traps::Direction,

    pub running_timer: f32,     // 150s (2m30s) max
    pub leaderboard_timer: f32, // 10s max
    pub match_winner: Option<String>,
}

impl PartyGame {
    pub fn new(grid_width: usize, grid_height: usize) -> Self {
        let grid = TileGrid::new(grid_width, grid_height);
        let start_p = grid.start_pos;

        // Chargement des noms Web3 depuis /home/shaza/Documents/projet web 3/players.json
        let web3_mgr = Web3Manager::new();
        let web3_players = web3_mgr.load_player_names(1);

        let mut players = Vec::new();
        for p in web3_players {
            let mut ps = PlayerSession::new(p.id, p.name, start_p, p.id == 0);
            ps.player.position = start_p;
            players.push(ps);
        }

        let traps = TrapManager::new();
        let mut mystery_box = MysteryBox::new();
        mystery_box.generate_round_draft(players.len());

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
            is_play_mode: true,
            phase: GamePhase::Drafting,
            round_number: 1,
            round_timer: 0.0,
            draft_timer: 10.0,       // 10s pour choisir son objet
            draft_cooldown: 0.0,
            placement_timer: 30.0,   // 30s max pour poser son objet sur la map
            placement_place_request: false,
            placement_dir: crate::traps::Direction::Up,
            running_timer: 150.0,    // 2 minutes 30s de course
            leaderboard_timer: 10.0, // 10s d'affichage du leaderboard
            match_winner: None,
        }
    }

    pub fn human_player(&self) -> &Player {
        &self.players[0].player
    }

    /// Gardé pour pouvoir être réactivé plus tard si besoin par le créateur du jeu
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

    /// Gardé pour l'éditeur complet (non accessible par les joueurs)
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

    /// Gardé pour l'éditeur complet (non accessible par les joueurs)
    pub fn delete_selected_block(&mut self) {
        let (cx, cy) = self.editor.cursor;
        self.grid.set_tile(cx as usize, cy as usize, TileType::Air);
        let _ = self.grid.save_to_file("custom_map.lvl");
        log::info!("Bloc supprimé à ({}, {}) (Sauvegardé)", cx, cy);
    }

    fn all_players_placed(&self) -> bool {
        self.players.iter().all(|p| p.placement_done)
    }

    /// Calcul déterministe des points selon les règles du jeu
    pub fn evaluate_round_scores(&mut self) {
        let total_players = self.players.len();
        if total_players == 0 {
            return;
        }

        let finished_count = self.players.iter().filter(|p| p.has_finished).count();
        let win_ratio = finished_count as f32 / total_players as f32;

        // 1. Calcul des Points Win ("point win")
        // "si tu arrive le premier tu gagne 3 point, si tu arrive a la ligne arriver tu gagne 1 point"
        // "si plus de 50% des joueur on attein la ligne arrive personne gagne de point win"
        if finished_count > 0 && win_ratio <= 0.50 {
            for p in &mut self.players {
                if p.has_finished {
                    if p.finish_rank == Some(1) {
                        p.win_points += 3.0; // 1er arrivé -> 3 points
                    } else {
                        p.win_points += 1.0; // Autres arrivés -> 1 point
                    }
                }
            }
        }

        // 2. Calcul des Points Trap ("point trap")
        // "a chaque fois que tu a tuer quelqu'un avec un objet de destruction tu gagne 1 point"
        // "a chaque fois que tu meurt de ton propre objet tu perd 1 point"
        // "si personne ne gagne tu ne peut pas gagner de point trap pour avoir tuer quelqu'un"
        for i in 0..self.players.len() {
            let (is_dead, killer_opt, victim_id) = {
                let p = &self.players[i];
                (p.is_dead, p.killed_by_owner_id, p.id)
            };

            if is_dead {
                if let Some(killer_id) = killer_opt {
                    if killer_id == victim_id {
                        // Autokill -> -1 point (toujours appliqué)
                        self.players[i].trap_points -= 1.0;
                    } else if finished_count > 0 {
                        // Kill sur un adversaire -> +1 point pour le tueur (UNIQUEMENT si au moins 1 personne a fini !)
                        if let Some(killer_player) = self.players.iter_mut().find(|p| p.id == killer_id) {
                            killer_player.trap_points += 1.0;
                        }
                    }
                }
            }
        }

        // 3. Calcul des Points Négatifs ("point negatif")
        // "si tu n'attein pas la ligne arriver mais que tu reste en vie tu perd 0.5 de point"
        for p in &mut self.players {
            if !p.has_finished && !p.is_dead {
                p.negative_points += 0.5;
            }
        }

        // 4. Recalcul des Scores Totaux et vérification de la victoire (30 points)
        for p in &mut self.players {
            p.recalculate_score();
            if p.total_score >= 30.0 && self.match_winner.is_none() {
                self.match_winner = Some(p.name.clone());
            }
        }
    }

    pub fn update(&mut self, dt: f32, input: &InputState) {
        self.round_timer += dt;

        self.traps.update(dt);
        self.particles.update(dt);

        // ─── 1. Phase de Draft (10s max) ─────────────────────────────────────────
        if self.phase == GamePhase::Drafting {
            self.draft_timer = (self.draft_timer - dt).max(0.0);
            if self.draft_cooldown > 0.0 {
                self.draft_cooldown -= dt;
            }

            if self.draft_cooldown <= 0.0 {
                let items_count = self.mystery_box.available_items.len();
                if items_count > 0 {
                    let mut curr_idx = self.mystery_box.selected_index.unwrap_or(0);
                    if input.left {
                        curr_idx = if curr_idx == 0 { items_count - 1 } else { curr_idx - 1 };
                        self.mystery_box.selected_index = Some(curr_idx);
                        self.draft_cooldown = 0.18;
                    } else if input.right {
                        curr_idx = (curr_idx + 1) % items_count;
                        self.mystery_box.selected_index = Some(curr_idx);
                        self.draft_cooldown = 0.18;
                    }
                }
            }

            // Fin du draft (10s ou validation Espace)
            if self.draft_timer <= 0.0 || (input.jump && self.draft_cooldown <= 0.0) {
                self.phase = GamePhase::Placement;
                self.editor.cursor = (self.grid.width as i32 / 2, self.grid.height as i32 / 2);
                self.placement_timer = 30.0;
                for p in &mut self.players { p.placement_done = false; }
                log::info!("📦 Phase de Draft Terminée ! → Phase de Placement (30s max).");
            }
            return;
        }

        // ─── 2. Phase de Placement (30s max) ─────────────────────────────────────
        if self.phase == GamePhase::Placement {
            self.placement_timer = (self.placement_timer - dt).max(0.0);

            if self.editor.move_cooldown > 0.0 {
                self.editor.move_cooldown -= dt;
            }

            if self.editor.move_cooldown <= 0.0 {
                let mut dx = 0;
                let mut dy = 0;
                if input.left  { dx -= 1; }
                if input.right { dx += 1; }
                if input.up    { dy += 1; }
                if input.down  { dy -= 1; }

                if dx != 0 || dy != 0 {
                    self.editor.cursor.0 = (self.editor.cursor.0 + dx).clamp(0, (self.grid.width - 1) as i32);
                    self.editor.cursor.1 = (self.editor.cursor.1 + dy).clamp(0, (self.grid.height - 1) as i32);
                    self.editor.move_cooldown = 0.10;
                }
            }

            if input.jump_pressed_this_frame || self.placement_place_request {
                self.placement_place_request = false;
                let (cx, cy) = (self.editor.cursor.0 as usize, self.editor.cursor.1 as usize);
                self.mystery_box.cursor_grid = (cx, cy);
                let placed = self.mystery_box.place_selected_item(
                    &mut self.grid,
                    &mut self.traps,
                    self.players[0].id,
                    self.placement_dir,
                );
                if placed {
                    self.players[0].placement_done = true;
                    log::info!("✅ Objet pioché posé avec succès à ({}, {})", cx, cy);
                }
            }

            if self.all_players_placed() || self.placement_timer <= 0.0 {
                self.phase = GamePhase::Running;
                self.running_timer = 150.0; // 2 minutes 30 secondes
                self.reset_player_to_spawn();
                for p in &mut self.players {
                    p.has_finished = false;
                    p.finish_rank = None;
                    p.is_dead = false;
                    p.killed_by_owner_id = None;
                }
                log::info!("🏁 Phase de Placement Terminée ! Lancement du jeu (2m30s max).");
            }
            return;
        }

        // ─── 3. Phase de Jeu (Running) (150s / 2m30 max) ──────────────────────────────
        if self.phase == GamePhase::Running {
            self.running_timer = (self.running_timer - dt).max(0.0);

            if self.is_play_mode {
                let finish_pos = self.grid.finish_pos;

                for i in 0..self.players.len() {
                    let has_finished = self.players[i].has_finished;
                    let is_dead = self.players[i].is_dead;

                    if !has_finished && !is_dead {
                        self.players[i].player.update(dt, input, &self.grid, &self.traps);

                        // Détection d'Arrivée
                        let player_pos = self.players[i].player.position;
                        let dist_to_finish = (player_pos - finish_pos).length();
                        if dist_to_finish < 1.2 {
                            self.players[i].has_finished = true;
                            let rank = self.players.iter().filter(|p| p.has_finished).count();
                            self.players[i].finish_rank = Some(rank);
                            log::info!("🏆 {} a franchi la ligne d'arrivée en rang {} !", self.players[i].name, rank);
                        }

                        // Détection de Mort (Vide)
                        let void_y = self.grid.get_void_kill_y();
                        if player_pos.y < void_y {
                            self.players[i].is_dead = true;
                            log::info!("💀 {} est tombé dans le vide !", self.players[i].name);
                        }
                    }
                }
            }

            let all_done = self.players.iter().all(|p| p.has_finished || p.is_dead);

            if all_done || self.running_timer <= 0.0 {
                self.evaluate_round_scores();
                self.phase = GamePhase::Leaderboard;
                self.leaderboard_timer = 10.0;
                log::info!("📊 Phase de Jeu Terminée ! Affichage du Leaderboard (10s).");
            }
            return;
        }

        // ─── 4. Phase de Leaderboard (10s max) ───────────────────────────────────────
        if self.phase == GamePhase::Leaderboard {
            self.leaderboard_timer = (self.leaderboard_timer - dt).max(0.0);

            if self.leaderboard_timer <= 0.0 {
                self.round_number += 1;
                self.phase = GamePhase::Drafting;
                self.draft_timer = 10.0;
                self.mystery_box.generate_round_draft(self.players.len());

                for p in &mut self.players {
                    p.placement_done = false;
                    p.has_finished = false;
                    p.finish_rank = None;
                    p.is_dead = false;
                    p.killed_by_owner_id = None;
                }
                self.reset_player_to_spawn();
                log::info!("🔄 Lancement de la Manche {} !", self.round_number);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_party_game_drafting_to_placement() {
        let mut game = PartyGame::new(32, 18);
        assert_eq!(game.phase, GamePhase::Drafting);

        let input = InputState::default();
        game.update(10.5, &input);
        assert_eq!(game.phase, GamePhase::Placement);
    }

    #[test]
    fn test_placement_to_running_on_timeout() {
        let mut game = PartyGame::new(32, 18);
        let input = InputState::default();
        game.update(10.5, &input);
        assert_eq!(game.phase, GamePhase::Placement);
        game.update(31.0, &input);
        assert_eq!(game.phase, GamePhase::Running);
    }

    #[test]
    fn test_win_points_50_percent_rule() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "P1", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
            PlayerSession::new(2, "P3", Vec2::ZERO, false),
            PlayerSession::new(3, "P4", Vec2::ZERO, false),
        ];

        // 1er et 2ème finissent (2 / 4 = 50% -> Win points accordés !)
        game.players[0].has_finished = true;
        game.players[0].finish_rank = Some(1);
        game.players[1].has_finished = true;
        game.players[1].finish_rank = Some(2);

        game.evaluate_round_scores();

        assert_eq!(game.players[0].win_points, 3.0); // 1er -> 3 points
        assert_eq!(game.players[1].win_points, 1.0); // 2ème -> 1 point

        // Deuxième manche : 3/4 finissent (75% > 50% -> Aucun point win !)
        for p in &mut game.players { p.win_points = 0.0; }
        game.players[0].has_finished = true;
        game.players[0].finish_rank = Some(1);
        game.players[1].has_finished = true;
        game.players[1].finish_rank = Some(2);
        game.players[2].has_finished = true;
        game.players[2].finish_rank = Some(3);

        game.evaluate_round_scores();

        assert_eq!(game.players[0].win_points, 0.0); // Annulé car > 50%
        assert_eq!(game.players[1].win_points, 0.0);
    }

    #[test]
    fn test_trap_kill_points_and_autokill_penalty() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "P1", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
            PlayerSession::new(2, "P3", Vec2::ZERO, false),
        ];

        // Case A : Au moins 1 a fini (P1 arrive)
        game.players[0].has_finished = true;
        game.players[0].finish_rank = Some(1);

        // P2 tué par le piège de P1 (+1 point trap pour P1)
        game.players[1].is_dead = true;
        game.players[1].killed_by_owner_id = Some(0);

        // P3 se tue lui-même avec son propre piège (-1 point autokill pour P3)
        game.players[2].is_dead = true;
        game.players[2].killed_by_owner_id = Some(2);

        game.evaluate_round_scores();

        assert_eq!(game.players[0].trap_points, 1.0); // +1 kill sur P2
        assert_eq!(game.players[2].trap_points, -1.0); // -1 autokill

        // Case B : Personne n'arrive (0 arrivés) -> Aucun point trap pour kill adversaire !
        let mut game_no_win = PartyGame::new(32, 18);
        game_no_win.players = vec![
            PlayerSession::new(0, "P1", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
        ];
        game_no_win.players[1].is_dead = true;
        game_no_win.players[1].killed_by_owner_id = Some(0);

        game_no_win.evaluate_round_scores();
        assert_eq!(game_no_win.players[0].trap_points, 0.0); // 0 point car personne n'a gagné !
    }

    #[test]
    fn test_survival_penalty_and_30_pts_victory() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "Winner", Vec2::ZERO, true),
            PlayerSession::new(1, "Survivor", Vec2::ZERO, false),
        ];

        // Winner a franchi la ligne d'arrivée et accumulé 30 points
        game.players[0].has_finished = true;
        game.players[0].finish_rank = Some(1);
        game.players[0].win_points = 27.0; // 27 + 3 (1er) = 30 points !

        // Survivor n'est pas arrivé mais reste en vie à la fin du temps (-0.5 point)
        game.players[1].has_finished = false;
        game.players[1].is_dead = false;

        game.evaluate_round_scores();

        assert_eq!(game.players[1].negative_points, 0.5);
        assert_eq!(game.players[1].total_score, -0.5);
        assert_eq!(game.match_winner, Some("Winner".to_string()));
    }
}
