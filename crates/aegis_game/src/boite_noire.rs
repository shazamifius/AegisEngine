//! # La boîte noire — enregistrer ce que le joueur fait, pour comprendre ce que le TAS ne sait pas
//!
//! Sa proposition, et elle est meilleure que celle qu'elle remplace : *« avoir un tableau exact de
//! la géométrie de mes mouvements et évaluer pourquoi le TAS ne sait pas faire, et moi oui. Le TAS
//! est censé tout savoir faire, il faut juste pouvoir le perfectionner. »*
//!
//! ## Pourquoi c'est le bon instrument
//!
//! Quand un humain franchit une carte que le solveur déclare infranchissable, **il est la preuve
//! qu'un chemin existe**. On ne tient pas une impression, on tient une contradiction — et une
//! contradiction se dissèque.
//!
//! Mieux : ce qu'il a fait est une **séquence d'entrées**, exactement la matière que le TAS
//! manipule. On peut donc la rejouer dans son simulateur, et le résultat tranche entre deux
//! diagnostics que rien d'autre ne sépare :
//!
//! - **la séquence rejouée arrive** → la physique est fidèle, c'est la RECHERCHE qui échoue
//!   (maille trop grossière, budget trop court, heuristique trop gloutonne) ;
//! - **la séquence rejouée n'arrive pas** → le simulateur du TAS DIVERGE du jeu, et tout ce que le
//!   solveur affirme est suspect, y compris ses succès.
//!
//! Le second cas serait bien plus grave que le premier, et rien à l'écran ne les distingue.
//!
//! ## Ce qu'elle coûte
//!
//! Une ligne par image en mémoire (une trentaine d'octets), écrite sur disque **à la fin de la
//! manche seulement** — jamais pendant. Une écriture par image ferait exactement le hoquet qu'on
//! cherche à éviter, pour un fichier qu'on ne lit qu'après coup.
//!
//! ⚠ Elle ne s'active qu'avec `AEGIS_BOITE_NOIRE=<dossier>`. Un jeu lancé normalement n'écrit rien.

use crate::grid::TileGrid;
use aegis_engine::math::Vec2;
use crate::player::{InputState, Player, PlayerState};
use crate::traps::TrapManager;
use std::io::Write;
use std::path::PathBuf;

/// Un instant de la partie, tel qu'il s'est réellement produit.
#[derive(Debug, Clone, Copy)]
struct Instant {
    dt: f32,
    gauche: bool,
    droite: bool,
    saut: bool,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    au_sol: bool,
}

pub struct BoiteNoire {
    dossier: Option<PathBuf>,
    instants: Vec<Instant>,
    carte: Option<String>,
    manche: u32,
}

impl Default for BoiteNoire {
    fn default() -> Self {
        Self::nouvelle()
    }
}

impl BoiteNoire {
    pub fn nouvelle() -> BoiteNoire {
        let dossier = std::env::var("AEGIS_BOITE_NOIRE").ok().map(PathBuf::from);
        if let Some(d) = &dossier {
            let _ = std::fs::create_dir_all(d);
            println!("[boite-noire] enregistrement dans {}", d.display());
        }
        BoiteNoire { dossier, instants: Vec::new(), manche: 0 , carte: None }
    }

    pub fn active(&self) -> bool {
        self.dossier.is_some()
    }

    /// Début de manche : on fige la carte telle qu'elle est au coup d'envoi.
    ///
    /// ⚠ **Au coup d'envoi, pas à la fin.** Les pièges bougent, meurent, tirent ; une carte relevée
    /// après coup ne serait plus celle que le joueur a franchie, et le rejeu porterait sur autre
    /// chose que ce qu'on veut comprendre.
    pub fn ouvrir_manche(&mut self, manche: u32, grid: &TileGrid, traps: &TrapManager) {
        if !self.active() {
            return;
        }
        self.manche = manche;
        self.instants.clear();
        self.carte = Some(decrire_carte(grid, traps));
    }

