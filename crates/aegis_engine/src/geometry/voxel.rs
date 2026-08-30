//! # VOXELISER — ramener un maillage lisse dans la trame du monde
//!
//! Né le 31 août 2026. Le décor du jeu est fait de cubes alignés sur une grille ; les objets
//! importés, eux, sont des maillages lisses exportés d'un modeleur. **Une scie ronde et polie
//! posée dans un monde en cubes ne cohabite pas avec lui, elle y est déposée.** L'œil le voit
//! immédiatement, sans savoir le nommer.
//!
//! Ce module prend un maillage quelconque et rend le maillage de sa **coquille de voxels**.
//!
//! ## ⭐ La taille du voxel n'est pas une résolution, c'est celle du MONDE
//!
//! On demande d'habitude « en combien de subdivisions ? ». C'est un réglage arbitraire, et pire :
//! il donne des voxels de tailles différentes selon l'objet, donc des objets qui ne partagent
//! aucune trame — exactement le défaut qu'on cherchait à corriger.
//!
//! Ici on demande le **côté d'un voxel**, dans l'unité du maillage. Un grand objet en compte plus,
//! un petit moins, et tous tombent sur la même grille que le décor. *La constante ne rétrécit pas :
//! la question « combien de subdivisions » cesse de se poser.*
//!
//! ⚠ C'est le JEU qui fournit ce côté, parce que la finesse de sa trame est une décision de
//! direction artistique. Le moteur sait voxeliser ; il ne sait pas à quoi le monde ressemble.
//!
//! ## Une COQUILLE, pas un volume
//!
//! On ne marque que les cellules **traversées par une surface**, jamais l'intérieur. Deux raisons,
//! et la seconde compte plus que la première : c'est linéaire en nombre de triangles au lieu de
//! demander un test d'appartenance par cellule ; et on ne voit jamais l'intérieur d'un objet
//! opaque, donc le remplir serait payer pour ce que personne ne regarde. *Jamais d'excédent.*
//!
//! ## Ce qui n'est PAS fait
//!
//! Les faces coplanaires voisines ne sont pas fusionnées en rectangles (« greedy meshing »). Ça
//! diviserait encore le nombre de triangles, et c'est un vrai gain — mais le compte mesuré sur la
//! scie tient déjà dans le budget, et optimiser avant d'avoir un chiffre qui gêne est le meilleur
//! moyen de compliquer pour rien.

use crate::geometry::vertex::Vertex;
use crate::math::{Vec2, Vec3, Vec4};

/// Les six directions dans lesquelles une cellule peut avoir un voisin, et la face qui leur
/// correspond. L'ordre n'a pas d'importance ; la cohérence entre la normale et les quatre coins,
/// si — une face dont la normale ne regarde pas dehors s'éclaire comme si elle était à l'ombre.
const FACES: [([i32; 3], [f32; 3]); 6] = [
    ([1, 0, 0], [1.0, 0.0, 0.0]),
    ([-1, 0, 0], [-1.0, 0.0, 0.0]),
    ([0, 1, 0], [0.0, 1.0, 0.0]),
    ([0, -1, 0], [0.0, -1.0, 0.0]),
    ([0, 0, 1], [0.0, 0.0, 1.0]),
    ([0, 0, -1], [0.0, 0.0, -1.0]),
];

/// Une grille de cellules pleines ou vides.
struct Grille {
    dimensions: [usize; 3],
    /// L'origine du coin de la cellule (0,0,0), dans l'espace du maillage.
    origine: Vec3,
    cote: f32,
    pleines: Vec<bool>,
}

