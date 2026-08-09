#version 450

layout(location = 0) in vec3 in_world_position;
layout(location = 1) in vec3 in_world_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_screen_uv;

layout(push_constant) uniform PushConstants {
    mat4 mvp_matrix;
    mat4 model_matrix;
    mat4 normal_matrix;
    vec4 glass_tint;
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.0, 4.4);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    // 1. Réfraction Snell-Descartes & Échantillonnage Flou Dépoli VRAM (textureLod Mip 3.5)
    float ior = 1.48;
    float eta = 1.0 / ior;

    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    float glass_thickness = pc.glass_tint.w;
    vec2 refraction_offset = (refract_dir.xy + N.xy * 0.18) * (glass_thickness * 0.15);

    // Échantillonnage 9-taps sur Mip 3.5 (Diffusion fluide laiteuse du fond)
    float r = 0.038;
    vec2 offsets[9] = vec2[](
        vec2(0.0, 0.0),
        vec2(-r, -r), vec2(r, -r), vec2(-r, r), vec2(r, r),
        vec2(-r * 1.4, 0.0), vec2(r * 1.4, 0.0), vec2(0.0, -r * 1.4), vec2(0.0, r * 1.4)
    );

    vec3 frosted_refracted_bg = vec3(0.0);
    for (int i = 0; i < 9; i++) {
        vec2 uv_sample = clamp(in_screen_uv + refraction_offset + offsets[i], vec2(0.001), vec2(0.999));
        frosted_refracted_bg += textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, 3.5).rgb * (1.0 / 9.0);
    }

    // 2. Transmittance Volumétrique Physique Beer-Lambert
    float optical_path = glass_thickness / (NdotV + 0.10);
    vec3 base_absorption = (vec3(1.0) - pc.glass_tint.rgb) * 1.2 + vec3(0.02, 0.01, 0.00);
    vec3 beer_lambert_decay = exp(-base_absorption * optical_path);

    // 100% de la couleur du fond transmise
    vec3 transmitted_color = frosted_refracted_bg * beer_lambert_decay;

    // 3. Spéculaire Studio HDR & Liseré Cyan Électrique 1-Pixel (#00E5FF)
    float fresnel = fresnel_schlick(NdotV, 0.06);

    vec3 light_rim_dir = normalize(vec3(-3.5, -2.0, 3.0)); // Rim light cyan (Bas-Gauche)
    vec3 light_key_dir = normalize(vec3(3.2, 4.0, 3.0));   // Key light blanche (Haut-Droite)

    vec3 H_rim = normalize(V + light_rim_dir);
    vec3 H_key = normalize(V + light_key_dir);

    // Détection géométrique précise des tranches à 90° et chanfreins à 45°
    float is_side_wall = step(abs(N.z), 0.15);
    float is_chamfer = step(0.15, abs(N.z)) * step(abs(N.z), 0.88);

    // Reflet spéculaire doux sur la face plate
    float spec_face = pow(max(dot(N, H_key), 0.0), 32.0) * 0.20 * fresnel;
    vec3 face_specular = vec3(0.92, 0.97, 1.00) * spec_face;

    // Liseré Cyan Électrique ultra-net (#00E5FF) sur le chanfrein à 45°
    float spec_chamfer = pow(max(dot(N, H_rim), 0.0), 180.0) * 75.0;
    vec3 cyan_glow = vec3(0.00, 0.95, 1.00) * spec_chamfer * is_chamfer;

    // Assombrissement réaliste de la tranche 90°
    vec3 side_darkening = mix(transmitted_color, vec3(0.04, 0.18, 0.45) * beer_lambert_decay, is_side_wall * 0.70);

    // Assemblage final direct
    vec3 final_rgb = side_darkening + face_specular + cyan_glow;

    out_color = vec4(clamp(final_rgb, 0.0, 1.0), 1.0);
}
