//! # LA PALETTE DU DÉCOR — les couleurs du monde, à un seul endroit
//!
//! ## ⚠ Pourquoi ce fichier est né le 31 août 2026
//!
//! Son verdict, après un atelier de quatre ambiances : *« honnêtement… les 4 sont nuls en fait, mais
//! le plus proche c'est cozy chaud »*. Aucun réglage de LUMIÈRE ne pouvait le satisfaire, et la
//! raison n'était pas le goût : **la palette du décor n'existait pas comme objet.**
//!
//! Elle vivait dispersée en **huit valeurs littérales, dans deux fichiers**, dont deux qui se
//! contredisaient :
//!
//! | | où | valeur |
//! |---|---|---|
//! | herbe | `party_render_pass.rs` | `0.32, 0.82, 0.36` |
//! | herbe | `grid.rs` — **jamais atteinte**, sauf en repli | `0.25, 0.75, 0.25` |
//! | terre | `party_render_pass.rs` | `0.48, 0.32, 0.20` |
//! | pierre | `party_render_pass.rs` | `0.60, 0.63, 0.68` |
//! | éclats, tavelures, 3 brins | `party_render_pass.rs` | 5 valeurs de plus |
//!
//! **Le piège était prêt à se refermer :** `grid.rs` porte une fonction qui s'appelle `color()` et
//! qui a toutes les apparences de la palette. La régler n'aurait **rien changé** à l'herbe ni à la
//! terre — les deux blocs les plus visibles — et la conclusion naturelle aurait été « ce n'était pas
//! la couleur ». *Deux définitions de la même chose, dont une se croit vraie.*
//!
//! ## Ce que ce fichier change, et ce qu'il ne change pas
//!
//! Il ne change **aucune couleur**. Les valeurs ci-dessous sont, au millième près, celles qui
//! étaient dispersées — c'est le critère de son premier jalon : *si l'image bouge avant qu'il ait
//! rien réglé, c'est qu'une couleur s'est perdue en route.*
//!
//! Ce qu'il change, c'est que ces couleurs **se règlent en direct**, par la console, comme
//! l'`Ambiance`. Le juge du rendu perçu est son œil ; un œil ne peut trancher que ce qu'il voit, et
//! tant qu'un essai coûtait une recompilation, le réglage se faisait de mémoire — donc mal.
//!
//! ## ⚠ La règle qui gouverne ces couleurs, dégagée le même jour
//!
//! **Le décor RECULE, le jeu AVANCE.** Herbe, terre, pierre : désaturés, teintes rapprochées — ils
//! portent l'ambiance et ne doivent pas réclamer l'attention. Lave, pics, glace, miel, arrivée : ils
//! restent francs, parce qu'un piège mortel doit se reconnaître en une fraction de seconde.
//!
//! *C'est ce qui réconcilie deux avis qui semblaient s'opposer : « palette saturée » (son plan de
//! rendu) et « désature tout » (un avis extérieur). Ils ne parlaient pas des mêmes objets.* C'est
//! aussi la leçon de la scie, payée le matin même : ce qui porte une information de jeu ne se sacrifie
//! pas au style.
//!
//! ⚠ **Cette palette ne couvre donc QUE le décor.** Les couleurs de gameplay vivent dans
//! `grid.rs::color()` et n'ont aucune raison d'être réglées à l'œil : elles sont un code, pas un goût.

/// Les couleurs du décor, en **linéaire** — jamais en sRGB.
///
/// ⚠ Ce point n'est pas une subtilité d'implémentation. `party_2d5.wgsl` traite la teinte reçue
/// comme du sRGB et la convertit (`vers_lineaire`). Les valeurs ici sont donc celles qu'on **écrit**,
/// pas celles que la carte multiplie — et c'est voulu : ce sont les nombres qu'un œil humain lit
/// dans une pipette, pas ceux d'un calcul de lumière.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Le dessus des blocs d'herbe.
    pub herbe: [f32; 3],
    /// Le corps des blocs de terre.
    pub terre: [f32; 3],
    /// Le corps des blocs de pierre.
    pub pierre: [f32; 3],
    /// Les éclats plus sombres encastrés dans la pierre (trois par bloc).
    pub eclats_pierre: [f32; 3],
    /// Les micro-tavelures de la terre (deux par bloc).
    pub tavelures_terre: [f32; 3],
    /// Les brins d'herbe, du plus clair au plus sombre.
    ///
    /// ⚠ **Trois champs et non un vert dérivé en trois nuances**, et c'est un choix assumé. Les
    /// dériver (`herbe × 0,85`, `× 0,70`) supprimerait deux valeurs — l'élégance habituelle du
    /// projet — mais retirerait à son œil la liberté de désaccorder volontairement les brins du
    /// bloc. *À faire si l'usage montre que régler trois verts est pénible ; pas avant de l'avoir
    /// constaté.*
    pub brin_clair: [f32; 3],
    pub brin_moyen: [f32; 3],
    pub brin_sombre: [f32; 3],

    /// Les trois plans de la jungle de fond, **du plus lointain au plus proche**.
    ///
    /// ⚠ Elles doivent aller en s'éclaircissant vers le fond ou en se rapprochant de sa couleur —
    /// c'est ce qui donne la profondeur, bien plus que la taille des plantes. Un plan lointain aussi
    /// contrasté qu'un plan proche annule la parallaxe à l'œil : on voit trois rangées, pas trois
    /// distances.
    ///
    /// ⚠⚠ Et elles portent la **garde de lisibilité** : le fond doit se lire comme « derrière » au
    /// premier coup d'œil. Un vert de jungle aussi franc que l'herbe jouable ferait chercher une
    /// plateforme là où il n'y en a pas — la leçon de la scie, dans l'autre sens.
    pub jungle_loin: [f32; 3],
    pub jungle_moyenne: [f32; 3],
    pub jungle_proche: [f32; 3],
}

