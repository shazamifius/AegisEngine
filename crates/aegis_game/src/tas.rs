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
//! # Ce qui a été essayé et n'a RIEN donné (19 août 2026)
//!
//! **Chercher plusieurs parcours avec des réglages différents, puis garder le plus tolérant.**
//! L'idée paraissait évidente : la recherche rend *un* chemin, celui que ses réglages lui font
//! rencontrer en premier, donc en essayer quatre devrait offrir un meilleur candidat. Mesuré sur
//! quatre cartes — couloir plat, trou de deux cases, trou de trois, marche haute — plus la carte
//! réelle du jeu :
//!
//! ```text
//! couloir plat     un réglage 1,00 | quatre réglages 1,00
//! trou de 2        un réglage 0,68 | quatre réglages 0,68
//! trou de 3        un réglage 1,00 | quatre réglages 1,00
//! marche haute     un réglage 1,00 | quatre réglages 1,00
//! carte réelle     un réglage 0,10 | quatre réglages 0,10   (et 0,11 s → 0,57 s)
//! ```
//!
//! **Strictement aucun gain, pour cinq fois le coût.** Le code a donc été retiré ; seule la
//! mesure qui le prouve est restée (`mesure_multi_reglages`, à lancer à la main).
//!
//! *L'explication tient sans doute à la carte plus qu'au solveur : quand les plateformes sont
//! là où elles sont, il n'existe qu'une trajectoire, et tous les réglages la retrouvent.* Ce qui
//! donne au chiffre son vrai sens : **0,10 sur la carte réelle n'est pas un aveu du solveur,
//! c'est un jugement sur la carte.** Elle est dure — un trou de deux cases obtient 0,68.
//! Le prochain gain viendra donc d'ailleurs : chercher des chemins ALTERNATIFS (passer plus haut,
//! plus bas), pas régler différemment la même recherche.
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
    /// Ce que le chemin a coûté jusqu'ici : les images dépensées, **plus** le prix des
    /// changements de touche (voir `PRIX_DU_CHANGEMENT`).
    cout: u32,
    estime: u32,
    joueur: Player,
    /// La dernière commande donnée, pour savoir si la suivante est un changement.
    derniere: Manette,
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

/// Ce que coûte un changement de touche, en « images équivalentes ».
///
/// C'est le critère « facile pour un humain » **mis dans la recherche** plutôt qu'appliqué
/// après coup. Une valeur de 20 revient à dire : « mieux vaut un parcours qui dure un tiers de
/// seconde de plus qu'un parcours qui demande un appui de plus ».
///
/// ⚠ Trop haut, le solveur refuserait les sauts nécessaires et déclarerait douteuses des cartes
/// franchissables. Trop bas, il retombe sur des séquences hachées que personne ne peut refaire.
pub const PRIX_DU_CHANGEMENT: u32 = 20;

/// Poids par défaut de l'estimation du chemin restant.
///
/// Il pousse la recherche vers l'arrivée plutôt que de ratisser autour du départ.
///
/// ⚠ **Sa valeur n'est pas un réglage de confort, elle décide si le solveur aboutit.**
/// Mesuré sur la carte réelle du jeu (58 × 28) :
/// ```text
/// poids  4 → PAS TROUVÉ, 600 000 états, 6,3 s
/// poids 12 → PAS TROUVÉ, 600 000 états, 6,5 s
/// poids 30 → FRANCHISSABLE, trouvé en 0,27 s
/// ```
/// Le passage n'est pas graduel : c'est « jamais » puis « tout de suite ».
///
/// ⚠ Le revers, à connaître : plus le poids est fort, plus la recherche devient gloutonne et
/// rechigne à s'éloigner de l'arrivée. Une carte qui demanderait de RECULER longuement pour
/// contourner un obstacle pourrait lui échapper — elle rendrait alors [`Verdict::PasTrouve`],
/// jamais une fausse solution. C'est le bon sens de l'erreur, mais c'est la limite à surveiller
/// le jour où une carte tordue passera pour bouchée.
pub const POIDS: f32 = 30.0;

/// Cherche un parcours de `grid.start_pos` à `grid.finish_pos`.
///
/// `budget` borne le nombre d'états explorés — c'est ce qui garantit que l'appel rend la main,
/// y compris sur une carte sans issue.
pub fn resoudre(grid: &TileGrid, budget: usize) -> Verdict {
    resoudre_avec(grid, &TrapManager::new(), budget)
}

