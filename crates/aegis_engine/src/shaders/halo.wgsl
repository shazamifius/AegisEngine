// ── LE HALO — LA LUMIERE QUE L'ECRAN NE PEUT PAS MONTRER, RENDUE VISIBLE AUTOUR ─────────────
//
// Preambule partage par les trois passes du halo. `build.rs` le colle en tete de qui ecrit
// `//!inclure halo`.
//
// ## Comment ca marche, en une phrase
//
// On retient ce qui depasse le blanc affichable, on le reduit de moitie en moitie jusqu'a une
// poignee de pixels, puis on remonte en melangeant chaque echelle a la suivante. Ce qui redescend
// est une diffusion sur TOUTES les echelles a la fois : un halo serre pres de la source, un
// rayonnement large et faible loin d'elle. C'est ce que fait un objectif, et un oeil.
//
// ## ⭐⭐ Les trois constantes qu'un halo demande d'habitude, et pourquoi il n'y en a AUCUNE ici
//
// **1. Le seuil.** Ce serait un curseur (0,8 ? 1,0 ?), a rejuger des qu'on touche a l'exposition.
// Il vaut `point_blanc / exposition` — la luminance qui arrive exactement au blanc de l'ecran.
// Voir `seuil_de_debordement` dans `commun.wgsl` : ce n'est pas un choix, c'est une consequence.
//
// **2. L'intensite.** Chaque montee melange **moitie-moitie** l'echelle courante et la somme des
// plus larges. Les poids valent donc 1/2, 1/4, 1/8… et leur somme fait exactement 1 : le halo
// rend l'energie du debordement, ni plus ni moins. Ce 0,5 n'est pas regle a l'oeil — c'est la
// seule valeur pour laquelle la serie converge vers l'unite. Il est grave dans le pipeline
// (`Melange::Moitie`), pas dans un shader.
//
// ⭐ Et il a une consequence qu'aucun reglage n'aurait donnee : **l'intensite du halo ne depend
// pas du nombre de niveaux**, donc pas de la taille de la fenetre. Redimensionner change le
// nombre d'octaves, jamais la force de l'effet. Un poids « 1/N » aurait fait scintiller
// l'intensite a chaque redimensionnement, et personne n'aurait su pourquoi.
//
// **3. Le rayon.** On descend jusqu'a ce qu'un niveau fasse moins de huit pixels — en dessous, une
// image ne porte plus d'information spatiale. Le rayon du halo est donc une FRACTION FIXE de
// l'ecran, la meme en 1080p et sur un casque, sans qu'aucun chiffre ne le decide.
//
// ## Ce qui n'est PAS traite, et qu'il faut savoir avant de le chercher
//
// Un pixel isole tres lumineux peut faire CLIGNOTER le halo d'une image a l'autre — le defaut est
// connu sous le nom de « lucioles », et le remede habituel est de ponderer les lectures par leur
// luminance a la premiere reduction. Ce n'est pas fait : cette scene n'a ni texture ni reflet
// aigu, donc aucune source connue de lucioles, et poser le remede d'un defaut qu'on n'a pas
// observe serait de l'excedent. *A rouvrir si ca scintille — pas avant.*

//!inclure commun
//!inclure plein_ecran

/// Une lecture, dont on ne garde eventuellement que la part qui depasse le blanc affichable.
///
/// ⚠ Le seuil s'applique AVANT la moyenne, jamais apres. Un pixel eclatant entoure de pixels
/// sombres survit donc a la reduction ; seuiller la moyenne l'effacerait, et c'est justement
/// l'etincelle isolee qui doit produire un halo.
fn lire(uv: vec2<f32>, deborde: bool) -> vec3<f32> {
    let lumiere = textureSample(source, echantillonneur, uv).rgb;
    let seuil = select(0.0, seuil_de_debordement(), deborde);
    return max(lumiere - vec3<f32>(seuil), vec3<f32>(0.0));
}

/// Reduit de moitie : cinq lectures, le centre pesant autant que les quatre diagonales reunies.
///
/// C'est le filtre « double » d'ARM (Bjorge, 2015), concu pour les GPU a tuiles. Cinq lectures
/// couvrent en fait seize texels : chacune est posee ENTRE quatre texels, dont le materiel fait
/// la moyenne gratuitement. Un gaussien separable classique demande quinze lectures — trois fois
/// le cout — pour un resultat que l'œil ne distingue pas une fois les echelles superposees.
fn reduire(uv: vec2<f32>, deborde: bool) -> vec3<f32> {
    let pas = texel_source();
    var somme = lire(uv, deborde) * 4.0;
    somme = somme + lire(uv + vec2<f32>( pas.x,  pas.y), deborde);
    somme = somme + lire(uv + vec2<f32>(-pas.x,  pas.y), deborde);
    somme = somme + lire(uv + vec2<f32>( pas.x, -pas.y), deborde);
    somme = somme + lire(uv + vec2<f32>(-pas.x, -pas.y), deborde);
    return somme / 8.0;
}

/// Agrandit au double : neuf lectures en tente, les quatre diagonales comptant double.
///
/// ⚠ Ce n'est pas un simple agrandissement bilineaire. Le materiel saurait le faire seul, et le
/// resultat porterait les losanges caracteristiques d'une interpolation etiree. La tente les
/// efface — c'est elle qui fait qu'un halo remonte ROND au lieu de remonter carre.
fn agrandir(uv: vec2<f32>) -> vec3<f32> {
    // La moitie d'un texel de DESTINATION, exprimee en texels de la source : la destination fait
    // le double, donc un demi-texel de destination vaut un quart de texel de source.
    let pas = texel_source() * 0.25;
    var somme = textureSample(source, echantillonneur, uv + vec2<f32>(-pas.x * 2.0, 0.0)).rgb;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>( pas.x * 2.0, 0.0)).rgb;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>(0.0, -pas.y * 2.0)).rgb;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>(0.0,  pas.y * 2.0)).rgb;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>(-pas.x,  pas.y)).rgb * 2.0;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>( pas.x,  pas.y)).rgb * 2.0;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>(-pas.x, -pas.y)).rgb * 2.0;
    somme = somme + textureSample(source, echantillonneur, uv + vec2<f32>( pas.x, -pas.y)).rgb * 2.0;
    return somme / 12.0;
}
