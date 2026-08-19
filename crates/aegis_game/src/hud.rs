//! Le calque d'interface : ce qui se dessine **sur l'écran**, jamais dans le monde.
//!
//! # Pourquoi ce fichier existe
//!
//! Le jeu savait dessiner des objets *dans la scène*, et rien d'autre. Le tableau des scores
//! était donc posé au centre de la carte, en 3D, pendant que la caméra restait sur le joueur
//! qui venait de mourir — et il ne s'affiche pour personne : mourir ailleurs qu'au centre exact
//! de la carte suffit à ne jamais le voir. Le même trou expliquait l'absence des minuteurs de
//! phase : ils décomptaient correctement, rien ne les affichait.
//!
//! Un élément d'interface a besoin de trois choses que la scène ne donne pas : une position
//! **liée à l'écran** et non au monde, une profondeur qui le met **devant tout**, et une couleur
//! **qui ne dépend d'aucune lampe**. C'est tout ce que fournit ce module.
//!
//! # Le repère
//!
//! Origine en **haut à gauche**, `y` vers le **bas**, et l'unité est la **hauteur de l'écran** —
//! la même idée que le `vh` du web. Donc `y` va de 0,0 à 1,0, et `x` de 0,0 à `aspect`.
//!
//! Ce choix a une raison précise : il rend les formes **indépendantes du format de la fenêtre**.
//! Un carré de côté 0,05 reste un carré en 16/9 comme en 4/3, et un texte garde ses proportions.
//! Un repère en fractions de largeur ET de hauteur (le réflexe le plus courant) aurait au
//! contraire écrasé chaque lettre dès qu'on change de résolution.
//!
//! # ⚠ Le sens de l'axe Y — mesuré à l'écran, pas déduit
//!
//! Vulkan place l'origine du volume de projection **en haut** et fait descendre `y`. Ce moteur
//! ne suit pas cette convention, et rien dans son code ne le dit : ses shaders sont écrits en
//! WGSL et compilés par `naga`, dont les options par défaut portent `ADJUST_COORDINATE_SPACE`.
//! Le SPIR-V produit **retourne l'axe Y**, si bien que la convention réellement en vigueur est
//! celle de WGSL — `y` vers le HAUT.
//!
//! Cette page a d'abord été écrite dans l'autre sens, et le HUD est sorti à l'envers, en bas de
//! l'écran. **Les tests unitaires ne pouvaient pas l'attraper** : ils vérifiaient que le calcul
//! était cohérent avec la convention qu'on lui avait donnée, pas que cette convention était la
//! bonne. Seule une capture d'écran l'a dit — `aegis_game --screenshot <fichier>.png`, à lancer
//! depuis le `nix-shell` du dépôt.
//!
//! # La profondeur
//!
//! Les couches vont de 0 (le fond du HUD) à 9 (le dessus). Toutes se situent entre 0,001 et 0,01
//! en profondeur normalisée, alors que la scène ne descend jamais sous ~0,5 : le HUD passe donc
//! devant le jeu **par construction**, sans qu'aucun ordre d'appel n'ait à être respecté.

use ash::vk;
use aegis_engine::bytes::as_bytes;
use aegis_engine::math::{Mat4, Vec3, Vec4};

use crate::party_game::PartyGame;
use crate::party_render_pass::{GpuMesh, PartyPushConstants};

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

/// Écrit un score de façon lisible.
///
/// Les points du jeu sont entiers la plupart du temps (+4, +3, +1, +1 par piège), mais la
/// pénalité de celui qui survit sans finir vaut -0,5 : un score peut donc tomber sur une
/// demi-unité. On affiche « 12 » et non « 12.0 », et « 11.5 » quand la demie existe vraiment —
/// le tableau reste net sans jamais mentir sur un demi-point.
pub fn score_lisible(score: f32) -> String {
    // Le pas le plus fin du barème est la demi-unité : sous le quart, c'est un artefact de
    // flottant, pas un demi-point.
    if (score - score.round()).abs() < 0.25 {
        format!("{}", score.round() as i32)
    } else {
        format!("{score:.1}")
    }
}

