//! # LA JUNGLE DU FOND — trois plans qui reculent, et qui bougent
//!
//! ## ⚠ Pourquoi ce fichier existe, et ce qu'il débloque
//!
//! Le fond était **le ciel regardé en face** : un aplat uni sur la moitié de l'écran. Aucun réglage
//! d'ambiance ne pouvait le sauver, et pire — il **bloquait l'ambiante** :
//!
//! `ambiance_hemispherique` mélange ciel et sol selon `direction.y`. La caméra regarde à
//! l'horizontale, donc le fond vaut `(ciel+sol)/2` ; **une face verticale de cube vaut exactement
//! pareil**. Éclairer les faces à l'ombre éclaircissait donc le fond d'autant. Ce n'était pas un
//! réglage à trouver : c'était le même nombre.
//!
//! Un décor posé DEVANT le ciel rompt cette égalité sans toucher à l'invariant : le ciel continue
//! de porter la lumière, et ce qu'on voit au fond cesse d'être un mur de couleur.
//!
//! *Sa demande, le 31 août 2026 : « une petite jungle ? ce serait trop bien, avec un peu de
//! mouvement et bien sûr de la parallaxe ».*
//!
//! ## ⭐ La parallaxe ne coûte pas une ligne
//!
//! La caméra est en perspective (38°). Deux plans posés à des profondeurs différentes se déplacent
//! déjà à des vitesses différentes quand elle bouge — **c'est la projection qui le fait.** Écrire un
//! calcul de parallaxe reviendrait à refaire à la main ce que la matrice fait déjà, et à introduire
//! un coefficient à justifier pour toujours. *Il n'y en a aucun ici.*
//!
//! ## ⚠⚠ LE PIÈGE QUI TUE LA PARALLAXE, et il est facile à ne pas voir
//!
//! Si les plantes sont placées **par rapport à la caméra**, elles glissent avec elle : elles restent
//! immobiles à l'écran, la parallaxe disparaît, et l'on croit que la profondeur ne marche pas.
//!
//! Elles ont donc des positions **absolues dans le monde**, dérivées d'une grille régulière et d'un
//! hachage — les mêmes pour tout le monde, à toutes les images. La caméra ne fait que **choisir
//! lesquelles sont dans le champ** ; elle n'en déplace aucune. *C'est aussi ce qui évite le
//! scintillement : une plante ne peut pas changer de forme entre deux images, sa forme ne dépend
//! que de son x.*
//!
//! ## ⚠ Et la garde de LISIBILITÉ, qui prime sur le style
//!
//! **Un élément de fond ne doit jamais pouvoir être pris pour une plateforme jouable.** C'est la
//! leçon de la scie, payée le matin même : ce qui porte une information de jeu ne se sacrifie pas au
//! décor, et l'inverse est vrai aussi. Trois choses l'assurent, et aucune n'est un réglage :
//! le fond est **derrière le plan de jeu** (z négatif), il **ne porte aucune ombre**, et ses couleurs
//! **tendent vers le fond** à mesure qu'il s'éloigne.

use aegis_engine::math::{Mat4, Vec3, Vec4};

/// Le hachage du décor, repris à l'identique de `party_render_pass` : deux générateurs de hasard
/// donneraient deux grains, et l'œil verrait que le fond n'appartient pas au même monde.
fn hash(x: i32, y: i32, graine: u32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(graine);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

/// Un plan de jungle : sa profondeur, son échelle, et l'espacement de ses plantes.
///
/// ⚠ Les trois vont ENSEMBLE et ne se règlent pas séparément : un plan lointain doit être plus
/// petit **et** plus espacé, sinon il se lit comme un plan proche mal placé. Les décorréler
/// donnerait trois réglages dont deux seraient toujours faux.
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    /// La profondeur, en unités du monde. Négative : le fond est DERRIÈRE le plan de jeu (z = 0).
    pub profondeur: f32,
    /// L'échelle des plantes de ce plan.
    pub echelle: f32,
    /// Combien d'unités du monde séparent deux emplacements possibles.
    pub pas: f32,
    /// L'amplitude du balancement, en unités du monde.
    pub souffle: f32,
}

/// Les trois plans, du plus lointain au plus proche.
///
/// ⚠ **L'ordre compte** : le plus lointain est dessiné en premier. Ils n'écrivent pas de profondeur
/// les uns par rapport aux autres de façon fiable à ces distances, et dessiner le proche d'abord le
/// ferait recouvrir par le lointain.
///
/// *Trois et non cinq : chaque plan coûte des cubes, et au-delà de trois l'œil ne distingue plus les
/// vitesses. Un quatrième serait de la dépense sans information — « jamais d'excédent ».*
pub const PLANS: [Plan; 3] = [
    Plan { profondeur: -34.0, echelle: 2.2, pas: 7.0, souffle: 0.10 },
    Plan { profondeur: -20.0, echelle: 1.5, pas: 5.0, souffle: 0.16 },
    Plan { profondeur: -11.0, echelle: 1.0, pas: 3.5, souffle: 0.24 },
];

