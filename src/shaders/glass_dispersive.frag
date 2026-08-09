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
    vec4 glass_tint; // RGB: Couleur saturée, A: Alpha (0.5)
    vec4 params;
} pc;

layout(location = 0) out vec4 out_color;

void main() {
    // TEST ARCHITECTURAL DEMANDÉ : Alpha Blend Simplissime à 50% d'Opacité
    out_color = vec4(pc.glass_tint.rgb, 0.50);
}
