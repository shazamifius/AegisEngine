// LA PASSE D'OMBRE — on ne dessine QUE la profondeur, vue depuis la lumiere.
//
// Aucune couleur n'est calculee ici, et c'est tout l'interet : cette passe redessine la scene une
// seconde fois, et sur la machine de reference du projet (un Meta Quest 2, 13,9 ms pour deux yeux)
// une seconde passe complete d'ombrage serait hors budget. Ce shader ne fait donc rien d'autre que
// placer les sommets.
//
// ⚠ Il partage les constantes poussees et le cadre du shader principal — non pas « par
// convention » mais litteralement : les deux fichiers inclus ci-dessous SONT ceux du shader
// principal. Deux structures qui divergent, c'etait deux verites a maintenir et une ombre
// decalee sans que rien ne paraisse faux ; elles ne peuvent plus diverger, il n'y en a qu'une.

//!inclure commun
//!inclure objet

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    return cadre.light_view_proj * pc.model_matrix * vec4<f32>(in.position, 1.0);
}

// Vide, et il doit l'etre : ecrire une couleur ici serait du travail jete.
@fragment
fn fs_main() {
}
