// Shader Studio Background Photoréaliste - Reconstitution exacte de la photo de référence
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0)
    );
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );

    out.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    
    // Dégradé Fond Studio Cinématographique
    let top_glow = vec3<f32>(0.97, 0.98, 1.00);      // Blanc Pur Studio avec douce lueur
    let bottom_tint = vec3<f32>(0.85, 0.91, 0.98);   // Bleu Glacial Doux
    
    let base_v = mix(bottom_tint, top_glow, pow(uv.y, 0.85));

    // Bande d'ombre verticale douce sur le côté gauche (Mur Studio en léger retrait)
    let left_wall_shadow = exp(-pow(uv.x - 0.22, 2.0) * 12.0) * 0.14;
    let soft_shadow_gradient = vec3<f32>(left_wall_shadow * 0.4, left_wall_shadow * 0.35, left_wall_shadow * 0.25);

    // Doux projecteur studio supérieur droit (Softbox Key Light)
    let softbox = exp(-length(uv - vec2<f32>(0.85, 0.80)) * 1.8) * 0.08;

    let final_rgb = clamp(base_v - soft_shadow_gradient + vec3<f32>(softbox), vec3<f32>(0.0), vec3<f32>(1.0));
    
    return vec4<f32>(final_rgb, 1.0);
}
