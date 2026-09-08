//! **OÙ CHAQUE TRIANGLE VIT EN MÉMOIRE — et pourquoi son adresse ne doit pas bouger.**
//!
//! [`allocation`](crate::render::allocation) décide **combien** de densité donner à chaque triangle.
//! Ce fichier décide **où** cette densité vit. *Ce sont deux questions différentes, et les confondre
//! coûte 96 % de la mémoire par image.*
//!
//! ## ⛔ LE DÉFAUT QUE CE FICHIER CORRIGE, ET IL EST MESURÉ
//!
//! `planifier` construit les adresses en cumulant : `base += micro_sommets(k)`. C'est compact et
//! c'est juste — mais **un seul triangle qui change de subdivision décale l'adresse de tous ceux qui
//! le suivent.**
//!
//! Le banc `persistance` l'a chiffré sur la table, à 72 Hz, en marchant et en tournant la tête :
//!
//! | | |
//! |---|---|
//! | triangles qui changent de subdivision | **7,7 sur 3 274** — 0,23 % |
//! | contenu qui cesse d'être vrai | **0,52 %** |
//! | mémoire qu'il faut **déplacer** | **96,3 %** |
//!
//! > **Ce qui empêche une mémoire de surface persistante n'est donc pas le mouvement de la caméra :
//! > c'est la forme de l'adresse.** *Le contenu reste vrai à 99,5 % ; le reste est de la copie que
//! > rien n'oblige à faire.*
//!
//! ## ⚠ Pourquoi ça compte au-delà de la performance
//!
//! `02-THESE.md` promet qu'une texture est *« l'état d'une surface »* et un shader *« la loi qui
//! fait évoluer cet état »* — **une mémoire et sa dérivée**. Cette phrase suppose un état qui
//! **survit** à l'image précédente. Sans adresse stable, il n'y a pas d'état : il y a une texture
//! recalculée sous un autre nom, et « texture = shader » reste une intention.
//!
//! Et *Temporally Adaptive Shading Reuse* (TU Graz, ACM TOG 40-2) le dit sans détour, depuis 2021 :
//!
//! > *« reuse units for which samples are newly allocated and **reallocated** in the atlas are
//! > always shaded, i.e., they are considered newly visible »*
//!
//! **Chez eux, une entrée réallouée est perdue.** Leur réutilisation vaut 57–90 % ; avec 96,3 % de
//! réallocation, la nôtre tomberait vers 4 %.
//!
//! ## ⭐ La solution n'est pas une invention : c'est le patron de l'industrie
//!
//! FastAtlas décrit ainsi son prédécesseur *Shading Atlas Streaming* : il *« packs small 1–3
//! triangle charts into regular grids using a **superblock scheme inspired by memory
//! management** »*. **Une free-list par classe de taille**, sans coalescence.
//!
//! ⭐ **Et notre unité EST le triangle**, là où eux doivent d'abord construire des chartes — parce
//! qu'une adresse barycentrique $(T,u,v)$ ne dépend d'aucun empaquetage 2D. *C'est le second versant
//! de l'innovation ①, et le 6 septembre ne l'avait pas vu : ce jour-là elle valait 4,9 % du budget
//! d'une entrée, « de la qualité, pas du budget ». La stabilité temporelle, elle, est du budget.*
//!
//! ## Comment ça marche, en trois phrases
//!
//! Les subdivisions ne prennent que **neuf** valeurs ($k = 0..8$), donc les blocs ne prennent que
//! neuf tailles : 3, 6, 15, 45, 153, 561, 2 145, 8 385 et 33 153 entrées. Chaque classe garde la
//! liste de ses blocs libres ; un triangle qui change de subdivision rend son bloc à sa classe et en
//! prend un dans la nouvelle. **Un triangle qui ne change pas ne bouge pas** — et c'est toute
//! l'affaire.
//!
//! ## ⚠⚠ CE QUE ÇA COÛTE, ET IL FAUT LE MESURER AVANT DE S'EN RÉJOUIR
//!
//! **Sans coalescence, un bloc libéré de classe $k$ ne resservira qu'à un futur triangle de classe
//! $k$.** L'empreinte réservée est donc le cumul des pics par classe, et elle est **supérieure** à
//! l'empreinte utile. *Si ce surcoût n'est pas borné, c'est exactement le critère d'invalidation de
//! `02-THESE.md` — « une empreinte non bornée » — et il faudra le dire.*
//!
//! [`Placement::fragmentation`] rend ce rapport, et le banc `persistance` le suit sur une séquence.
//!
//! ## ⛔ ET LA PREMIÈRE VERSION DE CE FICHIER A ÉCHOUÉ SUR CE POINT PRÉCIS — 8 septembre 2026
//!
//! Une free-list par classe, seule, **ne borne pas l'empreinte**. Mesuré sur 600 images d'une caméra
//! agitée : fragmentation **2,65× au milieu, 3,90× à la fin, 5,26× au pire, et toujours croissante**.
//! *Un test synthétique la donnait à 1,05× ; c'est le mouvement réel qui a tranché.*
//!
//! **La cause est structurelle** : les tailles de blocs — 3, 6, 15, 45, 153, 561, 2 145… — ne sont
//! multiples les unes des autres à aucun niveau, donc un bloc rendu par une classe ne peut **jamais**
//! servir à une autre. Chaque configuration de caméra laisse derrière elle le pic de sa propre
//! classe, et les pics s'additionnent au lieu de se recycler.
//!
//! ⚠ **Un buddy allocator ne corrige pas ça** : $\mathrm{micro\_sommets}(k) \approx 4^k/2$ tombe
//! juste **au-dessus** d'une puissance de deux, donc arrondir à la puissance supérieure gâcherait
//! jusqu'à **1,98×** en permanence. *On échangerait une fragmentation non bornée contre un gâchis
//! systématique — ce n'est pas un progrès.*
//!
//! ### ⭐⭐ La réponse : COMPACTER, et déclenché par le BUDGET
//!
//! C'est ce que fait l'industrie sur ce problème — FastAtlas : *« at an adjustable interval, we
//! re-shade the entire chart atlas from scratch »*. Mais un **intervalle** est une constante
//! arbitraire de plus, à justifier pour toujours et fausse sur la machine suivante.
//!
//! > **Ici le compactage se déclenche quand l'arène ne tient plus dans le budget** — une condition
//! > physique, pas un réglage. *La constante ne rétrécit pas : elle n'a jamais à exister.*
//!
//! Et l'empreinte redevient bornée **par construction** : elle ne peut pas dépasser le budget, parce
//! que c'est le dépassement lui-même qui déclenche sa remise à plat.

