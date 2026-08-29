//! # LA FILE DE RENDU — décrire ce qu'il y a à dessiner, au lieu de le dessiner tout de suite
//!
//! Née le 29 août 2026, et pas par goût de l'abstraction : **il est impossible de faire une ombre
//! sans elle.**
//!
//! ## Pourquoi une ombre exige ce composant
//!
//! Une ombre se calcule en redessinant la scène **depuis la lumière**, puis en comparant les
//! profondeurs. Or le jeu dessinait de façon impérative — 41 endroits qui poussent des constantes
//! et appellent un dessin, entrelacés avec la logique de jeu. Rejouer tout ça depuis un autre
//! point de vue demanderait soit d'exécuter deux fois du code qui n'est pas fait pour (avec ses
//! effets de bord), soit de recopier les 41 sites. Les deux sont des impasses.
//!
//! La sortie est de **séparer la description de l'exécution** : le jeu dit *quoi* dessiner, le
//! moteur décide *comment* et *combien de fois*. Une fois pour l'écran, une fois par lumière qui
//! porte une ombre, et demain une fois par œil sur un casque — sans que le jeu en sache rien.
//!
//! ## Ce que ça débloque au passage, et qui n'était pas cherché
//!
//! - **Le regroupement des appels de dessin.** Le banc mesure ~1 875 appels par image pour ~22 800
//!   triangles, soit douze triangles par appel — un chiffre qui blesse d'abord sur mobile, où le
//!   nombre d'appels compte plus que le nombre de triangles. On ne peut regrouper que ce qu'on
//!   voit d'avance : une file se trie, une suite d'appels impératifs non.
//! - **Le rendu stéréo.** Deux yeux, c'est la même file consommée deux fois.
//!
//! ## ⚠ La limite, écrite avant qu'elle morde
//!
//! Le tri par maillage n'est **valide que pour l'opaque**, où le test de profondeur décide seul du
//! résultat. Réordonner des surfaces transparentes changerait l'image. La file ne porte donc que
//! de l'opaque tant que rien ne distingue les deux — et ce commentaire est ce qu'il faudra venir
//! contredire, pas un défaut d'affichage à diagnostiquer.

use crate::core::math::{Mat4, Vec4};

/// Un objet à dessiner : quel maillage, où, de quelle couleur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dessin {
    /// Quel maillage, dans la table que l'appelant tient. Le moteur n'a pas à savoir ce que
    /// « 3 » désigne — c'est au jeu de le dire.
    pub maillage: u16,
    pub modele: Mat4,
    pub teinte: Vec4,
    pub params: Vec4,
    /// Si cet objet doit apparaître dans la passe d'ombre.
    ///
    /// ⚠ **Ce n'est pas un réglage de confort.** Le décor porte des ombres ; une particule, un
    /// halo ou un élément d'interface n'en portent pas et coûteraient une passe entière pour rien.
    pub porte_une_ombre: bool,
}

/// Ce qu'il y a à dessiner dans l'image en cours.
#[derive(Default)]
pub struct File {
    dessins: Vec<Dessin>,
}

impl File {
    pub fn nouvelle() -> Self {
        Self::default()
    }

    pub fn ajouter(&mut self, dessin: Dessin) {
        self.dessins.push(dessin);
    }

    /// Repart d'une file vide pour l'image suivante.
    ///
    /// ⚠ Garde la mémoire déjà allouée (`clear`, pas une nouvelle `Vec`) : une file se remplit et
    /// se vide 165 fois par seconde, et réallouer à chaque image serait un excédent pur.
    pub fn vider(&mut self) {
        self.dessins.clear();
    }

    pub fn dessins(&self) -> &[Dessin] {
        &self.dessins
    }

    pub fn est_vide(&self) -> bool {
        self.dessins.is_empty()
    }

    /// Ce qui doit apparaître dans une passe d'ombre.
    pub fn porteurs_d_ombre(&self) -> impl Iterator<Item = &Dessin> {
        self.dessins.iter().filter(|d| d.porte_une_ombre)
    }

