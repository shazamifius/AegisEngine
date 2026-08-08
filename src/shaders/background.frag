#version 450

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec2 uv = in_uv;
    vec3 col_center = vec3(0.96, 0.98, 1.00); // Studio Ice Cyan White
    vec3 col_edge = vec3(0.80, 0.88, 0.96);   // Studio Soft Blue Shadow
    
    float dist = length(uv - vec2(0.5));
    vec3 studio_bg = mix(col_center, col_edge, smoothstep(0.1, 0.95, dist));

    // Ombre verticale douce sur la gauche
    float shadow_wall = smoothstep(0.0, 0.45, uv.x);
    vec3 final_bg = mix(vec3(0.68, 0.76, 0.88), studio_bg, shadow_wall);

    out_color = vec4(final_bg, 1.0);
}
