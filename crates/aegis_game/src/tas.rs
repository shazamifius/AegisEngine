//! tas.rs — le solveur de parcours : **cette carte est-elle franchissable, et comment ?**
//!
//! # À quoi il sert dans le jeu
//!
//! Deux usages, et ils ne demandent pas la même chose :
//!
//! 1. **Avant la course** — vérifier qu'une carte que les joueurs viennent de piéger reste
//!    franchissable. Si elle ne l'est plus, un vote retire le ou les blocs qui bouchent.
//! 2. **Après une manche où personne n'a réussi** — montrer un parcours qui marche, pour que
//!    tout le monde voie *comment on fait*.
//!
//! Le second usage change tout : la solution n'a pas à être la plus **rapide**, elle doit être
//! la plus **imitable**. Un parcours au pixel près, impossible à refaire, ne montre rien.
//!
//! # Ce qui rend ce solveur honnête
//!
//! Il ne juge jamais sur un modèle approché : il rejoue **la vraie physique du jeu**
//! ([`Player::update`]), image par image, avec les vraies entrées. La recherche, elle, prend des
//! raccourcis — elle quantifie l'état pour reconnaître qu'elle est « déjà passée par là ».
//!
//! Cette asymétrie est délibérée et c'est elle qui rend le résultat sûr :
//!
//! * **une solution trouvée est VRAIE**, puisqu'elle est rejouable telle quelle et vérifiée par
//!   la physique du jeu — aucun raccourci de recherche ne peut fabriquer un faux succès ;
//! * **un échec n'est PAS une preuve d'impossibilité.** Il dit « je n'ai pas trouvé dans ce
//!   budget, avec cette quantification ». C'est pourquoi le verdict s'appelle
//!   [`Verdict::PasTrouve`] et non « impossible » : nommer ce résultat « impossible » ferait
//!   retirer des blocs parfaitement franchissables sur la foi d'une recherche trop courte.
//!
//! # Pourquoi HashLife ne se transpose pas ici
//!
//! HashLife (Gosper, 1984) mémoïse des macro-cellules d'un automate **discret** : deux régions
//! identiques ont un futur identique, donc on saute des générations entières. Ce jeu n'a rien de
//! discret — `position` et `velocity` sont des `f32`, et le comportement dépend en plus de
//! `stored_fall_momentum`, `boost_window_timer`, `jump_buffer`, `coyote_timer`,
//! `wall_cooldown`… Deux états visuellement identiques peuvent diverger complètement.
//!
//! Ce qu'on garde de l'idée : **reconnaître un état déjà vu pour ne pas l'explorer deux fois**.
//! C'est exactement le rôle de [`Empreinte`] — mais elle *approxime*, là où HashLife est exact.

use aegis_engine::math::Vec2;
use std::collections::{BinaryHeap, HashMap};

use crate::grid::TileGrid;
use crate::player::{InputState, Player, PlayerState};
use crate::traps::TrapManager;

/// Pas de simulation. Fixe, et c'est indispensable : la physique du jeu n'est déterministe qu'à
/// pas constant, et un TAS qui ne se rejoue pas à l'identique ne prouve rien.
pub const PAS: f32 = 1.0 / 60.0;

/// Ce qu'un joueur peut demander à un instant donné.
///
/// Volontairement réduit à trois touches : `up`/`down`/`crouch` ne changent pas le franchissement
/// d'un parcours, et chaque touche ajoutée multiplie l'arbre à explorer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Manette {
    pub gauche: bool,
    pub droite: bool,
    pub saut: bool,
}

impl Manette {
    /// Les six commandes qui ont un sens. « Gauche **et** droite » n'y figure pas : les deux
    /// s'annulent, et l'inclure doublerait l'arbre pour rien.
    pub const REPERTOIRE: [Manette; 6] = [
        Manette { gauche: false, droite: false, saut: false },
        Manette { gauche: false, droite: true, saut: false },
        Manette { gauche: true, droite: false, saut: false },
        Manette { gauche: false, droite: false, saut: true },
        Manette { gauche: false, droite: true, saut: true },
        Manette { gauche: true, droite: false, saut: true },
    ];