    /// Regroupe les dessins par maillage, pour ne lier chaque tampon qu'une fois.
    ///
    /// Tri **stable** : deux objets du même maillage gardent leur ordre d'ajout. Ça n'a aucune
    /// importance pour l'image (le test de profondeur décide), mais ça rend le rendu reproductible
    /// d'une exécution à l'autre — sans quoi aucun relevé ne serait comparable.
    pub fn regrouper(&mut self) {
        self.dessins.sort_by_key(|d| d.maillage);
    }

    /// Combien de fois il faudra changer de maillage en parcourant la file dans l'ordre.
    ///
    /// **C'est la mesure qui justifie [`File::regrouper`]**, et elle est là pour que le gain soit
    /// chiffré plutôt qu'affirmé. Sans elle, « le tri améliore les choses » resterait une croyance.
    pub fn changements_de_maillage(&self) -> usize {
        let mut changements = 0;
        let mut precedent = None;
        for d in &self.dessins {
            if precedent != Some(d.maillage) {
                changements += 1;
                precedent = Some(d.maillage);
            }
        }
        changements
    }
}

impl File {
    /// Dessine la file entière, dans son ordre courant.
    ///
    /// `maillages` est la table que l'appelant tient : `Dessin::maillage` y est un indice. Un
    /// indice hors table est **ignoré silencieusement pour le GPU mais compté** — voir la valeur
    /// rendue. Faire paniquer le rendu sur une donnée douteuse serait pire que de sauter un objet ;
    /// ne rien dire du tout serait pire encore, parce qu'un objet manquant se cherche longtemps.
    ///
    /// Rend le nombre de dessins **ignorés**, à journaliser par l'appelant si ce n'est pas zéro.
    ///
    /// # Safety
    /// Le tampon de commandes doit être en cours d'enregistrement, dans une passe de rendu
    /// compatible avec `layout`, et les maillages doivent être vivants.
    /// Joue la file : **un appel de dessin par MAILLAGE**, pas par objet.
    ///
    /// ⭐ C'est ici que 3 458 appels par image sont devenus quelques dizaines. La file etait deja
    /// triee par maillage (`regrouper`) ; il ne restait qu'a s'en servir — chaque suite d'objets
    /// partageant la meme geometrie devient une seule instruction a la carte.
    ///
    /// ⚠ La file doit avoir ete `regrouper()`ee, sinon les groupes sont fragmentes et le gain
    /// s'evapore sans que rien ne paraisse faux. Le nombre de groupes reellement emis est rendu,
    /// pour qu'on puisse le VOIR plutot que l'esperer.
    pub unsafe fn dessiner(
        &self,
        device: &ash::Device,
        cmd: ash::vk::CommandBuffer,
        instances: &crate::render::instances::Instances,
        maillages: &[&crate::geometry::gpu_mesh::GpuMesh],
    ) -> Bilan {
        let mut bilan = Bilan::default();
        let mut lot: Vec<crate::render::instances::Instance> = Vec::new();
        let mut maillage_courant: Option<u16> = None;

        // Emet le lot accumule, s'il y en a un.
        let vider = |lot: &mut Vec<crate::render::instances::Instance>,
                         courant: Option<u16>,
                         bilan: &mut Bilan| {
            let (Some(id), false) = (courant, lot.is_empty()) else {
                lot.clear();
                return;
            };
            let Some(maillage) = maillages.get(id as usize) else {
                bilan.ignores += lot.len();
                lot.clear();
                return;
            };
            if let Some(premiere) = instances.poser(lot) {
                maillage.dessiner_instances(device, cmd, premiere, lot.len() as u32);
                bilan.groupes += 1;
            } else {
                bilan.ignores += lot.len();
            }
            lot.clear();
        };

        for d in &self.dessins {
            if maillage_courant != Some(d.maillage) {
                vider(&mut lot, maillage_courant, &mut bilan);
                maillage_courant = Some(d.maillage);
            }
            lot.push(crate::render::instances::Instance {
                modele: d.modele,
                teinte: d.teinte,
                params: d.params,
            });
        }
        vider(&mut lot, maillage_courant, &mut bilan);
        bilan
    }
}

