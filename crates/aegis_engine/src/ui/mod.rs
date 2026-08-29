//! # L'INTERFACE 2D DU MOTEUR — le repère d'écran, la police, et ce qu'un texte occupe
//!
//! **Remonté du jeu vers le moteur le 29 août 2026**, sur sa décision : *« j'aimerai que le
//! système de 2D et le système de clic ce soit dans AEGIS, et que tu appelles ces scripts depuis
//! le jeu — mais je veux surtout pas de duplicata. L'idée c'est de créer un moteur de jeu COMPLET
//! pour la création de jeux vidéo, et donc le jeu qu'on crée n'est en fait qu'un test. »*
//!
//! Rien ici ne connaît le party platformer, et c'est le critère : une police 5×7, un repère
//! d'écran et la mesure d'un texte servent à **n'importe quel** jeu. Ce qui reste au jeu, c'est ce
//! qui parle de score, de manche ou de minuteur.
//!
//! ## Le repère
//!
//! `x ∈ [0, aspect]`, `y ∈ [0, 1]` **du haut vers le bas**. La HAUTEUR sert d'unité aux deux axes :
//! c'est ce qui fait qu'un bouton reste carré quand la fenêtre s'élargit, sans qu'aucun écran ait
//! à s'en occuper.

use crate::math::{Mat4, Vec3};


/// Profondeur de la couche la plus au fond du HUD. La scène de jeu vit vers 0,99 (caméra à 18
/// unités, plan proche à 0,1) : n'importe quelle valeur bien sous 0,5 met le HUD devant elle.
const Z_FOND: f32 = 0.010;

/// Écart de profondeur entre deux couches. Très au-dessus de la résolution d'un tampon de
/// profondeur 16 bits (~0,000015), donc deux couches voisines ne peuvent pas se confondre.
const Z_PAS: f32 = 0.001;

/// Épaisseur du pavé qui sert de rectangle. **Négative** à dessein : les faces avant et arrière
/// se dessinent toutes les deux (le pipeline n'élimine aucune face), et ce signe garantit que
/// celle qui gagne le test de profondeur est la face *avant*. En couleur plate les deux sont
/// identiques, mais un futur quad éclairé n'aurait pas à redécouvrir ce piège.
const EPAISSEUR: f32 = 0.0002;

/// Valeur à placer dans `params.w` pour obtenir une couleur **plate**, telle qu'elle a été
/// demandée — sans lampe et sans correction gamma. Voir `party_2d5.wgsl`.
pub const COULEUR_PLATE: f32 = 1.0;

/// La profondeur normalisée d'une couche de HUD. La couche 0 est au fond, la 9 au-dessus.
pub fn profondeur(couche: u8) -> f32 {
    Z_FOND - (couche.min(9) as f32) * Z_PAS
}

/// La matrice qui place un rectangle **en coordonnées d'écran**, prête à servir de `mvp`.
///
/// `x`, `y` désignent le coin **haut-gauche**, `largeur` et `hauteur` la taille — le tout dans le
/// repère décrit en tête de module. Aucune caméra n'intervient : le résultat est déjà en
/// coordonnées de projection.
pub fn matrice_quad(aspect: f32, x: f32, y: f32, largeur: f32, hauteur: f32, couche: u8) -> Mat4 {
    // Le pavé de base est centré sur l'origine et mesure 1 : on vise donc son CENTRE, et une
    // taille de 2,0 couvre l'écran entier (de -1 à +1 en coordonnées de projection).
    let centre_x = ((x + largeur * 0.5) / aspect) * 2.0 - 1.0;
    // ⚠ Le `1.0 -` n'est pas décoratif : dans ce moteur, l'axe Y du volume de projection pointe
    // vers le HAUT, à l'inverse de la convention Vulkan habituelle. Voir la note ci-dessous.
    let centre_y = 1.0 - (y + hauteur * 0.5) * 2.0;

    Mat4::from_translation(Vec3::new(centre_x, centre_y, profondeur(couche)))
        * Mat4::from_scale(Vec3::new(
            (largeur / aspect) * 2.0,
            hauteur * 2.0,
            -EPAISSEUR,
        ))
}


// ─────────────────────────────────────────────────────────────────────────────────────────────
//  La police
// ─────────────────────────────────────────────────────────────────────────────────────────────
//
// Le jeu ne savait dessiner aucun caractère — pas un seul, dans les dix mille lignes du moteur.
// C'est ce qui rendait le tableau des scores muet (les pseudos et les points n'y figuraient tout
// simplement pas : le nom du gagnant était même calculé puis jeté) et les minuteurs invisibles.
//
// Le choix retenu est une matrice de points 5×7 dessinée avec la brique qu'on a déjà : le
// rectangle d'écran. Pas d'atlas de texture, pas de fichier de police, aucune dépendance — la
// forme de chaque caractère est écrite en binaire ci-dessous, donc **lisible à l'œil dans le code
// même**, ce qui est la seule façon de relire une police sans la compiler.

