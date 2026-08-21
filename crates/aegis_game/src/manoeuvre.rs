//! # Chercher d'appui en appui, et non image par image
//!
//! ## Le défaut que ce module corrige
//!
//! Le solveur d'origine explore une **image** à la fois : à chaque soixantième de seconde il
//! essaie les six commandes possibles. Sur une manche réellement jouée, le chemin gagnant dure
//! **2 126 images**. Aucun budget n'atteint cette profondeur — et ce n'est pas une question de
//! réglage : à six branches par image, l'arbre est hors d'atteinte pour toujours.
//!
//! Mesuré sur ses deux manches enregistrées, l'échec a d'ailleurs une forme précise : le solveur
//! s'arrête à `(45.6, 12.0)` alors que l'arrivée est en `(51.5, 2.0)`. Il n'est pas perdu au loin,
//! il est **monté dans un cul-de-sac qui paraît proche à vol d'oiseau**, et il y dépense tout.
//! Un poids d'heuristique de 30 rend cette faute inévitable.
//!
//! ## Ce qui change, et pourquoi ça règle le problème plutôt que de le déplacer
//!
//! On ne cherche plus une suite de commandes, on cherche une suite d'**appuis** — les endroits où
//! le joueur se pose ou s'accroche. Entre deux appuis, le mouvement n'est pas exploré : il est
//! **joué en entier**, une fois, dans la vraie physique. La même manche demande alors une dizaine
//! de décisions au lieu de 2 126.
//!
//! C'est le patron que la robotique appelle *state lattice* et que les moteurs de jeu appellent
//! *jump links* : un répertoire de manœuvres réalisables, et une recherche qui ne raisonne plus
//! qu'en manœuvres.
//!
//! ## Les deux propriétés qui rendent la chose honnête
//!
//! 1. **Aucune manœuvre n'est calculée par une formule.** Chacune est simulée par `Player::update`,
//!    exactement le code du jeu. Une manœuvre qui tue, qui n'aboutit nulle part ou qui traîne trop
//!    est jetée. On ne peut donc pas inventer un saut que le jeu ne permet pas — le seul risque
//!    restant est d'en **oublier**, jamais d'en fabriquer.
//! 2. **On ne reconstruit jamais l'état du joueur.** Chaque piste transporte le `Player` réellement
//!    obtenu ; la manœuvre suivante part de cet état-là, au bit près. La solution finale est donc la
//!    simple concaténation des entrées, et elle se rejoue à l'identique depuis le départ.
//!
//! ⚠ *Ce que ce module ne prouve pas :* qu'une carte déclarée infranchissable le soit vraiment. Le
//! répertoire de manœuvres est fini ; une carte qui n'aurait de solution que par un geste absent du
//! répertoire serait refusée à tort. « Pas trouvé » reste « pas trouvé », jamais « impossible ».

use crate::grid::TileGrid;
use crate::player::{Player, PlayerState};
use crate::tas::{Manette, Solution, Verdict, PAS, RAYON_ARRIVEE};
use crate::traps::TrapManager;
use aegis_engine::math::Vec2;
use std::collections::{BinaryHeap, HashMap};

/// Au-delà, une manœuvre est considérée comme perdue dans le vide et abandonnée.
///
/// Trois secondes : plus long que la plus longue chute observée sur ses manches (19 tuiles, ~1,5 s)
/// avec de la marge. Ce n'est pas un réglage de qualité — c'est une garde contre les trajectoires
/// qui ne retombent jamais (tomber hors carte est déjà traité comme une mort).
const IMAGES_MAX: u16 = 180;