    /// Un tour de boucle. Appelé après la mise à jour, avec l'entrée qui l'a produit.
    pub fn noter(&mut self, dt: f32, input: &InputState, j: &Player) {
        if !self.active() || self.carte.is_none() {
            return;
        }
        // Borne : une manche dure 150 s, soit ~9 000 images à 60 Hz. Au-delà, quelque chose ne va
        // pas et on ne veut pas que la mémoire enfle en silence.
        if self.instants.len() >= 40_000 {
            return;
        }
        self.instants.push(Instant {
            dt,
            gauche: input.left,
            droite: input.right,
            saut: input.jump_pressed_this_frame,
            x: j.position.x,
            y: j.position.y,
            vx: j.velocity.x,
            vy: j.velocity.y,
            au_sol: j.state == PlayerState::OnGround,
        });
    }

    /// Fin de manche : on écrit, en disant si le joueur est ARRIVÉ.
    ///
    /// `verdict_tas` est ce que le solveur en pensait. **C'est le couple (arrivé, verdict) qui fait
    /// tout l'intérêt du fichier** : une manche où l'humain arrive alors que le TAS annonçait
    /// « pas trouvé » est précisément le cas à disséquer.
    pub fn fermer_manche(&mut self, arrive: bool, verdict_tas: &str) {
        let (Some(dossier), Some(carte)) = (self.dossier.as_ref(), self.carte.take()) else {
            return;
        };
        if self.instants.is_empty() {
            return;
        }
        let interessant = arrive && verdict_tas.contains("PasTrouvee");
        let nom = format!(
            "manche-{:03}-{}{}.txt",
            self.manche,
            if arrive { "arrive" } else { "echoue" },
            if interessant { "-CONTRADICTION" } else { "" }
        );
        let chemin = dossier.join(nom);
        let mut f = match std::fs::File::create(&chemin) {
            Ok(f) => f,
            Err(e) => {
                println!("[boite-noire] écriture impossible ({e})");
                return;
            }
        };
        let _ = writeln!(f, "# manche {} — humain: {} — tas: {}", self.manche,
            if arrive { "ARRIVE" } else { "echoue" }, verdict_tas);
        if interessant {
            let _ = writeln!(f, "# ⚠ CONTRADICTION : l'humain est arrivé là où le TAS ne trouvait rien.");
        }
        let _ = write!(f, "{carte}");
        let _ = writeln!(f, "# trace : dt gauche droite saut x y vx vy au_sol");
        for i in &self.instants {
            let _ = writeln!(
                f,
                "t {:.5} {} {} {} {:.4} {:.4} {:.4} {:.4} {}",
                i.dt, i.gauche as u8, i.droite as u8, i.saut as u8, i.x, i.y, i.vx, i.vy, i.au_sol as u8
            );
        }
        println!(
            "[boite-noire] {} ({} instants){}",
            chemin.display(),
            self.instants.len(),
            if interessant { "  ⚠ CONTRADICTION" } else { "" }
        );
        self.instants.clear();
    }
}

