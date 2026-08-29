// ── CE QUI EST VRAI POUR TOUTE UNE IMAGE ────────────────────────────────────────────────────
//
// Ce fichier n'est pas compile seul : `build.rs` le colle en tete des shaders qui ecrivent
// `//!inclure commun`.
//
// ## Pourquoi il existe, et ce qu'il a corrige
//
// La structure `Cadre` etait recopiee dans DEUX shaders, avec un commentaire qui disait lui-meme
// « trois copies de la meme verite, et les faire diverger decalerait les ombres sans qu'aucune
// ligne ne paraisse fausse ». Le fond en aurait fait une QUATRIEME. Une regle ecrite dans un
// commentaire ne protege de rien ; une definition unique, si.
//
// ## ⭐ Et surtout : le fond et les ombres partagent desormais UNE SEULE fonction
//
// Le defaut le plus visible du moteur au 29 aout 2026 n'etait pas dans l'eclairage : c'etait que
// le FOND ne connaissait pas l'eclairage du tout. Il peignait un « blanc pur studio » ecrit en
// dur (0,97) pendant que les objets etaient eclaires a 0,17 — des objets sombres poses sur un
// fond eclatant, deux mondes dans la meme image. L'œil s'adapte au blanc dominant et tout le
// reste parait terne. *C'etait la cause, pas une question de gout.*
//
// La correction n'est pas « donner au fond les bonnes couleurs » : c'est que le fond CESSE d'etre
// une decoration independante. `ambiance_hemispherique` est appelee par les deux — avec la
// normale pour un objet, avec la direction du regard pour le fond. Ils ne peuvent plus se
// contredire, non pas parce qu'on les a regles pareil, mais parce que c'est le meme calcul.
//
// ⚠⚠ Ce fichier ne doit contenir AUCUNE couleur : un test parcourt tous les shaders du moteur et
// echoue sur tout `vec3<f32>(a, b, c)` a trois composantes differentes. Le moteur sait comment la
// lumiere se comporte ; il n'a pas a savoir de quelle couleur est le ciel.

struct Lumiere {
    position_type: vec4<f32>,     // xyz = position monde, w = type (0 dir, 1 point, 2 projecteur)
    couleur_intensite: vec4<f32>, // rgb = couleur, w = intensite
    direction_cone: vec4<f32>,    // xyz = direction, w = cosinus du demi-angle du cone
};

struct Cadre {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    // L'inverse de `view_proj`. Elle sert au fond, qui n'a aucun sommet a transformer : c'est
    // elle qui, depuis un simple point de l'ecran, redonne la direction que le regard suit dans
    // le monde. Sans elle, un fond ne peut faire qu'un degrade arbitraire — c'est-a-dire choisir
    // une couleur, donc franchir la frontiere.
    inv_view_proj: mat4x4<f32>,
    camera_et_compte: vec4<f32>,  // xyz = position camera, w = nombre de lumieres allumees
    // ── CE QUE LE JEU DECIDE, ET QUE LE MOTEUR NE CHOISIT PAS ──────────────────────────────
    ciel_exposition: vec4<f32>,   // rgb = couleur du ciel, w = exposition
    sol_point_blanc: vec4<f32>,   // rgb = couleur du sol,  w = point blanc
    matiere: vec4<f32>,           // x = rugosite, y = reflectance
    lumieres: array<Lumiere, 16>,
};

@group(0) @binding(0) var<uniform> cadre: Cadre;

// ── LE CIEL, ET CE QU'IL EN RESTE DANS UNE OMBRE ────────────────────────────────────────────
//
// Une ambiante GRISE unique donne des ombres eteintes : de l'ABSENCE de lumiere. Or une ombre
// reelle est la couleur de ce qui l'eclaire encore — le ciel au-dessus, le sol qui renvoie en
// dessous.
//
// ⭐ La MEME fonction repond aux deux questions du moteur, et c'est tout son interet :
//   • « qu'est-ce qui eclaire cette surface ? »  → on lui passe la NORMALE
//   • « qu'y a-t-il derriere, la ou rien n'est dessine ? » → on lui passe la DIRECTION DU REGARD
//
// Aucune constante n'apparait ici, et ce n'est pas un hasard : la largeur de l'horizon, la
// courbe du degrade, la teinte — chacune aurait ete un chiffre arbitraire a justifier pour
// toujours. `direction.y * 0.5 + 0.5` est l'integrale analytique d'un ciel bicolore sur
// l'hemisphere ; elle ne se regle pas, elle se demontre.
//
// ⚠ Elle reste un PIS-ALLER de la vraie lumiere indirecte (etape 4 : les cascades de radiance).
// Le jour ou l'indirect existe, elle disparait — elle ne se raffine pas.
fn ambiance_hemispherique(direction: vec3<f32>) -> vec3<f32> {
    let vers_le_ciel = direction.y * 0.5 + 0.5;
    return mix(cadre.sol_point_blanc.rgb, cadre.ciel_exposition.rgb, vers_le_ciel);
}

