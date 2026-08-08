// Shader de Rastérisation Logicielle Compute (Nanite-Style Software Rasterizer)
// Utilise des Atomics 64-bits (Depth32 | PrimitiveID32) pour traiter les micropolygones

struct PrimitiveData {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    primitive_id: u32,
    _padding: vec3<u32>,
}

@group(0) @binding(0) var<storage, read> u_Primitives: array<PrimitiveData>;
@group(0) @binding(1) var<storage, read_write> u_VisibilityBuffer: array<atomic<u32>>; // Simulée par 2x uint32 (Depth & PrimID)

struct Uniforms {
    screen_width: u32,
    screen_height: u32,
    primitive_count: u32,
    _pad: u32,
}

@group(0) @binding(2) var<uniform> u_Params: Uniforms;

fn compute_barycentric(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> vec3<f32> {
    let v0 = b - a;
    let v1 = c - a;
    let v2 = p - a;

    let d00 = dot(v0, v0);
    let d01 = dot(v0, v1);
    let d11 = dot(v1, v1);
    let d20 = dot(v2, v0);
    let d21 = dot(v2, v1);

    let denom = d00 * d11 - d01 * d01;
    if (abs(denom) < 1e-6) {
        return vec3<f32>(-1.0, -1.0, -1.0);
    }

    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;

    return vec3<f32>(u, v, w);
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let prim_index = global_id.x;
    if (prim_index >= u_Params.primitive_count) {
        return;
    }

    let prim = u_Primitives[prim_index];

    let p0 = prim.v0.xy / prim.v0.w;
    let p1 = prim.v1.xy / prim.v1.w;
    let p2 = prim.v2.xy / prim.v2.w;

    // Conversion en coordonnées d'écran
    let screen_dim = vec2<f32>(f32(u_Params.screen_width), f32(u_Params.screen_height));
    let screen_p0 = (p0 * 0.5 + vec2<f32>(0.5)) * screen_dim;
    let screen_p1 = (p1 * 0.5 + vec2<f32>(0.5)) * screen_dim;
    let screen_p2 = (p2 * 0.5 + vec2<f32>(0.5)) * screen_dim;

    let min_p = clamp(min(min(screen_p0, screen_p1), screen_p2), vec2<f32>(0.0), screen_dim - vec2<f32>(1.0));
    let max_p = clamp(max(max(screen_p0, screen_p1), screen_p2), vec2<f32>(0.0), screen_dim - vec2<f32>(1.0));

    let start_x = u32(min_p.x);
    let start_y = u32(min_p.y);
    let end_x = u32(max_p.x);
    let end_y = u32(max_p.y);

    for (var y = start_y; y <= end_y; y++) {
        for (var x = start_x; x <= end_x; x++) {
            let pixel_pos = vec2<f32>(f32(x) + 0.5, f32(y) + 0.5);
            let bary = compute_barycentric(pixel_pos, screen_p0, screen_p1, screen_p2);

            if (bary.x >= 0.0 && bary.y >= 0.0 && bary.z >= 0.0) {
                let depth = bary.x * prim.v0.z + bary.y * prim.v1.z + bary.z * prim.v2.z;
                let pixel_index = y * u_Params.screen_width + x;

                // Mise à jour atomique de la visibilité la plus proche (Z-buffer)
                let depth_bits = bitcast<u32>(depth);
                atomicMax(&u_VisibilityBuffer[pixel_index * 2u], depth_bits);
                atomicStore(&u_VisibilityBuffer[pixel_index * 2u + 1u], prim.primitive_id);
            }
        }
    }
}
