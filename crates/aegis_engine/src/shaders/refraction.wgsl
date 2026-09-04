// ── LA RÉFRACTION — où le rayon ressort vraiment ────────────────────────────────────────────
//
// Le premier shader du moteur qui fasse de la physique de la MATIÈRE. Jusqu'ici le moteur savait
// simuler une caméra qui regarde (halo, courbe de tonalité, intégration sur le photosite) et rien
// de ce qu'elle regarde : sur les dix-huit phénomènes de l'étalon, il en exprimait trois, tous du
// côté de l'objectif.
//
// ## Ce que ce shader calcule, et dans quel ordre
//
//   1. le rayon entre dans la matière  → Snell dévie sa direction ;
//   2. **où ressort-il ?** — la question difficile, et c'est la méthode de Newton qui répond ;
//   3. le rayon ressort               → Snell le dévie une seconde fois ;
//   4. ce qui a survécu du fond       → Beer-Lambert sur la longueur RÉELLEMENT traversée.
//
// ## ⭐⭐ POURQUOI NEWTON, ET POURQUOI SA DÉRIVÉE EST GRATUITE
//
// Une fois dévié à l'entrée, le rayon ne va plus vers le pixel qu'on est en train de calculer : il
// part de travers. Il faut donc trouver **où il perce la face arrière**, alors qu'on ne connaît
// cette face qu'à travers une carte indexée par l'écran. C'est chercher la racine de
//
//     g(s) = profondeur de la surface vue en projetant (départ + s·direction)
//          − s
//
// et Newton exige la dérivée de `g`. **C'est là que la géométrie fait un cadeau : le gradient de
// cette fonction EST la normale de la surface** (JCGT vol. 15 n° 1, 2026). La dérivée ne s'estime
// donc pas par différences finies — elle **se lit** dans la carte, à côté de la profondeur, sans
// une seule lecture supplémentaire. Le pas de Newton se réécrit alors comme l'intersection du
// rayon avec le **plan tangent** au point courant.
//
// Convergence quadratique, **une lecture de carte par tour** : le nombre d'erreurs est divisé
// par lui-même à chaque pas.
//
// ## ⚠⚠ LA LECTURE FINALE N'EST PAS UN DÉTAIL
//
// Sans elle, la normale rendue est celle d'AVANT le dernier pas : Newton corrige la position sans
// que la normale suive — or c'est la normale, pas la position, qui entre dans Snell à la sortie.
// *Mesuré sur le banc : « zéro pas » et « un pas » donnaient exactement le même angle, au millième
// près. Le mécanisme avait l'air de tourner et ne servait à rien.*
//
// ## ⭐ LA GÉOMÉTRIE ENTRE PAR DEUX CARTES, ET PAR ELLES SEULES
//
// Ce shader ne connaît **aucune forme**. Il lit deux images — une normale et une distance par
// pixel — et tout ce qu'il calcule en découle. Que ces cartes viennent d'une intersection exacte,
// d'un rastériseur logiciel ou d'une passe GPU sur un maillage venu de Blender, **pas une ligne
// d'ici ne change**.
//
// *Il a d'abord calculé sa sphère lui-même, le 2 septembre 2026 au matin, et c'était délibéré :
// une carte exacte ISOLE l'erreur.* On a ainsi mesuré la physique seule (**1,789°**), puis le prix
// de la discrétisation en pixels (**2,132°** à 256², et la décroissance vers le calcul direct est
// prouvée : 3,168° à 128², 1,917° à 512²). **Deux chiffres séparés valent mieux qu'un chiffre
// global** : quand la rastérisation arrivera, tout écart de plus lui sera imputable, parce que le
// reste aura été mesuré seul.
//
// ## ⚠ CE QUE CE SHADER NE FAIT PAS ENCORE
//
// Il suppose **une seule couche de matière** par pixel — une face avant, une face arrière. Un
// objet creux, ou deux objets de verre l'un derrière l'autre, demanderaient plus de deux cartes.
// *C'est une limite du modèle, pas un défaut d'implémentation, et elle se lève par une liste
// chaînée par pixel — un chantier à part entière.*
//
// ## ⚠ AUCUNE COULEUR N'EST ÉCRITE ICI, et un test le garde
//
// Le fond d'essai est en **niveaux de gris** ; l'absorption `sigma` arrive par canal depuis
// l'appelant. *Le moteur fournit ce qui est VRAI, le jeu fournit ce qui est BEAU* — et sur une
// mesure, un fond achromatique a un second mérite : aucune teinte ne peut masquer un canal faux.