use crate::render::allocation::{Plan, K_MAX};
use crate::render::surface::{micro_sommets, OCTETS_PAR_ENTREE};

/// Le nombre de classes de taille : une par subdivision possible, de `K_MIN` à [`K_MAX`].
pub const CLASSES: usize = K_MAX as usize + 1;

/// Où chaque triangle vit, et ce qui reste libre.
///
/// *Il ne connaît ni la scène, ni la caméra : on lui dit quelle subdivision chaque triangle veut, il
/// répond qui a dû déménager.*
pub struct Placement {
    /// Par classe, les bases des blocs libres. *Une pile : le dernier rendu est le premier repris,
    /// ce qui garde chaud ce qui vient d'être touché.*
    libres: Vec<Vec<u32>>,
    /// La première entrée jamais allouée — le sommet de l'arène.
    sommet: u32,
    /// Par triangle : `(base, k)`, ou `None` tant qu'il n'a rien reçu.
    occupe: Vec<Option<(u32, u32)>>,
    /// Combien de fois l'arène a été remise à plat. *Un compteur, pas une alerte : il dit à quelle
    /// fréquence le compactage MORD, ce qu'aucune relecture ne dirait.*
    pub compactages: u32,
}

/// Ce qu'une mise à jour a changé.
pub struct Deplacement {
    /// Les triangles dont le bloc a changé — **leur contenu est à recalculer entièrement.**
    pub relogés: Vec<u32>,
    /// L'arène a été remise à plat pendant cette mise à jour : **tout** est à recalculer.
    pub compacte: bool,
    /// Les entrées que ces triangles représentent.
    pub entrees_relogées: u64,
    /// Le total des entrées occupées après la mise à jour.
    pub entrees_totales: u64,
}

impl Deplacement {
    /// La part de la mémoire qui a dû être recalculée — le chiffre que tout ce fichier cherche à
    /// faire tomber.
    pub fn part(&self) -> f64 {
        if self.entrees_totales == 0 {
            return 0.0;
        }
        self.entrees_relogées as f64 / self.entrees_totales as f64
    }
}

