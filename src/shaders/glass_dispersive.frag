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

    // 1. Réfraction Snell-Descartes & Sampling Mipmap Flou Dépoli Hardware (textureLod)
    float ior = 1.52;
    float eta = 1.0 / ior;

    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    float glass_thickness = pc.glass_tint.w;
    vec2 refraction_offset = (refract_dir.xy + N.xy * 0.45) * (glass_thickness * 0.40);

    // Mipmap LOD level pour flou dépoli matériel crémeux et laiteux (Level 4.4)
    float mip_lod = 4.4 + (1.0 - NdotV) * 0.6;

    // Dispersion chromatique Cauchy RGB sur la chaîne de Mipmaps VRAM Vulkan
    vec2 uv_r = clamp(in_screen_uv + refraction_offset * 1.20, vec2(0.001), vec2(0.999));
    vec2 uv_g = clamp(in_screen_uv + refraction_offset * 1.00, vec2(0.001), vec2(0.999));
    vec2 uv_b = clamp(in_screen_uv + refraction_offset * 0.80, vec2(0.001), vec2(0.999));

    float sample_r = textureLod(sampler2D(transmission_texture, transmission_sampler), uv_r, mip_lod).r;
    float sample_g = textureLod(sampler2D(transmission_texture, transmission_sampler), uv_g, mip_lod).g;
    float sample_b = textureLod(sampler2D(transmission_texture, transmission_sampler), uv_b, mip_lod).b;

    vec3 frosted_refracted_bg = vec3(sample_r, sample_g, sample_b);

    // 2. Absorption Volumétrique de Beer-Lambert (Bleu Saphir Profond Volumétrique)
    float optical_path = glass_thickness / (NdotV + 0.04);
    vec3 sigma_a = vec3(3.6, 1.20, 0.02); // Bleu Saphir intense au cœur de la dalle
    vec3 beer_lambert_decay = exp(-sigma_a * optical_path);

    vec3 transmitted_color = frosted_refracted_bg * pc.glass_tint.rgb * beer_lambert_decay;

    // 3. Éclairage Spéculaire HDR & Liseré Cyan Électrique "Fibre Optique" (#00E5FF)
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 light1_dir = normalize(vec3(-3.8, -2.2, 3.2)); // Rim Light Bottom-Left
    vec3 light2_dir = normalize(vec3(3.5, 4.5, 2.5));  // Key Light Top-Right

    vec3 H1 = normalize(V + light1_dir);
    vec3 H2 = normalize(V + light2_dir);

    // Détection stricte de la tranche 90° et du chanfrein 45°
    float is_side_wall = step(abs(N.z), 0.20);
    float is_chamfer = step(0.20, abs(N.z)) * step(abs(N.z), 0.90);

    float spec1 = pow(max(dot(N, H1), 0.0), 96.0) * 80.0;
    float spec2 = pow(max(dot(N, H2), 0.0), 48.0) * 20.0;

    // Liseré Cyan Électrique ultra-net (#00E5FF) sur le chanfrein à 45° (Fibre Optique)
    vec3 cyan_glow_tint = vec3(0.00, 0.90, 1.00);
    vec3 chamfer_specular = mix(vec3(1.0), cyan_glow_tint, is_chamfer * 0.95) * (spec1 * is_chamfer + spec2 * 0.3);

    // Tranche sombre 90° franche
    vec3 side_wall_darkening = mix(transmitted_color, vec3(0.01, 0.08, 0.35) * beer_lambert_decay, is_side_wall * 0.92);

    // Composition Finale BSDF Verre Dépoli Satiné Photoréaliste
    vec3 final_rgb = mix(side_wall_darkening, vec3(0.98, 1.0, 1.0), fresnel) + chamfer_specular;

    float alpha = clamp(0.45 + fresnel * 0.50 + (1.0 - beer_lambert_decay.r) * 0.45, 0.40, 0.98);

    // Tonemapping Filmique
    vec3 tonemapped = final_rgb / (final_rgb + vec3(0.52));
    vec3 gamma_corrected = pow(tonemapped, vec3(1.0 / 2.2));

    out_color = vec4(gamma_corrected, alpha);
}