/// La position d'un appui est retenue au QUART de tuile, pas à la tuile.
///
/// ⚠ Mesuré, pas choisi : franchir un mur de 3 demande de sauter depuis `x = 10.22` pour que
/// l'apogée à 4,22 dépasse un sommet à 4,00. À la tuile entière, la première visite de la tuile 10
/// — mettons `x = 10.95` — fermait la case et interdisait de réessayer 70 cm plus tôt. Le solveur
/// butait alors sur un mur que l'ancien franchissait : un faux « impossible », le pire verdict
/// possible ici puisqu'il fait voter des gens sur une carte qui n'a rien.
const QUARTS: f32 = 4.0;

/// À quels instants d'une course on tente de sauter, en images.
///
/// MESURÉ, et c'est ce qui a fixé le nombre : à neuf valeurs, le répertoire gonfle à soixante
/// manœuvres par appui et épuise le budget avant d'aboutir — un mur de 2 qui passait ne passait
/// plus. À cinq, tout ce que neuf trouvait est retrouvé, pour un tiers du coût. Affiner au-delà
/// n'achète rien : le geste qui manque encore (le mur de 3) ne tient pas à l'instant du saut.
const ELANS: [u16; 5] = [0, 6, 13, 24, 42];

/// Le genre d'un appui : sur quoi le joueur tient.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Genre {
    /// Les deux pieds sur du solide.
    Sol,
    /// Accroché à un mur, `true` si le mur est à sa gauche.
    Mur(bool),
}

/// La clé de fermeture de la recherche : deux états qui partagent cette clé sont tenus pour
/// équivalents.
///
/// ⚠ **C'est ici que se joue l'équilibre du solveur, et c'est le seul endroit arbitraire du
/// module.** Trop fine, la clé fait réexplorer mille variantes du même endroit — c'était le défaut
/// de l'ancienne empreinte, qui distinguait le huitième de tuile. Trop grossière, elle confond deux
/// situations vraiment différentes et peut faire manquer une solution.
///
/// La tuile suffit pour la position : deux joueurs sur la même tuile, dans le même genre d'appui et
/// à vitesse comparable peuvent faire la même chose ensuite. La vitesse, elle, ne se néglige pas :
/// arriver lancé ou à l'arrêt change ce qu'on peut franchir.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Appui {
    pub x: i32,
    pub y: i32,
    pub genre: Genre,
    /// Vitesse horizontale, par tranches de ~2,1 tuiles/s (quatre crans de chaque côté).
    vx: i8,
    /// Élan de chute mis en réserve, par tranches de 6. Nul ailleurs que sur un mur, où il décide
    /// de la puissance du saut mural.
    elan: u8,
}

impl Appui {
    fn de(joueur: &Player) -> Option<Appui> {
        let genre = match joueur.state {
            PlayerState::OnGround => Genre::Sol,
            PlayerState::WallSliding { left_wall } => Genre::Mur(left_wall),
            _ => return None,
        };
        Some(Appui {
            x: (joueur.position.x * QUARTS).floor() as i32,
            y: (joueur.position.y * QUARTS).floor() as i32,
            genre,
            vx: (joueur.velocity.x / 2.125).round().clamp(-4.0, 4.0) as i8,
            elan: (joueur.stored_fall_momentum / 6.0).round().clamp(0.0, 4.0) as u8,
        })
    }
}

/// Un geste complet, exprimé comme une intention et non comme une suite de touches.
///
/// Le répertoire est délibérément court. Chaque entrée correspond à une chose qu'un joueur *pense*
/// (« je cours jusqu'au bord et je me laisse tomber »), pas à une combinaison de clavier. C'est ce
/// qui permet d'en avoir une quinzaine plutôt que six puissance mille.
#[derive(Clone, Copy, Debug)]
pub enum Patron {
    /// Tenir une direction au sol. Les appuis traversés en chemin sont tous récoltés : une seule
    /// simulation donne toute la plateforme, à la manière d'un saut de *Jump Point Search*.
    Courir { droite: bool },
    /// Courir `elan` images puis sauter, en tenant `vol` pendant la montée (−1, 0 ou +1).
    Sauter { droite: bool, elan: u16, vol: i8 },
    /// Se laisser tomber en tenant une direction, puis piloter la chute avec `vol`.
    Tomber { droite: bool, vol: i8 },
    /// Depuis un mur : sauter, puis tenir `vol` — s'éloigner, revenir en zigzag, ou lâcher.
    Kick { vol: i8 },
    /// Depuis un mur : rester collé et glisser vers le bas pendant `images`.
    Glisser { images: u16 },
}

