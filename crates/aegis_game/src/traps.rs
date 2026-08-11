use aegis_engine::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub enum TrapKind {
    SawBlade { radius: f32, rotation: f32 },
    CannonTurret { dir: Direction, fire_rate: f32, timer: f32 },
    SpikeTrap,
    LaserEmitter { dir: Direction, active: bool, timer: f32 },
    Flamethrower { dir: Direction, active: bool, timer: f32 },
    MovingPlatform { p1: Vec2, p2: Vec2, speed: f32, t: f32, forward: bool },
}

#[derive(Debug, Clone)]
pub struct TrapInstance {
    pub position: Vec2,
    pub kind: TrapKind,
    pub owner_id: u32,
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub lifetime: f32,
    pub owner_id: u32,
}

pub struct TrapManager {
    pub traps: Vec<TrapInstance>,
    pub projectiles: Vec<Projectile>,
}

impl TrapManager {
    pub fn new() -> Self {
        Self {
            traps: Vec::new(),
            projectiles: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.traps.clear();
        self.projectiles.clear();
    }

    pub fn add_trap(&mut self, pos: Vec2, kind: TrapKind, owner_id: u32) {
        self.traps.push(TrapInstance { position: pos, kind, owner_id });
    }

    pub fn update(&mut self, dt: f32) {
        let mut new_projectiles = Vec::new();

        for trap in self.traps.iter_mut() {
            match &mut trap.kind {
                TrapKind::SawBlade { rotation, .. } => {
                    *rotation += 8.0 * dt;
                }
                TrapKind::CannonTurret { dir, fire_rate, timer } => {
                    *timer += dt;
                    if *timer >= *fire_rate {
                        *timer = 0.0;
                        let vel = match dir {
                            Direction::Up => Vec2::new(0.0, 10.0),
                            Direction::Down => Vec2::new(0.0, -10.0),
                            Direction::Left => Vec2::new(-10.0, 0.0),
                            Direction::Right => Vec2::new(10.0, 0.0),
                        };
                        new_projectiles.push(Projectile {
                            position: trap.position,
                            velocity: vel,
                            radius: 0.35,
                            lifetime: 6.0,
                            owner_id: trap.owner_id,
                        });
                    }
                }
                TrapKind::SpikeTrap => {}
                TrapKind::LaserEmitter { timer, active, .. } => {
                    *timer += dt;
                    if *timer >= 2.0 {
                        *timer = 0.0;
                        *active = !*active;
                    }
                }
                TrapKind::Flamethrower { timer, active, .. } => {
                    *timer += dt;
                    if *timer >= 1.5 {
                        *timer = 0.0;
                        *active = !*active;
                    }
                }
                TrapKind::MovingPlatform { p1, p2, speed, t, forward } => {
                    if *forward {
                        *t += *speed * dt;
                        if *t >= 1.0 {
                            *t = 1.0;
                            *forward = false;
                        }
                    } else {
                        *t -= *speed * dt;
                        if *t <= 0.0 {
                            *t = 0.0;
                            *forward = true;
                        }
                    }
                    trap.position = (*p1).lerp(*p2, *t);
                }
            }
        }

        self.projectiles.extend(new_projectiles);

        // Update existing projectiles
        for p in self.projectiles.iter_mut() {
            p.position += p.velocity * dt;
            p.lifetime -= dt;
        }

        // Remove expired projectiles
        self.projectiles.retain(|p| p.lifetime > 0.0);
    }

    /// Checks if a player box collides with a lethal trap or projectile.
    /// Returns Some(owner_id) if killed by a trap/projectile belonging to `owner_id`.
    pub fn check_player_death(&self, player_pos: Vec2, player_size: Vec2) -> Option<u32> {
        let half_w = player_size.x * 0.5;
        let p_center = player_pos + Vec2::new(0.0, player_size.y * 0.5);

        for trap in &self.traps {
            match &trap.kind {
                TrapKind::SawBlade { radius, .. } => {
                    let dist = trap.position.distance(p_center);
                    if dist < radius + half_w {
                        return Some(trap.owner_id);
                    }
                }
                TrapKind::SpikeTrap => {
                    let dist = trap.position.distance(p_center);
                    if dist < 0.6 + half_w {
                        return Some(trap.owner_id);
                    }
                }
                TrapKind::LaserEmitter { active, dir, .. } => {
                    if *active {
                        let diff = p_center - trap.position;
                        let hit = match dir {
                            Direction::Up => diff.x.abs() < 0.4 && diff.y > 0.0 && diff.y < 8.0,
                            Direction::Down => diff.x.abs() < 0.4 && diff.y < 0.0 && diff.y > -8.0,
                            Direction::Left => diff.y.abs() < 0.4 && diff.x < 0.0 && diff.x > -8.0,
                            Direction::Right => diff.y.abs() < 0.4 && diff.x > 0.0 && diff.x < 8.0,
                        };
                        if hit {
                            return Some(trap.owner_id);
                        }
                    }
                }
                TrapKind::Flamethrower { active, dir, .. } => {
                    if *active {
                        let diff = p_center - trap.position;
                        let hit = match dir {
                            Direction::Up => diff.x.abs() < 0.6 && diff.y > 0.0 && diff.y < 4.0,
                            Direction::Down => diff.x.abs() < 0.6 && diff.y < 0.0 && diff.y > -4.0,
                            Direction::Left => diff.y.abs() < 0.6 && diff.x < 0.0 && diff.x > -4.0,
                            Direction::Right => diff.y.abs() < 0.6 && diff.x > 0.0 && diff.x < 4.0,
                        };
                        if hit {
                            return Some(trap.owner_id);
                        }
                    }
                }
                _ => {}
            }
        }

        // Projectiles
        for proj in &self.projectiles {
            let dist = proj.position.distance(p_center);
            if dist < proj.radius + half_w {
                return Some(proj.owner_id);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trap_ownership_death() {
        let mut mgr = TrapManager::new();
        mgr.add_trap(Vec2::new(5.0, 5.0), TrapKind::SawBlade { radius: 0.75, rotation: 0.0 }, 42);

        let killer = mgr.check_player_death(Vec2::new(5.0, 4.5), Vec2::new(0.8, 1.75));
        assert_eq!(killer, Some(42));
    }
}
