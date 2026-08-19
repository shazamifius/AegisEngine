use aegis_engine::math::{Vec2, Vec4};
use crate::grid::{TileGrid, TileType};
use crate::player::{Player, InputState};
use crate::traps::TrapManager;
use crate::mystery_box::MysteryBox;
use crate::objects::cardboard_box::CardboardBoxObject;

/// Les durées de chaque phase, en secondes.
///
/// Elles étaient écrites en dur aux **sept** endroits qui arment un minuteur. Les rassembler
/// n'est pas du rangement : la barre de progression du HUD a besoin de la durée *totale* pour
/// savoir quelle fraction il reste, et une valeur redevinée là-bas mentirait en silence le jour
/// où l'une de ces durées change ici.
pub const DUREE_DRAFT: f32 = 10.0;
pub const DUREE_PLACEMENT: f32 = 30.0;
pub const DUREE_COURSE: f32 = 150.0;
pub const DUREE_LEADERBOARD: f32 = 10.0;


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

        // Le nom du joueur vient du pseudonyme choisi dans le launcher web3 (`~/.web3/pseudo`).
        // Un seul joueur pour l'instant : le multijoueur passera par le sidecar (phase 1 du plan).
        let web3_players = crate::web3_integration::joueurs(1);

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
            draft_timer: DUREE_DRAFT,
            draft_cooldown: 0.0,
            placement_timer: DUREE_PLACEMENT,
            placement_place_request: false,
            placement_dir: crate::traps::Direction::Up,
            running_timer: DUREE_COURSE,
            leaderboard_timer: DUREE_LEADERBOARD,
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
        self.grid.enregistrer();
        log::info!("Bloc posé à ({}, {}) : {:?} (Sauvegardé)", cx, cy, self.editor.selected_block);
    }

    /// Gardé pour l'éditeur complet (non accessible par les joueurs)
    pub fn delete_selected_block(&mut self) {
        let (cx, cy) = self.editor.cursor;
        self.grid.set_tile(cx as usize, cy as usize, TileType::Air);
        self.grid.enregistrer();
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

        // ─── LA MANCHE PAIE-T-ELLE ? Une seule question, posée aux DEUX bornes. ──────────────
        //
        // Le jeu punissait déjà le sur-sabotage (personne n'arrive → personne ne marque), mais pas
        // le cas inverse : au-delà de 50 % d'arrivées, les arrivants perdaient leurs points tandis
        // que les tueurs gardaient les leurs. Cette asymétrie n'avait pas été décidée — elle venait
        // de ce qu'on testait `finished_count > 0` au lieu de « la manche a-t-elle payé quelqu'un ».
        //
        // Une manche qui ne désigne personne ne paie personne, pièges compris. C'est la règle qui
        // existait déjà, appliquée à ses deux bouts : une condition remplace les deux, aucune
        // constante n'est ajoutée.
        let manche_payante = finished_count > 0 && win_ratio <= 0.50;

        // 1. Points d'arrivée
        // "si tu es le seul a gagner tu gagne 4 point / si tu es le premier 3 point sinon 1
        //  si il y a plus de 50% qui on gagner alors 0 point pour personne"
        if manche_payante {
            // Être SEUL vaut plus qu'être premier : c'est le maximum du jeu, et il récompense
            // d'avoir réussi là où le parcours a arrêté tous les autres.
            let seul_rescape = finished_count == 1;
            for p in &mut self.players {
                if p.has_finished {
                    p.win_points += if seul_rescape {
                        4.0
                    } else if p.finish_rank == Some(1) {
                        3.0
                    } else {
                        1.0
                    };
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
                        // Autokill -> -1 point (toujours appliqué, même dans une manche nulle :
                        // c'est une punition, pas une récompense).
                        self.players[i].trap_points -= 1.0;
                    } else if manche_payante {
                        // Kill sur un adversaire -> +1 au tueur, MAIS seulement s'il a franchi la
                        // ligne lui-même.
                        //
                        // Sans cette condition, camper domine : poser son piège, ne jamais courir et
                        // tuer quatre personnes rapporte plus qu'arriver premier, pour −0,5 de
                        // pénalité. Le piège redevient ce qu'il doit être — un multiplicateur de sa
                        // PROPRE réussite, pas une stratégie qui s'en passe.
                        let tueur_a_fini = self
                            .players
                            .iter()
                            .any(|p| p.id == killer_id && p.has_finished);
                        if tueur_a_fini {
                            if let Some(killer_player) =
                                self.players.iter_mut().find(|p| p.id == killer_id)
                            {
                                killer_player.trap_points += 1.0;
                            }
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

    /// Le classement, du meilleur au moins bon.
    ///
    /// À égalité de points, l'ordre est arrêté par l'identité du joueur. Sans ce départage, deux
    /// ex æquo peuvent permuter d'une image à l'autre selon l'humeur du tri — et le tableau
    /// scintille sous les yeux de la classe pendant les dix secondes où tout le monde le lit.
    pub fn classement(&self) -> Vec<&PlayerSession> {
        let mut ordre: Vec<&PlayerSession> = self.players.iter().collect();
        ordre.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.id.cmp(&b.id))
        });
        ordre
    }

    /// Le temps restant dans la phase en cours, sa durée totale, et ce qu'on y fait.
    ///
    /// Quatre phases, quatre minuteurs distincts (`draft_timer`, `placement_timer`,
    /// `running_timer`, `leaderboard_timer`) : les exposer un par un obligerait l'affichage à
    /// refaire ce choix à chaque endroit, et à se tromper le jour où une phase s'ajoute. Une
    /// seule question suffit.
    ///
    /// Ces minuteurs décomptaient déjà correctement — **rien ne les affichait**, donc personne
    /// dans la partie ne savait combien de temps il lui restait pour choisir ou pour poser.
    pub fn minuteur_de_phase(&self) -> (f32, f32, &'static str) {
        match self.phase {
            GamePhase::Drafting => (self.draft_timer, DUREE_DRAFT, "CHOISIS TON OBJET"),
            GamePhase::Placement => (self.placement_timer, DUREE_PLACEMENT, "POSE TON OBJET"),
            GamePhase::Running => (self.running_timer, DUREE_COURSE, "COURS"),
            GamePhase::Leaderboard => (self.leaderboard_timer, DUREE_LEADERBOARD, "SCORES"),
        }
    }

    pub fn update(&mut self, dt: f32, input: &InputState) {
        self.round_timer += dt;

        self.traps.update(dt);
        self.particles.update(dt);

        // ─── 1. Phase de Draft (10s max) ─────────────────────────────────────────
        if self.phase == GamePhase::Drafting {
            let avant = self.draft_timer;
            self.draft_timer = (self.draft_timer - dt).max(0.0);

            // Le carton s'ouvre : une gerbe, une seule fois, à l'instant du franchissement.
            // On compare les deux côtés du seuil plutôt que de retenir un drapeau — un drapeau
            // demanderait d'être remis à zéro, et c'est exactement l'oubli qui empêchait
            // l'animation de rejouer aux manches suivantes.
            let seuil = DUREE_DRAFT - CardboardBoxObject::DUREE_SECOUSSE;
            if avant > seuil && self.draft_timer <= seuil {
                self.particles.spawn_box_open_burst(CardboardBoxObject::position(
                    self.grid.width as f32,
                    self.grid.height as f32,
                ));
            }
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
                self.placement_timer = DUREE_PLACEMENT;
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
                self.running_timer = DUREE_COURSE;
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

                        // Détection de Mort — DEUX causes, et il en manquait une.
                        //
                        // ⚠ Jusqu'au 17 août 2026, seule la chute dans le vide était lue ici. Les
                        // pièges tuaient bel et bien (ragdoll à l'écran), mais la manche ne le voyait
                        // pas : `is_dead` restait faux et `killed_by_owner_id` restait `None`, donc
                        // `trap_points` valait TOUJOURS 0. Les tests étaient verts parce qu'ils
                        // écrivaient ces champs à la main.
                        let void_y = self.grid.get_void_kill_y();
                        if player_pos.y < void_y {
                            self.players[i].is_dead = true;
                            self.players[i].killed_by_owner_id = None; // le vide n'appartient à personne
                            log::info!("💀 {} est tombé dans le vide !", self.players[i].name);
                        } else if self.players[i].player.state == crate::player::PlayerState::Dead {
                            self.players[i].is_dead = true;
                            self.players[i].killed_by_owner_id = self.players[i].player.killed_by;
                            match self.players[i].killed_by_owner_id {
                                Some(k) => log::info!(
                                    "💀 {} est mort du piège de {} !",
                                    self.players[i].name, k
                                ),
                                None => log::info!(
                                    "💀 {} est mort d'un danger du terrain !",
                                    self.players[i].name
                                ),
                            }
                        }
                    }
                }
            }

            let all_done = self.players.iter().all(|p| p.has_finished || p.is_dead);

            if all_done || self.running_timer <= 0.0 {
                self.evaluate_round_scores();
                self.phase = GamePhase::Leaderboard;
                self.leaderboard_timer = DUREE_LEADERBOARD;
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
                self.draft_timer = DUREE_DRAFT;
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

    // ──────────────────────────────────────────────────────────────────────────────────────────
    // Tests ajoutés le 17 août 2026, avec le barème complet.
    // ──────────────────────────────────────────────────────────────────────────────────────────

    /// ⭐ LE TÉMOIN POSITIF QUI MANQUAIT — et son absence a laissé un tiers du barème mort.
    ///
    /// Tous les tests de points de piège écrivaient `is_dead` et `killed_by_owner_id` **à la main**.
    /// Ils prouvaient donc le calcul, jamais la chaîne : un piège tue-t-il vraiment, et la manche
    /// s'en aperçoit-elle ? Elle ne s'en apercevait pas — `player.rs` réduisait l'identité du tueur
    /// à un booléen, et la manche ne regardait que la chute dans le vide. `trap_points` valait 0
    /// depuis toujours, avec une suite verte.
    ///
    /// Ce test ne touche aucun champ de score : il pose un vrai piège, laisse tourner la vraie
    /// boucle de jeu, et regarde ce que la manche en conclut.
    #[test]
    fn un_piege_tue_pour_de_vrai_et_la_manche_le_voit() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "Poseur", Vec2::new(2.0, 5.0), true),
            PlayerSession::new(1, "Victime", Vec2::new(8.0, 5.0), false),
        ];
        game.phase = GamePhase::Running;
        game.is_play_mode = true;

        // Le piège de P0, posé exactement sur le centre du corps de P1.
        let centre_victime =
            game.players[1].player.position + Vec2::new(0.0, game.players[1].player.size.y * 0.5);
        game.traps
            .add_trap(centre_victime, crate::traps::TrapKind::SpikeTrap, 0);

        assert!(!game.players[1].is_dead, "la victime doit être vivante avant");

        game.update(1.0 / 60.0, &InputState::default());

        assert!(game.players[1].is_dead, "le piège doit tuer pour de vrai");
        assert_eq!(
            game.players[1].killed_by_owner_id,
            Some(0),
            "la manche doit savoir QUI a posé le piège — c'est exactement l'information qui était jetée"
        );
    }

    /// Le vide n'appartient à personne : mourir en tombant ne doit enrichir aucun joueur.
    #[test]
    fn tomber_dans_le_vide_ne_designe_aucun_tueur() {
        let mut game = PartyGame::new(32, 18);
        let sous_la_carte = game.grid.get_void_kill_y() - 1.0;
        game.players = vec![PlayerSession::new(0, "Tombeur", Vec2::new(5.0, sous_la_carte), true)];
        game.phase = GamePhase::Running;
        game.is_play_mode = true;
        game.players[0].player.position = Vec2::new(5.0, sous_la_carte);

        game.update(1.0 / 60.0, &InputState::default());

        assert!(game.players[0].is_dead);
        assert_eq!(game.players[0].killed_by_owner_id, None);
    }

    /// Être SEUL à réussir vaut 4 points — plus qu'être premier parmi plusieurs (3).
    /// C'est le maximum du jeu : le parcours a arrêté tout le monde sauf un.
    #[test]
    fn seul_a_reussir_vaut_plus_que_premier_parmi_plusieurs() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "P1", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
            PlayerSession::new(2, "P3", Vec2::ZERO, false),
            PlayerSession::new(3, "P4", Vec2::ZERO, false),
        ];

        // Un SEUL arrive (1/4 = 25 %) -> 4 points.
        game.players[0].has_finished = true;
        game.players[0].finish_rank = Some(1);
        game.evaluate_round_scores();
        assert_eq!(game.players[0].win_points, 4.0, "seul rescapé -> 4");

        // Deux arrivent (2/4 = 50 %) -> le premier retombe à 3, le second a 1.
        for p in &mut game.players {
            p.win_points = 0.0;
        }
        game.players[1].has_finished = true;
        game.players[1].finish_rank = Some(2);
        game.evaluate_round_scores();
        assert_eq!(game.players[0].win_points, 3.0, "premier parmi plusieurs -> 3");
        assert_eq!(game.players[1].win_points, 1.0, "autre arrivé -> 1");
    }

    /// La manche est nulle aux DEUX bornes — et « nulle » vaut aussi pour les pièges.
    ///
    /// Avant le 17 août, une manche où plus de 50 % arrivaient annulait les points d'arrivée mais
    /// **payait quand même les tueurs**. L'asymétrie n'avait pas été décidée : elle venait d'une
    /// condition qui demandait « quelqu'un est-il arrivé ? » au lieu de « la manche a-t-elle payé ? ».
    #[test]
    fn une_manche_trop_facile_ne_paie_personne_pieges_compris() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "Tueur", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
            PlayerSession::new(2, "P3", Vec2::ZERO, false),
            PlayerSession::new(3, "Mort", Vec2::ZERO, false),
        ];

        // 3 joueurs sur 4 finissent (75 % > 50 %) : la manche ne désigne personne.
        for (rang, i) in [(1, 0), (2, 1), (3, 2)] {
            game.players[i].has_finished = true;
            game.players[i].finish_rank = Some(rang);
        }
        // Et le tueur, qui a fini lui aussi, a tué le quatrième.
        game.players[3].is_dead = true;
        game.players[3].killed_by_owner_id = Some(0);

        game.evaluate_round_scores();

        assert_eq!(game.players[0].win_points, 0.0, "aucun point d'arrivée au-delà de 50 %");
        assert_eq!(
            game.players[0].trap_points, 0.0,
            "et aucun point de piège non plus : une manche nulle ne paie personne"
        );
    }

    /// Le campeur ne marque plus : on n'encaisse ses kills que si on a franchi la ligne soi-même.
    ///
    /// Sans cette règle, poser un piège et attendre rapporte plus qu'arriver premier — quatre kills
    /// valent 4 points contre 3, pour une pénalité de 0,5. Le piège redevient un multiplicateur de
    /// sa propre réussite, pas une stratégie qui s'en passe.
    #[test]
    fn le_campeur_qui_ne_finit_jamais_ne_touche_pas_ses_kills() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "Campeur", Vec2::ZERO, true),
            PlayerSession::new(1, "Coureur", Vec2::ZERO, false),
            PlayerSession::new(2, "Victime", Vec2::ZERO, false),
            PlayerSession::new(3, "Figurant", Vec2::ZERO, false),
        ];

        // Le coureur arrive (1/4 = 25 %) : la manche paie.
        game.players[1].has_finished = true;
        game.players[1].finish_rank = Some(1);
        // Le campeur, lui, n'a pas bougé — mais son piège a tué quelqu'un.
        game.players[2].is_dead = true;
        game.players[2].killed_by_owner_id = Some(0);

        game.evaluate_round_scores();

        assert_eq!(
            game.players[0].trap_points, 0.0,
            "le campeur n'a pas fini : son kill ne lui rapporte rien"
        );
        assert_eq!(game.players[1].win_points, 4.0, "le coureur est seul rescapé -> 4");

        // Même manche, mais le tueur a fini : là, le kill paie.
        let mut game2 = PartyGame::new(32, 18);
        game2.players = vec![
            PlayerSession::new(0, "Tueur arrivé", Vec2::ZERO, true),
            PlayerSession::new(1, "Victime", Vec2::ZERO, false),
            PlayerSession::new(2, "Figurant", Vec2::ZERO, false),
            PlayerSession::new(3, "Figurant2", Vec2::ZERO, false),
        ];
        game2.players[0].has_finished = true;
        game2.players[0].finish_rank = Some(1);
        game2.players[1].is_dead = true;
        game2.players[1].killed_by_owner_id = Some(0);

        game2.evaluate_round_scores();
        assert_eq!(game2.players[0].trap_points, 1.0, "le tueur qui a fini encaisse son kill");
    }

    /// L'autokill reste puni en toute circonstance : c'est une punition, pas une récompense, donc
    /// elle ne dépend pas de ce que la manche paie.
    #[test]
    fn l_autokill_coute_un_point_meme_dans_une_manche_nulle() {
        let mut game = PartyGame::new(32, 18);
        game.players = vec![
            PlayerSession::new(0, "Maladroit", Vec2::ZERO, true),
            PlayerSession::new(1, "P2", Vec2::ZERO, false),
        ];
        // Personne n'arrive : la manche ne paie rien.
        game.players[0].is_dead = true;
        game.players[0].killed_by_owner_id = Some(0);

        game.evaluate_round_scores();
        assert_eq!(game.players[0].trap_points, -1.0);
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

    #[test]
    fn le_classement_va_du_meilleur_au_moins_bon_et_ne_scintille_pas() {
        let mut game = PartyGame::new(40, 22);
        game.players.clear();
        for (id, (nom, score)) in [("A", 3.0), ("B", 9.0), ("C", 3.0), ("D", 12.0)]
            .into_iter()
            .enumerate()
        {
            let mut p = PlayerSession::new(id as u32, nom, Vec2::new(0.0, 0.0), id == 0);
            p.total_score = score;
            game.players.push(p);
        }

        let noms: Vec<&str> = game.classement().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(noms, vec!["D", "B", "A", "C"], "du plus grand score au plus petit");

        // Deux ex aequo (A et C, 3 points) doivent garder le MEME ordre a chaque appel : c'est
        // ce qui empeche le tableau de permuter sous les yeux des joueurs pendant 10 secondes.
        for _ in 0..20 {
            let encore: Vec<&str> = game.classement().iter().map(|p| p.name.as_str()).collect();
            assert_eq!(encore, noms, "le classement doit etre stable d'un appel a l'autre");
        }
    }

    /// Fait tourner la phase de choix en entier et rend (nombre d'emissions, position de la
    /// premiere gerbe vue). On COMPTE les particules reellement presentes plutot que de
    /// consulter un drapeau : c'est justement un drapeau qui mentait avant.
    fn joue_la_phase_de_choix(game: &mut PartyGame) -> (usize, Option<aegis_engine::math::Vec3>) {
        let rien = InputState::default();
        let dt = 1.0 / 60.0;
        let mut emissions = 0;
        let mut precedent = game.particles.particles.len();
        let mut ou = None;

        for _ in 0..(DUREE_DRAFT / dt) as usize + 5 {
            if game.phase != GamePhase::Drafting {
                break;
            }
            game.update(dt, &rien);
            let maintenant = game.particles.particles.len();
            if maintenant > precedent {
                emissions += 1;
                if ou.is_none() {
                    ou = Some(game.particles.particles[0].pos);
                }
            }
            precedent = maintenant;
        }
        (emissions, ou)
    }

    #[test]
    fn le_carton_lance_sa_gerbe_a_chaque_manche_et_une_seule_fois() {
        // Le temoin de la reparation. La gerbe partait dans un `clone()` jete a la ligne
        // suivante : elle n'arrivait nulle part. Et l'animation ne rejouait jamais, faute d'un
        // `reset_animation()` que personne n'appelait.
        let mut game = PartyGame::new(40, 22);
        assert_eq!(game.phase, GamePhase::Drafting);
        assert_eq!(game.particles.particles.len(), 0, "rien avant l'ouverture");

        let (emissions, ou) = joue_la_phase_de_choix(&mut game);
        assert_eq!(emissions, 1, "une gerbe, et une seule, pour la manche 1");

        // Elle doit viser le carton, pas l'origine du monde.
        let cible = CardboardBoxObject::position(game.grid.width as f32, game.grid.height as f32);
        let une = ou.expect("aucune particule emise");
        assert!(
            (une.x - cible.x).abs() < 3.0 && (une.z - cible.z).abs() < 3.0,
            "la gerbe part de {une:?} au lieu du carton {cible:?}"
        );

        // ─── Et surtout : la manche SUIVANTE. C'est tout l'objet du correctif — l'animation
        // ne se declenchait qu'au lancement du jeu, donc une seule fois de la soiree.
        game.phase = GamePhase::Leaderboard;
        game.leaderboard_timer = 0.0;
        game.update(1.0 / 60.0, &InputState::default());
        assert_eq!(game.phase, GamePhase::Drafting, "on doit etre reparti pour une manche");
        assert_eq!(game.round_number, 2);

        let (encore, _) = joue_la_phase_de_choix(&mut game);
        assert_eq!(encore, 1, "le carton doit REJOUER son ouverture a la manche 2");
    }

    #[test]
    fn tous_les_intitules_de_phase_tiennent_sur_l_ecran_le_plus_etroit() {
        // Le HUD tournera sur 35 ecrans de formats inconnus. Le plus etroit envisageable est le
        // carre (aspect 1.0) : au-dela, le bandeau sortirait des bords, et personne ne s'en
        // apercevrait avant la partie.
        let mut game = PartyGame::new(40, 22);
        for phase in [
            GamePhase::Drafting,
            GamePhase::Placement,
            GamePhase::Running,
            GamePhase::Leaderboard,
        ] {
            game.phase = phase;
            let (_, _, intitule) = game.minuteur_de_phase();
            let largeur = crate::hud::largeur_bandeau_minuteur(intitule);
            assert!(
                largeur < 1.0,
                "{phase:?} : « {intitule} » occupe {largeur:.3} de hauteur d'ecran, il deborderait d'un ecran carre"
            );
        }
    }
}
