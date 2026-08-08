#version 450

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec2 uv = in_uv;

    // Démarcation studio haut de gamme : Mur d'ombre bleu-gris de soie à gauche, lumière blanche cyan à droite
    vec3 shadow_left = vec3(0.68, 0.76, 0.86);   // Bleu Ardoise Soie Studio
    vec3 light_right = vec3(0.96, 0.98, 1.00);   // Blanc Cyan Glacial Studio

    // Transition mur d'ombre studio à x = 0.38
    float wall_edge = smoothstep(0.28, 0.48, uv.x);
    vec3 studio_bg = mix(shadow_left, light_right, wall_edge);

    // Ombre douce en bas à gauche
    float corner_shadow = smoothstep(0.0, 0.6, uv.x + uv.y * 0.5);
    studio_bg *= (0.88 + corner_shadow * 0.12);

    out_color = vec4(studio_bg, 1.0);
}
