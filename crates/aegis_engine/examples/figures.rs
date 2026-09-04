//! # LES FIGURES DU DOSSIER DE RECHERCHE — calculées, jamais recopiées
//!
//! Cet exemple produit les SVG de `recherche/figures/`. Il existe pour une raison précise, et
//! c'est la même que pour `examples/etat.rs` : **un texte se recopie donc il diverge, une commande
//! non.** Un graphique dessiné à la main dans un éditeur vectoriel est un chiffre de plus qui se
//! périme en silence, avec l'autorité d'une image.
//!
//! ```text
//! cargo run --release -p aegis_engine --example figures --no-default-features
//! ```
//!
//! ## ⚠ CE QUE CE FICHIER EST, ET CE QU'IL N'EST PAS
//!
//! **Il n'est PAS un banc de mesure.** Les chiffres qu'il trace sont recopiés depuis la sortie
//! des tests — donc ils peuvent, eux, se périmer. C'est pourquoi **chaque série porte, dans le
//! code, la commande exacte qui la reproduit** : la figure devient vérifiable en dix secondes au
//! lieu d'être crue.
//!
//! *Le geste juste serait que les tests écrivent leurs propres séries sur le disque et que ce
//! fichier les relise. Il n'est pas fait, et c'est une dette réelle : elle est écrite ici plutôt
//! que tue.*
//!
//! ## Les séries CALCULÉES, elles, ne peuvent pas se périmer
//!
//! Fresnel, Beer-Lambert et l'invariance des cascades sont **évalués ici, à l'exécution**, à
//! partir de leurs équations. Aucun point n'y est saisi à la main. *Une courbe analytique tracée
//! d'après une capture d'écran serait un dessin ; celle-ci est un calcul.*

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  LA PALETTE — sobre, sur papier chaud, lisible sous les deux thèmes de GitHub
// ══════════════════════════════════════════════════════════════════════════════════════════════
//
// ⚠ Le fond est peint EXPLICITEMENT. Un SVG transparent emprunte le fond du lecteur : en thème
// sombre, une encre noire sur un fond sombre disparaît entièrement. *Une figure illisible chez la
// moitié des lecteurs est une figure absente, et rien ne le signale à celui qui l'a écrite.*

const PAPIER: &str = "#f4f1ea";
const ENCRE: &str = "#1a1815";
const GRIS: &str = "#9c9488";
const GRILLE: &str = "#ddd7cb";
const TERRE: &str = "#7d4b2a";
const BLEU: &str = "#2f5d62";
const ROUGE: &str = "#8c3b2e";
const VERT: &str = "#4a6b3a";
const OR: &str = "#a07b1e";
const POLICE: &str = "sans-serif";

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  LES NOMBRES, ÉCRITS EN FRANÇAIS
// ══════════════════════════════════════════════════════════════════════════════════════════════
//
// ⚠ Ce n'est pas de la coquetterie. Tout ce projet s'écrit en français, et une figure qui affiche
// « 13.9 ms » au milieu d'un texte qui dit « 13,9 ms » fait douter le lecteur du chiffre lui-même.
// *Le séparateur des milliers est une espace INSÉCABLE : sans elle, « 262 144 » se coupe en fin
// de ligne et devient deux nombres.*

/// Un décimal à la française : virgule, et le nombre de décimales demandé.
fn fr(v: f64, decimales: usize) -> String {
    format!("{v:.*}", decimales).replace('.', ",")
}

/// Un entier avec ses milliers séparés par une espace insécable.
fn milliers(n: u64) -> String {
    let brut = n.to_string();
    let mut sortie = String::new();
    for (i, c) in brut.chars().enumerate() {
        if i > 0 && (brut.len() - i).is_multiple_of(3) {
            sortie.push('\u{202f}');
        }
        sortie.push(c);
    }
    sortie
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  UNE TOILE SVG MINIMALE
// ══════════════════════════════════════════════════════════════════════════════════════════════

struct Toile {
    largeur: f64,
    hauteur: f64,
    corps: String,
}

/// L'ancrage horizontal d'un texte.
#[derive(Clone, Copy, PartialEq)]
enum Ancre {
    Debut,
    Milieu,
    Fin,
}

impl Ancre {
    fn nom(self) -> &'static str {
        match self {
            Ancre::Debut => "start",
            Ancre::Milieu => "middle",
            Ancre::Fin => "end",
        }
    }
}

/// Ce qu'il faut pour poser un texte. Groupé en structure plutôt qu'en six paramètres : clippy
/// refuse au-delà de sept, et surtout un appel à six nombres nus ne se relit pas.
struct Texte<'a> {
    x: f64,
    y: f64,
    contenu: &'a str,
    taille: f64,
    couleur: &'a str,
    ancre: Ancre,
    gras: bool,
}

impl<'a> Texte<'a> {
    fn new(x: f64, y: f64, contenu: &'a str) -> Self {
        Texte {
            x,
            y,
            contenu,
            taille: 12.0,
            couleur: ENCRE,
            ancre: Ancre::Debut,
            gras: false,
        }
    }
    fn taille(mut self, t: f64) -> Self {
        self.taille = t;
        self
    }
    fn couleur(mut self, c: &'a str) -> Self {
        self.couleur = c;
        self
    }
    fn ancre(mut self, a: Ancre) -> Self {
        self.ancre = a;
        self
    }
    fn gras(mut self) -> Self {
        self.gras = true;
        self
    }
}

impl Toile {
    fn new(largeur: f64, hauteur: f64) -> Self {
        Toile {
            largeur,
            hauteur,
            corps: String::new(),
        }
    }

    fn rect(&mut self, x: f64, y: f64, l: f64, h: f64, remplissage: &str) {
        let _ = write!(
            self.corps,
            r#"<rect x="{x:.2}" y="{y:.2}" width="{l:.2}" height="{h:.2}" fill="{remplissage}"/>"#
        );
    }

    fn ligne(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, couleur: &str, epaisseur: f64) {
        let _ = write!(
            self.corps,
            r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{couleur}" stroke-width="{epaisseur}" stroke-linecap="round"/>"#
        );
    }

    /// Une ligne discontinue — pour tout ce qui est une RÉFÉRENCE et non une mesure : une
    /// asymptote, un budget, une vérité analytique. *Le trait plein dit « mesuré », le pointillé
    /// dit « attendu » ; les confondre visuellement, c'est laisser croire qu'on a mesuré une
    /// théorie.*
    fn pointille(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, couleur: &str, epaisseur: f64) {
        let _ = write!(
            self.corps,
            r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{couleur}" stroke-width="{epaisseur}" stroke-dasharray="5 4"/>"#
        );
    }

    fn polyligne(&mut self, points: &[(f64, f64)], couleur: &str, epaisseur: f64) {
        if points.is_empty() {
            return;
        }
        let mut d = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            let _ = write!(d, "{}{x:.2},{y:.2}", if i == 0 { "" } else { " " });
        }
        let _ = write!(
            self.corps,
            r#"<polyline points="{d}" fill="none" stroke="{couleur}" stroke-width="{epaisseur}" stroke-linejoin="round" stroke-linecap="round"/>"#
        );
    }

    fn disque(&mut self, x: f64, y: f64, r: f64, couleur: &str) {
        let _ = write!(
            self.corps,
            r#"<circle cx="{x:.2}" cy="{y:.2}" r="{r:.2}" fill="{couleur}"/>"#
        );
    }

    /// Un disque cerné de papier : sur une courbe, il se détache sans qu'on ait à l'écarter.
    fn point(&mut self, x: f64, y: f64, r: f64, couleur: &str) {
        self.disque(x, y, r + 1.6, PAPIER);
        self.disque(x, y, r, couleur);
    }

    fn texte(&mut self, t: Texte<'_>) {
        let poids = if t.gras { r#" font-weight="600""# } else { "" };
        let _ = write!(
            self.corps,
            r#"<text x="{:.2}" y="{:.2}" font-family="{POLICE}" font-size="{:.1}" fill="{}" text-anchor="{}"{poids}>{}</text>"#,
            t.x,
            t.y,
            t.taille,
            t.couleur,
            t.ancre.nom(),
            echapper(t.contenu)
        );
    }

    fn rendre(&self) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {:.0} {:.0}" width="{:.0}" height="{:.0}" role="img"><rect width="{:.0}" height="{:.0}" fill="{PAPIER}"/>{}</svg>"#,
            self.largeur,
            self.hauteur,
            self.largeur,
            self.hauteur,
            self.largeur,
            self.hauteur,
            self.corps
        )
    }
}