// ── LES DEUX CARTES ──────────────────────────────────────────────────────────────────────────
//
// `xyz` = la normale de la surface dans le monde, `w` = la distance depuis l'œil.
// **`w <= 0` veut dire « aucune matière sur ce pixel »**, et c'est la seule façon de le dire.
//
// ⭐ Elles sont lues par `textureLoad`, donc **sans échantillonneur et sans interpolation**. Ce
// n'est pas une économie : interpoler deux normales de part et d'autre d'une silhouette
// fabriquerait une normale qui n'existe sur aucune surface — et cette normale-là entrerait ensuite
// dans Snell, où elle dévierait un rayon vers nulle part. *Le lisse est un défaut ici.*
@group(0) @binding(0) var carte_avant: texture_2d<f32>;
@group(0) @binding(1) var carte_arriere: texture_2d<f32>;

// ── LE VOLUME DE MATIÈRE ─────────────────────────────────────────────────────────────────────
//
// ⭐⭐ **La géométrie entre par deux cartes plates ; la matière entre par ce volume, et par lui
// seul.** Le shader ne connaît donc aucune sucette, aucun jade, aucun brouillard : il échantillonne
// ce qu'on lui donne. *Écrire les bulles ici aurait gravé une décision d'artiste dans le moteur —
// exactement la faute du voxel du 31 août 2026, qu'un raccourci qui fonctionne rend si tentante.*
//
// `rgb` = le facteur qui module l'absorption de référence, par canal. **1 partout = milieu
// homogène**, et le résultat est alors celui d'avant, à la précision de la somme près.
//
// `a` = le **terme de source** : ce que la matière RENVOIE vers l'œil par unité de longueur — une
// bulle qui réfléchit l'ambiante, une brume qui renvoie le soleil. *Sans lui, une matière ne peut
// que retirer de la lumière, et une bulle d'air dans du sucre serait un trou noir.*
//
// ⚠ **Il est SCALAIRE, donc la lumière rendue est neutre**, là où le banc processeur
// (`epaisseur.rs`) porte une source par canal. Ce n'est pas un oubli : il n'y avait qu'un canal
// libre, et une bulle d'air réfléchit effectivement sans colorer. *Une source teintée — une matière
// qui rougeoie — demandera un second volume, et c'est un chantier, pas un paramètre.*
//
// ⚠ Celui-là est échantillonné avec **interpolation**, contrairement aux deux cartes : entre deux
// texels de matière, la valeur intermédiaire décrit un milieu qui existe. *Entre deux normales de
// part et d'autre d'une silhouette, non.*
@group(0) @binding(2) var volume_matiere: texture_3d<f32>;
@group(0) @binding(3) var echantillonneur: sampler;

// ── LA CARTE D'ENVIRONNEMENT ─────────────────────────────────────────────────────────────────
//
// ⭐⭐ **Ce que la lumière fait AVANT d'arriver sur la matière**, en projection équirectangulaire :
// l'azimut en `u`, l'élévation en `v`.
//
// # Pourquoi une carte, et pas une fonction écrite ici
//
// Le premier jet calculait l'environnement dans ce shader : une grande fenêtre, un sol sombre,
// quelques nombres. **Le test qui garde la frontière moteur/jeu l'a refusé dans l'heure** — et il
// avait raison sur le fond, même si sa sonde visait une direction en croyant voir une couleur :
// *où se trouve la fenêtre et combien elle éclaire sont des décisions de SCÈNE.* Les graver ici,
// c'est mettre un habitacle de voiture dans un moteur qui vise tous les mondes.
//
// **La géométrie entre par deux cartes, la matière par un volume, la lumière incidente par
// celle-ci.** Le shader ne sait donc rien de ce qu'il reflète — et l'appelant reste libre de la
// calculer, de la peindre ou de la photographier.
//
// ⚠ Son échantillonneur est SÉPARÉ de celui du volume : l'azimut **boucle** (`REPEAT`), là où un
// volume doit se figer à son bord. *Un seul échantillonneur partagé aurait posé une couture
// visible derrière la caméra — le genre de défaut qu'on attribue ensuite à la géométrie.*
@group(0) @binding(4) var carte_environnement: texture_2d<f32>;
@group(0) @binding(5) var echantillonneur_environnement: sampler;

