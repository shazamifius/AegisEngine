//! # Faire chercher le TAS par toute la table
//!
//! Sa demande : *« un système de chaîne entre toutes les personnes pour partager les informations
//! du TAS et que tout le monde fasse travailler le TAS sur des sections séparées »*.
//!
//! ## Ce qui se distribue, et ce qui ne se distribue pas
//!
//! **Découper la carte en sections ne marche pas.** Un chemin traverse les sections : les morceaux
//! ont besoin les uns des autres, et un A\* ne se coupe pas en tranches indépendantes. On
//! distribuerait le problème sans le diviser.
//!
//! **Ce qui se distribue parfaitement, en revanche, ce sont les CANDIDATS du bouchon.** « Retirer
//! ce bloc débloque-t-il ? » est une question close, indépendante des autres, et il y en a une
//! douzaine à chaque carte bouchée. C'est le même découpage que celui déjà utilisé entre les cœurs
//! d'une machine (`tas::designer_le_bouchon`) — et c'est justement pour cela qu'il était le
//! préalable : on ne distribue que ce qu'on a d'abord su paralléliser.
//!
//! ## L'asymétrie qui rend la chose SÛRE, et elle est rare
//!
//! Sur un réseau pair-à-pair, la question n'est pas « comment répartir » mais « comment croire
//! celui qui répond ». Ici la réponse est belle :
//!
//! > Une **solution** se vérifie en la rejouant — quelques millisecondes.
//! > Une **absence** de solution ne se vérifie qu'en refaisant toute la recherche.
//!
//! Donc un joueur qui annonce « ce bloc débloque » **joint le chemin**, et n'importe qui le
//! contrôle pour un coût dérisoire : mentir dans ce sens est impossible. Et mentir dans l'autre
//! (« ça ne débloque pas ») ne fait perdre qu'un candidat, qu'un autre reprendra — au pire on
//! ralentit, jamais on ne se trompe.
//!
//! **L'asymétrie joue donc en faveur du défenseur**, ce qui est l'inverse de d'habitude. C'est
//! exactement le patron « Own + Shields » du projet : celui qui calcule propose, les autres
//! vérifient à coût quasi nul.
//!
//! ## Les machines lentes n'ont pas besoin d'être détectées
//!
//! Sa crainte : *« il faut détecter les ordinateurs les plus nuls »*. Ce n'est pas nécessaire, et
//! c'est heureux — un classement des machines serait à la fois vexant et faux (une machine rapide
//! occupée est plus lente qu'une machine modeste au repos).
//!
//! On distribue **au fil de l'eau** : qui a fini redemande. Une machine lente traite un candidat
//! pendant qu'une rapide en enchaîne cinq, et l'équilibre se fait tout seul, sans mesure, sans
//! classement, sans qu'aucun joueur soit jamais désigné comme « le nul ».

use crate::grid::TileGrid;
use crate::tas::{self, Solution};
use crate::traps::TrapManager;

/// Une question posée à un pair : « en retirant ce bloc, la carte passe-t-elle ? »
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lot {
    /// De quelle carte on parle. Sans cela, une réponse calculée sur la carte de la manche
    /// précédente serait acceptée sur celle-ci — pas par malveillance, simplement par retard.
    pub carte: u64,
    pub bloc: (usize, usize),
}

/// Ce qu'un pair renvoie.
#[derive(Debug, Clone)]
pub struct Reponse {
    pub carte: u64,
    pub bloc: (usize, usize),
    /// Le chemin trouvé — **la pièce à conviction**. `None` = « je n'ai rien trouvé », qui
    /// n'engage personne et ne se vérifie pas (voir l'asymétrie, en tête de module).
    pub solution: Option<Solution>,
}

/// Empreinte d'une carte : ce qui identifie « la même carte » entre deux machines.
///
/// Un FNV-1a sur les tuiles. Pas de dépendance, pas de crypto : ce nombre n'a **rien à défendre**.
/// Un pair malveillant qui le forgerait ne gagnerait rien — sa réponse serait de toute façon
/// rejouée sur NOTRE carte, et c'est ce rejeu qui tranche. L'empreinte évite les méprises, elle ne
/// prétend pas résister à une attaque.
pub fn empreinte_carte(grid: &TileGrid) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for y in 0..grid.height as i32 {
        for x in 0..grid.width as i32 {
            h ^= grid.get_tile(x, y).is_solid() as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    }
    h
}

/// Traite un lot : c'est le travail qu'une machine fait pour les autres.
pub fn traiter(grid: &TileGrid, traps: &TrapManager, lot: &Lot, budget: usize) -> Reponse {
    let solution = if empreinte_carte(grid) != lot.carte {
        // Pas la même carte : on ne répond pas au hasard. Mieux vaut un « rien » honnête qu'une
        // réponse juste pour une autre question.
        None
    } else {
        let mut sans_lui = grid.clone();
        sans_lui.set_tile(lot.bloc.0, lot.bloc.1, crate::grid::TileType::Air);
        match tas::resoudre_avec(&sans_lui, traps, budget) {
            tas::Verdict::Franchissable(s) => Some(s),
            tas::Verdict::PasTrouve { .. } => None,
        }
    };
    Reponse { carte: lot.carte, bloc: lot.bloc, solution }
}

