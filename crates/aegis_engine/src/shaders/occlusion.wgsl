// ── L'OCCLUSION AMBIANTE — ce que le ciel ne voit pas ───────────────────────────────────────
//
// ## Le defaut qu'elle corrige, et il est structurel
//
// `ambiance_hemispherique` donne a chaque surface la lumiere du ciel selon son ORIENTATION, et
// rien d'autre. Une face tournee vers le haut recoit donc autant de ciel au fond d'un trou qu'en
// plein air. C'est faux, et ca se voit partout : les objets ne se posent pas, ils flottent ; deux
// cubes qui se touchent n'ont aucun creux entre eux ; un coin rentrant est aussi clair qu'une
// arete saillante.
//
// L'occlusion ambiante mesure la part du ciel qu'un point voit REELLEMENT, et multiplie
// l'ambiante par elle. C'est le seul terme qui fait qu'un objet touche le sol.
//
// ## ⭐ Pourquoi la normale n'est pas transmise, mais RECONSTRUITE
//
// Un calcul d'occlusion a besoin de la normale de la surface. La faire voyager demanderait une
// image de plus, ecrite pour chaque pixel de chaque image — de la bande passante, la ressource
// rare sur la machine de reference.
//
// Elle se retrouve pourtant exactement, a partir de la seule profondeur : deux points voisins a
// l'ecran donnent deux vecteurs du plan de la surface, dont le produit vectoriel EST la normale.
// Sur une scene organique cette reconstruction serait bruitee ; **ici la scene est faite de cubes,
// donc de plans**, et le resultat est exact, pas approche. *La donnee manquante n'a pas ete
// transmise : elle a cesse d'etre necessaire.*
//
// ⚠ Aux ARETES, les deux voisins tombent sur des surfaces differentes et la normale reconstruite
// n'a aucun sens. On prend donc, de chaque cote, le voisin le PLUS PROCHE en profondeur : celui
// qui a le plus de chances d'appartenir a la meme surface. C'est ce qui evite un lisere sombre le
// long de chaque arete du decor — un defaut qui ressemblerait a un contour dessine expres.
//
// ## Le rayon, et pourquoi il ne se regle pas en pixels
//
// Un rayon en pixels donnerait une occlusion qui grandit quand on s'approche et retrecit quand on
// recule : l'ombre de contact d'un cube changerait de taille selon la camera. Le rayon est donc en
// UNITES DU MONDE, et sa projection a l'ecran se calcule par pixel. Un demi-bloc : au-dela, on ne
// mesure plus un contact mais l'ombre portee d'un objet sur un autre, que la carte d'ombre fait
// deja — et bien mieux.

//!inclure commun
//!inclure plein_ecran

/// Combien de directions on interroge autour de chaque point.
///
/// ⚠ C'est le seul chiffre choisi de ce fichier, et c'est un compromis assume : chaque direction
/// est une lecture de profondeur de plus. Douze suffisent parce que l'angle de depart varie d'un
/// pixel a l'autre — deux voisins n'interrogent donc pas les memes directions, et l'oeil, qui ne
/// lit pas un pixel isole, en percoit bien davantage. Le reste devient du grain fin.
///
/// ⚠⚠ Il n'y a **AUCUNE passe de flou**, contrairement a ce que fait la plupart des montages.
/// C'est un choix, pas un oubli : elle couterait une passe entiere pour lisser un grain qu'on n'a
/// pas encore juge genant. *A ajouter si ca granule a l'oeil, pas avant.*
const DIRECTIONS: i32 = 12;

/// Le rayon d'action, en unites du monde. Un demi-bloc : c'est la distance a laquelle deux
/// surfaces se « touchent » a l'oeil.
const RAYON: f32 = 0.5;

/// Retrouve la position en espace VUE d'un point de l'ecran, a partir de sa profondeur.
///
/// ⚠ On travaille en espace vue et non en espace monde : les distances y sont les memes, et les
/// comparaisons de profondeur y sont directes. En espace monde il faudrait reprojeter a chaque
/// echantillon.
fn position_vue(uv: vec2<f32>, profondeur: f32) -> vec3<f32> {
    // De la coordonnee d'ecran au volume de projection. Le Y est retourne pour la meme raison que
    // partout ailleurs dans ce moteur — voir `plein_ecran.wgsl`.
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, profondeur, 1.0);
    let monde = cadre.inv_view_proj * ndc;
    return monde.xyz / monde.w;
}