struct Constantes {
    // La caméra décrite par sa base, et non par une matrice : la même description sert à
    // PROJETER (monde → pixel, ce dont Newton a besoin) et à DÉ-PROJETER (pixel → direction).
    // Deux matrices auraient coûté 128 octets à elles seules — tout le budget garanti.
    position: vec4<f32>,   // xyz = l'œil
    droite: vec4<f32>,     // xyz = axe droit,  w = tangente du demi-champ horizontal
    haut: vec4<f32>,       // xyz = axe haut,   w = tangente du demi-champ vertical
    avant: vec4<f32>,      // xyz = axe de visée
    matiere: vec4<f32>,    // xyz = sigma de REFERENCE par canal, w = rapport des indices n1/n2
    reglages: vec4<f32>,   // xy = taille en pixels, z = mode, w = nombre de tours de Newton
    volume_min: vec4<f32>,    // xyz = coin minimal de la boite du volume, w = nombre de pas
    volume_taille: vec4<f32>, // xyz = taille de la boite du volume ; w non lu
};
var<push_constant> k: Constantes;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) sommet: u32) -> Sortie {
    var out: Sortie;
    // Un seul triangle plus grand que l'écran : aucune diagonale au milieu de l'image, donc
    // aucune couture où les deux moitiés pourraient être calculées différemment.
    out.uv = vec2<f32>(f32((sommet << 1u) & 2u), f32(sommet & 2u));
    let ndc = out.uv * 2.0 - vec2<f32>(1.0);
    // ⚠ Y inversé à l'avance : naga compile avec ADJUST_COORDINATE_SPACE et retourne le Y de la
    // position de clip en sortie de vertex. L'oublier retourne l'image entière — ce qui saute aux
    // yeux sur une scène et ne se voit PAS sur une mesure.
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    return out;
}

// ── LA CAMÉRA, DANS LES DEUX SENS ────────────────────────────────────────────────────────────

/// Pixel → direction du rayon, normalisée.
fn direction_du_pixel(pixel: vec2<f32>) -> vec3<f32> {
    let sx = (pixel.x / k.reglages.x) * 2.0 - 1.0;
    // Y descend dans une image et monte dans le monde. Le retournement est ici, une seule fois.
    let sy = 1.0 - (pixel.y / k.reglages.y) * 2.0;
    return normalize(
        k.avant.xyz
            + k.droite.xyz * (sx * k.droite.w)
            + k.haut.xyz * (sy * k.haut.w)
    );
}

/// Monde → pixel. `w` vaut 0 quand le point est derrière l'œil : la projection n'a alors aucun
/// sens géométrique, et il faut le dire plutôt que de rendre un pixel plausible.
fn projeter(p: vec3<f32>) -> vec3<f32> {
    let v = p - k.position.xyz;
    let z = dot(v, k.avant.xyz);
    if (z <= 1e-6) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    let sx = (dot(v, k.droite.xyz) / z) / k.droite.w;
    let sy = (dot(v, k.haut.xyz) / z) / k.haut.w;
    return vec3<f32>(
        (sx * 0.5 + 0.5) * k.reglages.x,
        (0.5 - sy * 0.5) * k.reglages.y,
        1.0
    );
}

// ── LA CARTE ─────────────────────────────────────────────────────────────────────────────────

/// Lit une carte au pixel donné, en refusant proprement ce qui tombe hors de l'image.
///
/// ⚠ **Newton fait sortir du cadre**, et c'est normal : le rayon dévié vise un point que l'écran
/// ne montre pas forcément. Sans cette garde, `textureLoad` hors bornes rend un résultat que la
/// spécification ne définit pas — donc un résultat qui change d'une carte graphique à l'autre.
fn lire(carte: texture_2d<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let taille = vec2<i32>(textureDimensions(carte));
    let p = vec2<i32>(floor(pixel));
    if (p.x < 0 || p.y < 0 || p.x >= taille.x || p.y >= taille.y) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    return textureLoad(carte, p, 0);
}

/// La face ARRIÈRE vue depuis un pixel : `xyz` = la normale, `w` = la distance. `w <= 0` = rien.
///
/// ⭐ **C'est ici que la géométrie entre dans le shader**, et c'est la seule porte. Que la carte
/// vienne d'une intersection exacte, d'une rastérisation logicielle ou d'une passe GPU, tout ce
/// qui suit — Newton, la projection, Snell, Beer-Lambert — ne change pas d'une ligne.
fn arriere(pixel: vec2<f32>) -> vec4<f32> {
    return lire(carte_arriere, pixel);
}

