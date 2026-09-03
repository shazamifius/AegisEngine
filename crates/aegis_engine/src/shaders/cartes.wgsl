// ── LA GÉOMÉTRIE ENTRE DANS LES CARTES ──────────────────────────────────────────────────────
//
// Ce shader est le chaînon qui manquait entre un maillage et `refraction.wgsl`. Il ne calcule
// aucune lumière, aucune couleur, aucune matière : il répond à **une seule question par pixel**,
// pour une face donnée de l'objet —
//
//     « à quelle distance de l'œil est cette surface, et dans quelle direction regarde-t-elle ? »
//
// `xyz` = la normale de la surface dans le monde · `w` = la distance depuis l'œil.
//
// ## ⭐ Pourquoi c'est TOUT ce qu'il faut, et pourquoi ça ne changera pas
//
// `refraction.wgsl` ne connaît aucune forme. Il lit deux images de cette nature et fait le reste —
// Snell aux deux interfaces, Newton en espace écran, Beer-Lambert sur la longueur traversée. **Que
// ces images viennent d'une intersection analytique, d'un rastériseur logiciel ou de cette passe,
// rien de ce qui suit ne change d'une ligne.** C'est ce qui a été mesuré séparément le 2 septembre
// 2026 : la physique seule vaut 1,789° d'écart à la vérité, et le passage par des cartes exactes
// en 256² ajoute 0,34°, dont on a démontré que c'est le prix de la discrétisation en pixels.
//
// *Tout écart supplémentaire mesuré maintenant est donc imputable à la RASTÉRISATION, et à elle
// seule — parce que le reste a été chiffré avant qu'elle existe.*
//
// ## ⚠ La normale écrite est la SORTANTE, dans les deux cartes
//
// Y compris dans celle de sortie, où elle pointe donc à l'opposé du rayon. Ce n'est pas un oubli :
// `refracter` la retourne lui-même au moment de sortir de la matière, et c'est le shader de
// réfraction qui porte cette convention — pas celui-ci. *Deux endroits qui retournent la même
// normale, c'est une normale qui n'est retournée nulle part.*
//
// ## ⚠⚠ Ce shader n'a le droit d'écrire AUCUNE couleur, et un test le garde
//
// Il en va de la frontière du 29 août : le moteur fournit ce qui est VRAI, le jeu fournit ce qui
// est BEAU. Une normale et une distance sont des grandeurs géométriques ; elles n'ont pas de goût.

//!inclure commun
//!inclure objet

struct Sortie {
    @builtin(position) position: vec4<f32>,
    // ⚠ Interpolées, donc **non normalisées** à l'arrivée au fragment : une interpolation linéaire
    // entre deux vecteurs unitaires ne l'est plus. On renormalise au fragment, jamais ici.
    @location(0) normale_monde: vec3<f32>,
    @location(1) position_monde: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> Sortie {
    let modele = matrice_modele(in);
    let monde = modele * vec4<f32>(in.position, 1.0);

    var out: Sortie;
    out.position = cadre.view_proj * monde;
    out.position_monde = monde.xyz;
    // ⚠ `w = 0` : une normale est une DIRECTION, elle ne subit pas la translation de l'objet.
    // L'écrire à 1 donnerait une normale qui bouge quand l'objet se déplace — et l'image resterait
    // plausible, ce qui est le pire cas.
    //
    // ⚠ Cette ligne suppose une échelle UNIFORME. Sous une échelle non uniforme, la normale juste
    // demande la transposée de l'inverse. C'est écrit ici plutôt que corrigé d'avance : aucun objet
    // du moteur n'a d'échelle non uniforme aujourd'hui, et une matrice de plus par instance se paie
    // en bande passante — la ressource rare. *Le jour où un objet est étiré, ceci est le fautif.*
    out.normale_monde = (modele * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    let oeil = cadre.camera_et_compte.xyz;

    // ⭐ La distance EUCLIDIENNE à l'œil, pas la profondeur le long de l'axe de visée.
    //
    // C'est la grandeur que `refraction.wgsl` attend, parce que c'est celle qui a un sens pour un
    // rayon : il avance le long de sa propre direction, pas le long de l'axe de la caméra. Les deux
    // ne coïncident qu'au centre exact de l'image, et divergent d'autant plus qu'on s'en éloigne.
    // *Confondre les deux donnerait une erreur nulle au milieu et croissante vers les bords —
    // c'est-à-dire un défaut invisible sur un banc qui mesure une ligne centrale.*
    let distance = length(entree.position_monde - oeil);

    return vec4<f32>(normalize(entree.normale_monde), distance);
}
