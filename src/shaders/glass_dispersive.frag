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
    // 1. REFRACTION OPTIQUE SNELL-DESCARTES (Déformation des contours UV)
    // ------------------------------------------------------------------------
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    vec2 refraction_offset = (N.xy * 0.16 + refract_dir.xy * 0.18) * (1.0 - N.z * N.z * 0.65);
    vec2 uv_sample = clamp(in_screen_uv + refraction_offset, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. FUSION OPTIQUE SPECTRALE (Multiplication Lumineuse 2.0x Validée)
    // ------------------------------------------------------------------------
    vec3 fusion_optique = 2.0 * fond_transmis * pc.glass_tint.rgb;
    vec3 couleur_transmise = clamp(fusion_optique, vec3(0.0), vec3(1.0));

    // ------------------------------------------------------------------------
    // 3. CONTOUR LUMINEUX BLANC SUR 360° (Silhouette Edge Rim)
    // ------------------------------------------------------------------------
    // Smoothstep tranchant sur le bord extérieur pour une ligne fine blanche éclatante (#FFFFFF)
    float fil_blanc_intensité = smoothstep(0.38, 0.05, NdotV) * 1.8;
    vec3 ligne_blanche = vec3(1.0, 1.0, 1.0) * fil_blanc_intensité;

    // ------------------------------------------------------------------------
    // 4. REFLETS SPÉCULAIRES NETS (Points d'éclat lumineux studio)
    // ------------------------------------------------------------------------
    vec3 lumière_spot1 = normalize(vec3(4.0, 5.0, 4.0));
    vec3 lumière_spot2 = normalize(vec3(-2.0, 3.5, 5.0));

    vec3 H1 = normalize(V + lumière_spot1);
    vec3 H2 = normalize(V + lumière_spot2);

    // Éclat spéculaire de surface très poli (Spot studio intense)
    float spec1 = pow(max(dot(N, H1), 0.0), 256.0) * 1.6;
    float spec2 = pow(max(dot(N, H2), 0.0), 96.0) * 0.5;
    vec3 reflets_spéculaires = vec3(1.0, 1.0, 1.0) * (spec1 + spec2);

    // ------------------------------------------------------------------------
    // 5. TRANCHE CRISTALLINE FINESSE (Reflet translucide de la tranche)
    // ------------------------------------------------------------------------
    float tranche_finesse = smoothstep(0.65, 0.35, NdotV) * smoothstep(0.05, 0.25, NdotV) * 0.6;
    vec3 reflet_tranche = vec3(0.90, 0.96, 1.00) * tranche_finesse;

    // Assemblage final pure matière verre
    vec3 rgb_final = couleur_transmise + ligne_blanche + reflets_spéculaires + reflet_tranche;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