// ── LA PHYSIQUE ──────────────────────────────────────────────────────────────────────────────

/// Snell, sous forme vectorielle.
///
/// ⭐ **La réflexion totale interne n'est pas un cas à coder** : c'est ce qui reste quand la racine
/// n'existe pas. L'angle critique — 41,81° pour un indice de 1,5 — tombe de l'équation, et n'est
/// écrit nulle part. Un `if angle > 41.81` aurait été une constante arbitraire à justifier pour
/// toujours, et fausse dès le premier matériau différent.
fn refracter(incident: vec3<f32>, normale: vec3<f32>, eta: f32) -> vec4<f32> {
    let cos_i = -dot(normale, incident);
    let reste = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    if (reste < 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0); // pas de racine → réflexion totale
    }
    return vec4<f32>(eta * incident + (eta * cos_i - sqrt(reste)) * normale, 1.0);
}

/// ⭐⭐ **LES ÉQUATIONS DE FRESNEL, EXACTES** — quelle fraction de la lumière rebondit au lieu
/// d'entrer.
///
/// # Pourquoi les vraies, et pas l'approximation de Schlick
///
/// Schlick coûte une puissance cinquième et se trompe de moins d'un pour cent : c'est le choix de
/// toute l'industrie, et il serait défendable. **Les équations exactes coûtent une racine carrée de
/// plus**, et elles donnent trois choses que l'approximation ne donne pas :
///
/// 1. **Elles se démontrent au lieu de se régler.** *Une constante qui se dérive vaut mieux qu'une
///    constante qui s'ajuste* — et ici il n'y a plus aucune constante du tout.
/// 2. **Elles séparent les deux polarisations** (`Rs` et `Rp`). La lumière d'un habitacle de
///    voiture est fortement polarisée : ce que ce code moyenne aujourd'hui, il pourra le pondérer
///    demain, **sans rien réécrire**. *L'étalon note la polarisation comme « rien, et personne ne
///    le fait ».*
/// 3. **Elles donnent l'angle de Brewster gratuitement** — `Rp` s'annule vers 56,3° pour n = 1,5,
///    et ce n'est écrit nulle part : ça tombe de l'équation, exactement comme l'angle critique.
///
/// # La vérité contre laquelle ce code se mesure
///
/// À incidence normale, `R = ((n₁−n₂)/(n₁+n₂))²`. Pour n = 1,5 cela vaut **exactement 4,0 %** —
/// une valeur physique connue, indépendante de ce moteur. *C'est ce que le test vérifie sur le
/// pixel central, et c'est pour ça que la mesure prouve quelque chose : elle ne compare pas deux
/// de mes calculs entre eux.*
fn fresnel(cos_incidence: f32, eta: f32) -> f32 {
    let cos_i = clamp(abs(cos_incidence), 0.0, 1.0);
    let sin_t_carre = eta * eta * (1.0 - cos_i * cos_i);
    // Pas de racine → réflexion totale. **Elle n'est pas un cas à coder** : c'est ce qui reste
    // quand la lumière ne peut plus sortir, et l'angle critique tombe de l'équation.
    if (sin_t_carre >= 1.0) {
        return 1.0;
    }
    let cos_t = sqrt(1.0 - sin_t_carre);
    // Perpendiculaire au plan d'incidence (« s »), puis parallèle (« p »).
    let rs = (eta * cos_i - cos_t) / (eta * cos_i + cos_t);
    let rp = (eta * cos_t - cos_i) / (eta * cos_t + cos_i);
    // Lumière non polarisée : les deux moitiés à parts égales. *C'est ICI, et nulle part ailleurs,
    // qu'une pondération de polarisation viendra se placer.*
    return 0.5 * (rs * rs + rp * rp);
}