/// La palette du HUD.
///
/// Sobre, à fort contraste, sans néon : ce texte doit se lire au fond d'une salle, sur un
/// vidéoprojecteur, par-dessus une scène colorée et en mouvement.
///
/// Ces couleurs sortent **telles quelles** à l'écran — contrairement au reste de la scène, elles
/// ne traversent ni lampe ni correction gamma. Inutile donc de les pré-compenser : ce qui est
/// écrit ici est ce qui s'affiche.
pub mod couleurs {
    use aegis_engine::math::Vec4;

    /// Fond des panneaux. Le shader rend tout opaque : la lisibilité ne vient donc pas d'une
    /// transparence, mais d'un fond franchement sombre.
    pub const FOND: Vec4 = Vec4::new(0.05, 0.06, 0.08, 1.0);
    /// Fond d'une ligne de tableau, un cran au-dessus du panneau.
    pub const LIGNE: Vec4 = Vec4::new(0.11, 0.12, 0.15, 1.0);
    /// Fond d'une ligne qui parle de TOI.
    pub const LIGNE_MOI: Vec4 = Vec4::new(0.17, 0.19, 0.24, 1.0);

    pub const TEXTE: Vec4 = Vec4::new(0.93, 0.94, 0.96, 1.0);
    /// Pour ce qui accompagne sans être lu en premier (un intitulé, une unité).
    pub const TEXTE_FAIBLE: Vec4 = Vec4::new(0.56, 0.59, 0.64, 1.0);

    /// Le seul accent du HUD : il ne sert qu'à ce qui compte vraiment, sinon il ne veut plus rien
    /// dire. Ici : le premier du classement, et le temps qui manque.
    pub const OR: Vec4 = Vec4::new(0.93, 0.74, 0.24, 1.0);
    pub const ARGENT: Vec4 = Vec4::new(0.78, 0.80, 0.84, 1.0);
    pub const BRONZE: Vec4 = Vec4::new(0.76, 0.51, 0.30, 1.0);
    pub const URGENCE: Vec4 = Vec4::new(0.90, 0.34, 0.28, 1.0);
}

/// Hauteur du texte du bandeau de minuteur.
pub const BANDEAU_TEXTE: f32 = 0.030;
/// Marge intérieure du bandeau de minuteur.
pub const BANDEAU_MARGE: f32 = 0.016;
/// Écart entre l'intitulé de la phase et le nombre de secondes.
pub const BANDEAU_ECART: f32 = 0.034;

/// La largeur qu'occupe le bandeau de minuteur pour un intitulé donné.
///
/// Elle est nommée, et non calculée sur place, pour une raison concrète : ce HUD tournera sur
/// trente-cinq écrans dont on ne connaît pas le format. Un intitulé de phase trop long
/// dépasserait des bords de l'écran le plus étroit — et personne ne le verrait avant la partie.
/// Un test s'en assure pour tous les intitulés du jeu.
pub fn largeur_bandeau_minuteur(libelle: &str) -> f32 {
    BANDEAU_MARGE * 2.0
        + largeur_texte(libelle, BANDEAU_TEXTE)
        + BANDEAU_ECART
        // Deux chiffres : la plus longue durée du jeu est de 150 secondes, affichée « 150 » —
        // on réserve donc trois caractères, pas deux.
        + largeur_texte("000", BANDEAU_TEXTE)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
//  Le pinceau, et ce qu'on dessine avec
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// De quoi dessiner sur l'écran : tout ce qui **ne change pas** d'un élément d'interface à
/// l'autre — le périphérique, le tampon de commandes, la disposition du pipeline, le maillage,
/// le format de la fenêtre.
///
/// Le porter à part n'est pas de la coquetterie : sans lui, chaque rectangle et chaque lettre
/// traînaient ces cinq mêmes valeurs dans leur liste d'arguments, et le HUD entier vivait au
/// milieu du rendu 3D auquel il ne doit justement rien.
pub struct Pinceau<'a> {
    pub device: &'a ash::Device,
    pub cmd: vk::CommandBuffer,
    pub layout: vk::PipelineLayout,
    pub cube: &'a GpuMesh,
    /// Largeur de la fenêtre divisée par sa hauteur.
    pub aspect: f32,
}

