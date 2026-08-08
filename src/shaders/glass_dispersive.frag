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

    // 1. Réfraction Snell-Descartes & Échantillonnage 9-Taps Géant sur Mipmap Level 4.2 (Flou Dépoli Satiné Massif)
    float ior = 1.52;
    float eta = 1.0 / ior;

    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    float glass_thickness = pc.glass_tint.w;
    vec2 refraction_offset = (refract_dir.xy + N.xy * 0.45) * (glass_thickness * 0.35);

    // Échantillonnage 9-Taps élargi (r = 0.14) sur Mipmap Level 4.2 VRAM Vulkan pour diffusion laiteuse intégrale
    float r = 0.140;
    vec2 offsets[9] = vec2[](
        vec2(0.0, 0.0),
        vec2(-r, -r), vec2(r, -r), vec2(-r, r), vec2(r, r),
        vec2(-r * 1.8, 0.0), vec2(r * 1.8, 0.0), vec2(0.0, -r * 1.8), vec2(0.0, r * 1.8)
    );

    vec3 frosted_refracted_bg = vec3(0.0);
    for (int i = 0; i < 9; i++) {
        vec2 uv_sample = clamp(in_screen_uv + refraction_offset + offsets[i], vec2(0.001), vec2(0.999));
        frosted_refracted_bg += textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, 4.2).rgb * (1.0 / 9.0);
    }

    // 2. Ombre Portée Inter-dalles & Gradient Volumétrique Cœur Bleu Saphir Sombre
    float center_dist = length(in_uv - vec2(0.5));
    float saphire_core_mask = smoothstep(0.48, 0.0, center_dist);

    float optical_path = glass_thickness / (NdotV + 0.04);
    vec3 sigma_a = vec3(4.2, 1.60, 0.05) * (1.0 + saphire_core_mask * 1.6);
    vec3 beer_lambert_decay = exp(-sigma_a * optical_path);

    vec3 sapphire_tint = mix(pc.glass_tint.rgb, vec3(0.04, 0.22, 0.58), saphire_core_mask * 0.65);
    vec3 transmitted_color = frosted_refracted_bg * sapphire_tint * beer_lambert_decay;

    // 3. Éclairage Spéculaire HDR & Liseré Cyan Électrique 1-Pixel (#00E5FF)
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 light1_dir = normalize(vec3(-3.8, -2.2, 3.2)); // Rim Light Bottom-Left
    vec3 light2_dir = normalize(vec3(3.5, 4.5, 2.5));  // Key Light Top-Right

    vec3 H1 = normalize(V + light1_dir);
    vec3 H2 = normalize(V + light2_dir);

    // Détection stricte de la tranche 90° et du chanfrein 45°
    float is_side_wall = step(abs(N.z), 0.20);
    float is_chamfer = step(0.20, abs(N.z)) * step(abs(N.z), 0.90);

    // Liseré Cyan Électrique ultra-net (#00E5FF) vibrant sur le chanfrein à 45°
    float spec_chamfer = pow(max(dot(N, H1), 0.0), 320.0) * 180.0;
    vec3 cyan_glow_tint = vec3(0.00, 0.95, 1.00);
    vec3 chamfer_specular = cyan_glow_tint * spec_chamfer * is_chamfer;

    // Tranche sombre 90° franche
    vec3 side_wall_darkening = mix(transmitted_color, vec3(0.01, 0.06, 0.28) * beer_lambert_decay, is_side_wall * 0.92);

    // Composition Finale BSDF Verre Dépoli Satiné Photoréaliste OPAQUE
    vec3 final_rgb = mix(side_wall_darkening, vec3(0.98, 1.0, 1.0), fresnel) + chamfer_specular;

    // Tonemapping Filmique
    vec3 tonemapped = final_rgb / (final_rgb + vec3(0.48));
    vec3 gamma_corrected = pow(tonemapped, vec3(1.0 / 2.2));

    out_color = vec4(gamma_corrected, 1.0);
}
