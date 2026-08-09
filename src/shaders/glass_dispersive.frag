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
    // 1. RECALAGE UV 1-TO-1 SANS OFFSET
    // ------------------------------------------------------------------------
    vec2 uv_sample = clamp(in_screen_uv, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. PURE TRANSMISSION CRISTALLINE OPTIQUE (100% Fond Filtré par Absorption)
    // ------------------------------------------------------------------------
    // Le verre est 100% la lumière du fond filtrée par sa couleur pure sans mix() opaque
    vec3 couleur_transmise = clamp(1.8 * fond_transmis * pc.glass_tint.rgb + 0.08 * pc.glass_tint.rgb, vec3(0.0), vec3(1.0));

    // ------------------------------------------------------------------------
    // 3. LISERÉ LUMINEUX BLANC PUR (#FFFFFF) 360° ET SPÉCULAIRES NETS DE SURFACE
    // ------------------------------------------------------------------------
    // Liseré blanc pur (#FFFFFF) d'une finesse extrême cernant 100% de la tranche 360°
    float liseré_tranche = pow(1.0 - NdotV, 7.0) * 5.0;
    vec3 fil_blanc = vec3(1.0, 1.0, 1.0) * liseré_tranche;

    // Point chaud spéculaire blanc pur studio très vif
    vec3 lumière_fenêtre = normalize(vec3(3.5, 5.5, 4.0));
    vec3 H = normalize(V + lumière_fenêtre);
    float spec_spot = pow(max(dot(N, H), 0.0), 256.0) * 4.0;
    vec3 eclat_blanc = vec3(1.0, 1.0, 1.0) * spec_spot;

    vec3 rgb_final = couleur_transmise + fil_blanc + eclat_blanc;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