impl Pinceau<'_> {
    /// Un rectangle plein, en coordonnées d'écran (voir le repère en tête de module).
    ///
    /// C'est la brique **unique** du HUD : un panneau, une barre de minuteur et le trait d'un
    /// caractère sont tous ce même rectangle.
    unsafe fn quad(&self, x: f32, y: f32, largeur: f32, hauteur: f32, couleur: Vec4, couche: u8) {
        let push = PartyPushConstants {
            mvp_matrix: matrice_quad(self.aspect, x, y, largeur, hauteur, couche),
            // En couleur plate le shader ne consulte aucune normale : mettre l'identité plutôt
            // qu'une matrice recopiée évite de laisser croire qu'elle sert à quelque chose ici.
            model_matrix: Mat4::IDENTITY,
            color_tint: couleur,
            params: Vec4::new(0.0, 0.0, 0.0, COULEUR_PLATE),
        };
        unsafe {
            self.device.cmd_push_constants(
                self.cmd,
                self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                as_bytes(&push),
            );
            self.cube.draw(self.device, self.cmd);
        }
    }

    /// Écrit un texte, `x`/`y` désignant le coin haut-gauche de la première lettre. Renvoie la
    /// largeur occupée, pour enchaîner un second texte à la suite.
    unsafe fn texte(&self, x: f32, y: f32, hauteur: f32, couleur: Vec4, couche: u8, texte: &str) -> f32 {
        let point = hauteur / GLYPHE_LIGNES as f32;
        let mut plume = x;

        for c in texte.chars() {
            for (ligne, bits) in glyphe(c).iter().enumerate() {
                let mut segments = [(0u8, 0u8); 3];
                let n = segments_de_ligne(*bits, &mut segments);
                for &(depart, longueur) in &segments[..n] {
                    unsafe {
                        self.quad(
                            plume + depart as f32 * point,
                            y + ligne as f32 * point,
                            longueur as f32 * point,
                            point,
                            couleur,
                            couche,
                        );
                    }
                }
            }
            // Cinq colonnes de glyphe, plus une d'écart avec le caractère suivant.
            plume += point * (GLYPHE_COLONNES as f32 + 1.0);
        }

        largeur_texte(texte, hauteur)
    }

    /// Le même texte, centré horizontalement sur l'écran.
    unsafe fn texte_centre(&self, y: f32, hauteur: f32, couleur: Vec4, couche: u8, texte: &str) -> f32 {
        let x = (self.aspect - largeur_texte(texte, hauteur)) * 0.5;
        unsafe { self.texte(x, y, hauteur, couleur, couche, texte) }
    }
}

/// Ce que le HUD sait du pont vers le cœur réseau.
///
/// Un instantané de compteurs, jamais une socket : le rendu n'a aucune raison de connaître le
/// réseau, et le réseau aucune raison de connaître le rendu.
#[derive(Clone, Copy, Default)]
pub struct EtatPont {
    pub relie: bool,
    /// Poses que nous avons poussées vers le cœur.
    pub envoyes: u64,
    /// Instantanés que le cœur nous a renvoyés.
    pub recus: u64,
    /// Joueurs distants au dernier instantané.
    pub avatars: usize,
}

/// Le témoin du pont réseau, en bas à gauche.
///
/// Il affiche **les deux compteurs séparément**, et c'est tout l'intérêt : un pont où un seul
/// des deux monte est un pont à moitié mort, et le chiffre désigne aussitôt le sens en panne.
/// Un total unique, ou une simple pastille « connecté », masqueraient exactement ce cas.
unsafe fn pont(p: &Pinceau, etat: &EtatPont) {
    let h = 0.022;
    let marge = 0.014;
    let y = 1.0 - h - marge * 2.0;

    let texte = if etat.relie {
        format!("WEB3 {}/{} {}", etat.envoyes, etat.recus, etat.avatars)
    } else {
        "WEB3 SOLO".to_string()
    };
    let teinte = if etat.relie { couleurs::TEXTE_FAIBLE } else { couleurs::LIGNE };

    unsafe {
        let largeur = largeur_texte(&texte, h) + marge * 2.0;
        p.quad(marge, y - marge * 0.6, largeur, h + marge * 1.2, couleurs::FOND, 1);
        p.texte(marge * 2.0, y, h, teinte, 2, &texte);
    }
}