impl Default for Palette {
    /// ⚠ **Ces valeurs sont celles d'avant, à l'identique.** Les changer ici et prétendre avoir
    /// « seulement rangé » ferait exactement ce que le projet interdit : un déplacement qui modifie
    /// en douce ce qu'il déplace. Le premier jalon est que l'image ne bouge pas.
    fn default() -> Self {
        Self {
            herbe: [0.32, 0.82, 0.36],
            terre: [0.48, 0.32, 0.20],
            pierre: [0.60, 0.63, 0.68],
            eclats_pierre: [0.40, 0.43, 0.48],
            tavelures_terre: [0.42, 0.27, 0.16],
            brin_clair: [0.32, 0.90, 0.35],
            brin_moyen: [0.20, 0.78, 0.26],
            brin_sombre: [0.14, 0.65, 0.20],
            // Un point de départ, pas une décision : trois verts sourds qui vont en s'éclaircissant
            // vers le lointain, franchement plus ternes que l'herbe jouable. **Son œil tranchera** —
            // c'est pour ça qu'ils sont réglables.
            jungle_loin: [0.20, 0.26, 0.21],
            jungle_moyenne: [0.19, 0.29, 0.21],
            jungle_proche: [0.17, 0.31, 0.20],
        }
    }
}

impl Palette {
    /// Les champs réglables. **Une seule liste**, qui gouverne le réglage, l'aide et le test —
    /// même patron qu'`Ambiance::CHAMPS`, et pour la même raison : un champ ajouté sans être
    /// inscrit ici serait **invisible au laboratoire**, sans que rien ne le signale.
    pub const CHAMPS: [&'static str; 11] = [
        "herbe",
        "terre",
        "pierre",
        "eclats_pierre",
        "tavelures_terre",
        "brin_clair",
        "brin_moyen",
        "brin_sombre",
        "jungle_loin",
        "jungle_moyenne",
        "jungle_proche",
    ];

