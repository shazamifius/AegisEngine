// ── CE QUI CHANGE A CHAQUE OBJET ────────────────────────────────────────────────────────────
// 96 octets, et le chiffre compte : Vulkan ne garantit que 128 octets de constantes poussees.
// Ce shader en poussait 160 (une matrice vue-projection redondante par objet) et n'aurait donc
// tres probablement pas pu creer son pipeline sur un GPU mobile — la machine de reference du
// projet est un Meta Quest 2.
struct PushConstants {
    // ⚠ En couleur plate cette matrice est deja une matrice d'ECRAN : aucune camera ne lui est
    // appliquee. C'est ce qui tient le HUD en place pendant que la camera bouge.
    model_matrix: mat4x4<f32>,
    color_tint: vec4<f32>,
    params: vec4<f32>,
};

var<push_constant> pc: PushConstants;

// ── CE QUI EST VRAI POUR TOUTE L'IMAGE ──────────────────────────────────────────────────────
// La vue-projection est la meme pour tous les objets : l'envoyer par objet, c'etait ~2000 fois
// les memes 64 octets par image. Les lumieres arrivent par le meme chemin.
struct Lumiere {
    position_type: vec4<f32>,     // xyz = position monde, w = type (0 dir, 1 point, 2 projecteur)
    couleur_intensite: vec4<f32>, // rgb = couleur, w = intensite
    direction_cone: vec4<f32>,    // xyz = direction, w = cosinus du demi-angle du cone
};

struct Cadre {
    view_proj: mat4x4<f32>,
    camera_et_compte: vec4<f32>,  // xyz = position camera, w = nombre de lumieres allumees
    lumieres: array<Lumiere, 16>,
};

@group(0) @binding(0) var<uniform> cadre: Cadre;

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

    // Le shader compose lui-meme la vue-projection, sauf en couleur plate ou `model_matrix` est
    // deja une matrice d'ecran (le HUD, le lobby). `select(faux, vrai, condition)`.
    let en_espace_ecran = pc.params.w == 1.0;
    out.clip_position = select(cadre.view_proj * world_pos, world_pos, en_espace_ecran);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let light_dir = normalize(vec3<f32>(0.4, 0.9, 0.7));

    let NdotL = max(dot(N, light_dir), 0.0);
    let diffuse = vec3<f32>(1.0, 0.96, 0.88) * NdotL * 0.65;
    let ambient = vec3<f32>(0.35, 0.40, 0.45);

    let eclaire = in.color.rgb * (ambient + diffuse);
    let gamma_corrected = pow(eclaire, vec3<f32>(1.0 / 2.2));

    // `params.w` a 1.0 = couleur PLATE : celle qui a ete demandee, sans lampe et sans gamma.
    // C'est ce qu'exige une interface : un element de HUD ne vit pas dans la scene, il n'a donc
    // aucune raison de s'assombrir selon l'angle d'une lumiere, ni de changer si on la deplace.
    //
    // Ce champ voyageait deja jusqu'ici sans que personne ne le lise : les 42 appels du jeu y
    // mettent tous 0.0, donc leur rendu ne bouge pas d'un pixel.
    let plat = clamp(in.params.w, 0.0, 1.0);
    return vec4<f32>(mix(gamma_corrected, in.color.rgb, plat), 1.0);
}
