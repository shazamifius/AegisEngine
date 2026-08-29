// ── CE QUI CHANGE A CHAQUE OBJET ────────────────────────────────────────────────────────────
//
// Ce fichier n'est pas compile seul : `build.rs` le colle en tete des shaders qui ecrivent
// `//!inclure objet`. Il n'existe donc qu'un seul endroit ou l'entree d'un sommet est decrite,
// et c'est ce qui la rend impossible a faire diverger entre l'eclairage et la passe d'ombre.
//
// ## ⭐ Ce que ces lignes ont remplace, et le chiffre qui l'a decide
//
// Ces six valeurs voyageaient en CONSTANTES POUSSEES : un envoi a la carte par objet. Mesure sur
// la scene reelle : **3 458 appels de dessin par image pour 42 374 triangles**, soit douze
// triangles par appel — exactement un cube. Meta recommande de rester sous 150 appels par image
// sur un Quest 2, et il en faut deux jeux, un par oeil : on etait vingt-trois fois au-dessus, sur
// la machine de reference declaree du projet.
//
// Elles sont maintenant des ATTRIBUTS D'INSTANCE, lus dans un tampon ecrit une fois par image. La
// carte dessine tous les cubes d'une carte en un seul appel.
//
// ⚠⚠ Et une contrainte disparait avec : Vulkan ne garantit que 128 octets de constantes poussees,
// un plafond que ce moteur avait deja depasse sans que rien ne le signale. Un tampon d'instances
// n'a pas cette limite. *La constante arbitraire ne retrecit pas, elle cesse d'exister.*

struct VertexInput {
    // ── Ce qui appartient au MAILLAGE (point de liaison 0, lu par sommet) ───────────────────
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,

    // ── Ce qui appartient a L'INSTANCE (point de liaison 1, lu par objet) ───────────────────
    // ⚠ Une matrice 4x4 n'existe pas comme attribut : elle arrive en QUATRE vec4 consecutifs,
    // recomposes ci-dessous. Vulkan procede ainsi, et l'oublier donne un pipeline qui se cree
    // sans erreur et dessine a des positions absurdes.
    @location(5) modele_0: vec4<f32>,
    @location(6) modele_1: vec4<f32>,
    @location(7) modele_2: vec4<f32>,
    @location(8) modele_3: vec4<f32>,
    // ⚠ En couleur plate, `modele` est deja une matrice d'ECRAN : aucune camera ne lui est
    // appliquee. C'est ce qui tient le HUD en place pendant que la camera bouge.
    @location(9) teinte: vec4<f32>,
    @location(10) params: vec4<f32>,
};

/// Recompose la matrice de l'objet a partir de ses quatre colonnes.
fn matrice_modele(in: VertexInput) -> mat4x4<f32> {
    return mat4x4<f32>(in.modele_0, in.modele_1, in.modele_2, in.modele_3);
}