/// La carte en texte : lisible à l'œil, et relisible par une machine.
///
/// Le format est délibérément bête — une ligne par rangée, un caractère par tuile. Un format
/// binaire compact serait plus court et illisible ; or ce fichier existe pour être OUVERT quand on
/// cherche pourquoi quelque chose ne va pas.
fn decrire_carte(grid: &TileGrid, traps: &TrapManager) -> String {
    let mut s = String::new();
    s.push_str(&format!("carte {} {}\n", grid.width, grid.height));
    s.push_str(&format!(
        "depart {:.3} {:.3}\narrivee {:.3} {:.3}\n",
        grid.start_pos.x, grid.start_pos.y, grid.finish_pos.x, grid.finish_pos.y
    ));
    // Rangée du HAUT en premier, pour que le fichier ressemble à l'écran quand on l'ouvre.
    for y in (0..grid.height as i32).rev() {
        s.push_str("g ");
        for x in 0..grid.width as i32 {
            s.push(if grid.get_tile(x, y).is_solid() { '#' } else { '.' });
        }
        s.push('\n');
    }
    for t in &traps.traps {
        s.push_str(&format!(
            "piege {:.3} {:.3} {}\n",
            t.position.x,
            t.position.y,
            match t.kind {
                crate::traps::TrapKind::SpikeTrap => "pics",
                crate::traps::TrapKind::SawBlade { .. } => "scie",
                crate::traps::TrapKind::LaserEmitter { .. } => "laser",
                crate::traps::TrapKind::Flamethrower { .. } => "flammes",
                crate::traps::TrapKind::CannonTurret { .. } => "tourelle",
                crate::traps::TrapKind::MovingPlatform { .. } => "plateforme",
            }
        ));
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
//  L'ANALYSE — rejouer ce que l'humain a fait, dans le simulateur du solveur
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Un saut, tel que le joueur l'a réellement fait : d'où, vers où, et ce qu'il a fallu.
#[derive(Debug, Clone, Copy)]
pub struct Saut {
    pub depuis: (i32, i32),
    pub vers: (i32, i32),
    /// Hauteur gagnée au sommet, en tuiles — ce qu'un solveur doit savoir produire.
    pub montee: f32,
    /// Distance horizontale franchie.
    pub portee: f32,
    pub images: usize,
}

/// Ce qu'on apprend en rejouant une manche enregistrée.
#[derive(Debug)]
pub struct Analyse {
    pub instants: usize,
    /// Le SQUELETTE du chemin : les tuiles où le joueur a posé les pieds, dans l'ordre.
    ///
    /// C'est la vraie structure du parcours. Une manche fait ~2 400 images, mais seulement
    /// quelques dizaines d'appuis distincts : **le chemin est court, c'est sa description image
    /// par image qui est longue.** Toute la difficulté du solveur tient dans cet écart.
    pub appuis: Vec<(i32, i32)>,
    /// Les sauts, entre deux appuis.
    pub sauts: Vec<Saut>,
    /// Le plus haut saut de la manche — la contrainte que le solveur doit pouvoir reproduire.
    pub montee_max: f32,
    pub portee_max: f32,
    /// L'humain a-t-il atteint l'arrivée, d'après la trace ?
    pub humain_arrive: bool,
    /// Et en REJOUANT ses entrées dans la physique du TAS ?
    pub rejeu_arrive: bool,
    /// Première image où la position simulée s'écarte de plus d'une demi-tuile de l'enregistrée.
    pub divergence: Option<(usize, f32)>,
    /// Écart maximal constaté sur toute la manche.
    pub ecart_max: f32,
}

/// Rejoue un fichier de boîte noire et rend le diagnostic.
///
/// # Ce que le résultat veut dire, et c'est tout l'intérêt
///
/// - **`rejeu_arrive` vrai alors que le TAS annonçait « pas trouvé »** → la physique est fidèle, la
///   séquence gagnante EXISTE dans son monde, et c'est la RECHERCHE qui ne l'a pas trouvée. On sait
///   alors où creuser : maille, budget, heuristique.
/// - **`rejeu_arrive` faux alors que l'humain est arrivé** → le simulateur du TAS **diverge** du
///   jeu. Bien plus grave : tout ce que le solveur affirme devient suspect, y compris ses succès,
///   et `divergence` dit à quelle image ça commence.
///
/// Rien à l'écran ne sépare ces deux cas. Ce rejeu, si.
/// Une manche relue depuis son fichier : la carte telle qu'elle était, et ce que l'humain a tapé.
///
/// Séparé de [`analyser`] parce que ces deux besoins ne se recouvrent pas : le diagnostic veut un
/// verdict, le **solveur** veut la carte pour s'y mesurer. Une partie enregistrée devient ainsi un
/// cas de test permanent — et c'est le seul jeu d'épreuve dont on sache qu'un humain l'a franchi.
pub struct Manche {
    pub grid: TileGrid,
    /// `(dt, gauche, droite, saut)` pour chaque image jouée.
    pub entrees: Vec<(f32, bool, bool, bool)>,
    /// Les positions réellement enregistrées, en regard des entrées.
    pub positions: Vec<Vec2>,
    pub humain_arrive: bool,
}

/// Relit un fichier de boîte noire sans rien en juger.
pub fn charger_manche(chemin: &std::path::Path) -> Result<Manche, String> {
    let texte = std::fs::read_to_string(chemin).map_err(|e| format!("lecture : {e}"))?;

    let mut largeur = 0usize;
    let mut hauteur = 0usize;
    let mut depart = Vec2::new(0.0, 0.0);
    let mut arrivee = Vec2::new(0.0, 0.0);
    let mut rangees: Vec<String> = Vec::new();
    let mut entrees: Vec<(f32, bool, bool, bool)> = Vec::new();
    let mut positions: Vec<Vec2> = Vec::new();
    let mut humain_arrive = false;

    for ligne in texte.lines() {
        let m: Vec<&str> = ligne.split_whitespace().collect();
        match m.as_slice() {
            ["#", "manche", _, "—", "humain:", etat, ..] => humain_arrive = *etat == "ARRIVE",
            ["carte", w, h] => {
                largeur = w.parse().unwrap_or(0);
                hauteur = h.parse().unwrap_or(0);
            }
            ["depart", x, y] => depart = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0)),
            ["arrivee", x, y] => arrivee = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0)),
            ["g", r] => rangees.push((*r).to_string()),
            ["t", dt, g, d, s, x, y, ..] => {
                entrees.push((dt.parse().unwrap_or(0.0), *g == "1", *d == "1", *s == "1"));
                positions.push(Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0)));
            }
            _ => {}
        }
    }
    if largeur == 0 || rangees.len() != hauteur || entrees.is_empty() {
        return Err("fichier incomplet ou illisible".to_string());
    }

    let mut grid = TileGrid::vide(largeur, hauteur);
    grid.start_pos = depart;
    grid.finish_pos = arrivee;
    // Les rangées sont écrites du HAUT vers le bas : on les remet à l'endroit.
    for (i, r) in rangees.iter().enumerate() {
        let y = hauteur - 1 - i;
        for (x, c) in r.chars().enumerate() {
            if c == '#' {
                grid.set_tile(x, y, crate::grid::TileType::SolidBlock);
            }
        }
    }

    Ok(Manche { grid, entrees, positions, humain_arrive })
}