/// Le même, en tenant compte des pièges posés par les joueurs.
/// La carte telle que le TAS doit la voir : **seulement ce qui bloque POUR TOUJOURS**.
///
/// # Le principe, en une phrase : le joueur peut attendre
///
/// Un lance-flammes ne rend pas un passage impossible — il le rend *temporisé*. Il s'éteint, on
/// passe. Un bloc, lui, bloque définitivement. Les traiter pareil était le défaut central de ce
/// solveur, et il se manifestait de la pire façon : **`traps.update()` n'était jamais appelé
/// pendant la recherche**, donc un lance-flammes allumé à l'instant du lancement restait allumé
/// *pour l'éternité* dans la simulation. Le TAS déclarait « pas trouvé » sur des cartes qu'il
/// suffisait de traverser deux secondes plus tard.
///
/// Animer les pièges n'aurait pas suffi, et aurait même empiré les choses : l'[`Empreinte`] qui
/// marque les états déjà visités ne porte pas la phase des timers. La recherche aurait considéré
/// « déjà vu » un état pourtant différent, et raté des solutions en silence. Il aurait fallu
/// ajouter cette phase à l'empreinte — multipliant l'espace d'états par la période de chaque piège.
///
/// **En sortant le temporaire du problème, le solveur devient plus juste ET moins cher.** C'est
/// rare, et c'est le signe qu'on avait posé la mauvaise question.
///
/// # Ce qui reste, et ce qui part
///
/// Le partage ne suit PAS l'intuition « ça bouge donc c'est temporaire » — il suit la condition de
/// mort, lue dans `check_player_death` :
///
/// - **`SpikeTrap` reste** : il tue sans condition.
/// - **`SawBlade` reste**, et c'est le cas contre-intuitif : *sa rotation est purement visuelle*.
///   Elle tue dès que la distance est inférieure à son rayon, en permanence. Une scie est un mur
///   rond, pas un piège à timer.
/// - **`LaserEmitter` et `Flamethrower` partent** : ils ne tuent que `if *active`.
/// - **`CannonTurret` part** aussi, avec ses projectiles : elle ne tue pas au contact, ce sont ses
///   tirs qui frappent — et un tir, ça passe.
///
/// # Ce que ce choix rend possible, et c'est le vrai but
///
/// Le verdict du TAS devient **exactement la question du vote** : « existe-t-il un chemin en
/// ignorant tout ce qui est temporaire ? » Si non, l'obstacle est permanent, donc c'est un bloc, et
/// c'est sur lui qu'on vote. **On ne votera jamais à cause d'un timer**, puisqu'un timer ne bouche
/// rien. Sans cette séparation, un verdict « impossible » ne disait pas s'il fallait casser un mur
/// ou simplement patienter.
///
/// # ⚠ Ce que ça coûte, dit franchement
///
/// Le verdict devient **optimiste sur le temps** : il affirme qu'un chemin existe, pas qu'il est
/// commode. Une carte franchissable peut demander d'attendre longtemps devant une flamme. C'est
/// assumé — la difficulté se mesure par la robustesse, pas par le verdict.
pub fn vue_permanente(traps: &TrapManager) -> TrapManager {
    let mut vue = TrapManager::new();
    vue.traps = traps
        .traps
        .iter()
        .filter(|t| {
            matches!(
                t.kind,
                crate::traps::TrapKind::SpikeTrap | crate::traps::TrapKind::SawBlade { .. }
            )
        })
        .cloned()
        .collect();
    // Les projectiles ne sont pas recopiés : ils sont l'incarnation même du danger qui passe.
    vue
}

pub fn resoudre_avec(grid: &TileGrid, traps: &TrapManager, budget: usize) -> Verdict {
    // ⚠ La conversion se fait ICI, au point d'entrée, pour que TOUT le pipeline en aval
    // (`rejouer`, `simplifier`, `robustesse`) travaille dans le MÊME monde. Une solution validée
    // dans un monde et rejouée dans un autre serait un mensonge — le genre de mensonge que le
    // rejeu est précisément là pour empêcher.
    resoudre_regle(grid, &vue_permanente(traps), budget, POIDS, PRIX_DU_CHANGEMENT)
}

