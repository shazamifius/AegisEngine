#version 450

// Input Vertex Data
layout(location = 0) in vec3 in_world_position;
layout(location = 1) in vec3 in_world_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_screen_uv;

// Push Constants Mathématiques Pure (Vulkan 1.4 Native)
layout(push_constant) uniform PushConstants {
    mat4 mvp_matrix;
    mat4 model_matrix;
    mat4 normal_matrix;
    vec4 glass_tint; // RGB: Teinte / Absorption, A: Épaisseur (d)
    vec4 params;     // X: Rugosité (0.0 = Cristallin, 1.0 = Dépoli), Y: IOR (1.48)
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 3.5);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.25, 5.8);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float ior = pc.params.y > 0.0 ? pc.params.y : 1.48; // Indice de réfraction du verre = 1.48
    float epaisseur = pc.glass_tint.w;

    // ------------------------------------------------------------------------
    // 1. REFRACTION OPTIQUE SNELL-DESCARTES PURE (ZÉRO FLOU / CRYSTAL CLEAR)
    // ------------------------------------------------------------------------
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // Déformation optique exacte par la normale de surface (Snell Bend)
    vec2 refraction_offset = (N.xy * 0.16 + refract_dir.xy * 0.18) * (1.0 - N.z * N.z * 0.65);

    // ECHANTILLONNAGE DIRECT (ZÉRO FLOU / Mip level = 0.0)
    vec2 uv_sample = clamp(in_screen_uv + refraction_offset, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, 0.0).rgb;

    // ------------------------------------------------------------------------
    // 2. TRANSMISSION DIELECTRIQUE SANS EXTINCTION SOMBRE (Mélange Translucide)
    // ------------------------------------------------------------------------
    // Réfraction pure de l'image arrière transmise infusée de la couleur du verre
    vec3 couleur_transmise = mix(fond_transmis, pc.glass_tint.rgb, 0.35);

    // ------------------------------------------------------------------------
    // 3. FRESNEL & REFLETS SPECULAIRES 360° HIGH-GLOSS (Verre Polie Cristallin)
    // ------------------------------------------------------------------------
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 lumière_clef = normalize(vec3(3.2, 4.0, 3.0));
    vec3 lumière_liseré = normalize(vec3(-3.5, -2.0, 3.0));

    vec3 H_clef = normalize(V + lumière_clef);
    vec3 H_liseré = normalize(V + lumière_liseré);

    // Spéculaire de surface polie très vif
    float spec_surface = pow(max(dot(N, H_clef), 0.0), 128.0) * 0.65 * fresnel;
    vec3 reflet_surface = vec3(1.0, 1.0, 1.0) * spec_surface;

    // FRESNEL CONTINU SUR LE PERIMETRE 360° (Reflet de tranche rase)
    float fresnel_perimetre = pow(1.0 - NdotV, 3.0) * 0.85;
    vec3 reflet_perimetre = vec3(0.92, 0.97, 1.00) * fresnel_perimetre;

    // Liseré spéculaire additionnel sur la courbure du biseau
    float spec_liseré = pow(max(dot(N, H_liseré), 0.0), 96.0) * 90.0;
    vec3 glow_cyan = vec3(0.70, 0.95, 1.00) * spec_liseré * step(0.10, 1.0 - abs(N.z));

    vec3 rgb_final = couleur_transmise + reflet_surface + reflet_perimetre + glow_cyan;

    out_color = vec4(clamp(rgb_final, 0.0, 1.0), 1.0);
}