/// Le répertoire complet, tel qu'il est proposé à chaque appui.
///
/// ⚠ Un `Patron` proposé n'est jamais un geste garanti : il est *tenté*, et rejeté s'il ne mène à
/// rien. Proposer un saut mural depuis le sol ne coûte donc qu'une simulation perdue, pas une
/// erreur — et ça évite une table de ce-qui-est-permis-où qu'il faudrait tenir à jour.
fn repertoire(genre: Genre) -> Vec<Patron> {
    let mut v = Vec::with_capacity(20);
    match genre {
        Genre::Sol => {
            for droite in [true, false] {
                v.push(Patron::Courir { droite });
                // Sauter tout de suite, ou après un élan : c'est ce qui décide de la portée.
                for elan in ELANS {
                    for vol in [1i8, -1, 0] {
                        let vol = if droite { vol } else { -vol };
                        v.push(Patron::Sauter { droite, elan, vol });
                    }
                }
                // Se laisser tomber du bord — le geste qui ouvre les longues descentes, et que
                // l'ancien solveur ne trouvait jamais parce qu'il n'a pas de touche dédiée.
                v.push(Patron::Tomber { droite, vol: if droite { 1 } else { -1 } });
                v.push(Patron::Tomber { droite, vol: if droite { -1 } else { 1 } });
            }
        }
        Genre::Mur(a_gauche) => {
            let loin = if a_gauche { 1i8 } else { -1 };
            v.push(Patron::Kick { vol: loin });
            v.push(Patron::Kick { vol: -loin });
            v.push(Patron::Kick { vol: 0 });
            v.push(Patron::Glisser { images: 12 });
            v.push(Patron::Glisser { images: 40 });
        }
    }
    v
}

/// Ce que le patron demande à l'image `i` de son déroulement.
fn commande(p: Patron, i: u16, a_gauche: bool) -> Manette {
    let vers = |d: i8| Manette { gauche: d < 0, droite: d > 0, saut: false };
    match p {
        Patron::Courir { droite } => vers(if droite { 1 } else { -1 }),
        Patron::Sauter { droite, elan, vol } => {
            if i < elan {
                vers(if droite { 1 } else { -1 })
            } else {
                let mut m = vers(vol);
                // Le saut se maintient : le jeu réarme son tampon tant que la touche est tenue,
                // ce qui déclenche aussi un saut mural dès qu'on rencontre un mur.
                m.saut = true;
                m
            }
        }
        Patron::Tomber { droite, vol } => {
            // On court d'abord jusqu'au vide, puis on pilote la chute.
            if i < 30 { vers(if droite { 1 } else { -1 }) } else { vers(vol) }
        }
        Patron::Kick { vol } => {
            // ⚠ La première image presse VERS le mur : sans ça l'accroche est perdue avant même
            // que le saut ne parte, et le kick n'a jamais lieu.
            if i == 0 {
                let mut m = vers(if a_gauche { -1 } else { 1 });
                m.saut = true;
                m
            } else {
                vers(vol)
            }
        }
        Patron::Glisser { .. } => vers(if a_gauche { -1 } else { 1 }),
    }
}

/// Ce qu'une manœuvre a produit.
pub struct Aboutissement {
    pub appui: Appui,
    pub joueur: Player,
    pub entrees: Vec<Manette>,
    /// Vrai si l'arrivée a été franchie **pendant** la manœuvre.
    pub arrive: bool,
}