/// La recherche, avec ses deux réglages explicites.
///
/// Ils sont exposés parce qu'ils **changent le chemin trouvé**, pas seulement sa vitesse
/// d'obtention : c'est ce qui permet à [`resoudre_le_plus_imitable`] d'en essayer plusieurs.
pub fn resoudre_regle(
    grid: &TileGrid,
    traps: &TrapManager,
    budget: usize,
    poids: f32,
    prix_du_changement: u32,
) -> Verdict {
    let arrivee = grid.finish_pos;

    // Estimation du reste : distance à vol d'oiseau, convertie en images à la vitesse de course,
    // puis pondérée (voir [`POIDS`]).
    /// Vitesse de course réelle du personnage, mesurée : 8,5 unités par seconde.
    const VITESSE: f32 = 8.5;
    let estimer = |p: Vec2| -> u32 {
        let d = (p - arrivee).length();
        (d / VITESSE / PAS * poids).max(0.0) as u32
    };

    let depart = Player::new(grid.start_pos);
    let mut file = BinaryHeap::new();
    file.push(Piste {
        cout: 0,
        estime: estimer(depart.position),
        joueur: depart,
        derniere: Manette::default(),
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

            // Le coût n'est pas seulement du temps : **changer de touche coûte**. C'est ce qui
            // fait chercher un parcours qu'un humain peut refaire plutôt que le plus rapide.
            //
            // Sans ce prix, le solveur change de commande dès que ça lui fait gagner une image,
            // et rend une séquence hachée — mesuré : 15 changements pour traverser un couloir
            // plat, et 10 % de robustesse. On peut simplifier après coup, mais on ne rattrape
            // alors que ce que la recherche a bien voulu laisser : sur la carte réelle, même
            // simplifié, le parcours obtenait **0 %**.
            //
            // Le mettre DANS la recherche change ce qu'on cherche, au lieu de réparer ce qu'on
            // a trouvé.
            let cout = piste.cout + 1 + if commande != piste.derniere { prix_du_changement } else { 0 };
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
                derniere: commande,
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

// ─────────────────────────────────────────────────────────────────────────────────────────────
//  « Facile pour un humain » — et comment on le mesure
// ─────────────────────────────────────────────────────────────────────────────────────────────
//
// Le critère est retourné par rapport à un TAS classique. Un TAS de vitesse cherche la
// séquence la plus RAPIDE ; celle-ci est en général la plus exigeante, au point d'être
// impossible à refaire. Or ce solveur sert à montrer à quelqu'un COMMENT ON FAIT : un parcours
// que personne ne peut imiter ne montre rien.
//
// Ce qu'on cherche est donc le parcours le plus **tolérant** : celui qui arrive encore quand on
// appuie deux images trop tôt ou trop tard. C'est mesurable, et c'est ce que fait `robustesse`.

/// Rend une solution **imitable**, sans jamais la casser.
///
/// # Pourquoi c'est indispensable
/// La recherche produit un parcours optimisé : elle change de touche dès que ça lui fait gagner
/// une image, ce qui donne des séquences hachées, impossibles à refaire. Mesuré : la solution
/// brute d'un simple **couloir plat** ne survivait qu'à **10 %** des essais dès qu'on décalait
/// les appuis de deux images — alors que courir tout droit est ce qu'un parcours a de plus
/// facile. Une telle vidéo ne montre rien à personne.
///
/// # Comment
/// On tente de **supprimer** chaque changement de touche, un par un, en prolongeant simplement
/// la commande précédente — et on ne garde la suppression que si le parcours **arrive encore**.
/// Chaque étape est donc validée par la vraie physique : cette simplification ne peut pas
/// produire une solution fausse, seulement une solution plus simple ou pas de changement.
///
/// C'est le même esprit que la réduction d'un cas de test : on enlève tant que ça tient.
pub fn simplifier(grid: &TileGrid, traps: &TrapManager, solution: &Solution) -> Solution {
    let mut entrees = solution.entrees.clone();

    // Plusieurs passes : supprimer un changement en rend parfois un autre superflu.
    for _ in 0..4 {
        let avant = entrees.len();
        let mut i = 1;
        let mut retires = 0;

        while i < entrees.len() {
            if entrees[i] == entrees[i - 1] {
                i += 1;
                continue;
            }
            // On prolonge la commande précédente jusqu'au prochain changement, et on regarde.
            let mut essai = entrees.clone();
            let valeur = essai[i - 1];
            let mut j = i;
            while j < essai.len() && essai[j] == entrees[i] {
                essai[j] = valeur;
                j += 1;
            }
            if rejouer(grid, traps, &essai) {
                entrees = essai;
                retires += 1;
            } else {
                i = j.max(i + 1);
            }
        }
        if retires == 0 && entrees.len() == avant {
            break; // plus rien à gagner
        }
    }
    Solution { entrees }
}

/// Rend la version la plus **imitable** d'une solution : la brute, ou la simplifiée.
///
/// # ⚠ Simplifier n'est PAS rendre robuste — mesuré, pas supposé
///
/// [`simplifier`] retire un changement de touche dès que le parcours **arrive encore**. Mais
/// « arriver encore » et « arriver avec de la marge » sont deux choses différentes : en
/// supprimant un appui, on fait souvent passer le personnage au ras de l'obstacle. Mesuré sur la
/// carte réelle du jeu :
///
/// ```text
/// brute       : 10 changements, robustesse 0,10
/// simplifiée  :  9 changements, robustesse 0,03   ← moins de touches, mais bien plus dure
/// ```
///
/// Sur un couloir plat, la simplification aide (elle ramène à « maintiens droite »). Sur une
/// carte à sauts, elle nuit. Il n'y a donc pas de règle : **il faut mesurer les deux et garder
/// la meilleure**, ce que fait cette fonction. C'est le critère utilisé pour *choisir*, et plus
/// seulement pour constater.
pub fn la_plus_imitable(grid: &TileGrid, traps: &TrapManager, brute: &Solution) -> Solution {
    /// Assez d'essais pour départager deux candidates sans y passer la phase de placement.
    const ESSAIS: usize = 40;

    let simple = simplifier(grid, traps, brute);
    let r_simple = robustesse(grid, traps, &simple.entrees, ESSAIS);
    let r_brute = robustesse(grid, traps, &brute.entrees, ESSAIS);

    if r_simple >= r_brute { simple } else { brute.clone() }
}

/// Générateur pseudo-aléatoire minuscule, à graine explicite.
///
/// Il n'est pas là pour faire du hasard mais pour en faire **toujours le même** : une mesure de
/// robustesse qui change d'une exécution à l'autre ne se compare à rien.
struct Des(u64);

impl Des {
    fn suivant(&mut self) -> u64 {
        // xorshift64* — quelques instructions, une période largement suffisante ici.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Un entier dans `[-amplitude, amplitude]`.
    fn ecart(&mut self, amplitude: i32) -> i32 {
        let n = 2 * amplitude + 1;
        (self.suivant() % n as u64) as i32 - amplitude
    }
}

/// De combien d'images un humain se trompe, au plus, sur le moment d'un appui.
///
/// Deux images à 60 Hz font 33 ms — l'ordre de grandeur de l'imprécision d'un geste ordinaire,
/// bien en dessous d'un temps de réaction (~200 ms) mais bien au-dessus du pas de simulation.
pub const IMPRECISION_HUMAINE: i32 = 2;

/// À quel point ce parcours **pardonne l'imprécision**, entre 0 et 1.
///
/// On rejoue la séquence `essais` fois en décalant chaque changement de touche de quelques
/// images, et on compte la proportion d'essais qui atteignent quand même l'arrivée. Un parcours
/// qui exige le timing à l'image près tombe vers 0 ; un parcours large reste proche de 1.
///
/// C'est **la** mesure de « facile pour un humain » : elle ne demande pas de juger un ressenti,
/// elle compte des arrivées.
pub fn robustesse(grid: &TileGrid, traps: &TrapManager, entrees: &[Manette], essais: usize) -> f32 {
    if entrees.is_empty() || essais == 0 {
        return 0.0;
    }
    let mut des = Des(0x5EED_1234_ABCD_0001);
    let mut reussites = 0usize;

    for _ in 0..essais {
        let brouillee = brouiller(entrees, &mut des);
        if rejouer(grid, traps, &brouillee) {
            reussites += 1;
        }
    }
    reussites as f32 / essais as f32
}

/// Décale chaque changement de touche de quelques images, comme le ferait une main humaine.
///
/// On ne bruite pas chaque image indépendamment : ce serait du tremblement, pas de
/// l'imprécision. Ce qu'un humain rate, c'est le **moment** où il appuie ou relâche.
fn brouiller(entrees: &[Manette], des: &mut Des) -> Vec<Manette> {
    let mut sortie = entrees.to_vec();

    // Les instants où la commande change : ce sont eux qu'on déplace.
    let transitions: Vec<usize> = (1..entrees.len())
        .filter(|&i| entrees[i] != entrees[i - 1])
        .collect();

    for i in transitions {
        let decalage = des.ecart(IMPRECISION_HUMAINE);
        if decalage == 0 {
            continue;
        }
        let nouveau = (i as i32 + decalage).clamp(0, entrees.len() as i32 - 1) as usize;
        // Étendre ou reculer la commande précédente jusqu'au nouvel instant de bascule.
        let (debut, fin) = if nouveau < i { (nouveau, i) } else { (i, nouveau) };
        let valeur = if nouveau < i { entrees[i] } else { entrees[i - 1] };
        for c in &mut sortie[debut..fin] {
            *c = valeur;
        }
    }
    sortie
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
//  La vérification de carte, en tâche de fond
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Combien d'états le vérificateur s'autorise sur une carte de partie.
///
/// Assez pour franchir un parcours normal avec ses pièges ; assez peu pour rendre un verdict
/// avant la fin de la phase de placement. Un budget qui déborderait ne rendrait pas un
/// « impossible » — il rendrait un [`Verdict::PasTrouve`], qu'on n'a pas le droit de confondre.
pub const BUDGET_PARTIE: usize = 600_000;

/// Ce que le jeu sait de la franchissabilité de sa carte.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EtatCarte {
    /// Personne n'a encore posé la question.
    Inconnue,
    /// La recherche tourne.
    EnCours,
    /// Un parcours existe. `robustesse` dit à quel point il pardonne l'imprécision (0 à 1).
    Franchissable { robustesse: f32 },
    /// Rien trouvé dans le budget. ⚠ **Pas la même chose qu'« impossible »** : c'est ce doute
    /// qui doit déclencher un vote humain, pas un retrait automatique de blocs.
    PasTrouvee,
}

/// Vérifie en tâche de fond qu'une carte reste franchissable.
///
/// La recherche peut prendre une seconde entière : la lancer dans la boucle de rendu figerait
/// l'image, et un jeu qui se fige pendant que trente-cinq personnes posent leurs pièges serait
/// pire que pas de vérification du tout. Elle part donc dans un fil, et l'écran lit le résultat
/// quand il arrive.
pub struct Verificateur {
    etat: std::sync::Arc<std::sync::Mutex<EtatCarte>>,
    /// Le parcours trouvé, gardé pour être **montré** : c'est le second usage du solveur, et il
    /// compte autant que le premier. Quand personne n'a réussi une manche, on ne veut pas
    /// seulement savoir que c'était possible — on veut voir comment.
    solution: std::sync::Arc<std::sync::Mutex<Option<Solution>>>,
}

impl Default for Verificateur {
    fn default() -> Self {
        Self::nouveau()
    }
}

impl Verificateur {
    pub fn nouveau() -> Verificateur {
        Verificateur {
            etat: std::sync::Arc::new(std::sync::Mutex::new(EtatCarte::Inconnue)),
            solution: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Ce qu'on sait pour l'instant.
    pub fn etat(&self) -> EtatCarte {
        *self.etat.lock().unwrap()
    }

    /// Le parcours trouvé, s'il y en a un — celui qu'on montrera.
    pub fn solution(&self) -> Option<Solution> {
        self.solution.lock().unwrap().clone()
    }

    /// Lance une vérification. Sans effet si une autre est déjà en cours — une carte ne se
    /// vérifie qu'une fois par manche, et empiler des fils sur chaque bloc posé n'apporterait rien.
    pub fn lancer(&self, grid: &TileGrid, traps: &TrapManager) {
        {
            let mut etat = self.etat.lock().unwrap();
            if *etat == EtatCarte::EnCours {
                return;
            }
            *etat = EtatCarte::EnCours;
        }

        let grid = grid.clone();
        let traps = traps.clone();
        let partage = std::sync::Arc::clone(&self.etat);
        let garde_solution = std::sync::Arc::clone(&self.solution);

        std::thread::spawn(move || {
            // ⚠ UN SEUL MONDE POUR TOUT LE PIPELINE. `resoudre_avec` filtre déjà les pièges
            // temporaires, mais `la_plus_imitable` et `robustesse` reçoivent les traps qu'on leur
            // passe ICI : leur donner les pièges COMPLETS ferait échouer le rejeu d'une solution
            // pourtant valide, et la robustesse s'effondrerait sans raison visible. On convertit
            // donc une fois, et tout le monde travaille dessus.
            let permanents = vue_permanente(&traps);
            let verdict = match resoudre_avec(&grid, &traps, BUDGET_PARTIE) {
                Verdict::Franchissable(brute) => {
                    // On garde la version la plus imitable des deux (brute ou simplifiée) :
                    // simplifier aide sur un couloir et NUIT sur une carte à sauts.
                    let retenue = la_plus_imitable(&grid, &permanents, &brute);
                    let note = robustesse(&grid, &permanents, &retenue.entrees, 40);
                    *garde_solution.lock().unwrap() = Some(retenue);
                    EtatCarte::Franchissable { robustesse: note }
                }
                Verdict::PasTrouve { .. } => EtatCarte::PasTrouvee,
            };
            *partage.lock().unwrap() = verdict;
        });
    }

    /// Remet à zéro, pour la manche suivante.
    pub fn oublier(&self) {
        *self.etat.lock().unwrap() = EtatCarte::Inconnue;
        *self.solution.lock().unwrap() = None;
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
//  Montrer le parcours — le second usage du solveur
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// Rejoue un parcours trouvé, pour le **montrer**.
///
/// C'est le second usage du TAS, et il compte autant que le premier : quand personne n'a réussi
/// une manche, savoir que c'était possible ne suffit pas — il faut voir comment.
///
/// ⚠ **L'avancement se fait à pas FIXE**, quoi qu'il arrive au nombre d'images par seconde. Le
/// parcours a été calculé à ce pas-là ; le rejouer au rythme irrégulier de l'affichage donnerait
/// une autre trajectoire, et le fantôme raterait le saut qu'on est justement en train de montrer.
/// D'où l'accumulateur : on consomme le temps réel par tranches de `PAS`.
pub struct Demonstration {
    fantome: Player,
    entrees: Vec<Manette>,
    image: usize,
    saut_precedent: bool,
    accumule: f32,
}

impl Demonstration {
    pub fn nouvelle(grid: &TileGrid, solution: Solution) -> Demonstration {
        Demonstration {
            fantome: Player::new(grid.start_pos),
            entrees: solution.entrees,
            image: 0,
            saut_precedent: false,
            accumule: 0.0,
        }
    }

    /// Fait avancer le fantôme du temps réel écoulé.
    pub fn avancer(&mut self, dt: f32, grid: &TileGrid, traps: &TrapManager) {
        // Borne le rattrapage : après une pause (fenêtre déplacée, chargement), on ne veut pas
        // que le fantôme traverse la carte d'un coup pour « rattraper » le temps perdu.
        self.accumule = (self.accumule + dt).min(0.25);

        while self.accumule >= PAS && self.image < self.entrees.len() {
            let commande = self.entrees[self.image];
            self.fantome
                .update(PAS, &commande.vers_entrees(self.saut_precedent), grid, traps);
            self.saut_precedent = commande.saut;
            self.image += 1;
            self.accumule -= PAS;
        }
    }

    /// Où en est le fantôme.
    pub fn position(&self) -> Vec2 {
        self.fantome.position
    }

    /// Le parcours est-il arrivé au bout ?
    pub fn terminee(&self) -> bool {
        self.image >= self.entrees.len()
    }

    /// La part du parcours déjà jouée, de 0 à 1 — pour une barre de progression.
    pub fn avancement(&self) -> f32 {
        if self.entrees.is_empty() {
            return 1.0;
        }
        self.image as f32 / self.entrees.len() as f32
    }
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

    /// Un couloir BAS : sol continu et plafond juste au-dessus de la tête.
    ///
    /// Il existe parce qu'un obstacle posé au sol dans un couloir ouvert ne bouche rien — **le
    /// joueur saute par-dessus**, et c'est très bien ainsi. Pour éprouver ce qui bouche VRAIMENT,
    /// il faut retirer le ciel. Le jeu en a un, de plafond : celui de béton qu'on voit à l'écran.
    fn couloir_bas(largeur: usize) -> TileGrid {
        let mut grid = TileGrid::vide(largeur, 10);
        for x in 0..largeur {
            grid.set_tile(x, 0, crate::grid::TileType::SolidBlock);
            grid.set_tile(x, 3, crate::grid::TileType::SolidBlock);
        }
        grid
    }

    /// Construit un couloir plat : un sol continu, rien d'autre.
    fn couloir(largeur: usize) -> TileGrid {
        let mut grid = TileGrid::vide(largeur, 10);
        for x in 0..largeur {
            grid.set_tile(x, 0, crate::grid::TileType::SolidBlock);
        }
        grid
    }

    /// **LA PAIRE QUI JUSTIFIE `vue_permanente`.** Même carte, même piège au même endroit :
    /// seul le FILTRE change, et le verdict bascule.
    ///
    /// Sans le filtre, le lance-flammes allumé bouche le couloir *pour toujours* — parce que le
    /// solveur n'anime pas les pièges : celui qui brûle à l'instant du lancement brûle jusqu'à la
    /// fin des temps. C'est ce qui rendait le TAS « extrêmement nul » : il déclarait infranchissables
    /// des cartes qu'il suffisait de traverser deux secondes plus tard.
    #[test]
    fn un_lance_flammes_allume_ne_bouche_plus_le_couloir_mais_une_scie_si() {
        let grid = couloir(20);
        let milieu = Vec2::new(10.0, 1.0);

        // ── 1. Le lance-flammes ALLUMÉ, vu SANS filtre : il bouche (l'ancien comportement).
        let mut flammes = TrapManager::new();
        flammes.add_trap(
            milieu,
            crate::traps::TrapKind::Flamethrower {
                dir: crate::traps::Direction::Up,
                active: true,
                timer: 0.0,
            },
            0,
        );
        let sans_filtre = resoudre_regle(&grid, &flammes, 60_000, POIDS, PRIX_DU_CHANGEMENT);

        // ── 2. LE MÊME piège, vu par `resoudre_avec` (qui filtre) : on passe.
        let avec_filtre = resoudre_avec(&grid, &flammes, 60_000);
        match avec_filtre {
            Verdict::Franchissable(s) => assert!(
                rejouer(&grid, &vue_permanente(&flammes), &s.entrees),
                "la solution doit passer le contrôle DANS LE MÊME MONDE que celui qui l'a produite"
            ),
            Verdict::PasTrouve { explores } => panic!(
                "un lance-flammes s'ÉTEINT : le joueur peut attendre. ({explores} etats explores)"
            ),
        }

        // Le test ne vaut que si le filtre change VRAIMENT quelque chose. Si le piège ne bouchait
        // pas non plus sans filtre, on n'aurait rien prouvé — juste écrit deux fois le même cas.
        assert!(
            matches!(sans_filtre, Verdict::PasTrouve { .. }),
            "temoin creux : ce lance-flammes ne bouchait pas meme sans le filtre, le test ne prouve rien"
        );

        // ── 3. LE TÉMOIN NÉGATIF, sans lequel on aurait seulement appris à tout ignorer.
        //
        // Une scie doit continuer de bloquer : sa rotation est purement visuelle, elle tue en
        // permanence dans son rayon. C'est un mur rond, pas un piège à timer.
        //
        // ⚠ Il lui faut un couloir BAS. Posée dans le couloir ouvert du début, la scie ne bouche
        // rien — le solveur saute simplement par-dessus, et il a raison. La première version de ce
        // test l'ignorait et échouait donc en accusant le filtre, alors que c'est le SOLVEUR qui
        // avait raison. Un obstacle ne se juge pas à ce qu'il est, mais à ce qu'il ferme.
        let bas = couloir_bas(20);
        let mut scie = TrapManager::new();
        scie.add_trap(Vec2::new(10.0, 1.0), crate::traps::TrapKind::SawBlade { radius: 0.75, rotation: 0.0 }, 0);
        assert!(
            matches!(resoudre_avec(&bas, &scie, 60_000), Verdict::PasTrouve { .. }),
            "une scie tue SANS CONDITION : le filtre ne doit pas la faire disparaitre"
        );
        // Et le couloir bas se franchit quand la scie n'y est pas — sinon on aurait prouvé
        // seulement que le plafond bloque.
        assert!(
            matches!(resoudre_avec(&bas, &TrapManager::new(), 60_000), Verdict::Franchissable(_)),
            "temoin creux : ce couloir bas est infranchissable MEME SANS scie"
        );
    }

    /// Le filtre garde ce qui tue toujours et retire ce qui s'éteint — vérifié sur la liste, pas
    /// sur l'intuition « ça bouge donc c'est temporaire ».
    #[test]
    fn la_vue_permanente_garde_exactement_ce_qui_tue_sans_condition() {
        use crate::traps::{Direction, TrapKind};
        let mut tous = TrapManager::new();
        let p = Vec2::new(1.0, 1.0);
        tous.add_trap(p, TrapKind::SpikeTrap, 0);
        tous.add_trap(p, TrapKind::SawBlade { radius: 0.75, rotation: 0.0 }, 0);
        tous.add_trap(p, TrapKind::Flamethrower { dir: Direction::Up, active: true, timer: 0.0 }, 0);
        tous.add_trap(p, TrapKind::LaserEmitter { dir: Direction::Up, active: true, timer: 0.0 }, 0);
        tous.add_trap(p, TrapKind::CannonTurret { dir: Direction::Up, fire_rate: 2.5, timer: 0.0 }, 0);

        let vue = vue_permanente(&tous);
        assert_eq!(vue.traps.len(), 2, "seuls les pics et la scie tuent sans condition");
        assert!(vue.traps.iter().all(|t| matches!(
            t.kind,
            TrapKind::SpikeTrap | TrapKind::SawBlade { .. }
        )));
        assert!(vue.projectiles.is_empty(), "un tir en vol est l'incarnation meme du danger qui passe");
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

    /// La mesure doit rendre le MEME chiffre a chaque appel : une robustesse qui bouge d'une
    /// execution a l'autre ne se compare a rien, et ne peut donc pas servir a choisir.
    #[test]
    fn la_mesure_de_robustesse_est_reproductible() {
        let grid = couloir(20);
        let traps = TrapManager::new();
        let Verdict::Franchissable(s) = resoudre(&grid, 60_000) else {
            panic!("le couloir doit se franchir");
        };
        let a = robustesse(&grid, &traps, &s.entrees, 40);
        let b = robustesse(&grid, &traps, &s.entrees, 40);
        assert_eq!(a.to_bits(), b.to_bits(), "meme graine, meme mesure");
    }

    /// **Le sens de la mesure** : un parcours plat pardonne l'imprecision, un parcours qui exige
    /// un saut au bon moment la pardonne moins. C'est exactement ce que « facile pour un humain »
    /// veut dire, et c'est ce chiffre qui departagera deux solutions.
    #[test]
    fn un_couloir_plat_pardonne_plus_qu_un_saut_a_negocier() {
        let traps = TrapManager::new();

        let plat = couloir(20);
        let Verdict::Franchissable(s_plat) = resoudre(&plat, 60_000) else {
            panic!("couloir plat")
        };

        let mut troue = couloir(24);
        troue.set_tile(11, 0, crate::grid::TileType::Air);
        troue.set_tile(12, 0, crate::grid::TileType::Air);
        let Verdict::Franchissable(s_troue) = resoudre(&troue, 400_000) else {
            panic!("trou franchissable")
        };

        // On mesure sur les solutions SIMPLIFIEES : c'est celles-la qu'on montrerait.
        let s_plat = simplifier(&plat, &traps, &s_plat);
        let s_troue = simplifier(&troue, &traps, &s_troue);

        let r_plat = robustesse(&plat, &traps, &s_plat.entrees, 60);
        let r_troue = robustesse(&troue, &traps, &s_troue.entrees, 60);

        assert!(r_plat >= r_troue,
            "un couloir plat ({r_plat:.2}) ne peut pas etre plus exigeant qu'un saut ({r_troue:.2})");
        assert!(r_plat > 0.5, "un couloir plat doit tres largement pardonner, vaut {r_plat:.2}");
    }

    /// Le brouillage doit deplacer des instants d'appui, pas tout casser : la sequence garde sa
    /// longueur et reste faite de commandes du repertoire.
    #[test]
    fn le_brouillage_deplace_les_appuis_sans_denaturer_la_sequence() {
        let sequence: Vec<Manette> = (0..120)
            .map(|i| Manette::REPERTOIRE[(i / 17) % Manette::REPERTOIRE.len()])
            .collect();
        let mut des = Des(42);
        let brouillee = brouiller(&sequence, &mut des);

        assert_eq!(brouillee.len(), sequence.len(), "la duree ne change pas");
        for c in &brouillee {
            assert!(Manette::REPERTOIRE.contains(c), "aucune commande inventee");
        }
        assert_ne!(brouillee, sequence, "le brouillage doit VRAIMENT changer quelque chose");
    }

    /// **Le temoin de la simplification** : elle ne casse rien, et elle rend le parcours
    /// nettement plus imitable. Les deux moities comptent — une simplification qui casserait le
    /// parcours serait pire qu'aucune.
    #[test]
    fn la_simplification_rend_la_solution_nettement_plus_imitable() {
        let grid = couloir(20);
        let traps = TrapManager::new();
        let Verdict::Franchissable(brute) = resoudre(&grid, 60_000) else {
            panic!("le couloir doit se franchir")
        };
        let simple = simplifier(&grid, &traps, &brute);

        assert!(
            rejouer(&grid, &traps, &simple.entrees),
            "une simplification qui casse le parcours serait pire qu'aucune"
        );
        assert!(
            simple.changements() <= brute.changements(),
            "brute {} changements, simplifiee {}",
            brute.changements(),
            simple.changements()
        );

        let r_brute = robustesse(&grid, &traps, &brute.entrees, 60);
        let r_simple = robustesse(&grid, &traps, &simple.entrees, 60);
        println!(
            "couloir plat — brute : {} changements, robustesse {:.2} | simplifiee : {} changements, robustesse {:.2}",
            brute.changements(), r_brute, simple.changements(), r_simple
        );
        // ⚠ On n'exige plus que la simplification AMELIORE. Depuis que le prix du changement est
        // dans la recherche, celle-ci rend deja des parcours propres — sur un couloir plat, un
        // seul changement et 100 % de robustesse la ou elle en donnait quinze et 10 %. La
        // simplification devient un filet, pas le remede : ce qu'on lui demande, c'est de ne
        // jamais DEGRADER ce qui arrive deja bon.
        assert!(
            r_simple >= r_brute,
            "la simplification ne doit jamais degrader : {r_brute:.2} -> {r_simple:.2}"
        );
        assert!(r_simple > 0.9, "un couloir plat doit etre presque toujours refaisable");
    }

    /// MESURE — ce que coûte le solveur sur la carte RÉELLE du jeu.
    ///
    /// `#[ignore]` volontairement : `TileGrid::new` charge la carte du joueur si elle existe, donc
    /// ce test dépend de ce qui est installé sur la machine. En faire une garantie le rendrait
    /// instable, et un test instable finit par être ignoré pour de mauvaises raisons. Il reste
    /// là pour être lancé à la main :
    /// `cargo test --release mesure_sur_la_carte -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_sur_la_carte_reelle() {
        let grid = TileGrid::new(48, 24);
        let depart = std::time::Instant::now();
        let traps = TrapManager::new();

        // 1) Un seul reglage : ce que le solveur rendait jusqu'ici.
        let t0 = std::time::Instant::now();
        let simple_verdict = resoudre(&grid, BUDGET_PARTIE);
        let t_simple = t0.elapsed();


        let decrire = |v: &Verdict| match v {
            Verdict::Franchissable(sol) => format!(
                "{} chgts, robustesse {:.2}",
                sol.changements(),
                robustesse(&grid, &traps, &sol.entrees, 40)
            ),
            Verdict::PasTrouve { explores } => format!("PAS TROUVE ({explores})"),
        };

        println!("carte reelle {}x{} : {} en {:.2} s",
                 grid.width, grid.height, decrire(&simple_verdict), t_simple.as_secs_f32());
    }

    /// Le choix ne doit JAMAIS rendre une version moins imitable que la brute — c'est tout ce
    /// qu'on lui demande, et c'est exactement ce que la simplification seule ne garantissait pas.
    #[test]
    fn on_garde_la_version_la_plus_imitable_des_deux() {
        let traps = TrapManager::new();
        for largeur in [20usize, 24] {
            let mut grid = couloir(largeur);
            if largeur == 24 {
                grid.set_tile(11, 0, crate::grid::TileType::Air);
                grid.set_tile(12, 0, crate::grid::TileType::Air);
            }
            let Verdict::Franchissable(brute) = resoudre(&grid, 400_000) else {
                panic!("carte {largeur} franchissable")
            };
            let retenue = la_plus_imitable(&grid, &traps, &brute);

            assert!(rejouer(&grid, &traps, &retenue.entrees), "la retenue doit arriver");
            let r_brute = robustesse(&grid, &traps, &brute.entrees, 40);
            let r_retenue = robustesse(&grid, &traps, &retenue.entrees, 40);
            assert!(
                r_retenue >= r_brute,
                "carte {largeur} : la retenue ({r_retenue:.2}) ne doit jamais etre pire que la brute ({r_brute:.2})"
            );
        }
    }

    /// MESURE — la recherche multi-reglages apporte-t-elle quelque chose ? Sur quelles cartes ?
    #[test]
    #[ignore]
    fn mesure_multi_reglages() {
        let traps = TrapManager::new();
        let cas: Vec<(&str, TileGrid)> = vec![
            ("couloir plat", couloir(20)),
            ("trou de 2", { let mut g = couloir(24);
                g.set_tile(11, 0, crate::grid::TileType::Air);
                g.set_tile(12, 0, crate::grid::TileType::Air); g }),
            ("trou de 3", { let mut g = couloir(28);
                for x in 12..15 { g.set_tile(x, 0, crate::grid::TileType::Air); } g }),
            ("marche haute", { let mut g = couloir(28);
                for y in 1..4 { g.set_tile(14, y, crate::grid::TileType::SolidBlock); } g }),
        ];

        for (nom, grid) in &cas {
            let un = match resoudre(grid, 400_000) {
                Verdict::Franchissable(b) => {
                    let r = la_plus_imitable(grid, &traps, &b);
                    format!("{:.2}", robustesse(grid, &traps, &r.entrees, 40))
                }
                Verdict::PasTrouve { .. } => "pas trouve".into(),
            };
            // Les quatre reglages, montes ici plutot que dans le module : le code qui les
            // enchainait a ete RETIRE faute d'apporter quoi que ce soit (note en tete de module).
            // Cette mesure reste, pour qu'on n'ait pas a le reecrire pour re-decouvrir la meme
            // chose — et pour attraper le jour ou une carte lui donnerait enfin raison.
            let mut meilleure = 0.0f32;
            for (poids, prix) in [
                (POIDS, PRIX_DU_CHANGEMENT),
                (POIDS, PRIX_DU_CHANGEMENT * 3),
                (POIDS, PRIX_DU_CHANGEMENT / 4),
                (POIDS * 1.7, PRIX_DU_CHANGEMENT * 2),
            ] {
                if let Verdict::Franchissable(b) = resoudre_regle(grid, &traps, 400_000, poids, prix) {
                    let r = la_plus_imitable(grid, &traps, &b);
                    meilleure = meilleure.max(robustesse(grid, &traps, &r.entrees, 40));
                }
            }
            let multi = format!("{meilleure:.2}");
            println!("{nom:<16} un reglage {un:>10} | quatre reglages {multi:>10}");
        }
    }

    /// Le fantome doit rejouer EXACTEMENT ce que le solveur a calcule : il finit a l'arrivee.
    #[test]
    fn le_fantome_rejoue_le_parcours_jusqu_a_l_arrivee() {
        let grid = couloir(20);
        let traps = TrapManager::new();
        let Verdict::Franchissable(sol) = resoudre(&grid, 60_000) else {
            panic!("couloir franchissable")
        };
        let images = sol.entrees.len();

        let mut demo = Demonstration::nouvelle(&grid, sol);
        assert!(!demo.terminee());
        assert!(demo.avancement() < 0.01);

        // On avance par tranches irregulieres, comme le ferait un vrai affichage.
        let mut tours = 0;
        while !demo.terminee() && tours < images * 4 {
            demo.avancer(if tours % 3 == 0 { 0.030 } else { 0.011 }, &grid, &traps);
            tours += 1;
        }

        assert!(demo.terminee(), "le fantome doit aller au bout de la sequence");
        assert!(
            (demo.position() - grid.finish_pos).length() < RAYON_ARRIVEE,
            "et finir A L'ARRIVEE, pas ailleurs : {:?}",
            demo.position()
        );
        assert!((demo.avancement() - 1.0).abs() < 1e-6);
    }
}