/// L'environnement, lu dans sa carte — **aucune scène n'est décrite ici**.
///
/// La projection est équirectangulaire : l'azimut sur toute la largeur, l'élévation sur la
/// hauteur. *C'est la projection la moins chère à échantillonner et la seule qui ne demande ni
/// six faces ni indirection ; sa distorsion aux pôles est réelle, et elle ne coûte rien tant que
/// la source lumineuse n'y est pas.*
fn environnement(direction: vec3<f32>) -> vec3<f32> {
    let d = normalize(direction);
    // ⚠ `atan2(x, z)` et non `atan2(z, x)` : le premier tourne autour de l'axe vertical, ce qui
    // est ce qu'un azimut veut dire. *Écrit à l'envers, la carte pivote autour du mauvais axe et
    // l'erreur ressemble à un décalage de scène.*
    let azimut = atan2(d.x, d.z);
    let elevation = asin(clamp(d.y, -1.0, 1.0));
    let u = azimut / 6.2831853 + 0.5;
    // v descend dans une image et l'élévation monte dans le monde : le retournement est ici.
    let v = 0.5 - elevation / 3.1415927;
    return textureSampleLevel(
        carte_environnement,
        echantillonneur_environnement,
        vec2<f32>(u, v),
        0.0
    ).rgb;
}

/// Un fond d'essai **achromatique**, contrasté et directionnel : sans structure visible, une
/// réfraction juste et une réfraction fausse donnent la même image plate.
/// ⚠ L'azimut se prend en `atan2(x, z)`, pas `atan2(z, x)` : autour de l'axe de visée, le premier
/// varie comme `x` et couvre bien le champ, le second est coincé près de π/2 et ne bouge presque
/// pas. *Écrit à l'envers en premier jet, l'image sortait en dégradé lisse — juste, et incapable
/// de montrer la moindre déviation. Un fond sans structure rend une réfraction fausse et une
/// réfraction juste identiques à l'œil.*
fn fond(direction: vec3<f32>) -> vec3<f32> {
    // ⭐ `volume_taille.w` était le dernier champ libre des constantes — le plafond de 128 octets
    // étant atteint, c'est lui qui porte le choix. *Le damier reste le DÉFAUT : toutes les mesures
    // écrites avant ce jour continuent de mesurer exactement ce qu'elles mesuraient.*
    if (k.volume_taille.w > 0.5) {
        return environnement(direction);
    }
    let u = atan2(direction.x, direction.z) * 20.0;
    let v = asin(clamp(direction.y, -1.0, 1.0)) * 20.0;
    let damier = sin(u) * sin(v);
    let clair = 0.5 + 0.45 * sign(damier) * pow(abs(damier), 0.25);
    return vec3<f32>(clair);
}

// ── LE CŒUR : CHERCHER LA SORTIE ─────────────────────────────────────────────────────────────