/// Joue un patron jusqu'à ce qu'il aboutisse, et rend tous les appuis rencontrés.
///
/// Rendre **plusieurs** aboutissements n'est pas un raffinement : une course le long d'une
/// plateforme passe par vingt appuis utiles, et les récolter d'un coup évite vingt simulations
/// qui referaient le même travail.
fn derouler(
    depart: &Player,
    grid: &TileGrid,
    traps: &TrapManager,
    p: Patron,
    a_gauche: bool,
) -> Vec<Aboutissement> {
    let mut joueur = depart.clone();
    let mut entrees: Vec<Manette> = Vec::new();
    let mut recolte = Vec::new();
    let depart_appui = Appui::de(depart);
    let mut saut_precedent = false;
    let plafond = match p {
        Patron::Glisser { images } => images,
        _ => IMAGES_MAX,
    };

    for i in 0..plafond {
        let m = commande(p, i, a_gauche);
        joueur.update(PAS, &m.vers_entrees(saut_precedent), grid, traps);
        saut_precedent = m.saut;
        entrees.push(m);

        if joueur.state == PlayerState::Dead {
            return recolte;
        }
        // ⚠ SORTIR DE LA CARTE PAR LE HAUT N'EST PAS UN CHEMIN.
        //
        // `get_tile` rend `Air` au-delà des bords : au-dessus de la dernière rangée, il n'y a donc
        // aucune collision, et un mur de n'importe quelle hauteur se contourne en passant par le
        // ciel. Mesuré : sur un couloir muré du sol au plafond, le solveur franchissait à y = 11,69
        // pour une carte haute de 10 — un chemin que la physique accepte et qu'aucun joueur ne
        // devrait prendre.
        //
        // Le TAS s'interdit donc ce qu'il ne veut pas montrer. C'est délibérément conservateur : au
        // pire il refuse un passage qui existe, jamais il n'en invente un. Un faux « pas trouvé »
        // fait voter pour rien ; un faux « ça passe » ferait afficher une démonstration où le
        // personnage s'envole au-dessus du décor.
        //
        // ⚠ Ceci ne corrige que le SOLVEUR. Dans le jeu, un joueur peut toujours sortir par le
        // haut — c'est un défaut de la carte elle-même, à trancher à part.
        if joueur.position.y > grid.height as f32 {
            return recolte;
        }
        if (joueur.position - grid.finish_pos).length() < RAYON_ARRIVEE {
            recolte.push(Aboutissement {
                appui: Appui::de(&joueur).unwrap_or(Appui {
                    x: (joueur.position.x * QUARTS).floor() as i32,
                    y: (joueur.position.y * QUARTS).floor() as i32,
                    genre: Genre::Sol,
                    vx: 0,
                    elan: 0,
                }),
                joueur: joueur.clone(),
                entrees: entrees.clone(),
                arrive: true,
            });
            return recolte;
        }

        // Un appui neuf : on le récolte, et on continue — la course peut en offrir d'autres.
        if let Some(a) = Appui::de(&joueur) {
            if Some(a) != depart_appui && !recolte.iter().any(|r: &Aboutissement| r.appui == a) {
                recolte.push(Aboutissement {
                    appui: a,
                    joueur: joueur.clone(),
                    entrees: entrees.clone(),
                    arrive: false,
                });
                // Un saut, une chute ou un kick s'arrêtent à leur premier appui : continuer
                // reviendrait à enchaîner deux gestes sous un seul nom, et la recherche perdrait
                // le droit de choisir autre chose au milieu.
                if !matches!(p, Patron::Courir { .. }) {
                    return recolte;
                }
            }
        }
    }
    recolte
}

/// Une piste en cours d'exploration.
struct Piste {
    cout: u32,
    estime: u32,
    joueur: Player,
    entrees: Vec<Manette>,
}

