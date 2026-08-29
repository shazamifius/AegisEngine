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

// ── L'ECLAIRAGE ─────────────────────────────────────────────────────────────────────────────
// Lambert pour le diffus, GGX (Trowbridge-Reitz) pour le speculaire. C'est le modele que tout le
// monde emploie, et ce n'est pas un choix par defaut : il est ENERGETIQUEMENT COHERENT, donc une
// surface ne peut pas renvoyer plus de lumiere qu'elle n'en recoit. Sans ca, ajouter des lumieres
// finit toujours par delaver l'image -- exactement ce qu'on veut eviter quand la CLARTE est
// l'exigence des deux bouts, du vieux telephone a la 4090.

const PI: f32 = 3.14159265;

// Combien une surface renvoie a l'oblique. f0 = 0,04 est la valeur des dielectriques (bois,
// plastique, pierre) ; les metaux voudront la leur, quand il y aura des materiaux.
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Quelle proportion des micro-facettes regarde exactement dans la direction du reflet.
fn distribution_ggx(n_dot_h: f32, rugosite: f32) -> f32 {
    let a = rugosite * rugosite;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-7);
}

// Combien de micro-facettes s'ombrent entre elles. Schlick-GGX, variante directe.
fn geometrie_schlick(n_dot_v: f32, rugosite: f32) -> f32 {
    let r = rugosite + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / max(n_dot_v * (1.0 - k) + k, 1e-7);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(cadre.camera_et_compte.xyz - in.world_position);
    let n_dot_v = max(dot(N, V), 1e-4);

    // ⚠ Rugosite constante pour l'instant, et c'est ECRIT plutot que subi : il n'y a pas encore
    // de materiaux dans ce moteur. 0,55 donne un mat legerement satine, qui convient aux blocs
    // du jeu. Le jour ou les materiaux arrivent, cette ligne est le seul endroit a changer.
    let rugosite = 0.55;
    let f0 = vec3<f32>(0.04);

    var total = vec3<f32>(0.0);
    let combien = i32(cadre.camera_et_compte.w);

    // La boucle ne parcourt QUE les lumieres allumees : une scene a trois lumieres ne paie pas
    // pour les seize emplacements. C'est aussi pourquoi le cout reste previsible.
    for (var i = 0; i < combien; i = i + 1) {
        let lum = cadre.lumieres[i];
        let genre = lum.position_type.w;

        var L: vec3<f32>;
        var attenuation = 1.0;

        if (genre < 0.5) {
            // Directionnelle : `direction_cone.xyz` pointe VERS la lumiere (le soleil est loin,
            // sa direction ne depend pas d'ou l'on est) et rien ne s'attenue avec la distance.
            L = normalize(lum.direction_cone.xyz);
        } else {
            let vers = lum.position_type.xyz - in.world_position;
            let distance = max(length(vers), 1e-4);
            L = vers / distance;
            // Loi du carre inverse : la seule attenuation physiquement juste. Pas de rayon de
            // coupure arbitraire ici -- une lumiere lointaine s'eteint d'elle-meme.
            attenuation = 1.0 / (distance * distance);

            if (genre > 1.5) {
                // Projecteur : un cone, avec un bord adouci pour ne pas trancher net.
                let axe = normalize(lum.direction_cone.xyz);
                let cos_angle = dot(-L, axe);
                let cos_bord = lum.direction_cone.w;
                attenuation = attenuation * smoothstep(cos_bord, mix(cos_bord, 1.0, 0.15), cos_angle);
            }
        }

        let n_dot_l = max(dot(N, L), 0.0);
        if (n_dot_l <= 0.0) {
            continue;
        }

        let H = normalize(V + L);
        let radiance = lum.couleur_intensite.rgb * lum.couleur_intensite.w * attenuation;

        let D = distribution_ggx(max(dot(N, H), 0.0), rugosite);
        let G = geometrie_schlick(n_dot_v, rugosite) * geometrie_schlick(n_dot_l, rugosite);
        let F = fresnel_schlick(max(dot(H, V), 0.0), f0);

        let speculaire = (D * G) * F / max(4.0 * n_dot_v * n_dot_l, 1e-7);
        // Ce qui part en reflet ne peut pas repartir en diffus : c'est la conservation d'energie.
        let part_diffuse = (vec3<f32>(1.0) - F);

        total = total + (part_diffuse * in.color.rgb / PI + speculaire) * radiance * n_dot_l;
    }

    // Une ambiante plate, en attendant la vraie lumiere indirecte (etape 4 du plan : les cascades
    // de radiance). ⚠ Elle est un PIS-ALLER, pas un choix esthetique : c'est elle qui aplatit les
    // creux et donne l'impression de decor en carton. Le jour ou l'indirect existe, elle disparait.
    //
    // ⚠⚠ Elle a ete BAISSEE de 0,40 a 0,17 en meme temps que le tone mapping est arrive, et les
    // deux vont ensemble : l'ancienne valeur avait ete reglee pour un pipeline qui ECRETAIT a 1,0.
    // Elle saturait donc en permanence, ce qui masquait le manque de contraste au lieu de le
    // corriger. Garder l'ancienne valeur sous une courbe qui ne sature plus donnait une image
    // delavee -- constate, puis corrige.
    let ambiante = vec3<f32>(0.15, 0.17, 0.20) * in.color.rgb;

    // Reinhard ETENDU, avec un point blanc. Le Reinhard simple (x / (x+1)) compresse aussi les
    // tons moyens : tout le monde se retrouve vers 0,5, et l'image perd le contraste -- l'inverse
    // exact de la clarte recherchee. Avec un point blanc a 2,0, ce qui est sous 1,0 reste presque
    // lineaire et seules les hautes lumieres sont ramenees. Rien n'ecrete jamais, donc ajouter une
    // lampe ne peut pas produire d'aplat blanc.
    let point_blanc = 2.0;
    let eclaire = ambiante + total;
    let expose = eclaire * (vec3<f32>(1.0) + eclaire / (point_blanc * point_blanc))
               / (vec3<f32>(1.0) + eclaire);
    let gamma_corrected = pow(expose, vec3<f32>(1.0 / 2.2));

    // `params.w` a 1.0 = couleur PLATE : celle qui a ete demandee, sans lampe et sans gamma.
    // C'est ce qu'exige une interface : un element de HUD ne vit pas dans la scene, il n'a donc
    // aucune raison de s'assombrir selon l'angle d'une lumiere, ni de changer si on la deplace.
    let plat = clamp(in.params.w, 0.0, 1.0);
    return vec4<f32>(mix(gamma_corrected, in.color.rgb, plat), 1.0);
}
