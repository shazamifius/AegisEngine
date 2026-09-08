// LA MÉMOIRE DE SURFACE — le premier shader de calcul de ce moteur.
//
// Il écrit de la lumière SUR la surface, à une adresse barycentrique (T, u, v), sans jamais
// consulter l'écran. C'est l'étage 0 de la thèse dans sa plus petite forme vraie.
//
// ⚠ Ce qu'il calcule est volontairement le plus simple qui soit VRAI : un lambert direct, un seul
// soleil, aucune ombre, aucun indirect. Le sujet ici n'est pas la lumière, c'est de prouver
// qu'un shader peut écrire dans une mémoire persistante attachée à la géométrie.

struct Reglages {
    // ⭐ Le nombre de triangles que CETTE passe doit recalculer — la longueur de `a_refaire`, pas le
    // nombre de triangles du maillage.
    //
    // *Il n'y a pas deux modes « tout » et « partiel » : il y a une LISTE, pleine à la première
    // image et courte ensuite. Un mécanisme unique se teste ; deux modes font deux moteurs, dont un
    // seul est exercé.*
    a_refaire: u32,
    // n = 2^k, le nombre de segments par arête.
    cote: u32,
    // Les micro-sommets d'un triangle : (n+1)(n+2)/2.
    par_triangle: u32,
    _pad: u32,
    // `xyz` : la direction dans laquelle le soleil VOYAGE — de la lumière vers la surface. Le sens
    // est écrit parce qu'une convention supposée au lieu d'être lue a déjà coûté au projet.
    //
    // ⭐ `w = 1` FIGE le lambert à 1, ne laissant que la modulation spatiale. Ce n'est pas un mode
    // de rendu, c'est un INSTRUMENT : la valeur ne dépend alors plus que de la POSITION, identique
    // des deux côtés d'une arête partagée. *Sans ça, le saut mesuré à une arête est dominé par la
    // différence de NORMALES — une arête dure — et l'instrument sature avant de voir la couture
    // qu'on cherche.*
    soleil: vec4<f32>,
    // ⚠⚠ `xyz` = la teinte du signal, `w` = sa fréquence spatiale. **Elles viennent du DEHORS, et
    // c'est une frontière, pas une commodité.** Le moteur fournit ce qui est VRAI (de la lumière
    // sur une surface) ; choisir une couleur est le rôle du jeu, et un test du projet échoue si un
    // shader du moteur en contient une. *La première version de ce fichier portait
    // `vec3(1.0, 0.85, 0.7)` en dur — la garde ne l'a pas vue, parce qu'elle ne regardait pas les
    // shaders de calcul. Elle les regarde depuis.*
    signal: vec4<f32>,
}

var<push_constant> reglages: Reglages;

// ⚠ Les sommets sont lus comme un tableau de flottants BRUTS, pas comme une structure.
// Le `Vertex` du moteur fait 14 flottants (position 3, normale 3, tangente 4, uv0 2, uv1 2) et
// WGSL alignerait une structure équivalente sur 16 octets, décalant tout en silence.
// *Lire des flottants par index est moins joli et ne peut pas mentir.*
@group(0) @binding(0) var<storage, read> sommets: array<f32>;
@group(0) @binding(1) var<storage, read> indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> surface: array<u32>;
// ⭐ LE PLAN D'ALLOCATION — deux u32 par triangle : sa base, et son nombre de segments par arête.
//
// C'est ce qui remplace `triangle * par_triangle`. La subdivision cesse d'être uniforme, donc
// l'adresse cesse d'être une multiplication : elle devient une LECTURE. *Huit octets par triangle,
// et c'est le prix exact de la non-uniformité — il faut le dire, pas le cacher dans une formule.*
@group(0) @binding(3) var<storage, read> plan: array<u32>;
// ⭐⭐ LA LISTE DE TRAVAIL — quels triangles cette passe doit recalculer.
//
// C'est ce qui rend la mémoire de surface PERSISTANTE : ce qui n'est pas dans cette liste garde la
// valeur écrite à une image précédente. *Sans elle, « une mémoire et sa dérivée » reste une
// intention — on réécrirait un état complet à chaque image, ce qui est la définition d'une texture
// recalculée, pas d'un état qui évolue.*
//
// ⚠ Elle contient des INDICES de triangles, pas des drapeaux : un fil ne doit jamais être lancé pour
// un triangle qu'on ne veut pas toucher. *Un test « ce triangle est-il à refaire ? » à l'intérieur
// du shader lancerait tous les fils pour n'en garder que 3 % — ça n'économiserait rien.*
@group(0) @binding(4) var<storage, read> a_refaire: array<u32>;

