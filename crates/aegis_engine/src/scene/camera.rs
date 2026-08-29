use crate::core::math::{Mat4, Vec3};

/// Caméra 3D avec support de la Stéréoscopie VR (Œil Gauche / Œil Droit) et de la Projection Perspective.
///
/// ### Théorie Graphique :
/// La caméra génère les matrices de Vue (View) et de Projection (Projection).
/// En VR, la matrice de vue est décalée latéralement d'une demi-distance inter-oculaire (IPD/2)
/// pour simuler l'écartement naturel de la rétine humaine.
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub aspect_ratio: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    /// Crée une nouvelle caméra avec des paramètres de perspective standard.
    pub fn new(position: Vec3, target: Vec3, aspect_ratio: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y_radians: 60.0f32.to_radians(),
            aspect_ratio,
            z_near: 0.1,
            z_far: 1000.0,
        }
    }

    /// Calcule la Matrice de Vue globale (View Matrix) pour un observateur unique.
    pub fn compute_view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// La matrice de projection perspective.
    ///
    /// ⚠ **ELLE INVERSAIT L'AXE Y, ET C'ÉTAIT FAUX POUR CE MOTEUR** (corrigé le 29 août 2026).
    /// Le raisonnement d'alors était juste dans l'absolu — Vulkan fait descendre `y` — mais il ne
    /// tenait pas compte de la chaîne réelle : **tous les shaders d'Aegis sont écrits en WGSL et
    /// compilés par `naga`**, dont les options par défaut portent `ADJUST_COORDINATE_SPACE`. Le
    /// SPIR-V produit retourne donc déjà l'axe Y, et l'inverser une seconde fois ici remettait la
    /// scène à l'envers.
    ///
    /// Le défaut dormait parce que **cette caméra n'était utilisée par personne** : le jeu
    /// recalculait ses matrices à la main, avec la bonne convention. La brancher naïvement aurait
    /// mis tout le rendu la tête en bas — et aucun test ne l'aurait vu, puisqu'un test de matrice
    /// vérifie la cohérence avec la convention qu'on lui donne, jamais que cette convention est
    /// celle du moteur. *Seule une capture d'écran tranche ce genre de question.*
    pub fn compute_projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_radians, self.aspect_ratio, self.z_near, self.z_far)
    }

    /// Où un point du monde se retrouve **à l'écran**, en pixels — ou `None` s'il est derrière.
    ///
    /// Écrite ici, et une seule fois, parce que le contraire s'est produit : le jeu refaisait ce
    /// calcul à la main à deux endroits, en **recopiant les paramètres de la caméra** (`38°`,
    /// `0,1`, `500`) déjà écrits dans sa passe de rendu. Trois copies de la même vérité, dont deux
    /// servaient à savoir *ce qu'on a cliqué* : changer le champ de vision du rendu faisait viser
    /// les clics à côté, **sans que rien ne le signale** — on aurait cherché le défaut dans la
    /// détection de clic, pas dans une constante recopiée trois lignes plus loin.
    ///
    /// L'origine est en **haut à gauche**, comme la souris la donne.
    pub fn projeter_vers_ecran(
        &self,
        point: Vec3,
        largeur_px: f32,
        hauteur_px: f32,
    ) -> Option<(f32, f32)> {
        let vp = self.compute_projection_matrix() * self.compute_view_matrix();
        let clip = vp * crate::core::math::Vec4::new(point.x, point.y, point.z, 1.0);
        // `w <= 0` : le point est derrière l'observateur. Diviser donnerait une position d'écran
        // parfaitement plausible et complètement fausse — le genre de résultat qu'on ne remet
        // jamais en question parce qu'il ressemble à un vrai.
        if clip.w <= 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        Some((
            (ndc_x + 1.0) * 0.5 * largeur_px,
            (1.0 - ndc_y) * 0.5 * hauteur_px,
        ))
    }

    /// L'inverse : **où un point de l'écran tombe dans le monde**, sur le plan `z = hauteur_plan`.
    ///
    /// C'est ce dont un jeu en 2,5D a besoin pour savoir sur quelle case on vient de cliquer : on
    /// lance un rayon depuis le pixel et on regarde où il traverse le plan du décor. Rend `None`
    /// quand le rayon est parallèle au plan — il n'y a alors pas de réponse, et en inventer une
    /// placerait un objet n'importe où plutôt que de ne rien faire.
    ///
    /// Vit ici pour la même raison que son jumeau : le jeu le recalculait à la main en recopiant
    /// les réglages de la caméra, si bien que deux « vérités » coexistaient sur ce qu'on regarde.
    pub fn point_sur_plan_z(
        &self,
        x_px: f32,
        y_px: f32,
        largeur_px: f32,
        hauteur_px: f32,
        hauteur_plan: f32,
    ) -> Option<Vec3> {
        if largeur_px <= 0.0 || hauteur_px <= 0.0 {
            return None;
        }
        let ndc_x = (x_px / largeur_px) * 2.0 - 1.0;
        let ndc_y = 1.0 - (y_px / hauteur_px) * 2.0;
        let inv = (self.compute_projection_matrix() * self.compute_view_matrix()).inverse();
        let d = |z: f32| {
            let p = inv * crate::core::math::Vec4::new(ndc_x, ndc_y, z, 1.0);
            Vec3::new(p.x / p.w, p.y / p.w, p.z / p.w)
        };
        let (proche, loin) = (d(0.0), d(1.0));
        let dz = loin.z - proche.z;
        if dz.abs() <= 1e-6 {
            return None;
        }
        let t = (hauteur_plan - proche.z) / dz;
        Some(Vec3::new(
            proche.x + t * (loin.x - proche.x),
            proche.y + t * (loin.y - proche.y),
            hauteur_plan,
        ))
    }

    /// Calcule les Matrices de Vue Stéréoscopiques séparées pour l'Œil Gauche (0) et l'Œil Droit (1).
    ///
    /// ### Effet Stéréoscopique :
    /// En décalant le centre optique de `-ipd/2` pour l'œil gauche et `+ipd/2` pour l'œil droit,
    /// on génère la disparité horizontale exacte ressentie par le système visuel humain.
    pub fn compute_stereo_views(&self, ipd_meters: f32) -> (Mat4, Mat4) {
        let half_ipd = ipd_meters * 0.5;
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(self.up).normalize();

        let left_pos = self.position - right * half_ipd;
        let right_pos = self.position + right * half_ipd;

        let left_view = Mat4::look_at_rh(left_pos, left_pos + forward, self.up);
        let right_view = Mat4::look_at_rh(right_pos, right_pos + forward, self.up);

        (left_view, right_view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠ **CE TEST AFFIRMAIT `proj.cols[1].y < 0.0` — il protégeait le défaut** (29 août 2026).
    ///
    /// Il exigeait l'inversion de l'axe Y, celle-là même qui rendait cette caméra inutilisable
    /// dans un moteur dont les shaders passent par `naga` (qui retourne déjà Y). Autrement dit :
    /// un test vert interdisait de corriger un bug, et l'aurait fait rejeter comme une régression.
    ///
    /// *Il ne pouvait pas mieux faire* : un test de matrice vérifie la cohérence avec la
    /// convention qu'on lui donne, jamais que cette convention est celle du moteur. Le HUD de ce
    /// même moteur était déjà sorti à l'envers avec onze tests au vert. **La seule preuve est une
    /// capture d'écran** — elle a été faite, la scène est identique au pixel près.
    ///
    /// Ce qu'on peut éprouver ici sans rien supposer, en revanche, c'est que les matrices sont
    /// bien formées et que les deux sens se répondent : c'est l'objet des tests qui suivent.
    #[test]
    fn les_matrices_sont_bien_formees() {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 16.0 / 9.0);
        assert!(!cam.compute_view_matrix().is_nan());
        assert!(!cam.compute_projection_matrix().is_nan());
    }

    /// Projeter un point vers l'écran puis re-projeter ce pixel vers le monde redonne le point.
    ///
    /// ⚠ **CE QU'IL PROUVE, ET CE QU'IL NE PROUVE PAS.** J'avais écrit ici qu'il « mord dans les
    /// deux sens, quelle que soit la convention ». C'est FAUX, et la mutation l'a montré tout de
    /// suite : en remettant l'inversion de l'axe Y, ce test **passe toujours**. Les deux fonctions
    /// dérivent de la MÊME matrice, donc une inversion se compense d'elle-même dans l'aller-retour.
    ///
    /// Il garde une vraie utilité — modifier un seul des deux sens le fait tomber — mais la
    /// convention, elle, est éprouvée par le test suivant, et confirmée en dernier ressort par une
    /// capture d'écran. *Un test qui ne peut pas échouer sur une propriété ne la prouve pas, même
    /// quand son nom le laisse croire.*
    #[test]
    fn projeter_puis_revenir_retrouve_le_meme_point() {
        let cam = Camera {
            position: Vec3::new(5.0, 3.0, 16.0),
            target: Vec3::new(5.0, 3.0, 0.0),
            up: Vec3::Y,
            fov_y_radians: 38.0f32.to_radians(),
            aspect_ratio: 16.0 / 9.0,
            z_near: 0.1,
            z_far: 500.0,
        };
        let (w, h) = (1920.0, 1080.0);
        for point in [
            Vec3::new(5.0, 3.0, 0.0),
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(9.0, 6.0, 0.0),
        ] {
            let (sx, sy) = cam.projeter_vers_ecran(point, w, h).expect("devant la caméra");
            let retour = cam.point_sur_plan_z(sx, sy, w, h, 0.0).expect("le rayon coupe le plan");
            assert!(
                (retour.x - point.x).abs() < 1e-2 && (retour.y - point.y).abs() < 1e-2,
                "aller-retour rate : {point:?} -> ecran ({sx:.1},{sy:.1}) -> {retour:?}"
            );
        }
    }

    /// **LA GARDE SUR LA CONVENTION, celle qui manquait** : ce qui est plus HAUT dans le monde
    /// doit s'afficher plus HAUT à l'écran.
    ///
    /// Vérifié par mutation : remettre l'inversion de l'axe Y fait tomber ce test, et lui seul.
    /// C'est donc lui qui tient la convention — pas l'aller-retour, qui se compense, ni une
    /// assertion sur le signe d'un coefficient de matrice, qui décrivait un choix au lieu de
    /// l'éprouver. Le repère d'écran a son origine en haut, donc « plus haut » veut dire `y` plus
    /// PETIT : c'est la seule chose qu'un humain puisse vérifier d'un coup d'œil sur une capture.
    #[test]
    fn ce_qui_est_plus_haut_dans_le_monde_s_affiche_plus_haut_a_l_ecran() {
        let cam = Camera {
            position: Vec3::new(5.0, 3.0, 16.0),
            target: Vec3::new(5.0, 3.0, 0.0),
            up: Vec3::Y,
            fov_y_radians: 38.0f32.to_radians(),
            aspect_ratio: 16.0 / 9.0,
            z_near: 0.1,
            z_far: 500.0,
        };
        let (w, h) = (1920.0, 1080.0);
        let (_, y_bas) = cam.projeter_vers_ecran(Vec3::new(5.0, 0.0, 0.0), w, h).unwrap();
        let (_, y_haut) = cam.projeter_vers_ecran(Vec3::new(5.0, 6.0, 0.0), w, h).unwrap();
        assert!(
            y_haut < y_bas,
            "le point haut du monde doit avoir un y d'ecran plus petit : haut={y_haut:.1} bas={y_bas:.1}"
        );
    }

    /// Un point DERRIÈRE l'observateur n'a pas de position d'écran, et on le dit.
    ///
    /// Diviser par un `w` négatif donnerait une coordonnée parfaitement plausible et fausse — le
    /// genre de résultat qu'on ne remet jamais en question parce qu'il ressemble à un vrai. Ici,
    /// un objet dans le dos deviendrait cliquable.
    #[test]
    fn un_point_derriere_la_camera_n_a_pas_de_position_d_ecran() {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 16.0 / 9.0);
        assert!(cam.projeter_vers_ecran(Vec3::new(0.0, 0.0, 50.0), 800.0, 600.0).is_none());
        assert!(cam.projeter_vers_ecran(Vec3::new(0.0, 0.0, 0.0), 800.0, 600.0).is_some());
    }

    #[test]
    fn test_stereo_eye_separation() {
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0), 1.0);
        let (left_view, right_view) = cam.compute_stereo_views(0.064); // IPD de 64 mm

        // L'œil gauche et l'œil droit doivent produire des matrices de vue différentes
        assert_ne!(left_view, right_view);
    }
}