impl PartialEq for Piste {
    fn eq(&self, a: &Self) -> bool {
        self.estime == a.estime
    }
}
impl Eq for Piste {}
impl Ord for Piste {
    fn cmp(&self, a: &Self) -> std::cmp::Ordering {
        a.estime.cmp(&self.estime)
    }
}
impl PartialOrd for Piste {
    fn partial_cmp(&self, a: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(a))
    }
}

/// Minoration honnête du temps restant, en images.
///
/// Seul l'axe horizontal entre dans le calcul, et c'est volontaire : la vitesse horizontale est
/// bornée par `MAX_RUN_SPEED`, donc `dx / 8.5` est un temps qu'on ne peut pas battre. La verticale
/// n'offre aucune borne de ce genre — une chute libre va bien plus vite que la course — et
/// l'inclure rendrait l'estimation optimiste, donc capable de faire manquer la meilleure solution.
///
/// ⚠ C'est exactement l'inverse du réglage précédent (`POIDS = 30`), qui **surestimait** pour aller
/// vite et se laissait piéger par le premier cul-de-sac d'apparence proche.
fn estimation(joueur: &Player, arrivee: Vec2) -> u32 {
    let dx = (joueur.position.x - arrivee.x).abs();
    (dx / Player::MAX_RUN_SPEED / PAS) as u32
}

/// Cherche un chemin en raisonnant par manœuvres.
///
/// `plafond` borne le nombre de manœuvres déroulées, pas le nombre d'images : c'est la grandeur qui
/// dit vraiment ce que la recherche a coûté ici.
pub fn resoudre_par_appuis(
    grid: &TileGrid,
    traps: &TrapManager,
    plafond: usize,
) -> Verdict {
    let arrivee = grid.finish_pos;
    // ⚠ On part EXACTEMENT d'où part `rejouer` : `Player::new(start_pos)`, sans aucune mise en
    // condition. Ce n'est pas un détail de style — la première version amorçait le joueur par
    // trente images de chute qu'elle **n'inscrivait pas** dans la solution rendue. La séquence
    // était donc décalée de trente images par rapport au vrai début de partie, et se rejouait
    // faux : le solveur produisait des chemins qui traversaient les murs pleins, attrapés par
    // `un_mur_infranchissable_ne_produit_jamais_de_fausse_solution`.
    //
    // Partager le point de départ avec le vérificateur rend cette faute impossible à refaire,
    // plutôt que de la corriger — il n'y a plus deux façons de commencer une partie.
    let joueur = Player::new(grid.start_pos);
    let entrees_amorce: Vec<Manette> = Vec::new();

    let mut file = BinaryHeap::new();
    let mut vus: HashMap<Appui, u32> = HashMap::new();
    let mut deroules = 0usize;
    let mut plus_loin = joueur.position;
    let mut atteint: Vec<(i32, i32)> = Vec::new();

    let estime = estimation(&joueur, arrivee);
    file.push(Piste { cout: 0, estime, joueur, entrees: entrees_amorce });

    while let Some(piste) = file.pop() {
        if deroules >= plafond {
            break;
        }
        let Some(ici) = Appui::de(&piste.joueur) else { continue };
        if let Some(&meilleur) = vus.get(&ici) {
            if meilleur < piste.cout {
                continue;
            }
        }
        if piste.joueur.position.x > plus_loin.x {
            plus_loin = piste.joueur.position;
        }
        if !atteint.contains(&(ici.x, ici.y)) {
            atteint.push((ici.x, ici.y));
        }

        let a_gauche = matches!(ici.genre, Genre::Mur(true));
        for p in repertoire(ici.genre) {
            deroules += 1;
            for abo in derouler(&piste.joueur, grid, traps, p, a_gauche) {
                let mut entrees = piste.entrees.clone();
                entrees.extend_from_slice(&abo.entrees);
                if abo.arrive {
                    return Verdict::Franchissable(Solution { entrees });
                }
                let cout = entrees.len() as u32;
                let deja = vus.get(&abo.appui).copied();
                if deja.is_none_or(|d| cout < d) {
                    vus.insert(abo.appui, cout);
                    let estime = cout + estimation(&abo.joueur, arrivee);
                    file.push(Piste { cout, estime, joueur: abo.joueur, entrees });
                }
            }
        }
    }

    Verdict::PasTrouve { explores: deroules, plus_loin, atteint }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **LE test.** Ses deux manches, celles qu'il a gagnées et que le solveur refusait.
    ///
    /// Le critère n'a pas été choisi après coup : ces fichiers existaient avant la première ligne
    /// de ce module, et ce sont des parties réelles, pas des cartes fabriquées pour valider mon
    /// propre code. C'est toute la différence entre une preuve et une mise en scène.
    #[test]
    fn les_manches_qu_il_a_gagnees_sont_enfin_trouvees() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/manches");
        for nom in [
            "manche-002-arrive-CONTRADICTION.txt",
            "manche-003-arrive-CONTRADICTION.txt",
        ] {
            let m = crate::boite_noire::charger_manche(&base.join(nom)).expect("manche lisible");
            let traps = crate::traps::TrapManager::new();
            let debut = std::time::Instant::now();
            let verdict = resoudre_par_appuis(&m.grid, &traps, 40_000);
            let duree = debut.elapsed();

            match verdict {
                Verdict::Franchissable(s) => {
                    eprintln!(
                        "  ✅ {nom} : trouvé en {:.2?} — {} images ({:.1} s de jeu)",
                        duree,
                        s.entrees.len(),
                        s.duree()
                    );
                    // ⚠ Trouver ne suffit pas : on REJOUE la solution depuis le départ, dans la
                    // physique du jeu. Sans ce rejeu, on croirait sur parole une recherche qui
                    // pourrait avoir enchaîné des états incompatibles.
                    assert!(
                        crate::tas::rejouer(&m.grid, &traps, &s.entrees),
                        "{nom} : la solution trouvée NE se rejoue PAS — la recherche a produit un \
                         chemin que la physique refuse."
                    );
                }
                Verdict::PasTrouve { explores, plus_loin, .. } => panic!(
                    "{nom} : toujours pas trouvé après {explores} manœuvres ({:.2?}), au plus loin \
                     ({:.1}, {:.1}) — arrivée en ({:.1}, {:.1})",
                    duree, plus_loin.x, plus_loin.y, m.grid.finish_pos.x, m.grid.finish_pos.y
                ),
            }
        }
    }
}

