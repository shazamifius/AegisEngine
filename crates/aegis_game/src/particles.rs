use aegis_engine::math::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: Vec3,
    pub color: Vec4,
    pub emissive: f32,
    pub life: f32,
    pub max_life: f32,
}

#[derive(Debug, Clone)]
pub struct ParticleEffectManager {
    pub particles: Vec<Particle>,
    pub run_spawn_timer: f32,
    pub wall_slide_timer: f32,
    pub skid_cooldown: f32,
    pub prev_vel_x: f32,
    seed: u32,
}

impl ParticleEffectManager {
    pub fn new() -> Self {
        Self {
            particles: Vec::with_capacity(256),
            run_spawn_timer: 0.0,
            wall_slide_timer: 0.0,
            skid_cooldown: 0.0,
            prev_vel_x: 0.0,
            seed: 1337,
        }
    }

    fn next_rand(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed as f32) / (u32::MAX as f32)
    }

    pub fn update(&mut self, dt: f32) {
        if self.skid_cooldown > 0.0 {
            self.skid_cooldown = (self.skid_cooldown - dt).max(0.0);
        }

        let gravity = Vec3::new(0.0, -18.0, 0.0);
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.life += dt;
            if p.life >= p.max_life {
                self.particles.swap_remove(i);
            } else {
                p.pos += p.vel * dt;
                p.vel += gravity * dt;
                i += 1;
            }
        }
    }

    // 1. Poussière Légère de Course
    pub fn spawn_running_dust(&mut self, feet_pos: Vec2, facing_right: bool) {
        let dir = if facing_right { -1.0 } else { 1.0 };
        let r1 = self.next_rand();
        let r2 = self.next_rand();

        self.particles.push(Particle {
            pos: Vec3::new(feet_pos.x + dir * 0.15, feet_pos.y + 0.05, 0.2),
            vel: Vec3::new(dir * (0.8 + r1 * 1.2), 0.6 + r2 * 0.8, 0.0),
            size: Vec3::splat(0.07 + r1 * 0.04),
            color: Vec4::new(0.88, 0.88, 0.82, 0.65),
            emissive: 0.0,
            life: 0.0,
            max_life: 0.22 + r2 * 0.12,
        });
    }

    // 2. Traînée Généreuse de Cailloux & Poussière de Dérapage / Changement de Direction
    pub fn spawn_skid_gravel(&mut self, feet_pos: Vec2, old_vel_x: f32) {
        let skid_dir = old_vel_x.signum();
        for _ in 0..10 {
            let r1 = self.next_rand();
            let r2 = self.next_rand();
            let r3 = self.next_rand();

            let is_pebble = r1 > 0.4;
            let (size, color, emissive) = if is_pebble {
                (
                    Vec3::splat(0.06 + r2 * 0.05),
                    Vec4::new(0.48, 0.42, 0.36, 1.0), // Caillou Gris/Marron
                    0.0,
                )
            } else {
                (
                    Vec3::splat(0.10 + r2 * 0.06),
                    Vec4::new(0.92, 0.90, 0.82, 0.75), // Nuage de Poussière
                    0.0,
                )
            };

            self.particles.push(Particle {
                pos: Vec3::new(feet_pos.x + (r2 - 0.5) * 0.3, feet_pos.y + 0.08, 0.2),
                vel: Vec3::new(skid_dir * (2.2 + r2 * 4.5), 1.5 + r3 * 3.0, (r1 - 0.5) * 1.8),
                size,
                color,
                emissive,
                life: 0.0,
                max_life: 0.25 + r3 * 0.20,
            });
        }
    }

    // 3. Glissement Murale Riche & Satisfaisant (Étincelles Or + Poussière sur Tous Blocs)
    pub fn spawn_wall_slide_sparks(&mut self, wall_x: f32, contact_y: f32, left_wall: bool) {
        let wall_normal = if left_wall { 1.0 } else { -1.0 };
        for _ in 0..2 {
            let r1 = self.next_rand();
            let r2 = self.next_rand();
            let r3 = self.next_rand();

            let is_spark = r1 > 0.45;
            let (size, color, emissive) = if is_spark {
                (
                    Vec3::splat(0.05 + r2 * 0.04),
                    Vec4::new(0.98, 0.82, 0.15, 1.0), // Étincelle Or Brillant
                    5.0,
                )
            } else {
                (
                    Vec3::splat(0.08 + r2 * 0.05),
                    Vec4::new(0.60, 0.55, 0.48, 0.8), // Poussière de Roche
                    0.4,
                )
            };

            self.particles.push(Particle {
                pos: Vec3::new(wall_x, contact_y + (r2 - 0.5) * 0.35, 0.22),
                vel: Vec3::new(
                    wall_normal * (1.0 + r2 * 2.0),
                    -0.9 - r3 * 2.5,
                    (r1 - 0.5) * 1.2,
                ),
                size,
                color,
                emissive,
                life: 0.0,
                max_life: 0.22 + r3 * 0.18,
            });
        }
    }

    // 4. Anneau d'Impact d'Atterrissage de Haut (Attérrissage Brutal)
    pub fn spawn_landing_impact_ring(&mut self, feet_pos: Vec2, intensity: f32) {
        let particle_count = (16.0 + intensity * 24.0) as usize;
        for i in 0..particle_count {
            let r1 = self.next_rand();
            let r2 = self.next_rand();
            let dir_x = if i % 2 == 0 { 1.0 } else { -1.0 };

            let is_debris = r1 > 0.4;
            let (size, color, emissive) = if is_debris {
                (
                    Vec3::splat(0.09 + r2 * 0.08),
                    Vec4::new(0.42, 0.38, 0.32, 1.0), // Éclat de Roche
                    0.0,
                )
            } else {
                (
                    Vec3::splat(0.14 + r2 * 0.10),
                    Vec4::new(0.95, 0.92, 0.85, 0.80), // Onde de Choc Poussière
                    0.0,
                )
            };

            let speed_x = dir_x * (3.5 + r1 * 8.5 * intensity);
            let speed_y = 1.0 + r2 * 4.0 * intensity;

            self.particles.push(Particle {
                pos: Vec3::new(feet_pos.x + (r2 - 0.5) * 0.2, feet_pos.y + 0.05, 0.2),
                vel: Vec3::new(speed_x, speed_y, (r1 - 0.5) * 2.5),
                size,
                color,
                emissive,
                life: 0.0,
                max_life: 0.32 + r2 * 0.28,
            });
        }
    }

    // 5. Explosion d'Énergie au Boost Wall Jump (Rebond Mural à Haute Vélocité)
    pub fn spawn_boost_wall_jump_burst(&mut self, wall_x: f32, contact_y: f32, push_away_dir: f32, intensity: f32) {
        let count = (16.0 + intensity * 16.0) as usize;
        for _ in 0..count {
            let r1 = self.next_rand();
            let r2 = self.next_rand();
            let r3 = self.next_rand();

            let speed_x = push_away_dir * (3.5 + r1 * 11.0 * intensity);
            let speed_y = (r2 - 0.3) * 8.0 * intensity;

            self.particles.push(Particle {
                pos: Vec3::new(wall_x, contact_y + (r2 - 0.5) * 0.4, 0.22),
                vel: Vec3::new(speed_x, speed_y, (r3 - 0.5) * 2.0),
                size: Vec3::splat(0.08 + r2 * 0.08),
                color: Vec4::new(0.98, 0.85, 0.15, 1.0), // Or Émissif Énergétique
                emissive: 8.0,
                life: 0.0,
                max_life: 0.25 + r3 * 0.20,
            });
        }
    }
}
