// ── L'ADRESSE STABLE D'UN MICRO-SOMMET ──────────────────────────────────────────────────────
//
// Ce fichier n'est pas compile seul : `build.rs` le colle en tete des shaders qui ecrivent
// `//!inclure adresse`.
//
// ## Pourquoi il existe, et pourquoi UNE SEULE definition
//
// Le calcul d'adresse vit dans les DEUX sens et dans DEUX shaders : `surface.wgsl` va du rang
// vers (i,j) pour ecrire, `lecture.wgsl` va de (i,j) vers le rang pour lire. Les recopier serait
// exactement la faute que `commun.wgsl` a ete ecrit pour rendre impossible — deux textes a faire
// evoluer en parallele, donc tot ou tard deux adressages, donc une image plausible et fausse.
//
// La reference Rust est `render/surface.rs` : `rang_stable` / `depuis_rang_stable`, verifiees
// exhaustivement de k=0 a k=8 dans les deux sens, et MUTEES (4 mutations, 9 septembre 2026).
//
// ## Le principe
//
// Numeroter par ORDRE D'APPARITION dans le raffinement : les 3 coins, puis les 3 milieux
// d'aretes, puis les 9 points suivants. Le niveau se lit dans la valuation 2-adique des
// coordonnees, donc **un point ne change jamais d'adresse quand son triangle se subdivise**.
//
// ⚠ Le niveau 0 est un cas a part : aucune grille plus grossiere n'existe a soustraire. Sans lui
//   la formule rend 0 pour le coin (1,0) a k=0 — c'est le defaut qu'a trouve la premiere version.

fn micro_sommets(k: u32) -> u32 {
    let n = 1u << k;
    return (n + 1u) * (n + 2u) / 2u;
}

// Le niveau d'apparition : k - min(v2(i), v2(j), v2(n-i-j), k).
//
// ⭐ `firstTrailingBit(0u)` vaut 0xFFFFFFFF en WGSL, ce qui est exactement la valuation infinie
//    d'un zero. Les trois coins se traitent donc **sans aucun cas special** : le plafond a `k`
//    suffit. *Une constante de garde qui n'a jamais eu a exister.*
fn niveau_apparition(i: u32, j: u32, n: u32) -> u32 {
    let k = firstTrailingBit(n);
    let v = min(min(firstTrailingBit(i), firstTrailingBit(j)), firstTrailingBit(n - i - j));
    return k - min(v, k);
}

// Combien de points de la grille de pas `p` viennent avant (i,j), en ordre rangee.
// La rangee j' = m*p en contient N - m + 1 avec N = n/p : la somme est fermee, aucune boucle.
fn rang_grille(i: u32, j: u32, n: u32, p: u32) -> u32 {
    let grand_n = n / p;
    let m = (j + p - 1u) / p;
    var total = m * (grand_n + 1u) - m * (m - 1u) / 2u;
    if (j % p == 0u) {
        total = total + (i + p - 1u) / p;
    }
    return total;
}

// ⭐⭐ (i,j) → adresse. C'est le sens CHAUD : trois fois par pixel d'ecran.
fn rang_stable(i: u32, j: u32, n: u32) -> u32 {
    let k = firstTrailingBit(n);
    let a = niveau_apparition(i, j, n);
    let d = 1u << (k - a);
    if (a == 0u) {
        return rang_grille(i, j, n, d);
    }
    return micro_sommets(a - 1u) + rang_grille(i, j, n, d) - rang_grille(i, j, n, 2u * d);
}

// Combien de points du niveau `a` vivent dans les rangees avant j = m*d.
// Fermee, parce que la recherche binaire ci-dessous l'interroge : une version en boucle couterait
// jusqu'a 256 iterations par fil a k=8.
fn cumul_niveau(m: u32, a: u32, n: u32) -> u32 {
    let k = firstTrailingBit(n);
    let d = 1u << (k - a);
    let grand_n = n / d;
    let total = m * (grand_n + 1u) - m * (m - 1u) / 2u;
    if (a == 0u) {
        return total;
    }
    let n2 = n / (2u * d);
    let m2 = (m + 1u) / 2u;
    return total - (m2 * (n2 + 1u) - m2 * (m2 - 1u) / 2u);
}

// adresse → (i,j). Le sens RARE : une fois par micro-sommet ecrit.
//
// Deux recherches bornees par k ≤ 8, et **aucune racine carree** — donc plus de correction en
// virgule flottante a rattraper dans les deux sens, contrairement a l'ancien `depuis_rang`.
fn depuis_rang_stable(r: u32, n: u32) -> vec2<u32> {
    let k = firstTrailingBit(n);
    // 1. Le niveau : le plus petit `a` tel que micro_sommets(a) > r.
    var a = 0u;
    loop {
        if (a >= k || micro_sommets(a) > r) { break; }
        a = a + 1u;
    }
    var base = 0u;
    if (a > 0u) { base = micro_sommets(a - 1u); }
    let dans_niveau = r - base;

    // 2. La rangee : le plus grand `m` tel que cumul_niveau(m) <= dans_niveau.
    let d = 1u << (k - a);
    let grand_n = n / d;
    var bas = 0u;
    var haut = grand_n + 1u;
    loop {
        if (haut - bas <= 1u) { break; }
        let milieu = bas + (haut - bas) / 2u;
        if (cumul_niveau(milieu, a, n) <= dans_niveau) { bas = milieu; } else { haut = milieu; }
    }

    // 3. La colonne. Dans une rangee d'indice PAIR, un point sur deux appartient deja au niveau
    //    plus grossier ; dans une rangee impaire, ils sont tous du niveau `a`.
    let reste = dans_niveau - cumul_niveau(bas, a, n);
    var i = reste * d;
    if (a > 0u && (bas % 2u) == 0u) {
        i = (2u * reste + 1u) * d;
    }
    return vec2<u32>(i, bas * d);
}