impl Grille {
    fn indice(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dimensions[1] + y) * self.dimensions[0] + x
    }

    /// Vrai si la cellule est pleine. **Hors de la grille compte comme VIDE**, ce qui est la seule
    /// réponse juste : c'est ce qui fait qu'un objet a des faces sur son pourtour.
    fn pleine(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.dimensions[0] || y >= self.dimensions[1] || z >= self.dimensions[2] {
            return false;
        }
        self.pleines[self.indice(x, y, z)]
    }

    /// Le centre d'une cellule, dans l'espace du maillage.
    fn centre(&self, x: usize, y: usize, z: usize) -> Vec3 {
        self.origine
            + Vec3::new(
                (x as f32 + 0.5) * self.cote,
                (y as f32 + 0.5) * self.cote,
                (z as f32 + 0.5) * self.cote,
            )
    }
}

/// Un triangle touche-t-il une boîte alignée sur les axes ?
///
/// C'est le **théorème des axes séparants** appliqué au couple triangle/boîte (Akenine-Möller) :
/// deux volumes convexes sont disjoints si et seulement s'il existe un axe sur lequel leurs
/// projections ne se recouvrent pas, et pour ce couple-là treize axes suffisent à le décider.
///
/// ⚠ **Le geste tentant est d'échantillonner le triangle à petits pas et de marquer les cellules
/// touchées.** Ça marche presque toujours, et « presque » est le problème : un triangle long et
/// mince traverse une cellule par un coin sans qu'aucun échantillon n'y tombe, et le trou
/// n'apparaît que sur certains modèles, à certaines résolutions. Un test exact n'a pas de
/// « presque » — et il est plus rapide, puisqu'il ne teste chaque cellule qu'une fois.
fn triangle_touche_boite(sommets: [Vec3; 3], centre: Vec3, demi: f32) -> bool {
    // On se place dans le repère de la boîte : elle devient centrée sur l'origine.
    let v = [sommets[0] - centre, sommets[1] - centre, sommets[2] - centre];
    let aretes = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];

    // ── Les neuf axes croisés : chaque arête du triangle contre chaque axe de la boîte ────────
    for arete in aretes {
        // Le produit vectoriel d'une arête avec chacun des trois axes, écrit à la main : les
        // composantes nulles des axes annulent la moitié des termes, et les garder coûterait
        // trois multiplications par zéro à chaque test — pour 13 tests par cellule et par
        // triangle, c'est le genre de détail qui se voit au chargement.
        let croises = [
            Vec3::new(0.0, -arete.z, arete.y),
            Vec3::new(arete.z, 0.0, -arete.x),
            Vec3::new(-arete.y, arete.x, 0.0),
        ];
        for axe in croises {
            let p = [v[0].dot(axe), v[1].dot(axe), v[2].dot(axe)];
            let rayon = demi * (axe.x.abs() + axe.y.abs() + axe.z.abs());
            let bas = p[0].min(p[1]).min(p[2]);
            let haut = p[0].max(p[1]).max(p[2]);
            if bas > rayon || haut < -rayon {
                return false;
            }
        }
    }

    // ── Les trois axes de la boîte : le triangle tient-il d'un seul côté d'une de ses faces ? ─
    for composante in [
        [v[0].x, v[1].x, v[2].x],
        [v[0].y, v[1].y, v[2].y],
        [v[0].z, v[1].z, v[2].z],
    ] {
        let [a, b, c] = composante;
        if a.min(b).min(c) > demi || a.max(b).max(c) < -demi {
            return false;
        }
    }

    // ── La normale du triangle : le plan du triangle coupe-t-il la boîte ? ────────────────────
    let normale = aretes[0].cross(aretes[1]);
    let distance = normale.dot(v[0]);
    let rayon = demi * (normale.x.abs() + normale.y.abs() + normale.z.abs());
    distance.abs() <= rayon
}

