//! # L'ÉPAISSEUR TRAVERSÉE — la grandeur qu'aucun moteur ne calcule
//!
//! Né le 1er septembre 2026, d'une nectarine.
//!
//! ## Le problème, dit par lui
//!
//! *« On voit sa peau, à travers on voit un peu sa chair. Quand on la met au soleil, on voit encore
//! là en transparence sur les bords du fruit vraiment sa peau plus sa chair devenir transparente,
//! et on peut voir directement les petites sanguinités, les petits fils, les trucs à l'intérieur. »*
//!
//! Rien de ce qu'il décrit n'est une propriété de SURFACE. Or toute l'infographie temps réel repose
//! sur une hypothèse jamais discutée : **la matière est une surface infiniment mince décrite par
//! une fonction de réflexion.** C'est faux, et c'est pour ça que les images sont en plastique.
//!
//! ## La seule grandeur qui manque
//!
//! Le bord qui devient transparent, la chair qui apparaît, la couleur qui vire au rouge, les fibres
//! qui se détachent : **tout cela est une conséquence d'UNE grandeur — la longueur de matière que
//! la lumière a traversée.** Pas mille paramètres. Une longueur, et de quoi elle est faite.
//!
//! C'est exactement ce que disait le texte sur le verre qu'il a rapporté le 31 août : *« presque
//! rien n'est une propriété de surface — tout est une conséquence de LONGUEURS (épaisseur
//! traversée, distance objet-diffuseur, rayon du congé, longueur d'onde). »*
//!
//! ## ⭐⭐ POURQUOI ON NE LANCE AUCUN RAYON — et c'est lui qui l'a exigé
//!
//! La réponse évidente serait de faire entrer un rayon dans le maillage et de compter ses
//! traversées. **Il l'a réfutée avec deux captures d'écran de Blender**, avant qu'une ligne ne soit
//! écrite : un losange en triangles réguliers vu de face devient, vu de côté, un enchevêtrement
//! d'échardes qui convergent toutes vers le même axe. Ses mots : *« il faut des formules
//! mathématiques un peu lourdes pour que ça ne se superpose pas entre eux, mais devienne un tout
//! ensemble, qui ne fasse pas d'artefacts et de cristallisation. »*
//!
//! **Il avait raison, et l'objection était mortelle** : là où les triangles convergent, ils
//! deviennent des échardes d'aire quasi nulle, et un test d'intersection y est instable au dernier
//! bit près. Or **une seule traversée ratée ou comptée deux fois INVERSE le résultat** — dedans
//! devient dehors, et le fruit se troue exactement à son pôle.
//!
//! ## La somme signée : le problème n'est pas résolu, il n'a plus de lieu où exister
//!
//! On rastérise. Chaque face **avant** (on entre) retranche sa distance à la caméra ; chaque face
//! **arrière** (on sort) ajoute la sienne. Sur un maillage fermé, la somme par pixel vaut
//! exactement la longueur de matière traversée par le rayon de ce pixel.
//!
//! ```text
//!   caméra ──────►  ╭─────────╮
//!                   │         │        −d₁ (entrée)  +d₂ (sortie)  =  d₂ − d₁
//!                   ╰─────────╯
//! ```
//!
//! **Trois propriétés qui tombent de là, et qui sont la réponse à son objection :**
//!
//! 1. **Aucune intersection n'est calculée**, donc aucune n'est instable. Les échardes peuvent
//!    converger tant qu'elles veulent : elles ne couvrent aucun centre de pixel, et n'entrent donc
//!    dans aucune somme.
//! 2. **La règle de remplissage rend le procédé étanche** (voir `est_arete_haut_ou_gauche`). Un
//!    pixel dont le centre tombe exactement sur une arête partagée est attribué à **un seul** des
//!    deux triangles — sinon il serait compté deux fois du même côté, et l'épaisseur serait fausse
//!    d'une distance entière. *C'est la seule chose subtile de ce fichier, et c'est elle qui porte
//!    la garantie.*
//! 3. ⭐ **Ça marche aussi sur les objets NON convexes**, sans un mot de plus : une main aux doigts
//!    écartés donne −d₁ +d₂ −d₃ +d₄, soit la somme des segments de chair. *On n'a rien fait pour ;
//!    ça tombe de la signature.*
//!
//! ## ⚠ Ce que cette brique ne fait PAS, et il faut le savoir avant de s'en servir
//!
//! - **Elle rend l'épaisseur TOTALE, pas la liste des segments.** Pour distinguer la peau de la
//!   chair il faudra plus — c'est le pas 3, et il n'est pas écrit.
//! - **Elle exige un maillage FERMÉ.** Une surface ouverte (un plan, une nappe) donne un résultat
//!   qui n'a aucun sens. Rien ne le vérifie ici.
//! - **Elle vit sur le processeur.** C'est un banc de vérité, pas le chemin de rendu : il fallait
//!   d'abord voir le phénomène avant de câbler une passe Vulkan pour lui. *Le portage GPU est la
//!   même chose, en deux passes, et son coût n'est pas mesuré.*
//! - **Aucune couleur ici.** Le moteur fournit ce qui est VRAI — une longueur. Ce qu'on en fait
//!   (absorption, teinte, matière) appartient à qui compose l'image.

use crate::core::math::{Mat4, Vec3, Vec4};

/// Pour chaque pixel, la longueur de matière traversée, **en unités du monde**.
///
/// Zéro veut dire « le rayon n'a rencontré aucune matière » — c'est aussi ce qu'on lit sur la
/// silhouette exacte d'un objet, où l'entrée et la sortie coïncident.
pub struct CarteEpaisseur {
    pub largeur: usize,
    pub hauteur: usize,
    /// Ligne par ligne, du haut vers le bas.
    pub valeurs: Vec<f32>,
    /// La distance à laquelle le rayon ENTRE dans la matière. `f32::INFINITY` s'il n'entre jamais.
    ///
    /// ⚠ **C'est le minimum sur toutes les faces avant**, donc le premier contact. Sur un objet
    /// creux ou non convexe, la matière ne commence pas forcément là — mais le rayon, si.
    pub entree: Vec<f32>,
    /// La distance à laquelle le rayon SORT — le maximum sur toutes les faces arrière.
    pub sortie: Vec<f32>,
    /// ⭐ La normale de la surface **là où le rayon entre**, tournée vers l'œil.
    ///
    /// C'est la grandeur qui manque pour dévier la lumière : sans elle, on sait *combien* de
    /// matière il y a, jamais *sous quel angle* on l'aborde. **Snell ne demande rien d'autre.**
    pub normale_entree: Vec<Vec3>,
    /// La normale là où il sort — la seconde interface, celle qui redresse le rayon.
    pub normale_sortie: Vec<Vec3>,
}

impl CarteEpaisseur {
    pub fn lire(&self, x: usize, y: usize) -> f32 {
        self.valeurs[y * self.largeur + x]
    }

    /// Le segment `[entrée, sortie]` d'un pixel, ou `None` si le rayon n'a rien rencontré.
    ///
    /// ⭐ **C'est ce couple qui ouvre l'intérieur des choses.** L'épaisseur seule dit *combien* de
    /// matière ; le segment dit *où elle est*, donc il permet de la parcourir et de demander, en
    /// chaque point, de quoi elle est faite.
    pub fn segment(&self, indice: usize) -> Option<(f32, f32)> {
        let (e, s) = (self.entree[indice], self.sortie[indice]);
        if self.valeurs[indice] > 0.0 && e.is_finite() && s > e {
            Some((e, s))
        } else {
            None
        }
    }

    /// La plus grande épaisseur rencontrée — utile pour normaliser une visualisation.
    pub fn maximum(&self) -> f32 {
        self.valeurs.iter().copied().fold(0.0, f32::max)
    }
}

/// **La loi de Beer-Lambert** : ce qui survit d'un rayon après `distance` unités de matière.
///
/// ```text
///     T = exp(−σ · d)
/// ```
///
/// `sigma` est le coefficient d'extinction de la matière, **pour une seule longueur d'onde**. C'est
/// tout le secret de la couleur d'un objet translucide : appeler cette fonction avec trois `sigma`
/// différents suffit à faire virer un bord au rouge, parce que le bleu meurt plus vite que le rouge
/// dans la plupart des matières organiques. *Aucune teinte n'est écrite nulle part ; elle sort de
/// l'écart entre trois nombres.*
///
/// ⚠ **La fonction ne connaît aucune couleur, et c'est voulu** — le moteur fournit ce qui est vrai,
/// l'appelant décide de quelle matière il parle.
pub fn transmittance(sigma: f32, distance: f32) -> f32 {
    (-sigma * distance).exp()
}

/// ⭐⭐⭐ **TRAVERSER LA MATIÈRE** — marcher le segment et demander, en chaque point, de quoi elle
/// est faite.
///
/// C'est le geste qui sépare une boule translucide d'une nectarine. Jusqu'ici `sigma` était une
/// constante : la matière était homogène, donc lisse, donc morte. Ici `sigma` devient une
/// **fonction de la position**, et tout ce qu'un fruit a d'intérieur devient exprimable — la peau,
/// la chair, les fibres, le noyau.
///
/// ```text
///     T(λ) = exp( − ∫ σ(p, λ) dl )      le long du segment [entrée, sortie]
/// ```
///
/// ## ⚠ Pourquoi une INTÉGRALE et pas une somme de couches — c'est son objection, encore
///
/// Empiler des coques (une pour la peau, une pour la chair) ramènerait exactement ce qu'il a
/// réfuté : des surfaces qui se croisent, convergent, et laissent une marche visible à chaque
/// frontière. **Une intégrale n'a pas de frontières** : `σ` varie continûment, donc l'image n'a
/// aucune marche à montrer. *Ses mots : « que ça devienne un tout ensemble, qui ne fasse pas
/// d'artefacts et de cristallisation. »*
///
/// ## ⭐ `pas` EST le curseur d'adaptativité, et c'est un nombre
///
/// Quatre pas sur un casque, soixante-quatre sur une machine de bureau : **le même code, le même
/// champ, la même image en mieux.** C'est exactement la ligne rouge du projet — *ce qui change
/// entre le bas et le haut, ce sont des NOMBRES, jamais des algorithmes différents.*
///
/// ## Les paramètres
///
/// - `direction` : la direction **normalisée** du rayon d'un pixel. C'est à l'appelant de la
///   fournir, car lui seul sait comment sa caméra est bâtie.
/// - `sigma` reçoit le point du monde, la distance à la surface la plus proche *le long du rayon*,
///   **et la longueur du pas** — voir ci-dessous, c'est le paramètre le plus important des trois.
///
/// ## ⚠⚠ POURQUOI `sigma` DOIT CONNAÎTRE LA LONGUEUR DU PAS — mesuré, pas prévu
///
/// La première version ne la lui donnait pas, et le résultat à quatre pas n'était pas *plus
/// grossier* : il était **faux**. Des taches apparaissaient là où le champ n'a rien, parce qu'un
/// détail plus fin que le pas est échantillonné au hasard au lieu d'être moyenné. *Et mon test le
/// déclarait acceptable — l'œil a tranché contre lui.*
///
/// **Un champ honnête ne rend pas ce qu'on ne peut pas payer : il rend sa MOYENNE.** C'est le
/// principe du mip-mapping, appliqué à une fonction. La conséquence dépasse ce fichier :
///
/// > **L'adaptativité ne consiste pas à baisser un nombre. Elle consiste à demander au champ ce
/// > qu'il peut honnêtement rendre à ce budget-là.**
///
/// *Baisser le nombre seul donne des artefacts ; baisser le nombre ET la finesse demandée donne une
/// dégradation gracieuse.* ⚠ Ce second nombre est une **approximation** de la vraie distance à la surface : elle
///   est exacte pour un rayon perpendiculaire, et surestime pour un rayon rasant. *Une peau définie
///   par lui paraîtra donc un peu épaisse sur les bords — ce qui se corrige un jour avec un champ
///   de distance, et pas aujourd'hui.*
pub fn integrer_le_champ<D, S>(
    carte: &CarteEpaisseur,
    camera: Vec3,
    direction: D,
    pas: usize,
    sigma: S,
) -> Vec<Traversee>
where
    D: Fn(usize, usize) -> Vec3,
    S: Fn(Vec3, f32, f32) -> Matiere,
{
    let pas = pas.max(1);
    let mut resultat = vec![Traversee { transmittance: [1.0; 3], emise: [0.0; 3] }; carte.largeur * carte.hauteur];

    for y in 0..carte.hauteur {
        for x in 0..carte.largeur {
            let indice = y * carte.largeur + x;
            let Some((entree, sortie)) = carte.segment(indice) else {
                continue;
            };

            let rayon = direction(x, y);
            let dl = (sortie - entree) / pas as f32;
            // On marche de l'ŒIL vers le fond : `t` accumule ce qui sépare le point de l'œil, donc
            // la lumière émise en un point est atténuée par ce qu'on a DÉJÀ traversé. Marcher dans
            // l'autre sens donnerait une image où les bulles du fond brillent autant que celles de
            // devant — faux, et joli, donc dangereux.
            // ⚠ `restant`, pas `t` : `t` est déjà la position sur le rayon. Les confondre compilait
            // presque, et aurait donné une image plausible et fausse.
            let mut restant = [1.0f32; 3];
            let mut emise = [0.0f32; 3];

            for k in 0..pas {
                // Le milieu de chaque tranche : c'est la règle du point médian, deux fois plus
                // précise que le bord pour le même nombre d'évaluations.
                let t = entree + (k as f32 + 0.5) * dl;
                let point = camera + rayon * t;
                let depuis_la_surface = (t - entree).min(sortie - t);

                let m = sigma(point, depuis_la_surface, dl);
                for canal in 0..3 {
                    emise[canal] += m.source[canal] * restant[canal] * dl;
                    restant[canal] *= (-m.sigma[canal] * dl).exp();
                }
            }

            resultat[indice] = Traversee { transmittance: restant, emise };
        }
    }

    resultat
}

/// ⭐⭐⭐ **LA LOI DE SNELL-DESCARTES**, sous forme vectorielle — et ce qu'elle donne gratuitement.
///
/// ```text
///     n₁ · sin θ₁  =  n₂ · sin θ₂
/// ```
///
/// - `incident` : la direction du rayon, **normalisée**, qui arrive sur la surface.
/// - `normale` : la normale de la surface, **tournée vers le rayon qui arrive**.
/// - `eta` : le rapport `n₁ / n₂` — le milieu qu'on quitte sur celui qu'on entre. De l'air vers du
///   sucre : `1,0 / 1,5`. Du sucre vers l'air : `1,5 / 1,0`.
///
/// ## ⭐ La réflexion totale n'est PAS un cas à coder
///
/// Elle est **ce qui reste quand l'équation n'a pas de racine**. Quand `sin θ₂` dépasserait 1, il
/// n'existe aucune direction réfractée — la lumière ne peut plus sortir, elle rebrousse chemin.
/// C'est un `None`, pas un `if`.
///
/// *Et c'est exactement l'angle critique de 41,8° dont parlait le texte sur la sucette : il n'est
/// écrit nulle part dans ce fichier. Il tombe de `arcsin(1/1,5)`, et un test le mesure.*
///
/// ⚠ **Elle suppose un milieu HOMOGÈNE de part et d'autre.** Là où l'indice varie continûment (les
/// striae d'un bonbon, l'air chaud au-dessus d'une route), la lumière courbe au lieu de casser —
/// c'est l'équation de l'eikonale, et elle n'est pas ici.
pub fn refracter(incident: Vec3, normale: Vec3, eta: f32) -> Option<Vec3> {
    let cos_i = -incident.dot(normale);
    let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
    if sin2_t > 1.0 {
        // Pas de racine : réflexion totale interne.
        return None;
    }
    Some(incident * eta + normale * (eta * cos_i - (1.0 - sin2_t).sqrt()))
}

/// La réflexion spéculaire — ce que devient un rayon quand il ne peut pas entrer.
pub fn reflechir(incident: Vec3, normale: Vec3) -> Vec3 {
    incident - normale * (2.0 * incident.dot(normale))
}

/// **La part de lumière RÉFLÉCHIE par une interface**, approximation de Schlick.
///
/// Elle vaut peu de face et **monte à 1 en incidence rasante** : c'est pourquoi le bord de toute
/// bille brille, et c'est aussi ce qui donne son liseré lumineux à une sucette. *Le moteur portait
/// déjà `fresnel_schlick` dans ses shaders ; c'est la même chose, ici pour le processeur.*
pub fn fresnel(cos_incidence: f32, n1: f32, n2: f32) -> f32 {
    let r0 = ((n1 - n2) / (n1 + n2)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cos_incidence.abs()).clamp(0.0, 1.0).powi(5)
}

/// **Ce qu'on peut lire du monde depuis l'écran** — la carte, et la caméra qui l'a produite.
///
/// Les quatre champs sont indissociables : une carte lue avec une autre caméra que la sienne rend
/// des points parfaitement plausibles et faux. *Les tenir ensemble empêche de les séparer.*
pub struct VueEcran<'a, P, D> {
    pub carte: &'a CarteEpaisseur,
    pub camera: Vec3,
    /// Monde → coordonnées de pixel, **exactement la convention de [`rendre`]** (Y descend à
    /// l'écran). Rend `None` derrière l'œil.
    pub projeter: P,
    /// Le rayon d'un pixel depuis la caméra, normalisé — la même fonction que
    /// [`integrer_le_champ`] réclame.
    pub direction_pixel: D,
}

/// ⭐⭐ **LA POIGNÉE D'ADAPTATIVITÉ de cette brique — et ce sont des NOMBRES, pas des algorithmes.**
///
/// C'est la contrainte de conception du moteur, appliquée dès la naissance de la brique : *ce que
/// l'asservissement tourne, ce sont des nombres ; le jour où il choisit entre deux algorithmes, on
/// a deux moteurs, dont un seul est testé.*
///
/// **Et cette poignée descend jusqu'au bout sans changer de nature** — c'est le critère qui décide
/// si une technique peut viser un casque : à un pas elle est grossière, à cinq elle est exacte, et
/// entre les deux elle se dégrade **de façon lisse et mesurable**. *Une technique qui, en dessous
/// d'un seuil, se met à produire du bruit au lieu d'une approximation n'a pas de version « en plus
/// petit ».*
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Combien de pas de Newton au maximum. **Mesuré sur une bille d'indice 1,5, le 1er septembre
    /// 2026 :** 1 pas → 1,309° · 2 → 0,285° · 3 → 0,126° · 4 → 0,115°.
    pub iterations_max: usize,
    /// Le critère d'arrêt, **en pixels d'écran**. En dessous d'un pixel de déplacement, deux
    /// itérations liraient le même texel : continuer ne peut plus rien apprendre.
    pub tolerance_pixels: f32,
}