fn lire_profondeur(uv: vec2<f32>) -> f32 {
    return textureSample(source, echantillonneur, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0))).r;
}

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    let profondeur = lire_profondeur(in.uv);

    // Le plan lointain : il n'y a rien a occlure sur le ciel, et l'y calculer produirait un
    // cerne sombre autour de la silhouette de chaque objet. ZERO = on ne retire rien.
    if (profondeur >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let pas = texel_source();
    let ici = position_vue(in.uv, profondeur);

    // ── LA NORMALE, RECONSTRUITE PAR LE VOISIN LE PLUS PROCHE ────────────────────────────────
    //
    // De chaque cote, on garde celui dont la profondeur s'ecarte le moins : c'est celui qui
    // appartient a la meme surface. Prendre systematiquement le voisin de droite et du bas
    // donnerait une normale fausse sur toute arete tournee vers la gauche ou vers le haut.
    let droite = position_vue(in.uv + vec2<f32>(pas.x, 0.0), lire_profondeur(in.uv + vec2<f32>(pas.x, 0.0)));
    let gauche = position_vue(in.uv - vec2<f32>(pas.x, 0.0), lire_profondeur(in.uv - vec2<f32>(pas.x, 0.0)));
    let bas = position_vue(in.uv + vec2<f32>(0.0, pas.y), lire_profondeur(in.uv + vec2<f32>(0.0, pas.y)));
    let haut = position_vue(in.uv - vec2<f32>(0.0, pas.y), lire_profondeur(in.uv - vec2<f32>(0.0, pas.y)));

    let dx = select(ici - gauche, droite - ici, length(droite - ici) < length(ici - gauche));
    let dy = select(ici - haut, bas - ici, length(bas - ici) < length(ici - haut));
    let normale = normalize(cross(dx, dy));

    // ⚠ La normale doit regarder VERS la camera, sinon la moitie de l'ecran s'occlut a l'envers.
    // Le sens du produit vectoriel depend de l'orientation des axes ; plutot que de le deduire et
    // de se tromper une fois sur deux, on le CORRIGE : la surface qu'on voit nous fait face.
    let vers_camera = normalize(cadre.camera_et_compte.xyz - ici);
    let n = normale * select(-1.0, 1.0, dot(normale, vers_camera) > 0.0);

    // ── LE BALAYAGE ──────────────────────────────────────────────────────────────────────────
    //
    // On tourne autour du point en interrogeant la profondeur, et l'on compte les directions d'ou
    // quelque chose de PLUS PROCHE nous regarde : c'est du ciel en moins.
    //
    // L'angle de depart varie d'un pixel a l'autre — sans quoi les douze directions formeraient
    // la meme etoile partout, et l'occlusion se lirait comme un motif regulier plaque sur l'image.
    // ⚠ Le desordre devient du grain, et le grain se supporte ; un motif regulier, jamais.
    let desordre = fract(sin(dot(in.uv, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let tour = 6.2831853 / f32(DIRECTIONS);

    // Le rayon du monde, projete a l'ecran a CETTE distance : c'est ce qui rend l'ombre de contact
    // stable quand la camera avance ou recule.
    let distance_camera = max(length(cadre.camera_et_compte.xyz - ici), 1e-3);
    let rayon_ecran = RAYON / distance_camera;

    var occlusion = 0.0;
    for (var i = 0; i < DIRECTIONS; i = i + 1) {
        let angle = (f32(i) + desordre) * tour;
        // Les echantillons ne sont pas tous au bord du disque : la racine repartit les distances
        // de facon uniforme en SURFACE, ce qui donne autant de poids au contact proche qu'au
        // lointain. Sans elle, tout se concentrerait sur le pourtour et le contact serait rate.
        let portee = sqrt((f32(i) + 0.5) / f32(DIRECTIONS));
        let ailleurs = in.uv + vec2<f32>(cos(angle), sin(angle)) * rayon_ecran * portee;

        let p = lire_profondeur(ailleurs);
        if (p >= 1.0) {
            continue;
        }
        let vers = position_vue(ailleurs, p) - ici;
        let distance = length(vers);
        if (distance < 1e-4) {
            continue;
        }

        // Combien cette direction est « au-dessus » de la surface. Une surface coplanaire donne
        // zero ; quelque chose qui se dresse devant donne jusqu'a un.
        let hauteur = max(dot(normalize(vers), n), 0.0);

        // ⚠ Un occulteur LOINTAIN ne doit rien occlure : sinon un mur d'arriere-plan assombrirait
        // tout ce qui se detache devant lui, et le decor entier gagnerait un halo noir. La
        // decroissance est lineaire en distance, nulle au-dela du rayon.
        let portee_utile = clamp(1.0 - distance / RAYON, 0.0, 1.0);
        occlusion = occlusion + hauteur * portee_utile;
    }

    // ⚠⚠ ON REND LA PART A RETIRER, PAS LA VISIBILITE — et ce choix vaut d'etre explique.
    //
    // La suite de la chaine ne fait que deux gestes, tous deux confies a la CARTE plutot qu'a un
    // shader : multiplier l'ambiante par cette valeur (elle devient « ce qu'il faut retirer »),
    // puis la soustraire de la scene. Aucune passe ne lit jamais deux images, donc un seul
    // agencement de descripteurs sert toute la chaine.
    //
    // Stocker la visibilite obligerait quelqu'un a la retourner quelque part, et un seul oubli
    // donne l'image en negatif — le genre de defaut spectaculaire mais qu'on n'attribue pas.
    let a_retirer = clamp(occlusion / f32(DIRECTIONS), 0.0, 1.0);
    return vec4<f32>(a_retirer, a_retirer, a_retirer, 1.0);
}
