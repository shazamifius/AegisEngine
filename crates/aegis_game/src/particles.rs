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

impl Particle {
    /// La couleur de la particule **à cet instant**, fondu de fin compris.
    ///
    /// ## Le défaut qu'elle corrige, et il se voyait
    ///
    /// `life` ne servait qu'à SUPPRIMER la particule quand elle atteignait `max_life`. Une
    /// poussière de course vivait donc à pleine opacité pendant un quart de seconde, puis
    /// **disparaissait d'un coup**. C'est l'un des trois défauts qui faisaient dire, à l'œil,
    /// que les particules « se fondent mal ».
    ///
    /// ⚠ *Le carnet Unreal portait déjà la règle, écrite pour un autre moteur en juillet 2026 :
    /// « fondu de queue par l'ÂGE, jamais par U ». Elle n'avait simplement jamais traversé.*
    ///
    /// ## Pourquoi une décroissance linéaire sur TOUTE la vie, et pas un fondu sur la fin
    ///
    /// Un fondu « sur les 30 derniers pour cent » demanderait un 0,3 à justifier pour toujours.
    /// Ici l'opacité vaut simplement la fraction de vie qui reste : **aucune constante n'apparaît**,
    /// et le comportement est celui qu'on attend d'une poussière qui se disperse — pleine à
    /// l'émission, éteinte à sa mort. *La courbe exacte reste un goût : si l'œil trouve qu'elle
    /// traîne, c'est ici qu'on la change, et nulle part ailleurs.*
    pub fn couleur_maintenant(&self) -> Vec4 {
        // ⚠ `max_life` vient d'un tirage aléatoire ; se garder d'une division par zéro coûte une
        // comparaison et évite un `NaN` qui rendrait la particule invisible sans rien dire.
        let reste = if self.max_life > 0.0 {
            (1.0 - self.life / self.max_life).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Vec4::new(self.color.x, self.color.y, self.color.z, self.color.w * reste)
    }
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

    // 6. Explosion de Particules à l'Ouverture du Carton Mystère (Cardboard Open Burst)
    pub fn spawn_box_open_burst(&mut self, box_pos: Vec3) {
        for _ in 0..24 {
            let r1 = self.next_rand();
            let r2 = self.next_rand();
            let r3 = self.next_rand();

            let (color, emissive) = match (r1 * 4.0) as usize {
                0 => (Vec4::new(0.85, 0.65, 0.42, 1.0), 0.0), // Éclat de Carton Kraft
                1 => (Vec4::new(0.98, 0.85, 0.20, 1.0), 6.0), // Étoile d'Or Mystère
                2 => (Vec4::new(0.30, 0.90, 0.95, 1.0), 4.0), // Énergie Cyan
                _ => (Vec4::new(0.95, 0.30, 0.30, 1.0), 4.0), // Énergie Rouge
            };

            let speed_x = (r1 - 0.5) * 7.0;
            let speed_y = 2.5 + r2 * 6.0;

            self.particles.push(Particle {
                // Le `z` etait fige a 0.25 et ignorait celui du carton, qui flotte a douze
                // unites de la : la gerbe serait sortie tres loin derriere lui. On se sert du
                // point recu en entier plutot que d'en redevenir la moitie.
                pos: Vec3::new(box_pos.x + (r2 - 0.5) * 0.4, box_pos.y + 0.5, box_pos.z),
                vel: Vec3::new(speed_x, speed_y, (r3 - 0.5) * 2.0),
                size: Vec3::splat(0.08 + r2 * 0.08),
                color,
                emissive,
                life: 0.0,
                max_life: 0.35 + r3 * 0.25,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le témoin du fondu de fin. Il tombe si `couleur_maintenant` redevient un simple accès au
    /// champ `color` — c'est-à-dire si la particule se remet à disparaître d'un coup.
    fn poussiere(life: f32, max_life: f32) -> Particle {
        Particle {
            pos: Vec3::splat(0.0),
            vel: Vec3::splat(0.0),
            size: Vec3::splat(0.1),
            color: Vec4::new(0.88, 0.88, 0.82, 0.65),
            emissive: 0.0,
            life,
            max_life,
        }
    }

    #[test]
    fn une_particule_neuve_a_toute_l_opacite_qu_on_lui_a_donnee() {
        let p = poussiere(0.0, 0.30);
        assert!((p.couleur_maintenant().w - 0.65).abs() < 1e-6);
    }

    #[test]
    fn une_particule_s_efface_au_lieu_de_disparaitre_d_un_coup() {
        // ⚠ C'est LE défaut que ce fondu corrige : `life` ne servait qu'à supprimer la
        // particule, donc elle vivait à pleine opacité puis s'effaçait instantanément.
        let debut = poussiere(0.0, 0.30).couleur_maintenant().w;
        let milieu = poussiere(0.15, 0.30).couleur_maintenant().w;
        let fin = poussiere(0.29, 0.30).couleur_maintenant().w;

        assert!(milieu < debut, "l'opacite doit decroitre : {milieu} >= {debut}");
        assert!(fin < milieu, "l'opacite doit continuer de decroitre : {fin} >= {milieu}");
        assert!(fin < 0.05, "en fin de vie elle doit etre quasi eteinte, pas a {fin}");
    }

    #[test]
    fn la_couleur_elle_meme_ne_bouge_pas_avec_l_age() {
        // Seule l'opacite s'eteint. Une poussiere qui changerait de teinte en vieillissant
        // serait un effet que personne n'a demande.
        let jeune = poussiere(0.0, 0.30).couleur_maintenant();
        let vieille = poussiere(0.25, 0.30).couleur_maintenant();
        assert!((jeune.x - vieille.x).abs() < 1e-6);
        assert!((jeune.y - vieille.y).abs() < 1e-6);
        assert!((jeune.z - vieille.z).abs() < 1e-6);
    }

    #[test]
    fn une_duree_de_vie_nulle_ne_produit_pas_de_nan() {
        // `max_life` vient d'un tirage aleatoire. Une division par zero donnerait un NaN, et un
        // NaN en alpha rend la particule invisible SANS que rien ne le signale.
        let p = poussiere(0.0, 0.0).couleur_maintenant();
        assert!(p.w.is_finite(), "l'opacite doit rester un nombre, pas {}", p.w);
        assert_eq!(p.w, 0.0);
    }

    #[test]
    fn une_particule_en_sursis_ne_remonte_jamais_au_dessus_de_zero() {
        // Si une image passe entre le depassement de `max_life` et la suppression, l'opacite
        // doit rester bornee : sans le `clamp`, elle deviendrait negative.
        let p = poussiere(0.45, 0.30).couleur_maintenant();
        assert_eq!(p.w, 0.0);
    }
}
