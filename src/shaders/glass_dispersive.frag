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
    vec4 glass_tint; // RGB: Couleur de teinte / Absorption, A: Épaisseur (d)
    vec4 params;     // X: Rugosité Dépolie (0.0 = Cristallin, 1.0 = Sablé), Y: Indice Réfraction (IOR = 1.48)
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

// Formule de Réflexion Spéculaire Fresnel (Approximation de Schlick)
// F(θ) = F0 + (1 - F0) * (1 - cos(θ))^5
float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.0, 4.4);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float rugosite = clamp(pc.params.x, 0.0, 1.0);
    float ior = pc.params.y > 0.0 ? pc.params.y : 1.48; // Indice de réfraction du verre = 1.48
    float epaisseur = pc.glass_tint.w;

    // ------------------------------------------------------------------------
    // 1. LOI VECTORIELLE DE SNELL-DESCARTES (Réfraction Optique Exacte)
    // T = η*V + (η*(N·V) - sqrt(1 - η²*(1 - (N·V)²))) * N
    // ------------------------------------------------------------------------
    float eta = 1.0 / ior; // Air (1.0) vers Verre (1.48)
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // ------------------------------------------------------------------------
    // 2. INTEGRALE DE DISPERSION LAITEUSE (Flou du Verre Sablé / Dépoli)
    // Le flou laiteux est obtenu par l'échantillonnage de la pyramide d'images
    // Mipmaps générée en matériel GPU (Vulkan vkCmdBlitImage).
    // ------------------------------------------------------------------------
    float mip_level = rugosite * 4.5;
    vec2 refraction_offset = refract_dir.xy * (epaisseur * 0.12);

    // Noyau de convolution 9-Taps autour du rayon réfracté
    float r = rugosite * 0.040;
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
    // 3. LOI DE BEER-LAMBERT (Absorption Volumétrique de la Lumière)
    // I(d) = I0 * exp(-sigma * d)
    // ------------------------------------------------------------------------
    float trajet_optique = epaisseur / (NdotV + 0.10);
    vec3 sigma_absorption = (vec3(1.0) - pc.glass_tint.rgb) * 1.4 + vec3(0.02, 0.01, 0.00);
    vec3 attenuation_beer_lambert = exp(-sigma_absorption * trajet_optique);

    vec3 couleur_transmise = fond_transmis * attenuation_beer_lambert;

    // ------------------------------------------------------------------------
    // 4. REFLEXION SPECULAIRE & LISERE CYAN (Chanfrein 45° et Lumière Studio)
    // ------------------------------------------------------------------------
    float fresnel = fresnel_schlick(NdotV, 0.06);

    vec3 lumière_clef = normalize(vec3(3.2, 4.0, 3.0));
    vec3 lumière_liseré = normalize(vec3(-3.5, -2.0, 3.0));

    vec3 H_clef = normalize(V + lumière_clef);
    vec3 H_liseré = normalize(V + lumière_liseré);

    // Reflet spéculaire sur la face plate
    float spec_surface = pow(max(dot(N, H_clef), 0.0), 32.0) * 0.25 * fresnel;
    vec3 reflet_surface = vec3(0.95, 0.98, 1.00) * spec_surface;

    // Détection géométrique des chanfreins à 45° et tranches à 90°
    float est_chanfrein = step(0.15, abs(N.z)) * step(abs(N.z), 0.88);
    float est_tranche_verticale = step(abs(N.z), 0.15);

    // Liseré Cyan Électrique 1-Pixel (#00E5FF) accroché sur le chanfrein 45°
    float spec_liseré = pow(max(dot(N, H_liseré), 0.0), 160.0) * 80.0;
    vec3 glow_cyan = vec3(0.00, 0.95, 1.00) * spec_liseré * est_chanfrein;

    // Assombrissement réaliste de la tranche 90°
    vec3 couleur_tranche = mix(couleur_transmise, vec3(0.04, 0.18, 0.45) * attenuation_beer_lambert, est_tranche_verticale * 0.70);

    // Assemblage final pure lumière
    vec3 rgb_final = couleur_tranche + reflet_surface + glow_cyan;

    out_color = vec4(clamp(rgb_final, 0.0, 1.0), 1.0);
}