impl Placement {
    pub fn nouveau(triangles: usize) -> Self {
        Self {
            libres: vec![Vec::new(); CLASSES],
            sommet: 0,
            occupe: vec![None; triangles],
            compactages: 0,
        }
    }

    /// Donne à chaque triangle la subdivision qu'il veut, et dit qui a dû déménager.
    ///
    /// ## ⚠ L'ordre des deux passes n'est pas indifférent
    ///
    /// On **libère tout avant d'allouer quoi que ce soit**. Sinon un triangle qui rétrécit dans
    /// cette image ne rendrait son bloc qu'après qu'un autre ait déjà poussé le sommet pour rien —
    /// *l'arène grandirait alors à chaque image alors que la place existait.*
    ///
    /// ## ⭐ Le compactage, et pourquoi il n'a pas d'intervalle
    ///
    /// Si l'arène finit **au-dessus de `budget_octets`**, elle est remise à plat : tous les triangles
    /// sont réalloués dans l'ordre, sans trou. C'est cher — tout est à recalculer cette image-là —
    /// mais c'est ce qui rend l'empreinte **bornée par construction**.
    ///
    /// *Le déclencheur est une condition physique, jamais un nombre d'images choisi à la main : le
    /// compactage arrive exactement quand il devient nécessaire, et jamais avant.*
    pub fn mettre_a_jour(&mut self, k_voulu: &[u32], budget_octets: u64) -> Deplacement {
        debug_assert_eq!(k_voulu.len(), self.occupe.len(), "un k par triangle, pas un de plus");

        // Passe 1 — rendre les blocs de ceux qui changent de classe.
        for (t, k) in k_voulu.iter().enumerate() {
            if let Some((base, ancien)) = self.occupe[t] {
                if ancien != *k {
                    self.libres[ancien as usize].push(base);
                    self.occupe[t] = None;
                }
            }
        }

        // Passe 2 — servir ceux qui n'ont rien.
        let mut relogés = Vec::new();
        let mut entrees_relogées = 0u64;
        for (t, k) in k_voulu.iter().enumerate() {
            if self.occupe[t].is_none() {
                let taille = micro_sommets(*k);
                let base = match self.libres[*k as usize].pop() {
                    Some(libre) => libre,
                    None => {
                        let base = self.sommet;
                        self.sommet += taille;
                        base
                    }
                };
                self.occupe[t] = Some((base, *k));
                relogés.push(t as u32);
                entrees_relogées += taille as u64;
            }
        }

        // ── Le compactage, si et seulement s'il peut SERVIR à quelque chose ─────────────────
        //
        // ⚠⚠ La condition porte deux termes, et le second a été trouvé par un test qui refusait de
        // passer — c'est-à-dire au bon moment.
        //
        // Compacter ne supprime que la FRAGMENTATION ; ça ne supprime jamais de la densité. Donc si
        // l'empreinte **utile** dépasse déjà le budget, aucun compactage n'y changera rien — et la
        // première version compactait alors **à chaque image**, payant le prix fort en boucle pour
        // ne rien gagner. *Le pire cas possible, atteint précisément quand la machine est déjà à la
        // peine.*
        //
        // Faire tenir la densité dans le budget est le rôle du biais de `planifier` ; le placement,
        // lui, ne répond que de l'espace qu'il gaspille.
        if self.empreinte() > budget_octets && self.utile() < self.empreinte() {
            self.compacter(k_voulu);
            return Deplacement {
                relogés: (0..k_voulu.len() as u32).collect(),
                compacte: true,
                entrees_relogées: k_voulu.iter().map(|k| micro_sommets(*k) as u64).sum(),
                entrees_totales: k_voulu.iter().map(|k| micro_sommets(*k) as u64).sum(),
            };
        }

        Deplacement {
            relogés,
            compacte: false,
            entrees_relogées,
            entrees_totales: k_voulu.iter().map(|k| micro_sommets(*k) as u64).sum(),
        }
    }

    /// Remet l'arène à plat : chaque triangle reçoit sa place dans l'ordre, sans aucun trou.
    ///
    /// ⚠ **Après ça, l'empreinte réservée EST l'empreinte utile** — fragmentation exactement 1,00.
    /// C'est le prix fort (tout est à recalculer) payé rarement, plutôt que le prix faible payé à
    /// chaque image. *L'inverse de ce que faisait l'adressage cumulatif.*
    fn compacter(&mut self, k_voulu: &[u32]) {
        for liste in &mut self.libres {
            liste.clear();
        }
        self.sommet = 0;
        for (t, k) in k_voulu.iter().enumerate() {
            self.occupe[t] = Some((self.sommet, *k));
            self.sommet += micro_sommets(*k);
        }
        self.compactages += 1;
    }