/// **Vérifie la réponse d'un pair — sans lui faire confiance.**
///
/// C'est la fonction qui rend toute la distribution acceptable. Elle ne relance aucune recherche :
/// elle retire le bloc annoncé sur NOTRE carte et rejoue la séquence fournie. Si le personnage
/// atteint l'arrivée, la réponse est vraie, quel que soit celui qui l'a envoyée.
///
/// Trois façons de mentir, et ce qui leur arrive :
/// - **un chemin inventé** → il ne mène pas à l'arrivée, le rejeu le dit ;
/// - **un vrai chemin, mais pour un autre bloc** → on rejoue sur la carte privée du bloc ANNONCÉ,
///   pas de celui qu'il aurait aimé ; s'il fallait l'autre, le personnage bute ;
/// - **un chemin valable sur une autre carte** → l'empreinte ne correspond pas, et même sans elle
///   le rejeu échouerait sur la nôtre.
///
/// Renvoie `false` pour une réponse vide : « je n'ai rien trouvé » n'est pas une preuve, c'est une
/// absence. On ne la croit pas, on redonne simplement le candidat à quelqu'un d'autre.
pub fn verifier(grid: &TileGrid, traps: &TrapManager, r: &Reponse) -> bool {
    let Some(solution) = r.solution.as_ref() else {
        return false;
    };
    if empreinte_carte(grid) != r.carte {
        return false;
    }
    if r.bloc.0 >= grid.width || r.bloc.1 >= grid.height {
        return false;
    }
    let mut sans_lui = grid.clone();
    sans_lui.set_tile(r.bloc.0, r.bloc.1, crate::grid::TileType::Air);
    tas::rejouer(&sans_lui, &tas::vue_permanente(traps), &solution.entrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un couloir à plafond, muré : retirer le sommet du mur rouvre le passage.
    fn carte_bouchee() -> TileGrid {
        let mut g = TileGrid::vide(24, 10);
        for x in 0..24 {
            g.set_tile(x, 0, crate::grid::TileType::SolidBlock);
        }
        for y in 1..=4 {
            g.set_tile(12, y, crate::grid::TileType::SolidBlock);
        }
        g
    }

    #[test]
    fn deux_cartes_differentes_n_ont_pas_la_meme_empreinte() {
        let a = carte_bouchee();
        let mut b = a.clone();
        b.set_tile(12, 4, crate::grid::TileType::Air);
        assert_ne!(empreinte_carte(&a), empreinte_carte(&b));
        assert_eq!(empreinte_carte(&a), empreinte_carte(&a.clone()), "stable");
    }

    /// **Le chemin heureux** : un pair fait le travail, on le vérifie sans le refaire.
    #[test]
    fn une_reponse_honnete_est_acceptee_sans_relancer_la_recherche() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let lot = Lot { carte: empreinte_carte(&grid), bloc: (12, 4) };

        let r = traiter(&grid, &vide, &lot, 60_000);
        assert!(r.solution.is_some(), "retirer le sommet du mur doit debloquer");
        assert!(verifier(&grid, &vide, &r));
    }

    /// **Le test qui compte.** Un pair renvoie un vrai chemin, mais l'attribue à un bloc qui n'y
    /// est pour rien — pour se faire créditer d'un travail, ou pour faire retirer le bloc de son
    /// choix. Le rejeu a lieu sur la carte privée du bloc ANNONCÉ : le mensonge tombe.
    #[test]
    fn un_chemin_vrai_attribue_au_mauvais_bloc_est_rejete() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let honnete = traiter(&grid, &vide, &Lot { carte: empreinte_carte(&grid), bloc: (12, 4) }, 60_000);
        assert!(honnete.solution.is_some());

        // Le même chemin, présenté comme débloquant un bloc innocent, loin du mur.
        let menteur = Reponse { bloc: (3, 0), ..honnete.clone() };
        assert!(
            !verifier(&grid, &vide, &menteur),
            "un chemin ne vaut que pour le bloc qu'il debloque REELLEMENT"
        );
    }

    #[test]
    fn un_chemin_invente_ne_passe_pas_le_rejeu() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let bidon = Reponse {
            carte: empreinte_carte(&grid),
            bloc: (12, 4),
            solution: Some(Solution {
                entrees: vec![tas::Manette { gauche: false, droite: true, saut: false }; 50],
            }),
        };
        assert!(!verifier(&grid, &vide, &bidon), "courir tout droit 50 images n'arrive nulle part");
    }

    #[test]
    fn une_reponse_pour_une_autre_carte_est_rejetee() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let mut r = traiter(&grid, &vide, &Lot { carte: empreinte_carte(&grid), bloc: (12, 4) }, 60_000);
        r.carte ^= 1; // un bit de différence suffit
        assert!(!verifier(&grid, &vide, &r));
    }

    /// « Je n'ai rien trouvé » n'est pas une preuve. On ne le croit pas — on redonne le candidat.
    #[test]
    fn une_reponse_vide_n_est_jamais_une_preuve() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let rien = Reponse { carte: empreinte_carte(&grid), bloc: (12, 4), solution: None };
        assert!(!verifier(&grid, &vide, &rien));
    }

    /// Un pair qui n'a pas la même carte répond « rien » plutôt que n'importe quoi.
    #[test]
    fn un_lot_pour_une_autre_carte_n_est_pas_traite_au_hasard() {
        let grid = carte_bouchee();
        let vide = TrapManager::new();
        let r = traiter(&grid, &vide, &Lot { carte: 0xdead_beef, bloc: (12, 4) }, 60_000);
        assert!(r.solution.is_none());
    }
}