/// Renvoie `xyz` = la normale de la face arrière au point de sortie, `w` = la distance parcourue
/// DANS la matière. `w < 0` quand la recherche échoue.
fn chercher_la_sortie(depart: vec3<f32>, direction: vec3<f32>, estimation: f32) -> vec4<f32> {
    var s = estimation;
    let tours = i32(k.reglages.w);

    for (var tour = 0; tour < tours; tour = tour + 1) {
        let ecran = projeter(depart + direction * s);
        if (ecran.z == 0.0) { break; }

        let surface = arriere(ecran.xy);
        if (surface.w <= 0.0) { break; }

        // ⚠ Le point se reconstruit depuis le CENTRE du pixel lu, pas depuis la position
        // fractionnaire visée. La carte ne connaît que des pixels entiers : reconstruire depuis
        // autre chose que le centre du pixel réellement lu mélangerait deux endroits différents.
        let centre = floor(ecran.xy) + vec2<f32>(0.5);
        let point_surface = k.position.xyz + direction_du_pixel(centre) * surface.w;

        // ⭐ La normale EST le gradient de la fonction dont on cherche la racine : la dérivée que
        // Newton réclame se LIT dans la carte, elle ne s'estime pas. Aucune lecture de plus.
        let normale = surface.xyz;
        let denominateur = dot(direction, normale);
        if (abs(denominateur) < 1e-5) { break; }

        // Le pas de Newton, réécrit : l'intersection du rayon avec le plan tangent.
        let suivant = dot(point_surface - depart, normale) / denominateur;
        if (suivant <= 0.0) { break; }

        // Converger, c'est cesser de bouger À L'ÉCRAN — la seule grandeur que la carte connaisse.
        let apres = projeter(depart + direction * suivant);
        s = suivant;
        if (apres.z != 0.0 && distance(apres.xy, ecran.xy) < 0.05) { break; }
    }

    // ⚠⚠ LA LECTURE FINALE. Sans elle, la normale rendue est celle d'avant le dernier pas — et
    // c'est la normale qui entre dans Snell, pas la position.
    let ecran = projeter(depart + direction * s);
    if (ecran.z == 0.0) { return vec4<f32>(0.0, 0.0, 0.0, -1.0); }
    let surface = arriere(ecran.xy);
    if (surface.w <= 0.0) { return vec4<f32>(0.0, 0.0, 0.0, -1.0); }

    return vec4<f32>(surface.xyz, s);
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    let pixel = entree.position.xy;
    let regard = direction_du_pixel(pixel);
    let mode_direction = k.reglages.z > 0.5 && k.reglages.z < 1.5;
    // ⭐ Mode 2 : la réflectance de Fresnel, seule, en niveaux de gris. *Un moteur qui vise une
    // machine qu'il ne verra jamais doit pouvoir montrer ses grandeurs intermédiaires — sinon la
    // seule sonde est l'œil, et l'œil ne lit pas un pourcentage.*
    let mode_reflectance = k.reglages.z > 1.5;

    let avant = lire(carte_avant, pixel);
    let derriere = lire(carte_arriere, pixel);
    // Rien à cet endroit : on voit le fond directement. En mode direction, la direction du regard
    // est ce qui sort — un rayon qui ne traverse rien n'est pas dévié, et c'est la bonne réponse.
    if (avant.w <= 0.0 || derriere.w <= 0.0) {
        if (mode_direction) {
            return vec4<f32>(regard * 0.5 + vec3<f32>(0.5), 1.0);
        }
        if (mode_reflectance) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        return vec4<f32>(fond(regard), 1.0);
    }

    let entree_point = k.position.xyz + regard * avant.w;
    let normale_entree = avant.xyz;

    // ── 0. CE QUI NE RENTRE PAS ──
    //
    // ⭐⭐ **Jusqu'au 4 septembre 2026, ce shader transmettait CENT POUR CENT de la lumière.** Ce
    // n'était pas un reflet manquant : c'était **l'énergie qui ne se conservait pas**. Une bille de
    // verre sans reflet ne ressemble à rien de réel, et aucun réglage de couleur ne pouvait le
    // rattraper — *une somme de corrections justes ne franchit pas un mécanisme absent.*
    let part_reflechie = fresnel(dot(normale_entree, regard), k.matiere.w);
    if (mode_reflectance) {
        return vec4<f32>(vec3<f32>(part_reflechie), 1.0);
    }
    // Ce que le miroir montre : l'environnement dans la direction du rebond.
    let reflet = fond(reflect(regard, normale_entree)) * part_reflechie;

    // ── 1. Snell à l'entrée ──
    let dedans = refracter(regard, normale_entree, k.matiere.w);
    if (dedans.w < 0.5) {
        // Réflexion totale dès l'entrée : géométriquement impossible en venant du vide, mais on
        // ne suppose pas — on répond.
        if (mode_direction) {
            return vec4<f32>(regard * 0.5 + vec3<f32>(0.5), 1.0);
        }
        return vec4<f32>(reflet, 1.0);
    }

    // ── 2. Où ressort-il ? ──
    // L'estimation de départ est la corde vue de face : c'est ce que donnerait l'approximation
    // naïve « le rayon ne dévie pas », et Newton part de là.
    let sortie = chercher_la_sortie(entree_point, dedans.xyz, derriere.w - avant.w);
    if (sortie.w < 0.0) {
        if (mode_direction) {
            return vec4<f32>(dedans.xyz * 0.5 + vec3<f32>(0.5), 1.0);
        }
        return vec4<f32>(reflet + fond(dedans.xyz) * (1.0 - part_reflechie), 1.0);
    }
    let distance_traversee = sortie.w;

    // ── 3. Snell à la sortie ──
    // La normale pointe vers l'extérieur ; en sortant on la retourne, et le rapport d'indices
    // s'inverse. La réflexion totale, elle, arrive VRAIMENT ici : c'est ce qui allume les tranches
    // d'une plaque de verre.
    let dehors = refracter(dedans.xyz, -sortie.xyz, 1.0 / k.matiere.w);
    var direction_finale = dehors.xyz;
    if (dehors.w < 0.5) {
        // Réflexion totale interne : le rayon rebondit à l'intérieur au lieu de sortir.
        direction_finale = reflect(dedans.xyz, -sortie.xyz);
    }

    if (mode_direction) {
        return vec4<f32>(direction_finale * 0.5 + vec3<f32>(0.5), 1.0);
    }

    // ── 4. Ce qui a survécu — Beer-Lambert le long du trajet RÉEL ──
    //
    // ⭐⭐ **UNE SEULE FORMULE, ET LE MILIEU HOMOGÈNE EN EST UN CAS PARTICULIER EXACT.**
    //
    // Jusqu'au 4 septembre 2026, cette ligne était `exp(-sigma * distance)` : un `sigma` unique
    // pour tout le trajet, donc un verre teinté et rien d'autre. Une sucette de sucre bleu a un
    // feuillet de colorant mal mélangé et des bulles — **la matière change le long du rayon**, et
    // aucune formule fermée ne dit ça. Il faut marcher et accumuler l'épaisseur optique.
    //
    // *Et il n'y a PAS deux chemins de code.* Sur un volume neutre, chaque pas ajoute
    // `sigma * 1 * ds`, dont la somme vaut `sigma * distance` — la marche **redonne** l'ancienne
    // formule au lieu de la remplacer. Une branche `si le milieu est homogène` aurait été un
    // second chemin à tester pour toujours, et le premier à diverger.
    //
    // ⚠ L'échantillon se prend au **milieu** de chaque segment, jamais à son début : sur un
    // feuillet mince, prendre le bord fait manquer ou compter deux fois une couche entière selon
    // le pas — et l'erreur ne se voit pas, elle se contente de décaler la teinte.
    // ⭐⭐ ET LA MATIÈRE NE FAIT PAS QU'EN RETIRER : ELLE EN REND.
    //
    // C'est l'équation du transfert radiatif, dans sa forme la plus simple — absorption plus
    // émission, sans diffusion multiple :
    //
    //     L = fond · e^(−τ_total)  +  ∫ source(t) · e^(−τ(0→t)) dt
    //
    // *La lumière qu'un point renvoie doit REMONTER le chemin déjà parcouru pour atteindre l'œil,
    // donc elle est atténuée par ce qui se trouve entre lui et la surface d'entrée — pas par
    // l'épaisseur totale.* Confondre les deux donne une image plus sombre au centre qu'au bord,
    // ce qui ressemble à un défaut d'éclairage et n'en est pas un.
    var epaisseur_optique = vec3<f32>(0.0);
    var lumiere_rendue = vec3<f32>(0.0);
    let pas = max(1, i32(k.volume_min.w));
    let ds = distance_traversee / f32(pas);
    for (var i = 0; i < pas; i = i + 1) {
        let point = entree_point + dedans.xyz * ((f32(i) + 0.5) * ds);
        // Monde → volume. La boîte est décrite par son coin et sa taille : deux soustractions et
        // une division, pas de matrice.
        let uvw = (point - k.volume_min.xyz) / k.volume_taille.xyz;
        // ⚠ `textureSampleLevel` et non `textureSample` : dans une boucle, le niveau de détail
        // implicite se calcule à partir de dérivées d'écran qui n'ont aucun sens ici — et la
        // spécification l'interdit sous flux non uniforme.
        let echantillon = textureSampleLevel(volume_matiere, echantillonneur, uvw, 0.0);

        // ⚠ La MOITIÉ de l'épaisseur du segment courant, et c'est la même règle du point milieu
        // que pour l'échantillon : la lumière est rendue au centre du segment, donc elle ne
        // traverse que la moitié de celui-ci. *Atténuer par le segment entier, ou par rien,
        // décale la luminosité d'une façon qui se règle « à l'œil » ensuite — c'est ainsi que
        // naissent les constantes arbitraires.*
        let demie = k.matiere.xyz * echantillon.rgb * (ds * 0.5);
        lumiere_rendue = lumiere_rendue
            + echantillon.a * exp(-(epaisseur_optique + demie)) * ds;
        epaisseur_optique = epaisseur_optique + demie + demie;
    }
    let survie = exp(-epaisseur_optique);
    // ⚠ **La part réfléchie ne traverse rien** : elle n'est ni absorbée, ni colorée par le milieu.
    // C'est ce qui garde un reflet BLANC sur une bille bleue — et ce qui rend le reflet si
    // reconnaissable. *Le multiplier par l'absorption donnerait un reflet bleu, plausible et faux.*
    let transmis = (fond(direction_finale) * survie + lumiere_rendue) * (1.0 - part_reflechie);
    return vec4<f32>(reflet + transmis, 1.0);
}