pub fn analyser(chemin: &std::path::Path) -> Result<Analyse, String> {
    let manche = charger_manche(chemin)?;
    let grid = manche.grid;
    let arrivee = grid.finish_pos;
    let humain_arrive = manche.humain_arrive;
    let trace: Vec<(f32, bool, bool, bool, f32, f32)> = manche
        .entrees
        .iter()
        .zip(manche.positions.iter())
        .map(|((dt, g, d, s), p)| (*dt, *g, *d, *s, p.x, p.y))
        .collect();

    // ⚠ Rejeu dans la VUE PERMANENTE, comme le solveur : c'est son monde qu'on éprouve, pas celui
    // du jeu. Utiliser les pièges complets comparerait deux choses différentes et n'apprendrait
    // rien sur ce que le TAS pouvait trouver.
    let traps = crate::tas::vue_permanente(&TrapManager::new());
    let mut j = Player::new(grid.start_pos);
    let mut divergence = None;
    let mut ecart_max = 0.0f32;
    let mut rejeu_arrive = false;

    for (i, (dt, g, d, saut, x, y)) in trace.iter().enumerate() {
        let entree = InputState {
            left: *g,
            right: *d,
            up: *saut,
            jump_pressed_this_frame: *saut,
            ..Default::default()
        };
        j.update(*dt, &entree, &grid, &traps);
        let ecart = (j.position - Vec2::new(*x, *y)).length();
        ecart_max = ecart_max.max(ecart);
        if divergence.is_none() && ecart > 0.5 {
            divergence = Some((i, ecart));
        }
        if (j.position - arrivee).length() < crate::tas::RAYON_ARRIVEE {
            rejeu_arrive = true;
            break;
        }
    }

    // ── LA GÉOMÉTRIE DU CHEMIN HUMAIN ───────────────────────────────────────────────────────
    //
    // On ne garde que les CHANGEMENTS de tuile foulée : c'est le squelette du parcours. Une
    // manche de 2 400 images tient en quelques dizaines d'appuis — et cet écart est exactement le
    // problème du solveur, qui cherche image par image ce qui se décrit en dizaines d'étapes.
    let mut appuis: Vec<(i32, i32)> = Vec::new();
    let mut sauts: Vec<Saut> = Vec::new();
    let mut dernier_sol: Option<(usize, (i32, i32), f32)> = None;
    let mut sommet = f32::MIN;

    let mut j = Player::new(grid.start_pos);
    for (i, (dt, g, d, saut, _, _)) in trace.iter().enumerate() {
        let entree = InputState {
            left: *g, right: *d, up: *saut, jump_pressed_this_frame: *saut, ..Default::default()
        };
        j.update(*dt, &entree, &grid, &traps);
        sommet = sommet.max(j.position.y);
        if j.state == PlayerState::OnGround {
            let tuile = (j.position.x.round() as i32, j.position.y.round() as i32);
            if appuis.last() != Some(&tuile) {
                if let Some((i0, t0, y0)) = dernier_sol {
                    // Un saut n'est retenu que s'il a quitté le sol : sinon c'est de la marche.
                    if i - i0 > 3 && sommet > y0 + 0.2 {
                        sauts.push(Saut {
                            depuis: t0,
                            vers: tuile,
                            montee: sommet - y0,
                            portee: (tuile.0 - t0.0).abs() as f32,
                            images: i - i0,
                        });
                    }
                }
                appuis.push(tuile);
                dernier_sol = Some((i, tuile, j.position.y));
                sommet = j.position.y;
            }
        }
    }
    let montee_max = sauts.iter().map(|s| s.montee).fold(0.0f32, f32::max);
    let portee_max = sauts.iter().map(|s| s.portee).fold(0.0f32, f32::max);

    Ok(Analyse {
        instants: trace.len(),
        appuis,
        sauts,
        montee_max,
        portee_max,
        humain_arrive,
        rejeu_arrive,
        divergence,
        ecart_max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_variable_d_environnement_elle_n_ecrit_rien() {
        // `AEGIS_BOITE_NOIRE` n'est pas posée dans les tests : la boîte doit rester inerte.
        let mut b = BoiteNoire::nouvelle();
        assert!(!b.active());
        b.ouvrir_manche(1, &TileGrid::vide(10, 10), &TrapManager::new());
        b.fermer_manche(true, "PasTrouvee"); // ne doit pas paniquer ni écrire
    }

    /// La carte doit se relire : un format qu'on ne sait pas reparser ne sert qu'une fois.
    #[test]
    fn la_carte_ecrite_se_relit_tuile_par_tuile() {
        let mut g = TileGrid::vide(6, 4);
        g.set_tile(0, 0, crate::grid::TileType::SolidBlock);
        g.set_tile(5, 3, crate::grid::TileType::SolidBlock);
        let texte = decrire_carte(&g, &TrapManager::new());

        let lignes: Vec<&str> = texte.lines().filter(|l| l.starts_with("g ")).collect();
        assert_eq!(lignes.len(), 4, "une ligne par rangee");
        // La PREMIÈRE ligne du fichier est la rangée du HAUT (y = 3), pour ressembler à l'écran.
        assert_eq!(lignes[0].trim_start_matches("g ").chars().nth(5), Some('#'), "(5,3) solide");
        assert_eq!(lignes[3].trim_start_matches("g ").chars().next(), Some('#'), "(0,0) solide");
        assert!(texte.contains("carte 6 4"));
        assert!(texte.contains("depart"));
    }

    #[test]
    fn les_pieges_sont_nommes_en_clair() {
        let mut t = TrapManager::new();
        t.add_trap(Vec2::new(3.0, 2.0), crate::traps::TrapKind::SpikeTrap, 0);
        t.add_trap(
            Vec2::new(4.0, 2.0),
            crate::traps::TrapKind::LaserEmitter {
                dir: crate::traps::Direction::Up,
                active: true,
                timer: 0.0,
            },
            0,
        );
        let s = decrire_carte(&TileGrid::vide(6, 4), &t);
        assert!(s.contains("piege 3.000 2.000 pics"), "{s}");
        assert!(s.contains("piege 4.000 2.000 laser"), "{s}");
    }
}
