// Shader WGSL de Matériau Verre Physiquement Exact (BSDF Réfractif)
// Intègre la Dispersion Chromatique Cauchy (RGB), l'Absorption Volumétrique de Beer-Lambert et Fresnel Schlick

struct GlassUniforms {
    ior_base: f32,
    dispersion: f32,
    roughness: f32,
    thickness: f32,
    absorption_rgb: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u_Glass: GlassUniforms;
@group(0) @binding(1) var u_EnvSampler: sampler;
@group(0) @binding(2) var u_EnvTexture: texture_cube<f32>;

fn fresnel_schlick(cos_theta: f32, ior: f32) -> f32 {
    let r0 = pow((1.0 - ior) / (1.0 + ior), 2.0);
    return r0 + (1.0 - r0) * pow(1.0 - clamp(cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn fs_main(
    @location(0) in_normal: vec3<f32>,
    @location(1) in_view_dir: vec3<f32>
) -> @location(0) vec4<f32> {
    let N = normalize(in_normal);
    let V = normalize(in_view_dir);

    // 1. Dispersion Chromatique (Cauchy RGB)
    let ior_r = u_Glass.ior_base - u_Glass.dispersion * 0.1;
    let ior_g = u_Glass.ior_base;
    let ior_b = u_Glass.ior_base + u_Glass.dispersion * 0.1;

    let refr_r = refract(-V, N, 1.0 / ior_r);
    let refr_g = refract(-V, N, 1.0 / ior_g);
    let refr_b = refract(-V, N, 1.0 / ior_b);

    // Échantillonnage de la texture d'environnement réfractée pour R, G, B
    let sample_r = textureSample(u_EnvTexture, u_EnvSampler, refr_r).r;
    let sample_g = textureSample(u_EnvTexture, u_EnvSampler, refr_g).g;
    let sample_b = textureSample(u_EnvTexture, u_EnvSampler, refr_b).b;

    let refracted_color = vec3<f32>(sample_r, sample_g, sample_b);

    // 2. Loi de Beer-Lambert (Absorption Volumétrique)
    let absorption = exp(-u_Glass.absorption_rgb.rgb * u_Glass.thickness);
    let final_refracted = refracted_color * absorption;

    // 3. Fresnel Schlick
    let cos_theta = max(dot(V, N), 0.0);
    let F = fresnel_schlick(cos_theta, u_Glass.ior_base);

    // 4. Réflexion Spéculaire
    let refl_dir = reflect(-V, N);
    let reflected_color = textureSample(u_EnvTexture, u_EnvSampler, refl_dir).rgb;

    let final_color = mix(final_refracted, reflected_color, F);

    return vec4<f32>(final_color, 1.0);
}
