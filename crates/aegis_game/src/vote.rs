//! # Le vote pour retirer un bloc qui bouche
//!
//! Quand le TAS établit qu'aucun chemin ne mène à l'arrivée et **désigne le bloc dont le retrait
//! débloque** (`tas::designer_le_bouchon`), la partie ne peut pas simplement continuer : la moitié
//! des joueurs doit atteindre la ligne, et personne ne le peut. On demande donc à la table.
//!
//! ## Les règles, telles qu'il les a tranchées
//!
//! - **tout le monde vote** — y compris ceux qui ont déjà fini, et y compris **celui qui a posé le
//!   bloc** ;
//! - **deux tiers** des inscrits pour adopter ;
//! - **dès que le TAS dit « bouché »**, sans attendre la fin de la manche : « pour ne pas perdre
//!   de temps ».
//!
//! ## Pourquoi le poseur vote aussi, alors que c'est contre-intuitif
//!
//! Il a tranché « tout le monde », et c'est défendable au-delà de la simplicité : lui retirer sa
//! voix, c'est le punir d'avoir bien joué. Poser un bloc qui bloque n'est pas de la triche, c'est
//! le but du jeu — et le seuil des deux tiers suffit à l'empêcher de sauver son piège tout seul.
//!
//! ## Le seuil se calcule sur les INSCRITS, jamais sur les votants
//!
//! C'est la décision structurante de ce module. Compter deux tiers *des voix exprimées* ferait
//! adopter un retrait avec deux voix sur trois pendant que trente-deux personnes n'ont rien dit.
//! Sur les inscrits, **s'abstenir revient à s'opposer** — ce qui est le bon défaut quand l'action
//! est destructrice et irréversible.

use std::collections::HashMap;

/// Où en est un vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// Le compte n'est pas encore fait : ni les deux tiers, ni l'impossibilité de les atteindre.
    EnCours,
    /// Les deux tiers sont atteints : le bloc part.
    Adopte,
    /// Le seuil est devenu **inatteignable**, ou le temps a manqué. Le bloc reste.
    Rejete,
}

/// Un bulletin. Volontairement sans « abstention » : ne pas voter EST l'abstention, et elle a déjà
/// un effet — elle compte dans le dénominateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bulletin {
    Pour,
    Contre,
}

/// Combien de temps la table a pour se prononcer.
///
/// Assez court pour tenir dans une manche — il a demandé le déclenchement immédiat « pour ne pas
/// perdre de temps » —, assez long pour qu'on lise la question et qu'on appuie sur une touche.
pub const DUREE_VOTE: f32 = 15.0;

/// Ce que le vote fera si les deux tiers l'approuvent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Geste {
    /// Le bloc est de trop : on le retire.
    Retirer,
    /// Il manque un appui : on le pose. C'est le recours quand aucun retrait ne suffit —
    /// sans lui, le TAS répondait « aucun bloc seul » et personne n'avait rien à voter.
    Poser,
}

/// Un vote sur le retrait ou l'ajout d'un bloc précis.
#[derive(Debug, Clone)]
pub struct Vote {
    /// Ce qu'on propose de faire à ce bloc — le RETIRER, ou le POSER.
    ///
    /// Le geste fait partie de la question : « on retire celui-là ? » et « on ajoute un
    /// marchepied ici ? » ne se votent pas de la même façon, et un vote dont on ne sait pas
    /// ce qu'il fera n'est pas un vote.
    pub geste: Geste,
    /// Le bloc concerné, désigné par le TAS — jamais choisi à la main.
    pub bloc: (usize, usize),
    /// Qui a voté quoi. Un joueur ne pèse qu'une fois, quoi qu'il appuie ensuite.
    bulletins: HashMap<u32, Bulletin>,
    /// Le corps électoral, figé À L'OUVERTURE.
    ///
    /// ⚠ Figé, et c'est un choix : si le dénominateur suivait les départs, le seuil baisserait en
    /// cours de vote et un retrait pourrait être adopté parce que des gens se sont déconnectés.
    /// Une règle qui change pendant qu'on l'applique n'est plus une règle.
    inscrits: usize,
    /// Temps restant, en secondes.
    pub reste: f32,
    issue: Issue,
}

