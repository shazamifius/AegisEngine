//! # LE BANC DU RENDU — compter le TRAVAIL, pas les millisecondes
//!
//! Né le 29 août 2026, avant tout travail d'optimisation, et l'ordre n'est pas négociable : sans
//! instrument, on optimise à l'aveugle. Ce projet l'a déjà payé — un micro-banc mesurait une toile
//! déjà chaude en cache, les gains étaient **imaginaires**, et trois mécanismes ont été empilés
//! pour rattraper une prémisse fausse.
//!
//! ## Pourquoi compter le travail plutôt que le temps
//!
//! Son objectif est de faire tourner Aegis sur **téléphone, matériel très ancien et casque VR**.
//! Or les millisecondes de la machine de développement ne disent rien de ces machines-là : elles
//! mesurent *ce processeur, ce GPU, ce pilote, ce jour*. Un gain de 2 ms ici peut être nul sur un
//! téléphone, ou l'inverse.
//!
//! Ce qui se transporte, c'est la **quantité de travail demandée** : combien d'appels de dessin,
//! combien de triangles. Ces nombres sont **déterministes** — même scène, même compte, sur
//! n'importe quelle machine — donc comparables d'une version à l'autre et d'un appareil à l'autre.
//! Et ce sont eux qui blessent en premier sur mobile : un téléphone souffre du **nombre d'appels**
//! bien avant de souffrir du nombre de triangles.
//!
//! ⚠ **Le temps reste un indicateur, jamais une preuve portable.** Il a sa place — il dit si *ça
//! rame ici* — mais il ne se cite pas comme une propriété du moteur. Le distinguer est tout
//! l'intérêt de ce fichier : on ne mélange pas une grandeur qui voyage avec une qui ne voyage pas.
//!
//! ## Ce qu'il ne mesure PAS, et il faut le savoir avant de s'en servir
//!
//! Le **remplissage** (combien de pixels sont peints) n'est pas visible d'ici : c'est le GPU qui
//! le sait. Or c'est souvent lui qui étrangle une machine ancienne ou un casque, où la même scène
//! se dessine deux fois. Un décor peu peuplé mais couvrant tout l'écran passera donc pour léger
//! alors qu'il ne l'est pas. Le mesurer demande des requêtes GPU (`pipeline statistics`), qui
//! viendront quand ce compteur-ci aura montré ses limites — pas avant.
//!
//! *Une mesure dont on connaît l'angle mort vaut mieux qu'une mesure qu'on croit complète.*

use std::sync::atomic::{AtomicU64, Ordering};

// Des compteurs globaux, et c'est un choix. L'alternative — passer un `&mut` de mesure à travers
// toute la chaîne de rendu — alourdirait chaque signature pour une donnée qui n'intéresse que
// l'instrumentation. C'est exactement le défaut que le `Pinceau` a corrigé en son temps.
// `Relaxed` suffit : on additionne, on ne synchronise rien avec.
static IMAGES: AtomicU64 = AtomicU64::new(0);
static DESSINS: AtomicU64 = AtomicU64::new(0);
static TRIANGLES: AtomicU64 = AtomicU64::new(0);

/// Le travail demandé au GPU sur la période observée.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Travail {
    pub images: u64,
    pub dessins: u64,
    pub triangles: u64,
}

impl Travail {
    /// Le nombre d'appels de dessin **par image** — le chiffre qui décide sur téléphone.
    ///
    /// Rend `0.0` si aucune image n'a été rendue, plutôt que de diviser par zéro : un banc qui
    /// panique sur une mesure vide est un banc qu'on n'ose plus lancer.
    pub fn dessins_par_image(&self) -> f64 {
        if self.images == 0 {
            return 0.0;
        }
        self.dessins as f64 / self.images as f64
    }

    /// Les triangles soumis par image.
    pub fn triangles_par_image(&self) -> f64 {
        if self.images == 0 {
            return 0.0;
        }
        self.triangles as f64 / self.images as f64
    }
}

/// À appeler juste avant chaque appel de dessin, avec le nombre de triangles soumis.
#[inline]
pub fn noter_dessin(triangles: u32) {
    DESSINS.fetch_add(1, Ordering::Relaxed);
    TRIANGLES.fetch_add(u64::from(triangles), Ordering::Relaxed);
}

/// À appeler une fois par image présentée.
#[inline]
pub fn noter_image() {
    IMAGES.fetch_add(1, Ordering::Relaxed);
}

/// Ce qui a été compté depuis la dernière remise à zéro.
pub fn releve() -> Travail {
    Travail {
        images: IMAGES.load(Ordering::Relaxed),
        dessins: DESSINS.load(Ordering::Relaxed),
        triangles: TRIANGLES.load(Ordering::Relaxed),
    }
}

/// Repart de zéro — pour mesurer une phase précise plutôt que depuis le lancement.
pub fn remettre_a_zero() {
    IMAGES.store(0, Ordering::Relaxed);
    DESSINS.store(0, Ordering::Relaxed);
    TRIANGLES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠ Un seul test pour tout le module, et c'est délibéré : les compteurs sont GLOBAUX, donc
    /// deux tests qui les touchent en parallèle se voleraient leurs incréments et échoueraient au
    /// hasard. Un banc qui rend un verdict différent à chaque exécution est pire qu'aucun banc.
    #[test]
    fn les_compteurs_additionnent_et_se_remettent_a_zero() {
        remettre_a_zero();
        assert_eq!(releve(), Travail::default());
        // Une mesure vide ne divise pas par zéro.
        assert_eq!(releve().dessins_par_image(), 0.0);

        noter_image();
        noter_dessin(12);
        noter_dessin(30);
        let t = releve();
        assert_eq!((t.images, t.dessins, t.triangles), (1, 2, 42));
        assert_eq!(t.dessins_par_image(), 2.0);
        assert_eq!(t.triangles_par_image(), 42.0);

        noter_image();
        assert_eq!(releve().dessins_par_image(), 1.0, "deux images pour deux dessins");

        remettre_a_zero();
        assert_eq!(releve(), Travail::default());
    }
}