/// Ce que le solveur pense de la carte, en bas à droite.
///
/// Le libellé du doute est délibérément « PASSAGE DOUTEUX » et non « impossible » : le solveur
/// n'a pas trouvé dans son budget, ce qui n'est pas la même chose. Afficher « impossible »
/// ferait retirer des blocs parfaitement franchissables sur la foi d'une recherche trop courte —
/// et devant une classe, un mot faux à l'écran ne se rattrape pas.
unsafe fn verdict_carte(p: &Pinceau, carte: crate::tas::EtatCarte, bouchon: &crate::tas::Bouchon) {
    use crate::tas::{Bouchon, EtatCarte};

    /// En dessous, le parcours existe mais ne pardonne presque rien : on ne dit pas « OK ».
    /// Une carte franchie une fois sur deux par une machine parfaite est déjà très dure pour
    /// une classe.
    const SEUIL_IMITABLE: f32 = 0.5;

    let (texte, teinte) = match carte {
        EtatCarte::Inconnue => return,
        EtatCarte::EnCours => ("CARTE : VERIFICATION".to_string(), couleurs::TEXTE_FAIBLE),
        // ⚠ « OK » ne se dit que si le parcours est **imitable**. Une carte franchissable dont
        // la seule solution connue ne pardonne aucune imprécision n'est pas une carte réussie :
        // annoncer « OK » à trente-cinq personnes qui vont toutes mourir serait un mensonge à
        // l'écran. Le pourcentage est celui de la robustesse — voir `tas::robustesse`.
        EtatCarte::Franchissable { robustesse } if robustesse >= SEUIL_IMITABLE => (
            format!("CARTE OK {}%", (robustesse * 100.0).round() as i32),
            couleurs::TEXTE_FAIBLE,
        ),
        EtatCarte::Franchissable { robustesse } => (
            format!("CARTE DURE {}%", (robustesse * 100.0).round() as i32),
            couleurs::OR,
        ),
        // ⚠ QUAND ON SAIT QUOI RETIRER, ON LE DIT. « PASSAGE DOUTEUX » laisse trente-cinq
        // personnes devant une carte muette, à deviner quel bloc casser sur un niveau qu'elles
        // viennent de bâtir ensemble. Nommer le bloc transforme le vote en question fermée —
        // « on retire celui-là ? » — au lieu d'un choix à l'aveugle.
        //
        // Le libellé reste au conditionnel : le solveur a établi que ce retrait SUFFIT, pas qu'il
        // est juste envers celui qui a posé le bloc. Ça, c'est au vote de le dire.
        EtatCarte::PasTrouvee => match bouchon {
            Bouchon::Bloc { x, y } => (format!("BOUCHE — RETIRER ({x},{y}) ?"), couleurs::URGENCE),
            // On distingue « je n'ai pas trouvé de bloc seul » de « je n'ai pas cherché » : le
            // second se corrige en cherchant, le premier veut dire qu'il faudra en retirer
            // plusieurs. Les confondre ferait proposer un vote qui ne débloquerait rien.
            Bouchon::AucunSeul { .. } => ("BOUCHE — PAS D'UN SEUL BLOC".to_string(), couleurs::URGENCE),
            Bouchon::RienABoucher => ("PASSAGE DOUTEUX".to_string(), couleurs::URGENCE),
        },
    };

    let h = 0.022;
    let marge = 0.014;
    let largeur = largeur_texte(&texte, h) + marge * 2.0;
    let y = 1.0 - h - marge * 2.0;
    let x = p.aspect - largeur - marge;

    unsafe {
        p.quad(x, y - marge * 0.6, largeur, h + marge * 1.2, couleurs::FOND, 1);
        p.texte(x + marge, y, h, teinte, 2, &texte);
    }
}

/// Tout le HUD, dans l'ordre où il se superpose.
///
/// # Sécurité
/// Le pinceau doit porter un tampon de commandes en cours d'enregistrement, dans une passe de
/// rendu où le pipeline principal est lié.
pub unsafe fn dessiner(
    p: &Pinceau,
    game: &PartyGame,
    etat_pont: &EtatPont,
    carte: crate::tas::EtatCarte,
    bouchon: &crate::tas::Bouchon,
    demonstration: bool,
) {
    unsafe {
        minuteur(p, game);
        pont(p, etat_pont);
        verdict_carte(p, carte, bouchon);
        if game.phase == crate::party_game::GamePhase::Leaderboard {
            leaderboard(p, game, demonstration);
            if demonstration {
                annonce_demonstration(p);
            }
        }
    }
}