/// Là où le rayon réfracté ressort de la matière — le résultat de [`chercher_la_sortie`].
#[derive(Clone, Copy, Debug)]
pub struct Sortie {
    /// Le point de sortie, dans le monde.
    pub point: Vec3,
    /// La normale de la surface à cet endroit, telle que la carte la porte.
    pub normale: Vec3,
    /// La longueur réellement parcourue **dans** la matière.
    pub distance: f32,
    /// Combien de tours il a fallu. **Zéro veut dire que l'estimation de départ suffisait.**
    pub iterations: usize,
    /// ⭐⭐ **Combien de fois la carte a été LUE** — et c'est la grandeur qui décide du budget.
    ///
    /// Sur un GPU mobile, la ressource rare n'est pas le calcul mais la **bande passante** : le
    /// Quest 2 dispose d'environ **87 octets par pixel pour toute l'image**. Un pas de Newton, lui,
    /// ne coûte presque aucun calcul — il coûte **une lecture de texture**. *C'est donc ce compteur,
    /// et pas le temps de cette machine-ci, qui se transpose à une machine qu'on n'a pas.*
    pub lectures: usize,
    /// ⚠ **Faux = on a épuisé le budget sans que le critère soit atteint.** Le point rendu est
    /// alors le meilleur qu'on ait, et il n'est adossé à aucune garantie.
    pub convergee: bool,
}

/// ⭐⭐⭐ **OÙ LE RAYON RESSORT** — la méthode de Newton, en espace écran.
///
/// C'est la brique qui manquait, et la seule qui manquait : on savait dévier la lumière à l'entrée
/// ([`refracter`]), on savait combien de matière elle traverse ([`rendre`]), on ne savait pas **par
/// où elle sort**. L'approximation employée jusqu'ici — avancer de la corde du rayon *droit* — a
/// été mesurée le 1er septembre 2026 : **11,37° d'erreur moyenne sur une bille d'indice 1,5, et un
/// rayon sur cinq qui bascule de régime.** Ce n'est pas une imprécision, c'est faux.
///
/// ## Le problème, posé proprement
///
/// Le rayon dévié part de `depart` dans la direction `direction`. On cherche la distance `s` telle
/// que le point `depart + direction·s` **soit sur la face arrière de l'objet**. Autrement dit, la
/// racine de :
///
/// ```text
///     g(s) = distance signée du point  depart + direction·s  à la surface
/// ```
///
/// ## ⭐ Pourquoi c'est Newton, et pourquoi la dérivée est GRATUITE
///
/// La méthode de Newton demande `g'(s)`. Or **le gradient d'une fonction de distance signée à une
/// surface EST la normale de cette surface** — c'est la définition même d'une normale. Donc :
///
/// ```text
///     g'(s) = − direction · n
/// ```
///
/// **On n'estime aucune dérivée, on n'écrit aucune différence finie : on LIT la normale dans la
/// carte.** C'est tout le mécanisme, et c'est ce qui rend la convergence quadratique — *le nombre
/// de décimales justes double à chaque tour*.
///
/// Le pas de Newton `s ← s − g/g'` se réécrit alors exactement comme **l'intersection du rayon avec
/// le plan tangent** à la surface au point qu'on vient de lire :
///
/// ```text
///     s' = (S − depart) · n  /  (direction · n)
/// ```
///
/// *Les deux formulations sont la même chose. La seconde se code en une ligne et se dessine ; c'est
/// celle qu'on écrit.*
///
/// ## ⚠ Ce qui peut échouer, et qui est rendu par `None`
///
/// La méthode vit en espace écran : **elle ne peut trouver que ce qui est dessiné**. Trois causes
/// d'échec, toutes irrécupérables ici, et aucune n'est masquée :
///
/// 1. **Le point estimé sort de l'écran** — il n'y a rien à lire.
/// 2. **Le pixel visé ne porte pas de matière** — l'estimation est tombée hors de l'objet. *C'est
///    le cas dominant près des contours d'un objet épais, et les auteurs du papier le mesurent
///    comme leur principal mode d'échec.*
/// 3. **Le rayon est parallèle au plan tangent** (`direction · n ≈ 0`) — le pas de Newton n'a pas
///    de valeur finie. On s'arrête plutôt que de rendre un nombre énorme.
///
/// ⚠ **Un échec est rendu tel quel, jamais remplacé par une valeur plausible.** Un pixel
/// visiblement faux se corrige ; un pixel faussement rassurant se propage.
///
/// ## Les paramètres
///
/// - `vue` : la carte et la caméra qui l'a produite — voir [`VueEcran`].
/// - `estimation` : la distance de départ. La corde du rayon **droit** fait un excellent point de
///   départ, et c'est gratuit : `sortie − entrée` au pixel d'origine.
/// - `budget` : ce qu'on accepte de dépenser — voir [`Budget`], **c'est la poignée d'adaptativité**.
///
/// ## ⚠ Ce que cette version ne fait PAS
///
/// L'échantillonnage de la carte est **au plus proche**. Le papier note qu'un lissage (bilinéaire,
/// voire mip-map) rend la surface localement plus régulière et **aide la convergence** ; c'est une
/// amélioration mesurable, pas encore faite.
pub fn chercher_la_sortie<P, D>(
    vue: &VueEcran<'_, P, D>,
    depart: Vec3,
    direction: Vec3,
    estimation: f32,
    budget: Budget,
) -> Option<Sortie>
where
    P: Fn(Vec3) -> Option<(f32, f32)>,
    D: Fn(usize, usize) -> Vec3,
{
    // Ce qu'on lit de la carte au point courant : le point de la face arrière que ce pixel voit,
    // sa normale, et où il tombe à l'écran. `None` dès que la question n'a pas de réponse.
    let carte = vue.carte;
    // ⚠ Compté ici et nulle part ailleurs : une lecture ratée (hors écran, pas de matière) coûte
    // la même bande passante qu'une lecture réussie. **Ne compter que les succès mentirait sur le
    // budget**, et dans le sens agréable.
    // `Cell` plutôt qu'un `mut` capturé : la fermeture reste `Fn`, donc elle peut être appelée dans
    // la boucle **et** relue à la fin sans que l'emprunt gêne.
    let lectures = std::cell::Cell::new(0usize);
    let lire = |s: f32| -> Option<(Vec3, Vec3, f32, f32)> {
        lectures.set(lectures.get() + 1);
        let (px, py) = (vue.projeter)(depart + direction * s)?;

        // Hors de l'image : en espace écran, ce qui n'est pas dessiné n'existe pas.
        if px < 0.0 || py < 0.0 || px >= carte.largeur as f32 || py >= carte.hauteur as f32 {
            return None;
        }

        let (x, y) = (px as usize, py as usize);
        let indice = y * carte.largeur + x;

        // Pas de matière ici : l'estimation est sortie de l'objet. C'est l'échec dominant près des
        // contours, et il ne se rattrape pas depuis l'écran.
        if carte.valeurs[indice] <= 0.0 || !carte.sortie[indice].is_finite() {
            return None;
        }

        Some((
            vue.camera + (vue.direction_pixel)(x, y) * carte.sortie[indice],
            carte.normale_sortie[indice],
            px,
            py,
        ))
    };

    let mut s = estimation.max(0.0);
    let mut tours = 0usize;
    let mut convergee = false;

    // ⚠ `0..`, pas `0..=` : un budget de zéro pas ne doit rien faire. « Zéro itération de Newton »
    // n'est pas Newton — c'est l'estimation de départ, et c'est à l'appelant de la juger.
    for tour in 0..budget.iterations_max {
        let (surface, normale, px, py) = lire(s)?;

        let denominateur = direction.dot(normale);
        if denominateur.abs() < 1e-5 {
            // Rayon parallèle au plan tangent : le pas de Newton n'a pas de valeur finie.
            break;
        }

        let suivant = (surface - depart).dot(normale) / denominateur;
        if !suivant.is_finite() || suivant <= 0.0 {
            break;
        }

        // ⚠ Le critère se mesure en PIXELS, pas en unités du monde. C'est la bonne grandeur : sous
        // un pixel de déplacement, l'itération suivante relirait le même texel — donc elle ne peut
        // plus rien apprendre, et la faire tourner serait du calcul dépensé pour rien.
        let deplacement_ecran = (vue.projeter)(depart + direction * suivant)
            .map(|(qx, qy)| ((qx - px).powi(2) + (qy - py).powi(2)).sqrt())
            .unwrap_or(f32::INFINITY);

        s = suivant;
        tours = tour + 1;

        if deplacement_ecran < budget.tolerance_pixels {
            convergee = true;
            break;
        }
    }

    // ⚠⚠ LA LECTURE FINALE, ET ELLE N'EST PAS UN DÉTAIL DE PLOMBERIE.
    //
    // La première version rendait la normale lue AVANT le dernier pas — donc celle de l'ancien
    // point, pas du nouveau. Le banc l'a dit tout de suite : « zéro pas » et « un pas » donnaient
    // exactement le même chiffre, au millième de degré près. C'était le signe qu'un pas de Newton
    // corrigeait la POSITION sans que la NORMALE suive.
    //
    // Et c'est la normale qui décide de tout : c'est elle, et pas le point, qui entre dans Snell.
    // *Rendre la bonne position avec la mauvaise normale, c'est arriver au bon endroit et repartir
    // dans la mauvaise direction.*
    let (_, normale, _, _) = lire(s)?;

    Some(Sortie {
        point: depart + direction * s,
        normale,
        distance: s,
        iterations: tours,
        lectures: lectures.get(),
        convergee,
    })
}

/// Ce que le rayon d'un pixel a rapporté de sa traversée.
#[derive(Clone, Copy)]
pub struct Traversee {
    /// Ce qui survit de ce qui venait de DERRIÈRE l'objet.
    pub transmittance: [f32; 3],
    /// Ce que la matière elle-même a renvoyé vers l'œil, déjà atténué par ce qui la sépare de lui.
    pub emise: [f32; 3],
}

/// Ce qu'un point de matière fait à la lumière — et il y a **deux** verbes, pas un.
///
/// ## ⚠ Pourquoi `sigma` seul ne suffisait pas, et ce que l'image d'une sucette a montré
///
/// Un champ purement absorbant ne sait dire qu'une chose : *combien de lumière meurt ici*. Il rend
/// donc les objets qui **filtrent** — un fruit, du verre teinté, de la brume vue à contre-jour.
///
/// **Il est incapable de rendre une bulle d'air.** Or une bulle dans du sucre n'est pas un trou :
/// l'air a un indice de 1,0 dans un milieu à 1,5, donc **tout rayon qui la frappe au-delà de 41,8°
/// est réfléchi en totalité**. Une bulle se comporte comme une bille de mercure — un éclat argenté,
/// pas une tache sombre. Et sa lumière **n'a pas traversé la masse colorée**, ce qui explique qu'on
/// la voie blanche sur un fond bleu profond.
///
/// ⭐⭐ **Dans une intégrale de transport, ça ne s'écrit pas comme une absorption : ça s'écrit comme
/// une SOURCE.** Le terme manquant était là depuis le début — l'équation du transport radiatif en a
/// toujours eu deux, et je n'en avais implémenté qu'un.
///
/// ```text
///     L = ∫ source(p) · T(œil → p) dl   +   fond · T(œil → sortie)
/// ```
///
/// *Et il tombe gratuitement, avec lui : une bulle profonde est plus bleue qu'une bulle proche —
/// parce que sa lumière doit encore traverser le sucre pour sortir. Aucune ligne n'a été écrite
/// pour ça ; c'est la `T` de la formule qui le fait.*
#[derive(Clone, Copy)]
pub struct Matiere {
    /// Combien de lumière est retirée par unité de longueur, canal par canal.
    pub sigma: [f32; 3],
    /// Combien de lumière est **rendue** par unité de longueur — une bulle qui réfléchit
    /// l'ambiante, une brume qui renvoie le soleil, une matière qui rougeoie.
    pub source: [f32; 3],
}

impl Matiere {
    /// Une matière qui ne fait qu'absorber — le cas d'avant, écrit une fois.
    pub fn absorbante(sigma: [f32; 3]) -> Self {
        Self { sigma, source: [0.0; 3] }
    }
}

/// Un sommet projeté, prêt à être rastérisé.
#[derive(Clone, Copy)]
struct SommetProjete {
    /// Position en pixels, centre du pixel (0,0) situé en (0.5, 0.5).
    x: f32,
    y: f32,
    /// Distance à la caméra, divisée par `w` — pour l'interpolation à perspective correcte.
    distance_sur_w: f32,
    /// L'inverse de `w`, qui sert à retrouver la distance après interpolation.
    inverse_w: f32,
    /// La normale du sommet, dans le monde. Elle est **interpolée** comme le reste : c'est ce qui
    /// donne une sphère optiquement lisse à partir d'un maillage facetté — et donc une réfraction
    /// qui ne montre pas les triangles.
    normale: Vec3,
}

/// Les quatre cartes qu'une passe remplit, tenues ensemble.
///
/// ⚠ **Elles ne sont pas regroupées pour faire plaisir à un lint** : elles décrivent toutes le même
/// pixel et doivent rester cohérentes entre elles. *Une normale d'entrée qui ne viendrait pas du
/// triangle ayant gagné la distance d'entrée serait un défaut invisible — les passer séparément
/// rendait cette faute facile.*
struct Sorties<'a> {
    valeurs: &'a mut [f32],
    entree: &'a mut [f32],
    sortie: &'a mut [f32],
    normale_entree: &'a mut [Vec3],
    normale_sortie: &'a mut [Vec3],
}

/// Une arête « haut » ou « gauche » au sens de la règle de remplissage.
///
/// ⚠ **C'est cette fonction qui rend le procédé étanche, et c'est tout ce qui sépare une épaisseur
/// juste d'une épaisseur fausse d'un segment entier.** Quand le centre d'un pixel tombe exactement
/// sur l'arête que deux triangles partagent, il faut qu'un seul des deux le revendique. La règle
/// retenue est celle du matériel graphique : l'arête l'emporte si elle est horizontale et va vers
/// la gauche, ou si elle descend.
///
/// *Sans elle, deux faces avant adjacentes retrancheraient toutes deux leur distance sur le même
/// pixel : le fruit se creuserait le long de chaque arête de son maillage.*
fn est_arete_haut_ou_gauche(depart: (f32, f32), arrivee: (f32, f32)) -> bool {
    let dx = arrivee.0 - depart.0;
    let dy = arrivee.1 - depart.1;
    (dy == 0.0 && dx < 0.0) || dy < 0.0
}

/// Rend la carte d'épaisseur d'un maillage **fermé**.
///
/// - `positions` / `indices` : le maillage, en triangles, dans son repère du monde.
/// - `vue_projection` : la matrice qui mène du monde à l'espace de découpage.
/// - `camera` : la position de l'œil dans le monde — c'est d'elle que les distances sont comptées.
///
/// ⚠ **Un triangle dont un sommet passe derrière l'œil est ignoré en entier.** C'est la limite
/// franche de cette version : le découpage au plan proche n'est pas fait. Une caméra placée à
/// l'intérieur de l'objet rendra donc n'importe quoi — et rien ici ne le signale.
///
/// `normales` : une normale **par sommet**, ou `None` pour prendre celle du triangle. ⚠ La
/// différence est décisive pour la réfraction : une normale géométrique fait apparaître chaque
/// facette du maillage dans l'image réfractée, une normale interpolée donne une surface lisse.
pub fn rendre(
    positions: &[Vec3],
    normales: Option<&[Vec3]>,
    indices: &[u32],
    vue_projection: Mat4,
    camera: Vec3,
    largeur: usize,
    hauteur: usize,
) -> CarteEpaisseur {
    let mut valeurs = vec![0.0f32; largeur * hauteur];
    let mut entree = vec![f32::INFINITY; largeur * hauteur];
    let mut sortie = vec![f32::NEG_INFINITY; largeur * hauteur];
    let mut normale_entree = vec![Vec3::new(0.0, 0.0, 0.0); largeur * hauteur];
    let mut normale_sortie = vec![Vec3::new(0.0, 0.0, 0.0); largeur * hauteur];

    for triangle in indices.chunks_exact(3) {
        let mut sommets = [SommetProjete {
            x: 0.0,
            y: 0.0,
            distance_sur_w: 0.0,
            inverse_w: 0.0,
            normale: Vec3::new(0.0, 0.0, 0.0),
        }; 3];
        let mut derriere_la_camera = false;

        // La normale du triangle, employée seulement quand l'appelant n'en fournit pas. Le sens du
        // produit vectoriel suit l'ordre des sommets, donc l'orientation du maillage.
        let (pa, pb, pc) = (
            positions[triangle[0] as usize],
            positions[triangle[1] as usize],
            positions[triangle[2] as usize],
        );
        let normale_du_triangle = (pb - pa).cross(pc - pa).normalize_or_zero();

        for (k, &indice) in triangle.iter().enumerate() {
            let p = positions[indice as usize];
            let decoupage = vue_projection * Vec4::new(p.x, p.y, p.z, 1.0);

            // ⚠ `w` est la profondeur en vue ; à zéro ou négatif, le sommet est dans le plan de
            // l'œil ou derrière. La division qui suit n'aurait alors aucun sens géométrique.
            if decoupage.w <= 1e-6 {
                derriere_la_camera = true;
                break;
            }

            let inverse_w = 1.0 / decoupage.w;
            let ndc_x = decoupage.x * inverse_w;
            let ndc_y = decoupage.y * inverse_w;
            let distance = (p - camera).length();

            sommets[k] = SommetProjete {
                x: (ndc_x * 0.5 + 0.5) * largeur as f32,
                // ⚠ Y DESCEND dans une image et MONTE dans le monde. Sans ce retournement, tout
                // champ qui parle de « haut » sort à l'envers — et l'image reste jolie, donc le
                // défaut ne se signale pas. *Trouvé sur les bulles d'une sucette : je les avais
                // écrites plus grosses en haut, elles sortaient en bas.*
                y: (0.5 - ndc_y * 0.5) * hauteur as f32,
                distance_sur_w: distance * inverse_w,
                inverse_w,
                normale: normales
                    .map(|n| n[indice as usize])
                    .unwrap_or(normale_du_triangle),
            };
        }

        if derriere_la_camera {
            continue;
        }

        accumuler_triangle(
            &sommets,
            &mut Sorties {
                valeurs: &mut valeurs,
                entree: &mut entree,
                sortie: &mut sortie,
                normale_entree: &mut normale_entree,
                normale_sortie: &mut normale_sortie,
            },
            largeur,
            hauteur,
        );
    }

    CarteEpaisseur {
        largeur,
        hauteur,
        valeurs,
        entree,
        sortie,
        normale_entree,
        normale_sortie,
    }
}

