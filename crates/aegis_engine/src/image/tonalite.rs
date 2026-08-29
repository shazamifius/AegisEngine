//! # Mesurer ce qu'une image fait à l'œil — en chiffres, pas en impressions
//!
//! Né le 30 août 2026, sur un constat qui ne pouvait pas se discuter autrement : *« c'est
//! horriblement moche, c'est tout triste, il fait extrêmement gris »*. Une phrase juste, et
//! impossible à corriger telle quelle — on ne sait pas quoi changer, ni de combien, ni si le
//! changement suivant a rapproché ou éloigné du but.
//!
//! ## ⚠ Ce que ce module N'EST PAS
//!
//! **Il ne juge pas.** Le juge du rendu perçu est un œil humain, et cette règle a déjà été payée
//! sur ce projet : des métriques flatteuses (+13 dB de séparation de bruit) avaient été présentées
//! comme un succès alors que l'oreille tranchait « vraiment pas concluant ». *Une bonne métrique
//! ne vaut jamais le verdict de la perception.*
//!
//! Ce qu'il fait, c'est **rendre décidable** ce qui ne l'était pas : nommer ce qui manque, le
//! chiffrer, et permettre de comparer deux réglages autrement que de mémoire.
//!
//! ## Pourquoi le RVB ne peut pas répondre
//!
//! En RVB, `(0, 255, 0)` et `(0, 0, 255)` sont à égale distance de l'origine — alors que l'œil
//! voit le vert **six fois plus lumineux** que le bleu. Toute moyenne, tout écart, tout « niveau
//! de gris » calculé en RVB décrit donc un espace que personne n'habite.
//!
//! On travaille en **CIELAB**, construit expérimentalement pour qu'une distance égale y
//! corresponde à une différence perçue égale, puis en **LCh** — la même chose en coordonnées
//! polaires, où `C` est la vivacité d'une couleur et `h` sa teinte en degrés. C'est dans cet
//! espace, et lui seul, que « il fait gris » devient un nombre.
//!
//! ## Les repères employés, et d'où ils viennent
//!
//! Trois seuils seulement, et aucun n'est un choix de goût :
//!
//! - **C\* ≈ 10** : en dessous, une couleur cesse d'être perçue comme teintée et se lit comme un
//!   gris. C'est ce qui sépare « une couleur sombre » de « du gris ».
//! - **ΔE ≈ 2,3** : le seuil de différence juste perceptible. En dessous, deux couleurs sont *la
//!   même* pour un observateur.
//! - **ΔL\* ≈ 15** : en dessous, deux plans adjacents ne se séparent pas franchement. C'est la
//!   grandeur qui décide si un personnage se détache de son fond — indépendamment des couleurs.
//!
//! Le reste est rapporté **en clair, sans verdict** : les proportions, l'étendue, les familles de
//! teintes. Poser un seuil sur ce qui relève du goût serait exactement l'erreur que ce module
//! existe pour éviter.

/// Une couleur dans l'espace perceptuel CIELAB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    /// Clarté, de 0 (noir) à 100 (blanc de référence).
    pub l: f32,
    /// Axe vert ↔ rouge.
    pub a: f32,
    /// Axe bleu ↔ jaune.
    pub b: f32,
}

/// La même couleur en coordonnées polaires : clarté, vivacité, teinte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lch {
    pub l: f32,
    /// Vivacité perçue. Sous ~10, l'œil ne voit plus de teinte : c'est du gris.
    pub c: f32,
    /// Teinte en degrés : 0 rouge, 90 jaune, 180 vert-cyan, 270 bleu.
    pub h: f32,
}

/// Le blanc de référence D65, celui des écrans.
const BLANC: [f32; 3] = [0.950_47, 1.0, 1.088_83];

