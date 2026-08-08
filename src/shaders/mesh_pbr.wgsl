// Shader 3D Native Vulkan 1.4 : Éclairage PBR Cook-Torrance Specular & Diffuse sans couture

struct PushConstants {
    mvp_matrix: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
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
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = pc.mvp_matrix * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.world_normal = normalize((pc.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv = in.uv0;
    return out;
}

const PI: f32 = 3.14159265359;

// Fresnel Schlick
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Distribution Micro-facettes GGX (Trowbridge-Reitz)
fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    let denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return num / (PI * denom * denom + 0.0001);
}

// Occultation Géométrique Schlick-GGX
fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = (roughness + 1.0);
    let k = (r * r) / 8.0;
    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;
    return num / (denom + 0.0001);
}

fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx2 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(vec3<f32>(0.0, 1.2, 3.5) - in.world_position); // Vue Caméra

    // Propriétés du matériau PBR (Orbe Métallique Bleu Saphir)
    let albedo = vec3<f32>(0.15, 0.45, 0.90);
    let roughness = 0.25;
    let metallic = 0.85;

    let f0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Lumière Principale (Soleil)
    let light_pos = vec3<f32>(2.0, 3.0, 2.5);
    let L = normalize(light_pos - in.world_position);
    let H = normalize(V + L);

    let radiance = vec3<f32>(3.0, 2.8, 2.5); // Éclairage Blanc Chaud 3.0 Lux

    // BRDF Cook-Torrance
    let NDF = distribution_ggx(N, H, roughness);
    let G = geometry_smith(N, V, L, roughness);
    let F = fresnel_schlick(max(dot(H, V), 0.0), f0);

    let kS = F;
    let kD = (vec3<f32>(1.0) - kS) * (1.0 - metallic);

    let numerator = NDF * G * F;
    let denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
    let specular = numerator / denominator;

    let NdotL = max(dot(N, L), 0.0);
    let Lo = (kD * albedo / PI + specular) * radiance * NdotL;

    // Lumière d'Ambiance Sphérique
    let ambient = vec3<f32>(0.05, 0.08, 0.12) * albedo;
    let color = ambient + Lo;

    // Correction Gamma
    let tonemapped = color / (color + vec3<f32>(1.0));
    let gamma_corrected = pow(tonemapped, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(gamma_corrected, 1.0);
}