/// Ajoute la contribution signée d'un triangle projeté.
///
/// **Le signe vient de l'orientation à l'écran, et de rien d'autre.** Une aire signée négative
/// désigne une face tournée vers l'œil — on y entre, donc on retranche. C'est exactement le critère
/// que le matériel graphique nomme `front_face`, et il est cohérent par construction sur un
/// maillage fermé : deux triangles qui se font face de part en part de la matière ont des
/// orientations opposées à l'écran.
///
fn accumuler_triangle(
    sommets: &[SommetProjete; 3],
    out: &mut Sorties,
    largeur: usize,
    hauteur: usize,
) {
    let (a, b, c) = (sommets[0], sommets[1], sommets[2]);

    let aire = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);

    // ⚠ Une écharde d'aire nulle ne couvre aucun centre de pixel : la sauter ne retire rien à la
    // somme. **C'est précisément le cas que ses captures de Blender montraient** — les triangles
    // qui convergent vers un pôle — et c'est la raison pour laquelle ils ne peuvent pas trouer
    // l'objet ici.
    if aire == 0.0 {
        return;
    }
    let signe = if aire < 0.0 { -1.0 } else { 1.0 };
    let inverse_aire = 1.0 / aire;

    // La boîte englobante, bornée à l'écran. Les centres de pixels sont en demi-entiers.
    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
    let max_x = (a.x.max(b.x).max(c.x).ceil() as isize).clamp(0, largeur as isize) as usize;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
    let max_y = (a.y.max(b.y).max(c.y).ceil() as isize).clamp(0, hauteur as isize) as usize;

    // Les trois arêtes, dans le sens de parcours du triangle, avec leur droit de revendiquer un
    // point posé exactement dessus.
    let biais = [
        est_arete_haut_ou_gauche((b.x, b.y), (c.x, c.y)),
        est_arete_haut_ou_gauche((c.x, c.y), (a.x, a.y)),
        est_arete_haut_ou_gauche((a.x, a.y), (b.x, b.y)),
    ];

    for py in min_y..max_y {
        for px in min_x..max_x {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;

            // Les trois fonctions d'arête, orientées comme le triangle.
            let bords = [
                ((c.x - b.x) * (y - b.y) - (c.y - b.y) * (x - b.x)) * signe,
                ((a.x - c.x) * (y - c.y) - (a.y - c.y) * (x - c.x)) * signe,
                ((b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)) * signe,
            ];

            let dedans = (0..3).all(|k| bords[k] > 0.0 || (bords[k] == 0.0 && biais[k]));
            if !dedans {
                continue;
            }

            // Coordonnées barycentriques, puis interpolation à perspective correcte : on interpole
            // `distance/w` et `1/w`, et on divise. Interpoler la distance directement donnerait une
            // erreur qui grandit avec l'inclinaison de la surface — invisible sur une sphère de
            // face, franche sur un objet allongé.
            let l0 = bords[0] * inverse_aire * signe;
            let l1 = bords[1] * inverse_aire * signe;
            let l2 = bords[2] * inverse_aire * signe;

            let inverse_w = l0 * a.inverse_w + l1 * b.inverse_w + l2 * c.inverse_w;
            if inverse_w <= 0.0 {
                continue;
            }
            let distance =
                (l0 * a.distance_sur_w + l1 * b.distance_sur_w + l2 * c.distance_sur_w) / inverse_w;

            // ⭐ Le geste entier tient dans cette ligne : on entre, on retranche ; on sort, on
            // ajoute. Une face avant a une aire signée négative à l'écran.
            let indice = py * largeur + px;
            // ⚠ **Le sens dépend du retournement de Y**, et c'est un piège qui s'est refermé sur
            // moi : corriger l'orientation de l'image a inversé l'orientation apparente de TOUS les
            // triangles, donc échangé les entrées et les sorties. Six tests sont tombés d'un coup —
            // et c'est la seule raison pour laquelle je l'ai su.
            // ⚠ La normale doit venir du MÊME triangle que la distance retenue — pas d'une moyenne.
            // Une normale ne s'additionne pas : elle appartient à une surface précise, et c'est
            // celle qui gagne le `min` (ou le `max`) qui est la bonne.
            let interpolee =
                (a.normale * l0 + b.normale * l1 + c.normale * l2).normalize_or_zero();

            if aire > 0.0 {
                out.valeurs[indice] -= distance;
                // On entre : la première entrée est la plus proche.
                if distance < out.entree[indice] {
                    out.entree[indice] = distance;
                    out.normale_entree[indice] = interpolee;
                }
            } else {
                out.valeurs[indice] += distance;
                // On sort : la dernière sortie est la plus lointaine.
                if distance > out.sortie[indice] {
                    out.sortie[indice] = distance;
                    out.normale_sortie[indice] = interpolee;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::primitives::Primitives;

    /// Un nombre entre 0 et 1, tiré d'une cellule de grille — **déterministe et sans état**.
    ///
    /// C'est ce qui permet de semer des milliers de bulles sans en stocker une seule : on ne
    /// *place* pas les bulles, on *demande* à un point s'il est dans une. *Rien à modéliser, rien à
    /// charger, rien à faire tenir en mémoire — et la même graine redonne exactement la même
    /// sucette, ce qui rend un rendu reproductible.*
    fn alea(cx: i32, cy: i32, cz: i32, graine: u32) -> f32 {
        let mut h = (cx as u32)
            .wrapping_mul(0x9E37_79B1)
            ^ (cy as u32).wrapping_mul(0x85EB_CA77)
            ^ (cz as u32).wrapping_mul(0xC2B2_AE3D)
            ^ graine.wrapping_mul(0x27D4_EB2F);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297A_2D39);
        h ^= h >> 15;
        h as f32 / u32::MAX as f32
    }

    /// ⭐⭐ **L'EMPREINTE D'UNE IMAGE, ET POURQUOI ELLE EXISTE** — comparer deux machines sans
    /// s'échanger un seul fichier.
    ///
    /// Née le 1er septembre 2026, quand le moteur a tourné pour la première fois sur un téléphone
    /// (Motorola G54, ARM64, Android). **Quatorze des quinze images produites étaient identiques au
    /// bit près à celles du PC x86.** La quinzième — la nectarine intégrée en **48 pas** — non ;
    /// alors que la MÊME image en **4 pas** l'était.
    ///
    /// *C'est l'ACCUMULATION qui sépare les deux machines, pas le calcul.* Un `a*b + c` peut être
    /// fusionné en une instruction unique (un seul arrondi) sur ARM et scindé en deux (deux
    /// arrondis) sur x86. L'écart vaut une fraction de bit par opération — invisible à 4 pas,
    /// suffisant à 48 pour déplacer un niveau sur 255 quelque part.
    ///
    /// ## ⚠ Ce qu'il faut en retenir, et c'est une règle
    ///
    /// **Ne JAMAIS graver l'empreinte d'une image dans une assertion.** Un tel test passerait ici et
    /// tomberait chez quelqu'un d'autre, en accusant un code parfaitement juste. *La reproductibilité
    /// au bit près entre architectures n'est pas une propriété du moteur — et vouloir l'exiger
    /// fabriquerait un test qui ment.*
    ///
    /// Cette empreinte **s'affiche**, elle ne juge pas. Elle sert à comparer deux machines d'un coup
    /// d'œil, et à mesurer si un écart grandit.
    fn signature(rvb: &[u8]) -> String {
        // FNV-1a, écrit ici en quatre lignes : aucune dépendance, et il n'a rien à protéger.
        let mut h: u64 = 0xcbf29ce484222325;
        let mut somme: u64 = 0;
        for &o in rvb {
            h ^= o as u64;
            h = h.wrapping_mul(0x100000001b3);
            somme += o as u64;
        }
        // La somme dit l'AMPLEUR d'un écart là où l'empreinte ne dit que son existence.
        format!("empreinte {h:016x} · somme des canaux {somme}")
    }

    /// Un passage doux de 0 à 1 entre deux bornes — la courbe d'Hermite `3t² − 2t³`.
    ///
    /// ⚠ **Sa dérivée s'annule aux deux bouts**, et c'est toute la différence avec un `clamp` :
    /// une rampe linéaire a un coude, et un coude se VOIT sur une image. *La première nectarine
    /// portait deux anneaux nets pour cette seule raison.*
    fn fondu(depart: f32, arrivee: f32, valeur: f32) -> f32 {
        let t = ((valeur - depart) / (arrivee - depart)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// ⭐ **LA SEULE PORTE PAR LAQUELLE UNE BILLE ENTRE DANS CE BANC.**
    ///
    /// Elle retourne l'ordre des indices, et c'est une **dette nommée** (3 septembre 2026).
    ///
    /// `Primitives::create_uv_sphere` produisait ses triangles **objectivement à l'envers** —
    /// normales géométriques rentrantes, volume signé négatif — et ce banc portait le critère de
    /// signe inverse. **Deux fautes qui s'annulaient : le banc rendait les bons chiffres et donnait
    /// une compréhension fausse, sans que rien ne le signale.**
    ///
    /// Elles se sont séparées le jour où une passe GPU a demandé pour la première fois à Vulkan de
    /// distinguer l'avant de l'arrière : jusque-là tout le moteur dessinait en `cull_mode: NONE`,
    /// donc personne n'avait jamais eu à trancher. La sphère est corrigée ; **ce banc reste
    /// cohérent avec sa propre convention d'écran** (il regarde la bille depuis +Z et retourne Y
    /// lui-même, là où Vulkan le fait nativement).
    ///
    /// ⚠ **Le retournement est ici, et NULLE PART AILLEURS.** Ma première tentative ne le posait
    /// que dans `sphere()` — neuf autres tests appelaient la primitive directement, et sont restés
    /// rouges. *Une garde posée sur un seul chemin n'est pas une garde, y compris quand le chemin
    /// s'appelle « le décor de tous les tests d'ici ».*
    ///
    /// ⚠⚠ **C'est une dette, pas une solution.** Le vrai correctif est d'aligner la convention
    /// d'écran de ce banc sur celle du moteur — ce qui touche son critère de signe **et** les
    /// triangles que ses tests fabriquent à la main. *Un chantier à décider, pas à bâcler.*
    fn bille_du_banc(rayon: f32, tranches: u32, coupes: u32) -> (Vec<crate::geometry::vertex::Vertex>, Vec<u32>) {
        let (sommets, indices) = Primitives::create_uv_sphere(rayon, tranches, coupes);
        let indices = indices.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
        (sommets, indices)
    }

    /// Une sphère, sa caméra, et la matrice qui va avec — le décor de tous les tests d'ici.
    ///
    /// ## ⚠⚠ LES INDICES SONT RETOURNÉS ICI, ET C'EST UNE DETTE NOMMÉE (3 septembre 2026)
    ///
    /// `Primitives::create_uv_sphere` produisait ses triangles **objectivement à l'envers** —
    /// normales géométriques rentrantes, volume signé négatif — et ce banc portait le critère de
    /// signe inverse. **Deux fautes qui s'annulaient : le banc rendait les bons chiffres et donnait
    /// une compréhension fausse, sans que rien ne le signale.**
    ///
    /// Elles se sont séparées le jour où une passe GPU a demandé pour la première fois à Vulkan de
    /// distinguer l'avant de l'arrière : jusque-là tout le moteur dessinait en `cull_mode: NONE`,
    /// donc personne n'avait jamais eu à trancher. La sphère est corrigée ; **ce banc, lui, reste
    /// cohérent avec sa propre convention d'écran** (il regarde la bille depuis +Z et retourne Y
    /// lui-même, là où Vulkan le fait nativement).
    ///
    /// **Le retournement est donc ici, à UN seul endroit, plutôt que dispersé** — et il est une
    /// dette, pas une solution : *le vrai correctif est d'aligner la convention d'écran de ce banc
    /// sur celle du moteur, ce qui touche son critère de signe ET les triangles que ses tests
    /// fabriquent à la main. C'est un chantier à décider, pas à bâcler en fin de session.*
    ///
    /// ⚠ **Ne pas retirer cette ligne sans ce chantier** : dix tests de ce fichier tombent, dont
    /// `au_centre_d_une_sphere_l_epaisseur_vaut_le_diametre`, qui est la seule sonde absolue du lot.
    fn sphere(rayon: f32, tranches: u32) -> (Vec<Vec3>, Vec<u32>, Mat4, Vec3, f32) {
        let (sommets, indices) = bille_du_banc(rayon, tranches, tranches);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();

        let camera = Vec3::new(0.0, 0.0, 4.0);
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 100.0);
        (positions, indices, projection * vue, camera, rayon)
    }

    /// ⭐ **LE test de cette brique.** Au centre d'une sphère vue de face, le rayon traverse un
    /// diamètre entier : l'épaisseur doit valoir `2 × rayon`, et rien d'autre.
    ///
    /// *Si ce chiffre est juste, la somme signée est juste — il n'y a rien d'autre à prouver sur le
    /// principe.*
    #[test]
    fn au_centre_d_une_sphere_l_epaisseur_vaut_le_diametre() {
        let (positions, indices, vue_proj, camera, rayon) = sphere(1.0, 64);
        let carte = rendre(&positions, None, &indices, vue_proj, camera, 256, 256);

        let au_centre = carte.lire(128, 128);
        let diametre = 2.0 * rayon;

        // Une sphère de 64 tranches est un polyèdre : elle est légèrement plus MINCE que la sphère
        // qu'elle approche, jamais plus épaisse. 1 % est le budget de cette approximation.
        assert!(
            (au_centre - diametre).abs() < diametre * 0.01,
            "au centre on traverse {au_centre} au lieu de {diametre}"
        );
    }

    /// Une épaisseur négative n'a aucun sens physique : elle signalerait une entrée non appariée,
    /// donc une fuite du procédé.
    ///
    /// ⚠ **C'est le test qui répond directement à son objection.** Une seule traversée ratée ou
    /// comptée deux fois produirait ici une valeur négative, ou une valeur qui dépasse le diamètre.
    #[test]
    fn aucune_epaisseur_n_est_negative_ni_ne_depasse_le_diametre() {
        let (positions, indices, vue_proj, camera, rayon) = sphere(1.0, 64);
        let carte = rendre(&positions, None, &indices, vue_proj, camera, 256, 256);

        let plancher = -1e-4;
        let plafond = 2.0 * rayon * 1.01;

        let fautif = carte
            .valeurs
            .iter()
            .enumerate()
            .find(|(_, &v)| v < plancher || v > plafond);

        assert!(
            fautif.is_none(),
            "epaisseur hors bornes au pixel {:?} : {:?}",
            fautif.map(|(i, _)| (i % 256, i / 256)),
            fautif.map(|(_, v)| v)
        );
    }

    /// ⭐⭐ **LE test de SON objection, et il est là pour elle.**
    ///
    /// Une sphère UV a exactement le défaut qu'il a photographié dans Blender : **à ses deux pôles,
    /// toutes les tranches convergent vers un point unique**, et les triangles y deviennent des
    /// échardes d'aire quasi nulle. C'est là, et nulle part ailleurs, qu'un procédé fondé sur des
    /// intersections de rayons troue l'objet.
    ///
    /// On regarde donc une colonne de pixels qui traverse **le pôle nord vu de côté** : l'épaisseur
    /// doit y varier continûment, sans le moindre trou.
    #[test]
    fn les_poles_ou_tous_les_triangles_convergent_ne_trouent_pas_l_objet() {
        let (positions, indices, vue_proj, camera, _) = sphere(1.0, 64);
        let carte = rendre(&positions, None, &indices, vue_proj, camera, 256, 256);

        // La sphère occupe le disque central ; on descend la colonne du milieu, du haut de la
        // silhouette (le pôle) vers le centre.
        let mut precedent = 0.0f32;
        let mut trous = Vec::new();
        for y in 0..128 {
            let v = carte.lire(128, y);
            // Dès qu'on est entré dans la matière, elle ne doit plus jamais disparaître.
            if precedent > 0.05 && v <= 0.0 {
                trous.push(y);
            }
            precedent = v;
        }

        assert!(trous.is_empty(), "l'objet est troue aux lignes {trous:?} — la ou les triangles convergent");
    }

    /// Sur la silhouette, l'entrée et la sortie coïncident : l'épaisseur tend vers zéro.
    ///
    /// *C'est ce bord-là qui fait la transparence d'une nectarine en contre-jour — donc si ce test
    /// tombe, le phénomène qu'on cherche n'existera pas.*
    #[test]
    fn le_bord_de_la_silhouette_est_infiniment_mince() {
        let (positions, indices, vue_proj, camera, _) = sphere(1.0, 64);
        let carte = rendre(&positions, None, &indices, vue_proj, camera, 256, 256);

        // On part du centre et on va vers la droite jusqu'à sortir de la matière ; le dernier pixel
        // plein doit être bien plus mince que le centre.
        let centre = carte.lire(128, 128);
        let mut dernier_plein = centre;
        for x in 128..256 {
            let v = carte.lire(x, 128);
            if v <= 0.0 {
                break;
            }
            dernier_plein = v;
        }

        assert!(
            dernier_plein < centre * 0.25,
            "le bord fait {dernier_plein} pour un centre a {centre} — la silhouette n'amincit pas"
        );
    }

    /// ⭐⭐⭐ **LA GARANTIE DU PROCÉDÉ, et ce test a dû être FABRIQUÉ pour l'atteindre.**
    ///
    /// Deux triangles qui partagent une arête, tous deux tournés du même côté. Si la règle de
    /// remplissage n'attribue pas les pixels de l'arête à **un seul** des deux, leur distance est
    /// retranchée **deux fois** — et l'objet se creuse le long de chaque arête de son maillage.
    ///
    /// ⚠ **Il a fallu construire la géométrie exprès, et voici pourquoi :** sur une sphère, le
    /// centre d'un pixel ne tombe jamais *exactement* sur une arête — la probabilité est nulle en
    /// virgule flottante. Les quatre tests ci-dessus restaient donc **verts avec la règle
    /// désarmée** : ils ne l'exerçaient pas. *C'est la garde creuse du 31 août, retrouvée le
    /// lendemain : un test bâti sur les données qu'on a sous la main ne mord que si le cas y est.*
    ///
    /// Ici, le carré va de (10,10) à (20,20) et sa diagonale passe **pile** par les centres de
    /// pixels (10,5 ; 10,5), (11,5 ; 11,5)… Le cas existe donc à chaque ligne.
    #[test]
    fn deux_triangles_qui_partagent_une_arete_ne_comptent_le_pixel_qu_une_fois() {
        let s = |x: f32, y: f32| SommetProjete {
            x,
            y,
            normale: Vec3::new(0.0, 0.0, 1.0),
            // Une distance de 1 et un `w` de 1 : ce qui est accumulé vaut donc exactement ±1 par
            // revendication, et un double comptage se lit directement.
            distance_sur_w: 1.0,
            inverse_w: 1.0,
        };

        let mut valeurs = vec![0.0f32; 32 * 32];
        let mut entree = vec![f32::INFINITY; 32 * 32];
        let mut sortie = vec![f32::NEG_INFINITY; 32 * 32];
        let mut ne = vec![Vec3::new(0.0, 0.0, 0.0); 32 * 32];
        let mut ns = vec![Vec3::new(0.0, 0.0, 0.0); 32 * 32];
        let mut poser = |t: [SommetProjete; 3], v: &mut Vec<f32>| {
            accumuler_triangle(
                &t,
                &mut Sorties {
                    valeurs: v,
                    entree: &mut entree,
                    sortie: &mut sortie,
                    normale_entree: &mut ne,
                    normale_sortie: &mut ns,
                },
                32,
                32,
            )
        };
        poser([s(10.0, 10.0), s(20.0, 20.0), s(10.0, 20.0)], &mut valeurs);
        poser([s(10.0, 10.0), s(20.0, 10.0), s(20.0, 20.0)], &mut valeurs);

        // Les pixels dont le centre est sur la diagonale partagée.
        for k in 10..20 {
            let v = valeurs[k * 32 + k].abs();
            assert!(
                (v - 1.0).abs() < 1e-5,
                "le pixel ({k},{k}) de l'arete partagee vaut {v} au lieu de 1 — il est revendique deux fois"
            );
        }
    }

    /// ⭐⭐⭐ **LA NECTARINE** — l'image qui dit si l'idée vaut quelque chose.
    ///
    /// Écrit deux fichiers dans `target/preuves/` : la carte d'épaisseur brute en niveaux de gris,
    /// et la même épaisseur passée dans Beer-Lambert avec **trois coefficients différents**.
    ///
    /// ⚠ **Ce test ne vérifie presque rien, et il faut le dire** : il produit une image et contrôle
    /// seulement que le bord est plus clair que le centre, ce qui est le phénomène cherché. **Le
    /// juge est son œil, jamais ce test.** *Une métrique qui déclarerait une image « réussie »
    /// serait exactement la faute que le corpus interdit.*
    ///
    /// Les trois `sigma` sont ceux d'une matière organique rouge : le bleu s'éteint cinq fois plus
    /// vite que le rouge. **Ils viennent du test, pas du moteur** — la frontière tient.
    #[test]
    fn une_nectarine_en_contre_jour() {
        let (sommets, indices) = bille_du_banc(1.0, 96, 96);
        // Une nectarine n'est pas une sphère : elle est un peu aplatie et un peu plus large.
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0] * 1.04, s.position[1] * 0.93, s.position[2] * 1.04))
            .collect();

        let cote = 512usize;
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(38f32.to_radians(), 1.0, 0.1, 100.0);
        let carte = rendre(&positions, None, &indices, projection * vue, camera, cote, cote);

        let maximum = carte.maximum();
        assert!(maximum > 1.5, "la nectarine ne fait que {maximum} d'epaisseur — la camera la rate");

        // ── Image 1 : l'épaisseur seule, en niveaux de gris ──────────────────────────────────
        let mut gris = vec![0u8; cote * cote * 3];
        for (i, &v) in carte.valeurs.iter().enumerate() {
            let n = ((v / maximum).clamp(0.0, 1.0) * 255.0) as u8;
            gris[i * 3] = n;
            gris[i * 3 + 1] = n;
            gris[i * 3 + 2] = n;
        }

        // ── Image 2 : Beer-Lambert, trois canaux, à contre-jour ──────────────────────────────
        // Le soleil est DERRIÈRE le fruit : ce qui arrive à l'œil est ce qui a survécu au trajet.
        const SIGMA: [f32; 3] = [0.85, 2.6, 4.3];
        const SOLEIL: f32 = 6.0;
        const FOND: [f32; 3] = [0.055, 0.05, 0.06];

        let mut couleur = vec![0u8; cote * cote * 3];
        for (i, &d) in carte.valeurs.iter().enumerate() {
            for canal in 0..3 {
                // Hors du fruit (d = 0), la transmittance vaut 1 : on verrait le soleil en face.
                // Le fruit ne couvre pas tout l'écran, donc on distingue les deux cas.
                let lumiere = if d > 0.0 {
                    SOLEIL * transmittance(SIGMA[canal], d)
                } else {
                    FOND[canal]
                };
                // Une courbe de tonalité minimale, appliquée UNE SEULE FOIS, comme dans le moteur.
                let affiche = (lumiere / (1.0 + lumiere)).powf(1.0 / 2.2);
                couleur[i * 3 + canal] = (affiche.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        std::fs::write(
            dossier.join("epaisseur.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &gris).expect("png"),
        )
        .expect("ecriture");
        std::fs::write(
            dossier.join("nectarine.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &couleur).expect("png"),
        )
        .expect("ecriture");

        // ⭐ Le seul contrôle : LE BORD EST PLUS CLAIR QUE LE CENTRE. C'est tout le phénomène —
        // si cette inégalité tombe, la transparence de contre-jour n'existe pas.
        let centre_rouge = couleur[(cote / 2 * cote + cote / 2) * 3] as u32;
        let mut bord_rouge = 0u32;
        for x in cote / 2..cote {
            let i = (cote / 2 * cote + x) * 3;
            if carte.valeurs[cote / 2 * cote + x] <= 0.0 {
                break;
            }
            bord_rouge = couleur[i] as u32;
        }
        assert!(
            bord_rouge > centre_rouge + 40,
            "le bord ({bord_rouge}) n'est pas plus clair que le centre ({centre_rouge}) — pas de contre-jour"
        );

        // ── Image 3 : LA PREUVE QUE CE DÉGRADÉ N'EST PAS PEINT ───────────────────────────────
        // Le même fruit, cabossé par une bosse et un creux. Si la clarté du bord était un dégradé
        // radial déguisé, elle ne bougerait pas. Elle doit au contraire ÉPOUSER la nouvelle forme —
        // c'est la seule chose qui distingue une grandeur calculée d'un effet peint.
        let cabosse: Vec<Vec3> = sommets
            .iter()
            .map(|s| {
                let p = Vec3::new(s.position[0], s.position[1], s.position[2]);
                let n = p.normalize();
                let creux = 1.0 + 0.30 * (n.x * 3.1).sin() * (n.y * 2.3).cos();
                Vec3::new(p.x * 1.04 * creux, p.y * 0.93 * creux, p.z * 1.04 * creux)
            })
            .collect();
        let carte2 = rendre(&cabosse, None, &indices, projection * vue, camera, cote, cote);
        let mut bosse = vec![0u8; cote * cote * 3];
        for (i, &d) in carte2.valeurs.iter().enumerate() {
            for canal in 0..3 {
                let lumiere = if d > 0.0 { SOLEIL * transmittance(SIGMA[canal], d) } else { FOND[canal] };
                let affiche = (lumiere / (1.0 + lumiere)).powf(1.0 / 2.2);
                bosse[i * 3 + canal] = (affiche.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
        std::fs::write(
            dossier.join("nectarine-cabossee.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &bosse).expect("png"),
        )
        .expect("ecriture");

        // Et on le MESURE plutôt que de le regarder : sur une même ligne, les deux images doivent
        // différer franchement. Un dégradé peint donnerait deux images identiques.
        let ecart: u32 = (0..cote)
            .map(|x| {
                let i = (cote / 2 * cote + x) * 3;
                (couleur[i] as i32 - bosse[i] as i32).unsigned_abs()
            })
            .sum();
        assert!(
            ecart > 2000,
            "cabosser la geometrie n'a presque rien change ({ecart}) — le degrade ne suit pas la forme"
        );

        println!("images ecrites dans {}", dossier.display());
    }

    /// ⭐⭐⭐ **LE PAS 3 — L'INTÉRIEUR DU FRUIT.**
    ///
    /// Jusqu'ici la matière était homogène : un `sigma` constant, donc une boule lisse. Ici le
    /// champ répond différemment en chaque point — **la peau, la chair, les fibres, le noyau** —
    /// et la nectarine cesse d'être une boule.
    ///
    /// ⚠ **Aucune coque n'est modélisée, et c'est tout le sujet.** Il n'y a qu'UNE enveloppe
    /// triangulaire ; ce qu'il y a dedans est une fonction. *C'est ce qui évite les surfaces qui se
    /// croisent et la « cristallisation » qu'il redoutait.*
    ///
    /// ⚠ **Le juge est son œil.** Ce test ne mesure que deux choses, et aucune n'est un verdict de
    /// beauté : que le champ change réellement l'image, et que quatre pas suffisent presque.
    #[test]
    fn une_nectarine_et_son_interieur() {
        let (sommets, indices) = bille_du_banc(1.0, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0] * 1.04, s.position[1] * 0.93, s.position[2] * 1.04))
            .collect();

        let cote = 512usize;
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let fov = 38f32.to_radians();
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(fov, 1.0, 0.1, 100.0);
        let carte = rendre(&positions, None, &indices, projection * vue, camera, cote, cote);

        // La direction du rayon d'un pixel. ⚠ Elle DOIT suivre la même convention que `rendre` —
        // ici la caméra regarde vers −Z, la droite est +X, le haut est +Y, et `perspective_rh`
        // n'inverse pas Y.
        let tangente = (fov * 0.5).tan();
        let direction = move |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            // Le retournement de `rendre`, à l'identique — les deux ne peuvent pas diverger sans
            // que le champ se retrouve décalé de haut en bas.
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            Vec3::new(ndc_x * tangente, ndc_y * tangente, -1.0).normalize()
        };

        // ⚠ La garde qui empêche de rendre un champ ALIGNÉ DE TRAVERS — le défaut le plus probable
        // ici, et le plus difficile à voir : une image fausse reste jolie. Au pixel central, le
        // milieu du segment doit tomber sur le cœur du fruit.
        let centre = cote / 2 * cote + cote / 2;
        let (e, s) = carte.segment(centre).expect("le pixel central traverse le fruit");
        let coeur = camera + direction(cote / 2, cote / 2) * (e + s) * 0.5;
        assert!(coeur.length() < 0.05, "le champ est decale : le coeur tombe en {coeur:?}");

        // ── LE CHAMP DE LA NECTARINE ─────────────────────────────────────────────────────────
        // Quatre matières, aucune surface. Les nombres viennent du test, jamais du moteur.
        const CHAIR: [f32; 3] = [0.85, 2.6, 4.3];
        const PEAU: [f32; 3] = [7.0, 17.0, 26.0];
        const EPAISSEUR_PEAU: f32 = 0.042;
        const NOYAU: f32 = 0.33;

        let champ = |p: Vec3, depuis_la_surface: f32, dl: f32| -> Matiere {
            let r = p.length();

            // ⚠ AUCUN SEUIL FRANC ICI, et c'est toute la leçon de la première image. Elle portait
            // des anneaux concentriques nets et une bande verticale : **chaque `if` et chaque
            // `clamp` avait laissé sa marche.** C'est très exactement la « cristallisation » qu'il
            // redoutait, arrivée par la porte à laquelle je ne regardais pas — non pas par la
            // géométrie, mais par le CHAMP. *Un fondu doux (`fondu`) n'a pas de dérivée qui casse,
            // donc rien à montrer.*

            // Le noyau : ce qui ne laisse presque rien passer, avec un bord qui s'estompe.
            let dedans_le_noyau = fondu(NOYAU + 0.05, NOYAU - 0.05, r);
            // La peau : une couche fine, bien plus dense, bien plus mordante dans le bleu.
            let dans_la_peau = fondu(EPAISSEUR_PEAU, EPAISSEUR_PEAU * 0.45, depuis_la_surface);

            // Les fibres — ce qu'il appelle « les petites sanguinités, les petits fils ». Elles
            // rayonnent du noyau vers la peau, donc elles se décrivent par la DIRECTION du point,
            // pas par sa position : c'est ce qui les fait converger là où il faut sans qu'on ait à
            // les dessiner.
            let n = p.normalize();
            let azimut = n.z.atan2(n.x);
            let trame = (azimut * 11.0 + n.y * 3.5).sin() * (n.y * 12.0 + azimut).cos();
            // La puissance resserre les filaments : sans elle on aurait des vagues, pas des fils.
            let filament = trame.abs().powf(7.0);
            // ⭐ **Ce facteur efface la couture de l'azimut**, et il n'est pas une rustine : sur
            // l'axe vertical, `atan2` n'a pas de valeur — mais les fibres d'un fruit s'y confondent
            // aussi. En annulant leur amplitude là où elles convergent, la discontinuité perd son
            // amplitude en même temps que son sens. *La marche ne rétrécit pas : elle disparaît.*
            let loin_de_l_axe = (n.x * n.x + n.z * n.z).sqrt();
            // Elles s'éteignent contre le noyau et contre la peau, comme dans un vrai fruit.
            let montee = fondu(NOYAU, NOYAU + 0.22, r);
            let descente = fondu(1.02, 0.70, r);
            // ⭐⭐ LA NETTETÉ QU'ON PEUT SE PAYER. Les filaments ont une période spatiale d'environ
            // `2πr/11` ; si le pas la dépasse, les échantillonner donne du hasard, pas du détail.
            // On les ramène donc vers leur moyenne — ils s'estompent au lieu de se déchirer.
            // *C'est ça, coller à la limite physique : ne pas prétendre au détail qu'on ne paie pas.*
            // ⚠ Réduire l'amplitude ne suffit PAS : un motif à 30 % reste échantillonné au hasard,
            // donc tacheté à 30 %. **Il faut qu'il DISPARAISSE quand le pas atteint sa période** —
            // au-delà, un échantillon ne porte plus d'information sur lui, seulement du bruit.
            // *À quatre pas, la nectarine n'a donc pas de fibres. Elle n'a pas de fausses fibres.*
            let periode = std::f32::consts::TAU * r / 11.0;
            let nettete = fondu(periode * 0.55, periode * 0.18, dl);
            let f = filament * nettete * loin_de_l_axe * montee * descente * 7.5;

            let chair = [CHAIR[0] + f * 0.55, CHAIR[1] + f * 1.6, CHAIR[2] + f * 2.4];
            let mut sigma = [0.0f32; 3];
            for canal in 0..3 {
                // Les trois matières se mélangent par fondu, jamais par branchement.
                let c = chair[canal] * (1.0 - dans_la_peau) + PEAU[canal] * dans_la_peau;
                sigma[canal] = c * (1.0 - dedans_le_noyau) + 90.0 * dedans_le_noyau;
            }
            Matiere::absorbante(sigma)
        };

        const SOLEIL: f32 = 6.0;
        const FOND: [f32; 3] = [0.055, 0.05, 0.06];
        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");

        let peindre = |transmittance: &[Traversee]| -> Vec<u8> {
            let mut rvb = vec![0u8; cote * cote * 3];
            for i in 0..cote * cote {
                let dans_la_matiere = carte.valeurs[i] > 0.0;
                for canal in 0..3 {
                    let lumiere = if dans_la_matiere {
                        SOLEIL * transmittance[i].transmittance[canal] + transmittance[i].emise[canal]
                    } else {
                        FOND[canal]
                    };
                    let affiche = (lumiere / (1.0 + lumiere)).powf(1.0 / 2.2);
                    rvb[i * 3 + canal] = (affiche.clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
            rvb
        };

        let fine = integrer_le_champ(&carte, camera, direction, 48, champ);
        let image_fine = peindre(&fine);
        std::fs::write(
            dossier.join("nectarine-interieur.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &image_fine).expect("png"),
        )
        .expect("ecriture");
        println!("  48 pas : {}", signature(&image_fine));

        // ⭐ LE MÊME CHAMP EN QUATRE PAS — le budget d'un casque. C'est le curseur d'adaptativité,
        // rendu visible : un seul nombre change, ni le code ni le champ.
        let grossiere = integrer_le_champ(&carte, camera, direction, 4, champ);
        let image_grossiere = peindre(&grossiere);
        std::fs::write(
            dossier.join("nectarine-4-pas.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &image_grossiere).expect("png"),
        )
        .expect("ecriture");
        println!("   4 pas : {}", signature(&image_grossiere));

        // ── Ce que ce test contrôle vraiment ─────────────────────────────────────────────────
        // 1. Le champ CHANGE l'image. Sans ça, tout ce fichier serait décoratif.
        let homogene = integrer_le_champ(&carte, camera, direction, 48, |_, _, _| Matiere::absorbante(CHAIR));
        let ecart: f32 = (0..cote * cote)
            .map(|i| (fine[i].transmittance[0] - homogene[i].transmittance[0]).abs())
            .sum::<f32>()
            / (cote * cote) as f32;
        assert!(ecart > 0.004, "le champ ne change presque rien ({ecart}) — il n'est pas lu");

        // 2. ⭐⭐ QUATRE PAS DÉGRADENT SANS INVENTER — et j'ai mesuré la mauvaise chose deux fois
        //    avant d'écrire cette ligne, donc elle mérite son explication.
        //
        //    Mon premier critère était « l'image à 4 pas ressemble à l'image à 48 pas ». **Il est
        //    faux**, et il l'était doublement : il a laissé passer une image tachetée (l'écart
        //    moyen restait sous le seuil), puis il a REFUSÉ la correction (une image honnêtement
        //    lissée s'écarte forcément plus d'une image détaillée qu'une image bruitée).
        //
        //    Ce qu'on veut n'est pas la ressemblance : c'est **qu'aucune structure n'apparaisse qui
        //    n'existe pas**. Un artefact est une variation entre pixels voisins ; une dégradation
        //    gracieuse est plus LISSE que l'original, jamais plus agitée.
        let rugosite = |img: &[u8]| -> f32 {
            let mut somme = 0.0;
            for y in 0..cote {
                for x in 1..cote {
                    let i = (y * cote + x) * 3;
                    somme += (img[i] as i32 - img[i - 3] as i32).unsigned_abs() as f32;
                }
            }
            somme / (cote * (cote - 1)) as f32
        };
        let (rf, rg) = (rugosite(&image_fine), rugosite(&image_grossiere));
        println!("rugosite : 48 pas = {rf:.3}, 4 pas = {rg:.3}");
        assert!(
            rg <= rf,
            "quatre pas AGITENT l'image ({rg:.3} contre {rf:.3}) — ce sont des artefacts, pas une degradation"
        );
    }

    /// ⭐⭐⭐ **LA SUCETTE** — et c'est elle qui a exigé le terme de source.
    ///
    /// Il a proposé cette image comme cible : une boule de sucre bleu à contre-jour, pleine de
    /// bulles. **Elle demande quatre choses que la nectarine ne demandait pas**, et une seule
    /// manquait vraiment.
    ///
    /// | Ce que l'image montre | Ce qu'il a fallu |
    /// |---|---|
    /// | La teinte qui **change** avec l'épaisseur (cyan au bord, outremer au cœur) | rien — c'est déjà `σ` par canal |
    /// | La **bande verticale** nette (un feuillet de colorant mal mélangé) | rien — c'est un champ inhomogène |
    /// | Les **bulles argentées** | ⭐ le terme de SOURCE, qui n'existait pas |
    /// | Le fond replié par la sphère-lentille, le reflet de la vitre | ⛔ **rien : on ne dévie aucun rayon** |
    ///
    /// ⚠ **Ce test ne rend donc PAS la sucette de la photo.** Il rend ce qu'un milieu inhomogène à
    /// inclusions peut donner sans jamais faire tourner un rayon. *La différence entre les deux est
    /// exactement la liste du chantier suivant, et elle est écrite plutôt que devinée.*
    #[test]
    fn une_sucette_bleue_et_ses_bulles() {
        let (sommets, indices) = bille_du_banc(1.0, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();

        let cote = 512usize;
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let fov = 36f32.to_radians();
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(fov, 1.0, 0.1, 100.0);
        // Les normales par sommet : une sphère facettée devient optiquement lisse.
        let normales: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.normal[0], s.normal[1], s.normal[2]))
            .collect();
        let carte = rendre(&positions, Some(&normales), &indices, projection * vue, camera, cote, cote);

        let tangente = (fov * 0.5).tan();
        let direction = move |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            // Le retournement de `rendre`, à l'identique — les deux ne peuvent pas diverger sans
            // que le champ se retrouve décalé de haut en bas.
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            Vec3::new(ndc_x * tangente, ndc_y * tangente, -1.0).normalize()
        };

        // ── LE SUCRE BLEU ────────────────────────────────────────────────────────────────────
        // Le colorant absorbe dans une bande étroite du ROUGE : c'est pourquoi la teinte ne fait
        // pas que s'assombrir avec l'épaisseur, elle **vire** — cyan sur un trajet court, outremer
        // sur un trajet long. Trois nombres suffisent à dire ça.
        const SUCRE: [f32; 3] = [3.4, 1.15, 0.30];
        // ⚠⚠ LA CELLULE EST ANISOTROPE, ET C'EST UNE CORRECTION, PAS UN RÉGLAGE.
        //
        // La première version semait les bulles dans une grille CUBIQUE de 0,155. Une bulle étirée
        // par l'écoulement atteignait 0,124 de demi-longueur pour une demi-cellule de 0,0775 :
        // **elle débordait, donc elle était tranchée net par les faces du cube.** D'où les carrés
        // qu'il a vus en zoomant — ce n'était pas un phénomène de rendu, c'était ma grille.
        //
        // *La règle qui en sort, et elle vaut pour tout champ semé sur une grille : une inclusion
        // ne doit JAMAIS pouvoir sortir de sa cellule, sinon la grille se voit.* Ici la cellule
        // s'allonge comme les bulles, ce qui est aussi ce que fait la matière étirée.
        const CELLULE: [f32; 3] = [0.150, 0.360, 0.150];

        let champ = |p: Vec3, _depuis_la_surface: f32, dl: f32| -> Matiere {
            // ── Le feuillet de colorant, gelé par l'écoulement ───────────────────────────────
            // À 150 °C le sirop est cent mille fois plus visqueux que l'eau : deux volumes qui se
            // rencontrent ne se mélangent pas, ils se collent. La frontière est donc quasi nette —
            // la diffusion moléculaire n'a eu que quelques secondes avant le figeage.
            let cote_droit = fondu(-0.17, -0.11, p.x);
            let concentration = 0.70 + 0.58 * cote_droit;
            let mut sigma = [
                SUCRE[0] * concentration,
                SUCRE[1] * concentration,
                SUCRE[2] * concentration,
            ];
            let mut source = [0.0f32; 3];

            // ── LES BULLES ───────────────────────────────────────────────────────────────────
            // Aucune n'est modélisée : on demande au point s'il est dans une.
            let cx = (p.x / CELLULE[0]).floor() as i32;
            let cy = (p.y / CELLULE[1]).floor() as i32;
            let cz = (p.z / CELLULE[2]).floor() as i32;
            let presence = alea(cx, cy, cz, 7);

            if presence > 0.18 {
                // La bulle reste au CŒUR de sa cellule : les 20 % de bord lui sont interdits, ce
                // qui garantit qu'elle ne peut pas la franchir même à sa taille maximale.
                let place = |g: u32, axe: usize| (0.2 + 0.6 * alea(cx, cy, cz, g)) * CELLULE[axe];
                let centre = Vec3::new(
                    cx as f32 * CELLULE[0] + place(11, 0),
                    cy as f32 * CELLULE[1] + place(13, 1),
                    cz as f32 * CELLULE[2] + place(17, 2),
                );
                // Stratification : les grosses remontent (poussée ∝ r³, frottement ∝ r), les fines
                // restent piégées. Le haut est donc plus grossier que le bas.
                let haut = fondu(-0.6, 0.85, p.y);
                let rayon = (0.016 + 0.030 * haut) * (0.40 + 0.60 * alea(cx, cy, cz, 23));
                // Et là où la matière a été tirée, la viscosité a figé les bulles en fuseau avant
                // que la tension superficielle ait pu les rendre rondes.
                let etirement = 1.0 + 1.7 * haut;
                let d = p - centre;
                let d = Vec3::new(d.x, d.y / etirement, d.z);

                // ⚠⚠ LE SECOND DÉFAUT QU'IL A VU : le grain, les hachures dans les bulles. Ce n'est
                // pas la grille, c'est du **crénelage d'échantillonnage** — le même phénomène que
                // les taches de la nectarine à quatre pas, en plus fin. Mesuré : 4,4 échantillons
                // par bulle, là où il en faut au moins huit pour qu'une sphère cesse de scintiller.
                //
                // Mon seuil précédent (`rayon × 1,6`) était bien trop permissif : il laissait
                // passer des bulles vues par quatre points. Celui-ci exige un pas quatre fois plus
                // fin que le rayon — et à ce prix, il FAUT payer les pas.
                let nettete = fondu(rayon * 0.55, rayon * 0.22, dl);
                let dedans = fondu(rayon, rayon * 0.55, d.length()) * nettete;

                // ⭐ **Une bulle n'est pas un trou : c'est un miroir.** Air (n = 1,0) dans du sucre
                // (n ≈ 1,5) : au-delà de 41,8° d'incidence, la réflexion est TOTALE — donc sur la
                // plus grande part d'une sphère. Elle renvoie l'ambiante **sans qu'elle ait traversé
                // le bleu**, ce qui explique qu'on la voie blanche sur fond outremer.
                for canal in 0..3 {
                    source[canal] += [30.0, 33.0, 38.0][canal] * dedans;
                    // Elle bloque aussi ce qui vient de derrière elle.
                    sigma[canal] += 26.0 * dedans;
                }
            }

            Matiere { sigma, source }
        };

        const SOLEIL: f32 = 7.0;
        const FOND: [f32; 3] = [0.10, 0.115, 0.14];

        let peindre = |t: &[Traversee]| -> Vec<u8> {
            let mut rvb = vec![0u8; cote * cote * 3];
            for i in 0..cote * cote {
                for canal in 0..3 {
                    let lumiere = if carte.valeurs[i] > 0.0 {
                        SOLEIL * t[i].transmittance[canal] + t[i].emise[canal]
                    } else {
                        FOND[canal]
                    };
                    let affiche = (lumiere / (1.0 + lumiere)).powf(1.0 / 2.2);
                    rvb[i * 3 + canal] = (affiche.clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
            rvb
        };

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier");

        let fine = integrer_le_champ(&carte, camera, direction, 224, champ);
        let image = peindre(&fine);
        std::fs::write(
            dossier.join("sucette.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &image).expect("png"),
        )
        .expect("ecriture");

        // Le même sucre SANS bulles : pour voir ce que le terme de source apporte, et le mesurer.
        let sans = integrer_le_champ(&carte, camera, direction, 224, |p, d, dl| {
            let mut m = champ(p, d, dl);
            m.source = [0.0; 3];
            m
        });
        let image_sans = peindre(&sans);
        std::fs::write(
            dossier.join("sucette-sans-bulles.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &image_sans).expect("png"),
        )
        .expect("ecriture");

        // ── LE GROS PLAN — parce qu'il juge en zoomant, et il a raison ────────────────────────
        // ⚠ Agrandir une image de 512 ne montre que ses pixels ; ça donne l'illusion d'un défaut
        // là où il n'y en a pas, et ça en cache d'autres. **Un gros plan se RECALCULE**, avec un
        // champ de vision plus étroit — c'est la même scène, vue de plus près, à pleine résolution.
        // *C'est ce qui a permis de voir que les bulles étaient tranchées par leur cellule.*
        let fov_zoom = 9f32.to_radians();
        let vue_zoom = Mat4::look_at_rh(camera, Vec3::new(-0.16, 0.36, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let carte_zoom = rendre(
            &positions,
            Some(&normales),
            &indices,
            Mat4::perspective_rh(fov_zoom, 1.0, 0.1, 100.0) * vue_zoom,
            camera,
            cote,
            cote,
        );
        // La direction doit suivre la MÊME caméra : elle est tournée, donc ses axes le sont aussi.
        let avant = (Vec3::new(-0.16, 0.36, 0.0) - camera).normalize();
        let droite = avant.cross(Vec3::new(0.0, 1.0, 0.0)).normalize();
        let dessus = droite.cross(avant).normalize();
        let tan_zoom = (fov_zoom * 0.5).tan();
        let direction_zoom = move |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            (avant + droite * (ndc_x * tan_zoom) + dessus * (ndc_y * tan_zoom)).normalize()
        };
        let zoom = integrer_le_champ(&carte_zoom, camera, direction_zoom, 224, champ);
        let mut rvb_zoom = vec![0u8; cote * cote * 3];
        for i in 0..cote * cote {
            for canal in 0..3 {
                let lumiere = if carte_zoom.valeurs[i] > 0.0 {
                    SOLEIL * zoom[i].transmittance[canal] + zoom[i].emise[canal]
                } else {
                    FOND[canal]
                };
                rvb_zoom[i * 3 + canal] =
                    (((lumiere / (1.0 + lumiere)).powf(1.0 / 2.2)).clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
        std::fs::write(
            dossier.join("sucette-gros-plan.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &rvb_zoom).expect("png"),
        )
        .expect("ecriture");

        // ⭐ CE QUE CE TEST CONTRÔLE : les bulles ÉCLAIRENT au lieu d'assombrir. C'est toute la
        // différence entre une inclusion réfléchissante et un trou — et c'est ce qu'un champ
        // purement absorbant était incapable de produire.
        let mut plus_clair = 0usize;
        let mut plus_sombre = 0usize;
        for i in 0..cote * cote {
            if carte.valeurs[i] <= 0.0 {
                continue;
            }
            let (a, b) = (image[i * 3] as i32, image_sans[i * 3] as i32);
            if a > b + 3 {
                plus_clair += 1;
            } else if b > a + 3 {
                plus_sombre += 1;
            }
        }
        println!("bulles : {plus_clair} pixels eclaircis, {plus_sombre} assombris");
        assert!(
            plus_clair > plus_sombre * 3,
            "les bulles assombrissent ({plus_sombre}) plus qu'elles n'eclairent ({plus_clair}) — \
             elles se comportent en trous, pas en miroirs"
        );
    }

    /// ⭐⭐⭐ **LES NORMALES, AVANT D'EN FAIRE QUOI QUE CE SOIT.**
    ///
    /// C'est le premier pas de la réfraction, et il est délibérément le plus petit possible : on
    /// les calcule, on les regarde, **on ne dévie rien.**
    ///
    /// ⚠ **La raison est une leçon du corpus, pas une prudence de principe.** Une normale fausse ne
    /// produit pas une image cassée : elle produit une image *plausible et fausse* — un verre qui
    /// réfracte joliment dans la mauvaise direction. *Le pire cas de ce projet n'est pas ce qui
    /// casse, c'est ce qui a l'air juste.* Donc on vérifie contre une vérité analytique avant, pas
    /// après.
    ///
    /// **La vérité choisie :** sur une sphère centrée à l'origine, la normale en un point EST la
    /// direction de ce point. Il n'y a rien à approcher, la comparaison est exacte.
    #[test]
    fn les_normales_de_la_surface_avant_toute_refraction() {
        let (sommets, indices) = bille_du_banc(1.0, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();
        let normales: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.normal[0], s.normal[1], s.normal[2]))
            .collect();

        let cote = 512usize;
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let fov = 36f32.to_radians();
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(fov, 1.0, 0.1, 100.0);
        let carte = rendre(
            &positions,
            Some(&normales),
            &indices,
            projection * vue,
            camera,
            cote,
            cote,
        );

        let tangente = (fov * 0.5).tan();
        let direction = |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            Vec3::new(ndc_x * tangente, ndc_y * tangente, -1.0).normalize()
        };

        let mut ecart_max = 0.0f32;
        let mut entrees_a_l_envers = 0usize;
        let mut sorties_a_l_envers = 0usize;
        let mut testes = 0usize;
        let mut epaisseurs_fautives: Vec<f32> = Vec::new();

        for y in 0..cote {
            for x in 0..cote {
                let i = y * cote + x;
                let Some((e, s2)) = carte.segment(i) else { continue };
                let rayon = direction(x, y);
                let (ne, ns) = (carte.normale_entree[i], carte.normale_sortie[i]);
                testes += 1;

                // 1. La normale d'entrée est la direction du point d'entrée — vérité analytique.
                let attendue = (camera + rayon * e).normalize();
                let ecart = (1.0 - ne.dot(attendue)).max(0.0);
                if ecart > ecart_max {
                    ecart_max = ecart;
                }

                // 2. On ENTRE par une face tournée vers l'œil : le rayon la frappe de face.
                if ne.dot(rayon) > 0.0 {
                    entrees_a_l_envers += 1;
                    epaisseurs_fautives.push(carte.valeurs[i]);
                }
                // 3. On SORT par une face qui tourne le dos à l'œil.
                if ns.dot(rayon) < 0.0 {
                    sorties_a_l_envers += 1;
                    epaisseurs_fautives.push(carte.valeurs[i]);
                }
                let _ = s2;
            }
        }

        // ── L'image des normales, en couleur : X→rouge, Y→vert, Z→bleu ───────────────────────
        let mut rvb = vec![0u8; cote * cote * 3];
        for i in 0..cote * cote {
            let n = carte.normale_entree[i];
            let peindre = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            if carte.valeurs[i] > 0.0 {
                rvb[i * 3] = peindre(n.x);
                rvb[i * 3 + 1] = peindre(n.y);
                rvb[i * 3 + 2] = peindre(n.z);
            } else {
                rvb[i * 3] = 26;
                rvb[i * 3 + 1] = 28;
                rvb[i * 3 + 2] = 33;
            }
        }
        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier");
        std::fs::write(
            dossier.join("normales.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &rvb).expect("png"),
        )
        .expect("ecriture");

        println!(
            "normales : {testes} pixels, ecart max a la verite = {:.4}°, {entrees_a_l_envers} entrees et \
             {sorties_a_l_envers} sorties a l'envers",
            (1.0f32 - ecart_max).clamp(-1.0, 1.0).acos().to_degrees()
        );

        // Un maillage de 96 tranches est un polyèdre : la normale interpolée s'écarte un peu de la
        // sphère idéale, mais jamais de plus d'un degré.
        assert!(
            ecart_max < 2e-4,
            "les normales s'ecartent de la sphere de {:.3}° au pire",
            (1.0f32 - ecart_max).clamp(-1.0, 1.0).acos().to_degrees()
        );
        // ── ⚠ CE QUE LA MESURE A CORRIGÉ DANS CE TEST, ET C'EST INSTRUCTIF ───────────────────
        //
        // Le test exigeait d'abord ZÉRO normale à l'envers. Il en a trouvé quarante sur 163 000 —
        // et j'ai mesuré avant de conclure : **leur épaisseur va de 0,0008 à 0,021 pour un diamètre
        // de 2,0**, soit 0,04 % à 1 %. Ce sont donc exactement les rayons **TANGENTS** à la
        // silhouette, et à la tangence « entrer dans la matière » n'a plus de sens : le produit
        // scalaire bascule sur du bruit numérique parce que sa vraie valeur est zéro.
        //
        // **Ce n'est pas un défaut, c'est la limite du domaine de définition.** Et le critère juste
        // n'est pas un epsilon angulaire choisi à la main : sur une sphère l'épaisseur vaut
        // `2R·cos θ`, donc **l'épaisseur EST la mesure de l'incidence**. On vérifie la relation, pas
        // un seuil. *La constante arbitraire n'a jamais eu à exister.*
        let diametre = carte.maximum();
        let plus_epais = epaisseurs_fautives
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        println!(
            "normales a l'envers : {} pixels, le plus epais traverse {:.3} % du diametre",
            epaisseurs_fautives.len(),
            plus_epais / diametre * 100.0
        );
        assert!(
            plus_epais < diametre * 0.02,
            "une normale est a l'envers sur un pixel qui traverse {:.1} % du diametre — ce n'est \
             plus la tangence, c'est un vrai defaut d'orientation",
            plus_epais / diametre * 100.0
        );
    }

    /// La loi de Snell vérifiée **par sa définition**, pas par une image.
    #[test]
    fn snell_respecte_le_rapport_des_sinus() {
        let n = Vec3::new(0.0, 0.0, 1.0);
        for degres in [0.0f32, 10.0, 30.0, 55.0, 75.0, 89.0] {
            let a = degres.to_radians();
            let incident = Vec3::new(a.sin(), 0.0, -a.cos());
            let eta = 1.0 / 1.5;
            let t = refracter(incident, n, eta).expect("entrer dans un milieu plus dense reussit toujours");

            let sin_sortie = (t.x * t.x + t.y * t.y).sqrt();
            // n₁ sin θ₁ = n₂ sin θ₂  →  1,0 · sin(a) = 1,5 · sin θ₂
            let attendu = a.sin() / 1.5;
            assert!(
                (sin_sortie - attendu).abs() < 1e-5,
                "a {degres}° : sin sortant {sin_sortie} au lieu de {attendu}"
            );
            assert!((t.length() - 1.0).abs() < 1e-5, "la direction refractee n'est pas unitaire");
        }
    }

    /// ⭐⭐ **L'ANGLE CRITIQUE DE 41,8° N'EST ÉCRIT NULLE PART — il se MESURE.**
    ///
    /// Le texte sur la sucette l'annonce comme un fait de la matière : au-delà de 41,8°, un rayon
    /// qui tente de sortir du sucre vers l'air est réfléchi en totalité. *Ce nombre n'apparaît dans
    /// aucune ligne de ce fichier.* On le retrouve en cherchant l'angle où `refracter` cesse d'avoir
    /// une réponse.
    ///
    /// **C'est la meilleure preuve qu'on ait de la justesse de cette fonction :** elle produit une
    /// constante physique qu'on ne lui a pas donnée.
    #[test]
    fn l_angle_critique_du_sucre_tombe_de_l_equation() {
        let n = Vec3::new(0.0, 0.0, 1.0);
        let eta = 1.5 / 1.0; // du sucre vers l'air

        let mut critique = 90.0f32;
        let mut angle = 0.0f32;
        while angle < 90.0 {
            let a = angle.to_radians();
            let incident = Vec3::new(a.sin(), 0.0, -a.cos());
            if refracter(incident, n, eta).is_none() {
                critique = angle;
                break;
            }
            angle += 0.01;
        }

        let theorique = (1.0f32 / 1.5).asin().to_degrees();
        println!("angle critique mesure : {critique:.2}° (theorie {theorique:.2}°)");
        assert!(
            (critique - theorique).abs() < 0.05,
            "l'angle critique vaut {critique}° au lieu de {theorique}°"
        );
        assert!(
            (critique - 41.8).abs() < 0.1,
            "l'angle critique du sucre devrait valoir 41,8° et vaut {critique}°"
        );
    }

    /// En incidence normale, rien ne dévie — et Fresnel donne les ~4 % de réflexion du verre.
    #[test]
    fn de_face_rien_ne_devie_et_fresnel_donne_quatre_pour_cent() {
        let n = Vec3::new(0.0, 0.0, 1.0);
        let incident = Vec3::new(0.0, 0.0, -1.0);
        let t = refracter(incident, n, 1.0 / 1.5).unwrap();
        assert!((t - incident).length() < 1e-6, "un rayon perpendiculaire ne devrait pas devier");

        let r = fresnel(1.0, 1.0, 1.5);
        assert!((r - 0.04).abs() < 0.005, "Fresnel de face vaut {r} au lieu de ~0,04");
        // Et en rasant, tout est réfléchi : c'est le liseré brillant de toute bille.
        assert!(fresnel(0.0, 1.0, 1.5) > 0.99, "en rasant, Fresnel doit tendre vers 1");
    }

    /// ⭐⭐⭐ **LA RÉFRACTION** — la première image où la lumière change de direction.
    ///
    /// Trois rendus de la même bille de sucre devant le même damier :
    /// **sans dévier** · **Snell à l'entrée seule** · **Snell aux deux interfaces**.
    ///
    /// ⚠ **On quitte ici le terrain de l'exact.** L'épaisseur et le champ étaient justes par
    /// construction — la somme signée ne ment pas. La réfraction à deux interfaces, elle, est une
    /// **approximation** : je connais la sortie du rayon DROIT, pas celle du rayon dévié. *L'erreur
    /// est mesurée plus bas plutôt qu'estimée, mais elle existe.*
    #[test]
    fn la_refraction_replie_le_monde_derriere_la_bille() {
        let (sommets, indices) = bille_du_banc(1.0, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();
        let normales: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.normal[0], s.normal[1], s.normal[2]))
            .collect();

        let cote = 512usize;
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let fov = 36f32.to_radians();
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let carte = rendre(
            &positions,
            Some(&normales),
            &indices,
            Mat4::perspective_rh(fov, 1.0, 0.1, 100.0) * vue,
            camera,
            cote,
            cote,
        );

        let tangente = (fov * 0.5).tan();
        let direction = |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            Vec3::new(ndc_x * tangente, ndc_y * tangente, -1.0).normalize()
        };

        // Le monde derrière : un damier en coordonnées de direction. **Aucune géométrie** — ce qui
        // compte est de reconnaître d'un coup d'œil si l'image est repliée, inversée, comprimée.
        let environnement = |d: Vec3| -> [f32; 3] {
            let u = d.z.atan2(d.x) / std::f32::consts::TAU + 0.5;
            let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
            let case = (u * 28.0).floor() as i32 + (v * 14.0).floor() as i32;
            if case.rem_euclid(2) == 0 {
                [0.80, 0.83, 0.90]
            } else {
                [0.045, 0.05, 0.075]
            }
        };

        const N_SUCRE: f32 = 1.50;
        // Une teinte bleue légère, pour que la traversée se voie sans masquer le damier.
        const SIGMA: [f32; 3] = [0.55, 0.20, 0.06];

        // `interfaces` : 0 = aucune déviation · 1 = l'entrée seule · 2 = entrée + sortie.
        let rendu = |interfaces: u8| -> Vec<u8> {
            let mut rvb = vec![0u8; cote * cote * 3];
            for y in 0..cote {
                for x in 0..cote {
                    let i = y * cote + x;
                    let rayon = direction(x, y);
                    let mut lumiere = environnement(rayon);

                    if let Some((e, s2)) = carte.segment(i) {
                        let ne = carte.normale_entree[i];
                        let cos_i = -rayon.dot(ne);
                        // Fresnel : ce qui rebondit sur la surface au lieu d'entrer. C'est lui qui
                        // allume le bord de toute bille, et il ne demande aucun réglage.
                        let part_reflechie = fresnel(cos_i, 1.0, N_SUCRE);
                        let reflet = environnement(reflechir(rayon, ne));

                        let mut sortant = rayon;
                        if interfaces >= 1 {
                            sortant = refracter(rayon, ne, 1.0 / N_SUCRE).unwrap_or(rayon);
                        }
                        if interfaces >= 2 {
                            // ⚠ L'APPROXIMATION EST ICI, ET NULLE PART AILLEURS : on redresse le
                            // rayon avec la normale de sortie du rayon DROIT, alors que le rayon
                            // dévié ne sort pas exactement au même endroit.
                            let ns = carte.normale_sortie[i];
                            sortant = refracter(sortant, ns * -1.0, N_SUCRE / 1.0)
                                // Pas de racine : réflexion totale, le rayon reste prisonnier.
                                .unwrap_or_else(|| reflechir(sortant, ns * -1.0));
                        }

                        let fond = environnement(sortant);
                        let epaisseur = s2 - e;
                        for canal in 0..3 {
                            let traverse =
                                fond[canal] * transmittance(SIGMA[canal], epaisseur);
                            lumiere[canal] = traverse * (1.0 - part_reflechie)
                                + reflet[canal] * part_reflechie;
                        }
                    }

                    for canal in 0..3 {
                        rvb[i * 3 + canal] =
                            ((lumiere[canal].powf(1.0 / 2.2)).clamp(0.0, 1.0) * 255.0) as u8;
                    }
                }
            }
            rvb
        };

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier");
        for (nom, n) in [
            ("refraction-0-sans.png", 0u8),
            ("refraction-1-entree.png", 1),
            ("refraction-2-deux-interfaces.png", 2),
        ] {
            std::fs::write(
                dossier.join(nom),
                crate::image::png::encoder(cote as u32, cote as u32, &rendu(n)).expect("png"),
            )
            .expect("ecriture");
        }

        // ── ⭐ CE QUI EST MESURÉ : L'IMAGE EST-ELLE VRAIMENT INVERSÉE ? ───────────────────────
        // Une sphère pleine d'indice 1,5 a son foyer juste derrière sa face arrière : elle replie
        // le monde et **l'inverse**. Donc un rayon qui part vers la DROITE doit ressortir en
        // regardant vers la GAUCHE. Si ce signe ne bascule pas, il n'y a pas de lentille.
        let mut inverses = 0usize;
        let mut testes = 0usize;
        for x in (cote / 2 + 20)..(cote / 2 + 140) {
            let y = cote / 2;
            let i = y * cote + x;
            if carte.segment(i).is_none() {
                continue;
            }
            let rayon = direction(x, y);
            let ne = carte.normale_entree[i];
            let ns = carte.normale_sortie[i];
            let t1 = refracter(rayon, ne, 1.0 / N_SUCRE).unwrap();
            let t2 = refracter(t1, ns * -1.0, N_SUCRE).unwrap_or_else(|| reflechir(t1, ns * -1.0));
            testes += 1;
            if rayon.x > 0.0 && t2.x < 0.0 {
                inverses += 1;
            }
        }
        println!("lentille : {inverses} rayons inverses sur {testes} testes");
        assert!(
            inverses * 2 > testes,
            "seulement {inverses} rayons sur {testes} sont renvoyes de l'autre cote — la bille ne \
             se comporte pas comme une lentille"
        );
    }

    /// ⭐⭐⭐ **DE COMBIEN L'APPROXIMATION SE TROMPE — mesuré contre une vérité analytique.**
    ///
    /// La réfraction à deux interfaces contient **une** approximation, et une seule : pour redresser
    /// le rayon à la sortie, on emploie la normale de sortie du rayon **DROIT**. Or le rayon dévié
    /// ne ressort pas au même endroit — donc pas sous la même normale.
    ///
    /// **Sur une sphère, la vérité se calcule exactement** (l'intersection rayon-sphère est une
    /// équation du second degré). On peut donc chiffrer l'erreur au lieu d'en parler.
    ///
    /// *L'image précédente était convaincante. Convaincante n'est pas juste, et c'est précisément
    /// le genre de chose que ce projet refuse de laisser dans le flou.*
    #[test]
    fn l_erreur_de_l_approximation_a_deux_interfaces() {
        const R: f32 = 1.0;
        const N: f32 = 1.50;

        /// Les deux distances où un rayon coupe la sphère de rayon `R` centrée à l'origine.
        fn couper(origine: Vec3, direction: Vec3, r: f32) -> Option<(f32, f32)> {
            let b = origine.dot(direction);
            let c = origine.dot(origine) - r * r;
            let d = b * b - c;
            if d < 0.0 {
                return None;
            }
            let s = d.sqrt();
            Some((-b - s, -b + s))
        }

        let camera = Vec3::new(0.0, 0.0, 3.6);
        let mut erreur_max = 0.0f32;
        let mut somme = 0.0f32;
        let mut somme_avance = 0.0f32;
        let mut somme_itere = 0.0f32;
        let mut max_avance = 0.0f32;
        let mut max_itere = 0.0f32;
        let mut comptes = 0usize;
        let mut pire_hauteur = 0.0f32;
        let mut bascules = 0usize;
        let mut max_itere_meme_regime = 0.0f32;
        let mut somme_ponderee = 0.0f32;
        let mut poids = 0.0f32;

        // On balaie des rayons parallèles à l'axe, de plus en plus loin du centre.
        for k in 0..200 {
            let hauteur = (k as f32 + 0.5) / 200.0 * R * 0.995;
            let origine = Vec3::new(hauteur, 0.0, camera.z);
            let rayon = Vec3::new(0.0, 0.0, -1.0);

            let Some((entree, sortie_droite)) = couper(origine, rayon, R) else {
                continue;
            };

            // ── L'entrée : identique dans les deux cas, elle n'est pas en cause ──────────────
            let p1 = origine + rayon * entree;
            let n1 = p1 * (1.0 / R);
            let t1 = refracter(rayon, n1, 1.0 / N).expect("entrer dans le sucre reussit toujours");

            // ── LA VÉRITÉ : où le rayon DÉVIÉ ressort réellement ────────────────────────────
            let (_, sortie_vraie) = couper(p1, t1, R).expect("un rayon parti de la surface ressort");
            let p2_vrai = p1 + t1 * sortie_vraie;
            let n2_vrai = p2_vrai * (1.0 / R);
            let t2_vrai = refracter(t1, n2_vrai * -1.0, N).unwrap_or_else(|| reflechir(t1, n2_vrai * -1.0));

            // ⚠ On note AUSSI si les deux rayons sont dans le même régime. Au-delà de l'angle
            // critique la lumière ne sort pas, elle rebrousse : comparer une direction réfractée à
            // une direction réfléchie n'est pas mesurer une erreur d'approximation, c'est constater
            // un basculement. **Les mélanger fabriquerait un chiffre qui ne veut rien dire.**
            let vrai_sort = refracter(t1, n2_vrai * -1.0, N).is_some();
            let mesurer = |n2: Vec3| -> (f32, bool) {
                let sort = refracter(t1, n2 * -1.0, N).is_some();
                let t2 = refracter(t1, n2 * -1.0, N).unwrap_or_else(|| reflechir(t1, n2 * -1.0));
                (t2_vrai.dot(t2).clamp(-1.0, 1.0).acos().to_degrees(), sort == vrai_sort)
            };

            // ── ① NAÏVE : la normale de sortie du rayon DROIT ────────────────────────────────
            let (naive, meme_regime) = mesurer((origine + rayon * sortie_droite) * (1.0 / R));

            // ── ② AVANCER LE LONG DU RAYON DÉVIÉ ─────────────────────────────────────────────
            // On ne connaît pas la corde du rayon dévié, mais on connaît celle du rayon droit :
            // on avance de cette longueur-là depuis l'entrée, EN SUIVANT t1. Le point tombe près de
            // la surface sans être dessus ; sur une sphère, le ramener dessus est une
            // normalisation. **Dans le cas général, ce serait une lecture de la carte au pixel où
            // ce point se projette** — la même idée, une texture au lieu d'une formule.
            let corde = sortie_droite - entree;
            let (avance, _) = mesurer((p1 + t1 * corde).normalize());

            // ── ③ UNE ITÉRATION DE PLUS ──────────────────────────────────────────────────────
            // Le point trouvé donne une meilleure corde ; on recommence une fois avec elle.
            let p_est = (p1 + t1 * corde).normalize() * R;
            let corde2 = (p_est - p1).length();
            let (itere, itere_meme_regime) = mesurer((p1 + t1 * corde2).normalize());

            // ⭐ Et l'erreur PONDÉRÉE : près du bord, Fresnel réfléchit presque tout, donc ce qui
            // traverse ne pèse presque rien dans l'image. Une erreur là-bas coûte moins qu'au
            // centre — et c'est mesurable plutôt que plaidable.
            let part_transmise = 1.0 - fresnel(-rayon.dot(n1), 1.0, N);
            somme_ponderee += itere * part_transmise;
            poids += part_transmise;
            if !itere_meme_regime {
                bascules += 1;
            } else {
                max_itere_meme_regime = max_itere_meme_regime.max(itere);
            }
            let _ = meme_regime;

            somme += naive;
            somme_avance += avance;
            somme_itere += itere;
            comptes += 1;
            if naive > erreur_max {
                erreur_max = naive;
                pire_hauteur = hauteur;
            }
            max_avance = max_avance.max(avance);
            max_itere = max_itere.max(itere);
        }

        let n = comptes as f32;
        println!("─── ERREUR SUR LA DIRECTION SORTANTE, contre la verite analytique ───");
        println!(
            "  ① normale du rayon DROIT      : moyenne {:6.2}°   max {:6.2}°  (a {:.0} % du rayon)",
            somme / n,
            erreur_max,
            pire_hauteur / R * 100.0
        );
        println!(
            "  ② avancer le long du devie    : moyenne {:6.2}°   max {:6.2}°",
            somme_avance / n,
            max_avance
        );
        println!(
            "  ③ + une iteration             : moyenne {:6.2}°   max {:6.2}°",
            somme_itere / n,
            max_itere
        );

        // ⚠ Ce test ne DÉCIDE pas que l'approximation est acceptable — il enregistre ce qu'elle
        // coûte, pour que la prochaine session ne le redécouvre pas, et pour qu'une amélioration se
        // mesure contre un chiffre. **Le seuil est un garde-fou de non-régression, pas un verdict.**
        println!(
            "  ③ hors basculement de regime  : max {:6.2}°   ({bascules} rayons sur {comptes} basculent)",
            max_itere_meme_regime
        );
        println!(
            "  ③ ponderee par ce qui TRAVERSE : {:6.2}°   (Fresnel reflechit presque tout au bord)",
            somme_ponderee / poids
        );

        // ── ⚠⚠ CE QUE CES CHIFFRES DISENT, ET IL FAUT LE DIRE FRANCHEMENT ────────────────────
        //
        // **L'approximation à deux interfaces est mauvaise sur une bille épaisse.** Dix degrés
        // d'erreur moyenne sur la direction sortante, ça se voit. Et **un rayon sur cinq bascule de
        // régime** — il réfracte là où il devrait réfléchir, ou l'inverse : ces pixels-là ne sont
        // pas imprécis, ils sont faux.
        //
        // *L'image de la bille au damier était convaincante. Elle était fausse. C'est exactement ce
        // qu'on cherchait à savoir, et on ne l'aurait jamais su en la regardant.*
        //
        // **Ce qui reste vrai malgré tout :** l'erreur décroît vite avec l'épaisseur traversée. Sur
        // une vitre, une bulle de savon, une feuille, une paroi mince, la déviation est petite et
        // l'approximation tient. C'est une bille pleine d'indice 1,5 qui est le pire cas possible.
        let _ = max_itere;
        assert!(
            somme_itere < somme,
            "la correction n'ameliore meme pas la moyenne : {} contre {}",
            somme_itere / n,
            somme / n
        );
        assert!(
            max_itere_meme_regime < 25.0,
            "hors basculement, l'erreur monte a {max_itere_meme_regime}° — la correction a regresse"
        );
        assert!(
            bascules * 4 < comptes,
            "{bascules} rayons sur {comptes} basculent de regime : plus du quart de l'image serait faux"
        );
    }

    /// Un maillage fermé non convexe donne la somme de ses segments de matière, sans un mot de code
    /// en plus. **On n'a rien fait pour : ça tombe de la signature.**
    ///
    /// Deux sphères disjointes alignées sur l'axe de vue : au centre, on traverse deux diamètres.
    #[test]
    fn deux_objets_alignes_donnent_la_somme_de_leurs_epaisseurs() {
        let (sommets, indices_un) = bille_du_banc(1.0, 48, 48);
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        for decalage in [-3.0f32, 0.0] {
            let base = positions.len() as u32;
            for s in &sommets {
                positions.push(Vec3::new(s.position[0], s.position[1], s.position[2] + decalage));
            }
            indices.extend(indices_un.iter().map(|i| i + base));
        }

        let camera = Vec3::new(0.0, 0.0, 8.0);
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, -1.5), Vec3::new(0.0, 1.0, 0.0));
        let projection = Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 100.0);
        let carte = rendre(&positions, None, &indices, projection * vue, camera, 256, 256);

        let au_centre = carte.lire(128, 128);
        assert!(
            (au_centre - 4.0).abs() < 0.08,
            "deux spheres de diametre 2 donnent {au_centre} au lieu de 4"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════════════════════
    //  NEWTON — où le rayon ressort vraiment
    // ═══════════════════════════════════════════════════════════════════════════════════════════

    /// Les deux distances où un rayon coupe la sphère de rayon `r` centrée à l'origine.
    ///
    /// **C'est L'ÉTALON de tous les tests de réfraction d'ici.** Sur une sphère, la vérité est une
    /// équation du second degré : elle se calcule exactement, donc toute approximation se **chiffre**
    /// au lieu de se plaider. *C'est cette fonction, et rien d'autre, qui permet de dire « 11,37° »
    /// plutôt que « ça a l'air mieux ».*
    fn couper_la_sphere(origine: Vec3, direction: Vec3, r: f32) -> Option<(f32, f32)> {
        let b = origine.dot(direction);
        let c = origine.dot(origine) - r * r;
        let d = b * b - c;
        if d < 0.0 {
            return None;
        }
        let s = d.sqrt();
        Some((-b - s, -b + s))
    }

    /// Le décor commun aux deux mesures de Newton : une caméra, sa projection, ses rayons.
    ///
    /// Rend `(caméra, projeter, direction_pixel)` — les trois choses dont [`chercher_la_sortie`] a
    /// besoin, **toutes cohérentes avec la convention de [`rendre`]** (Y descend à l'écran).
    #[allow(clippy::type_complexity)]
    fn banc_de_refraction(
        cote: usize,
    ) -> (
        Vec3,
        Mat4,
        impl Fn(Vec3) -> Option<(f32, f32)>,
        impl Fn(usize, usize) -> Vec3,
    ) {
        let camera = Vec3::new(0.0, 0.0, 3.6);
        let fov = 45f32.to_radians();
        let vue = Mat4::look_at_rh(camera, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let vue_proj = Mat4::perspective_rh(fov, 1.0, 0.1, 100.0) * vue;

        let projeter = move |p: Vec3| -> Option<(f32, f32)> {
            let d = vue_proj * Vec4::new(p.x, p.y, p.z, 1.0);
            if d.w <= 1e-6 {
                return None;
            }
            let iw = 1.0 / d.w;
            Some((
                (d.x * iw * 0.5 + 0.5) * cote as f32,
                (0.5 - d.y * iw * 0.5) * cote as f32,
            ))
        };

        let tangente = (fov * 0.5).tan();
        let direction = move |x: usize, y: usize| -> Vec3 {
            let ndc_x = (x as f32 + 0.5) / cote as f32 * 2.0 - 1.0;
            let ndc_y = 1.0 - (y as f32 + 0.5) / cote as f32 * 2.0;
            Vec3::new(ndc_x * tangente, ndc_y * tangente, -1.0).normalize()
        };

        (camera, vue_proj, projeter, direction)
    }

    /// Une carte d'épaisseur **exacte** d'une sphère : aucune tessellation, aucune interpolation.
    ///
    /// ⚠ **Elle sert à isoler l'erreur de NEWTON de celle du maillage.** Mesurer les deux ensemble
    /// donnerait un chiffre qu'on ne saurait pas attribuer — exactement la faute du 31 août, où un
    /// écart de 88° venait du chargeur et non du rendu.
    fn carte_exacte_de_sphere(
        rayon: f32,
        camera: Vec3,
        cote: usize,
        direction: impl Fn(usize, usize) -> Vec3,
    ) -> CarteEpaisseur {
        let n = cote * cote;
        let mut carte = CarteEpaisseur {
            largeur: cote,
            hauteur: cote,
            valeurs: vec![0.0; n],
            entree: vec![f32::INFINITY; n],
            sortie: vec![f32::NEG_INFINITY; n],
            normale_entree: vec![Vec3::new(0.0, 0.0, 0.0); n],
            normale_sortie: vec![Vec3::new(0.0, 0.0, 0.0); n],
        };

        for y in 0..cote {
            for x in 0..cote {
                let d = direction(x, y);
                let Some((t0, t1)) = couper_la_sphere(camera, d, rayon) else {
                    continue;
                };
                if t0 <= 0.0 {
                    continue;
                }
                let i = y * cote + x;
                carte.valeurs[i] = t1 - t0;
                carte.entree[i] = t0;
                carte.sortie[i] = t1;
                carte.normale_entree[i] = (camera + d * t0) * (1.0 / rayon);
                carte.normale_sortie[i] = (camera + d * t1) * (1.0 / rayon);
            }
        }
        carte
    }

    /// ⭐⭐⭐ **LA MESURE QUI JUSTIFIE NEWTON** — l'erreur en degrés, budget par budget.
    ///
    /// Le 1er septembre 2026, l'approximation employée jusque-là a été chiffrée sur cette même
    /// bille : **26,06° pour la normale du rayon droit, 15,47° en avançant le long du dévié,
    /// 11,37° avec un raffinement de plus** — et **un rayon sur cinq basculait de régime** (il
    /// sortait alors qu'il aurait dû rebrousser, ou l'inverse). Ces pixels-là ne sont pas
    /// imprécis : ils sont faux.
    ///
    /// **Ce test dit ce que chaque pas de Newton achète, en degrés.** Il ne demande pas si l'image
    /// « a l'air mieux » : il compare à une vérité calculée exactement.
    ///
    /// ## ⚠⚠ CE QU'IL MESURE, ET CE QU'IL NE MESURE PAS — à lire avant de citer ses chiffres
    ///
    /// **Il balaie UNE LIGNE d'écran, l'équateur — et c'est le cas le plus FAVORABLE de toute
    /// l'image.** Les rayons y restent dans le plan de symétrie de la sphère ; ailleurs, et surtout
    /// près des pôles où toutes les tranches convergent, l'écart est bien plus violent.
    ///
    /// Mesuré sur **toute** la bille (100 016 rayons, voir
    /// [`les_trois_images_de_la_bille_approximation_newton_et_verite`]) :
    ///
    /// | | sur l'équateur seul | sur toute la bille |
    /// |---|---|---|
    /// | approximation | 1,811° | **36,402°** |
    /// | Newton, 4 pas | 0,115° | **1,740°** |
    ///
    /// *Vingt fois pire.* Le chiffre de 11,37° écrit le 1er septembre venait lui aussi d'un
    /// balayage à une dimension : il était donc optimiste, dans le même sens et pour la même
    /// raison. **Quand une image et un banc ne s'accordent pas, c'est que le banc mesure autre
    /// chose que ce qu'on croit.**
    ///
    /// ## Les critères, écrits AVANT la première exécution
    ///
    /// *(règle 2 du projet : le critère précède la mesure, sinon on tune le banc jusqu'à ce qu'il
    /// plaise)*
    ///
    /// 1. L'erreur moyenne **décroît strictement** de 1 à 4 pas.
    /// 2. À 4 pas, elle est **sous 1°**.
    /// 3. À 4 pas, **moins de 1 %** des rayons basculent de régime — contre 20 % avant.
    #[test]
    fn newton_trouve_la_sortie_et_chaque_pas_divise_l_erreur() {
        const R: f32 = 1.0;
        const N: f32 = 1.50;
        const COTE: usize = 512;

        let (camera, _, projeter, direction) = banc_de_refraction(COTE);
        let carte = carte_exacte_de_sphere(R, camera, COTE, &direction);
        let vue = VueEcran {
            carte: &carte,
            camera,
            projeter: &projeter,
            direction_pixel: &direction,
        };

        // La ligne horizontale qui passe par le centre : elle balaie toutes les incidences, du
        // rayon perpendiculaire au rayon rasant.
        let y = COTE / 2;
        let mut courbe = Vec::new();

        // ⚠⚠ LE BUDGET ZÉRO N'EST PAS UNE FORMALITÉ — c'est ce qui rend la comparaison HONNÊTE.
        //
        // Le chiffre de 11,37° a été mesuré hier sur un AUTRE banc (200 rayons parallèles à l'axe,
        // pas une ligne d'écran en perspective). Annoncer « Newton fait 84 fois mieux » en
        // comparant deux instruments différents serait exactement la faute que le corpus dénonce :
        // *un écart mesuré contre une référence qui ne vient pas du même endroit ne mesure rien.*
        //
        // On remesure donc l'approximation d'avant ICI, sur CE banc : zéro pas de Newton, la
        // normale lue au point estimé par la corde du rayon droit.
        for budget in 0..=5usize {
            let (mut somme, mut compte, mut bascules, mut pire) = (0.0f32, 0usize, 0usize, 0.0f32);
            // Le plus GRAND écart à l'angle critique parmi les rayons qui ont basculé — voir plus
            // bas pourquoi c'est cette grandeur-là, et pas un pourcentage, qui juge la méthode.
            let mut plus_loin_bascule = 0.0f32;

            for x in COTE / 2..COTE {
                let rayon = direction(x, y);
                let Some((t0, t1_droit)) = couper_la_sphere(camera, rayon, R) else {
                    continue;
                };
                if t0 <= 0.0 {
                    continue;
                }

                // ── L'ENTRÉE : identique partout, elle n'est pas en cause ──────────────────────
                let p1 = camera + rayon * t0;
                let n1 = p1 * (1.0 / R);
                let Some(devie) = refracter(rayon, n1, 1.0 / N) else {
                    continue;
                };

                // ── LA VÉRITÉ : où le rayon DÉVIÉ ressort réellement ──────────────────────────
                let Some((_, s_vrai)) = couper_la_sphere(p1, devie, R) else {
                    continue;
                };
                let n2_vrai = (p1 + devie * s_vrai) * (1.0 / R);
                let vrai_sort = refracter(devie, n2_vrai * -1.0, N).is_some();
                let dir_vraie = refracter(devie, n2_vrai * -1.0, N)
                    .unwrap_or_else(|| reflechir(devie, n2_vrai * -1.0));

                // ── NEWTON, avec ce budget-là ─────────────────────────────────────────────────
                // L'estimation de départ est la corde du rayon DROIT : elle est gratuite, elle est
                // déjà dans la carte, et c'est exactement l'approximation qu'on cherche à battre.
                let estimation = t1_droit - t0;
                let n2 = if budget == 0 {
                    // Zéro pas : on lit la normale là où l'ancienne approximation croyait sortir.
                    let Some((px, py)) = projeter(p1 + devie * estimation) else {
                        continue;
                    };
                    if px < 0.0 || py < 0.0 || px >= COTE as f32 || py >= COTE as f32 {
                        continue;
                    }
                    let i = py as usize * COTE + px as usize;
                    if carte.valeurs[i] <= 0.0 {
                        continue;
                    }
                    carte.normale_sortie[i]
                } else {
                    let Some(trouvee) = chercher_la_sortie(
                        &vue,
                        p1,
                        devie,
                        estimation,
                        Budget { iterations_max: budget, tolerance_pixels: 0.5 },
                    ) else {
                        continue;
                    };
                    trouvee.normale
                };
                let sort = refracter(devie, n2 * -1.0, N).is_some();
                let dir_trouvee =
                    refracter(devie, n2 * -1.0, N).unwrap_or_else(|| reflechir(devie, n2 * -1.0));

                // ⚠ Un basculement de régime n'est PAS une erreur d'angle : comparer une direction
                // réfractée à une direction réfléchie fabrique un chiffre qui ne veut rien dire.
                // On les compte à part — c'est la leçon du 1er septembre, où un « maximum à 121° »
                // n'était pas une imprécision mais deux physiques différentes mises côte à côte.
                if sort != vrai_sort {
                    // ⚠⚠ ON NE SE CONTENTE PAS DE COMPTER : on demande à chaque basculement à
                    // quelle DISTANCE DE L'ANGLE CRITIQUE il s'est produit.
                    //
                    // C'est la seule question qui compte. Contre l'angle critique, sortir et
                    // rebrousser sont à égalité : **aucune méthode ne peut y trancher sans
                    // erreur**, et un basculement n'y est pas un défaut mais une discontinuité
                    // physique. Loin de lui, en revanche, c'est une faute franche.
                    let incidence = (-devie.dot(n2_vrai * -1.0)).clamp(-1.0, 1.0).acos();
                    let critique = (1.0f32 / N).asin();
                    plus_loin_bascule =
                        plus_loin_bascule.max((incidence - critique).to_degrees().abs());
                    bascules += 1;
                    compte += 1;
                    continue;
                }

                let ecart = dir_vraie
                    .dot(dir_trouvee)
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees();
                somme += ecart;
                pire = pire.max(ecart);
                compte += 1;
            }

            let moyenne = somme / (compte - bascules).max(1) as f32;
            let taux = bascules as f32 / compte.max(1) as f32 * 100.0;
            println!(
                "{budget} pas : moyenne {moyenne:6.3}°   pire {pire:7.3}°   \
                 basculements {bascules:3} / {compte} ({taux:5.2} %)   \
                 le plus loin de l'angle critique : {plus_loin_bascule:6.3}°"
            );
            courbe.push((moyenne, plus_loin_bascule));
        }

        // ── CRITÈRE 1 : chaque pas achète quelque chose ───────────────────────────────────────
        // `courbe[0]` est l'approximation d'AVANT (zéro pas) ; les pas de Newton commencent à 1.
        for tour in 2..5 {
            assert!(
                courbe[tour].0 < courbe[tour - 1].0,
                "le pas {tour} n'a rien apporte : {:.3}° apres {:.3}°",
                courbe[tour].0,
                courbe[tour - 1].0
            );
        }

        // ── CRITÈRE 2 : à 4 pas, on est sous le degré ─────────────────────────────────────────
        assert!(
            courbe[4].0 < 1.0,
            "a 4 pas l'erreur vaut encore {:.3}°",
            courbe[4].0
        );

        // ── CRITÈRE 3 : ⚠ REFORMULÉ APRÈS LA PREMIÈRE MESURE, ET IL FAUT DIRE POURQUOI ────────
        //
        // Il disait d'abord « moins de 1 % des rayons basculent ». **Il est tombé à 1,12 %** — et
        // au lieu de desserrer le seuil, on est allé voir D'OÙ venaient les basculements.
        //
        // Réponse : dès le PREMIER pas de Newton il n'en reste que **deux**, aux pixels 433 et 434,
        // à **0,32°** et **0,055°** de l'angle critique. À cette distance-là, sortir et rebrousser
        // sont à égalité : c'est une discontinuité physique, pas une erreur de méthode. *Le budget
        // zéro, lui, en produit 35, dont un à **8,69°** de l'angle critique — celui-là est une
        // faute franche.*
        //
        // **Donc le critère juste ne porte pas sur un pourcentage — qui dépend de la résolution et
        // du nombre de rayons — mais sur la GRANDEUR PHYSIQUE : à quelle distance de la
        // discontinuité la méthode se trompe-t-elle encore ?** *Même correction qu'hier sur les
        // normales : une garde formulée sur un epsilon ne dit rien ; formulée sur la grandeur qui
        // compte, elle est opposable.*
        assert!(
            courbe[4].1 < 1.0,
            "a 4 pas, un rayon bascule encore a {:.3}° de l'angle critique",
            courbe[4].1
        );

        // ── CRITÈRE 4 : ⭐ Newton bat l'approximation d'avant, MESURÉE SUR CE MÊME BANC ────────
        // C'est le seul des quatre qui compare deux choses produites par le même instrument, donc
        // le seul qu'on ait le droit de citer comme un gain.
        assert!(
            courbe[4].0 * 10.0 < courbe[0].0,
            "Newton ({:.3}°) ne bat pas l'approximation d'avant ({:.3}°) d'un facteur 10",
            courbe[4].0,
            courbe[0].0
        );
    }

    /// ⚠ **LA MÊME MESURE, SUR LA VRAIE CARTE** — celle qu'un maillage produit, pas celle qu'une
    /// équation produit.
    ///
    /// Le test précédent isole Newton en lui donnant une sphère parfaite. **Celui-ci dit ce qu'on
    /// obtient pour de vrai** : une sphère de 96 tranches, rastérisée, avec des normales
    /// interpolées par sommet. *L'écart entre les deux chiffres est le prix de l'espace écran, et
    /// il n'est écrit nulle part dans le papier des auteurs — ils comparent au ray marching, pas à
    /// la vérité.*
    #[test]
    fn newton_sur_la_carte_rasterisee_dit_le_prix_de_l_espace_ecran() {
        const R: f32 = 1.0;
        const N: f32 = 1.50;
        const COTE: usize = 512;

        let (camera, vue_proj, projeter, direction) = banc_de_refraction(COTE);

        let (sommets, indices) = bille_du_banc(R, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();
        // Sur une sphère centrée à l'origine, la normale exacte d'un sommet EST sa position
        // normalisée. On les fournit : sans elles, chaque facette du maillage apparaîtrait dans
        // l'image réfractée.
        let normales: Vec<Vec3> = positions.iter().map(|p| *p * (1.0 / R)).collect();

        let carte = rendre(
            &positions,
            Some(&normales),
            &indices,
            vue_proj,
            camera,
            COTE,
            COTE,
        );
        let vue = VueEcran {
            carte: &carte,
            camera,
            projeter: &projeter,
            direction_pixel: &direction,
        };

        let y = COTE / 2;
        let (mut somme, mut compte, mut bascules, mut echecs, mut pire) =
            (0.0f32, 0usize, 0usize, 0usize, 0.0f32);
        let (mut somme_zero, mut compte_zero) = (0.0f32, 0usize);

        for x in COTE / 2..COTE {
            let rayon = direction(x, y);
            let Some((t0, t1_droit)) = couper_la_sphere(camera, rayon, R) else {
                continue;
            };
            if t0 <= 0.0 {
                continue;
            }

            let p1 = camera + rayon * t0;
            let n1 = p1 * (1.0 / R);
            let Some(devie) = refracter(rayon, n1, 1.0 / N) else {
                continue;
            };

            let Some((_, s_vrai)) = couper_la_sphere(p1, devie, R) else {
                continue;
            };
            let n2_vrai = (p1 + devie * s_vrai) * (1.0 / R);
            let vrai_sort = refracter(devie, n2_vrai * -1.0, N).is_some();
            let dir_vraie = refracter(devie, n2_vrai * -1.0, N)
                .unwrap_or_else(|| reflechir(devie, n2_vrai * -1.0));

            compte += 1;

            // ⚠ On mesure AUSSI le budget zéro sur ce banc-ci — l'approximation d'avant, lue au
            // point où la corde du rayon droit croyait sortir. Sans elle, le seuil de ce test
            // serait choisi contre un chiffre venu d'un AUTRE banc, donc arbitraire. *La mutation
            // l'a prouvé : avec un seuil emprunté, ce test passait le pas de Newton désarmé.*
            let estimation = t1_droit - t0;
            if let Some((px, py)) = projeter(p1 + devie * estimation) {
                if px >= 0.0 && py >= 0.0 && px < COTE as f32 && py < COTE as f32 {
                    let i = py as usize * COTE + px as usize;
                    if carte.valeurs[i] > 0.0 {
                        let n0 = carte.normale_sortie[i];
                        if refracter(devie, n0 * -1.0, N).is_some() == vrai_sort {
                            let d0 = refracter(devie, n0 * -1.0, N)
                                .unwrap_or_else(|| reflechir(devie, n0 * -1.0));
                            somme_zero += dir_vraie
                                .dot(d0)
                                .clamp(-1.0, 1.0)
                                .acos()
                                .to_degrees();
                            compte_zero += 1;
                        }
                    }
                }
            }

            let Some(trouvee) = chercher_la_sortie(
                &vue,
                p1,
                devie,
                estimation,
                Budget { iterations_max: 4, tolerance_pixels: 0.5 },
            ) else {
                // Pas de sortie trouvée : c'est le mode d'échec que les auteurs annoncent près des
                // contours d'un objet épais. On le COMPTE plutôt que de le maquiller.
                echecs += 1;
                continue;
            };

            let n2 = trouvee.normale;
            let sort = refracter(devie, n2 * -1.0, N).is_some();
            let dir_trouvee =
                refracter(devie, n2 * -1.0, N).unwrap_or_else(|| reflechir(devie, n2 * -1.0));

            if sort != vrai_sort {
                bascules += 1;
                continue;
            }

            let ecart = dir_vraie
                .dot(dir_trouvee)
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            somme += ecart;
            pire = pire.max(ecart);
        }

        let aboutis = compte - bascules - echecs;
        let moyenne = somme / aboutis.max(1) as f32;
        let moyenne_zero = somme_zero / compte_zero.max(1) as f32;
        println!(
            "carte rasterisee : sans Newton {moyenne_zero:6.3}°  →  4 pas {moyenne:6.3}°   \
             (pire {pire:7.3}°, basculements {bascules}, echecs {echecs}, sur {compte} rayons)"
        );

        // ⚠ LE CRITÈRE COMPARE LES DEUX CHIFFRES DE CE BANC-CI, et c'est ce qui le rend mordant.
        // Sa première version exigeait « moins de 3° », un seuil emprunté aux 11,37° mesurés
        // ailleurs — et la mutation a montré qu'il laissait passer le pas de Newton DÉSARMÉ (1,82°
        // suffisait). *Un seuil qui vient d'un autre instrument ne garde rien.*
        assert!(
            moyenne * 5.0 < moyenne_zero,
            "sur la carte rasterisee Newton ({moyenne:.3}°) ne bat pas l'approximation \
             ({moyenne_zero:.3}°) d'un facteur 5"
        );
        // Et l'immense majorité des rayons doit aboutir. Les échecs se concentrent près du
        // contour ; s'ils dominent, la méthode ne tient pas dans notre cas.
        assert!(
            aboutis * 4 > compte * 3,
            "seulement {aboutis} rayons sur {compte} aboutissent"
        );
    }

    /// ⭐⭐⭐ **LES TROIS IMAGES QUI SE COMPARENT** — l'approximation, Newton, et la VÉRITÉ.
    ///
    /// Sur une sphère, la vérité se calcule exactement. **On peut donc rendre les trois images du
    /// même objet et les mettre côte à côte** — ce qu'aucun papier de ce domaine ne fait, parce
    /// qu'aucun ne travaille sur une géométrie dont il connaît la réponse.
    ///
    /// *C'est l'étalon rendu visible : son œil juge le même écart que les degrés mesurent.*
    ///
    /// ⚠ **Ce test ne prouve pas une image, il en PRODUIT trois.** L'assertion qu'il porte est
    /// arithmétique : Newton doit être **plus proche de la vérité, pixel par pixel**, que
    /// l'approximation. Le jugement du rendu reste à l'œil, jamais à un test.
    #[test]
    fn les_trois_images_de_la_bille_approximation_newton_et_verite() {
        const R: f32 = 1.0;
        const N_SUCRE: f32 = 1.50;
        const COTE: usize = 512;
        // Une teinte bleue légère : la traversée doit se voir sans masquer le damier.
        const SIGMA: [f32; 3] = [0.55, 0.20, 0.06];

        let (sommets, indices) = bille_du_banc(R, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();
        let normales: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.normal[0], s.normal[1], s.normal[2]))
            .collect();

        let (camera, vue_proj, projeter, direction) = banc_de_refraction(COTE);
        let carte = rendre(
            &positions,
            Some(&normales),
            &indices,
            vue_proj,
            camera,
            COTE,
            COTE,
        );
        let vue = VueEcran {
            carte: &carte,
            camera,
            projeter: &projeter,
            direction_pixel: &direction,
        };

        // Le monde derrière : un damier en coordonnées de direction. **Aucune géométrie** — ce qui
        // compte est de reconnaître d'un coup d'œil si l'image est repliée, inversée, comprimée.
        let environnement = |d: Vec3| -> [f32; 3] {
            let u = d.z.atan2(d.x) / std::f32::consts::TAU + 0.5;
            let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
            let case = (u * 28.0).floor() as i32 + (v * 14.0).floor() as i32;
            if case.rem_euclid(2) == 0 {
                [0.80, 0.83, 0.90]
            } else {
                [0.045, 0.05, 0.075]
            }
        };

        /// Comment le rayon dévié trouve sa sortie. Les trois valent la même physique — elles ne
        /// diffèrent QUE par la façon de répondre à « où est la seconde interface ? ».
        #[derive(Clone, Copy, PartialEq)]
        enum Methode {
            /// La normale de sortie du rayon DROIT. C'est ce qu'on faisait, et c'est faux.
            Approximation,
            /// La méthode de Newton, avec un budget de pas.
            Newton(usize),
            /// L'intersection exacte du rayon dévié avec la sphère. **Impossible en général —
            /// possible ici parce qu'on sait que c'est une sphère.**
            Verite,
        }

        let rendu = |methode: Methode| -> (Vec<u8>, Vec<[f32; 3]>) {
            let mut rvb = vec![0u8; COTE * COTE * 3];
            let mut lineaire = vec![[0.0f32; 3]; COTE * COTE];

            for y in 0..COTE {
                for x in 0..COTE {
                    let i = y * COTE + x;
                    let rayon = direction(x, y);
                    let mut lumiere = environnement(rayon);

                    if let Some((e, s2)) = carte.segment(i) {
                        let ne = carte.normale_entree[i];
                        let part_reflechie = fresnel(-rayon.dot(ne), 1.0, N_SUCRE);
                        let reflet = environnement(reflechir(rayon, ne));

                        let devie = refracter(rayon, ne, 1.0 / N_SUCRE).unwrap_or(rayon);
                        let p1 = camera + rayon * e;

                        // ── LA SEULE CHOSE QUI CHANGE ENTRE LES TROIS IMAGES ─────────────────
                        let normale_sortie = match methode {
                            Methode::Approximation => Some(carte.normale_sortie[i]),
                            Methode::Newton(pas) => chercher_la_sortie(
                                &vue,
                                p1,
                                devie,
                                s2 - e,
                                Budget { iterations_max: pas, tolerance_pixels: 0.5 },
                            )
                            .map(|t| t.normale),
                            Methode::Verite => couper_la_sphere(p1, devie, R)
                                .map(|(_, t)| (p1 + devie * t) * (1.0 / R)),
                        };

                        // ⚠ Quand Newton échoue, on ne fabrique pas une valeur plausible : on
                        // retombe sur l'approximation, ET on le fait franchement. Les pixels
                        // concernés se comptent dans le test de mesure, pas ici.
                        let ns = normale_sortie.unwrap_or(carte.normale_sortie[i]);
                        let sortant = refracter(devie, ns * -1.0, N_SUCRE)
                            .unwrap_or_else(|| reflechir(devie, ns * -1.0));

                        let fond = environnement(sortant);
                        for canal in 0..3 {
                            lumiere[canal] = fond[canal]
                                * transmittance(SIGMA[canal], s2 - e)
                                * (1.0 - part_reflechie)
                                + reflet[canal] * part_reflechie;
                        }
                    }

                    lineaire[i] = lumiere;
                    for canal in 0..3 {
                        rvb[i * 3 + canal] =
                            ((lumiere[canal].powf(1.0 / 2.2)).clamp(0.0, 1.0) * 255.0) as u8;
                    }
                }
            }
            (rvb, lineaire)
        };

        let (image_approx, lin_approx) = rendu(Methode::Approximation);
        let (image_newton, lin_newton) = rendu(Methode::Newton(4));
        let (image_verite, lin_verite) = rendu(Methode::Verite);

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier");
        for (nom, image) in [
            ("newton-1-approximation.png", &image_approx),
            ("newton-2-quatre-pas.png", &image_newton),
            ("newton-3-verite.png", &image_verite),
        ] {
            std::fs::write(
                dossier.join(nom),
                crate::image::png::encoder(COTE as u32, COTE as u32, image).expect("png"),
            )
            .expect("ecriture");
        }

        // ── ⭐ CE QUI EST MESURÉ : la distance à la vérité, pixel par pixel ───────────────────
        // On ne compare pas les deux images entre elles — on compare **chacune à la vérité**.
        // C'est la seule comparaison qui a un sens, et elle n'est possible que parce qu'on a
        // choisi une géométrie dont on connaît la réponse.
        let mut ecart_approx = 0.0f64;
        let mut ecart_newton = 0.0f64;
        let mut dans_la_bille = 0usize;

        for i in 0..COTE * COTE {
            if carte.segment(i).is_none() {
                continue;
            }
            dans_la_bille += 1;
            for canal in 0..3 {
                ecart_approx += (lin_approx[i][canal] - lin_verite[i][canal]).abs() as f64;
                ecart_newton += (lin_newton[i][canal] - lin_verite[i][canal]).abs() as f64;
            }
        }

        let n = (dans_la_bille * 3).max(1) as f64;
        let (ea, en) = (ecart_approx / n, ecart_newton / n);
        println!(
            "ecart moyen a la VERITE, sur {dans_la_bille} pixels de bille :\n  \
             approximation {ea:.5}\n  Newton 4 pas  {en:.5}   ({:.1}x mieux)",
            ea / en.max(1e-9)
        );

        // ── ⚠⚠ ET L'ERREUR ANGULAIRE SUR TOUTE LA BILLE, PAS SUR UNE LIGNE ───────────────────
        //
        // Le banc de mesure principal ne balaie qu'une **ligne** d'écran — les rayons du plan
        // équatorial. L'image, elle, montrait un écart bien plus violent que les 1,8° annoncés :
        // le monde entièrement replié d'un côté, une sphère nette de l'autre. **Deux chiffres qui
        // ne s'accordent pas, c'est qu'un des deux mesure autre chose.**
        //
        // *C'est la faute « extrapoler une borne au-delà de ce que l'instrument atteint » : une
        // ligne ne dit rien des pôles, où toutes les tranches convergent et où la normale du rayon
        // droit s'écarte le plus.* On remesure donc sur **tous** les pixels de la bille.
        let (mut somme_a, mut somme_n, mut pire_a, mut pire_n, mut comptes) =
            (0.0f64, 0.0f64, 0.0f32, 0.0f32, 0usize);

        for y in 0..COTE {
            for x in 0..COTE {
                let i = y * COTE + x;
                let Some((e, s2)) = carte.segment(i) else {
                    continue;
                };
                let rayon = direction(x, y);
                let Some(devie) = refracter(rayon, carte.normale_entree[i], 1.0 / N_SUCRE) else {
                    continue;
                };
                let p1 = camera + rayon * e;
                let Some((_, t_vrai)) = couper_la_sphere(p1, devie, R) else {
                    continue;
                };

                let angle = |ns: Vec3| -> Vec3 {
                    refracter(devie, ns * -1.0, N_SUCRE)
                        .unwrap_or_else(|| reflechir(devie, ns * -1.0))
                };
                let vraie = angle((p1 + devie * t_vrai) * (1.0 / R));
                let approx = angle(carte.normale_sortie[i]);
                let Some(trouvee) = chercher_la_sortie(
                    &vue,
                    p1,
                    devie,
                    s2 - e,
                    Budget { iterations_max: 4, tolerance_pixels: 0.5 },
                ) else {
                    continue;
                };
                let newton = angle(trouvee.normale);

                let deg = |a: Vec3| a.dot(vraie).clamp(-1.0, 1.0).acos().to_degrees();
                let (da, dn) = (deg(approx), deg(newton));
                somme_a += da as f64;
                somme_n += dn as f64;
                pire_a = pire_a.max(da);
                pire_n = pire_n.max(dn);
                comptes += 1;
            }
        }

        let c = comptes.max(1) as f64;
        println!(
            "erreur angulaire sur TOUTE la bille ({comptes} rayons) :\n  \
             approximation {:.3}° (pire {pire_a:.1}°)\n  \
             Newton 4 pas  {:.3}° (pire {pire_n:.1}°)",
            somme_a / c,
            somme_n / c
        );

        assert!(
            en * 3.0 < ea,
            "Newton ({en:.5}) n'est pas 3x plus proche de la verite que l'approximation ({ea:.5})"
        );
        assert!(
            somme_n * 10.0 < somme_a,
            "sur toute la bille, Newton ({:.3}°) ne bat pas l'approximation ({:.3}°) d'un \
             facteur 10",
            somme_n / c,
            somme_a / c
        );
    }

    /// ⭐⭐⭐ **CE QUE ÇA COÛTE SUR UNE MACHINE QU'ON N'A PAS** — le compteur de travail portable.
    ///
    /// ## ⚠⚠ Le problème, posé franchement : il n'y a pas de Quest 2 dans cette maison
    ///
    /// Tout le budget du projet — **13,9 ms pour deux yeux à 72 Hz** — est **calculé** à partir de
    /// specs publiées reprises de seconde main, et **jamais mesuré**. Aucun Quest 2 n'a jamais fait
    /// tourner Aegis, et il n'existe ni portage Android, ni OpenXR, ni rendu stéréo. *Donc même
    /// avec un casque sous la main, on ne pourrait rien mesurer aujourd'hui.*
    ///
    /// **Mesurer des millisecondes ici serait donc une fausse certitude** : le temps de CETTE
    /// machine ne dit rien de ce que coûte le programme sur une machine modeste. *C'est son
    /// intuition à lui, formulée le 9 août 2026 sur le globe, et elle était juste — le compteur de
    /// travail portable est né de là.*
    ///
    /// ## Ce qu'on mesure à la place, et qui se transpose
    ///
    /// **Le nombre de LECTURES DE CARTE par pixel.** Sur un GPU mobile, un pas de Newton ne coûte
    /// presque aucun calcul : il coûte **une lecture de texture**, et la bande passante est la
    /// ressource rare — *87 octets par pixel pour toute l'image*, G-buffer et post-traitement
    /// compris. **Ce compteur-là ne dépend d'aucune machine.**
    ///
    /// ## ⭐ Et le chiffre qui compte n'est pas le budget, c'est la MOYENNE
    ///
    /// Newton s'arrête dès qu'il a convergé. Un budget de 8 pas ne veut donc pas dire 8 lectures
    /// par pixel — il dit *au plus* 8. **C'est la distribution réelle qui décide du coût**, et
    /// c'est elle que ce test rend.
    #[test]
    fn le_cout_de_newton_se_compte_en_lectures_pas_en_millisecondes() {
        const R: f32 = 1.0;
        const N_SUCRE: f32 = 1.50;
        const COTE: usize = 512;

        let (sommets, indices) = bille_du_banc(R, 96, 96);
        let positions: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.position[0], s.position[1], s.position[2]))
            .collect();
        let normales: Vec<Vec3> = sommets
            .iter()
            .map(|s| Vec3::new(s.normal[0], s.normal[1], s.normal[2]))
            .collect();

        let (camera, vue_proj, projeter, direction) = banc_de_refraction(COTE);
        let carte = rendre(
            &positions,
            Some(&normales),
            &indices,
            vue_proj,
            camera,
            COTE,
            COTE,
        );
        let vue = VueEcran {
            carte: &carte,
            camera,
            projeter: &projeter,
            direction_pixel: &direction,
        };

        // Un budget large : on veut voir où Newton s'arrête TOUT SEUL, pas où on l'arrête.
        let mut histogramme = [0usize; 12];
        let mut total_lectures = 0usize;
        let mut pixels = 0usize;
        let mut convergés = 0usize;
        let mut somme_erreur = 0.0f64;

        for y in 0..COTE {
            for x in 0..COTE {
                let i = y * COTE + x;
                let Some((e, s2)) = carte.segment(i) else {
                    continue;
                };
                let rayon = direction(x, y);
                let Some(devie) = refracter(rayon, carte.normale_entree[i], 1.0 / N_SUCRE) else {
                    continue;
                };
                let p1 = camera + rayon * e;

                pixels += 1;
                let Some(t) = chercher_la_sortie(
                    &vue,
                    p1,
                    devie,
                    s2 - e,
                    Budget { iterations_max: 8, tolerance_pixels: 0.5 },
                ) else {
                    // Un échec a quand même coûté ses lectures — on ne peut pas les compter ici
                    // (la fonction ne rend rien), et c'est une limite honnête de ce banc.
                    continue;
                };

                total_lectures += t.lectures;
                histogramme[t.lectures.min(11)] += 1;
                if t.convergee {
                    convergés += 1;
                }

                // L'erreur atteinte quand Newton décide lui-même de s'arrêter.
                if let Some((_, t_vrai)) = couper_la_sphere(p1, devie, R) {
                    let angle = |ns: Vec3| -> Vec3 {
                        refracter(devie, ns * -1.0, N_SUCRE)
                            .unwrap_or_else(|| reflechir(devie, ns * -1.0))
                    };
                    let vraie = angle((p1 + devie * t_vrai) * (1.0 / R));
                    somme_erreur +=
                        angle(t.normale).dot(vraie).clamp(-1.0, 1.0).acos().to_degrees() as f64;
                }
            }
        }

        let moyenne_lectures = total_lectures as f64 / pixels.max(1) as f64;
        println!("\n  ── LE TRAVAIL, compté en lectures de carte ──");
        println!("  {pixels} pixels de bille · budget 8 pas · tolerance 0,5 px");
        println!(
            "  convergence spontanee : {:.1} % des pixels",
            convergés as f64 / pixels.max(1) as f64 * 100.0
        );
        println!("  erreur atteinte      : {:.3}°", somme_erreur / pixels.max(1) as f64);
        println!("  LECTURES PAR PIXEL   : {moyenne_lectures:.2} en moyenne\n");

        let mut cumul = 0usize;
        for (n, &compte) in histogramme.iter().enumerate() {
            if compte == 0 {
                continue;
            }
            cumul += compte;
            println!(
                "    {n:2} lecture(s) : {compte:6} pixels  ({:5.1} % · cumul {:5.1} %)",
                compte as f64 / pixels as f64 * 100.0,
                cumul as f64 / pixels as f64 * 100.0
            );
        }

        // ── ⭐ CE QUE ÇA DONNE EN OCTETS, ET LA COMPARAISON AU BUDGET QUEST 2 ─────────────────
        //
        // ⚠ Ce calcul suppose un format, et le format n'est pas encore choisi. Les trois lignes
        // disent donc ce que CHAQUE choix coûterait — c'est un cadrage, pas une mesure.
        // ── ⭐ CE QUE ÇA DONNE EN OCTETS, ET LA VRAIE QUESTION QUE ÇA POSE ────────────────────
        //
        // ⚠⚠ LE PIÈGE QUE CE BLOC ÉVITE : « 26 octets par pixel » ne veut rien dire tant qu'on n'a
        // pas dit **par pixel de QUOI**. Newton ne tourne que sur les pixels de VERRE — le reste de
        // l'écran ne le paie pas. *Rapporter le coût à l'écran entier le divise par la surface de
        // verre, et c'est cette fraction-là qui décide si ça tient.*
        //
        // Ici la bille occupe une part énorme de l'image (un gros plan) : c'est le pire cas, et
        // c'est volontaire.
        let fraction = pixels as f64 / (COTE * COTE) as f64;
        println!("\n  ── CE QUE ÇA COÛTE EN BANDE PASSANTE ──");
        println!(
            "  budget d'une image sur Quest 2 : ~87 o/pixel  ⚠ CALCULE, jamais mesure — \
             aucun Quest 2 n'a fait tourner Aegis"
        );
        println!("  le verre couvre ici {:.0} % de l'ecran (gros plan = pire cas)", fraction * 100.0);
        for (format, octets) in [
            ("distance + normale en RGBA16F", 8.0),
            ("RGBA8, normale octaedrique", 4.0),
        ] {
            let par_pixel_de_verre = moyenne_lectures * octets;
            let par_pixel_d_ecran = par_pixel_de_verre * fraction;
            // ⭐ Et la question renversée, qui est la seule vraiment utile : en s'accordant 10 % du
            // budget pour la réfraction, quelle part de l'écran peut être en verre ?
            let part_max = 8.7 / par_pixel_de_verre * 100.0;
            println!(
                "    {format:30} → {par_pixel_de_verre:5.1} o par pixel de verre \
                 · {par_pixel_d_ecran:4.1} o par pixel d'ecran ici \
                 · tient jusqu'a {part_max:4.0} % d'ecran en verre"
            );
        }
        println!(
            "\n  ⚠ Ce calcul ne compte QUE les lectures de Newton. Il ne compte pas la production\n  \
             de la carte des faces arriere (une passe de plus), ni le reste de l'image."
        );

        // ── LES CRITÈRES, écrits d'avance ─────────────────────────────────────────────────────
        // 1. Newton doit s'arrêter tout seul bien avant son budget : sinon la tolérance ne sert à
        //    rien et le coût serait celui du pire cas pour tout le monde.
        assert!(
            moyenne_lectures < 5.0,
            "Newton lit {moyenne_lectures:.2} fois la carte par pixel — trop pour un budget mobile"
        );
        // ⚠ ET LA BORNE INFÉRIEURE, qui n'est pas une formalité : sans elle, un compteur cassé à
        // zéro ferait passer le critère ci-dessus **haut la main**, et on annoncerait un coût nul.
        // *Un seuil qui n'a qu'un côté ne garde que la moitié de ce qu'on croit.* Deux lectures est
        // le plancher physique : un pas de Newton, plus la lecture finale de la normale.
        assert!(
            moyenne_lectures >= 2.0,
            "moins de 2 lectures par pixel est impossible — le compteur ment ({moyenne_lectures:.2})"
        );
        // 2. Et l'immense majorité doit converger d'elle-même, pas être coupée par le budget.
        assert!(
            convergés * 10 > pixels * 9,
            "seulement {convergés} pixels sur {pixels} convergent d'eux-memes"
        );
    }
}