/// Ce qu'une lecture de la file a reellement produit.
///
/// ⚠ Rendu plutot que journalise : *un compte qu'on ne peut pas lire est un compte auquel on
/// finit par croire.* C'est ce chiffre, et lui seul, qui dit si le regroupement a servi.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Bilan {
    /// Combien d'appels de dessin ont ete emis.
    pub groupes: usize,
    /// Combien d'objets n'ont pas ete dessines — maillage inconnu, ou tampon plein.
    pub ignores: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dessin(maillage: u16, ombre: bool) -> Dessin {
        Dessin {
            maillage,
            modele: Mat4::IDENTITY,
            teinte: Vec4::ONE,
            params: Vec4::ZERO,
            porte_une_ombre: ombre,
        }
    }

    /// ⚠ LE TEST QUI JUSTIFIE TOUT LE FICHIER : le regroupement doit réellement réduire les
    /// changements d'état, sinon le tri est un coût sans contrepartie.
    #[test]
    fn regrouper_reduit_vraiment_les_changements_de_maillage() {
        let mut f = File::nouvelle();
        // L'ordre le plus defavorable : on alterne a chaque objet.
        for _ in 0..50 {
            f.ajouter(dessin(1, true));
            f.ajouter(dessin(2, true));
            f.ajouter(dessin(3, true));
        }
        assert_eq!(
            f.changements_de_maillage(),
            150,
            "en alternant, chaque objet coute un changement"
        );

        f.regrouper();
        assert_eq!(
            f.changements_de_maillage(),
            3,
            "apres regroupement il ne doit rester qu'un changement par maillage distinct"
        );
        assert_eq!(f.dessins().len(), 150, "regrouper ne perd aucun objet");
    }

    /// Le tri est STABLE : sans ça, deux exécutions pourraient produire deux ordres, et aucun
    /// relevé du banc ne serait comparable d'une version à l'autre.
    #[test]
    fn le_regroupement_conserve_l_ordre_au_sein_d_un_maillage() {
        let mut f = File::nouvelle();
        for i in 0..5u16 {
            let mut d = dessin(if i % 2 == 0 { 7 } else { 4 }, true);
            // On marque chaque objet pour pouvoir le reconnaitre apres le tri.
            d.teinte = Vec4::new(i as f32, 0.0, 0.0, 1.0);
            f.ajouter(d);
        }
        f.regrouper();

        let marques: Vec<f32> = f.dessins().iter().map(|d| d.teinte.x).collect();
        // maillage 4 d'abord (indices impairs 1, 3), puis maillage 7 (indices pairs 0, 2, 4).
        assert_eq!(marques, vec![1.0, 3.0, 0.0, 2.0, 4.0], "l'ordre interne doit tenir");
    }

    /// La passe d'ombre ne doit voir que ce qui porte une ombre — sinon elle paie pour les halos,
    /// les particules et l'interface.
    #[test]
    fn la_passe_d_ombre_ne_voit_que_les_porteurs() {
        let mut f = File::nouvelle();
        f.ajouter(dessin(1, true));
        f.ajouter(dessin(2, false));
        f.ajouter(dessin(3, true));

        let porteurs: Vec<u16> = f.porteurs_d_ombre().map(|d| d.maillage).collect();
        assert_eq!(porteurs, vec![1, 3]);
        assert_eq!(f.dessins().len(), 3, "l'ecran, lui, voit tout");
    }

    /// Vider garde la mémoire : une file se remplit 165 fois par seconde.
    #[test]
    fn vider_libere_la_file_sans_rendre_la_memoire() {
        let mut f = File::nouvelle();
        assert!(f.est_vide());
        for _ in 0..1000 {
            f.ajouter(dessin(1, true));
        }
        let capacite = f.dessins.capacity();
        f.vider();
        assert!(f.est_vide());
        assert_eq!(f.changements_de_maillage(), 0, "une file vide ne change rien");
        assert!(
            f.dessins.capacity() >= capacite,
            "vider ne doit pas rendre la memoire, sinon on reallouerait a chaque image"
        );
    }
}
