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
    // 1. REFRACTION OPTIQUE LINEAIRE SANS ONDULATION NI VAGUE (Géométrie Droite)
    // ------------------------------------------------------------------------
    // Réfraction optique Snell vectorielle linéaire constante (Empêche toute vague/bosses sur les masques)
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // Offset vectoriel Snell linéaire constant
    vec2 refraction_offset = refract_dir.xy * 0.08;
    vec2 uv_sample = clamp(in_screen_uv + refraction_offset, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. FUSION OPTIQUE CRISTALLINE (Superposition 2.0x Lumineuse)
    // ------------------------------------------------------------------------
    // La tranche et le corps transmettent tous les deux la lumière avec filtrage optique 2.0x
    vec3 couleur_transmise = clamp(2.0 * fond_transmis * pc.glass_tint.rgb, vec3(0.0), vec3(1.0));

    // Dégradé de lumière studio douce sur la surface
    float éclairage_studio = 0.85 + 0.30 * max(dot(N, vec3(0.3, 0.6, 0.7)), 0.0);
    couleur_transmise *= éclairage_studio;

    // ------------------------------------------------------------------------
    // 3. REFLETS SPÉCULAIRES BLANCS PURS INTENSES (#FFFFFF) SUR LA TRANCHE ET LA SURFACE
    // ------------------------------------------------------------------------
    vec3 lumière_source1 = normalize(vec3(3.2, 4.5, 4.0));
    vec3 lumière_source2 = normalize(vec3(-2.8, 3.2, 4.5));

    vec3 H1 = normalize(V + lumière_source1);
    vec3 H2 = normalize(V + lumière_source2);

    // Points chauds spéculaires blancs purs hyper intenses
    float spec_tranche = pow(max(dot(N, H1), 0.0), 128.0) * 2.8;
    float spec_surface = pow(max(dot(N, H2), 0.0), 256.0) * 1.5;

    vec3 reflets_blancs = vec3(1.0, 1.0, 1.0) * (spec_tranche + spec_surface);

    // Liseré blanc pur (#FFFFFF) très fin et très lumineux sur le périmètre de la tranche
    float liseré_périmètre = pow(1.0 - NdotV, 7.0) * 4.0;
    vec3 fil_lumineux = vec3(1.0, 1.0, 1.0) * liseré_périmètre;

    // Assemblage final pure matière verre
    vec3 rgb_final = couleur_transmise + reflets_blancs + fil_lumineux;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