/// Jusqu'où, de part et d'autre de la caméra, on cherche des plantes.
///
/// ⚠ Ce n'est pas un rayon de vue arbitraire : c'est la demi-largeur du champ au plan le plus
/// lointain, majorée. En dessous, une plante apparaîtrait au bord de l'écran ; au-dessus, on
/// calculerait des plantes que personne ne voit. *Généreux plutôt que juste : une plante hors champ
/// coûte une matrice, une plante qui apparaît d'un coup se voit.*
const PORTEE: f32 = 60.0;

/// Ce qu'il faut dessiner pour la jungle, à cet instant et depuis cette caméra.
///
/// Rendue comme une liste plutôt qu'écrite directement dans la file : **la fonction devient pure**,
/// donc testable sans fenêtre, sans GPU et sans qu'aucune image ne soit dessinée. C'est ce qui
/// permet de vérifier la parallaxe et la garde de lisibilité par un test, pas à l'œil.
pub struct Feuille {
    pub modele: Mat4,
    pub teinte: Vec4,
}

/// Compose la jungle visible depuis `camera_x`, à l'instant `temps`.
///
/// `couleurs` donne une teinte par plan, du plus lointain au plus proche — elles viennent de la
/// palette, donc elles se règlent à l'œil comme le reste du décor.
pub fn jungle(camera_x: f32, temps: f32, couleurs: [[f32; 3]; 3]) -> Vec<Feuille> {
    let mut sortie = Vec::with_capacity(256);

    for (indice, plan) in PLANS.iter().enumerate() {
        let teinte = Vec4::new(
            couleurs[indice][0],
            couleurs[indice][1],
            couleurs[indice][2],
            1.0,
        );

        // ⚠ Les emplacements sont des ENTIERS de `pas` en coordonnées du MONDE, jamais relatifs à
        // la caméra : c'est ce qui fait qu'une plante reste où elle est pendant qu'on se déplace.
        let premier = ((camera_x - PORTEE) / plan.pas).floor() as i32;
        let dernier = ((camera_x + PORTEE) / plan.pas).ceil() as i32;

        for colonne in premier..=dernier {
            let h = hash(colonne, indice as i32, 7717);
            // Une plante sur trois manque : une haie continue se lit comme un mur, pas comme une
            // jungle. Le vide fait partie de la forme.
            if h.is_multiple_of(3) {
                continue;
            }

            let x = colonne as f32 * plan.pas + ((h >> 3) % 100) as f32 * 0.01 * plan.pas;
            // Le pied descend sous la ligne de jeu : une jungle dont on voit le sol paraît posée
            // sur une étagère.
            let pied = -6.0 - ((h >> 11) % 40) as f32 * 0.05;
            let hauteur = 4 + (h >> 17) % 4;

            // Le balancement : chaque plante a sa PHASE, tirée de son hachage. Sans elle, toute la
            // jungle oscillerait d'un seul bloc — ce qui se lit comme une erreur, pas comme du vent.
            let phase = ((h >> 23) % 628) as f32 * 0.01;
            let souffle = (temps * 0.6 + phase).sin() * plan.souffle;

            plante(
                &mut sortie,
                x,
                pied,
                plan.profondeur,
                plan.echelle,
                hauteur,
                souffle,
                teinte,
                h,
            );
        }
    }

    sortie
}