/// Dit pourquoi un personnage traverse la carte tout seul pendant les scores.
///
/// Sans cette ligne, la démonstration serait un fantôme inexpliqué au milieu de l'écran. Ce
/// qu'on montre n'a de valeur que si l'on sait ce qu'on regarde.
unsafe fn annonce_demonstration(p: &Pinceau) {
    let h = 0.024;
    let marge = 0.018;
    let texte = "PERSONNE N'A REUSSI - VOICI COMMENT";
    let largeur = largeur_texte(texte, h) + marge * 2.0;
    let y = 0.76;

    unsafe {
        p.quad((p.aspect - largeur) * 0.5, y - marge * 0.6, largeur, h + marge * 1.2, couleurs::FOND, 6);
        p.texte_centre(y, h, couleurs::OR, 7, texte);
    }
}

/// Ce qu'il faut faire maintenant, et combien de temps il reste pour le faire.
///
/// Les minuteurs de phase décomptaient déjà juste ; rien ne les montrait. Une partie où l'on
/// ignore qu'il reste trois secondes pour poser son objet n'est pas une partie difficile, c'est
/// une partie illisible.
unsafe fn minuteur(p: &Pinceau, game: &PartyGame) {
    let (restant, total, libelle) = game.minuteur_de_phase();
    // `ceil` et non un arrondi : tant qu'il reste un souffle de seconde, elle s'affiche. Voir le
    // « 1 » s'éteindre alors qu'on a encore le temps d'agir serait déloyal.
    let secondes = format!("{:02}", restant.max(0.0).ceil() as u32);

    let h = BANDEAU_TEXTE;
    let marge = BANDEAU_MARGE;
    let l_secondes = largeur_texte(&secondes, h);
    let largeur = largeur_bandeau_minuteur(libelle);
    let hauteur = h + marge * 1.4;

    let x0 = (p.aspect - largeur) * 0.5;
    let y0 = 0.016;
    let y_texte = y0 + marge * 0.7;

    // Sous les cinq dernières secondes, le temps cesse d'être une information de fond.
    let teinte = if restant <= 5.0 { couleurs::URGENCE } else { couleurs::TEXTE };

    unsafe {
        p.quad(x0, y0, largeur, hauteur, couleurs::FOND, 1);
        p.texte(x0 + marge, y_texte, h, couleurs::TEXTE_FAIBLE, 2, libelle);
        p.texte(x0 + largeur - marge - l_secondes, y_texte, h, teinte, 2, &secondes);

        // La barre dit d'un seul coup d'œil ce que le chiffre demande de lire.
        let epaisseur = 0.006;
        let y_barre = y0 + hauteur;
        let fraction = if total > 0.0 { (restant / total).clamp(0.0, 1.0) } else { 0.0 };
        p.quad(x0, y_barre, largeur, epaisseur, couleurs::LIGNE, 1);
        p.quad(x0, y_barre, largeur * fraction, epaisseur, teinte, 2);
    }
}