#[cfg(test)]
mod sonde {
    use super::*;
    use crate::grid::TileType;

    /// Sonde : par où passe la solution sur le « mur infranchissable » des tests d'origine ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn par_ou_passe_le_mur_dit_infranchissable() {
        let mut grid = TileGrid::vide(24, 10);
        for x in 0..24 {
            grid.set_tile(x, 0, TileType::SolidBlock);
        }
        for y in 1..10 {
            grid.set_tile(12, y, TileType::SolidBlock);
        }
        let traps = TrapManager::new();
        let Verdict::Franchissable(s) = resoudre_par_appuis(&grid, &traps, 40_000) else {
            eprintln!("  pas trouvé");
            return;
        };
        eprintln!("  solution : {} images", s.entrees.len());
        eprintln!("  rejeu    : {}", crate::tas::rejouer(&grid, &traps, &s.entrees));

        // On refait le trajet pour voir la hauteur atteinte au moment de franchir x = 12.
        let mut j = Player::new(grid.start_pos);
        let mut prec = false;
        let (mut ymax, mut y_au_passage) = (f32::MIN, f32::NAN);
        let mut avant = j.position.x;
        for m in &s.entrees {
            j.update(PAS, &m.vers_entrees(prec), &grid, &traps);
            prec = m.saut;
            ymax = ymax.max(j.position.y);
            if avant <= 12.5 && j.position.x > 12.5 {
                y_au_passage = j.position.y;
            }
            avant = j.position.x;
        }
        eprintln!("  hauteur max atteinte : {ymax:.2}  (le mur monte jusqu'à y=10)");
        eprintln!("  hauteur au franchissement de x=12.5 : {y_au_passage:.2}");
        eprintln!("  grille haute de {} → au-dessus, get_tile rend Air", grid.height);
    }
}

