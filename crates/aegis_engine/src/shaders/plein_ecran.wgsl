// ── CE QUI EST VRAI DE TOUTE PASSE QUI RELIT UNE IMAGE ──────────────────────────────────────
//
// Ce fichier n'est pas compile seul : `build.rs` le colle en tete des shaders qui ecrivent
// `//!inclure plein_ecran`. Il sert au halo (descente, montee) et a la composition.
//
// ## Le triangle, et pourquoi il n'est pas deux
//
// Un seul triangle plus grand que l'ecran, plutot que deux qui le pavent : il n'y a alors aucune
// diagonale au milieu de l'image, donc aucune couture ou les deux moities pourraient etre
// calculees legerement differemment. C'est aussi trois sommets au lieu de six.
//
// ## ⭐ Pourquoi il porte des coordonnees de texture, alors que le fond n'en a pas besoin
//
// Une passe de post-traitement ecrit dans une image d'une AUTRE TAILLE que celle qu'elle lit
// (moitie a la descente, double a la montee). Elle a donc besoin de savoir ou elle se trouve
// *dans sa destination*, en [0,1] — et un fragment ne connait que ses coordonnees en PIXELS.
// Les convertir demanderait la taille de la destination, qu'aucun shader ne peut lire.
//
// L'interpolation la donne gratuitement : `uv` vaut exactement 0 a un bord et 1 a l'autre, quelle
// que soit la taille. *La donnee manquante n'a pas ete transmise, elle a cesse d'etre necessaire.*
// C'est ce qui evite d'avoir a reintroduire des constantes poussees pour porter deux entiers.

@group(1) @binding(0) var source: texture_2d<f32>;
@group(1) @binding(1) var echantillonneur: sampler;

struct SortiePleinEcran {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) sommet: u32) -> SortiePleinEcran {
    var out: SortiePleinEcran;
    // Sommet 0 → (0,0), 1 → (2,0), 2 → (0,2). Les deux derniers debordent : c'est voulu.
    out.uv = vec2<f32>(f32((sommet << 1u) & 2u), f32(sommet & 2u));
    let ndc = out.uv * 2.0 - vec2<f32>(1.0);

    // ⚠⚠ LE Y EST INVERSE ICI, ET C'EST LE MEME PIEGE QUE PARTOUT DANS CE MOTEUR.
    // naga compile avec ADJUST_COORDINATE_SPACE : il inverse le Y de la position de clip EN
    // SORTIE DE VERTEX. On l'inverse donc a l'avance pour qu'il retombe a l'endroit. L'oublier
    // retourne l'image entiere — ce qui saute aux yeux sur une scene, et ne se voit PAS DU TOUT
    // sur un flou. C'est exactement le genre de faute qui s'installe pour des semaines.
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    return out;
}

/// Le pas d'un texel de la SOURCE, en coordonnees normalisees.
///
/// C'est la seule dimension qu'un shader de post-traitement peut connaitre — et elle suffit,
/// parce que le rayon d'un filtre s'exprime naturellement en texels de ce qu'il lit.
fn texel_source() -> vec2<f32> {
    return 1.0 / vec2<f32>(textureDimensions(source));
}