    /// Traduit vers les entrées du jeu. `jump_pressed_this_frame` est posé au **front montant**
    /// du saut : c'est ce que fait un vrai clavier, et le jeu s'en sert pour le tampon de saut.
    fn vers_entrees(self, saut_precedent: bool) -> InputState {
        InputState {
            left: self.gauche,
            right: self.droite,
            jump: self.saut,
            jump_pressed_this_frame: self.saut && !saut_precedent,
            ..Default::default()
        }
    }
}

/// Une séquence d'entrées qui mène à l'arrivée.
#[derive(Clone, Debug)]
pub struct Solution {
    pub entrees: Vec<Manette>,
}

impl Solution {
    /// Combien de temps ce parcours prend, en secondes.
    pub fn duree(&self) -> f32 {
        self.entrees.len() as f32 * PAS
    }

    /// Combien de fois la commande change au fil du parcours.
    ///
    /// Sert de mesure grossière de difficulté : un parcours qui demande cent changements de
    /// touche en deux secondes n'est pas imitable, quelle que soit sa durée.
    pub fn changements(&self) -> usize {
        self.entrees.windows(2).filter(|p| p[0] != p[1]).count()
    }
}

/// Ce que le solveur peut répondre.
#[derive(Debug)]
pub enum Verdict {
    /// Un parcours existe, et le voici — rejouable tel quel.
    Franchissable(Solution),
    /// Rien trouvé dans le budget accordé. **Ce n'est pas une preuve d'impossibilité** : voir
    /// la note en tête de module.
    PasTrouve { explores: usize },
}

/// L'état du joueur, réduit à ce qui permet de dire « je suis déjà passé par là ».
///
/// # Le compromis, et il est assumé
/// Les minuteurs internes (tampon de saut, coyote, mur, élan de chute) sont **ignorés**. Deux
/// états de même empreinte peuvent donc diverger : la recherche risque d'écarter un chemin qui
/// aurait marché.
///
/// C'est un compromis sur la **complétude**, jamais sur la **justesse** — une solution trouvée
/// reste rejouée par la vraie physique avant d'être rendue.
///
/// # ⚠ La maille, et l'erreur qu'elle a coûtée
/// Première version : position au quart d'unité, vitesse à l'unité. Le solveur ne franchissait
/// **même pas un couloir plat** — treize états explorés puis plus rien, sur un budget de
/// soixante mille.
///
/// La raison n'a rien d'exotique : au démarrage, le joueur avance de **0,012 unité par image**
/// (0,7 u/s à 60 Hz). Il lui faut une vingtaine d'images pour franchir une maille de 0,25 —
/// pendant lesquelles tous les états portent la même empreinte. La recherche les prenait donc
/// pour le même endroit, gardait le premier (le moins avancé) et coupait tous les autres. Le
/// chemin « courir à droite » mourait à la deuxième image.
///
/// **Règle qui en sort, et elle vaut pour toute recherche sur un monde continu :** la maille
/// doit être plus FINE que ce que l'état parcourt en un pas, sinon la recherche confond
/// « progresser » et « faire du surplace ». D'où le huitième d'unité et le quart d'unité par
/// seconde ci-dessous.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Empreinte {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    au_sol: bool,
}

impl Empreinte {
    fn de(joueur: &Player) -> Empreinte {
        Empreinte {
            x: (joueur.position.x * 8.0).round() as i32,
            y: (joueur.position.y * 8.0).round() as i32,
            vx: (joueur.velocity.x * 4.0).round() as i32,
            vy: (joueur.velocity.y * 4.0).round() as i32,
            au_sol: joueur.state == PlayerState::OnGround,
        }
    }
}

/// Un nœud de la file de recherche. Ordonné pour que le plus prometteur sorte en premier.
struct Piste {
    cout: u32,     // images déjà dépensées
    estime: u32,   // cout + distance restante estimée
    joueur: Player,
    saut_precedent: bool,
    entrees: Vec<Manette>,
}

