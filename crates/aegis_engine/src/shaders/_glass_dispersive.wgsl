// Shader 3D Vulkan 1.4 Native : Rendu Photoréaliste de Dalles de Verre Dépoli (Frosted BSDF)
// Réfraction Dépolie Laiteuse "Verre Poli par la Mer", Absorption Volumétrique Saphir & Liseré Cyan Fibre Optique (#00E5FF)

struct PushConstants {
    mvp_matrix: mat4x4<f32>,
    model_matrix: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    glass_tint: vec4<f32>, // (RGB Teinte, Épaisseur cm)
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
    @location(3) screen_uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = pc.model_matrix * vec4<f32>(in.position, 1.0);
    let clip_pos = pc.mvp_matrix * vec4<f32>(in.position, 1.0);

    out.clip_position = clip_pos;
    out.world_position = world_pos.xyz;
    out.world_normal = normalize((pc.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv = in.uv0;

    let ndc = clip_pos.xy / clip_pos.w;
    out.screen_uv = vec2<f32>(ndc.x * 0.5 + 0.5, ndc.y * 0.5 + 0.5);

    return out;
}

const PI: f32 = 3.14159265359;

fn fresnel_schlick(cos_theta: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@group(0) @binding(0) var transmission_texture: texture_2d<f32>;
@group(0) @binding(1) var transmission_sampler: sampler;

// Noyau de Flou Dépoli Laiteux 25-Taps "Verre Poli par la Mer" (Diffusion Veloutée et Dense)
fn sample_frosted_background(base_uv: vec2<f32>, blur_radius: f32) -> vec3<f32> {
    let r = blur_radius * 0.125; // Large rayon pour diffusion laiteuse et fondu complet du fond
    
    let offsets = array<vec2<f32>, 25>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(-r*0.5, -r*0.5), vec2<f32>(r*0.5, -r*0.5), vec2<f32>(-r*0.5, r*0.5), vec2<f32>(r*0.5, r*0.5),
        vec2<f32>(-r, 0.0),      vec2<f32>(r, 0.0),       vec2<f32>(0.0, -r),     vec2<f32>(0.0, r),
        vec2<f32>(-r*0.8, -r*0.8), vec2<f32>(r*0.8, -r*0.8), vec2<f32>(-r*0.8, r*0.8), vec2<f32>(r*0.8, r*0.8),
        vec2<f32>(-r*1.5, 0.0),  vec2<f32>(r*1.5, 0.0),   vec2<f32>(0.0, -r*1.5), vec2<f32>(0.0, r*1.5),
        vec2<f32>(-r*1.3, -r*1.3), vec2<f32>(r*1.3, -r*1.3), vec2<f32>(-r*1.3, r*1.3), vec2<f32>(r*1.3, r*1.3),
        vec2<f32>(-r*2.2, 0.0),  vec2<f32>(r*2.2, 0.0),   vec2<f32>(0.0, -r*2.2), vec2<f32>(0.0, r*2.2)
    );

    let weights = array<f32, 25>(
        0.12,
        0.07, 0.07, 0.07, 0.07,
        0.05, 0.05, 0.05, 0.05,
        0.04, 0.04, 0.04, 0.04,
        0.03, 0.03, 0.03, 0.03,
        0.02, 0.02, 0.02, 0.02,
        0.01, 0.01, 0.01, 0.01
    );

    var color = vec3<f32>(0.0);
    for (var i = 0; i < 25; i++) {
        let sample_uv = clamp(base_uv + offsets[i], vec2<f32>(0.001), vec2<f32>(0.999));
        let bg_sample = textureSample(transmission_texture, transmission_sampler, sample_uv).rgb;
        color += bg_sample * weights[i];
    }
    return color;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let camera_pos = vec3<f32>(0.0, 0.0, 4.4);
    let V = normalize(camera_pos - in.world_position);

    let NdotV = abs(dot(N, V));

    // 1. Réfraction Snell-Descartes & Flou Dépoli Laiteux "Verre Poli par la Mer"
    let ior = 1.52;
    let eta = 1.0 / ior;

    let k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    var refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    let glass_thickness = pc.glass_tint.w;
    let refraction_offset = (refract_dir.xy + N.xy * 0.45) * (glass_thickness * 0.40);

    // Flou laiteux très dense
    let roughness_blur = 1.6 + (1.0 - NdotV) * 3.5;

    // Échantillonnage avec dispersion chromatique Cauchy RGB + Flou Laiteux
    let uv_r = in.screen_uv + refraction_offset * 1.20;
    let uv_g = in.screen_uv + refraction_offset * 1.00;
    let uv_b = in.screen_uv + refraction_offset * 0.80;

    let sample_r = sample_frosted_background(uv_r, roughness_blur);
    let sample_g = sample_frosted_background(uv_g, roughness_blur);
    let sample_b = sample_frosted_background(uv_b, roughness_blur);

    let frosted_refracted_bg = vec3<f32>(sample_r.x, sample_g.y, sample_b.z);

    // 2. Absorption Volumétrique de Beer-Lambert (Bleu Saphir Profond Volumétrique)
    let optical_path = glass_thickness / (NdotV + 0.04);
    let sigma_a = vec3<f32>(3.6, 1.20, 0.02); // Bleu Saphir intense au cœur de la dalle
    let beer_lambert_decay = exp(-sigma_a * optical_path);

    let transmitted_color = frosted_refracted_bg * pc.glass_tint.rgb * beer_lambert_decay;

    // 3. Éclairage Spéculaire HDR & Liseré Cyan Électrique "Fibre Optique" (#00E5FF)
    let fresnel = fresnel_schlick(NdotV, 0.08);

    let light1_dir = normalize(vec3<f32>(-3.8, -2.2, 3.2)); // Rim Light Bottom-Left
    let light2_dir = normalize(vec3<f32>(3.5, 4.5, 2.5));  // Key Light Top-Right

    let H1 = normalize(V + light1_dir);
    let H2 = normalize(V + light2_dir);

    // Détection stricte de la tranche 90° et du chanfrein 45°
    let is_side_wall = step(abs(N.z), 0.20);
    let is_chamfer = step(0.20, abs(N.z)) * step(abs(N.z), 0.90);

    let spec1 = pow(max(dot(N, H1), 0.0), 96.0) * 80.0;
    let spec2 = pow(max(dot(N, H2), 0.0), 48.0) * 20.0;

    // Liseré Cyan Électrique ultra-net (#00E5FF) de 1 pixel sur le chanfrein à 45° (Fibre Optique)
    let cyan_glow_tint = vec3<f32>(0.00, 0.90, 1.00);
    let chamfer_specular = mix(vec3<f32>(1.0), cyan_glow_tint, is_chamfer * 0.95) * (spec1 * is_chamfer + spec2 * 0.3);

    // Tranche sombre 90° franche
    let side_wall_darkening = mix(transmitted_color, vec3<f32>(0.01, 0.08, 0.35) * beer_lambert_decay, is_side_wall * 0.92);

    // Composition Finale BSDF Verre Dépoli Satiné Photoréaliste
    let final_rgb = mix(side_wall_darkening, vec3<f32>(0.98, 1.0, 1.0), fresnel) + chamfer_specular;

    let alpha = clamp(0.45 + fresnel * 0.50 + (1.0 - beer_lambert_decay.r) * 0.45, 0.40, 0.98);

    // Tonemapping Filmique
    let tonemapped = final_rgb / (final_rgb + vec3<f32>(0.52));
    let gamma_corrected = pow(tonemapped, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(gamma_corrected, alpha);
}
