//! # LE CONTRAT ENTRE LE PROCESSEUR ET LE SHADER — ce qu'une commande de dessin transporte
//!
//! **Remonté du jeu vers le moteur le 29 août 2026.** Il s'appelait `PushConstants` et ne
//! portait rien du party platformer : une matrice pour placer, une pour éclairer, une teinte et
//! quatre paramètres libres. C'est la forme qu'attend le pipeline standard du moteur, donc c'est
//! au moteur de la définir — un jeu qui la redéfinirait de son côté ferait diverger le CPU et le
//! shader au premier champ ajouté, en silence, avec des pixels faux pour seul symptôme.
//!
//! ⚠ `#[repr(C)]` n'est PAS décoratif : ces octets sont recopiés tels quels vers la carte
//! graphique. La disposition que Rust choisirait librement ne correspondrait à rien de ce que le
//! shader lit. Ajouter un champ ici oblige à toucher le shader dans le même commit.

use crate::math::{Mat4, Vec4};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PushConstants {
    /// Modèle × vue × projection : où la forme atterrit à l'écran.
    pub mvp_matrix: Mat4,
    /// La matrice de modèle seule, dont le shader tire les normales pour l'éclairage.
    pub model_matrix: Mat4,
    pub color_tint: Vec4,
    /// Quatre réglages libres lus par le shader. `w` vaut `COULEUR_PLATE` pour sortir la teinte
    /// telle quelle, sans lampe ni correction gamma — c'est ce dont l'interface 2D a besoin.
    pub params: Vec4,
}