/// Convertit une couleur d'écran (sRGB, 0 à 1) vers CIELAB.
///
/// ⚠ Le passage par le linéaire n'est pas optionnel : c'est la **même** conversion que celle du
/// shader, et pour la même raison. Faire la moyenne de valeurs sRGB revient à moyenner des
/// logarithmes en croyant moyenner des quantités.
pub fn srgb_vers_lab(r: f32, g: f32, b: f32) -> Lab {
    let lineaire = |c: f32| -> f32 {
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lineaire(r), lineaire(g), lineaire(b));

    // sRGB vers XYZ, matrice de la norme.
    let x = 0.412_39 * r + 0.357_58 * g + 0.180_48 * b;
    let y = 0.212_64 * r + 0.715_17 * g + 0.072_19 * b;
    let z = 0.019_33 * r + 0.119_19 * g + 0.950_53 * b;

    let f = |t: f32| -> f32 {
        // Le segment droit près du noir évite une dérivée infinie en zéro — sans lui, le bruit
        // des tons sombres est amplifié sans limite.
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / BLANC[0]), f(y / BLANC[1]), f(z / BLANC[2]));

    Lab { l: 116.0 * fy - 16.0, a: 500.0 * (fx - fy), b: 200.0 * (fy - fz) }
}

pub fn lab_vers_lch(lab: Lab) -> Lch {
    let c = (lab.a * lab.a + lab.b * lab.b).sqrt();
    let mut h = lab.b.atan2(lab.a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    Lch { l: lab.l, c, h }
}

/// La différence perçue entre deux couleurs (CIE76).
///
/// ⚠ C'est la formule de 1976, pas CIEDE2000 : elle surestime les écarts dans les bleus très
/// saturés. Elle suffit ici parce qu'on l'emploie pour **comparer des plans entre eux**, pas pour
/// contrôler une impression textile. *Le dire vaut mieux que laisser croire à une précision qu'on
/// n'a pas.*
pub fn ecart_percu(a: Lab, b: Lab) -> f32 {
    ((a.l - b.l).powi(2) + (a.a - b.a).powi(2) + (a.b - b.b).powi(2)).sqrt()
}

/// Une famille de teintes présente dans l'image, avec ce qu'elle occupe.
#[derive(Clone, Debug)]
pub struct Famille {
    /// Teinte moyenne de la famille, en degrés.
    pub teinte: f32,
    /// Part de l'image qu'elle occupe, de 0 à 1 — **parmi les pixels réellement colorés**.
    pub part: f32,
    /// Clarté moyenne des pixels de cette famille.
    pub clarte: f32,
    /// Vivacité moyenne.
    pub vivacite: f32,
}

/// Ce qu'une image fait, mesuré.
#[derive(Clone, Debug)]
pub struct Analyse {
    /// Clarté médiane, de 0 à 100.
    pub clarte_mediane: f32,
    /// L'écart entre les 5 % les plus sombres et les 5 % les plus clairs.
    ///
    /// ⭐ **C'est la grandeur la plus parlante de tout ce module.** Une image dont tout se tient
    /// dans vingt points de clarté n'a pas de hiérarchie : rien n'avance, rien ne recule, et
    /// l'œil ne trouve pas où se poser. C'est ce qu'on appelle « plat », ou « triste ».
    pub etendue_tonale: f32,
    /// Vivacité médiane. Sous 10, l'image est perçue comme grise.
    pub vivacite_mediane: f32,
    /// Part des pixels que l'œil lit comme du gris (vivacité sous 10).
    pub part_grise: f32,
    /// Les familles de teintes, de la plus présente à la moins.
    pub familles: Vec<Famille>,
    /// De −1 (tout froid) à +1 (tout chaud), pondéré par la vivacité.
    pub temperature: f32,
    /// Combien de pixels ont été mesurés.
    pub pixels: usize,
}

/// Sous cette vivacité, l'œil ne perçoit plus de teinte : c'est un gris.
const SEUIL_GRIS: f32 = 10.0;
/// Sous cet écart de clarté, deux plans ne se séparent pas franchement.
pub const SEPARATION_MINIMALE: f32 = 15.0;

/// Mesure une image RVB (trois octets par pixel, en sRGB).
///
/// ⚠ Les pixels sont échantillonnés à intervalle régulier au-delà d'un million : au-delà, la
/// mesure ne bouge plus de façon perceptible et le temps de calcul, lui, continue de croître.
/// *Jamais d'excédent* — et le compte réellement mesuré est rendu, pour qu'aucune moyenne ne
/// puisse être lue sans savoir sur quoi elle porte.
pub fn analyser(rvb: &[u8]) -> Analyse {
    let total = rvb.len() / 3;
    let pas = (total / 1_000_000).max(1);

    let mut clartes = Vec::with_capacity(total / pas + 1);
    let mut vivacites = Vec::with_capacity(total / pas + 1);
    // Douze secteurs de 30° : assez fin pour distinguer un vert d'un jaune, assez large pour que
    // deux nuances du même vert ne comptent pas comme deux familles.
    let mut secteurs = [(0.0f64, 0.0f64, 0.0f64, 0usize); 12];
    let mut chaleur = 0.0f64;
    let mut poids_chaleur = 0.0f64;
    let mut mesures = 0usize;

    for i in (0..total).step_by(pas) {
        let lch = lab_vers_lch(srgb_vers_lab(
            f32::from(rvb[i * 3]) / 255.0,
            f32::from(rvb[i * 3 + 1]) / 255.0,
            f32::from(rvb[i * 3 + 2]) / 255.0,
        ));
        clartes.push(lch.l);
        vivacites.push(lch.c);
        mesures += 1;

        if lch.c >= SEUIL_GRIS {
            let secteur = ((lch.h / 30.0) as usize).min(11);
            secteurs[secteur].0 += f64::from(lch.h);
            secteurs[secteur].1 += f64::from(lch.l);
            secteurs[secteur].2 += f64::from(lch.c);
            secteurs[secteur].3 += 1;

            // Chaud = vers le rouge-jaune (0 à 90° et 300 à 360°), froid = vers le cyan-bleu.
            // Le cosinus donne une transition continue plutôt qu'une frontière arbitraire.
            let radians = (lch.h - 60.0).to_radians();
            chaleur += f64::from(radians.cos() * lch.c);
            poids_chaleur += f64::from(lch.c);
        }
    }

    clartes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    vivacites.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let centile = |tri: &[f32], p: f32| -> f32 {
        if tri.is_empty() {
            return 0.0;
        }
        let index = ((tri.len() - 1) as f32 * p).round() as usize;
        tri[index]
    };

    let colores: usize = secteurs.iter().map(|s| s.3).sum();
    let mut familles: Vec<Famille> = secteurs
        .iter()
        .filter(|s| s.3 > 0)
        .map(|s| Famille {
            teinte: (s.0 / s.3 as f64) as f32,
            part: if colores == 0 { 0.0 } else { s.3 as f32 / colores as f32 },
            clarte: (s.1 / s.3 as f64) as f32,
            vivacite: (s.2 / s.3 as f64) as f32,
        })
        .collect();
    familles.sort_by(|a, b| b.part.partial_cmp(&a.part).unwrap_or(std::cmp::Ordering::Equal));

    Analyse {
        clarte_mediane: centile(&clartes, 0.5),
        etendue_tonale: centile(&clartes, 0.95) - centile(&clartes, 0.05),
        vivacite_mediane: centile(&vivacites, 0.5),
        part_grise: if mesures == 0 {
            0.0
        } else {
            (mesures - colores) as f32 / mesures as f32
        },
        familles,
        temperature: if poids_chaleur == 0.0 {
            0.0
        } else {
            (chaleur / poids_chaleur) as f32
        },
        pixels: mesures,
    }
}

/// Le nom du schéma que forment les deux familles dominantes, d'après leur écart de teinte.
///
/// Les bornes sont celles de la théorie classique des couleurs (Itten) et ne prétendent à rien de
/// plus : ce sont des NOMS donnés à des intervalles, pas des jugements. Une image analogue n'est
/// pas « meilleure » qu'une image complémentaire — elle est plus calme, ce qui peut être ce qu'on
/// cherche, ou l'inverse.
pub fn schema(ecart_de_teinte: f32) -> &'static str {
    let e = ecart_de_teinte.abs().min(360.0 - ecart_de_teinte.abs());
    match e {
        e if e < 15.0 => "monochrome — une seule teinte, calme, au risque de la monotonie",
        e if e < 50.0 => "analogue — teintes voisines, harmonie douce et cohesive",
        e if e < 100.0 => "ecart moyen — ni voisines ni opposees, souvent le moins lisible",
        e if e < 140.0 => "triade — trois teintes equidistantes, vif et equilibre",
        _ => "complementaire — teintes opposees, contraste maximal",
    }
}