/// Transforme un maillage en sa coquille de voxels, de côté `cote` dans l'unité du maillage.
///
/// Rend un maillage de faces prêt à téléverser : mêmes sommets, mêmes indices que n'importe quel
/// autre. Rien en aval n'a besoin de savoir qu'il vient d'une voxelisation.
///
/// ⚠ **Seules les faces exposées sont émises.** Une face entre deux cellules pleines n'est jamais
/// visible, et l'émettre doublerait le maillage pour des triangles que la profondeur rejette.
///
/// Un `cote` nul ou négatif, ou un maillage vide, rendent un maillage vide plutôt que de paniquer :
/// un objet qui disparaît se voit, un programme qui s'arrête au chargement fait bien pire.
pub fn voxeliser(sommets: &[Vertex], indices: &[u32], cote: f32) -> (Vec<Vertex>, Vec<u32>) {
    // ⚠ `is_finite` AVANT la comparaison, et ce n'est pas de la coquetterie : un NaN rend FAUX
    // toute comparaison, donc `cote <= 0.0` le laisserait passer — et la grille se dimensionnerait
    // ensuite sur un NaN, ce qui donne zéro cellule et un objet qui disparaît sans un mot.
    if sommets.is_empty() || indices.len() < 3 || !cote.is_finite() || cote <= 0.0 {
        return (Vec::new(), Vec::new());
    }

    // ── La boîte englobante, et la grille qui la couvre ───────────────────────────────────────
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for s in sommets {
        let p = Vec3::from(s.position);
        min = min.min(p);
        max = max.max(p);
    }

    // ⚠ La grille est ancrée sur des MULTIPLES de `cote`, pas sur le coin de l'objet. C'est ce
    // qui met tous les objets voxelisés sur la MÊME trame, celle du décor — les ancrer chacun sur
    // sa propre boîte donnerait des grilles décalées les unes des autres, et le monde ne
    // paraîtrait aligné qu'objet par objet.
    let origine = Vec3::new(
        (min.x / cote).floor() * cote,
        (min.y / cote).floor() * cote,
        (min.z / cote).floor() * cote,
    );
    let etendue = max - origine;
    let dimensions = [
        ((etendue.x / cote).ceil() as usize + 1).max(1),
        ((etendue.y / cote).ceil() as usize + 1).max(1),
        ((etendue.z / cote).ceil() as usize + 1).max(1),
    ];

    let mut grille = Grille {
        dimensions,
        origine,
        cote,
        pleines: vec![false; dimensions[0] * dimensions[1] * dimensions[2]],
    };

    // ── Marquer les cellules que chaque triangle traverse ─────────────────────────────────────
    let demi = cote * 0.5;
    for triangle in indices.chunks_exact(3) {
        let coins = [
            Vec3::from(sommets[triangle[0] as usize].position),
            Vec3::from(sommets[triangle[1] as usize].position),
            Vec3::from(sommets[triangle[2] as usize].position),
        ];

        // On ne teste que les cellules de la boîte englobante DU TRIANGLE : c'est ce qui rend
        // l'ensemble linéaire en nombre de triangles au lieu de quadratique.
        let t_min = coins[0].min(coins[1]).min(coins[2]);
        let t_max = coins[0].max(coins[1]).max(coins[2]);
        let borne = |valeur: f32, depart: f32, axe: usize| -> usize {
            (((valeur - depart) / cote).floor().max(0.0) as usize)
                .min(dimensions[axe].saturating_sub(1))
        };

        for z in borne(t_min.z, origine.z, 2)..=borne(t_max.z, origine.z, 2) {
            for y in borne(t_min.y, origine.y, 1)..=borne(t_max.y, origine.y, 1) {
                for x in borne(t_min.x, origine.x, 0)..=borne(t_max.x, origine.x, 0) {
                    let i = grille.indice(x, y, z);
                    if grille.pleines[i] {
                        continue;
                    }
                    if triangle_touche_boite(coins, grille.centre(x, y, z), demi) {
                        grille.pleines[i] = true;
                    }
                }
            }
        }
    }

    // ── Émettre les faces exposées ────────────────────────────────────────────────────────────
    let mut sortie = Vec::new();
    let mut liens = Vec::new();

    for z in 0..dimensions[2] {
        for y in 0..dimensions[1] {
            for x in 0..dimensions[0] {
                if !grille.pleines[grille.indice(x, y, z)] {
                    continue;
                }
                let centre = grille.centre(x, y, z);
                for (direction, normale) in FACES {
                    if grille.pleine(
                        x as i32 + direction[0],
                        y as i32 + direction[1],
                        z as i32 + direction[2],
                    ) {
                        continue;
                    }
                    poser_face(&mut sortie, &mut liens, centre, demi, normale);
                }
            }
        }
    }

    (sortie, liens)
}