    /// L'adresse et la subdivision d'un triangle.
    pub fn bloc(&self, triangle: u32) -> Option<(u32, u32)> {
        self.occupe[triangle as usize]
    }

    /// Les octets **réservés** — le sommet de l'arène, trous compris.
    ///
    /// *C'est ce qu'il faut allouer sur la carte, et c'est ce qu'un budget doit contenir.*
    pub fn empreinte(&self) -> u64 {
        self.sommet as u64 * OCTETS_PAR_ENTREE
    }

    /// Les octets **utiles** — ceux que des triangles occupent vraiment.
    pub fn utile(&self) -> u64 {
        self.occupe
            .iter()
            .flatten()
            .map(|(_, k)| micro_sommets(*k) as u64)
            .sum::<u64>()
            * OCTETS_PAR_ENTREE
    }

    /// Ce que la stabilité coûte : réservé ÷ utile.
    ///
    /// ⚠ **C'est le chiffre qui décide si ce fichier tient.** Sans coalescence, un bloc libéré ne
    /// ressert qu'à sa propre classe ; si ce rapport monte sans borne, l'empreinte n'est plus bornée
    /// et le critère d'invalidation de `02-THESE.md` tombe. *Le mesurer, jamais l'espérer.*
    pub fn fragmentation(&self) -> f64 {
        let utile = self.utile();
        if utile == 0 {
            return 1.0;
        }
        self.empreinte() as f64 / utile as f64
    }

    /// Le plan à envoyer au GPU : les mêmes deux `u32` par triangle, avec des bases **stables**.
    ///
    /// ⚠ Les niveaux d'arêtes sont repris tels quels du plan d'origine : le raccord dépend des
    /// **voisins**, pas du placement. *Les mélanger ferait dépendre l'étanchéité de l'ordre
    /// d'allocation, ce qui n'aurait aucun sens.*
    pub fn appliquer(&self, modele: &Plan) -> Plan {
        let mut plan = modele.clone();
        for (t, place) in self.occupe.iter().enumerate() {
            if let Some((base, k)) = place {
                plan.par_triangle[t] = (*base, 1u32 << k);
            }
        }
        plan.entrees = self.sommet;
        plan
    }
}

