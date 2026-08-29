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

// ⚠ PLUS AUCUN LIEN AVEC VULKAN ICI. Depuis que le dessin est monté dans le moteur, ce fichier
// ne connaît plus ni `ash`, ni les matrices, ni le GPU : il décrit ce que le PARTY PLATFORMER
// affiche, et confie le comment au moteur. Le compilateur l'a attesté en déclarant ces imports
// inutilisés — c'est la preuve que la séparation a bien mordu, obtenue gratuitement.
use crate::party_game::PartyGame;
// ── LE 2D EST DANS LE MOTEUR ────────────────────────────────────────────────────────────────
// Le repère d'écran, la police 5×7 et la mesure d'un texte ont été remontés dans
// `aegis_engine::ui` le 29 août 2026 : ils ne connaissent pas le party platformer, donc ils
// appartiennent au moteur. Ce renvoi n'est PAS une copie — c'est le même code, à un seul endroit.
// Ce qui reste ci-dessous parle de score, de manche et de minuteur : ça, c'est le jeu.
// Le jeu ne garde que les deux fonctions de MESURE, et c'est le partage juste : les écrans
// composent (« ce libellé tient-il ici ? »), le moteur dessine. Tout le reste — la police, le
// repère, le pinceau — a cessé d'être visible d'ici, et le compilateur l'a dit tout seul.
pub use aegis_engine::ui::{hauteur_pour_tenir, largeur_texte, Pinceau};

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

/// **Le vote, au centre de l'écran.** Il interrompt tout le reste, et c'est voulu.
///
/// Le verdict du solveur vit discrètement en bas à droite : c'est une information. Un vote est une
/// *question posée à la personne*, avec un chronomètre — s'il partageait ce coin, on le manquerait
/// exactement comme le tableau des scores a été manqué pendant des semaines, parce qu'il
/// s'affichait là où personne ne regardait.
unsafe fn bandeau_de_vote(p: &Pinceau, v: &crate::vote::Vote) {
    // ⚠ QUATRE LIGNES COURTES, PAS TROIS DONT UNE LONGUE. La première version mettait les
    // touches, le décompte et le chronomètre sur une seule ligne : la CAPTURE a montré qu'elle
    // débordait des DEUX côtés de l'écran — « O = » perdu à gauche, le chronomètre à droite.
    // Chaque ligne étant centrée séparément, rien ne le signalait dans le code.
    let lignes = [
        ("CARTE BOUCHEE".to_string(), couleurs::URGENCE),
        (format!("RETIRER LE BLOC ({},{}) ?", v.bloc.0, v.bloc.1), couleurs::TEXTE),
        ("O = OUI      N = NON".to_string(), couleurs::OR),
        // Le seuil est affiché EN CLAIR. Sans lui, « 12 pour » ne veut rien dire : le joueur ne
        // sait pas s'il manque une voix ou douze — et il a besoin de le savoir pour décider si son
        // bulletin compte encore.
        (
            format!("{} / {} REQUIS   {:.0}S", v.pour(), v.seuil(), v.reste),
            couleurs::TEXTE_FAIBLE,
        ),
    ];

    let h = 0.030;
    let marge = 0.018;
    let interligne = h * 1.6;
    let largeur = lignes
        .iter()
        .map(|(t, _)| largeur_texte(t, h))
        .fold(0.0f32, f32::max)
        + marge * 2.0;
    let hauteur = interligne * lignes.len() as f32 + marge;
    let x = (p.aspect - largeur) * 0.5;
    let y = 0.22;

    unsafe {
        // Couche haute : ce panneau passe DEVANT tout, y compris le tableau des scores.
        p.quad(x, y - marge, largeur, hauteur, couleurs::FOND, 1);
        for (i, (texte, teinte)) in lignes.iter().enumerate() {
            let lx = x + (largeur - largeur_texte(texte, h)) * 0.5; // centré ligne à ligne
            p.texte(lx, y + interligne * i as f32, h, *teinte, 2, texte);
        }
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
            // Et le symétrique : quand aucun retrait ne suffit, un marchepied peut suffire. Le
            // verbe change parce que le geste change — on ne demande pas la même chose aux gens.
            Bouchon::Ajout { x, y } => (format!("BOUCHE — AJOUTER ({x},{y}) ?"), couleurs::URGENCE),
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
    vote: Option<&crate::vote::Vote>,
    demonstration: bool,
) {
    unsafe {
        minuteur(p, game);
        pont(p, etat_pont);
        verdict_carte(p, carte, bouchon);
        // En dernier : le vote se dessine PAR-DESSUS le reste. Une question qu'on doit trancher
        // en quinze secondes ne se laisse pas recouvrir par un tableau de scores.
        if let Some(v) = vote {
            bandeau_de_vote(p, v);
        }
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
    use super::score_lisible;

    // Les tests du repère d'écran, de la police et des mesures ont suivi leur code dans
    // `aegis_engine::ui` : un test doit vivre là où vit ce qu'il éprouve, sinon le moteur
    // reste sans garde et le jeu en garde pour ce qu'il ne contient plus.


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
