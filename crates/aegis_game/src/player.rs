use aegis_engine::math::{Vec2, Vec3, Vec4};
use crate::grid::TileGrid;
use crate::traps::TrapManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    OnGround,
    InAir,
    WallSliding { left_wall: bool },
    Dead,
    Finished,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    pub jump_pressed_this_frame: bool,
    pub crouch: bool,
}

#[derive(Debug, Clone)]
pub struct RagdollLimb {
    pub pos: Vec3,
    pub velocity: Vec3,
    pub rotation: Vec3,
    pub angular_velocity: Vec3,
    pub scale: Vec3,
    pub color: Vec4,
}

#[derive(Debug, Clone, Default)]
pub struct Ragdoll {
    pub active: bool,
    pub timer: f32,
    pub limbs: Vec<RagdollLimb>,
}

impl Ragdoll {
    pub fn trigger(&mut self, origin: Vec2, impact_vel: Vec2) {
        self.active = true;
        self.timer = 0.0;
        self.limbs.clear();

        let base_pos = Vec3::new(origin.x, origin.y, 0.2);

        // 6 Membres pour le Ragdoll de mort dynamique : Tête, Torse, Bras G, Bras D, Jambe G, Jambe D
        let parts = [
            (Vec3::new(0.0, 1.32, 0.0), Vec3::new(0.52, 0.42, 0.48), Vec4::new(0.96, 0.96, 0.98, 1.0), Vec3::new(-2.0, 7.5, 3.0)),  // Head
            (Vec3::new(0.0, 0.80, 0.0), Vec3::new(0.60, 0.65, 0.48), Vec4::new(0.20, 0.65, 0.95, 1.0), Vec3::new(1.0, 6.0, -2.0)),  // Torso
            (Vec3::new(-0.35, 0.80, 0.0), Vec3::new(0.18, 0.50, 0.22), Vec4::new(0.15, 0.18, 0.25, 1.0), Vec3::new(-4.0, 8.0, 5.0)),// Left Arm
            (Vec3::new(0.35, 0.80, 0.0), Vec3::new(0.18, 0.50, 0.22), Vec4::new(0.15, 0.18, 0.25, 1.0), Vec3::new(4.0, 8.5, -4.0)), // Right Arm
            (Vec3::new(-0.15, 0.25, 0.0), Vec3::new(0.22, 0.48, 0.25), Vec4::new(0.12, 0.15, 0.28, 1.0), Vec3::new(-3.0, 5.0, 2.0)),// Left Leg
            (Vec3::new(0.15, 0.25, 0.0), Vec3::new(0.22, 0.48, 0.25), Vec4::new(0.12, 0.15, 0.28, 1.0), Vec3::new(3.0, 5.5, -3.0)), // Right Leg
        ];

        for (rel_pos, scale, color, pop_vel) in parts {
            let limb_vel = Vec3::new(impact_vel.x * 0.4, impact_vel.y * 0.4, 0.0) + pop_vel;
            let ang_vel = Vec3::new(pop_vel.y * 1.5, pop_vel.x * 2.0, pop_vel.z * 1.8);
            self.limbs.push(RagdollLimb {
                pos: base_pos + rel_pos,
                velocity: limb_vel,
                rotation: Vec3::ZERO,
                angular_velocity: ang_vel,
                scale,
                color,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        if !self.active { return; }
        self.timer += dt;
        let gravity = Vec3::new(0.0, -25.0, 0.0);

        for limb in &mut self.limbs {
            limb.pos += limb.velocity * dt;
            limb.velocity += gravity * dt;
            limb.rotation += limb.angular_velocity * dt;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    pub position: Vec2, // Bottom-center of character
    pub velocity: Vec2,
    pub size: Vec2,     // Width: 0.8, Height: 1.75
    pub state: PlayerState,
    pub facing_right: bool,
    pub coyote_timer: f32,
    pub jump_buffer: f32,
    pub wall_cooldown: f32,
    pub tilt_angle: f32,
    pub landing_timer: f32,
    pub landing_duration: f32,
    pub landing_intensity: f32,
    pub ceiling_bump_timer: f32,
    pub ceiling_bump_intensity: f32,
    pub highest_air_y: f32,
    pub anim_arm_front: f32,
    pub anim_arm_back: f32,
    pub anim_leg_front: f32,
    pub anim_leg_back: f32,
    pub particles: crate::particles::ParticleEffectManager,
    pub prev_vel_x: f32,
    pub stored_fall_momentum: f32,
    pub boost_window_timer: f32,
    pub ragdoll: Ragdoll,
}

impl Player {
    pub const WIDTH: f32 = 0.80;
    pub const HEIGHT: f32 = 1.75;
    pub const RUN_ACCEL: f32 = 42.0;
    pub const MAX_RUN_SPEED: f32 = 8.5;
    pub const FRICTION: f32 = 36.0;
    pub const GRAVITY: f32 = 32.0;
    pub const JUMP_VELOCITY: f32 = 13.5;
    pub const WALL_SLIDE_SPEED: f32 = 3.5;
    pub const WALL_KICK_SPEED: f32 = 9.0;

    pub fn new(spawn_pos: Vec2) -> Self {
        Self {
            position: spawn_pos,
            velocity: Vec2::ZERO,
            size: Vec2::new(Self::WIDTH, Self::HEIGHT),
            state: PlayerState::OnGround,
            facing_right: true,
            coyote_timer: 0.0,
            jump_buffer: 0.0,
            wall_cooldown: 0.0,
            tilt_angle: 0.0,
            landing_timer: 0.0,
            landing_duration: 0.20,
            landing_intensity: 0.0,
            ceiling_bump_timer: 0.0,
            ceiling_bump_intensity: 0.0,
            highest_air_y: spawn_pos.y,
            anim_arm_front: 0.0,
            anim_arm_back: 0.0,
            anim_leg_front: 0.0,
            anim_leg_back: 0.0,
            particles: crate::particles::ParticleEffectManager::new(),
            prev_vel_x: 0.0,
            stored_fall_momentum: 0.0,
            boost_window_timer: 0.0,
            ragdoll: Ragdoll::default(),
        }
    }

    pub fn reset(&mut self, start_pos: Vec2) {
        self.position = start_pos;
        self.velocity = Vec2::ZERO;
        self.state = PlayerState::InAir;
        self.coyote_timer = 0.0;
        self.jump_buffer = 0.0;
        self.wall_cooldown = 0.0;
        self.tilt_angle = 0.0;
        self.ragdoll.active = false;
    }

    pub fn update(&mut self, dt: f32, input: &InputState, grid: &TileGrid, traps: &TrapManager) {
        if self.state == PlayerState::Dead {
            self.ragdoll.update(dt);
            return;
        }

        if self.state == PlayerState::Finished {
            return;
        }

        // 1. Timers & État initial
        let was_wall_sliding = matches!(self.state, PlayerState::WallSliding { .. });
        let left_wall_saved = if let PlayerState::WallSliding { left_wall } = self.state { left_wall } else { false };

        if self.state == PlayerState::OnGround {
            self.coyote_timer = 0.15;
            self.tilt_angle += (0.0 - self.tilt_angle) * (15.0 * dt).min(1.0);
        } else if self.coyote_timer > 0.0 {
            self.coyote_timer -= dt;
        }

        if input.jump_pressed_this_frame || input.jump {
            self.jump_buffer = 0.15;
        } else if self.jump_buffer > 0.0 {
            self.jump_buffer -= dt;
        }

        if self.wall_cooldown > 0.0 {
            self.wall_cooldown -= dt;
        }

        if self.landing_timer > 0.0 {
            self.landing_timer = (self.landing_timer - dt).max(0.0);
        }

        if self.ceiling_bump_timer > 0.0 {
            self.ceiling_bump_timer = (self.ceiling_bump_timer - dt).max(0.0);
        }

        // 2. Acceleration Horizontale
        let mut target_dir = 0.0;
        if input.left {
            target_dir -= 1.0;
            self.facing_right = false;
        }
        if input.right {
            target_dir += 1.0;
            self.facing_right = true;
        }

        if target_dir != 0.0 {
            self.velocity.x += target_dir * Self::RUN_ACCEL * dt;
            self.velocity.x = self.velocity.x.clamp(-Self::MAX_RUN_SPEED, Self::MAX_RUN_SPEED);
        } else if self.state == PlayerState::OnGround {
            if self.velocity.x > 0.0 {
                self.velocity.x = (self.velocity.x - Self::FRICTION * dt).max(0.0);
            } else if self.velocity.x < 0.0 {
                self.velocity.x = (self.velocity.x + Self::FRICTION * dt).min(0.0);
            }
        }

        if self.boost_window_timer > 0.0 {
            self.boost_window_timer = (self.boost_window_timer - dt).max(0.0);
        }

        // 3. Wall-Slide Detection & Capture de la Vélocité de Chute
        let entry_fall_speed = (-self.velocity.y).max(0.0);
        let checking_left = grid.check_solid_collision(self.position + Vec2::new(-0.06, 0.1), Vec2::new(self.size.x, self.size.y - 0.2));
        let checking_right = grid.check_solid_collision(self.position + Vec2::new(0.06, 0.1), Vec2::new(self.size.x, self.size.y - 0.2));

        let pushing_into_left = checking_left && input.left;
        let pushing_into_right = checking_right && input.right;

        let can_wall_slide = (pushing_into_left || pushing_into_right) 
            && self.state != PlayerState::OnGround 
            && self.velocity.y < 0.5 
            && self.wall_cooldown <= 0.0;

        if can_wall_slide {
            let left_wall = pushing_into_left;
            if !matches!(self.state, PlayerState::WallSliding { .. }) {
                // Instamment à l'accroche murale : capture de l'élan de chute accumulé !
                if entry_fall_speed > 7.5 {
                    self.stored_fall_momentum = (entry_fall_speed - 5.5).clamp(0.0, 24.0);
                    self.boost_window_timer = 0.38; // Fenêtre de saut boosté ouverte pendant 0.38s !
                }
            }
            self.state = PlayerState::WallSliding { left_wall };
            self.velocity.y = self.velocity.y.max(-Self::WALL_SLIDE_SPEED);

            let target_tilt = if left_wall { 0.20 } else { -0.20 };
            self.tilt_angle += (target_tilt - self.tilt_angle) * (15.0 * dt).min(1.0);
        } else {
            if matches!(self.state, PlayerState::WallSliding { .. }) {
                self.state = PlayerState::InAir;
            }
            self.tilt_angle += (0.0 - self.tilt_angle) * (12.0 * dt).min(1.0);
        }

        // 4. Gravity
        self.velocity.y -= Self::GRAVITY * dt;

        // 5. Jump Execution (Avec Mécanique de BOOST WALL JUMP)
        if self.jump_buffer > 0.0 {
            if self.state == PlayerState::OnGround || self.coyote_timer > 0.0 {
                self.velocity.y = Self::JUMP_VELOCITY;
                self.coyote_timer = 0.0;
                self.jump_buffer = 0.0;
                self.state = PlayerState::InAir;
            } else if was_wall_sliding {
                let push_away_dir = if left_wall_saved { 1.0 } else { -1.0 };
                
                // Calcul du BOOST WALL JUMP basé sur la vélocité de chute accumulée !
                let is_boosted = self.boost_window_timer > 0.0 && self.stored_fall_momentum > 2.0;
                let (kick_speed, jump_vel, intensity) = if is_boosted {
                    let boost_factor = (self.stored_fall_momentum / 18.0).clamp(0.20, 1.20);
                    let k = Self::WALL_KICK_SPEED + boost_factor * 12.0; // Propulsé bien plus loin dans la direction inverse (jusqu'à 21.0 !)
                    let j = Self::JUMP_VELOCITY + boost_factor * 7.5;   // Propulsé bien plus haut ! (jusqu'à 21.0 !)
                    (k, j, boost_factor)
                } else {
                    (Self::WALL_KICK_SPEED, Self::JUMP_VELOCITY, 0.0)
                };

                self.velocity.x = push_away_dir * kick_speed;
                self.velocity.y = jump_vel;
                self.facing_right = left_wall_saved;
                
                if is_boosted {
                    let wall_x = if left_wall_saved { self.position.x - 0.40 } else { self.position.x + 0.40 };
                    self.particles.spawn_boost_wall_jump_burst(wall_x, self.position.y + 0.5, push_away_dir, intensity);
                    self.tilt_angle = push_away_dir * -0.45; // Inclinaison dynamique très prononcée vers la trajectoire !
                }

                self.stored_fall_momentum = 0.0;
                self.boost_window_timer = 0.0;
                self.jump_buffer = 0.0;
                self.wall_cooldown = 0.12;
                self.state = PlayerState::InAir;
            }
        }

        // 6. Movement Integration
        self.move_and_slide(dt, grid);

        // 7. Hazards & Finish Flag (Déclenchement du Ragdoll de mort)
        if grid.check_hazard_collision(self.position, self.size) || traps.check_player_death(self.position, self.size).is_some() {
            if self.state != PlayerState::Dead {
                self.state = PlayerState::Dead;
                self.ragdoll.trigger(self.position, self.velocity);
            }
            return;
        }

        // 6. Animation Procédurale Lissée (Membres & Articulations Choupi)
        let is_wall_sliding = matches!(self.state, PlayerState::WallSliding { .. });
        let is_running = self.state == PlayerState::OnGround && self.velocity.x.abs() > 0.3;
        let is_in_air = self.state == PlayerState::InAir;
        let vy = self.velocity.y;
        let is_rising = is_in_air && vy > 0.1;
        let is_falling = is_in_air && vy <= 0.1;

        let landing_squash = if self.landing_timer > 0.0 && self.landing_duration > 0.0 {
            let progress = (1.0 - self.landing_timer / self.landing_duration).clamp(0.0, 1.0);
            (progress * std::f32::consts::PI).sin() * (1.0 - progress * 0.5) * self.landing_intensity
        } else {
            0.0
        };

        let (target_arm_f, target_arm_b, target_leg_f, target_leg_b) = if self.ceiling_bump_timer > 0.0 {
            // Posture rigolote "BONK !" : les bras s'écartent en arrière de surprise !
            (-1.10, 1.10, -0.30, 0.30)
        } else if let PlayerState::WallSliding { left_wall } = self.state {
            let wall_dir = if left_wall { -1.0 } else { 1.0 };
            let facing_sign = if self.facing_right { 1.0 } else { -1.0 };
            let wall_facing = wall_dir * facing_sign; // Les bras et jambes vont EXACTEMENT sur la paroi du mur !
            (wall_facing * 1.30, wall_facing * 0.95, wall_facing * 0.80, wall_facing * 0.45)
        } else if is_rising {
            (0.95, 0.70, 0.55, -0.45)
        } else if is_falling {
            (0.40, -0.30, -0.25, 0.35)
        } else if landing_squash > 0.01 {
            // Accroupissement prononcé à l'impact (Deep landing crouch)
            (0.55 * landing_squash, -0.55 * landing_squash, 0.70 * landing_squash, -0.70 * landing_squash)
        } else if is_running {
            let step_phase = self.position.x * 4.0;
            let run_swing = -step_phase.sin() * 0.50;
            let run_leg = step_phase.sin() * 0.45;
            (run_swing, -run_swing, run_leg, -run_leg)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let lerp_factor = (22.0 * dt).min(1.0);
        self.anim_arm_front += (target_arm_f - self.anim_arm_front) * lerp_factor;
        self.anim_arm_back += (target_arm_b - self.anim_arm_back) * lerp_factor;
        self.anim_leg_front += (target_leg_f - self.anim_leg_front) * lerp_factor;
        self.anim_leg_back += (target_leg_b - self.anim_leg_back) * lerp_factor;

        // 7. Moteur de Particules Procédurales Dynamiques
        self.particles.update(dt);

        // 7a. Poussière Légère de Course
        if self.state == PlayerState::OnGround && self.velocity.x.abs() > 1.8 {
            self.particles.run_spawn_timer += dt;
            if self.particles.run_spawn_timer >= 0.07 {
                self.particles.run_spawn_timer = 0.0;
                self.particles.spawn_running_dust(self.position, self.facing_right);
            }

            // 7b. Traînée Généreuse de Cailloux & Poussière au Changement de Direction (Skid / Dérapage)
            let skid_threshold = 2.5;
            if (self.prev_vel_x > skid_threshold && input.left) || (self.prev_vel_x < -skid_threshold && input.right) {
                self.particles.spawn_skid_gravel(self.position, self.prev_vel_x);
            }
        }
        self.prev_vel_x = self.velocity.x;

        // 7c. Glissement Murale Procédural Riche & Satisfaisant sur TOUS LES BLOCS SOLIDES
        let check_left = grid.check_solid_collision(self.position + Vec2::new(-0.06, 0.2), Vec2::new(self.size.x, self.size.y - 0.4));
        let check_right = grid.check_solid_collision(self.position + Vec2::new(0.06, 0.2), Vec2::new(self.size.x, self.size.y - 0.4));
        let is_sliding = matches!(self.state, PlayerState::WallSliding { .. }) || (self.state == PlayerState::InAir && self.velocity.y < 0.0 && (check_left || check_right));
        if is_sliding {
            let left_wall = check_left || matches!(self.state, PlayerState::WallSliding { left_wall: true });
            let wall_x = if left_wall { self.position.x - 0.40 } else { self.position.x + 0.40 };
            self.particles.spawn_wall_slide_sparks(wall_x, self.position.y + 0.5, left_wall);
        }

        // 7d. Anneau d'Impact d'Atterrissage Brutal (Landing Impact Ring)
        if (self.landing_duration - self.landing_timer).abs() < 0.02 && self.landing_intensity > 0.15 {
            self.particles.spawn_landing_impact_ring(self.position, self.landing_intensity);
        }

        // Objectif d'Arrivée
        if grid.get_tile(self.position.x.floor() as i32, self.position.y.floor() as i32) == crate::grid::TileType::FinishFlag {
            self.state = PlayerState::Finished;
            log::info!("🎉 VICTOIRE ! Niveau terminé avec succès !");
        }
    }

    fn move_and_slide(&mut self, dt: f32, grid: &TileGrid) {
        // Horizontal Movement
        let delta_x = self.velocity.x * dt;
        if delta_x != 0.0 {
            let next_pos = self.position + Vec2::new(delta_x, 0.0);
            if !grid.check_solid_collision(next_pos, self.size) {
                self.position.x = next_pos.x;
            } else {
                self.velocity.x = 0.0;
            }
        }

        // Vertical Movement
        let delta_y = self.velocity.y * dt;
        if delta_y != 0.0 {
            let next_pos = self.position + Vec2::new(0.0, delta_y);
            if !grid.check_solid_collision(next_pos, self.size) {
                self.position.y = next_pos.y;
                if !matches!(self.state, PlayerState::WallSliding { .. }) {
                    self.state = PlayerState::InAir;
                    self.highest_air_y = self.highest_air_y.max(self.position.y);
                }
            } else {
                if self.velocity.y > 0.0 {
                    // BONK ! Choc de la tête contre le plafond !
                    self.ceiling_bump_timer = 0.28;
                    self.ceiling_bump_intensity = (self.velocity.y / 14.0).clamp(0.25, 0.65);
                } else if self.velocity.y < 0.0 {
                    if self.state == PlayerState::InAir {
                        let fall_height = (self.highest_air_y - self.position.y).max(0.0);
                        let speed_factor = -self.velocity.y / 24.0;
                        let impact_power = (speed_factor * 0.6 + fall_height / 10.0 * 0.4).clamp(0.15, 0.75);

                        self.landing_duration = (0.18 + impact_power * 0.32).clamp(0.18, 0.48);
                        self.landing_timer = self.landing_duration;
                        self.landing_intensity = impact_power;
                    }
                    self.state = PlayerState::OnGround;
                    self.highest_air_y = self.position.y;
                }
                self.velocity.y = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::TileType;

    #[test]
    fn test_player_dimensions() {
        let player = Player::new(Vec2::new(5.0, 5.0));
        assert_eq!(player.size.x, Player::WIDTH);
        assert_eq!(player.size.y, Player::HEIGHT);
    }

    #[test]
    fn test_holding_space_wall_jump_fluidity() {
        let mut player = Player::new(Vec2::new(5.0, 5.0));
        let grid = TileGrid::new(32, 18);
        let traps = TrapManager::new();

        player.state = PlayerState::WallSliding { left_wall: true };
        let mut input = InputState::default();
        input.jump = true;

        player.update(0.016, &input, &grid, &traps);

        assert_eq!(player.state, PlayerState::InAir);
        assert!(player.velocity.x > 0.0);
        assert!(player.velocity.y > 0.0);
    }

    #[test]
    fn test_boost_wall_jump_momentum() {
        let mut player = Player::new(Vec2::new(4.4, 5.0));
        let mut grid = TileGrid::new(32, 18);
        grid.set_tile(3, 5, TileType::SolidBlock); // Mur solide [3.0, 4.0] à gauche
        let traps = TrapManager::new();

        // 1. Chute rapide avec accumulation de vélocité
        player.velocity.y = -22.0;
        let mut input = InputState::default();
        input.left = true; // S'accroche au mur gauche

        // Simulation d'accroche murale
        player.state = PlayerState::InAir;
        player.update(0.016, &input, &grid, &traps);

        // La vélocité de chute a été capturée dans le réservoir de boost !
        assert!(player.stored_fall_momentum > 5.0);
        assert!(player.boost_window_timer > 0.0);

        // 2. Exécution du BOOST WALL JUMP (Saut au mur)
        input.jump = true;
        player.update(0.016, &input, &grid, &traps);

        // La vélocité horizontale (direction inverse) et verticale doit être significativement supérieure à un Wall Jump standard !
        assert!(player.velocity.x > Player::WALL_KICK_SPEED);
        assert!(player.velocity.y > Player::JUMP_VELOCITY);
    }
}