const FLOTTANTS_PAR_SOMMET: u32 = 14u;

fn position(s: u32) -> vec3<f32> {
    let b = s * FLOTTANTS_PAR_SOMMET;
    return vec3<f32>(sommets[b], sommets[b + 1u], sommets[b + 2u]);
}

fn normale(s: u32) -> vec3<f32> {
    let b = s * FLOTTANTS_PAR_SOMMET + 3u;
    return vec3<f32>(sommets[b], sommets[b + 1u], sommets[b + 2u]);
}

// Le début de la rangée j : la somme des rangées précédentes, qui comptent (n - m + 1) sommets.
fn debut_rangee(j: u32, n: u32) -> u32 {
    return j * (2u * n + 3u - j) / 2u;
}

// L'inverse du rang : retrouver (i, j) depuis le rang du fil.
//
// On cherche le plus grand j tel que debut_rangee(j) <= r, c'est-à-dire la plus petite racine de
// j² - (2n+3)j + 2r = 0. La racine carrée est flottante, donc j peut tomber d'une unité à côté :
// les deux corrections ferment le cas dans les deux sens.
// *La même arithmétique existe en Rust dans `render/surface.rs`, et un test y vérifie la bijection
// sur tous les rangs de k = 0 à 6.*
fn depuis_rang(r: u32, n: u32) -> vec2<u32> {
    let b = f32(2u * n + 3u);
    let disc = max(b * b - 8.0 * f32(r), 0.0);
    var j = u32((b - sqrt(disc)) * 0.5);
    loop {
        if (j + 1u > n || debut_rangee(j + 1u, n) > r) { break; }
        j = j + 1u;
    }
    loop {
        if (j == 0u || debut_rangee(j, n) <= r) { break; }
        j = j - 1u;
    }
    return vec2<u32>(r - debut_rangee(j, n), j);
}

// La lumière en un point barycentrique (u, v) du triangle.
//
// Extraite pour pouvoir être évaluée AILLEURS que sur le micro-sommet du fil — c'est exactement ce
// qu'exige le raccord des arêtes ci-dessous.
fn lumiere_en(a: u32, b: u32, c: u32, u: f32, v: f32) -> vec3<f32> {
    let w = 1.0 - u - v;
    // La position ne se stocke PAS : elle se recalcule. C'est tout l'argument mémoire de la voie
    // barycentrique — 24 octets par entrée qu'OSC-GI paie et qu'on ne paie pas.
    let p = w * position(a) + u * position(b) + v * position(c);
    let nrm = normalize(w * normale(a) + u * normale(b) + v * normale(c));
    let lambert = select(
        max(dot(nrm, -normalize(reglages.soleil.xyz)), 0.0),
        1.0,
        reglages.soleil.w > 0.5
    );
    let modulation = reglages.signal.xyz * (0.5 + 0.5 * sin(p * reglages.signal.w));
    return modulation * lambert;
}