#[cfg(test)]
mod sonde2 {
    use super::*;

    /// Sonde : la solution trouvée sur SA manche reste-t-elle DANS la carte ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn la_solution_reste_t_elle_dans_la_carte() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/manches");
        let m = crate::boite_noire::charger_manche(
            &base.join("manche-002-arrive-CONTRADICTION.txt"),
        )
        .unwrap();
        let traps = TrapManager::new();
        let Verdict::Franchissable(s) = resoudre_par_appuis(&m.grid, &traps, 40_000) else {
            eprintln!("  pas trouvé");
            return;
        };
        let mut j = Player::new(m.grid.start_pos);
        let mut prec = false;
        let (mut ymax, mut hors) = (f32::MIN, 0usize);
        for cmd in &s.entrees {
            j.update(PAS, &cmd.vers_entrees(prec), &m.grid, &traps);
            prec = cmd.saut;
            ymax = ymax.max(j.position.y);
            if j.position.y + Player::HEIGHT > m.grid.height as f32 {
                hors += 1;
            }
        }
        eprintln!("  carte haute de {} tuiles", m.grid.height);
        eprintln!("  hauteur max de la solution du TAS : {ymax:.2}");
        eprintln!("  images passées HORS carte (au-dessus) : {hors} / {}", s.entrees.len());

        // Et l'humain, lui ?
        let mut yh = f32::MIN;
        for p in &m.positions {
            yh = yh.max(p.y);
        }
        eprintln!("  hauteur max de l'HUMAIN sur la même carte : {yh:.2}");
    }
}

#[cfg(test)]
mod sonde3 {
    use super::*;
    use crate::grid::TileType;

    /// Sonde : jusqu'à quelle hauteur de mur le répertoire de manœuvres passe-t-il ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn hauteur_de_mur_franchie_par_les_manoeuvres() {
        let plafond_txt = std::env::var("PLAFOND").unwrap_or_else(|_| "1000".into());
        #[allow(non_snake_case)]
        let PLAFOND_SONDE: usize = plafond_txt.parse().unwrap();
        for h in 1..=5 {
            let mut grid = TileGrid::vide(24, 10);
            for x in 0..24 {
                grid.set_tile(x, 0, TileType::SolidBlock);
            }
            for y in 1..=h {
                grid.set_tile(12, y, TileType::SolidBlock);
            }
            let traps = TrapManager::new();
            let par_appuis = resoudre_par_appuis(&grid, &traps, PLAFOND_SONDE);
            let ancien = crate::tas::resoudre_image_par_image(&grid, &traps, 60_000);
            eprintln!(
                "  mur de {h} : manœuvres = {:<12} image-par-image = {}",
                match &par_appuis {
                    Verdict::Franchissable(s) => format!("PASSE ({} img)", s.entrees.len()),
                    Verdict::PasTrouve { .. } => "bloqué".to_string(),
                },
                match &ancien {
                    Verdict::Franchissable(s) => format!("PASSE ({} img)", s.entrees.len()),
                    Verdict::PasTrouve { .. } => "bloqué".to_string(),
                }
            );
        }
    }
}

#[cfg(test)]
mod sonde4 {
    use super::*;
    use crate::grid::TileType;

