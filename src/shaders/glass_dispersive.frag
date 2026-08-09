#version 450

// Input Vertex Data
layout(location = 0) in vec3 in_world_position;
layout(location = 1) in vec3 in_world_normal;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec2 in_screen_uv;

// Push Constants Mathématiques Pure (Vulkan 1.4 Native)
layout(push_constant) uniform PushConstants {
    mat4 mvp_matrix;
    mat4 model_matrix;
    mat4 normal_matrix;
    vec4 glass_tint; // RGB: Couleur du verre, A: Translucidité
    vec4 params;     // X: Rugosité, Y: IOR (1.48)
} pc;

layout(set = 0, binding = 0) uniform texture2D transmission_texture;
layout(set = 0, binding = 1) uniform sampler transmission_sampler;

layout(location = 0) out vec4 out_color;

const float PI = 3.14159265359;

float fresnel_schlick(float cos_theta, float f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 3.5);
}

void main() {
    vec3 N = normalize(in_world_normal);
    vec3 camera_pos = vec3(0.0, 0.25, 5.8);
    vec3 V = normalize(camera_pos - in_world_position);

    float NdotV = abs(dot(N, V));

    float ior = pc.params.y > 0.0 ? pc.params.y : 1.48; // Indice de réfraction du verre = 1.48

    // ------------------------------------------------------------------------
    // 1. REFRACTION OPTIQUE SNELL-DESCARTES (Déformation des contours sous le verre)
    // ------------------------------------------------------------------------
    float eta = 1.0 / ior;
    float k = 1.0 - eta * eta * (1.0 - NdotV * NdotV);
    vec3 refract_dir = -V;
    if (k >= 0.0) {
        refract_dir = -V * eta + N * (eta * NdotV - sqrt(k));
    }

    // Déplacement optique Snell accentué par la normale biseautée
    vec2 refraction_offset = refract_dir.xy * 0.28 + N.xy * 0.18;
    vec2 uv_sample = clamp(in_screen_uv + refraction_offset, vec2(0.001), vec2(0.999));
    vec3 fond_transmis = texture(sampler2D(transmission_texture, transmission_sampler), uv_sample).rgb;

    // ------------------------------------------------------------------------
    // 2. TRANSMISSION CRISTALLINE (Conservation des couleurs vives sous-jacentes)
    // ------------------------------------------------------------------------
    vec3 couleur_transmise = mix(fond_transmis, pc.glass_tint.rgb, 0.22);

    // ------------------------------------------------------------------------
    // 3. FIL LUMINEUX CONTINU FRESNEL SUR 360° (Tranche Cristalline hyper brillante)
    // ------------------------------------------------------------------------
    float fresnel = fresnel_schlick(NdotV, 0.08);

    vec3 lumière_clef = normalize(vec3(3.2, 4.0, 3.0));
    vec3 H_clef = normalize(V + lumière_clef);

    // Reflet spéculaire de surface très poli
    float spec_surface = pow(max(dot(N, H_clef), 0.0), 128.0) * 0.75 * fresnel;
    vec3 reflet_surface = vec3(1.0, 1.0, 1.0) * spec_surface;

    // FIL LUMINEUX CONTINU TRES FIN ET HYPER BRILLANT (360° Perimeter Crystal Rim)
    float fil_lumineux = pow(1.0 - NdotV, 6.0) * 2.8;
    vec3 rim_cristallin = vec3(0.85, 0.94, 1.00) * fil_lumineux;

    // Assemblage final pure matière verre
    vec3 rgb_final = couleur_transmise + reflet_surface + rim_cristallin;

    out_color = vec4(clamp(rgb_final, vec3(0.0), vec3(1.0)), 0.65);
}
