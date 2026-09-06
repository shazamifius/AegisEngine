// L'ÉCRAN LIT LA SURFACE — l'étage 0, geste 2.
//
// > *Tout vit sur la SURFACE. L'écran ne fait que la lire.*
//
// Ce shader existe pour rendre cette phrase mesurable. Il porte DEUX chemins vers la même image :
//
//   mode 0 — le pixel CALCULE sa lumière, comme tous les moteurs le font ;
//   mode 1 — le pixel LIT la lumière écrite sur la surface par `surface.wgsl`.
//
// ⭐ **Tout le reste est rigoureusement identique** : même géométrie, même caméra, même cadrage,
// même courbe. *Toute différence entre les deux images est donc imputable à la mémoire de surface,
// et à rien d'autre.* C'est la seule façon de faire dire quelque chose à une comparaison d'images —
// le corpus porte le cas inverse, une garde qui comparait deux rendus cadrés différemment et qui
// n'est passée qu'une fois, par hasard.
//
// ## ⚠ Pourquoi ce shader ne prend AUCUN sommet en entrée
//
// Il lit la géométrie depuis les mêmes tampons de stockage que la passe de calcul, et déduit son
// triangle de l'index du sommet : `triangle = index / 3`, `coin = index % 3`. C'est ce qui permet
// au fragment de connaître son adresse $(T, u, v)$ **sans `primitive_index`**, dont la disponibilité
// n'est pas acquise sur la cible mobile du projet.
//
// *Conséquence assumée : la géométrie est dessinée NON INDEXÉE, donc chaque sommet partagé est lu
// plusieurs fois. C'est un coût de bande passante réel, et il n'est pas mesuré ici.*

struct Reglages {
    view_proj: mat4x4<f32>,
    triangles: u32,
    cote: u32,
    par_triangle: u32,
    // 0 = le pixel calcule · 1 = le pixel lit la surface.
    mode: u32,
    // La direction dans laquelle le soleil VOYAGE — de la lumière vers la surface.
    soleil: vec4<f32>,
    // ⚠⚠ `xyz` = la teinte du signal, `w` = sa fréquence. Elles viennent du DEHORS : le moteur
    // fournit ce qui est VRAI, le jeu ce qui est BEAU, et un test du projet échoue si un shader du
    // moteur choisit une couleur. Il doit être RIGOUREUSEMENT le même signal qu'en mode 1.
    signal: vec4<f32>,
}

var<push_constant> reglages: Reglages;

@group(0) @binding(0) var<storage, read> sommets: array<f32>;
@group(0) @binding(1) var<storage, read> indices: array<u32>;
@group(0) @binding(2) var<storage, read> surface: array<u32>;
// ⭐ Le plan d'allocation : deux u32 par triangle (sa base, ses segments par arête). L'adresse se
// LIT, elle ne se calcule plus — c'est ce que coûte une subdivision qui varie d'un triangle à l'autre.
@group(0) @binding(3) var<storage, read> plan: array<u32>;

const FLOTTANTS_PAR_SOMMET: u32 = 14u;

fn position(s: u32) -> vec3<f32> {
    let b = s * FLOTTANTS_PAR_SOMMET;
    return vec3<f32>(sommets[b], sommets[b + 1u], sommets[b + 2u]);
}

fn normale(s: u32) -> vec3<f32> {
    let b = s * FLOTTANTS_PAR_SOMMET + 3u;
    return vec3<f32>(sommets[b], sommets[b + 1u], sommets[b + 2u]);
}

fn debut_rangee(j: u32, n: u32) -> u32 {
    return j * (2u * n + 3u - j) / 2u;
}

// Lit une entrée de la mémoire de surface : deux u32, quatre demi-flottants, dont trois utilisés.
fn entree(base_tri: u32, i: u32, j: u32, n: u32) -> vec3<f32> {
    let base = (base_tri + debut_rangee(j, n) + i) * 2u;
    let rv = unpack2x16float(surface[base]);
    let b = unpack2x16float(surface[base + 1u]);
    return vec3<f32>(rv.x, rv.y, b.x);
}

