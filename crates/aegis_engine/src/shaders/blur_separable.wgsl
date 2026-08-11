// Shader de Flou Gaussien Séparable 15-Taps pour Effet Verre Dépoli Laiteux (Frosted Diffusion)
struct PushConstants {
    direction: vec2<f32>, // (1.0/w, 0.0) ou (0.0, 1.0/h)
};

var<push_constant> pc: PushConstants;

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

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let dir = pc.direction * 3.2; // Multiplicateur de rayon pour flou laiteux et dense

    let weights = array<f32, 15>(
        0.132,
        0.125, 0.125,
        0.106, 0.106,
        0.082, 0.082,
        0.057, 0.057,
        0.036, 0.036,
        0.020, 0.020,
        0.010, 0.010
    );

    let offsets = array<f32, 15>(
        0.0,
        1.414, -1.414,
        3.294, -3.294,
        5.176, -5.176,
        7.058, -7.058,
        8.941, -8.941,
        10.823, -10.823,
        12.705, -12.705
    );

    var color = vec3<f32>(0.0);
    for (var i = 0; i < 15; i++) {
        let sample_uv = clamp(uv + dir * offsets[i], vec2<f32>(0.001), vec2<f32>(0.999));
        color += textureSample(input_texture, input_sampler, sample_uv).rgb * weights[i];
    }

    return vec4<f32>(color, 1.0);
}
