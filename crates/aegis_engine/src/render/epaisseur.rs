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

    /// Un passage doux de 0 à 1 entre deux bornes — la courbe d'Hermite `3t² − 2t³`.
    ///
    /// ⚠ **Sa dérivée s'annule aux deux bouts**, et c'est toute la différence avec un `clamp` :
    /// une rampe linéaire a un coude, et un coude se VOIT sur une image. *La première nectarine
    /// portait deux anneaux nets pour cette seule raison.*
    fn fondu(depart: f32, arrivee: f32, valeur: f32) -> f32 {
        let t = ((valeur - depart) / (arrivee - depart)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Une sphère, sa caméra, et la matrice qui va avec — le décor de tous les tests d'ici.
    fn sphere(rayon: f32, tranches: u32) -> (Vec<Vec3>, Vec<u32>, Mat4, Vec3, f32) {
        let (sommets, indices) = Primitives::create_uv_sphere(rayon, tranches, tranches);
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
        let (sommets, indices) = Primitives::create_uv_sphere(1.0, 96, 96);
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
        let (sommets, indices) = Primitives::create_uv_sphere(1.0, 96, 96);
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

        // ⭐ LE MÊME CHAMP EN QUATRE PAS — le budget d'un casque. C'est le curseur d'adaptativité,
        // rendu visible : un seul nombre change, ni le code ni le champ.
        let grossiere = integrer_le_champ(&carte, camera, direction, 4, champ);
        let image_grossiere = peindre(&grossiere);
        std::fs::write(
            dossier.join("nectarine-4-pas.png"),
            crate::image::png::encoder(cote as u32, cote as u32, &image_grossiere).expect("png"),
        )
        .expect("ecriture");

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
        let (sommets, indices) = Primitives::create_uv_sphere(1.0, 96, 96);
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
        let (sommets, indices) = Primitives::create_uv_sphere(1.0, 96, 96);
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

    /// Un maillage fermé non convexe donne la somme de ses segments de matière, sans un mot de code
    /// en plus. **On n'a rien fait pour : ça tombe de la signature.**
    ///
    /// Deux sphères disjointes alignées sur l'axe de vue : au centre, on traverse deux diamètres.
    #[test]
    fn deux_objets_alignes_donnent_la_somme_de_leurs_epaisseurs() {
        let (sommets, indices_un) = Primitives::create_uv_sphere(1.0, 48, 48);
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
}
