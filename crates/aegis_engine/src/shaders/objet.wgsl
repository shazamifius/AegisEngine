// ── CE QUI CHANGE A CHAQUE OBJET ────────────────────────────────────────────────────────────
//
// Ce fichier n'est pas compile seul : `build.rs` le colle en tete des shaders qui ecrivent
// `//!inclure objet`. Il n'existe donc qu'un seul endroit ou cette structure est decrite, et
// c'est ce qui la rend impossible a faire diverger.
//
// 96 octets, et le chiffre compte : Vulkan ne garantit que 128 octets de constantes poussees.
// Le shader principal en poussait 160 (une matrice vue-projection redondante par objet) et
// n'aurait donc tres probablement pas pu creer son pipeline sur un GPU mobile — la machine de
// reference du projet est un Meta Quest 2.
//
// ⚠ Un shader qui inclut ce fichier DOIT declarer la meme plage de constantes poussees dans son
// layout de pipeline. Le fond, lui, ne l'inclut pas : il n'a pas d'objet.

struct PushConstants {
    // ⚠ En couleur plate cette matrice est deja une matrice d'ECRAN : aucune camera ne lui est
    // appliquee. C'est ce qui tient le HUD en place pendant que la camera bouge.
    model_matrix: mat4x4<f32>,
    color_tint: vec4<f32>,
    params: vec4<f32>,
};

var<push_constant> pc: PushConstants;
