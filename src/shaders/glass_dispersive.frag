#version 450

// Données transmises par le Vertex Shader
layout(location = 0) in vec3 in_world_position;
layout(location = 1) in vec3 in_world_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_screen_uv;

// Push Constants
layout(push_constant) uniform PushConstants {
    mat4 mvp_matrix;
    mat4 model_matrix;
    mat4 normal_matrix;
    vec4 glass_tint; // RGB: Albédo / Absorption optique, A: Épaisseur (d)
    vec4 params;     // X: Rugosité (0.0=Cristallin, 1.0=Dépoli), Y: IOR (1.48)
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.25, 7.2);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = max(dot(N, V), 0.0);
    float ior = pc.params.y > 1.0 ? pc.params.y : 1.48;

    // ------------------------------------------------------------------------
    // 1. RÉFRACTION OPTIQUE SNELL-DESCARTES
    // ------------------------------------------------------------------------
    vec3 T = refract(-V, N, 1.0 / ior);
    float refract_len = length(T);

    vec2 delta_uv = vec2(0.0);
    if (refract_len > 0.001) {
        float thickness = pc.glass_tint.a > 0.0 ? pc.glass_tint.a : 0.25;
        delta_uv = T.xy * thickness * 0.22;
    } else {
        delta_uv = N.xy * 0.08;
    }

    vec2 uv_refracted = clamp(in_screen_uv + delta_uv, vec2(0.001), vec2(0.999));

    // ------------------------------------------------------------------------
    // 2. FLOU VOLUMÉTRIQUE Mipmap Hardware Vulkan (textureLod)
    // ------------------------------------------------------------------------
    float roughness = clamp(pc.params.x, 0.0, 1.0);
    float max_lod = 4.0;
    float lod = roughness * max_lod;

    vec3 fond_transmis = textureLod(sampler2D(transmission_texture, transmission_sampler), uv_refracted, lod).rgb;

    // ------------------------------------------------------------------------
    // 3. ABSORPTION OPTIQUE DE BEER-LAMBERT & OCCLUSION AUX BORDS (TIR DARK RIM)
    // ------------------------------------------------------------------------
    float thickness = pc.glass_tint.a > 0.0 ? pc.glass_tint.a : 0.25;
    float dist = thickness / max(NdotV, 0.12);

    // Absorption optique cyan/bleu profonde
    vec3 sigma_a = (vec3(1.0) - pc.glass_tint.rgb) * 3.5;
    vec3 transmittance = exp(-sigma_a * dist);

    // Occlusion de réflexion interne aux bords rasants (Rim Darkening)
    float edge_darkening = pow(smoothstep(0.0, 0.38, NdotV), 0.65);
    vec3 border_tint = mix(vec3(0.05, 0.18, 0.35), vec3(1.0), edge_darkening);

    vec3 couleur_transmise = fond_transmis * transmittance * border_tint;

    // ------------------------------------------------------------------------
    // 4. RÉFLEXION FRESNEL-SCHLICK & ÉCLATS SPÉCULAIRES NETS (ENERGY CONSERVED)
    // ------------------------------------------------------------------------
    float F0 = 0.04; // Verre IOR 1.48
    float fresnel = F0 + (1.0 - F0) * pow(1.0 - NdotV, 5.0);

    // Lumière studio principale
    vec3 L1 = normalize(vec3(3.5, 6.0, 4.5));
    vec3 H1 = normalize(V + L1);
    float NdotH1 = max(dot(N, H1), 0.0);
    float spec_power = mix(256.0, 16.0, roughness);
    float spec1 = pow(NdotH1, spec_power) * (1.0 - roughness * 0.7) * 2.2;

    // Lumière de contre-jour rim
    vec3 L2 = normalize(vec3(-4.0, 3.5, -2.0));
    vec3 H2 = normalize(V + L2);
    float spec2 = pow(max(dot(N, H2), 0.0), 32.0) * 0.4;

    vec3 sky_reflection = mix(vec3(0.75, 0.88, 0.98), vec3(1.0), spec1);
    vec3 couleur_réfléchie = sky_reflection * fresnel;

    // Mélange Fresnel avec conservation d'énergie
    vec3 rgb_final = mix(couleur_transmise, couleur_réfléchie, fresnel)
                   + vec3(0.9, 0.95, 1.0) * (spec1 + spec2);

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 1.0);
}