/// ⚠ Le `&` doit passer en premier, sinon on ré-échappe les esperluettes qu'on vient d'écrire.
fn echapper(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  UN REPÈRE — l'ossature commune à toutes les figures à axes
// ══════════════════════════════════════════════════════════════════════════════════════════════

struct Repere {
    gauche: f64,
    haut: f64,
    largeur: f64,
    hauteur: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    /// Quand l'axe vertical est logarithmique, `y_min` et `y_max` sont des LOGARITHMES décimaux.
    /// *Une échelle linéaire sur une convergence écrase tout contre l'axe et cache exactement ce
    /// qu'on cherche à montrer : le rythme de la décroissance.*
    log_y: bool,
}

impl Repere {
    fn x(&self, v: f64) -> f64 {
        self.gauche + (v - self.x_min) / (self.x_max - self.x_min) * self.largeur
    }

    fn y(&self, v: f64) -> f64 {
        let v = if self.log_y { v.log10() } else { v };
        self.haut + self.hauteur - (v - self.y_min) / (self.y_max - self.y_min) * self.hauteur
    }

    fn cadre(&self, t: &mut Toile) {
        t.ligne(
            self.gauche,
            self.haut + self.hauteur,
            self.gauche + self.largeur,
            self.haut + self.hauteur,
            ENCRE,
            1.2,
        );
        t.ligne(self.gauche, self.haut, self.gauche, self.haut + self.hauteur, ENCRE, 1.2);
    }

    fn graduation_y(&self, t: &mut Toile, valeurs: &[f64], format: impl Fn(f64) -> String) {
        for &v in valeurs {
            let y = self.y(v);
            t.ligne(self.gauche, y, self.gauche + self.largeur, y, GRILLE, 0.8);
            t.texte(
                Texte::new(self.gauche - 8.0, y + 4.0, &format(v))
                    .taille(11.0)
                    .couleur(GRIS)
                    .ancre(Ancre::Fin),
            );
        }
    }

    fn graduation_x(&self, t: &mut Toile, valeurs: &[f64], format: impl Fn(f64) -> String) {
        for &v in valeurs {
            let x = self.x(v);
            t.ligne(x, self.haut + self.hauteur, x, self.haut + self.hauteur + 5.0, GRIS, 1.0);
            t.texte(
                Texte::new(x, self.haut + self.hauteur + 20.0, &format(v))
                    .taille(11.0)
                    .couleur(GRIS)
                    .ancre(Ancre::Milieu),
            );
        }
    }
}

/// Le titre et le sous-titre d'une figure, toujours au même endroit.
fn entete(t: &mut Toile, titre: &str, sous_titre: &str) {
    t.texte(Texte::new(28.0, 32.0, titre).taille(15.0).gras());
    if !sous_titre.is_empty() {
        t.texte(
            Texte::new(28.0, 52.0, sous_titre)
                .taille(11.5)
                .couleur(GRIS),
        );
    }
}

/// ⚠⚠ La NATURE de chaque figure, en pied de page, et elle n'est jamais facultative.
///
/// *Mesuré, calculé, ou lu chez quelqu'un : trois statuts qu'un lecteur ne peut pas deviner d'une
/// courbe, et dont dépend entièrement ce qu'elle autorise à conclure.*
fn pied(t: &mut Toile, nature: &str) {
    t.texte(
        Texte::new(28.0, t.hauteur - 14.0, nature)
            .taille(10.0)
            .couleur(GRIS),
    );
}

/// Une pastille de légende.
fn legende(t: &mut Toile, x: f64, y: f64, couleur: &str, etiquette: &str) {
    t.ligne(x, y - 4.0, x + 18.0, y - 4.0, couleur, 2.4);
    t.texte(Texte::new(x + 24.0, y, etiquette).taille(11.0));
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  DES OUTILS DE MISE EN PAGE — parce qu'un texte qui déborde est un texte perdu
// ══════════════════════════════════════════════════════════════════════════════════════════════

/// La largeur de toutes les figures. **Elle vaut 920 et pas davantage** : GitHub affiche le
/// contenu d'un document dans une colonne d'environ 830 pixels, et une image plus large y est
/// réduite — donc son texte rétrécit avec elle. *Une figure trop grande ne se voit pas mieux,
/// elle se lit moins bien.*
const LARGEUR: f64 = 920.0;

/// Où commence la colonne d'annotations, à droite du graphe.
const COLONNE: f64 = 664.0;

/// Découpe un texte en lignes d'au plus `largeur` caractères, sans couper de mot.
fn decouper(texte: &str, largeur: usize) -> Vec<String> {
    let mut lignes = Vec::new();
    let mut courante = String::new();
    for mot in texte.split(' ') {
        if !courante.is_empty() && courante.chars().count() + mot.chars().count() + 1 > largeur {
            lignes.push(std::mem::take(&mut courante));
        }
        if !courante.is_empty() {
            courante.push(' ');
        }
        courante.push_str(mot);
    }
    if !courante.is_empty() {
        lignes.push(courante);
    }
    lignes
}

/// Un paragraphe posé dans la colonne de droite, replié à la bonne largeur, et qui rend le `y`
/// atteint — pour que le bloc suivant sache où commencer sans qu'on ait à compter les lignes
/// à la main. *Compter les lignes à la main est exactement ce qui fait chevaucher deux textes
/// le jour où l'un d'eux gagne un mot.*
fn paragraphe(t: &mut Toile, y: f64, texte: &str, couleur: &str) -> f64 {
    let mut y = y;
    for ligne in decouper(texte, 32) {
        t.texte(Texte::new(COLONNE, y, &ligne).taille(10.5).couleur(couleur));
        y += 14.5;
    }
    y + 8.0
}

/// Un chiffre mis en avant, avec ce qu'il mesure en dessous. Rend le `y` atteint.
fn chiffre(t: &mut Toile, y: f64, valeur: &str, quoi: &str, detail: &str, couleur: &str) -> f64 {
    t.texte(Texte::new(COLONNE, y, valeur).taille(19.0).couleur(couleur).gras());
    t.texte(Texte::new(COLONNE, y + 16.0, quoi).taille(10.5));
    let mut suite = y + 30.0;
    if !detail.is_empty() {
        t.texte(Texte::new(COLONNE, suite, detail).taille(9.5).couleur(GRIS));
        suite += 14.0;
    }
    suite + 14.0
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 1 — LA CONVERGENCE DE NEWTON
// ══════════════════════════════════════════════════════════════════════════════════════════════
//
// Source, rejouable :
//   nix-shell shell.nix --run "cargo test --release -p aegis_engine --lib \
//     newton_trouve_la_sortie -- --nocapture"
// Relevé du 4 septembre 2026.

fn figure_newton() -> String {
    // (pas, erreur moyenne en degrés, pire cas en degrés)
    let serie = [
        (0.0, 1.811, 19.790),
        (1.0, 1.309, 41.634),
        (2.0, 0.285, 16.566),
        (3.0, 0.126, 2.035),
        (4.0, 0.115, 0.719),
        (5.0, 0.115, 0.719),
    ];

    let mut t = Toile::new(LARGEUR, 440.0);
    entete(
        &mut t,
        "La convergence de Newton — où le rayon ressort",
        "Erreur angulaire contre la vérité analytique d'une sphère. Échelle logarithmique.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 82.0,
        largeur: 540.0,
        hauteur: 272.0,
        x_min: -0.25,
        x_max: 5.25,
        // ⚠ Le bas de l'échelle descend sous le plancher mesuré : sans cette marge,
        // l'étiquette du plancher tombe sur l'axe des abscisses et devient illisible.
        y_min: (0.05f64).log10(),
        y_max: (60.0f64).log10(),
        log_y: true,
    };

    r.graduation_y(&mut t, &[0.1, 0.3, 1.0, 3.0, 10.0, 30.0], |v| {
        format!("{}°", fr(v, if v < 1.0 { 1 } else { 0 }))
    });
    r.graduation_x(&mut t, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], |v| format!("{v:.0}"));
    r.cadre(&mut t);

    // Le plancher, tracé AVANT les courbes : une référence passe dessous, jamais dessus.
    let y_plancher = r.y(0.115);
    t.pointille(r.gauche, y_plancher, r.gauche + r.largeur, y_plancher, BLEU, 1.2);
    t.texte(
        Texte::new(r.gauche + 8.0, y_plancher + 18.0, "plancher 0,115° — la discrétisation, pas la méthode")
            .taille(10.5)
            .couleur(BLEU),
    );

    let moyennes: Vec<(f64, f64)> = serie.iter().map(|&(p, m, _)| (r.x(p), r.y(m))).collect();
    let pires: Vec<(f64, f64)> = serie.iter().map(|&(p, _, w)| (r.x(p), r.y(w))).collect();

    t.polyligne(&pires, GRIS, 1.6);
    t.polyligne(&moyennes, TERRE, 2.6);
    for &(x, y) in &pires {
        t.point(x, y, 3.0, GRIS);
    }
    for &(x, y) in &moyennes {
        t.point(x, y, 4.0, TERRE);
    }

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "nombre de pas de Newton")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    legende(&mut t, COLONNE, 104.0, TERRE, "moyenne");
    legende(&mut t, COLONNE, 126.0, GRIS, "pire cas");
    let y = chiffre(&mut t, 176.0, "× 15,7", "en quatre pas", "1,811° → 0,115°", TERRE);
    let y = paragraphe(
        &mut t,
        y,
        "Le pire cas REMONTE au pas 1 : la sonde change de régime, elle ne se dégrade pas.",
        ROUGE,
    );
    paragraphe(
        &mut t,
        y,
        "Ces rayons-là passent d'un côté à l'autre de l'angle critique, où la nature elle-même est discontinue.",
        GRIS,
    );

    pied(
        &mut t,
        "MESURÉ le 4 sept. 2026 · cargo test --release -p aegis_engine --lib newton_trouve_la_sortie -- --nocapture",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 2 — LE PRIX DE LA DISCRÉTISATION EN PIXELS
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_discretisation() -> String {
    // (côté de l'image, erreur moyenne en degrés, pixels de verre mesurés)
    let serie: [(f64, f64, u64); 3] =
        [(128.0, 3.168, 2584), (256.0, 2.132, 10280), (512.0, 1.917, 41140)];

    let mut t = Toile::new(LARGEUR, 440.0);
    entete(
        &mut t,
        "Le prix de la discrétisation — et la preuve que c'en est bien une",
        "L'écart décroît quand les pixels rétrécissent : une faute de repère, elle, y serait indifférente.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 82.0,
        largeur: 520.0,
        hauteur: 268.0,
        x_min: 6.7,
        x_max: 9.3,
        y_min: 1.55,
        y_max: 3.45,
        log_y: false,
    };

    r.graduation_y(&mut t, &[1.6, 2.0, 2.4, 2.8, 3.2], |v| format!("{}°", fr(v, 1)));
    r.graduation_x(&mut t, &[7.0, 8.0, 9.0], |v| {
        format!("{}²", (2.0f64).powf(v) as i64)
    });
    r.cadre(&mut t);

    // Les deux références. ⚠ Elles ne diffèrent que de 0,049° : leurs étiquettes se
    // chevaucheraient si on les posait toutes deux au-dessus de leur trait. On en met une
    // au-dessus, l'autre en dessous.
    for (valeur, couleur, etiquette, decalage) in [
        (1.789, BLEU, "1,789° — le shader calcule sa sphère lui-même", -8.0),
        (1.740, VERT, "1,740° — le banc processeur, la même vérité", 16.0),
    ] {
        let y = r.y(valeur);
        t.pointille(r.gauche, y, r.gauche + r.largeur, y, couleur, 1.2);
        t.texte(
            Texte::new(r.gauche + 10.0, y + decalage, etiquette)
                .taille(10.5)
                .couleur(couleur),
        );
    }

    let points: Vec<(f64, f64)> = serie
        .iter()
        .map(|&(c, e, _)| (r.x(c.log2()), r.y(e)))
        .collect();
    t.polyligne(&points, TERRE, 2.6);
    for (i, &(x, y)) in points.iter().enumerate() {
        t.point(x, y, 4.5, TERRE);
        t.texte(
            Texte::new(x, y - 15.0, &format!("{}°", fr(serie[i].1, 3)))
                .taille(11.5)
                .couleur(TERRE)
                .ancre(Ancre::Milieu)
                .gras(),
        );
        // ⚠ Le compte de pixels va SOUS le point pour les deux premiers et AU-DESSUS pour le
        // dernier : en bas à droite, il tomberait sur les lignes de référence.
        let dy = if i + 1 == points.len() { -32.0 } else { 22.0 };
        t.texte(
            Texte::new(x, y + dy, &format!("{} px de verre", milliers(serie[i].2)))
                .taille(9.5)
                .couleur(GRIS)
                .ancre(Ancre::Milieu),
        );
    }

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "résolution de la carte")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    let y = paragraphe(
        &mut t,
        110.0,
        "Le test n'exige AUCUN chiffre absolu — seulement la décroissance.",
        ENCRE,
    );
    let y = paragraphe(
        &mut t,
        y,
        "Un seuil se périmerait à la première carte graphique différente ; une tendance, non.",
        GRIS,
    );
    paragraphe(
        &mut t,
        y + 10.0,
        "Et la sonde tranche : une faute de repère serait indifférente à la taille des pixels.",
        GRIS,
    );

    pied(
        &mut t,
        "MESURÉ le 4 sept. 2026 · test l_erreur_des_cartes_retrecit_avec_les_pixels_donc_c_est_bien_la_discretisation",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 3 — CE QUE NEWTON COÛTE, COMPTÉ EN LECTURES DE CARTE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_lectures() -> String {
    // (lectures de carte, part des pixels en %)
    let serie = [(2, 11.8), (3, 55.4), (4, 25.9), (5, 5.6), (6, 1.0), (7, 0.1)];

    let mut t = Toile::new(LARGEUR, 410.0);
    entete(
        &mut t,
        "Ce que Newton coûte — en lectures de carte, pas en millisecondes",
        "Une milliseconde ne se transpose pas d'une machine à l'autre ; un nombre de lectures, si.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 82.0,
        largeur: 520.0,
        hauteur: 246.0,
        x_min: 1.4,
        x_max: 7.6,
        y_min: 0.0,
        y_max: 60.0,
        log_y: false,
    };

    r.graduation_y(&mut t, &[0.0, 15.0, 30.0, 45.0, 60.0], |v| format!("{v:.0} %"));
    r.graduation_x(&mut t, &[2.0, 3.0, 4.0, 5.0, 6.0, 7.0], |v| format!("{v:.0}"));
    r.cadre(&mut t);

    let demi = r.largeur / (r.x_max - r.x_min) * 0.34;
    for &(lectures, part) in &serie {
        let cx = r.x(lectures as f64);
        let y = r.y(part);
        let bas = r.y(0.0);
        t.rect(cx - demi, y, demi * 2.0, bas - y, TERRE);
        t.texte(
            Texte::new(cx, y - 8.0, &format!("{} %", fr(part, 1)))
                .taille(11.0)
                .couleur(ENCRE)
                .ancre(Ancre::Milieu)
                .gras(),
        );
    }

    // La moyenne : c'est elle, et non le pire cas, qui se paie sur toute une image.
    let x_moyenne = r.x(3.28);
    t.pointille(x_moyenne, r.haut, x_moyenne, r.haut + r.hauteur, BLEU, 1.6);
    t.texte(
        Texte::new(x_moyenne + 8.0, r.haut + 16.0, "3,28 lectures en moyenne")
            .taille(11.0)
            .couleur(BLEU)
            .gras(),
    );

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "lectures de carte par pixel de verre")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    let mut y = 104.0;
    for (valeur, quoi, detail) in [
        ("100 280", "pixels de bille mesurés", "un gros plan, donc le pire cas"),
        ("97,3 %", "convergent d'eux-mêmes", "le budget de 8 pas ne les coupe pas"),
        ("1,735°", "d'erreur atteinte", "contre la vérité analytique"),
    ] {
        y = chiffre(&mut t, y, valeur, quoi, detail, TERRE);
    }
    paragraphe(
        &mut t,
        y - 6.0,
        "Un budget de 8 pas ne veut pas dire 8 lectures : il dit AU PLUS 8. C'est la distribution qui décide du coût.",
        GRIS,
    );

    pied(
        &mut t,
        "MESURÉ le 4 sept. 2026 · test le_cout_de_newton_se_compte_en_lectures_pas_en_millisecondes",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 4 — LES ÉQUATIONS DE FRESNEL, CALCULÉES ICI
// ══════════════════════════════════════════════════════════════════════════════════════════════
//
// ⭐ Cette courbe n'est PAS relevée : elle est évaluée à l'exécution depuis les équations. Le seul
// point mesuré de la figure est celui du GPU à incidence normale.

fn figure_fresnel() -> String {
    let n1 = 1.0f64;
    let n2 = 1.5f64;
    let eta = n1 / n2;

    /// `Rs`, `Rp` pour un angle d'incidence donné, en venant du vide.
    fn reflectances(theta: f64, eta: f64) -> (f64, f64) {
        let cos_i = theta.cos();
        let sin_t2 = eta * eta * (1.0 - cos_i * cos_i);
        if sin_t2 >= 1.0 {
            return (1.0, 1.0);
        }
        let cos_t = (1.0 - sin_t2).sqrt();
        let rs = (eta * cos_i - cos_t) / (eta * cos_i + cos_t);
        let rp = (eta * cos_t - cos_i) / (eta * cos_t + cos_i);
        (rs * rs, rp * rp)
    }

    let mut t = Toile::new(LARGEUR, 450.0);
    entete(
        &mut t,
        "Fresnel exact — et ce que l'approximation de Schlick ne donne pas",
        "n₁ = 1,0 → n₂ = 1,5. Les deux polarisations séparées, et l'angle de Brewster qui tombe de l'équation.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 82.0,
        largeur: 520.0,
        hauteur: 282.0,
        x_min: 0.0,
        x_max: 90.0,
        y_min: 0.0,
        y_max: 1.0,
        log_y: false,
    };

    r.graduation_y(&mut t, &[0.0, 0.25, 0.5, 0.75, 1.0], |v| format!("{:.0} %", v * 100.0));
    r.graduation_x(&mut t, &[0.0, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0], |v| {
        format!("{v:.0}°")
    });
    r.cadre(&mut t);

    let mut c_s = Vec::new();
    let mut c_p = Vec::new();
    let mut c_m = Vec::new();
    let mut c_schlick = Vec::new();
    let f0 = ((n1 - n2) / (n1 + n2)).powi(2);
    for i in 0..=900 {
        let deg = i as f64 * 0.1;
        let (rs, rp) = reflectances(deg.to_radians(), eta);
        c_s.push((r.x(deg), r.y(rs)));
        c_p.push((r.x(deg), r.y(rp)));
        c_m.push((r.x(deg), r.y(0.5 * (rs + rp))));
        let schlick = f0 + (1.0 - f0) * (1.0 - deg.to_radians().cos()).powi(5);
        c_schlick.push((r.x(deg), r.y(schlick)));
    }

    t.polyligne(&c_schlick, GRIS, 1.4);
    t.polyligne(&c_s, BLEU, 1.8);
    t.polyligne(&c_p, ROUGE, 1.8);
    t.polyligne(&c_m, TERRE, 2.8);

    // L'angle de Brewster : Rp s'annule, et personne ne l'a écrit.
    let brewster = (n2 / n1).atan().to_degrees();
    let xb = r.x(brewster);
    t.pointille(xb, r.y(0.0), xb, r.haut + 34.0, ROUGE, 1.1);
    t.texte(
        Texte::new(xb, r.haut + 28.0, &format!("Brewster {}° — Rp = 0", fr(brewster, 1)))
            .taille(10.5)
            .couleur(ROUGE)
            .ancre(Ancre::Milieu),
    );

    // ── Le seul point MESURÉ de cette figure ──
    // ⚠ Son annotation ne peut PAS aller à droite du point : les quatre courbes y passent, à
    // quelques pour cent les unes des autres. Elle monte donc, avec un trait de rappel.
    let y0 = r.y(0.03922);
    t.ligne(r.x(0.0) + 4.0, y0 - 4.0, r.x(4.0), y0 - 52.0, ENCRE, 0.8);
    t.point(r.x(0.0), y0, 5.0, ENCRE);
    t.texte(
        Texte::new(r.x(4.0) + 6.0, y0 - 56.0, "3,922 % mesurés sur le GPU")
            .taille(11.0)
            .couleur(ENCRE)
            .gras(),
    );
    t.texte(
        Texte::new(r.x(4.0) + 6.0, y0 - 42.0, "contre 4,000 % analytiques — un pas de quantification")
            .taille(10.0)
            .couleur(GRIS),
    );

    legende(&mut t, COLONNE, 104.0, TERRE, "moyenne (non polarisée)");
    legende(&mut t, COLONNE, 126.0, BLEU, "Rs — perpendiculaire");
    legende(&mut t, COLONNE, 148.0, ROUGE, "Rp — parallèle");
    legende(&mut t, COLONNE, 170.0, GRIS, "Schlick (approximation)");

    let y = paragraphe(
        &mut t,
        208.0,
        "Schlick se trompe de moins d'un pour cent et coûte moins cher. C'est le choix de toute l'industrie.",
        GRIS,
    );
    paragraphe(
        &mut t,
        y,
        "Ce qu'il ne peut pas donner : la séparation Rs/Rp, l'angle de Brewster, et la polarisation le jour où on la voudra — sans rien réécrire.",
        ENCRE,
    );

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "angle d'incidence")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    pied(
        &mut t,
        "CALCULÉ à l'exécution depuis les équations · le point noir est MESURÉ (test la_reflectance_de_fresnel_vaut_quatre_pour_cent_de_face)",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 5 — BEER-LAMBERT : LA COULEUR NAÎT D'UN ÉCART, PAS D'UNE TEINTE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_beer_lambert() -> String {
    // Trois coefficients d'extinction, un par canal. Aucune teinte n'est écrite : c'est leur
    // écart qui fait la couleur.
    let sigmas = [
        (0.35f64, ROUGE, "σ_R = 0,35"),
        (0.9, VERT, "σ_V = 0,90"),
        (2.1, BLEU, "σ_B = 2,10"),
    ];

    let mut t = Toile::new(LARGEUR, 470.0);
    entete(
        &mut t,
        "Beer-Lambert — aucune couleur n'est écrite nulle part",
        "T(d) = exp(−σ·d), un σ par canal. La teinte est une conséquence de la LONGUEUR traversée.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 82.0,
        largeur: 520.0,
        hauteur: 250.0,
        x_min: 0.0,
        x_max: 4.0,
        y_min: 0.0,
        y_max: 1.0,
        log_y: false,
    };

    r.graduation_y(&mut t, &[0.0, 0.25, 0.5, 0.75, 1.0], |v| format!("{:.0} %", v * 100.0));
    r.graduation_x(&mut t, &[0.0, 1.0, 2.0, 3.0, 4.0], |v| format!("{v:.0}"));
    r.cadre(&mut t);

    for &(sigma, couleur, _) in &sigmas {
        let mut courbe = Vec::new();
        for i in 0..=400 {
            let d = i as f64 * 0.01;
            courbe.push((r.x(d), r.y((-sigma * d).exp())));
        }
        t.polyligne(&courbe, couleur, 2.4);
    }

    // Trois épaisseurs, et la teinte qui en sort — calculée, jamais choisie.
    for d in [0.5f64, 1.5, 3.0] {
        let x = r.x(d);
        t.pointille(x, r.haut, x, r.haut + r.hauteur, GRILLE, 1.0);
        let canaux: Vec<f64> = sigmas.iter().map(|&(s, _, _)| (-s * d).exp()).collect();
        let hexa = format!(
            "#{:02x}{:02x}{:02x}",
            (canaux[0] * 255.0) as u8,
            (canaux[1] * 255.0) as u8,
            (canaux[2] * 255.0) as u8
        );
        t.rect(x - 15.0, r.haut + r.hauteur + 16.0, 30.0, 30.0, &hexa);
        t.texte(
            Texte::new(x, r.haut + r.hauteur + 60.0, &format!("d = {}", fr(d, 1)))
                .taille(10.0)
                .couleur(GRIS)
                .ancre(Ancre::Milieu),
        );
    }

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 88.0, "longueur de matière traversée")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    for (i, &(_, couleur, etiquette)) in sigmas.iter().enumerate() {
        legende(&mut t, COLONNE, 104.0 + i as f64 * 22.0, couleur, etiquette);
    }
    let y = paragraphe(
        &mut t,
        190.0,
        "Le bord d'un objet est plus clair parce que le trajet y est plus court — et personne ne l'a codé.",
        ENCRE,
    );
    paragraphe(
        &mut t,
        y,
        "C'est la même formule qui donne le vert des tranches de verre : le fer y absorbe le rouge sur une longueur que la face ne montre pas.",
        GRIS,
    );

    pied(&mut t, "CALCULÉ à l'exécution · les σ sont choisis pour l'illustration, la formule est celle du moteur");
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 6 — LE BUDGET DU QUEST 2, ET POURQUOI IL EST PLUS DUR QUE LE TÉLÉPHONE À 99 $
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_budget() -> String {
    // (machine, FLOP par pixel, octets par pixel)
    // Sources : Adreno 610/830 = chiffres HypeHype, SIGGRAPH 2025. Adreno 650 du Quest 2 =
    // ~900 GFLOPS / ~44 Go/s, specs PUBLIÉES reprises de seconde main, jamais vérifiées.
    let machines = [
        ("téléphone à 99 $\n1080p · 30 Hz · 1 œil", 4032.0, 242.0),
        ("Meta Quest 2\n1832×1920 · 72 Hz · 2 yeux", 1778.0, 87.0),
    ];

    let mut t = Toile::new(LARGEUR, 452.0);
    entete(
        &mut t,
        "Le casque est plus contraint que le téléphone bon marché",
        "Le réflexe dit l'inverse. Le calcul dit : 2,3× moins de calcul et 2,8× moins de mémoire PAR PIXEL.",
    );

    let bas = 306.0;
    let hauteur_max = 190.0;

    for (colonne, (titre, unite, max)) in [("FLOP par pixel", "FLOP", 4200.0), ("octets par pixel", "o", 260.0)]
        .iter()
        .enumerate()
    {
        let ox = 130.0 + colonne as f64 * 420.0;
        t.texte(
            Texte::new(ox + 110.0, 96.0, titre)
                .taille(12.5)
                .ancre(Ancre::Milieu)
                .gras(),
        );

        for (i, (nom, flop, octets)) in machines.iter().enumerate() {
            let valeur = if colonne == 0 { *flop } else { *octets };
            let h = valeur / max * hauteur_max;
            let x = ox + i as f64 * 124.0;
            let couleur = if i == 0 { GRIS } else { TERRE };
            t.rect(x, bas - h, 88.0, h, couleur);
            t.texte(
                Texte::new(x + 44.0, bas - h - 10.0, &format!("{} {unite}", milliers(valeur as u64)))
                    .taille(12.0)
                    .ancre(Ancre::Milieu)
                    .gras(),
            );
            for (l, ligne) in nom.split('\n').enumerate() {
                t.texte(
                    Texte::new(x + 44.0, bas + 20.0 + l as f64 * 13.0, ligne)
                        .taille(if l == 0 { 11.0 } else { 9.5 })
                        .couleur(if l == 0 { ENCRE } else { GRIS })
                        .ancre(Ancre::Milieu),
                );
            }
        }

        let rapport = if colonne == 0 {
            machines[0].1 / machines[1].1
        } else {
            machines[0].2 / machines[1].2
        };
        t.texte(
            Texte::new(ox + 110.0, bas + 66.0, &format!("÷ {}", fr(rapport, 1)))
                .taille(16.0)
                .couleur(ROUGE)
                .ancre(Ancre::Milieu)
                .gras(),
        );
    }

    t.texte(
        Texte::new(28.0, 408.0, "Une image entière : 13,9 ms pour deux yeux. La bande passante est la ressource rare, pas le calcul.")
            .taille(11.5)
            .couleur(ENCRE),
    );

    pied(
        &mut t,
        "⚠ CALCULÉ à partir de specs publiées de seconde main — jamais mesuré. Aucun Quest 2 n'a jamais fait tourner Aegis, et il n'y en aura jamais.",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 7 — CE QUE LA MESURE DE TRAVERSE ÉLIMINE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_traverse() -> String {
    let mut t = Toile::new(LARGEUR, 428.0);
    entete(
        &mut t,
        "L'ordre de grandeur qui ferme une famille entière",
        "La GI stochastique mesurée par des gens dont c'est le métier, contre notre budget total.",
    );

    let gauche = 330.0;
    let largeur = 460.0;
    let echelle = 15.0;

    // (étiquette, valeur en ms, couleur, précisions)
    let barres = [
        (
            "GI diffuse SEULE — Traverse",
            10.0,
            ROUGE,
            "Adreno 840 (2026) · lancer de rayons MATÉRIEL · 1 œil · 1170×540, GI tracée au quart · 27 im/s",
        ),
        (
            "notre budget TOTAL",
            13.9,
            BLEU,
            "Adreno 650 (2020) · sans lancer de rayons · 2 yeux · 72 Hz · tout compris",
        ),
    ];

    for (i, (nom, valeur, couleur, detail)) in barres.iter().enumerate() {
        let y = 108.0 + i as f64 * 96.0;
        let l = valeur / echelle * largeur;
        t.rect(gauche, y, l, 34.0, couleur);
        t.texte(
            Texte::new(gauche + l + 10.0, y + 23.0, &format!("{} ms", fr(*valeur, 1)))
                .taille(14.0)
                .couleur(couleur)
                .gras(),
        );
        t.texte(
            Texte::new(gauche - 12.0, y + 16.0, nom)
                .taille(12.0)
                .ancre(Ancre::Fin)
                .gras(),
        );
        for (l2, ligne) in decouper(detail, 42).iter().enumerate() {
            t.texte(
                Texte::new(gauche - 12.0, y + 34.0 + l2 as f64 * 12.0, ligne)
                    .taille(9.5)
                    .couleur(GRIS)
                    .ancre(Ancre::Fin),
            );
        }
    }

    t.ligne(gauche, 100.0, gauche, 296.0, GRIS, 1.0);
    for v in [0.0f64, 5.0, 10.0, 15.0] {
        let x = gauche + v / echelle * largeur;
        t.ligne(x, 296.0, x, 301.0, GRIS, 1.0);
        t.texte(
            Texte::new(x, 315.0, &format!("{v:.0} ms"))
                .taille(10.5)
                .couleur(GRIS)
                .ancre(Ancre::Milieu),
        );
    }

    let conclusion = [
        "Le meilleur GPU mobile de 2026, avec du lancer de rayons matériel, à un onzième de notre nombre de",
        "pixels, pour UN œil, dépense les trois quarts de tout notre budget rien qu'en lumière indirecte diffuse —",
        "et n'atteint pas trente images par seconde.",
    ];
    for (i, l) in conclusion.iter().enumerate() {
        t.texte(
            Texte::new(28.0, 344.0 + i as f64 * 15.0, l)
                .taille(11.0)
                .couleur(if i == 2 { ROUGE } else { ENCRE }),
        );
    }

    pied(
        &mut t,
        "LU chez quelqu'un · J. de Winther, « Towards Real-time Dynamic GI on Mobile », Moving Mobile Graphics, SIGGRAPH 2026",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 8 — LES DIX-HUIT PHÉNOMÈNES, ET CE QUE LE MOTEUR EN EXPRIME
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_phenomenes() -> String {
    // (numéro, nom, état) — état : 2 = exprimé, 1 = partiel, 0 = absent.
    // Source : l'étalon de la matière, relu dans le code le 4 septembre 2026.
    let phenomenes = [
        (1, "Intégration de l'hémisphère", 1),
        (2, "Fresnel", 2),
        (3, "Polarisation", 0),
        (4, "Réfraction à deux interfaces", 2),
        (5, "Réflexion totale interne", 2),
        (6, "Absorption sur la longueur", 2),
        (7, "Diffusion — flou ∝ distance", 0),
        (8, "Congé d'arête", 0),
        (9, "Dispersion", 0),
        (10, "Caustiques", 0),
        (11, "Fluorescence", 0),
        (12, "Échos entre plaques", 0),
        (13, "Occlusion du ciel", 1),
        (14, "Ombre d'un transparent", 0),
        (15, "Profondeur de champ", 0),
        (16, "Halo d'objectif", 2),
        (17, "Compression de dynamique", 2),
        (18, "Intégration sur le photosite", 2),
    ];

    let mut t = Toile::new(LARGEUR, 570.0);
    entete(
        &mut t,
        "Sept phénomènes sur dix-huit — l'écart, mesuré plutôt que ressenti",
        "L'étalon de la matière. Les quatre derniers relèvent de l'APPAREIL qui regarde ; les quatorze autres, de ce qu'il regarde.",
    );

    let x0 = 44.0;
    let y0 = 92.0;
    let pas = 25.0;
    let bande_debut = 14; // l'indice du n° 15

    for (i, (num, nom, etat)) in phenomenes.iter().enumerate() {
        let y = y0 + i as f64 * pas;
        let (couleur, glyphe) = match etat {
            2 => (VERT, "●"),
            1 => (OR, "◐"),
            _ => (GRIS, "○"),
        };
        if i >= bande_debut {
            t.rect(x0 - 14.0, y - 15.0, 400.0, pas, "#ebe6da");
        }
        t.texte(
            Texte::new(x0, y, &format!("{num:>2}"))
                .taille(10.5)
                .couleur(GRIS)
                .ancre(Ancre::Fin),
        );
        t.texte(Texte::new(x0 + 12.0, y, glyphe).taille(13.0).couleur(couleur));
        t.texte(
            Texte::new(x0 + 34.0, y, nom)
                .taille(11.5)
                .couleur(if *etat == 0 { GRIS } else { ENCRE }),
        );
    }

    // ⚠ L'étiquette de la bande se place à SA DROITE, à sa hauteur médiane — au milieu, elle
    // tomberait sur le nom d'un phénomène.
    let y_bande = y0 + (bande_debut as f64 + 1.5) * pas;
    t.texte(
        Texte::new(x0 + 396.0, y_bande, "l'appareil")
            .taille(10.5)
            .couleur(GRIS)
            .ancre(Ancre::Fin),
    );

    let exprimes = phenomenes.iter().filter(|(_, _, e)| *e == 2).count();
    let partiels = phenomenes.iter().filter(|(_, _, e)| *e == 1).count();
    let absents = phenomenes.iter().filter(|(_, _, e)| *e == 0).count();

    t.texte(Texte::new(COLONNE, 122.0, &exprimes.to_string()).taille(52.0).couleur(VERT).gras());
    t.texte(Texte::new(COLONNE + 42.0, 122.0, "/ 18").taille(20.0).couleur(GRIS));
    t.texte(Texte::new(COLONNE, 148.0, "exprimés").taille(12.0));

    for (i, (n, quoi, couleur)) in [
        (exprimes, "exprimés", VERT),
        (partiels, "partiels", OR),
        (absents, "absents", GRIS),
    ]
    .iter()
    .enumerate()
    {
        let y = 190.0 + i as f64 * 26.0;
        t.disque(COLONNE + 6.0, y - 4.0, 5.0, couleur);
        t.texte(Texte::new(COLONNE + 22.0, y, &format!("{n} {quoi}")).taille(12.0));
    }

    let y = paragraphe(
        &mut t,
        294.0,
        "⚠ Et le chiffre ment si on s'arrête là : aucun des quatre phénomènes de matière n'est dans une image du JEU.",
        ROUGE,
    );
    let y = paragraphe(
        &mut t,
        y,
        "Ils vivent dans une passe exercée par ses seuls tests. Un phénomène « exprimé » que personne ne voit reste, pour le joueur, un phénomène absent.",
        GRIS,
    );
    paragraphe(
        &mut t,
        y,
        "Le 31 août, ils étaient trois — et c'étaient les trois derniers : ceux de l'appareil, jamais de la matière.",
        ENCRE,
    );

    pied(
        &mut t,
        "MESURÉ dans le code au 4 sept. 2026 · l'étalon vient d'une chaîne causale physique posée le 31 août 2026",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 9 — L'INVARIANCE DES CASCADES DE RADIANCE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_cascades() -> String {
    let mut t = Toile::new(LARGEUR, 400.0);
    entete(
        &mut t,
        "L'invariance des cascades — diviser par 4 en multipliant par 4",
        "Le produit sondes × directions reste rigoureusement constant. Aucune constante à régler.",
    );

    let niveaux = 4;
    let ox = 96.0;
    let oy = 104.0;
    let cote = 140.0;
    let ecart = 48.0;

    for n in 0..niveaux {
        let x = ox + n as f64 * (cote + ecart);
        let par_cote = 8u32 >> n;
        let pas = cote / par_cote as f64;
        t.rect(x, oy, cote, cote, "#ebe6da");
        for i in 0..par_cote {
            for j in 0..par_cote {
                let cx = x + (i as f64 + 0.5) * pas;
                let cy = oy + (j as f64 + 0.5) * pas;
                t.disque(cx, cy, (1.4 + n as f64 * 0.9).min(5.0), TERRE);
            }
        }
        let sondes = par_cote * par_cote;
        let rayons = 4u32.pow(n as u32) * 4;

        t.texte(
            Texte::new(x + cote / 2.0, oy - 14.0, &format!("cascade {n}"))
                .taille(12.0)
                .ancre(Ancre::Milieu)
                .gras(),
        );
        let mot = if sondes == 1 { "sonde" } else { "sondes" };
        t.texte(
            Texte::new(x + cote / 2.0, oy + cote + 22.0, &format!("{sondes} {mot}"))
                .taille(11.5)
                .couleur(TERRE)
                .ancre(Ancre::Milieu),
        );
        t.texte(
            Texte::new(x + cote / 2.0, oy + cote + 38.0, &format!("× {rayons} directions"))
                .taille(11.5)
                .couleur(BLEU)
                .ancre(Ancre::Milieu),
        );
        t.ligne(x + 20.0, oy + cote + 48.0, x + cote - 20.0, oy + cote + 48.0, GRIS, 0.8);
        t.texte(
            Texte::new(x + cote / 2.0, oy + cote + 64.0, &format!("= {}", sondes * rayons))
                .taille(13.0)
                .ancre(Ancre::Milieu)
                .gras(),
        );

        if n + 1 < niveaux {
            t.texte(
                Texte::new(x + cote + ecart / 2.0, oy + cote / 2.0 + 6.0, "→")
                    .taille(20.0)
                    .couleur(GRIS)
                    .ancre(Ancre::Milieu),
            );
        }
    }

    t.texte(
        Texte::new(28.0, 352.0, "Même travail et même mémoire à chaque niveau — et le nombre de cascades tombe d'un logarithme au lieu de se régler.")
            .taille(11.5)
            .couleur(ENCRE),
    );

    pied(
        &mut t,
        "LU chez quelqu'un · A. Sannikov, Radiance Cascades · ⚠ RIEN de ceci n'est implémenté dans Aegis à ce jour",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 10 — LA BANDE PASSANTE DES CARTES DE GÉOMÉTRIE
// ══════════════════════════════════════════════════════════════════════════════════════════════
//
// ⚠⚠ CETTE FIGURE A ÉTÉ FAUSSE AVANT DE PARAÎTRE, et la façon dont elle l'était vaut d'être dite.
// Sa première version traçait la ligne de référence à **87 o/pixel** — le budget d'une image
// entière — et y posait les seuils de 33 % et 66 % que le test annonce. Les deux ne pouvaient pas
// se rencontrer : à 33 % de couverture, le format 16 bits coûte 8,6 o par pixel d'écran, pas 87.
//
// Le test, lui, ne compare pas au budget entier : il s'accorde **10 % du budget** pour cette seule
// fonction (`let part_max = 8.7 / par_pixel_de_verre * 100.0`). *Le chiffre était juste, la ligne
// à laquelle je le rapportais ne l'était pas — et il a fallu ouvrir le test pour le voir, pas le
// deviner.*

fn figure_bande_passante() -> String {
    let mut t = Toile::new(LARGEUR, 440.0);
    entete(
        &mut t,
        "Ce que la réfraction coûte en bande passante — la ressource rare",
        "Lectures de Newton seules, à 3,28 lectures mesurées par pixel de verre.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 86.0,
        largeur: 520.0,
        hauteur: 258.0,
        x_min: 0.0,
        x_max: 100.0,
        y_min: 0.0,
        y_max: 30.0,
        log_y: false,
    };

    r.graduation_y(&mut t, &[0.0, 8.7, 15.0, 22.5, 30.0], |v| format!("{} o", fr(v, 1)));
    r.graduation_x(&mut t, &[0.0, 25.0, 50.0, 75.0, 100.0], |v| format!("{v:.0} %"));
    r.cadre(&mut t);

    // ⭐ La part qu'on S'ACCORDE, et c'est une décision, pas une limite physique.
    let y_part = r.y(8.7);
    t.pointille(r.gauche, y_part, r.gauche + r.largeur, y_part, ENCRE, 1.4);
    t.texte(
        Texte::new(r.gauche + 10.0, y_part - 8.0, "8,7 o/pixel — les 10 % du budget qu'on accorde à la réfraction")
            .taille(10.5)
            .couleur(ENCRE)
            .gras(),
    );

    for (octets_par_lecture, couleur, seuil) in
        [(8.0f64, ROUGE, 33.0f64), (4.0, VERT, 66.0)]
    {
        let mut courbe = Vec::new();
        for i in 0..=100 {
            let couverture = i as f64;
            let cout = 3.28 * octets_par_lecture * couverture / 100.0;
            if cout > r.y_max {
                break;
            }
            courbe.push((r.x(couverture), r.y(cout)));
        }
        t.polyligne(&courbe, couleur, 2.4);
        let x = r.x(seuil);
        t.point(x, y_part, 4.5, couleur);
        t.texte(
            Texte::new(x, y_part + 20.0, &format!("{seuil:.0} %"))
                .taille(12.0)
                .couleur(couleur)
                .ancre(Ancre::Milieu)
                .gras(),
        );
    }

    // Ce que la scène de mesure couvrait réellement.
    let x38 = r.x(38.0);
    t.pointille(x38, r.haut, x38, r.haut + r.hauteur, GRIS, 1.0);
    t.texte(
        Texte::new(x38 + 6.0, r.haut + r.hauteur - 12.0, "38 % — la scène mesurée (gros plan, pire cas)")
            .taille(10.0)
            .couleur(GRIS),
    );

    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "part de l'écran couverte par du verre")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    legende(&mut t, COLONNE, 104.0, ROUGE, "RGBA16F — 26,2 o/px");
    legende(&mut t, COLONNE, 126.0, VERT, "RGBA8 octaédr. — 13,1 o/px");

    let y = paragraphe(
        &mut t,
        166.0,
        "⚠ Ce calcul ne compte QUE les lectures de Newton — ni la passe qui produit la carte des faces arrière, ni le reste de l'image.",
        ROUGE,
    );
    let y = paragraphe(
        &mut t,
        y,
        "Le vrai coût est donc plus élevé que ces courbes, et l'écart n'est pas chiffré.",
        GRIS,
    );
    paragraphe(
        &mut t,
        y,
        "Diviser le format par deux double la surface de verre qu'on peut se permettre. C'est le genre d'arbitrage que la bande passante impose, et le calcul, lui, ne bouge pas.",
        ENCRE,
    );

    pied(
        &mut t,
        "CALCULÉ · lectures MESURÉES (3,28/pixel) × budget CALCULÉ de seconde main (87 o/pixel, dont 10 % accordés ici)",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 11 — LA CONVERGENCE DU MAILLAGE VERS LA SPHÈRE VRAIE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_maillage() -> String {
    // (subdivisions, écart angulaire en degrés, volume signé, désaccords de silhouette)
    let serie: [(f64, f64, f64, u64); 4] = [
        (8.0, 1.774, 0.0, 92),
        (16.0, 0.490, 4.12194, 36),
        (32.0, 0.118, 4.17200, 4),
        (64.0, 0.036, 4.18459, 0),
    ];
    let volume_vrai = 4.0 * std::f64::consts::PI / 3.0;

    let mut t = Toile::new(LARGEUR, 440.0);
    entete(
        &mut t,
        "Une bille rastérisée retrouve la sphère analytique",
        "Deux grandeurs sans rapport l'une avec l'autre convergent vers la même vérité : l'angle et le volume.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 86.0,
        largeur: 500.0,
        hauteur: 250.0,
        x_min: 2.7,
        x_max: 6.3,
        y_min: (0.022f64).log10(),
        y_max: (4.0f64).log10(),
        log_y: true,
    };

    r.graduation_y(&mut t, &[0.03, 0.1, 0.3, 1.0, 3.0], |v| {
        format!("{}°", fr(v, 2))
    });
    r.graduation_x(&mut t, &[3.0, 4.0, 5.0, 6.0], |v| {
        format!("{}", (2.0f64).powf(v) as i64)
    });
    r.cadre(&mut t);

    let points: Vec<(f64, f64)> = serie
        .iter()
        .map(|&(s, a, _, _)| (r.x(s.log2()), r.y(a)))
        .collect();
    t.polyligne(&points, TERRE, 2.6);
    for (i, &(x, y)) in points.iter().enumerate() {
        t.point(x, y, 4.5, TERRE);
        // ⚠ Les étiquettes alternent au-dessus / en dessous : sur une droite en descente,
        // les poser toutes du même côté fait chevaucher chaque chiffre avec le suivant.
        t.texte(
            Texte::new(x + 14.0, y - 8.0, &format!("{}°", fr(serie[i].1, 3)))
                .taille(11.5)
                .couleur(TERRE)
                .gras(),
        );
        t.texte(
            Texte::new(x + 14.0, y + 6.0, &format!("{} désaccords", serie[i].3))
                .taille(9.0)
                .couleur(GRIS),
        );
    }

    // ⚠ Sous la droite, et non au-dessus : le coin haut-gauche porte déjà le premier point
    // et son étiquette.
    t.texte(
        Texte::new(r.gauche + 22.0, r.haut + r.hauteur - 24.0, "pente ≈ −2 : quadrupler les subdivisions divise l'erreur par seize")
            .taille(11.0)
            .couleur(GRIS),
    );
    t.texte(
        Texte::new(r.gauche + r.largeur / 2.0, r.haut + r.hauteur + 44.0, "subdivisions du maillage")
            .taille(11.5)
            .couleur(GRIS)
            .ancre(Ancre::Milieu),
    );

    t.texte(Texte::new(COLONNE, 104.0, "et le volume signé").taille(12.0).gras());
    t.texte(
        Texte::new(COLONNE, 122.0, &format!("vraie sphère : {}", fr(volume_vrai, 5)))
            .taille(10.5)
            .couleur(GRIS),
    );
    for (i, &(s, _, v, _)) in serie.iter().enumerate() {
        if v == 0.0 {
            continue;
        }
        let y = 152.0 + (i as f64 - 1.0) * 34.0;
        let ecart = (v - volume_vrai).abs() / volume_vrai * 100.0;
        t.texte(
            Texte::new(COLONNE, y, &format!("{s:.0} subdiv."))
                .taille(11.0)
                .couleur(GRIS),
        );
        t.texte(Texte::new(COLONNE + 74.0, y, &fr(v, 5)).taille(11.5).gras());
        t.texte(
            Texte::new(COLONNE + 154.0, y, &format!("{} %", fr(ecart, 2)))
                .taille(10.5)
                .couleur(TERRE),
        );
    }
    paragraphe(
        &mut t,
        278.0,
        "Deux grandeurs qui n'ont rien en commun — un angle de sortie de rayon, un volume intégré — convergent vers la même vérité.",
        GRIS,
    );

    pied(
        &mut t,
        "MESURÉ le 4 sept. 2026 · tests l_ecart_a_la_bille_vraie_retrecit_quand_on_subdivise et le_volume_signe_de_la_bille…",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 12 — LE POIDS DU CODE, VIVANT ET ENDORMI
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_code() -> String {
    let mut t = Toile::new(LARGEUR, 370.0);
    entete(
        &mut t,
        "Le poids du code — et ce qui dort dedans",
        "Relu sur le disque le 4 septembre 2026 par « cargo run --example etat ». Ce bloc ne s'écrit plus à la main.",
    );

    // (nom, lignes vivantes, lignes endormies, fichiers)
    let parts: [(&str, f64, f64, u64); 2] =
        [("moteur", 18955.0, 2655.0, 52), ("jeu", 15541.0, 0.0, 33)];
    let max = 23000.0;
    let ox = 150.0;
    let largeur = 560.0;

    for (i, (nom, vivant, endormi, fichiers)) in parts.iter().enumerate() {
        let y = 104.0 + i as f64 * 82.0;
        let lv = vivant / max * largeur;
        let le = endormi / max * largeur;
        t.rect(ox, y, lv, 38.0, TERRE);
        if le > 0.0 {
            t.rect(ox + lv, y, le, 38.0, GRIS);
        }
        t.texte(
            Texte::new(ox - 12.0, y + 22.0, nom)
                .taille(13.0)
                .ancre(Ancre::Fin)
                .gras(),
        );
        t.texte(
            Texte::new(ox + 12.0, y + 24.0, &format!("{} lignes vivantes", milliers(*vivant as u64)))
                .taille(11.5)
                .couleur(PAPIER),
        );
        if le > 0.0 {
            t.texte(
                Texte::new(ox + lv + le + 10.0, y + 24.0, &format!("+ {} endormies", milliers(*endormi as u64)))
                    .taille(11.0)
                    .couleur(GRIS),
            );
        }
        t.texte(
            Texte::new(ox - 12.0, y + 38.0, &format!("{fichiers} fichiers"))
                .taille(9.5)
                .couleur(GRIS)
                .ancre(Ancre::Fin),
        );
    }

    let notes = [
        ("⚠ Le JEU est presque aussi gros que le MOTEUR — ça contredit l'image d'un « moteur avec un petit jeu dessus ».", ROUGE),
        ("25 fichiers dorment sous un préfixe « _ » : le compilateur ne les voit pas, git les garde entiers.", GRIS),
        ("Ils portent de vraies formules testées — réservoir ReSTIR, WBOIT/MBOIT, LEAN — et zéro appel au GPU.", GRIS),
        ("Un fichier qui porte le nom d'une technique ne l'implémente pas. C'est le piège n° 1 de ce terrain.", ENCRE),
    ];
    for (i, (n, couleur)) in notes.iter().enumerate() {
        t.texte(
            Texte::new(28.0, 282.0 + i as f64 * 16.0, n)
                .taille(11.0)
                .couleur(couleur),
        );
    }

    pied(&mut t, "MESURÉ le 4 sept. 2026 · cargo run --release -p aegis_engine --example etat --no-default-features");
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 13 — CE QUE L'APPROXIMATION COÛTE, ET CE QUE NEWTON RATTRAPE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_approximation() -> String {
    // Erreur moyenne sur la direction sortante, contre la vérité analytique.
    let barres = [
        ("le rayon ne dévie pas", 26.06, ROUGE),
        ("avancer le long du dévié", 15.47, ROUGE),
        ("+ une itération", 11.37, OR),
        ("Newton, quatre pas", 1.740, VERT),
    ];

    let mut t = Toile::new(LARGEUR, 402.0);
    entete(
        &mut t,
        "Ce que coûte l'approximation — et ce que Newton rattrape",
        "Erreur moyenne sur la direction du rayon SORTANT, contre la vérité analytique d'une sphère.",
    );

    let ox = 290.0;
    let largeur = 440.0;
    let max = 28.0;

    for (i, (nom, valeur, couleur)) in barres.iter().enumerate() {
        let y = 102.0 + i as f64 * 48.0;
        let l = valeur / max * largeur;
        t.rect(ox, y, l, 28.0, couleur);
        t.texte(
            Texte::new(ox - 12.0, y + 19.0, nom)
                .taille(12.0)
                .ancre(Ancre::Fin),
        );
        t.texte(
            Texte::new(ox + l + 10.0, y + 19.0, &format!("{}°", fr(*valeur, 2)))
                .taille(13.0)
                .couleur(couleur)
                .gras(),
        );
    }

    t.ligne(ox, 96.0, ox, 102.0 + 4.0 * 48.0 - 14.0, GRIS, 1.0);
    // ⚠ Le facteur de gain se pose à côté de SA barre, jamais dans la zone de texte du bas :
    // là, il tombait au milieu d'une phrase et se lisait comme un mot de plus.
    t.texte(
        Texte::new(ox + 100.0, 102.0 + 3.0 * 48.0 + 19.0, "× 15,0 contre l'approximation")
            .taille(13.0)
            .couleur(VERT)
            .gras(),
    );

    let notes = [
        ("L'approximation universelle du temps réel — « le rayon traverse tout droit » — se trompe", ENCRE),
        ("de vingt-six degrés en moyenne sur une sphère. Sur toute l'image, elle atteint 36,4°.", ENCRE),
        ("Ce chiffre-là a une histoire : le banc balayait UNE LIGNE d'écran et annonçait 1,8°.", GRIS),
        ("L'équateur est le cas le plus favorable de l'image. Le vrai chiffre est vingt fois pire.", GRIS),
    ];
    for (i, (n, couleur)) in notes.iter().enumerate() {
        t.texte(
            Texte::new(28.0, 312.0 + i as f64 * 15.0, n)
                .taille(10.5)
                .couleur(couleur),
        );
    }

    pied(
        &mut t,
        "MESURÉ le 4 sept. 2026 · tests l_erreur_de_l_approximation_a_deux_interfaces et les_trois_images_de_la_bille…",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  FIGURE 14 — LA MARCHE INHOMOGÈNE REDONNE LA FORMULE FERMÉE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn figure_volume() -> String {
    let mut t = Toile::new(LARGEUR, 464.0);
    entete(
        &mut t,
        "Le milieu homogène est un cas EXACT de la marche, pas une branche à part",
        "Zéro octet d'écart sur 262 144. Une branche « si le milieu est homogène » aurait été un second chemin à tester pour toujours.",
    );

    let r = Repere {
        gauche: 82.0,
        haut: 92.0,
        largeur: 520.0,
        hauteur: 232.0,
        x_min: 0.0,
        x_max: 3.0,
        y_min: 0.0,
        y_max: 1.05,
        log_y: false,
    };

    r.graduation_y(&mut t, &[0.0, 0.5, 1.0], |v| fr(v, 1));
    r.graduation_x(&mut t, &[0.0, 1.0, 2.0, 3.0], |v| format!("{v:.0}"));
    r.cadre(&mut t);

    // La densité le long du rayon : un feuillet dense au tiers du trajet.
    let densite = |s: f64| 1.0 + 3.0 * (-((s - 1.0) / 0.16).powi(2)).exp();
    let mut courbe_densite = Vec::new();
    let mut courbe_tau = Vec::new();
    let mut tau = 0.0f64;
    let pas = 600;
    for i in 0..=pas {
        let s = i as f64 / pas as f64 * 3.0;
        courbe_densite.push((r.x(s), r.y(densite(s) / 4.4)));
        if i > 0 {
            tau += densite(s - 1.5 / pas as f64) * 3.0 / pas as f64 * 0.35;
        }
        courbe_tau.push((r.x(s), r.y((-tau).exp())));
    }
    let mut fermee = Vec::new();
    for i in 0..=300 {
        let s = i as f64 / 300.0 * 3.0;
        fermee.push((r.x(s), r.y((-0.35 * s).exp())));
    }
    t.polyligne(&fermee, GRIS, 1.4);
    t.polyligne(&courbe_densite, BLEU, 2.0);
    t.polyligne(&courbe_tau, TERRE, 2.6);

    // ⚠ La légende va SOUS les courbes, dans le coin bas-gauche : au-dessus, elle tomberait sur
    // la transmittance, qui part précisément du coin haut-gauche.
    // ⚠ La légende va SOUS l'axe. Dans le cadre, les trois courbes occupent le haut, le bas et
    // la diagonale : il n'y reste aucune zone franchement vide, et une légende posée « dans un
    // trou » se fait traverser dès que les données changent un peu.
    t.texte(
        Texte::new(r.gauche, r.haut + r.hauteur + 30.0, "abscisse le long du rayon, dans la matière")
            .taille(11.5)
            .couleur(GRIS),
    );
    legende(&mut t, r.gauche + 250.0, r.haut + r.hauteur + 44.0, BLEU, "densité échantillonnée dans le volume");
    legende(&mut t, r.gauche + 250.0, r.haut + r.hauteur + 62.0, TERRE, "transmittance accumulée  exp(−τ)");
    legende(&mut t, r.gauche + 250.0, r.haut + r.hauteur + 80.0, GRIS, "ce que la formule fermée aurait donné");

    let mut y = 104.0;
    for (valeur, quoi, detail) in [
        ("0", "octet d'écart sur 262 144", "volume neutre, 1 pas contre 32"),
        ("4,69", "niveaux déplacés par octet", "avec un feuillet dense"),
        ("51 ms", "pour cuire un volume 128³", "34 Mo"),
    ] {
        y = chiffre(&mut t, y, valeur, quoi, detail, TERRE);
    }
    paragraphe(
        &mut t,
        y - 6.0,
        "Sur un volume neutre, chaque pas ajoute σ·1·ds, dont la somme vaut σ·d : la marche REDONNE l'ancienne formule au lieu de la remplacer.",
        GRIS,
    );

    pied(
        &mut t,
        "Courbes CALCULÉES pour l'illustration · les trois chiffres sont MESURÉS le 4 sept. 2026 (test un_volume_inhomogene…)",
    );
    t.rendre()
}

// ══════════════════════════════════════════════════════════════════════════════════════════════
//  L'ÉCRITURE
// ══════════════════════════════════════════════════════════════════════════════════════════════

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("racine du dépôt introuvable")?
        .join("recherche/figures");
    fs::create_dir_all(&racine)?;

    /// Le nom du fichier et la fonction qui le produit.
    type Figure = (&'static str, fn() -> String);

    let figures: [Figure; 14] = [
        ("newton-convergence.svg", figure_newton),
        ("discretisation.svg", figure_discretisation),
        ("newton-lectures.svg", figure_lectures),
        ("fresnel.svg", figure_fresnel),
        ("beer-lambert.svg", figure_beer_lambert),
        ("budget-quest2.svg", figure_budget),
        ("traverse-gi.svg", figure_traverse),
        ("phenomenes.svg", figure_phenomenes),
        ("cascades-invariance.svg", figure_cascades),
        ("bande-passante.svg", figure_bande_passante),
        ("maillage-convergence.svg", figure_maillage),
        ("poids-du-code.svg", figure_code),
        ("approximation.svg", figure_approximation),
        ("volume-marche.svg", figure_volume),
    ];

    println!("\n\x1b[1mLES FIGURES DU DOSSIER DE RECHERCHE\x1b[0m");
    println!("───────────────────────────────────");
    let mut total = 0usize;
    for (nom, produire) in figures {
        let contenu = produire();
        total += contenu.len();
        fs::write(racine.join(nom), &contenu)?;
        println!("  {nom:<28} {:>6} o", contenu.len());
    }
    println!("───────────────────────────────────");
    println!(
        "  {} figures · {:.1} Ko · {}",
        figures.len(),
        total as f64 / 1024.0,
        racine.display()
    );
    println!(
        "\n  ⚠ Les séries relevées portent leur commande de re-vérification DANS le code.\n\
           \x20   Fresnel, Beer-Lambert et l'invariance des cascades sont CALCULÉS ici.\n"
    );
    Ok(())
}
