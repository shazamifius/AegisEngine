#version 450

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_tangent;
layout(location = 3) in vec2 in_uv0;
layout(location = 4) in vec2 in_uv1;

layout(push_constant) uniform PushConstants {
    mat4 mvp_matrix;
    mat4 model_matrix;
    mat4 normal_matrix;
    vec4 glass_tint;
} pc;

layout(location = 0) out vec3 out_world_position;
layout(location = 1) out vec3 out_world_normal;
layout(location = 2) out vec2 out_uv;
layout(location = 3) out vec2 out_screen_uv;

void main() {
    vec4 world_pos = pc.model_matrix * vec4(in_position, 1.0);
    vec4 clip_pos = pc.mvp_matrix * vec4(in_position, 1.0);

    gl_Position = clip_pos;
    out_world_position = world_pos.xyz;
    out_world_normal = normalize(mat3(pc.normal_matrix) * in_normal);
    out_uv = in_uv0;

    vec2 ndc = clip_pos.xy / clip_pos.w;
    out_screen_uv = ndc * 0.5 + 0.5;
}