impl Vote {
    /// Ouvre le vote. `inscrits` est le nombre de joueurs présents à cet instant.
    pub fn ouvrir(geste: Geste, bloc: (usize, usize), inscrits: usize) -> Vote {
        Vote {
            geste,
            bloc,
            bulletins: HashMap::new(),
            inscrits,
            reste: DUREE_VOTE,
            // Un vote sans électeur ne peut rien adopter : il est clos d'emblée plutôt que
            // d'attendre quinze secondes pour rien.
            issue: if inscrits == 0 { Issue::Rejete } else { Issue::EnCours },
        }
    }

    /// Le nombre de voix « pour » nécessaires : **deux tiers des inscrits, arrondis vers le haut**.
    ///
    /// Vers le haut, parce qu'un arrondi vers le bas ferait passer 2 voix sur 3 inscrits à… 2, ce
    /// qui est bien deux tiers, mais ferait aussi passer 4 sur 6 à 4 — et 6 sur 10 à 6, soit 60 %.
    /// Le seuil doit être *au moins* deux tiers, jamais « à peu près ».
    pub fn seuil(&self) -> usize {
        self.inscrits.div_ceil(3) * 2
    }

    pub fn inscrits(&self) -> usize {
        self.inscrits
    }

    pub fn pour(&self) -> usize {
        self.bulletins.values().filter(|b| **b == Bulletin::Pour).count()
    }

    pub fn contre(&self) -> usize {
        self.bulletins.values().filter(|b| **b == Bulletin::Contre).count()
    }

    /// Combien n'ont encore rien dit.
    pub fn silencieux(&self) -> usize {
        self.inscrits.saturating_sub(self.bulletins.len())
    }

    pub fn issue(&self) -> Issue {
        self.issue
    }

    /// Enregistre un bulletin. **Le premier compte ; les suivants sont ignorés.**
    ///
    /// Pas par avarice de code : un vote où l'on peut changer d'avis jusqu'à la dernière seconde
    /// se gagne en surveillant le compteur, pas en se décidant. Renvoie `true` si le bulletin a
    /// été retenu.
    pub fn voter(&mut self, joueur: u32, bulletin: Bulletin) -> bool {
        if self.issue != Issue::EnCours || self.bulletins.contains_key(&joueur) {
            return false;
        }
        self.bulletins.insert(joueur, bulletin);
        self.recompter();
        true
    }

    /// Fait avancer le temps. Renvoie l'issue.
    pub fn update(&mut self, dt: f32) -> Issue {
        if self.issue != Issue::EnCours {
            return self.issue;
        }
        self.reste = (self.reste - dt).max(0.0);
        if self.reste <= 0.0 {
            // ⚠ À l'expiration, on REJETTE — on ne « prend pas la majorité des exprimés ».
            // Retirer un bloc que trente-cinq personnes ont construit ensemble est irréversible :
            // le silence ne doit jamais l'autoriser.
            self.issue = Issue::Rejete;
        }
        self.issue
    }

