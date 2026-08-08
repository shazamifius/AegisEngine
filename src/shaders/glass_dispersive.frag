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

    // 1. Réfraction Snell-Descartes & Échantillonnage Flou Dépoli VRAM (textureLod Mip 3.8)
    float ior = 1.48;
    float eta = 1.0 / ior;

    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    float glass_thickness = pc.glass_tint.w;
    vec2 refraction_offset = (refract_dir.xy + N.xy * 0.35) * (glass_thickness * 0.25);

    // Échantillonnage 9-Taps élargi (r = 0.065) sur Mipmap Level 3.8 pour flou dépoli laiteux (Vraie transmission du fond)
    float r = 0.065;
    vec2 offsets[9] = vec2[](
        vec2(0.0, 0.0),
        vec2(-r, -r), vec2(r, -r), vec2(-r, r), vec2(r, r),
        vec2(-r * 1.6, 0.0), vec2(r * 1.6, 0.0), vec2(0.0, -r * 1.6), vec2(0.0, r * 1.6)
    );

    vec3 frosted_refracted_bg = vec3(0.0);
    for (int i = 0; i < 9; i++) {
        vec2 uv_sample = clamp(in_screen_uv + refraction_offset + offsets[i], vec2(0.001), vec2(0.999));
        frosted_refracted_bg += textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, 3.8).rgb * (1.0 / 9.0);
    }

    // 2. Transmittance Physique Beer-Lambert (Volume Saphir Sombre pour Dalle Dense, Translucidité Claire pour Dalle Haute)
    float optical_path = glass_thickness / (NdotV + 0.08);
    vec3 base_absorption = (vec3(1.0) - pc.glass_tint.rgb) * 2.5 + vec3(0.08, 0.03, 0.00);
    vec3 beer_lambert_decay = exp(-base_absorption * optical_path);

    // Dalle sombre = Teinte saphir profonde ; Dalle claire = Transmission translucide du fond
    vec3 tint_factor = mix(vec3(1.0), pc.glass_tint.rgb, (1.0 - pc.glass_tint.r) * 0.85);
    vec3 transmitted_color = frosted_refracted_bg * beer_lambert_decay * tint_factor;

    // 3. Éclairage Spéculaire HDR & Liseré Cyan Électrique 1-Pixel (#00E5FF)
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 light1_dir = normalize(vec3(-3.5, -2.0, 3.0)); // Rim Light Bottom-Left
    vec3 light2_dir = normalize(vec3(3.2, 4.0, 2.5));   // Key Light Top-Right

    vec3 H1 = normalize(V + light1_dir);
    vec3 H2 = normalize(V + light2_dir);

    // Détection de la tranche 90° et du chanfrein 45°
    float is_side_wall = step(abs(N.z), 0.20);
    float is_chamfer = step(0.20, abs(N.z)) * step(abs(N.z), 0.90);

    // Liseré Cyan Électrique ultra-net (#00E5FF) sur le chanfrein à 45°
    float spec_chamfer = pow(max(dot(N, H1), 0.0), 220.0) * 110.0;
    vec3 cyan_glow_tint = vec3(0.00, 0.95, 1.00);
    vec3 chamfer_specular = cyan_glow_tint * spec_chamfer * is_chamfer;

    // Tranche sombre 90° franche
    vec3 side_wall_darkening = mix(transmitted_color, vec3(0.02, 0.10, 0.32) * beer_lambert_decay, is_side_wall * 0.88);

    // Composition OPAQUE : 100% de la lumière provient du fond flouté transmis + spéculaire
    vec3 final_rgb = mix(side_wall_darkening, vec3(0.98, 1.0, 1.0), fresnel) + chamfer_specular;

    // Tonemapping Filmique
    vec3 tonemapped = final_rgb / (final_rgb + vec3(0.42));
    vec3 gamma_corrected = pow(tonemapped, vec3(1.0 / 2.2));

    out_color = vec4(gamma_corrected, 1.0);
}