// ── L'ESPACE DES COULEURS, ET LA FAUTE QUI RENDAIT TOUT TERNE ───────────────────────────────
//
// ## Ce qui se passait avant le 29 aout 2026, mesure au pixel pres
//
// La chaine d'affichage encodait la gamma DEUX FOIS. Le shader finissait par
// `pow(couleur, 1/2.2)` — et la surface de presentation est un format `B8G8R8A8_SRGB`, qui fait
// **deja** cette conversion tout seul a l'ecriture. Chaque encodage remonte les tons moyens :
// deux encodages delavent tout, ecrasent le contraste et desaturent les couleurs.
//
// *C'etait la cause du « les couleurs ne sont vraiment pas belles du tout, tres ternes ».*
//
// **La preuve etait dans le HUD, et elle est numerique.** `hud.rs` porte un commentaire qui
// affirmait que ses couleurs « sortent telles quelles a l'ecran ». Mesure sur une capture, le
// fond des panneaux — demande a (0,05 / 0,06 / 0,08), donc (13, 15, 20) sur 255 — sortait a
// **(63, 69, 80)**. C'est-a-dire, au bit pres sur les trois canaux, la valeur d'une couleur
// LINEAIRE ecrite dans une cible sRGB. Le HUD etait 4,8 fois trop clair, et le commentaire
// decrivait une intention que personne n'avait verifiee.
//
// ## La regle, et elle n'a rien de negociable
//
// **On calcule en LINEAIRE, on ecrit en LINEAIRE, et le format encode.** Une couleur ecrite par
// un humain (`0.32, 0.82, 0.36` pour de l'herbe) est en revanche pensee en sRGB : c'est ce qu'un
// selecteur de couleur lui montre. Elle se convertit donc a l'ENTREE, une fois — et jamais a la
// sortie. C'est aussi ce qui rend l'eclairage physiquement juste : un albedo sRGB traite comme
// lineaire fausse tout le calcul d'energie, silencieusement.
fn vers_lineaire(couleur_srgb: vec3<f32>) -> vec3<f32> {
    // La vraie courbe sRGB, pas l'approximation en 2,2 : elle a un segment droit pres du noir,
    // sans lequel les tons les plus sombres remontent visiblement.
    let bas = couleur_srgb / 12.92;
    let haut = pow((couleur_srgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(haut, bas, couleur_srgb <= vec3<f32>(0.04045));
}

// ── DE LA LUMIERE A UN PIXEL ────────────────────────────────────────────────────────────────
//
// Reinhard ETENDU, avec un point blanc. Le Reinhard simple (x / (x+1)) compresse aussi les tons
// moyens : tout le monde se retrouve vers 0,5 et l'image perd le contraste — l'inverse exact de
// la clarte recherchee. Avec un point blanc a 2,0, ce qui est sous 1,0 reste presque lineaire et
// seules les hautes lumieres sont ramenees. Rien n'ecrete jamais, donc ajouter une lampe ne peut
// pas produire d'aplat blanc.
//
// ⚠⚠ TOUT ce qui finit a l'ecran doit passer par ici, le fond compris. Deux chemins vers le
// pixel, c'est deux courbes, donc deux mondes qui ne se repondent plus — le defaut exact que ce
// fichier existe pour fermer.
//
// ⚠ Et il rend du LINEAIRE : aucune gamma ne s'ecrit ici, la surface s'en charge. Voir ci-dessus.
fn presenter(lumiere_lineaire: vec3<f32>) -> vec3<f32> {
    let point_blanc = cadre.sol_point_blanc.w;
    // L'exposition est le diaphragme : elle multiplie ce qui arrive, avant la courbe.
    let eclaire = lumiere_lineaire * cadre.ciel_exposition.w;
    return eclaire * (vec3<f32>(1.0) + eclaire / (point_blanc * point_blanc))
         / (vec3<f32>(1.0) + eclaire);
}
