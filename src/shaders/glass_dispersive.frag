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

// Formule de Réflexion Spéculaire Fresnel (Approximation de Schlick F0 = 0.08 pour le verre)
float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 4.0);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.25, 5.8);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float rugosite = clamp(pc.params.x, 0.0, 1.0);
    float ior = pc.params.y > 0.0 ? pc.params.y : 1.48; // Indice de réfraction du verre = 1.48
    float epaisseur = pc.glass_tint.w;

    // ------------------------------------------------------------------------
    // 1. REFRACTION OPTIQUE SNELL-DESCARTES (Torsion & Déformation Optique UV)
    // ------------------------------------------------------------------------
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // Déformation optique continue sur tous les bords
    vec2 refraction_offset = refract_dir.xy * (epaisseur * 0.40) + N.xy * (0.06 * (1.0 - NdotV));

    // ------------------------------------------------------------------------
    // 2. INTEGRALE DE DISPERSION LAITEUSE (Flou Pyramide Mipmaps VRAM)
    // ------------------------------------------------------------------------
    float mip_level = rugosite * 4.0;
    float r = rugosite * 0.032;
    vec2 offsets[9] = vec2[](
        vec2(0.0, 0.0),
        vec2(-r, -r), vec2(r, -r), vec2(-r, r), vec2(r, r),
        vec2(-r * 1.4, 0.0), vec2(r * 1.4, 0.0), vec2(0.0, -r * 1.4), vec2(0.0, r * 1.4)
    );

    vec3 fond_transmis = vec3(0.0);
    for (int i = 0; i < 9; i++) {
        vec2 uv_sample = clamp(in_screen_uv + refraction_offset + offsets[i], vec2(0.001), vec2(0.999));
        fond_transmis += textureLod(sampler2D(transmission_texture, transmission_sampler), uv_sample, mip_level).rgb * (1.0 / 9.0);
    }

    // ------------------------------------------------------------------------
    // 3. TRANSMISSION LUMINEUSE CONTINU & CONSERVATION DE COULEUR (Pas d'extinction terne)
    // ------------------------------------------------------------------------
    float trajet_optique = epaisseur / (NdotV + 0.10);
    // Atténuation ultra-doucement dosée pour préserver 90%+ des couleurs sous-jacentes (Rouge, Vert)
    vec3 sigma_absorption = (vec3(1.0) - pc.glass_tint.rgb) * 0.15 + vec3(0.002);
    vec3 attenuation_beer_lambert = exp(-sigma_absorption * trajet_optique);

    vec3 fond_attenué = fond_transmis * attenuation_beer_lambert;
    vec3 couleur_transmise = mix(fond_attenué, pc.glass_tint.rgb * fond_attenué, 0.15);

    // ------------------------------------------------------------------------
    // 4. EFFET FRESNEL SUR TOUT LE PERIMETRE & SPECULAIRE STUDIO (360° Rim Light)
    // ------------------------------------------------------------------------
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 lumière_clef = normalize(vec3(3.2, 4.0, 3.0));
    vec3 lumière_liseré = normalize(vec3(-3.5, -2.0, 3.0));

    vec3 H_clef = normalize(V + lumière_clef);
    vec3 H_liseré = normalize(V + lumière_liseré);

    // Reflet spéculaire de surface
    float spec_surface = pow(max(dot(N, H_clef), 0.0), 32.0) * 0.35 * fresnel;
    vec3 reflet_surface = vec3(0.96, 0.99, 1.00) * spec_surface;

    // FRESNEL SUR TOUT LE PERIMETRE (360° du contour de la forme)
    float fresnel_perimetre = pow(1.0 - NdotV, 3.5) * 0.75;
    vec3 reflet_perimetre = vec3(0.85, 0.95, 1.00) * fresnel_perimetre;

    // Eclat additionnel sur le chanfrein 45°
    float est_chanfrein = step(0.15, abs(N.z)) * step(abs(N.z), 0.88);
    float spec_liseré = pow(max(dot(N, H_liseré), 0.0), 120.0) * 80.0;
    vec3 glow_cyan = vec3(0.60, 0.92, 1.00) * spec_liseré * est_chanfrein;

    // Assemblage final pure lumière
    vec3 rgb_final = couleur_transmise + reflet_surface + reflet_perimetre + glow_cyan;

    out_color = vec4(clamp(rgb_final, 0.0, 1.0), 1.0);
}