    /// Sonde : quel GESTE l'ancien solveur emploie-t-il pour franchir un mur de 3 ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn quel_geste_franchit_le_mur_de_trois() {
        let mut grid = TileGrid::vide(24, 10);
        for x in 0..24 {
            grid.set_tile(x, 0, TileType::SolidBlock);
        }
        for y in 1..=3 {
            grid.set_tile(12, y, TileType::SolidBlock);
        }
        let traps = TrapManager::new();
        let Verdict::Franchissable(s) =
            crate::tas::resoudre_image_par_image(&grid, &traps, 60_000)
        else {
            eprintln!("  l'ancien ne trouve pas non plus");
            return;
        };
        let mut j = Player::new(grid.start_pos);
        let mut prec = false;
        eprintln!("  départ ({:.2}, {:.2}) → arrivée ({:.2}, {:.2})",
            grid.start_pos.x, grid.start_pos.y, grid.finish_pos.x, grid.finish_pos.y);
        for (i, m) in s.entrees.iter().enumerate() {
            let avant = j.state;
            j.update(PAS, &m.vers_entrees(prec), &grid, &traps);
            prec = m.saut;
            // On n'imprime que ce qui compte : les changements d'état.
            if j.state != avant {
                eprintln!(
                    "  img {i:3} [{}{}{}] pos=({:5.2},{:5.2}) v=({:6.2},{:6.2}) {:?} → {:?}",
                    if m.gauche { "G" } else { "." },
                    if m.droite { "D" } else { "." },
                    if m.saut { "S" } else { "." },
                    j.position.x, j.position.y, j.velocity.x, j.velocity.y, avant, j.state
                );
            }
        }
    }
}

#[cfg(test)]
mod sonde5 {
    use super::*;
    use crate::grid::TileType;

    /// Sonde : le franchissement d'un mur de 3 est-il IMITABLE, ou un exploit d'une image ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn le_mur_de_trois_est_il_imitable() {
        for h in 1..=3 {
            let mut grid = TileGrid::vide(24, 10);
            for x in 0..24 {
                grid.set_tile(x, 0, TileType::SolidBlock);
            }
            for y in 1..=h {
                grid.set_tile(12, y, TileType::SolidBlock);
            }
            let traps = TrapManager::new();
            let Verdict::Franchissable(s) =
                crate::tas::resoudre_image_par_image(&grid, &traps, 60_000)
            else {
                eprintln!("  mur de {h} : pas de solution");
                continue;
            };
            let simple = crate::tas::simplifier(&grid, &traps, &s);
            let r_brute = crate::tas::robustesse(&grid, &traps, &s.entrees, 200);
            let r_simple = crate::tas::robustesse(&grid, &traps, &simple.entrees, 200);
            eprintln!(
                "  mur de {h} : robustesse brute {:.0} %   simplifiée {:.0} %   \
                 (décalage humain de ±{} images)",
                r_brute * 100.0,
                r_simple * 100.0,
                crate::tas::IMPRECISION_HUMAINE
            );
        }
    }
}

#[cfg(test)]
mod sonde6 {
    use super::*;

    /// Sonde : combien d'images une manœuvre coûte-t-elle RÉELLEMENT, en moyenne ?
    #[test]
    #[ignore = "sonde de mesure"]
    fn cout_reel_moyen_d_une_manoeuvre() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/manches");
        let m = crate::boite_noire::charger_manche(
            &base.join("manche-002-arrive-CONTRADICTION.txt"),
        )
        .unwrap();
        let traps = TrapManager::new();
        let joueur = Player::new(m.grid.start_pos);
        let (mut total, mut n) = (0usize, 0usize);
        for genre in [Genre::Sol, Genre::Mur(true)] {
            for p in repertoire(genre) {
                for abo in derouler(&joueur, &m.grid, &traps, p, false) {
                    total += abo.entrees.len();
                    n += 1;
                }
            }
        }
        eprintln!("  {n} déroulements, {total} images simulées → {:.1} images/manœuvre", total as f32 / n as f32);
        eprintln!("  répertoire : {} au sol, {} au mur",
            repertoire(Genre::Sol).len(), repertoire(Genre::Mur(true)).len());
    }
}
