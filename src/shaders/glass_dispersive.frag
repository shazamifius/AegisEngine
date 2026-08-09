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

    // ------------------------------------------------------------------------
    // 1. RECALAGE PARFAIT DES UV (Suppression totale du décalage/gélule fantôme)
    // ------------------------------------------------------------------------
    // Échantillonnage direct 1-to-1 sans offset d'UV décalant le masque
    vec2 uv_sample = clamp(in_screen_uv, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. FUSION OPTIQUE CRISTALLINE (Superposition 2.0x Validée à 100%)
    // ------------------------------------------------------------------------
    vec3 couleur_transmise = clamp(2.0 * fond_transmis * pc.glass_tint.rgb, vec3(0.0), vec3(1.0));

    // Dégradé doux de surface (Lumière de studio ambiante)
    float éclairage_ambiant = 0.85 + 0.30 * max(dot(N, vec3(0.3, 0.6, 0.7)), 0.0);
    couleur_transmise *= éclairage_ambiant;

    // ------------------------------------------------------------------------
    // 3. REFLETS SPÉCULAIRES ET LISERÉ LUMINEUX BLANC PUR (#FFFFFF) SUR 360°
    // ------------------------------------------------------------------------
    vec3 lumière_fenêtre1 = normalize(vec3(4.0, 6.0, 5.0));
    vec3 lumière_fenêtre2 = normalize(vec3(-3.0, 4.0, 4.5));

    vec3 H1 = normalize(V + lumière_fenêtre1);
    vec3 H2 = normalize(V + lumière_fenêtre2);

    // Points chauds spéculaires blancs purs très intenses (#FFFFFF)
    float spec_spot1 = pow(max(dot(N, H1), 0.0), 256.0) * 3.5;
    float spec_spot2 = pow(max(dot(N, H2), 0.0), 96.0) * 1.2;
    vec3 reflets_blancs = vec3(1.0, 1.0, 1.0) * (spec_spot1 + spec_spot2);

    // FIL BLANC PUR (#FFFFFF) TRES BRILLANT RECALÉ SUR LE BORD EXACT
    float fil_silhouette = pow(1.0 - NdotV, 6.0) * 4.5;
    vec3 fil_lumineux = vec3(1.0, 1.0, 1.0) * fil_silhouette;

    vec3 rgb_final = couleur_transmise + reflets_blancs + fil_lumineux;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