    /// Change une couleur désignée par son nom. **Pure** : testable sans fenêtre et sans GPU.
    pub fn regler(&mut self, champ: &str, valeurs: &[f32]) -> Result<(), String> {
        if !Self::CHAMPS.contains(&champ) {
            return Err(format!(
                "couleur inconnue : {champ:?} — connues : {}",
                Self::CHAMPS.join(", ")
            ));
        }
        if valeurs.len() != 3 {
            return Err(format!(
                "{champ} attend 3 valeurs (r v b), {} recue(s)",
                valeurs.len()
            ));
        }
        // ⚠ Aucune borne haute : une couleur au-delà de 1 est une couleur qui ÉMET, et c'est
        // parfaitement légitime dans une chaîne HDR — c'est même ce qui déclenche le halo. Seul
        // ce qui n'a pas de sens est refusé : une composante négative, un infini, un NaN.
        // *`is_finite` d'abord : avec un NaN, toute comparaison est fausse, y compris `< 0.0`.*
        if valeurs.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("une composante doit etre finie et positive".to_string());
        }
        let c = [valeurs[0], valeurs[1], valeurs[2]];
        match champ {
            "herbe" => self.herbe = c,
            "terre" => self.terre = c,
            "pierre" => self.pierre = c,
            "eclats_pierre" => self.eclats_pierre = c,
            "tavelures_terre" => self.tavelures_terre = c,
            "brin_clair" => self.brin_clair = c,
            "brin_moyen" => self.brin_moyen = c,
            "brin_sombre" => self.brin_sombre = c,
            "jungle_loin" => self.jungle_loin = c,
            "jungle_moyenne" => self.jungle_moyenne = c,
            "jungle_proche" => self.jungle_proche = c,
            _ => unreachable!("la liste des champs a deja tranche"),
        }
        Ok(())
    }

    /// La palette courante, écrite **telle qu'elle se recolle dans le code du jeu**.
    ///
    /// ⚠ La forme compte autant que le contenu : quand un réglage lui plaît, il doit devenir le
    /// défaut **sans être retranscrit à la main**. Une transcription, c'est une virgule perdue et un
    /// rendu qu'on ne retrouve plus — et on ne saurait même pas que c'est ça.
    pub fn decrire(&self) -> String {
        let l = |nom: &str, c: [f32; 3]| {
            format!("    {nom}: [{:.3}, {:.3}, {:.3}],\n", c[0], c[1], c[2])
        };
        let mut s = String::from("Palette {\n");
        s.push_str(&l("herbe", self.herbe));
        s.push_str(&l("terre", self.terre));
        s.push_str(&l("pierre", self.pierre));
        s.push_str(&l("eclats_pierre", self.eclats_pierre));
        s.push_str(&l("tavelures_terre", self.tavelures_terre));
        s.push_str(&l("brin_clair", self.brin_clair));
        s.push_str(&l("brin_moyen", self.brin_moyen));
        s.push_str(&l("brin_sombre", self.brin_sombre));
        s.push_str(&l("jungle_loin", self.jungle_loin));
        s.push_str(&l("jungle_moyenne", self.jungle_moyenne));
        s.push_str(&l("jungle_proche", self.jungle_proche));
        s.push('}');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_palette_par_defaut_porte_exactement_les_couleurs_d_avant() {
        // ⚠ LE TÉMOIN DU DÉPLACEMENT. Ces huit valeurs étaient dispersées dans
        // `party_render_pass.rs` avant le 31 août 2026. Si ce test tombe, ce n'est pas lui qu'il
        // faut corriger : c'est qu'une couleur a changé sans qu'on le décide.
        let p = Palette::default();
        assert_eq!(p.herbe, [0.32, 0.82, 0.36]);
        assert_eq!(p.terre, [0.48, 0.32, 0.20]);
        assert_eq!(p.pierre, [0.60, 0.63, 0.68]);
        assert_eq!(p.eclats_pierre, [0.40, 0.43, 0.48]);
        assert_eq!(p.tavelures_terre, [0.42, 0.27, 0.16]);
        assert_eq!(p.brin_clair, [0.32, 0.90, 0.35]);
        assert_eq!(p.brin_moyen, [0.20, 0.78, 0.26]);
        assert_eq!(p.brin_sombre, [0.14, 0.65, 0.20]);
    }

    #[test]
    fn chaque_champ_annonce_se_regle_vraiment() {
        // La garde qui rend impossible le défaut qu'`Ambiance` décrit : un champ inscrit dans la
        // liste mais oublié dans le `match` serait annoncé à la console et sans effet.
        for champ in Palette::CHAMPS {
            let mut p = Palette::default();
            p.regler(champ, &[0.111, 0.222, 0.333])
                .unwrap_or_else(|e| panic!("{champ} devrait se regler : {e}"));
            assert_ne!(p, Palette::default(), "{champ} est annonce mais ne change rien");
        }
    }

    #[test]
    fn une_couleur_inconnue_dit_lesquelles_existent() {
        let mut p = Palette::default();
        let e = p.regler("gazon", &[0.1, 0.2, 0.3]).unwrap_err();
        assert!(e.contains("herbe"), "le message doit lister les noms connus : {e}");
    }

    #[test]
    fn une_couleur_veut_trois_composantes() {
        let mut p = Palette::default();
        assert!(p.regler("herbe", &[0.5]).is_err());
        assert!(p.regler("herbe", &[0.5, 0.5, 0.5, 0.5]).is_err());
        assert_eq!(p, Palette::default(), "un refus ne doit rien avoir change");
    }

    #[test]
    fn une_composante_qui_n_a_pas_de_sens_est_refusee() {
        let mut p = Palette::default();
        assert!(p.regler("herbe", &[-0.1, 0.5, 0.5]).is_err());
        assert!(p.regler("herbe", &[f32::NAN, 0.5, 0.5]).is_err());
        assert!(p.regler("herbe", &[f32::INFINITY, 0.5, 0.5]).is_err());
        assert_eq!(p, Palette::default());
    }

    #[test]
    fn une_couleur_au_dela_de_un_est_acceptee_car_elle_emet() {
        // Dans une chaine HDR, dépasser 1 n'est pas une erreur : c'est ce qui fait rayonner une
        // surface et déclenche le halo. Brider ici interdirait de trouver ce qu'on n'a pas prévu.
        let mut p = Palette::default();
        assert!(p.regler("herbe", &[2.5, 2.5, 2.5]).is_ok());
    }

    #[test]
    fn la_description_se_recolle_dans_le_code() {
        let d = Palette::default().decrire();
        assert!(d.starts_with("Palette {"));
        // Chaque champ annoncé doit apparaître dans la description, sinon un réglage trouvé à
        // l'œil se perdrait au moment de le rendre définitif.
        for champ in Palette::CHAMPS {
            assert!(d.contains(champ), "{champ} manque dans la description");
        }
    }
}