impl PartialEq for Piste {
    fn eq(&self, autre: &Self) -> bool {
        self.estime == autre.estime
    }
}
impl Eq for Piste {}
impl Ord for Piste {
    fn cmp(&self, autre: &Self) -> std::cmp::Ordering {
        // `BinaryHeap` sort le plus GRAND : on inverse pour obtenir le plus petit estimé.
        autre.estime.cmp(&self.estime)
    }
}
impl PartialOrd for Piste {
    fn partial_cmp(&self, autre: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(autre))
    }
}

/// À quelle distance de l'arrivée on considère la ligne franchie. Même valeur que le jeu.
pub const RAYON_ARRIVEE: f32 = 1.2;

/// Cherche un parcours de `grid.start_pos` à `grid.finish_pos`.
///
/// `budget` borne le nombre d'états explorés — c'est ce qui garantit que l'appel rend la main,
/// y compris sur une carte sans issue.
pub fn resoudre(grid: &TileGrid, budget: usize) -> Verdict {
    resoudre_avec(grid, &TrapManager::new(), budget)
}

/// Le même, en tenant compte des pièges posés par les joueurs.
pub fn resoudre_avec(grid: &TileGrid, traps: &TrapManager, budget: usize) -> Verdict {
    let arrivee = grid.finish_pos;

    // Estimation du reste : distance à vol d'oiseau, convertie en images à la vitesse de course.
    //
    // ⚠ Elle est **pondérée**, et c'est délibéré. Une recherche non pondérée cherche le chemin
    // le plus COURT et explore donc une nuée d'états équivalents avant d'avancer — sur un simple
    // couloir plat, soixante mille états n'y suffisaient pas. Or ce solveur ne cherche pas le
    // parcours le plus rapide : il cherche un parcours **franchissable et imitable**. Rien ne
    // justifie de payer l'optimalité en temps, et la payer empêchait de trouver quoi que ce soit.
    //
    // Le poids pousse la recherche vers l'arrivée plutôt que de ratisser autour du départ.
    const POIDS: f32 = 4.0;
    /// Vitesse de course réelle du personnage, mesurée : 8,5 unités par seconde.
    const VITESSE: f32 = 8.5;
    let estimer = |p: Vec2| -> u32 {
        let d = (p - arrivee).length();
        (d / VITESSE / PAS * POIDS).max(0.0) as u32
    };

    let depart = Player::new(grid.start_pos);
    let mut file = BinaryHeap::new();
    file.push(Piste {
        cout: 0,
        estime: estimer(depart.position),
        joueur: depart,
        saut_precedent: false,
        entrees: Vec::new(),
    });

    let mut vus: HashMap<Empreinte, u32> = HashMap::new();
    let mut explores = 0usize;

    while let Some(piste) = file.pop() {
        if explores >= budget {
            break;
        }
        explores += 1;

        for commande in Manette::REPERTOIRE {
            let mut joueur = piste.joueur.clone();
            joueur.update(PAS, &commande.vers_entrees(piste.saut_precedent), grid, traps);

            // Mort : ce chemin ne mène nulle part, on ne le prolonge pas.
            if joueur.state == PlayerState::Dead {
                continue;
            }
            // Tombé sous la carte sans être encore déclaré mort.
            if joueur.position.y < grid.get_void_kill_y() {
                continue;
            }

            let mut entrees = piste.entrees.clone();
            entrees.push(commande);

            if (joueur.position - arrivee).length() < RAYON_ARRIVEE {
                return Verdict::Franchissable(Solution { entrees });
            }

            let cout = piste.cout + 1;
            let empreinte = Empreinte::de(&joueur);
            // On ne reprend un état déjà vu que si on y arrive plus tôt.
            match vus.get(&empreinte) {
                Some(&deja) if deja <= cout => continue,
                _ => {
                    vus.insert(empreinte, cout);
                }
            }

            file.push(Piste {
                cout,
                estime: cout + estimer(joueur.position),
                joueur,
                saut_precedent: commande.saut,
                entrees,
            });
        }
    }

    Verdict::PasTrouve { explores }
}

