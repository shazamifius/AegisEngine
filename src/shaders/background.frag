#version 450

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec2 uv = in_uv;

    // Démarcation nette de studio : Mur d'ombre sombre à gauche, lumière blanche glaciale à droite
    vec3 shadow_left = vec3(0.16, 0.26, 0.44);   // Bleu Saphir Sombre Studio
    vec3 light_right = vec3(0.96, 0.98, 1.00);   // Blanc Cyan Glacial Studio

    // Arête de démarcation nette du studio à x = 0.42
    float wall_edge = smoothstep(0.415, 0.425, uv.x);
    vec3 studio_bg = mix(shadow_left, light_right, wall_edge);

    out_color = vec4(studio_bg, 1.0);
}
