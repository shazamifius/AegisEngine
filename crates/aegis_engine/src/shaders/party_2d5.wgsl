struct PushConstants {
    mvp_matrix: mat4x4<f32>,
    model_matrix: mat4x4<f32>,
    color_tint: vec4<f32>,
    params: vec4<f32>,
};

var<push_constant> pc: PushConstants;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = pc.model_matrix * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;

    let normal_matrix = mat3x3<f32>(
        pc.model_matrix[0].xyz,
        pc.model_matrix[1].xyz,
        pc.model_matrix[2].xyz
    );
    out.world_normal = normalize(normal_matrix * in.normal);

    out.color = pc.color_tint;
    out.uv = in.uv0;
    out.params = pc.params;

    out.clip_position = pc.mvp_matrix * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let light_dir = normalize(vec3<f32>(0.4, 0.9, 0.7));

    let NdotL = max(dot(N, light_dir), 0.0);
    let diffuse = vec3<f32>(1.0, 0.96, 0.88) * NdotL * 0.65;
    let ambient = vec3<f32>(0.35, 0.40, 0.45);

    let final_color = in.color.rgb * (ambient + diffuse);
    let gamma_corrected = pow(final_color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(gamma_corrected, 1.0);
}