/// Le tableau des scores — **à l'écran**, donc visible quel que soit l'endroit où l'on est mort.
///
/// Il existait déjà, en 3D, posé au centre de la *carte*, pendant que la caméra restait sur le
/// cadavre du joueur : personne ne l'avait jamais vu. Les pseudos n'y figuraient pas — le nom du
/// gagnant était même extrait du jeu puis jeté sans être écrit — et les points étaient rendus par
/// des petits cubes plafonnés à dix, si bien que 4 points et 12 points donnaient exactement la
/// même image.
///
/// Le tri, lui, était juste. Il ne servait simplement à rien.
unsafe fn leaderboard(p: &Pinceau, game: &PartyGame, laisser_la_place: bool) {
    /// Au-delà, le tableau cesse d'être lisible d'un coup d'œil — et la classe compte jusqu'à
    /// trente-cinq joueurs. Le rang de chacun reste garanti visible : voir plus bas.
    const LIGNES_MAX: usize = 8;
    /// Un pseudo plus long mordrait sur la colonne des points.
    const PSEUDO_MAX: usize = 11;

    let classement = game.classement();
    if classement.is_empty() {
        return;
    }

    let mut lignes: Vec<(usize, &crate::party_game::PlayerSession)> = classement
        .iter()
        .enumerate()
        .map(|(i, j)| (i + 1, *j))
        .take(LIGNES_MAX)
        .collect();

    // Être vingt-deuxième sur trente-cinq ne doit pas vouloir dire ne rien lire de soi : si le
    // joueur n'est pas dans le haut du tableau, il prend la dernière ligne affichée, avec son
    // vrai rang.
    if !lignes.iter().any(|(_, j)| j.is_human) {
        if let Some((rang, moi)) = classement.iter().enumerate().find(|(_, j)| j.is_human) {
            if let Some(derniere) = lignes.last_mut() {
                *derniere = (rang + 1, moi);
            }
        }
    }

    let h_ligne = 0.052;
    let h_titre = 0.042;
    let h_texte = 0.028;
    let marge = 0.022;
    let largeur = 0.78;

    let hauteur = h_titre + marge * 2.4 + lignes.len() as f32 * h_ligne + marge;
    let x0 = (p.aspect - largeur) * 0.5;
    // Centré d'ordinaire ; remonté quand une démonstration passe derrière — un tableau posé
    // pile sur ce qu'on demande aux joueurs de regarder ne vaut pas mieux qu'un tableau invisible.
    // 0,085 : juste sous le bandeau du minuteur (0,016 + sa hauteur), pas dessus.
    let y0 = if laisser_la_place { 0.085 } else { (1.0 - hauteur) * 0.5 };

    let (titre, teinte_titre) = match &game.match_winner {
        Some(nom) => (format!("{nom} GAGNE"), couleurs::OR),
        None => (format!("MANCHE {}", game.round_number), couleurs::TEXTE),
    };

    unsafe {
        p.quad(x0, y0, largeur, hauteur, couleurs::FOND, 3);
        p.texte_centre(y0 + marge, h_titre, teinte_titre, 5, &titre);

        for (i, (rang, joueur)) in lignes.iter().enumerate() {
            let y = y0 + h_titre + marge * 2.4 + i as f32 * h_ligne;
            let h_barre = h_ligne - 0.008;
            let y_texte = y + (h_barre - h_texte) * 0.5;

            let fond = if joueur.is_human { couleurs::LIGNE_MOI } else { couleurs::LIGNE };
            p.quad(x0 + marge, y, largeur - marge * 2.0, h_barre, fond, 4);

            let teinte = match rang {
                1 => couleurs::OR,
                2 => couleurs::ARGENT,
                3 => couleurs::BRONZE,
                _ => couleurs::TEXTE_FAIBLE,
            };

            // Le rang s'aligne par la DROITE de sa colonne, comme les points : « 1 » et « 11 »
            // finissent au même endroit, donc le pseudo commence toujours au même x. Aligné à
            // gauche, un rang à deux chiffres venait coller à la première lettre du pseudo.
            let colonne_rang = largeur_texte("00", h_texte);
            let r = format!("{rang}");
            p.texte(
                x0 + marge * 1.9 + colonne_rang - largeur_texte(&r, h_texte),
                y_texte, h_texte, teinte, 5, &r,
            );

            let pseudo: String = joueur.name.chars().take(PSEUDO_MAX).collect();
            p.texte(
                x0 + marge * 1.9 + colonne_rang + 0.024,
                y_texte, h_texte, couleurs::TEXTE, 5, &pseudo,
            );

            // Les points s'alignent par la DROITE : deux nombres de largeurs différentes doivent
            // se comparer d'un coup d'œil, ce qu'un alignement à gauche interdit.
            let points = score_lisible(joueur.total_score);
            let l_points = largeur_texte(&points, h_texte);
            p.texte(x0 + largeur - marge * 1.9 - l_points, y_texte, h_texte, teinte, 5, &points);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_engine::math::Vec4;

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

    #[test]
    fn un_score_s_ecrit_sans_decimale_inutile() {
        assert_eq!(score_lisible(0.0), "0");
        assert_eq!(score_lisible(12.0), "12");
        assert_eq!(score_lisible(-1.0), "-1");
        // La demi-unite existe pour de vrai dans le bareme (-0,5 pour qui survit sans finir).
        assert_eq!(score_lisible(11.5), "11.5");
        assert_eq!(score_lisible(-0.5), "-0.5");
        // Un residu de flottant ne doit pas inventer une decimale.
        assert_eq!(score_lisible(3.0000001), "3");
    }
}