    /// Le décompte, appelé après chaque bulletin.
    fn recompter(&mut self) {
        let seuil = self.seuil();
        if self.pour() >= seuil {
            self.issue = Issue::Adopte;
            return;
        }
        // ⚠ CLÔTURE ANTICIPÉE. Dès que les voix restantes ne peuvent plus atteindre le seuil,
        // le vote est joué : attendre la fin du chronomètre ne ferait que voler du temps de jeu à
        // la manche — et il a demandé ce mécanisme précisément « pour ne pas perdre de temps ».
        if self.pour() + self.silencieux() < seuil {
            self.issue = Issue::Rejete;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_seuil_est_bien_deux_tiers_arrondis_vers_le_haut() {
        // 35 joueurs — sa classe : 24 voix, pas 23.
        assert_eq!(Vote::ouvrir(Geste::Retirer, (0, 0), 35).seuil(), 24);
        assert_eq!(Vote::ouvrir(Geste::Retirer, (0, 0), 3).seuil(), 2);
        assert_eq!(Vote::ouvrir(Geste::Retirer, (0, 0), 6).seuil(), 4);
        // ⚠ Le cas qui justifie l'arrondi VERS LE HAUT : 10 joueurs, 6 voix seraient 60 %.
        assert_eq!(Vote::ouvrir(Geste::Retirer, (0, 0), 10).seuil(), 8);
    }

    #[test]
    fn deux_tiers_adoptent() {
        let mut v = Vote::ouvrir(Geste::Retirer, (4, 2), 6);
        for j in 0..3 {
            v.voter(j, Bulletin::Pour);
        }
        assert_eq!(v.issue(), Issue::EnCours, "3 sur 6 ne fait pas deux tiers");
        v.voter(3, Bulletin::Pour);
        assert_eq!(v.issue(), Issue::Adopte, "4 sur 6, c'est deux tiers");
    }

    /// **Le cœur de la règle** : s'abstenir revient à s'opposer, parce que le seuil se compte sur
    /// les inscrits. Sans cela, deux voix sur trois-cent suffiraient à casser la carte.
    #[test]
    fn le_silence_ne_vaut_jamais_approbation() {
        let mut v = Vote::ouvrir(Geste::Retirer, (4, 2), 30);
        for j in 0..3 {
            v.voter(j, Bulletin::Pour);
        }
        assert_eq!(v.issue(), Issue::EnCours);
        assert_eq!(v.pour(), 3);
        assert_eq!(v.seuil(), 20);
        // Le temps passe, personne d'autre ne se prononce : le bloc RESTE.
        assert_eq!(v.update(DUREE_VOTE + 1.0), Issue::Rejete);
    }

    /// Dès que le seuil devient inatteignable, on n'attend pas le chronomètre : la manche a mieux
    /// à faire de ces secondes.
    #[test]
    fn un_vote_perdu_d_avance_se_clot_tout_de_suite() {
        let mut v = Vote::ouvrir(Geste::Retirer, (4, 2), 6); // seuil 4
        for j in 0..3 {
            v.voter(j, Bulletin::Contre);
        }
        // 3 contre sur 6 : au mieux 3 pour, le seuil de 4 est hors d'atteinte.
        assert_eq!(v.issue(), Issue::Rejete);
        assert!(v.reste > 0.0, "cloture ANTICIPEE : il restait du temps");
    }

    #[test]
    fn un_joueur_ne_pese_qu_une_fois() {
        let mut v = Vote::ouvrir(Geste::Retirer, (4, 2), 6);
        assert!(v.voter(7, Bulletin::Pour));
        assert!(!v.voter(7, Bulletin::Pour), "le second bulletin est refuse");
        assert!(!v.voter(7, Bulletin::Contre), "changer d'avis non plus");
        assert_eq!(v.pour(), 1);
    }

    #[test]
    fn un_vote_clos_n_accepte_plus_de_bulletin() {
        let mut v = Vote::ouvrir(Geste::Retirer, (4, 2), 3); // seuil 2
        v.voter(0, Bulletin::Pour);
        v.voter(1, Bulletin::Pour);
        assert_eq!(v.issue(), Issue::Adopte);
        assert!(!v.voter(2, Bulletin::Contre), "trop tard");
    }

    /// Un vote sans personne ne doit pas rester ouvert quinze secondes pour rien.
    #[test]
    fn un_vote_sans_electeur_est_clos_d_emblee() {
        assert_eq!(Vote::ouvrir(Geste::Retirer, (0, 0), 0).issue(), Issue::Rejete);
    }
}
