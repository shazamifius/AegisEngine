// LA PASSE D'OMBRE — on ne dessine QUE la profondeur, vue depuis la lumiere.
//
// Aucune couleur n'est calculee ici, et c'est tout l'interet : cette passe redessine la scene une
// seconde fois, et sur la machine de reference du projet (un Meta Quest 2, 13,9 ms pour deux yeux)
// une seconde passe complete d'ombrage serait hors budget. Ce shader ne fait donc rien d'autre que
// placer les sommets.
//
// ⚠ Il partage les constantes poussees et le cadre du shader principal : c'est la MEME description
// d'objet qui sert aux deux passes. Deux structures qui divergent, c'est deux verites a maintenir,
// et une ombre decalee sans que rien ne paraisse faux.

struct PushConstants {
    model_matrix: mat4x4<f32>,
    color_tint: vec4<f32>,
    params: vec4<f32>,
};

var<push_constant> pc: PushConstants;

struct Lumiere {
    position_type: vec4<f32>,
    couleur_intensite: vec4<f32>,
    direction_cone: vec4<f32>,
};

struct Cadre {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    camera_et_compte: vec4<f32>,
    ciel_exposition: vec4<f32>,
    sol_point_blanc: vec4<f32>,
    matiere: vec4<f32>,
    lumieres: array<Lumiere, 16>,
};

@group(0) @binding(0) var<uniform> cadre: Cadre;

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