/// Nombre de colonnes de la matrice d'un caractère.
pub const GLYPHE_COLONNES: u8 = 5;
/// Nombre de lignes de la matrice d'un caractère.
pub const GLYPHE_LIGNES: u8 = 7;

/// La matrice de points d'un caractère : 7 lignes, bit 4 = colonne de gauche.
///
/// Un caractère non prévu rend un **cadre plein**, jamais du vide : une lettre manquante doit se
/// voir à l'écran plutôt que disparaître en silence — sinon le trou se découvre le jour où
/// quelqu'un a un pseudo avec un accent, devant la classe.
pub fn glyphe(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        ' ' => [0, 0, 0, 0, 0, 0, 0],

        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],

        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],

        '\'' => [0b00100, 0b00100, 0b01000, 0b00000, 0b00000, 0b00000, 0b00000],
        ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110, 0b01000],
        '-' => [0b00000, 0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        '/' => [0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        // Ajoutés le 20 août 2026 après une CAPTURE D'ÉCRAN du bandeau de vote : ils s'y
        // affichaient en cadres pleins — « RETIRER LE BLOC ▯12,4▯ ? » et « O ▯ OUI ». Le glyphe
        // inconnu a fait exactement son travail, pour la deuxième fois après l'apostrophe : il
        // rend un défaut VISIBLE plutôt que du vide, qu'on ne remarque pas.
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '=' => [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000],
        // Le tiret CADRATIN, vu en cadre plein à l'écran : « BOUCHE ▯ PAS D'UN SEUL BLOC ».
        // Troisième glyphe manquant trouvé par l'œil et non par un test, après l'apostrophe et les
        // parenthèses. ⚠ Le trait d'union `-` existe déjà plus haut : l'ajouter ici aurait créé
        // une branche morte, jamais atteinte, que rien n'aurait signalé.
        '—' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],

        // Inconnu : un cadre plein. Voir la doctrine ci-dessus — se voir, jamais s'effacer.
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111],
    }
}

/// Découpe une ligne de glyphe en **segments horizontaux continus**, et renvoie leur nombre.
///
/// Chaque segment est un `(colonne_de_départ, longueur)`. Cinq colonnes ne peuvent pas contenir
/// plus de trois segments séparés (le pire cas est `X.X.X`), d'où la taille fixe : aucune
/// allocation ne se glisse dans la boucle de rendu.
///
/// L'intérêt est de dessiner `11111` comme **un** rectangle et non cinq : sur un tableau des
/// scores complet, ça divise le nombre de rectangles par plus de deux, gratuitement.
pub fn segments_de_ligne(bits: u8, sortie: &mut [(u8, u8); 3]) -> usize {
    let mut n = 0;
    let mut colonne = 0u8;
    while colonne < GLYPHE_COLONNES {
        let allume = bits & (1 << (GLYPHE_COLONNES - 1 - colonne)) != 0;
        if allume {
            let debut = colonne;
            while colonne < GLYPHE_COLONNES && bits & (1 << (GLYPHE_COLONNES - 1 - colonne)) != 0 {
                colonne += 1;
            }
            sortie[n] = (debut, colonne - debut);
            n += 1;
        } else {
            colonne += 1;
        }
    }
    n
}

/// Largeur totale d'un texte, dans le repère du HUD, pour une hauteur de caractère donnée.
///
/// Sert à **centrer sans dessiner** : le HUD a besoin de savoir où commencer avant de commencer.
pub fn largeur_texte(texte: &str, hauteur_caractere: f32) -> f32 {
    let n = texte.chars().count();
    if n == 0 {
        return 0.0;
    }
    let point = hauteur_caractere / GLYPHE_LIGNES as f32;
    // `n` caractères de 5 points, séparés par `n - 1` colonnes d'espace.
    point * (n as f32 * GLYPHE_COLONNES as f32 + (n - 1) as f32)
}