// La coordonnée barycentrique du point situé à la fraction `f` de l'arête `e`.
//
//   arête 0 = (v0 → v1) : (u, v) = (f, 0)
//   arête 1 = (v1 → v2) : (u, v) = (1 − f, f)
//   arête 2 = (v2 → v0) : (u, v) = (0, 1 − f)
fn bary_arete(e: u32, f: f32) -> vec2<f32> {
    if (e == 0u) { return vec2<f32>(f, 0.0); }
    if (e == 1u) { return vec2<f32>(1.0 - f, f); }
    return vec2<f32>(0.0, 1.0 - f);
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let rang = gid.x;  // le micro-sommet dans son triangle
    if (gid.y >= reglages.a_refaire) {
        return;
    }
    // ⭐ Le triangle se LIT dans la liste de travail : `gid.y` numérote le travail, pas le maillage.
    let triangle = a_refaire[gid.y];

    // L'adresse se LIT : ce triangle a sa propre base et sa propre subdivision.
    let base_tri = plan[triangle * 2u];
    let mot = plan[triangle * 2u + 1u];
    let n = mot & 0xffffu;
    // Les niveaux EFFECTIFS des trois arêtes, en exposants, empaquetés par quatre bits.
    let ke = vec3<u32>((mot >> 16u) & 0xfu, (mot >> 20u) & 0xfu, (mot >> 24u) & 0xfu);
    // ⚠ Le dispatch est dimensionné sur le PLUS SUBDIVISÉ des triangles ; les fils en trop d'un
    // triangle grossier sortent ici. *Sans ce test, ils écriraient dans la plage du triangle
    // suivant — un débordement silencieux qui rendrait une image presque juste.*
    let sommets_ici = (n + 1u) * (n + 2u) / 2u;
    if (rang >= sommets_ici) {
        return;
    }
    let ij = depuis_rang(rang, n);
    // La coordonnée barycentrique — c'est ELLE l'adresse, et elle ne coûte qu'une division.
    let u = f32(ij.x) / f32(n);
    let v = f32(ij.y) / f32(n);
    let w = 1.0 - u - v;

    let a = indices[triangle * 3u];
    let b = indices[triangle * 3u + 1u];
    let c = indices[triangle * 3u + 2u];

    // ⭐⭐⭐ LE RACCORD DES ARÊTES — la règle des micro-maillages, et ce qui rend l'étage 0 étanche.
    //
    // Un micro-sommet posé SUR une arête dont le niveau effectif est plus grossier que celui du
    // triangle ne porte pas sa propre valeur : il est interpolé entre les deux micro-sommets alignés
    // sur le pas grossier qui l'encadrent.
    //
    // *Les deux triangles qui partagent l'arête ont le MÊME niveau effectif — c'est le minimum des
    // deux — donc ils interpolent entre les mêmes points, avec les mêmes poids. L'égalité n'est pas
    // approchée : elle est exacte.*
    //
    // ⚠ Seul le BORD est décimé. Un grand triangle finement subdivisé garde toute sa densité
    // intérieure ; il ne cède que sur la ligne où il doit s'accorder avec son voisin.
    //
    // ⚠ Un COIN appartient à deux arêtes — mais il tombe toujours sur le pas grossier des deux
    // (t vaut 0 ou n, et le pas divise n), donc les deux branches rendent la même chose et l'ordre
    // des tests n'a aucune importance. *Le vérifier vaut mieux que de l'espérer : c'est le genre de
    // cas où deux règles correctes se contredisent à leur intersection.*
    var arete = -1;
    var t = 0u;
    if (ij.y == 0u) { arete = 0; t = ij.x; }
    else if (ij.x + ij.y == n) { arete = 1; t = ij.y; }
    else if (ij.x == 0u) { arete = 2; t = n - ij.y; }

    var lumiere: vec3<f32>;
    if (arete >= 0 && (1u << ke[u32(arete)]) < n) {
        let e = u32(arete);
        let pas = n / (1u << ke[e]);
        let t0 = (t / pas) * pas;
        if (t0 == t) {
            lumiere = lumiere_en(a, b, c, u, v);
        } else {
            let poids = f32(t - t0) / f32(pas);
            let d0 = bary_arete(e, f32(t0) / f32(n));
            let d1 = bary_arete(e, f32(t0 + pas) / f32(n));
            lumiere = mix(
                lumiere_en(a, b, c, d0.x, d0.y),
                lumiere_en(a, b, c, d1.x, d1.y),
                poids
            );
        }
    } else {
        lumiere = lumiere_en(a, b, c, u, v);
    }

    let base = (base_tri + rang) * 2u;
    surface[base] = pack2x16float(vec2<f32>(lumiere.x, lumiere.y));
    surface[base + 1u] = pack2x16float(vec2<f32>(lumiere.z, 0.0));
}