/// Une chose que la mesure permet de dire, et son degré d'urgence.
#[derive(Clone, Debug, PartialEq)]
pub struct Remarque {
    /// `true` quand un seuil PERCEPTUEL documenté est franchi — pas un avis.
    pub mesurable: bool,
    pub texte: String,
}

/// Traduit une analyse en observations.
///
/// ⚠ Chaque remarque marquée `mesurable` s'appuie sur un seuil de perception documenté ; les
/// autres sont des observations de composition, et **restent discutables par construction**. La
/// distinction est portée dans le type, pas dans le ton, pour qu'on ne puisse pas les confondre en
/// lisant vite.
pub fn diagnostiquer(a: &Analyse) -> Vec<Remarque> {
    let mut sortie = Vec::new();

    sortie.push(Remarque {
        mesurable: a.etendue_tonale < 35.0,
        texte: format!(
            "etendue tonale {:.0} points sur 100 (du 5e au 95e centile de clarte) — \
             en dessous de 35, rien n'avance ni ne recule : c'est ce qu'on percoit comme « plat »",
            a.etendue_tonale
        ),
    });

    sortie.push(Remarque {
        mesurable: a.part_grise > 0.5,
        texte: format!(
            "{:.0} % de l'image est percue comme GRISE (vivacite sous {SEUIL_GRIS:.0}), \
             vivacite mediane {:.1}",
            a.part_grise * 100.0,
            a.vivacite_mediane
        ),
    });

    sortie.push(Remarque {
        mesurable: false,
        texte: format!(
            "clarte mediane {:.0} sur 100, temperature {:+.2} ({})",
            a.clarte_mediane,
            a.temperature,
            if a.temperature > 0.2 {
                "chaude"
            } else if a.temperature < -0.2 {
                "froide"
            } else {
                "neutre"
            }
        ),
    });

    if let (Some(un), Some(deux)) = (a.familles.first(), a.familles.get(1)) {
        let ecart = (un.teinte - deux.teinte).abs();
        sortie.push(Remarque {
            mesurable: false,
            texte: format!(
                "deux familles dominantes a {:.0}° ({:.0} %) et {:.0}° ({:.0} %), \
                 ecart {:.0}° : {}",
                un.teinte,
                un.part * 100.0,
                deux.teinte,
                deux.part * 100.0,
                ecart,
                schema(ecart)
            ),
        });

        let separation = (un.clarte - deux.clarte).abs();
        sortie.push(Remarque {
            mesurable: separation < SEPARATION_MINIMALE,
            texte: format!(
                "les deux dominantes ne different que de {separation:.0} points de clarte — \
                 en dessous de {SEPARATION_MINIMALE:.0}, deux plans ne se separent pas \
                 franchement, quelles que soient leurs teintes"
            ),
        });
    }

    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les couleurs de référence doivent tomber là où la norme les place.
    #[test]
    fn les_conversions_tombent_sur_les_valeurs_de_reference() {
        // Le blanc de référence : clarté 100, aucune teinte.
        let blanc = srgb_vers_lab(1.0, 1.0, 1.0);
        assert!((blanc.l - 100.0).abs() < 0.1, "clarte du blanc : {}", blanc.l);
        assert!(blanc.a.abs() < 0.1 && blanc.b.abs() < 0.1, "le blanc n'a pas de teinte");

        let noir = srgb_vers_lab(0.0, 0.0, 0.0);
        assert!(noir.l.abs() < 0.1, "clarte du noir : {}", noir.l);

        // ⭐ Le gris moyen d'un écran (128/255) tombe vers L* ≈ 53, PAS vers 50. C'est
        // exactement pourquoi ce module ne travaille pas en RVB : « la moitié du signal » n'est
        // pas « la moitié de ce qu'on voit ».
        let gris = srgb_vers_lab(0.502, 0.502, 0.502);
        assert!((gris.l - 53.4).abs() < 1.0, "clarte du gris moyen : {}", gris.l);
        assert!(gris.c_est_neutre(), "un gris ne doit porter aucune teinte");
    }

    impl Lab {
        fn c_est_neutre(&self) -> bool {
            self.a.abs() < 0.5 && self.b.abs() < 0.5
        }
    }

    /// Les teintes doivent se ranger là où l'œil les attend.
    #[test]
    fn les_teintes_se_rangent_ou_l_oeil_les_attend() {
        let rouge = lab_vers_lch(srgb_vers_lab(1.0, 0.0, 0.0));
        let vert = lab_vers_lch(srgb_vers_lab(0.0, 1.0, 0.0));
        let bleu = lab_vers_lch(srgb_vers_lab(0.0, 0.0, 1.0));

        assert!(rouge.h < 45.0 || rouge.h > 350.0, "le rouge est vers 0 : {}", rouge.h);
        assert!((100.0..175.0).contains(&vert.h), "le vert est vers 135 : {}", vert.h);
        assert!((250.0..330.0).contains(&bleu.h), "le bleu est vers 300 : {}", bleu.h);

        // Le vert pur est bien plus clair que le bleu pur — l'œil y est six fois plus sensible.
        assert!(vert.l > bleu.l + 30.0, "vert {} contre bleu {}", vert.l, bleu.l);
    }

    /// ⭐ Le cas qui compte : une image plate doit être MESURÉE comme plate.
    #[test]
    fn une_image_sans_hierarchie_tonale_se_voit_dans_le_chiffre() {
        // Deux gris très proches : exactement ce que produit un fond neutre sous des objets de
        // même clarté — l'image dont on dit « elle est triste » sans savoir pourquoi.
        let mut plate = Vec::new();
        for i in 0..2000 {
            let v = if i % 2 == 0 { 130u8 } else { 138u8 };
            plate.extend_from_slice(&[v, v, v]);
        }
        let a = analyser(&plate);
        assert!(a.etendue_tonale < 10.0, "etendue mesuree : {}", a.etendue_tonale);
        assert!(a.part_grise > 0.99, "tout est gris : {}", a.part_grise);

        let remarques = diagnostiquer(&a);
        assert!(
            remarques.iter().filter(|r| r.mesurable).count() >= 2,
            "une image plate et grise doit declencher au moins deux constats mesurables"
        );

        // Et le contraire doit se voir aussi : du noir et du blanc franchement séparés.
        let mut contrastee = Vec::new();
        for i in 0..2000 {
            let v = if i % 2 == 0 { 20u8 } else { 235u8 };
            contrastee.extend_from_slice(&[v, v, v]);
        }
        let b = analyser(&contrastee);
        assert!(b.etendue_tonale > 70.0, "etendue mesuree : {}", b.etendue_tonale);
    }

    /// L'instrument doit savoir produire une PRÉSENCE avant qu'on croie ses absences.
    #[test]
    fn une_image_franchement_coloree_est_vue_comme_telle() {
        let mut vive = Vec::new();
        for i in 0..3000 {
            // Deux tiers de vert, un tiers de rouge : les proportions doivent se retrouver.
            if i % 3 == 0 {
                vive.extend_from_slice(&[220, 30, 30]);
            } else {
                vive.extend_from_slice(&[30, 200, 60]);
            }
        }
        let a = analyser(&vive);
        assert!(a.part_grise < 0.01, "rien n'est gris ici : {}", a.part_grise);
        assert!(a.vivacite_mediane > 40.0, "vivacite : {}", a.vivacite_mediane);
        assert_eq!(a.familles.len(), 2, "deux familles, pas plus");
        assert!(
            (a.familles[0].part - 0.666).abs() < 0.05,
            "la dominante occupe deux tiers : {}",
            a.familles[0].part
        );
    }

    /// Les noms de schéma doivent suivre l'écart, pas l'inverse.
    #[test]
    fn le_schema_suit_l_ecart_de_teinte() {
        assert!(schema(5.0).starts_with("monochrome"));
        assert!(schema(30.0).starts_with("analogue"));
        assert!(schema(180.0).starts_with("complementaire"));
        // Un écart de 350° est un écart de 10° dans l'autre sens : le cercle se referme.
        assert!(schema(350.0).starts_with("monochrome"));
    }

    /// Deux couleurs identiques n'ont aucun écart ; le seuil de perception est respecté.
    #[test]
    fn l_ecart_percu_respecte_le_seuil_de_perception() {
        let a = srgb_vers_lab(0.5, 0.5, 0.5);
        assert!(ecart_percu(a, a) < 1e-4);

        // Un pas de 1/255 sur un canal reste sous le seuil de différence juste perceptible.
        let b = srgb_vers_lab(0.5 + 1.0 / 255.0, 0.5, 0.5);
        assert!(ecart_percu(a, b) < 2.3, "un pas de quantification doit rester imperceptible");

        // Un écart franc doit, lui, dépasser largement le seuil.
        let c = srgb_vers_lab(0.5, 0.8, 0.5);
        assert!(ecart_percu(a, c) > 20.0);
    }
}
