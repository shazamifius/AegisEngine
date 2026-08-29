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
    /// Où la forme se pose dans le monde, et de quoi le shader tire ses normales.
    ///
    /// ⚠ **La vue-projection N'EST PLUS ici** — elle vit dans le cadre de l'image
    /// ([`crate::render::cadre`]), parce qu'elle est identique pour tous les objets d'une image :
    /// la pousser par objet envoyait 64 octets rigoureusement redondants à chaque appel de dessin.
    /// Le shader compose lui-même `view_proj * model`.
    ///
    /// ⚠⚠ **Sauf en couleur plate** (`params.w == COULEUR_PLATE`), où cette matrice est déjà une
    /// matrice d'ÉCRAN et où le shader ne lui applique aucune caméra. C'est ce qui tient le HUD en
    /// place pendant que la caméra bouge.
    pub model_matrix: Mat4,
    pub color_tint: Vec4,
    /// Quatre réglages libres lus par le shader. `w` vaut `COULEUR_PLATE` pour sortir la teinte
    /// telle quelle, sans lampe ni correction gamma — c'est ce dont l'interface 2D a besoin.
    pub params: Vec4,
}
