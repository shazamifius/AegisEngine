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
    vec4 params; // x: roughness, y: glossiness, z: ior, w: thickness
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Fonction de Distribution des Microfacettes GGX (Trowbridge-Reitz)
float distribution_ggx(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;

    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.0, 4.4);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float roughness = clamp(pc.params.x, 0.01, 0.99);
    float glossiness = clamp(pc.params.y, 0.01, 0.99);
    float ior = pc.params.z;
    float glass_thickness = pc.params.w;

    // 1. Réfraction Snell-Descartes & Dispersion Optique
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // Échantillonnage de Flou Dépoli Piloté par la Glossiness (LOD = (1 - Glossiness) * 4.8)
    float target_mip = roughness * 4.8;
    vec2 refraction_offset = refract_dir.xy * (glass_thickness * 0.12);

    // Noyau 9-Taps dynamique proportionnel à la Rugosité (1 - Glossiness)
    float r = roughness * 0.042;
    vec2 offsets[9] = vec2[](
        vec2(0.0, 0.0),
        vec2(-r, -r), vec2(r, -r), vec2(-r, r), vec2(r, r),
        vec2(-r * 1.4, 0.0), vec2(r * 1.4, 0.0), vec2(0.0, -r * 1.4), vec2(0.0, r * 1.4)
    );

    vec3 frosted_refracted_bg = vec3(0.0);
    for (int i = 0; i < 9; i++) {
        vec2 uv_sample = clamp(in_screen_uv + refraction_offset + offsets[i], vec2(0.001), vec2(0.999));
        frosted_refracted_bg += textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, target_mip).rgb * (1.0 / 9.0);
    }

    // 2. Transmittance Volumétrique Beer-Lambert Physique
    float optical_path = glass_thickness / (NdotV + 0.10);
    vec3 base_absorption = (vec3(1.0) - pc.glass_tint.rgb) * 1.4 + vec3(0.02, 0.01, 0.00);
    vec3 beer_lambert_decay = exp(-base_absorption * optical_path);

    vec3 transmitted_color = frosted_refracted_bg * beer_lambert_decay;

    // 3. Spéculaire Microfacette GGX (Brillance de surface pilotée par la Glossiness)
    float fresnel = fresnel_schlick(NdotV, 0.06);

    vec3 light_rim_dir = normalize(vec3(-3.5, -2.0, 3.0)); // Rim light cyan (Bas-Gauche)
    vec3 light_key_dir = normalize(vec3(3.2, 4.0, 3.0));   // Key light blanche (Haut-Droite)

    vec3 H_key = normalize(V + light_key_dir);
    vec3 H_rim = normalize(V + light_rim_dir);

    // Distribution GGX
    float D = distribution_ggx(N, H_key, roughness);
    float spec_ggx = D * 0.15 * glossiness * fresnel;
    vec3 surface_specular = vec3(0.95, 0.98, 1.00) * spec_ggx;

    // Liseré Cyan Électrique sur le Chanfrein 45°
    float is_chamfer = step(0.15, abs(N.z)) * step(abs(N.z), 0.88);
    float is_side_wall = step(abs(N.z), 0.15);

    float spec_chamfer = pow(max(dot(N, H_rim), 0.0), 160.0) * 80.0;
    vec3 cyan_glow = vec3(0.00, 0.95, 1.00) * spec_chamfer * is_chamfer;

    vec3 side_darkening = mix(transmitted_color, vec3(0.04, 0.18, 0.45) * beer_lambert_decay, is_side_wall * 0.70);

    vec3 final_rgb = side_darkening + surface_specular + cyan_glow;

    out_color = vec4(clamp(final_rgb, 0.0, 1.0), 1.0);
}