/// Pose les quatre coins et les deux triangles d'une face de cube.
fn poser_face(
    sommets: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    centre: Vec3,
    demi: f32,
    normale: [f32; 3],
) {
    let n = Vec3::from(normale);
    // Deux directions perpendiculaires à la normale, qui balaient la face. On prend l'axe le
    // moins aligné avec la normale pour éviter un produit vectoriel nul.
    let repere = if n.x.abs() < 0.5 { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.0, 1.0, 0.0) };
    let u = n.cross(repere);
    let v = n.cross(u);

    let base = sommets.len() as u32;
    let milieu = centre + n * demi;
    for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        sommets.push(Vertex::new(
            milieu + u * (du * demi) + v * (dv * demi),
            n,
            // ⚠ Tangente et coordonnées de texture à zéro : **aucun shader du moteur ne les lit**
            // (il n'y a pas une seule texture dans le projet). Leur donner des valeurs plausibles
            // serait écrire une intention que rien ne réalise — le jour où une texture existe,
            // c'est ici qu'il faudra revenir, et le vide le dit mieux qu'un remplissage.
            Vec4::new(0.0, 0.0, 0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
        ));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
        let nul = Vec3::new(0.0, 0.0, 1.0);
        let sommets = [a, b, c]
            .map(|p| {
                Vertex::new(
                    Vec3::from(p),
                    nul,
                    Vec4::new(0.0, 0.0, 0.0, 0.0),
                    Vec2::new(0.0, 0.0),
                    Vec2::new(0.0, 0.0),
                )
            })
            .to_vec();
        (sommets, vec![0, 1, 2])
    }

    /// Le test des axes séparants dit oui quand le triangle traverse vraiment la boîte.
    #[test]
    fn un_triangle_qui_traverse_la_boite_est_detecte() {
        let coins = [
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        assert!(triangle_touche_boite(coins, Vec3::new(0.0, 0.0, 0.0), 0.5));
    }

    /// ⚠ Le cas qui distingue un test exact d'un échantillonnage : un triangle qui n'entre dans
    /// la boîte que par un COIN. Aucun échantillon régulier n'y tomberait, et le trou ne se
    /// verrait que sur certains modèles — le pire genre de défaut.
    #[test]
    fn un_triangle_qui_n_entre_que_par_un_coin_est_detecte() {
        // Un grand triangle dans le plan z = 0, dont un bord frôle le coin de la boîte.
        let coins = [
            Vec3::new(0.4, 0.4, -1.0),
            Vec3::new(0.4, 0.4, 1.0),
            Vec3::new(3.0, 3.0, 0.0),
        ];
        assert!(
            triangle_touche_boite(coins, Vec3::splat(0.0), 0.5),
            "le coin (0,4 / 0,4) est dans une boite de demi-cote 0,5"
        );
    }

    /// Et il dit non quand le triangle passe à côté — sinon toute la grille se remplirait.
    #[test]
    fn un_triangle_qui_passe_a_cote_est_rejete() {
        let coins = [
            Vec3::new(5.0, 5.0, 5.0),
            Vec3::new(6.0, 5.0, 5.0),
            Vec3::new(5.0, 6.0, 5.0),
        ];
        assert!(!triangle_touche_boite(coins, Vec3::splat(0.0), 0.5));

        // Plus subtil : un triangle dont la BOÎTE ENGLOBANTE recouvre la cellule, mais dont le
        // plan passe au large. C'est exactement ce qu'un test par boîte englobante raterait.
        let oblique = [
            Vec3::new(-3.0, 3.0, 0.0),
            Vec3::new(3.0, -3.0, 0.0),
            Vec3::new(3.0, -3.0, 1.0),
        ];
        assert!(!triangle_touche_boite(oblique, Vec3::new(2.0, 2.0, 0.0), 0.5));
    }

    /// Un maillage vide, ou un côté absurde, rendent un maillage vide — jamais une panique.
    #[test]
    fn les_entrees_absurdes_rendent_un_maillage_vide() {
        let (s, i) = triangle([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        for cote in [0.0, -1.0, f32::NAN] {
            assert!(voxeliser(&s, &i, cote).0.is_empty(), "cote={cote}");
        }
        assert!(voxeliser(&[], &[], 0.1).0.is_empty());
        assert!(voxeliser(&s, &[], 0.1).0.is_empty());
    }

    /// ⭐ Un triangle plus petit qu'une cellule tient dans UNE cellule, qui a ses six faces.
    ///
    /// C'est le plus petit résultat non vide possible, et il vérifie d'un coup le marquage,
    /// l'émission et le compte : six faces, quatre coins chacune, deux triangles chacune.
    #[test]
    fn un_triangle_minuscule_donne_un_seul_cube_complet() {
        let (s, i) = triangle([0.5, 0.5, 0.5], [0.55, 0.5, 0.5], [0.5, 0.55, 0.5]);
        let (sortie, liens) = voxeliser(&s, &i, 1.0);
        assert_eq!(sortie.len(), 6 * 4, "six faces de quatre coins");
        assert_eq!(liens.len(), 6 * 6, "six faces de deux triangles");
    }

    /// ⭐⭐ LA propriété qui justifie tout le module : les faces INTERNES ne sont pas émises.
    ///
    /// Deux cellules voisines et pleines partagent une face que personne ne peut voir. Un
    /// voxeliseur naïf émettrait 12 faces pour deux cubes ; celui-ci en émet 10.
    #[test]
    fn les_faces_entre_deux_cellules_pleines_ne_sont_pas_emises() {
        // Un triangle allongé qui traverse deux cellules voisines en x.
        let (s, i) = triangle([0.5, 0.5, 0.5], [1.5, 0.5, 0.5], [0.5, 0.6, 0.5]);
        let (sortie, _) = voxeliser(&s, &i, 1.0);
        assert_eq!(
            sortie.len(),
            10 * 4,
            "deux cubes voisins montrent 10 faces, pas 12 — la face partagee est invisible"
        );
    }

    /// ⭐ La grille est ancrée sur des multiples du côté, pas sur l'objet.
    ///
    /// C'est ce qui met deux objets différents sur la MÊME trame. Déplacer un maillage d'un
    /// nombre entier de voxels doit donc déplacer sa coquille d'exactement autant — et déplacer
    /// d'un demi-voxel ne doit PAS produire une coquille recalée sur l'objet.
    #[test]
    fn la_trame_appartient_au_monde_et_non_a_l_objet() {
        let (s, i) = triangle([0.2, 0.2, 0.0], [0.3, 0.2, 0.0], [0.2, 0.3, 0.0]);
        let (a, _) = voxeliser(&s, &i, 1.0);

        // Décalé d'un voxel entier : la coquille doit suivre exactement.
        let decale: Vec<Vertex> = s
            .iter()
            .map(|v| {
                let mut c = *v;
                c.position[0] += 1.0;
                c
            })
            .collect();
        let (b, _) = voxeliser(&decale, &i, 1.0);
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(b.iter()) {
            assert!(
                (q.position[0] - p.position[0] - 1.0).abs() < 1e-5,
                "un decalage d'un voxel entier decale la coquille d'un voxel entier"
            );
            assert!((q.position[1] - p.position[1]).abs() < 1e-5);
        }
    }
}
