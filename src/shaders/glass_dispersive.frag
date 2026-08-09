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
    vec4 glass_tint; // RGB: Teinte optique du verre
    vec4 params;     // X: Rugosité, Y: IOR (1.48)
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.25, 5.8);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float ior = pc.params.y > 0.0 ? pc.params.y : 1.48;

    // ------------------------------------------------------------------------
    // 1. REFRACTION OPTIQUE LISSE SANS CROCHET ESCALIER
    // ------------------------------------------------------------------------
    // Déplacement optique vectoriel Snell basé sur le rayon de visée V (Continu sans cassure d'escalier)
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    vec2 refraction_offset = refract_dir.xy * 0.12 * (1.0 - NdotV * 0.5);
    vec2 uv_sample = clamp(in_screen_uv + refraction_offset, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. FUSION OPTIQUE & TRANSLUCIDITÉ DU BISEAU (Élimination du plastique opaque)
    // ------------------------------------------------------------------------
    // Détection physique de la pente du biseau (tranche inclinée vs face centrale)
    float est_biseau = smoothstep(0.96, 0.75, abs(N.z));

    // Corps de la plaque : fusion optique 2.0x lumineuse
    vec3 couleur_corps = clamp(2.0 * fond_transmis * pc.glass_tint.rgb, vec3(0.0), vec3(1.0));

    // Tranche biseautée : translucide cristalline (laisse parfaitement voir le fond rose à travers)
    vec3 couleur_biseau = mix(fond_transmis, pc.glass_tint.rgb, 0.28);

    vec3 couleur_transmise = mix(couleur_corps, couleur_biseau, est_biseau);

    // Dégradé de lumière ambiante sur la surface (Lumière de studio douce)
    float éclairage_ambiant = 0.85 + 0.30 * max(dot(N, vec3(0.3, 0.6, 0.7)), 0.0);
    couleur_transmise *= éclairage_ambiant;

    // ------------------------------------------------------------------------
    // 3. SPÉCULAIRES BRILLANTS ET LISERÉ LUMINEUX BLANC SUR 360° (Matière Verre Polie)
    // ------------------------------------------------------------------------
    // Reflet spéculaire d'une fenêtre / softbox de studio sur la surface polie
    vec3 lumière_fenêtre1 = normalize(vec3(3.5, 4.5, 4.0));
    vec3 lumière_fenêtre2 = normalize(vec3(-2.5, 3.0, 5.0));

    vec3 H1 = normalize(V + lumière_fenêtre1);
    vec3 H2 = normalize(V + lumière_fenêtre2);

    float spec1 = pow(max(dot(N, H1), 0.0), 128.0) * 1.35; // Éclat intense de fenêtre
    float spec2 = pow(max(dot(N, H2), 0.0), 64.0) * 0.45;  // Reflet secondaire
    vec3 reflets_spéculaires = vec3(1.0, 1.0, 1.0) * (spec1 + spec2);

    // Liseré lumineux blanc pur (#FFFFFF) très vif et ultra-fin le long de la tranche
    float liseré_tranche = pow(1.0 - NdotV, 6.0) * 3.8;
    vec3 fil_lumineux = vec3(1.0, 1.0, 1.0) * liseré_tranche;

    // Assemblage final pure matière verre
    vec3 rgb_final = couleur_transmise + reflets_spéculaires + fil_lumineux;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
