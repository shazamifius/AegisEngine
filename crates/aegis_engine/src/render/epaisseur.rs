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
}

impl CarteEpaisseur {
    pub fn lire(&self, x: usize, y: usize) -> f32 {
        self.valeurs[y * self.largeur + x]
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
pub fn rendre(
    positions: &[Vec3],
    indices: &[u32],
    vue_projection: Mat4,
    camera: Vec3,
    largeur: usize,
    hauteur: usize,
) -> CarteEpaisseur {
    let mut valeurs = vec![0.0f32; largeur * hauteur];

    for triangle in indices.chunks_exact(3) {
        let mut sommets = [SommetProjete { x: 0.0, y: 0.0, distance_sur_w: 0.0, inverse_w: 0.0 }; 3];
        let mut derriere_la_camera = false;

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
                y: (ndc_y * 0.5 + 0.5) * hauteur as f32,
                distance_sur_w: distance * inverse_w,
                inverse_w,
            };
        }

        if derriere_la_camera {
            continue;
        }

        accumuler_triangle(&sommets, &mut valeurs, largeur, hauteur);
    }

    CarteEpaisseur { largeur, hauteur, valeurs }
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
    valeurs: &mut [f32],
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
            valeurs[py * largeur + px] += if aire < 0.0 { -distance } else { distance };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::primitives::Primitives;

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
        let carte = rendre(&positions, &indices, vue_proj, camera, 256, 256);

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
        let carte = rendre(&positions, &indices, vue_proj, camera, 256, 256);

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
        let carte = rendre(&positions, &indices, vue_proj, camera, 256, 256);

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
        let carte = rendre(&positions, &indices, vue_proj, camera, 256, 256);

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
            // Une distance de 1 et un `w` de 1 : ce qui est accumulé vaut donc exactement ±1 par
            // revendication, et un double comptage se lit directement.
            distance_sur_w: 1.0,
            inverse_w: 1.0,
        };

        let mut valeurs = vec![0.0f32; 32 * 32];
        accumuler_triangle(&[s(10.0, 10.0), s(20.0, 20.0), s(10.0, 20.0)], &mut valeurs, 32, 32);
        accumuler_triangle(&[s(10.0, 10.0), s(20.0, 10.0), s(20.0, 20.0)], &mut valeurs, 32, 32);

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
        let carte = rendre(&positions, &indices, projection * vue, camera, cote, cote);

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
        let carte2 = rendre(&cabosse, &indices, projection * vue, camera, cote, cote);
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
        let carte = rendre(&positions, &indices, projection * vue, camera, 256, 256);

        let au_centre = carte.lire(128, 128);
        assert!(
            (au_centre - 4.0).abs() < 0.08,
            "deux spheres de diametre 2 donnent {au_centre} au lieu de 4"
        );
    }
}