// ⭐⭐ LA LECTURE — l'opération que toute la thèse promet, et la voici en entier.
//
// Le fragment arrive avec une coordonnée barycentrique CONTINUE ; la mémoire ne porte des valeurs
// qu'aux micro-sommets. Il faut donc trouver le micro-triangle qui contient (u, v) et interpoler
// entre ses trois coins.
//
// La grille barycentrique alterne deux orientations. À l'intérieur de la cellule (i, j) :
//   · si fu + fv ≤ 1, le micro-triangle pointe « vers le haut » — coins (i,j), (i+1,j), (i,j+1) ;
//   · sinon il pointe « vers le bas » — coins (i+1,j), (i,j+1), (i+1,j+1).
//
// *Les poids du second cas se vérifient à la main : (1−fv) + (1−fu) + (fu+fv−1) = 1.*
//
// ⭐ **Il n'y a aucune recherche ici.** Pas de table de hachage, pas de plus-proche-voisin, pas de
// parcours : deux planchers et une comparaison. C'est ce que l'adresse barycentrique achète, et
// c'est l'argument que le budget ne mesurait pas.
fn lire_surface(triangle: u32, u: f32, v: f32) -> vec3<f32> {
    let base_tri = plan[triangle * 2u];
    let n = plan[triangle * 2u + 1u];
    let fn_ = f32(n);
    // ⚠ Le clamp n'est pas une prudence : à u + v == 1 exactement — sur l'arête opposée au premier
    // coin — le plancher rendrait i + j == n et le coin (i+1, j+1) sortirait du triangle, où il
    // empiéterait sur la rangée suivante. *Un débordement qui rend une couleur plausible.*
    let uu = clamp(u, 0.0, 1.0);
    let vv = clamp(v, 0.0, 1.0 - uu);

    let U = uu * fn_;
    let V = vv * fn_;
    var i = u32(floor(U));
    var j = u32(floor(V));
    if (i + j >= n) {
        // Sur l'arête, on recule d'une cellule pour rester dans le domaine.
        if (i > 0u) { i = i - 1u; } else if (j > 0u) { j = j - 1u; }
    }
    let fu = U - f32(i);
    let fv = V - f32(j);

    // ⚠⚠ `i + j + 2u <= n` N'EST PAS UNE PRÉCAUTION : sans lui, la branche du micro-triangle
    // inversé atteint le coin (i+1, j+1), qui n'existe pas sur la dernière cellule. En théorie le
    // cas ne peut pas arriver — sur la diagonale extérieure, i+j = n−1 force fu+fv ≤ 1 — mais en
    // virgule flottante un fu+fv à 1,0000001 y bascule.
    //
    // *Ici il n'y aurait aucune erreur : la lecture irait chercher dans la plage du triangle
    // SUIVANT et rendrait une image plausible. C'est un test Rust sur la même arithmétique qui l'a
    // trouvé, parce que là-bas un index hors bornes panique au lieu de mentir.*
    if (fu + fv <= 1.0 || i + j + 2u > n) {
        return (1.0 - fu - fv) * entree(base_tri, i, j, n)
             + fu * entree(base_tri, i + 1u, j, n)
             + fv * entree(base_tri, i, j + 1u, n);
    }
    return (1.0 - fv) * entree(base_tri, i + 1u, j, n)
         + (1.0 - fu) * entree(base_tri, i, j + 1u, n)
         + (fu + fv - 1.0) * entree(base_tri, i + 1u, j + 1u, n);
}

struct Sortie {
    @builtin(position) position: vec4<f32>,
    // Les deux coordonnées barycentriques — c'est l'ADRESSE, interpolée par le rastériseur lui-même.
    @location(0) uv: vec2<f32>,
    // ⚠ `flat` : un index de triangle ne s'interpole pas. Sans ce mot, les pixels de l'intérieur
    // liraient un triangle qui n'existe pas, et l'image resterait presque juste.
    @location(1) @interpolate(flat) triangle: u32,
    // ⚠ Interpolée donc NON normalisée à l'arrivée : on renormalise au fragment.
    @location(2) normale_monde: vec3<f32>,
    @location(3) position_monde: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> Sortie {
    let triangle = vi / 3u;
    let coin = vi % 3u;
    let s = indices[triangle * 3u + coin];
    let p = position(s);

    var out: Sortie;
    out.position = reglages.view_proj * vec4<f32>(p, 1.0);
    // Le coin 0 est l'origine barycentrique : (u,v) = (0,0), puis (1,0) et (0,1).
    out.uv = vec2<f32>(f32(coin == 1u), f32(coin == 2u));
    out.triangle = triangle;
    out.normale_monde = normale(s);
    out.position_monde = p;
    return out;
}

@fragment
fn fs_main(in: Sortie) -> @location(0) vec4<f32> {
    var lumiere: vec3<f32>;

    if (reglages.mode == 1u) {
        // L'écran ne fait que LIRE.
        lumiere = lire_surface(in.triangle, in.uv.x, in.uv.y);
    } else {
        // Le chemin ordinaire : le pixel refait le calcul. ⚠ Il doit être RIGOUREUSEMENT celui de
        // `surface.wgsl` — la moindre divergence ferait accuser la mémoire de surface d'un écart
        // qui viendrait d'ici.
        let nrm = normalize(in.normale_monde);
        let lambert = max(dot(nrm, -normalize(reglages.soleil.xyz)), 0.0);
        let p = in.position_monde;
        let modulation = reglages.signal.xyz * (0.5 + 0.5 * sin(p * reglages.signal.w));
        lumiere = modulation * lambert;
    }

    // ⚠⚠ On écrit du LINÉAIRE, et c'est la surface qui encode.
    //
    // La première version élevait ici à 1/2,2 — et une garde du moteur l'a fait tomber :
    // *« la surface de présentation est en _SRGB, elle encode DÉJÀ la gamma ; ces shaders en
    // encodent une seconde et délavent toute l'image »*. Le banc rend donc vers un format `_SRGB`,
    // exactement comme le chemin réel du moteur, et la courbe s'applique **une seule fois**.
    //
    // *Le problème n'a pas été contourné, il a disparu : la conversion est faite par le matériel,
    // à l'écriture, et il n'y a plus rien à tenir cohérent entre deux endroits.*
    return vec4<f32>(clamp(lumiere, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
