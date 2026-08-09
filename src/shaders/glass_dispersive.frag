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
    vec4 glass_tint; // RGB: Couleur de teinte optique du verre
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
    // 2. FUSION OPTIQUE SPECTRALE (Mode Multiplication Lumineuse 2.0x / Overlay)
    // ------------------------------------------------------------------------
    // Remplacement de la moyenne platte mix() par la vraie multiplication de filtre optique lumineux :
    // La lumière du fond est filtrée par le verre avec compensation de luminance 2.0x (Pas de gris mort)
    vec3 fusion_optique = 2.0 * fond_transmis * pc.glass_tint.rgb;
    vec3 couleur_transmise = clamp(fusion_optique, vec3(0.0), vec3(1.0));

    // ------------------------------------------------------------------------
    // 3. LIGNE FINE BLANCHE HYPER BRILLANTE SUR TOUT LE CONTOUR (360° Razor-Sharp Rim)
    // ------------------------------------------------------------------------
    float contour_brillant = pow(1.0 - NdotV, 8.0) * 4.5;
    vec3 ligne_blanche = vec3(1.0, 1.0, 1.0) * contour_brillant;

    // Speculaire de surface polie
    vec3 lumière_clef = normalize(vec3(3.2, 4.0, 3.0));
    vec3 H_clef = normalize(V + lumière_clef);
    float spec_surface = pow(max(dot(N, H_clef), 0.0), 128.0) * 0.85;
    vec3 reflet_surface = vec3(1.0, 1.0, 1.0) * spec_surface;

    vec3 rgb_final = couleur_transmise + ligne_blanche + reflet_surface;

    out_color = vec4(clamp(rgb_final, 0.0, 1.0), 1.0);
}
