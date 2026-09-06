// LA MÉMOIRE DE SURFACE — le premier shader de calcul de ce moteur.
//
// Il écrit de la lumière SUR la surface, à une adresse barycentrique (T, u, v), sans jamais
// consulter l'écran. C'est l'étage 0 de la thèse dans sa plus petite forme vraie.
//
// ⚠ Ce qu'il calcule est volontairement le plus simple qui soit VRAI : un lambert direct, un seul
// soleil, aucune ombre, aucun indirect. Le sujet ici n'est pas la lumière, c'est de prouver
// qu'un shader peut écrire dans une mémoire persistante attachée à la géométrie.

struct Reglages {
    triangles: u32,
    // n = 2^k, le nombre de segments par arête.
    cote: u32,
    // Les micro-sommets d'un triangle : (n+1)(n+2)/2.
    par_triangle: u32,
    _pad: u32,
    // La direction dans laquelle le soleil VOYAGE — de la lumière vers la surface.
    // Le sens est écrit parce qu'une convention supposée au lieu d'être lue a déjà coûté au projet.
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

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let rang = gid.x;      // le micro-sommet dans son triangle
    let triangle = gid.y;  // le triangle du maillage
    if (triangle >= reglages.triangles || rang >= reglages.par_triangle) {
        return;
    }

    let n = reglages.cote;
    let ij = depuis_rang(rang, n);
    // La coordonnée barycentrique — c'est ELLE l'adresse, et elle ne coûte qu'une division.
    let u = f32(ij.x) / f32(n);
    let v = f32(ij.y) / f32(n);
    let w = 1.0 - u - v;

    let a = indices[triangle * 3u];
    let b = indices[triangle * 3u + 1u];
    let c = indices[triangle * 3u + 2u];

    // La position du micro-sommet ne se stocke PAS : elle se recalcule. C'est tout l'argument
    // mémoire de la voie barycentrique — 24 octets par entrée qu'OSC-GI paie et qu'on ne paie pas.
    let p = w * position(a) + u * position(b) + v * position(c);
    let nrm = normalize(w * normale(a) + u * normale(b) + v * normale(c));

    // Le lambert d'un soleil unique. `soleil.xyz` va DE la lumière VERS la surface, donc l'énergie
    // reçue par une face vaut le cosinus entre sa normale et la direction opposée.
    let lambert = max(dot(nrm, -normalize(reglages.soleil.xyz)), 0.0);

    // ⚠ Une modulation qui varie dans l'espace, pour que le banc puisse distinguer une adresse
    // JUSTE d'une adresse qui écrirait la bonne valeur au mauvais endroit. Un lambert seul rendrait
    // deux micro-sommets de même normale indiscernables — et le test passerait sur une adresse
    // fausse. *Se demander ce que la garde mesure QUAND elle passe.*
    //
    // La teinte et la fréquence viennent de l'appelant : ce shader ne choisit aucune couleur.
    let modulation = reglages.signal.xyz * (0.5 + 0.5 * sin(p * reglages.signal.w));
    let lumiere = modulation * lambert;

    let base = (triangle * reglages.par_triangle + rang) * 2u;
    surface[base] = pack2x16float(vec2<f32>(lumiere.x, lumiere.y));
    surface[base + 1u] = pack2x16float(vec2<f32>(lumiere.z, 0.0));
}
