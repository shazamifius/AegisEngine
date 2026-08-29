// ── LE SHADER D'ECLAIRAGE ───────────────────────────────────────────────────────────────────
//
// Ce qu'il declare lui-meme tient en deux lignes : le reste (les constantes poussees, le cadre,
// le ciel, la courbe de tonalite) vit dans des preambules PARTAGES avec la passe d'ombre et le
// fond. C'est ce qui garantit que les trois parlent du meme monde.

//!inclure commun
//!inclure objet

// La carte d'ombre : ce que la lumiere a vu de plus proche dans chaque direction.
@group(0) @binding(1) var carte_ombre: texture_depth_2d;
// ⚠ Un echantillonneur de COMPARAISON. Un echantillonneur ordinaire moyennerait des PROFONDEURS,
// ce qui n'a aucun sens : la moyenne de "3 m" et "10 m" ne dit rien sur l'ombre. Celui-ci teste
// d'abord, puis moyenne les RESULTATS (0 ou 1) -- c'est ce qui rend un bord adouci possible.
@group(0) @binding(2) var comparateur: sampler_comparison;

// Rend 1 en pleine lumiere, 0 dans l'ombre.
fn part_de_lumiere(position_monde: vec3<f32>, n_dot_l: f32) -> f32 {
    let dans_la_lumiere = cadre.light_view_proj * vec4<f32>(position_monde, 1.0);
    let ndc = dans_la_lumiere.xyz / dans_la_lumiere.w;

    // ⚠⚠ L'AXE Y EST RETOURNE, ET C'EST LE PIEGE LE PLUS COUTEUX DE CE MOTEUR.
    // Les shaders d'Aegis sont compiles par naga avec ADJUST_COORDINATE_SPACE, qui inverse le Y
    // de la position de clip EN SORTIE DE VERTEX. La carte d'ombre est donc ecrite avec un Y
    // oppose a celui que rend ce calcul-ci, fait dans le fragment ou aucun ajustement n'a lieu.
    // Oublier cette ligne donne des ombres qui existent mais tombent au mauvais endroit -- un
    // defaut qu'on met des heures a attribuer, parce que tout le reste a l'air juste.
    // (Meme famille que l'inversion de Y ajoutee A TORT dans la projection camera, le 29 aout.)
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);

    // Hors de la carte : pleinement eclaire. Assombrir tout ce que la lumiere ne couvre pas serait
    // bien plus visible qu'une ombre manquante.
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
        return 1.0;
    }

    // Le decalage principal est applique en DESSINANT la carte (depth_bias), pas ici : il s'adapte
    // a l'inclinaison de la surface, la ou l'erreur d'arrondi est la plus grande. Ce petit reste
    // couvre les surfaces presque paralleles aux rayons, que la pente ne rattrape pas.
    let marge = mix(0.0015, 0.0002, n_dot_l);
    let profondeur = ndc.z - marge;

    // Quatre echantillons en croix : un compromis entre un bord dur et le cout de la lecture.
    let texel = 1.0 / f32(textureDimensions(carte_ombre).x);
    var somme = 0.0;
    somme = somme + textureSampleCompare(carte_ombre, comparateur, uv + vec2<f32>(-texel, -texel), profondeur);
    somme = somme + textureSampleCompare(carte_ombre, comparateur, uv + vec2<f32>( texel, -texel), profondeur);
    somme = somme + textureSampleCompare(carte_ombre, comparateur, uv + vec2<f32>(-texel,  texel), profondeur);
    somme = somme + textureSampleCompare(carte_ombre, comparateur, uv + vec2<f32>( texel,  texel), profondeur);
    return somme * 0.25;
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    // ⚠ Les coordonnees de texture ne sont PAS transmises : le fragment ne les lit pas, et le
    // moteur n'a aucune texture. Elles etaient interpolees a chaque pixel de chaque image pour
    // rien. *Jamais d'excedent* — elles reviendront le jour ou quelque chose les lira.
    @location(3) params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let modele = matrice_modele(in);
    let world_pos = modele * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;

    let normal_matrix = mat3x3<f32>(modele[0].xyz, modele[1].xyz, modele[2].xyz);
    out.world_normal = normalize(normal_matrix * in.normal);

    out.color = in.teinte;
    out.params = in.params;

    // Le shader compose lui-meme la vue-projection, sauf en couleur plate ou la matrice de
    // l'objet est deja une matrice d'ecran (le HUD, le lobby). `select(faux, vrai, condition)`.
    let en_espace_ecran = in.params.w == 1.0;
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

    // ⚠⚠ LA COULEUR DEMANDEE EST EN sRGB, LE CALCUL SE FAIT EN LINEAIRE.
    // Un humain qui ecrit `0.32, 0.82, 0.36` pour de l'herbe pense a ce qu'un selecteur de
    // couleur lui montre — c'est du sRGB. La traiter comme lineaire fausse tout le calcul
    // d'energie, silencieusement, et delave le resultat. La conversion se fait ICI, une seule
    // fois, et plus rien n'encode de gamma ensuite (la surface de presentation s'en charge).
    let albedo = vers_lineaire(in.color.rgb);

    // ⚠ Ces deux valeurs ETAIENT ecrites en dur ici, et c'etait la faute : une rugosite et une
    // reflectance sont des decisions de MATIERE, donc du jeu. Elles viennent maintenant du cadre.
    let rugosite = cadre.matiere.x;
    let f0 = vec3<f32>(cadre.matiere.y);

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

        // ⚠ SEULE LA PREMIERE LUMIERE porte une ombre : il n'y a qu'une carte. Les suivantes
        // eclairent sans ombrer, ce qui est ecrit plutot que subi -- une lampe qui traverse les
        // murs se remarque, et il vaut mieux savoir que c'est une limite connue qu'un defaut.
        var part_eclairee = 1.0;
        if (i == 0) {
            part_eclairee = part_de_lumiere(in.world_position, n_dot_l);
        }
        if (part_eclairee <= 0.0) {
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

        total = total + (part_diffuse * albedo / PI + speculaire) * radiance * n_dot_l * part_eclairee;
    }

    // Ce qui eclaire encore une surface a l'ombre : le ciel au-dessus, le sol en dessous.
    // La formule vit dans `commun.wgsl` -- c'est la MEME que celle qui peint le fond, appelee ici
    // avec la normale au lieu de la direction du regard. Rien ne les separe, donc rien ne peut
    // les faire diverger.
    let ambiante = ambiance_hemispherique(N) * albedo;

    // Exposition et courbe de tonalite : `commun.wgsl` a nouveau, et pour la meme raison. Un
    // second chemin vers le pixel serait une seconde courbe, donc deux mondes qui ne se
    // repondent plus. Le resultat est LINEAIRE : la surface de presentation encode la gamma.
    let eclaire = presenter(ambiante + total);

    // `params.w` a 1.0 = couleur PLATE : celle qui a ete demandee, sans lampe et sans courbe.
    // C'est ce qu'exige une interface : un element de HUD ne vit pas dans la scene, il n'a donc
    // aucune raison de s'assombrir selon l'angle d'une lumiere, ni de changer si on la deplace.
    //
    // ⚠ Elle sort en `albedo`, c'est-a-dire en LINEAIRE — et c'est ce qui la fait enfin arriver
    // a l'ecran telle qu'elle a ete demandee. Ecrite brute, elle traversait quand meme l'encodage
    // sRGB de la surface : le fond des panneaux du HUD, demande a (13, 15, 20), sortait mesure a
    // (63, 69, 80). Presque cinq fois trop clair, sous un commentaire qui promettait le contraire.
    let plat = clamp(in.params.w, 0.0, 1.0);
    return vec4<f32>(mix(eclaire, albedo, plat), 1.0);
}