/// Rejoue une séquence d'entrées et dit si elle atteint réellement l'arrivée.
///
/// C'est le **contrôle** du solveur : ce qu'il propose doit passer ici, sinon il ment. Un
/// solveur qui se juge lui-même sur son propre modèle n'a aucune valeur — c'est la physique du
/// jeu, et elle seule, qui a le dernier mot.
pub fn rejouer(grid: &TileGrid, traps: &TrapManager, entrees: &[Manette]) -> bool {
    let mut joueur = Player::new(grid.start_pos);
    let mut saut_precedent = false;

    for commande in entrees {
        joueur.update(PAS, &commande.vers_entrees(saut_precedent), grid, traps);
        saut_precedent = commande.saut;

        if joueur.state == PlayerState::Dead {
            return false;
        }
        if (joueur.position - grid.finish_pos).length() < RAYON_ARRIVEE {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Le socle de tout le reste.** Sans déterminisme, une séquence d'entrées ne veut rien
    /// dire : elle donnerait un résultat différent à chaque exécution, et le TAS entier
    /// s'effondrerait. On rejoue deux fois la même séquence et on exige le même état, au bit près.
    #[test]
    fn la_physique_du_jeu_est_deterministe_a_pas_fixe() {
        let grid = TileGrid::new(40, 22);
        let traps = TrapManager::new();

        // Une séquence variée : course, sauts, demi-tours, relâchements.
        let sequence: Vec<Manette> = (0..300)
            .map(|i| Manette::REPERTOIRE[(i * 7 + i / 13) % Manette::REPERTOIRE.len()])
            .collect();

        let jouer = || {
            let mut j = Player::new(grid.start_pos);
            let mut precedent = false;
            for c in &sequence {
                j.update(PAS, &c.vers_entrees(precedent), &grid, &traps);
                precedent = c.saut;
            }
            (j.position, j.velocity, j.state)
        };

        let (p1, v1, e1) = jouer();
        let (p2, v2, e2) = jouer();

        assert_eq!(p1.x.to_bits(), p2.x.to_bits(), "la position doit etre identique au bit pres");
        assert_eq!(p1.y.to_bits(), p2.y.to_bits());
        assert_eq!(v1.x.to_bits(), v2.x.to_bits(), "la vitesse aussi");
        assert_eq!(v1.y.to_bits(), v2.y.to_bits());
        assert_eq!(e1, e2);
    }

    /// Le répertoire ne doit contenir ni doublon, ni la commande contradictoire.
    #[test]
    fn le_repertoire_de_commandes_est_sain() {
        for (i, a) in Manette::REPERTOIRE.iter().enumerate() {
            assert!(!(a.gauche && a.droite), "gauche+droite s'annulent, inutile de l'explorer");
            for b in &Manette::REPERTOIRE[i + 1..] {
                assert_ne!(a, b, "deux commandes identiques doublent l'arbre pour rien");
            }
        }
    }

    /// Le front montant du saut doit être vu UNE fois, pas à chaque image où la touche est tenue.
    #[test]
    fn le_saut_ne_se_declenche_qu_au_front_montant() {
        let saut = Manette { gauche: false, droite: false, saut: true };
        assert!(saut.vers_entrees(false).jump_pressed_this_frame, "touche enfoncee : front montant");
        assert!(!saut.vers_entrees(true).jump_pressed_this_frame, "touche tenue : plus de front");
    }

    /// Une séquence vide ne franchit rien — le contrôle ne doit pas se laisser convaincre par
    /// un solveur qui rendrait une solution creuse.
    #[test]
    fn le_controle_refuse_une_sequence_qui_n_arrive_nulle_part() {
        let grid = TileGrid::new(40, 22);
        assert!(!rejouer(&grid, &TrapManager::new(), &[]));
    }

    /// Construit un couloir plat : un sol continu, rien d'autre.
    fn couloir(largeur: usize) -> TileGrid {
        let mut grid = TileGrid::vide(largeur, 10);
        for x in 0..largeur {
            grid.set_tile(x, 0, crate::grid::TileType::SolidBlock);
        }
        grid
    }

    /// **Le témoin du solveur** : il trouve un chemin, ET ce chemin passe le contrôle.
    ///
    /// Les deux moitiés comptent. Un solveur qui rend une séquence sans qu'on la rejoue peut
    /// affirmer n'importe quoi : c'est la physique du jeu qui doit avoir le dernier mot, pas
    /// le modèle de recherche.
    #[test]
    fn le_solveur_franchit_un_couloir_plat_et_sa_solution_passe_le_controle() {
        let grid = couloir(20);
        match resoudre(&grid, 60_000) {
            Verdict::Franchissable(s) => {
                assert!(!s.entrees.is_empty());
                assert!(
                    rejouer(&grid, &TrapManager::new(), &s.entrees),
                    "la solution proposee doit franchir la ligne quand on la REJOUE"
                );
                assert!(s.duree() > 0.0);
            }
            Verdict::PasTrouve { explores } => {
                panic!("un couloir plat doit se franchir ({explores} etats explores)")
            }
        }
    }

    /// Un budget minuscule doit rendre la main sans mentir : « pas trouve », jamais « impossible ».
    #[test]
    fn un_budget_epuise_dit_PAS_TROUVE_et_non_IMPOSSIBLE() {
        let grid = couloir(60);
        match resoudre(&grid, 3) {
            Verdict::PasTrouve { explores } => assert!(explores <= 3),
            Verdict::Franchissable(_) => {
                // Acceptable si la carte est triviale, mais pas avec trois etats explores.
                panic!("trois etats ne peuvent pas suffire a traverser soixante cases")
            }
        }
    }

    /// Le premier vrai usage : la carte est piegee, reste-t-elle franchissable ?
    ///
    /// Un trou de deux cases au milieu du couloir. Il faut sauter — donc le solveur doit trouver
    /// une sequence qui contient un saut, et pas seulement « courir a droite ».
    #[test]
    fn le_solveur_saute_par_dessus_un_trou() {
        let mut grid = couloir(24);
        grid.set_tile(11, 0, crate::grid::TileType::Air);
        grid.set_tile(12, 0, crate::grid::TileType::Air);

        match resoudre(&grid, 400_000) {
            Verdict::Franchissable(s) => {
                assert!(
                    rejouer(&grid, &TrapManager::new(), &s.entrees),
                    "la solution doit franchir le trou quand on la REJOUE"
                );
                assert!(
                    s.entrees.iter().any(|c| c.saut),
                    "on ne traverse pas un trou de deux cases sans sauter"
                );
            }
            Verdict::PasTrouve { explores } => {
                panic!("un trou de deux cases se saute ({explores} etats explores)")
            }
        }
    }

    /// Le second usage : un passage VRAIMENT bouche. Le solveur doit epuiser son budget sans
    /// jamais pretendre avoir trouve — c'est ce verdict qui declenchera le vote pour retirer
    /// un bloc.
    #[test]
    fn un_mur_infranchissable_ne_produit_jamais_de_fausse_solution() {
        let mut grid = couloir(24);
        // Un mur du sol au plafond, sur toute la hauteur : rien ne passe.
        for y in 1..10 {
            grid.set_tile(12, y, crate::grid::TileType::SolidBlock);
        }

        match resoudre(&grid, 150_000) {
            Verdict::PasTrouve { .. } => {}
            Verdict::Franchissable(s) => {
                // Si le solveur pretend avoir trouve, le controle doit le confondre.
                assert!(
                    !rejouer(&grid, &TrapManager::new(), &s.entrees),
                    "le solveur a rendu une solution QUI PASSE a travers un mur plein"
                );
                panic!("solution rendue sur une carte bouchee (le controle l'a refusee)");
            }
        }
    }
}