/// La hauteur de caractère qui fait tenir `texte` dans `largeur_max`.
///
/// **Sa règle, le 29 août 2026 :** *« peu importe la taille de la fenêtre, il faut que le texte ne
/// parte jamais sur les autres box »*. Elle n'était garantie nulle part — chaque écran posait son
/// texte à une abscisse et espérait. Résultat mesuré sur ses captures : `MALUS SI PERSONNE
/// N'ARRIVE` réclame 0,58 de large, une fenêtre étroite n'en laisse que 0,33, et le libellé
/// s'écrivait PAR-DESSUS ses propres boutons — illisible des deux côtés.
///
/// Corriger écran par écran aurait été une rustine : le même défaut vivait sur trois écrans à la
/// fois (le sous-titre coupé aux deux bords, le code d'accès sur son explication, la liste des
/// membres sur la sienne), et le suivant serait revenu au premier libellé un peu long. La garantie
/// vit donc **ici**, dans la seule fonction qui sait ce qu'un texte occupe.
///
/// On RÉTRÉCIT, on n'agrandit jamais : un texte court garde la taille voulue. Et l'on ne tronque
/// pas — un réglage à moitié nommé se devine mal, alors qu'un libellé plus petit se lit encore.
///
/// ⚠ Rend `0.0` quand la place est nulle ou négative. Un texte de hauteur nulle ne dessine rien,
/// ce qui vaut mieux qu'un texte qui déborde sur son voisin : l'absence se remarque, la
/// superposition se subit.
pub fn hauteur_pour_tenir(texte: &str, hauteur_voulue: f32, largeur_max: f32) -> f32 {
    if hauteur_voulue <= 0.0 || largeur_max <= 0.0 {
        return 0.0;
    }
    let voulue = largeur_texte(texte, hauteur_voulue);
    if voulue <= largeur_max {
        return hauteur_voulue;
    }
    // `largeur_texte` est proportionnelle à la hauteur : la mise à l'échelle est exacte, pas
    // approchée — aucune marge de sécurité à inventer, donc aucune constante à justifier.
    hauteur_voulue * largeur_max / voulue
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::math::Vec4;

    /// Applique la matrice à un point du pavé de base (qui va de -0,5 à +0,5 sur chaque axe).
    fn projete(m: Mat4, x: f32, y: f32) -> (f32, f32) {
        let p = m * Vec4::new(x, y, 0.0, 1.0);
        (p.x, p.y)
    }

    #[test]
    fn un_quad_plein_ecran_touche_exactement_les_quatre_bords() {
        let aspect = 16.0 / 9.0;
        let m = matrice_quad(aspect, 0.0, 0.0, aspect, 1.0, 0);

        // Le coin local (-0.5, -0.5) du pave tombe en BAS a gauche : `y` monte, dans ce moteur.
        let (gauche, bas) = projete(m, -0.5, -0.5);
        let (droite, haut) = projete(m, 0.5, 0.5);

        assert!((gauche - -1.0).abs() < 1e-5, "bord gauche: {gauche}");
        assert!((droite - 1.0).abs() < 1e-5, "bord droit: {droite}");
        assert!((haut - 1.0).abs() < 1e-5, "bord haut: {haut}");
        assert!((bas - -1.0).abs() < 1e-5, "bord bas: {bas}");
    }

    #[test]
    fn l_origine_est_en_haut_a_gauche_et_y_descend() {
        // ⚠ Ce test a longtemps affirme l'INVERSE, et il passait : il verifiait que le calcul
        // etait cohérent avec la convention qu'on lui avait donnée, pas que cette convention
        // etait celle du moteur. Le HUD sortait a l'envers et aucun test ne bronchait. Les
        // valeurs ci-dessous viennent d'une capture d'ecran reelle — voir la note en tete de
        // module sur `ADJUST_COORDINATE_SPACE`.
        let m = matrice_quad(16.0 / 9.0, 0.0, 0.0, 0.05, 0.05, 0);
        let (x, y) = projete(m, 0.0, 0.0);
        assert!(x < -0.9, "devrait coller au bord gauche, vaut {x}");
        assert!(y > 0.9, "devrait coller au bord HAUT (y positif ici), vaut {y}");

        // Et le meme carre pose en bas doit partir vers le bas : c'est le sens de `y` qui est en
        // jeu, et l'inverser est l'erreur la plus facile a commettre ici — elle a ete commise.
        let bas = matrice_quad(16.0 / 9.0, 0.0, 0.95, 0.05, 0.05, 0);
        let (_, y_bas) = projete(bas, 0.0, 0.0);
        assert!(y_bas < -0.9, "devrait coller au bord BAS, vaut {y_bas}");
    }

    #[test]
    fn un_carre_reste_carre_quand_le_format_de_l_ecran_change() {
        // C'est la raison d'être du repère : la même demande doit donner la même FORME sur un
        // écran large et sur un écran carré. On compare le rapport largeur/hauteur du résultat
        // une fois ramené en pixels (donc en re-multipliant l'axe x par l'aspect).
        for aspect in [1.0_f32, 4.0 / 3.0, 16.0 / 9.0, 21.0 / 9.0] {
            let m = matrice_quad(aspect, 0.1, 0.1, 0.2, 0.2, 0);
            let (x0, y0) = projete(m, -0.5, -0.5);
            let (x1, y1) = projete(m, 0.5, 0.5);

            let largeur_pixels = (x1 - x0) * aspect;
            let hauteur_pixels = y1 - y0;
            assert!(
                (largeur_pixels - hauteur_pixels).abs() < 1e-5,
                "aspect {aspect} : {largeur_pixels} x {hauteur_pixels} n'est pas carré"
            );
        }
    }

    #[test]
    fn une_couche_haute_passe_devant_une_couche_basse_et_tout_le_hud_devant_la_scene() {
        assert!(profondeur(9) < profondeur(0), "la couche 9 doit être devant la 0");
        assert!(profondeur(0) < 0.5, "tout le HUD doit passer devant la scène (~0,99)");
        assert!(profondeur(9) > 0.0, "rien ne doit sortir du volume de projection");

        // Saturer plutôt que déborder : une couche 200 demandée par erreur reste dessinable.
        assert_eq!(profondeur(200), profondeur(9));
    }

    #[test]
    fn un_caractere_inconnu_se_voit_au_lieu_de_disparaitre() {
        // Un pseudo avec un accent, une emoji, un caractere oublie : le jour ou ca arrive, il
        // faut le VOIR. Un glyphe vide serait un echec avale — la pire facon d'echouer ici.
        let inconnu = glyphe('\u{e9}');
        assert_ne!(inconnu, [0; 7], "un caractere non prevu ne doit jamais etre invisible");
        assert_eq!(glyphe(' '), [0; 7], "l'espace, lui, est bien vide");
    }

    #[test]
    fn les_lettres_tiennent_dans_les_cinq_colonnes() {
        // Un bit au-dela de la 5e colonne deborderait sur le caractere voisin sans que rien ne
        // le signale : on verifie toute la table d'un coup plutot que de relire a l'oeil.
        for c in "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ :.,-+!?/%'".chars() {
            for (ligne, bits) in glyphe(c).iter().enumerate() {
                assert!(
                    *bits < (1 << GLYPHE_COLONNES),
                    "'{c}' deborde a la ligne {ligne} : {bits:#07b}"
                );
            }
        }
    }

    #[test]
    fn la_casse_ne_change_rien() {
        assert_eq!(glyphe('a'), glyphe('A'));
    }

    #[test]
    fn une_ligne_se_decoupe_en_segments_continus() {
        let mut s = [(0u8, 0u8); 3];

        assert_eq!(segments_de_ligne(0b00000, &mut s), 0, "ligne vide");

        assert_eq!(segments_de_ligne(0b11111, &mut s), 1, "ligne pleine = UN rectangle");
        assert_eq!(s[0], (0, 5));

        // Le pire cas des cinq colonnes, celui qui fixe la taille du tableau.
        assert_eq!(segments_de_ligne(0b10101, &mut s), 3);
        assert_eq!(&s[..3], &[(0, 1), (2, 1), (4, 1)]);

        // Un bord droit, la ou une erreur de decalage se cacherait volontiers.
        assert_eq!(segments_de_ligne(0b00011, &mut s), 1);
        assert_eq!(s[0], (3, 2));
    }

    #[test]
    fn le_decoupage_ne_perd_ni_n_invente_aucun_point() {
        // Propriete plutot qu'exemples : sur les 32 lignes possibles, la somme des longueurs
        // des segments doit valoir exactement le nombre de bits allumes.
        for bits in 0u8..32 {
            let mut s = [(0u8, 0u8); 3];
            let n = segments_de_ligne(bits, &mut s);
            let total: u8 = s[..n].iter().map(|(_, l)| l).sum();
            assert_eq!(
                total as u32,
                bits.count_ones(),
                "ligne {bits:#07b} : {total} points dessines pour {} allumes",
                bits.count_ones()
            );
        }
    }

    #[test]
    fn la_largeur_d_un_texte_suit_le_nombre_de_caracteres() {
        assert_eq!(largeur_texte("", 0.1), 0.0);

        let un = largeur_texte("A", 0.7);   // hauteur 0,7 => un point fait 0,1
        assert!((un - 0.5).abs() < 1e-6, "un caractere = 5 points, vaut {un}");

        let deux = largeur_texte("AB", 0.7); // 5 + 1 d'espace + 5
        assert!((deux - 1.1).abs() < 1e-6, "deux caracteres = 11 points, vaut {deux}");
    }
}