/// Une plante : un tronc fin, puis une couronne large et étalée.
///
/// ⚠ Le balancement croît avec la HAUTEUR : un tronc dont le pied bouge autant que la cime glisse
/// au lieu de plier. C'est ce qui distingue « ça bouge » de « ça vit », et ça ne coûte qu'une
/// multiplication.
#[allow(clippy::too_many_arguments)]
fn plante(
    sortie: &mut Vec<Feuille>,
    x: f32,
    pied: f32,
    z: f32,
    echelle: f32,
    hauteur: u32,
    souffle: f32,
    teinte: Vec4,
    h: u32,
) {
    let large = 0.35 * echelle;

    for etage in 0..hauteur {
        let t = etage as f32 / hauteur.max(1) as f32;
        let y = pied + etage as f32 * echelle;
        sortie.push(Feuille {
            modele: Mat4::from_translation(Vec3::new(x + souffle * t, y, z))
                * Mat4::from_scale(Vec3::new(large, echelle, large)),
            teinte,
        });
    }

    // La couronne : trois à cinq masses posées autour du sommet. Elles portent tout le souffle,
    // puisqu'elles sont au bout du tronc.
    let cime = pied + hauteur as f32 * echelle;
    let masses = 3 + (h >> 5) % 3;
    for m in 0..masses {
        let g = hash(x as i32, m as i32, 4242 + h);
        let dx = ((g % 200) as f32 * 0.01 - 1.0) * echelle;
        let dy = ((g >> 9) % 100) as f32 * 0.01 * echelle;
        let taille = (0.9 + ((g >> 17) % 70) as f32 * 0.01) * echelle;
        sortie.push(Feuille {
            modele: Mat4::from_translation(Vec3::new(x + dx + souffle, cime + dy, z))
                * Mat4::from_scale(Vec3::new(taille, taille * 0.6, taille)),
            teinte,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COULEURS: [[f32; 3]; 3] = [[0.1, 0.2, 0.15], [0.15, 0.28, 0.2], [0.2, 0.36, 0.25]];

    #[test]
    fn la_jungle_est_derriere_le_plan_de_jeu() {
        // ⚠ LA GARDE DE LISIBILITÉ, et c'est la plus importante de ce fichier : un élément de fond
        // qui remonterait à z >= 0 pourrait être pris pour une plateforme. C'est la leçon de la
        // scie — ce qui porte une information de jeu ne se confond pas avec du décor.
        for f in jungle(0.0, 0.0, COULEURS) {
            assert!(
                f.modele.cols[3].z < -5.0,
                "un element de fond est remonte a z = {}",
                f.modele.cols[3].z
            );
        }
    }

    #[test]
    fn une_plante_ne_suit_jamais_la_camera() {
        // ⚠ LE TÉMOIN DE LA PARALLAXE. Si les positions devenaient relatives à la caméra, ce test
        // tomberait — et la parallaxe aurait disparu sans que rien d'autre ne le signale : à
        // l'écran, on verrait un fond qui « suit », ce qui ressemble à un choix.
        let ici = jungle(0.0, 0.0, COULEURS);
        let la_bas = jungle(12.0, 0.0, COULEURS);

        // Les plantes communes aux deux vues doivent être aux mêmes coordonnées du monde.
        let x_ici: Vec<f32> = ici.iter().map(|f| f.modele.cols[3].x).collect();
        let communes = la_bas
            .iter()
            .filter(|f| x_ici.iter().any(|x| (x - f.modele.cols[3].x).abs() < 1e-4))
            .count();
        assert!(
            communes > 20,
            "trop peu de plantes communes ({communes}) : les positions suivent la camera"
        );
    }

    #[test]
    fn le_souffle_fait_bouger_la_cime_plus_que_le_pied() {
        // Un tronc dont le pied bouge autant que la cime glisse au lieu de plier.
        let t0 = jungle(0.0, 0.0, COULEURS);
        let t1 = jungle(0.0, 1.7, COULEURS);
        assert_eq!(t0.len(), t1.len(), "le souffle ne doit rien creer ni detruire");

        let bouge = t0
            .iter()
            .zip(t1.iter())
            .filter(|(a, b)| (a.modele.cols[3].x - b.modele.cols[3].x).abs() > 1e-4)
            .count();
        assert!(bouge > 10, "presque rien ne bouge entre deux instants ({bouge})");
    }

    #[test]
    fn la_jungle_est_deterministe() {
        // Deux appels au même instant depuis la même caméra doivent donner la même jungle : sinon
        // elle scintillerait d'une image à l'autre, et ce défaut-là se voit sans se nommer.
        let a = jungle(3.0, 0.4, COULEURS);
        let b = jungle(3.0, 0.4, COULEURS);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.modele.cols[3].x, y.modele.cols[3].x);
            assert_eq!(x.modele.cols[3].y, y.modele.cols[3].y);
        }
    }

    #[test]
    fn le_cout_reste_borne() {
        // ⚠ Le budget de la machine de référence est 13,9 ms pour DEUX yeux. Une jungle qui
        // grossirait sans qu'on le voie mangerait ce budget en silence. Ce test n'est pas une
        // opinion sur le nombre : c'est un plafond qui refuse une dérive.
        let n = jungle(0.0, 0.0, COULEURS).len();
        assert!(n > 50, "la jungle est trop pauvre pour se voir : {n} cubes");
        assert!(n < 900, "la jungle a derive a {n} cubes");
    }

    #[test]
    fn chaque_plan_porte_sa_propre_couleur() {
        // Sans ça, les trois plans se confondraient et la profondeur ne se lirait plus.
        let f = jungle(0.0, 0.0, COULEURS);
        for c in COULEURS {
            assert!(
                f.iter().any(|x| (x.teinte.x - c[0]).abs() < 1e-6),
                "la couleur {c:?} n'apparait nulle part"
            );
        }
    }
}