/// ⭐⭐ **CE QU'IL FAUT RECALCULER POUR PASSER D'UNE IMAGE À LA SUIVANTE.**
///
/// C'est la fonction dont dépend toute la persistance : ce qui n'est pas dans la liste qu'elle rend
/// **garde la valeur écrite à l'image précédente**. Une omission ici ne lève aucune erreur — elle
/// laisse une valeur périmée à l'écran, ce qui ressemble à une image juste.
///
/// ## Les deux termes, et le second est celui qu'on oublie
///
/// 1. **Les triangles RELOGÉS** — leur bloc a changé, donc leur contenu est entièrement à refaire.
/// 2. **Les triangles dont le RACCORD a changé** — ils gardent leur place *et* leur subdivision,
///    mais le niveau effectif d'une de leurs arêtes a bougé parce qu'un **voisin** a changé de
///    subdivision. Le raccord aligne une arête sur le minimum des deux triangles : leur bord est
///    donc décimé différemment.
///
/// > ### ⚠ Le second terme n'est pas une précaution — il est MESURÉ nécessaire.
/// >
/// > En le retirant, le banc `reecriture` passe de 210 à 99 triangles refaits, et **209 entrées
/// > divergent** de la référence : très exactement les coutures que le raccord existe pour empêcher.
/// > *Sans lui, le banc resterait vert sur le nombre de triangles et l'image serait cousue.*
pub fn a_refaire(
    deplacement: &Deplacement,
    aretes_avant: &[[u32; 3]],
    aretes_apres: &[[u32; 3]],
) -> Vec<u32> {
    let mut liste = deplacement.relogés.clone();
    let deja: std::collections::HashSet<u32> = liste.iter().copied().collect();
    for (t, apres) in aretes_apres.iter().enumerate() {
        if !deja.contains(&(t as u32)) && aretes_avant.get(t) != Some(apres) {
            liste.push(t as u32);
        }
    }
    liste.sort_unstable();
    liste
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::allocation::planifier;

    /// ⭐⭐ **LA PROPRIÉTÉ QUE TOUT CE FICHIER EXISTE POUR TENIR.**
    ///
    /// Un triangle dont la subdivision ne change pas ne doit **jamais** voir son adresse bouger, quoi
    /// qu'il arrive à ses voisins. *C'est très exactement ce que l'adressage cumulatif ne sait pas
    /// faire, et c'est ce que le banc `persistance` a mesuré à 96,3 %.*
    #[test]
    fn une_adresse_ne_bouge_pas_tant_que_la_subdivision_ne_bouge_pas() {
        let mut p = Placement::nouveau(5);
        p.mettre_a_jour(&[3, 1, 4, 1, 5], u64::MAX);
        let avant: Vec<_> = (0..5).map(|t| p.bloc(t).unwrap().0).collect();

        // Le triangle 0 grossit beaucoup — en cumulatif, tout le reste se décalerait.
        let d = p.mettre_a_jour(&[7, 1, 4, 1, 5], u64::MAX);

        assert_eq!(d.relogés, vec![0], "seul le triangle qui change doit déménager");
        for t in 1..5u32 {
            assert_eq!(
                p.bloc(t).unwrap().0,
                avant[t as usize],
                "le triangle {t} n'a pas changé de subdivision : son adresse doit être intacte"
            );
        }
    }

    /// ⭐ Deux triangles ne doivent jamais se recouvrir — sinon ils s'écrasent en silence.
    ///
    /// *Une adresse fausse ne produit pas d'erreur : elle produit une image presque juste. C'est la
    /// même garde que la bijection de `surface.rs`, portée au niveau du placement.*
    #[test]
    fn deux_triangles_n_occupent_jamais_la_meme_memoire() {
        let mut p = Placement::nouveau(200);
        // Une séquence de subdivisions qui bouge beaucoup, pour brasser les classes.
        for tour in 0..40u32 {
            let k: Vec<u32> = (0..200u32).map(|t| (t * 7 + tour * 13) % (K_MAX + 1)).collect();
            p.mettre_a_jour(&k, u64::MAX);

            let mut occupe = vec![false; p.sommet as usize];
            for t in 0..200u32 {
                let (base, kk) = p.bloc(t).unwrap();
                for e in base..base + micro_sommets(kk) {
                    assert!(
                        !occupe[e as usize],
                        "tour {tour} : l'entrée {e} est réclamée deux fois (triangle {t})"
                    );
                    occupe[e as usize] = true;
                }
            }
        }
    }

    /// Un bloc rendu doit être repris, sinon l'arène monte sans fin.
    ///
    /// *Sans cette propriété, chaque mouvement de caméra ferait croître la mémoire pour toujours —
    /// une fuite, avec l'apparence d'une allocation qui marche.*
    #[test]
    fn un_bloc_rendu_est_repris_au_lieu_d_agrandir_l_arene() {
        let mut p = Placement::nouveau(1);
        p.mettre_a_jour(&[4], u64::MAX);
        let sommet_apres_un_tour = p.sommet;

        // On fait osciller le même triangle entre deux classes, cinquante fois.
        for i in 0..50 {
            p.mettre_a_jour(&[if i % 2 == 0 { 2 } else { 4 }], u64::MAX);
        }

        assert_eq!(
            p.sommet,
            sommet_apres_un_tour + micro_sommets(2),
            "l'arène ne doit grandir que d'un bloc de classe 2 — les autres sont repris"
        );
    }

    /// ⚠⚠ **LA GARDE QUI PORTE LE CRITÈRE D'INVALIDATION DE LA THÈSE.**
    ///
    /// `02-THESE.md` : *« ce qui invaliderait : une empreinte NON BORNÉE »*. Sans coalescence, un
    /// bloc libéré ne ressert qu'à sa classe — donc la fragmentation est un vrai risque, pas une
    /// formalité. *Ce test la borne sur une longue séquence agitée ; s'il tombe un jour, c'est le
    /// fichier entier qu'il faut revoir, pas le seuil.*
    #[test]
    fn la_fragmentation_reste_bornee_sur_une_longue_sequence() {
        let mut p = Placement::nouveau(300);
        let mut pire = 1.0f64;
        for tour in 0..300u32 {
            // Des subdivisions qui dérivent lentement, comme sous une caméra qui bouge.
            let k: Vec<u32> = (0..300u32)
                .map(|t| {
                    let brut = ((t as f32 * 0.37 + tour as f32 * 0.11).sin() * 4.0 + 4.0) as u32;
                    brut.min(K_MAX)
                })
                .collect();
            p.mettre_a_jour(&k, u64::MAX);
            pire = pire.max(p.fragmentation());
        }
        assert!(
            pire < 2.0,
            "la fragmentation atteint {pire:.2}× l'empreinte utile — au-delà de 2×, la stabilité \
             coûte plus cher que la copie qu'elle évite"
        );
    }

    /// ⭐⭐ **Le compactage ramène l'empreinte sous le budget — c'est ce qui la rend bornée.**
    ///
    /// *Sans lui, la mesure sur 600 images d'une caméra agitée montait à 5,26× et croissait encore.
    /// Ce test est cette correction, rendue exécutable.*
    ///
    /// ⚠ Le budget est choisi **au-dessus** de ce que la densité demandée coûte : sinon on
    /// mesurerait la densité, pas la fragmentation. *La première version de ce test l'ignorait et
    /// exigeait l'impossible — et c'est en refusant de passer qu'elle a montré un vrai défaut du
    /// code : compacter alors que la densité seule dépasse déjà le budget ne gagne rien, et se
    /// répétait à chaque image.*
    #[test]
    fn le_compactage_ramene_l_empreinte_sous_le_budget() {
        // ⚠ Une PERMUTATION des mêmes subdivisions ne fragmente pas : le nombre de blocs par
        // classe reste identique, donc chaque bloc rendu est aussitôt repris. *La première version
        // de ce test en utilisait une, et la garde anti-test-creux ci-dessous l'a attrapée.*
        //
        // Ce qui fragmente vraiment, c'est que les GRANDES classes se succèdent : elles portent
        // presque tous les octets, et un bloc de classe 8 rendu ne sert à aucune autre. *Une vague
        // qui ne remue que les petites classes ne fragmente que 2 % — mesuré en écrivant ce test.*
        //
        // Le cas réel derrière : une caméra qui s'approche puis s'éloigne d'un objet, où tous ses
        // triangles montent et descendent de classe ensemble.
        let subdivisions = |tour: u32| -> Vec<u32> {
            (0..300u32).map(|t| (5 + (tour / 10) % 4 + (t % 2)).min(K_MAX)).collect()
        };

        let mut p = Placement::nouveau(300);
        // Ce que la densité la plus coûteuse de la séquence demande, placée sans aucun trou.
        let pire = (0..200u32)
            .max_by_key(|t| {
                subdivisions(*t).iter().map(|k| micro_sommets(*k) as u64).sum::<u64>()
            })
            .unwrap();
        p.mettre_a_jour(&subdivisions(pire), u64::MAX);
        let budget = (p.utile() as f64 * 1.15) as u64;

        let mut deborde = false;
        for tour in 0..200u32 {
            let k = subdivisions(tour);
            let d = p.mettre_a_jour(&k, budget);
            deborde |= d.compacte;
            assert!(
                p.empreinte() <= budget.max(p.utile()),
                "tour {tour} : l'arène tient {} o pour un budget de {budget} o et {} o utiles",
                p.empreinte(),
                p.utile()
            );
        }
        assert!(deborde, "le cas de test doit faire déborder l'arène, sinon il ne teste rien");
    }

    /// ⛔ **Et le pire cas : quand la densité seule dépasse le budget, il ne faut PAS s'agiter.**
    ///
    /// Compacter ne supprime que la fragmentation. Si l'empreinte utile dépasse déjà le budget,
    /// aucun compactage n'y changera rien — et compacter quand même, à chaque image, ferait payer le
    /// prix fort en boucle **exactement quand la machine est déjà à la peine.**
    ///
    /// *Ce test garde ce cas, et il n'existerait pas sans le test voisin qui a refusé de passer.*
    #[test]
    fn aucun_acharnement_quand_c_est_la_densite_qui_deborde() {
        let mut p = Placement::nouveau(300);
        let k: Vec<u32> = (0..300u32).map(|t| t % 7).collect();
        p.mettre_a_jour(&k, u64::MAX);
        // Un budget que la densité ne peut PAS tenir, quoi qu'on fasse du placement.
        let impossible = p.utile() / 4;

        for tour in 0..100u32 {
            let k: Vec<u32> = (0..300u32).map(|t| (t * 11 + tour * 7) % 7).collect();
            p.mettre_a_jour(&k, impossible);
        }
        assert!(
            p.compactages <= 1,
            "le placement s'acharne : {} compactages pour un budget que la densité ne peut pas \
             tenir. Faire tenir la densité est le rôle du biais, pas du placement.",
            p.compactages
        );
    }

    /// ⚠ Et il ne doit PAS se déclencher quand la place ne manque pas.
    ///
    /// *Un compactage gratuit coûterait le prix fort — tout à recalculer — pour rien. C'est le
    /// symétrique du test ci-dessus, et sans lui « ça tient dans le budget » serait obtenu en
    /// compactant à chaque image.*
    #[test]
    fn aucun_compactage_tant_que_la_place_ne_manque_pas() {
        let mut p = Placement::nouveau(50);
        for tour in 0..100u32 {
            let k: Vec<u32> = (0..50u32).map(|t| (t + tour) % 5).collect();
            p.mettre_a_jour(&k, u64::MAX / 2);
        }
        assert_eq!(p.compactages, 0, "avec un budget immense, aucun compactage ne se justifie");
    }

    /// ⭐⭐ **La liste de travail doit porter LES DEUX termes.**
    ///
    /// *Le second — les triangles dont le raccord a changé sans qu'ils bougent — est celui qu'on
    /// oublie, et son oubli ne se voit pas : il laisse des valeurs périmées sur les BORDS, ce qui
    /// s'appelle une couture.*
    #[test]
    fn la_liste_de_travail_porte_les_relogés_et_les_bords_changés() {
        let deplacement = Deplacement {
            relogés: vec![2, 5],
            compacte: false,
            entrees_relogées: 0,
            entrees_totales: 0,
        };
        let avant = vec![[4, 4, 4], [4, 4, 4], [8, 8, 8], [4, 4, 4], [2, 4, 4], [8, 8, 8]];
        let mut apres = avant.clone();
        // Le triangle 4 n'a pas bougé, mais son voisin a changé : une de ses arêtes est décimée.
        apres[4] = [4, 4, 4];

        let liste = a_refaire(&deplacement, &avant, &apres);
        assert_eq!(liste, vec![2, 4, 5], "il faut les relogés ET le bord qui change");

        // ⚠ Et sans changement d'arête, on ne refait QUE les relogés : la liste ne doit pas gonfler.
        assert_eq!(
            a_refaire(&deplacement, &avant, &avant),
            vec![2, 5],
            "un raccord inchangé ne doit ajouter personne"
        );
    }

    /// ⚠ Un triangle relogé ne doit pas être compté deux fois — il serait recalculé en double.
    #[test]
    fn un_triangle_reloge_dont_le_raccord_change_aussi_n_apparait_qu_une_fois() {
        let deplacement = Deplacement {
            relogés: vec![1],
            compacte: false,
            entrees_relogées: 0,
            entrees_totales: 0,
        };
        let avant = vec![[4, 4, 4], [8, 8, 8]];
        let apres = vec![[4, 4, 4], [2, 2, 2]];
        assert_eq!(a_refaire(&deplacement, &avant, &apres), vec![1]);
    }

    /// Le plan produit doit rester lisible par le shader : bases et subdivisions cohérentes.
    #[test]
    fn le_plan_produit_porte_les_bases_stables() {
        let modele = planifier(&[10_000.0, 4.0, 250.0], u64::MAX / 2);
        let k: Vec<u32> = modele.par_triangle.iter().map(|(_, c)| c.trailing_zeros()).collect();
        let mut p = Placement::nouveau(3);
        p.mettre_a_jour(&k, u64::MAX);
        let plan = p.appliquer(&modele);

        for (t, voulu) in k.iter().enumerate() {
            let (base, cote) = plan.par_triangle[t];
            assert_eq!(base, p.bloc(t as u32).unwrap().0, "la base doit venir du placement");
            assert_eq!(cote.trailing_zeros(), *voulu, "la subdivision ne doit pas être altérée");
        }
        assert_eq!(plan.entrees, p.sommet, "le tampon se dimensionne sur l'arène, pas sur la somme");
        assert_eq!(plan.aretes, modele.aretes, "le raccord dépend des voisins, pas du placement");
    }
}
