use aegis_engine::math::{Vec2, Vec3};
use crate::grid::{TileGrid, TileType};
use crate::traps::{TrapManager, TrapKind};

/// Où poser l'objet n° `index` parmi `total_items`, à l'intérieur du carton ouvert, et à quelle
/// échelle le dessiner.
///
/// # ⚠ Le défaut corrigé le 21 août 2026 : les objets partaient aux quatre coins
///
/// L'espacement se calculait `width_span / (cols - 1)`, c'est-à-dire **toute la largeur du carton
/// divisée par le nombre de colonnes moins une**. À 38 objets (7 colonnes) cela donne 1,27 — une
/// grille dense et pleine, celle qu'on avait réglée à l'œil et qui rendait bien.
///
/// Mais le tirage vaut `nombre de joueurs + 3`. **Seul, on tire donc 4 objets** : deux colonnes,
/// un espacement de 7,6, et les quatre objets se retrouvent plaqués aux quatre coins extrêmes du
/// carton — l'air d'avoir été jetés dehors. Le réglage n'était juste que pour le nombre auquel on
/// l'avait éprouvé.
///
/// La correction **borne** l'espacement et **centre** la grille sur l'étendue réellement occupée.
/// À 38 objets le pas calculé (1,27) reste sous la borne, donc **rien ne change** de ce qui
/// plaisait ; à 4 objets, ils se regroupent au milieu au lieu de fuir vers les bords.
/// **QUEL OBJET DU CARTON VISE-T-ON ?** — la visée, sortie de la boucle d'événements.
///
/// Elle vivait dans `main.rs`, mêlée au traitement du clic, et elle y était FAUSSE : elle
/// recalculait la position du carton à la main (`w/2, h/2, 0`) là où le rendu la prend de
/// `CardboardBoxObject::position` (`w×0,5, h×0,5 − 0,5, 12.0`). **Douze unités d'écart en
/// profondeur.** Les objets étaient donc cherchés loin devant l'endroit où ils sont peints, et le
/// clic tombait sur le voisin. Ses mots, en jouant : *« j'ai cliqué sur un cube de droite, ça m'a
/// pris un laser »*.
///
/// Elle prend la TAILLE DE LA CARTE, jamais une position toute faite : c'est ce qui rend la
/// divergence impossible plutôt qu'improbable. Aucun appelant ne peut plus lui donner un carton
/// qui n'est pas celui qu'on voit — et un test peut enfin l'éprouver, ce qu'aucun test ne pouvait
/// faire tant que le calcul dormait au milieu d'une boucle d'événements.
///
/// Le rayon de tolérance est une FRACTION de la hauteur d'écran, pas un nombre de pixels : la
/// précision d'un humain ne change pas parce qu'il a acheté un écran plus fin.
pub fn objet_vise(
    camera: &aegis_engine::scene::camera::Camera,
    largeur_carte: f32,
    hauteur_carte: f32,
    total: usize,
    souris: (f32, f32),
    largeur_px: f32,
    hauteur_px: f32,
) -> Option<usize> {
    if total == 0 {
        return None;
    }
    let box_pos = crate::objects::cardboard_box::CardboardBoxObject::position(
        largeur_carte,
        hauteur_carte,
    );
    let rayon = hauteur_px * RAYON_VISEE;
    let mut meilleur = None;
    let mut plus_proche = rayon * rayon;
    for i in 0..total {
        let (offset, _) = compute_box_item_offset(i, total);
        let Some((sx, sy)) = camera.projeter_vers_ecran(box_pos + offset, largeur_px, hauteur_px)
        else {
            continue;
        };
        let (dx, dy) = (souris.0 - sx, souris.1 - sy);
        let d2 = dx * dx + dy * dy;
        if d2 < plus_proche {
            plus_proche = d2;
            meilleur = Some(i);
        }
    }
    meilleur
}

/// La tolérance de visée, en fraction de la hauteur d'écran.
///
/// Elle valait « 90 pixels » : généreuse sur un portable, deux fois plus serrée sur un 4K pour
/// exactement la même image.
const RAYON_VISEE: f32 = 0.07;

pub fn compute_box_item_offset(index: usize, total_items: usize) -> (Vec3, f32) {
    if total_items == 0 {
        return (Vec3::ZERO, 1.0);
    }

    // Calcul dynamique de la grille (Colonnes x Rangées)
    let cols = match total_items {
        1..=4 => 2,
        5..=6 => 3,
        7..=12 => 4,
        13..=20 => 5,
        21..=30 => 6,
        _ => 7,
    };

    let rows = total_items.div_ceil(cols);
    let col = index % cols;
    let row = index / cols;

    // ⚠ LA CAVITÉ UTILE N'EST PAS CELLE DE L'OUVERTURE — c'est ce qui a fait rater les DEUX
    // réglages précédents (22 août 2026).
    //
    // `bornes_carton` mesure le MODÈLE ouvert : demi-largeur 0,8185/2 × 11,5 = 4,7. Mais c'est la
    // boîte englobante, donc l'OUVERTURE — et les objets sont posés au FOND (`off_z` = 12,2, fond
    // à 12,0), où les parois ont convergé. Disposer sur 4,7 place les objets là où il n'y a plus
    // de carton : c'est le défaut qu'il a vu en premier, « les items sont en dehors de la boîte ».
    // Puis les borner à 2,4 a produit l'inverse, « beaucoup trop proches, encore beaucoup de vide ».
    //
    // La cavité au PLAN DES OBJETS est mesurée sur sa capture, avec une échelle donnée par le
    // réglage connu : l'écart entre deux objets valait 2,4 unités pour 264 px, soit 116 px/unité ;
    // la cavité du fond s'étend sur ±295 px du centre → **±2,5 unités**. Cette valeur explique les
    // deux plaintes d'un coup, ce qu'aucune des deux précédentes ne faisait : à ±3,8 les objets
    // sortaient bel et bien, à ±1,2 il restait la moitié de la cavité vide.
    //
    // ⚠ Elle vient d'une capture, pas d'un outil : c'est SON œil qui tranche en jeu, et un
    // `bornes_carton` qui saurait donner la section au plan Z des objets la remplacerait.
    const DEMI_LARGEUR: f32 = 2.5;
    const DEMI_HAUTEUR: f32 = 2.0;
    // La grille est posée un peu bas dans le carton (le rabat avant masque le tiers inférieur) :
    // ce décalage doit être RETIRÉ de l'étendue utile, sinon la rangée du bas sort par en dessous.
    const DESCENTE: f32 = 0.4;

    // L'échelle est calculée AVANT la position, parce que c'est elle qui dit combien de place
    // chaque objet prend — et donc combien il en reste pour les écarter.
    let scale_mult = match total_items {
        1..=4 => 1.25,
        5..=9 => 0.95,
        10..=16 => 0.70,
        17..=25 => 0.55,
        26..=36 => 0.45,
        _ => 0.38,
    };

    // ⚠ LES DEUX BORNES ARBITRAIRES ONT DISPARU (`PAS_MAX_X` = 2,4 / `PAS_MAX_Y` = 1,6).
    // Elles avaient été posées à l'œil le matin même pour empêcher quatre objets de fuir aux
    // parois, et elles réglaient le mauvais problème : le défaut n'était pas que les objets
    // s'écartaient, c'est qu'ils DÉBORDAIENT. Les serrer au centre corrigeait le symptôme en
    // créant l'inverse — « beaucoup trop proches, encore beaucoup de vide ».
    //
    // La règle juste ne se règle pas, elle se DÉDUIT : un objet ne doit pas dépasser la paroi,
    // donc son CENTRE ne peut se poser que dans l'étendue amputée de son propre encombrement.
    // Les objets remplissent alors tout l'espace qu'ils peuvent occuper, à n'importe quel nombre,
    // et le débordement devient impossible par construction plutôt que par réglage.
    //
    // Le ×1.25 est le grossissement de l'objet SURVOLÉ (`party_render_pass`) : c'est lui qui
    // décide du débordement, pas l'état au repos — viser l'état au repos ferait mordre la paroi
    // au premier survol.
    let encombrement = scale_mult * 1.25;

    let utile_x = (2.0 * DEMI_LARGEUR - encombrement).max(0.0);
    let utile_y = (2.0 * (DEMI_HAUTEUR - DESCENTE) - encombrement).max(0.0);

    let dx = if cols > 1 { utile_x / (cols - 1) as f32 } else { 0.0 };
    let dy = if rows > 1 { utile_y / (rows - 1) as f32 } else { 0.0 };

    // On centre sur l'étendue RÉELLEMENT occupée : un tirage qui ne remplit pas sa dernière
    // rangée reste centré au lieu de pencher d'un côté.
    let start_x = -dx * (cols - 1) as f32 * 0.5;
    let start_y = -dy * (rows - 1) as f32 * 0.5 - DESCENTE;

    let off_x = start_x + (col as f32) * dx;
    let off_y = start_y + (row as f32) * dy;
    let off_z = 12.2; // Lancement légèrement devant le fond du carton (Z = 12.0) pour une visibilité 100% parfaite !


    (Vec3::new(off_x, off_y, off_z), scale_mult)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemType {
    SolidBlock,
    GrassBlock,
    MetalBlock,
    IceBlock,
    LavaBlock,
    SawBlade,
    CannonTurret,
    SpikeTrap,
    LaserEmitter,
    Flamethrower,
}

impl ItemType {
    pub fn all_types() -> Vec<ItemType> {
        vec![
            ItemType::SolidBlock,
            ItemType::GrassBlock,
            ItemType::MetalBlock,
            ItemType::IceBlock,
            ItemType::LavaBlock,
            ItemType::SawBlade,
            ItemType::CannonTurret,
            ItemType::SpikeTrap,
            ItemType::LaserEmitter,
            ItemType::Flamethrower,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ItemType::SolidBlock => "Bloc Terre",
            ItemType::GrassBlock => "Bloc Herbe",
            ItemType::MetalBlock => "Bloc Métal",
            ItemType::IceBlock => "Bloc Glace (Glissant)",
            ItemType::LavaBlock => "Bloc Lave (Mortel)",
            ItemType::SawBlade => "Scie Rotative 3D",
            ItemType::CannonTurret => "Tourelle Canon 3D",
            ItemType::SpikeTrap => "Dalle à Pics 3D",
            ItemType::LaserEmitter => "Émetteur Laser 3D",
            ItemType::Flamethrower => "Cracheur de Feu 3D",
        }
    }
}

pub struct MysteryBox {
    pub available_items: Vec<ItemType>,
    pub selected_index: Option<usize>,
    pub cursor_grid: (usize, usize),
}

impl MysteryBox {
    pub fn new() -> Self {
        let mut box_obj = Self {
            available_items: Vec::new(),
            selected_index: None,
            cursor_grid: (10, 5),
        };
        // Mode Test Visuel Réel 35 Joueurs -> 35 + 3 = 38 objets affichés en direct en jeu !
        box_obj.generate_round_draft(35);
        box_obj
    }

    /// Génère le tirage du carton mystère selon la règle : N joueurs + 3 propositions supplémentaires !
    pub fn generate_round_draft(&mut self, player_count: usize) {
        let total_items = (player_count + 3).clamp(4, 45);
        let all_types = ItemType::all_types();
        let mut items = Vec::with_capacity(total_items);

        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(123456789);

        for _ in 0..total_items {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let idx = ((seed >> 33) as usize) % all_types.len();
            items.push(all_types[idx].clone());
        }

        self.available_items = items;
        self.selected_index = if !self.available_items.is_empty() { Some(0) } else { None };
    }

    pub fn select_item(&mut self, index: usize) {
        if index < self.available_items.len() {
            self.selected_index = Some(index);
        }
    }

    pub fn place_selected_item(
        &mut self,
        grid: &mut TileGrid,
        traps: &mut TrapManager,
        owner_id: u32,
        dir: crate::traps::Direction,
    ) -> bool {
        let idx = match self.selected_index {
            Some(i) => i,
            None => return false,
        };

        let (gx, gy) = self.cursor_grid;
        if gx >= grid.width || gy >= grid.height {
            return false;
        }

        let item = &self.available_items[idx];
        let pos = Vec2::new(gx as f32 + 0.5, gy as f32 + 0.5);

        match item {
            ItemType::SolidBlock => grid.set_tile(gx, gy, TileType::SolidBlock),
            ItemType::GrassBlock => grid.set_tile(gx, gy, TileType::GrassBlock),
            ItemType::MetalBlock => grid.set_tile(gx, gy, TileType::MetalBlock),
            ItemType::IceBlock => grid.set_tile(gx, gy, TileType::Ice),
            ItemType::LavaBlock => grid.set_tile(gx, gy, TileType::Lava),
            ItemType::SawBlade => traps.add_trap(pos, TrapKind::SawBlade { radius: 0.75, rotation: 0.0 }, owner_id),
            ItemType::CannonTurret => traps.add_trap(pos, TrapKind::CannonTurret { dir, fire_rate: 2.5, timer: 0.0 }, owner_id),
            ItemType::SpikeTrap => traps.add_trap(pos, TrapKind::SpikeTrap, owner_id),
            ItemType::LaserEmitter => traps.add_trap(pos, TrapKind::LaserEmitter { dir, active: false, timer: 0.0 }, owner_id),
            ItemType::Flamethrower => traps.add_trap(pos, TrapKind::Flamethrower { dir, active: false, timer: 0.0 }, owner_id),
        }

        // Remove item after placing
        self.available_items.remove(idx);
        self.selected_index = if !self.available_items.is_empty() { Some(0) } else { None };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera_de_partie(aspect: f32) -> aegis_engine::scene::camera::Camera {
        // Les mêmes réglages que `PartyRenderPass::camera` : c'est la caméra qu'on regarde.
        aegis_engine::scene::camera::Camera {
            position: Vec3::new(24.0, 12.0, 28.0),
            target: Vec3::new(24.0, 12.0, 0.0),
            up: Vec3::Y,
            fov_y_radians: 38.0f32.to_radians(),
            aspect_ratio: aspect,
            z_near: 0.1,
            z_far: 500.0,
        }
    }

    /// ⚠ **LE TEST QUI AURAIT ATTRAPÉ LE DÉFAUT DU 29 AOÛT** : cliquer exactement sur un objet
    /// doit sélectionner CET objet, et pas son voisin.
    ///
    /// Il n'existait pas parce que le calcul dormait au milieu de la boucle d'événements de
    /// `main.rs`, où rien ne peut l'éprouver. Le défaut a donc été trouvé en jouant — « j'ai
    /// cliqué sur un cube de droite, ça m'a pris un laser » — et il aurait pu y rester longtemps :
    /// une visée décalée ne plante pas, elle donne un mauvais objet, ce qui ressemble à de la
    /// maladresse plutôt qu'à un bug.
    ///
    /// Vérifié par mutation : remettre la position d'alors (`w/2, h/2, 0.0`, douze unités devant)
    /// le fait tomber sur plusieurs tailles de tirage.
    #[test]
    fn cliquer_sur_un_objet_selectionne_cet_objet_la() {
        let (w, h) = (1920.0_f32, 1080.0_f32);
        let cam = camera_de_partie(w / h);
        let (carte_l, carte_h) = (48.0_f32, 24.0_f32);
        let box_pos = crate::objects::cardboard_box::CardboardBoxObject::position(carte_l, carte_h);

        for total in [1usize, 2, 4, 6, 9, 12, 20] {
            for i in 0..total {
                let (offset, _) = compute_box_item_offset(i, total);
                let (sx, sy) = cam
                    .projeter_vers_ecran(box_pos + offset, w, h)
                    .expect("un objet du carton est devant la camera");
                assert_eq!(
                    objet_vise(&cam, carte_l, carte_h, total, (sx, sy), w, h),
                    Some(i),
                    "a {total} objets, viser le centre du {i} doit rendre le {i}"
                );
            }
        }
    }

    /// Un clic loin du carton ne sélectionne RIEN.
    ///
    /// Sans cette borne, la visée rendrait toujours « le moins loin », donc un clic à l'autre bout
    /// de l'écran choisirait un objet — le joueur poserait un piège qu'il n'a pas demandé.
    #[test]
    fn un_clic_loin_du_carton_ne_selectionne_rien() {
        let (w, h) = (1920.0_f32, 1080.0_f32);
        let cam = camera_de_partie(w / h);
        assert_eq!(objet_vise(&cam, 48.0, 24.0, 6, (5.0, 5.0), w, h), None);
        // Et un carton vide ne rend jamais d'indice, même au centre.
        assert_eq!(objet_vise(&cam, 48.0, 24.0, 0, (w / 2.0, h / 2.0), w, h), None);
    }

    #[test]
    fn test_n_plus_3_draft_generation() {
        let mut box_obj = MysteryBox::new();

        // 1 joueur -> 4 items
        box_obj.generate_round_draft(1);
        assert_eq!(box_obj.available_items.len(), 4);

        // 2 joueurs -> 5 items
        box_obj.generate_round_draft(2);
        assert_eq!(box_obj.available_items.len(), 5);

        // 9 joueurs -> 12 items
        box_obj.generate_round_draft(9);
        assert_eq!(box_obj.available_items.len(), 12);

        // 40 joueurs -> 43 items
        box_obj.generate_round_draft(40);
        assert_eq!(box_obj.available_items.len(), 43);
    }

    #[test]
    fn test_adaptive_grid_offsets_stay_inside_box() {
        let total_items = 43; // 40 joueurs + 3
        for i in 0..total_items {
            let (offset, scale) = compute_box_item_offset(i, total_items);
            // Les bornes sont celles de la CAVITÉ AU PLAN DES OBJETS (cf. `compute_box_item_offset`),
            // pas celles de l'ouverture du carton : c'est la confusion des deux qui a fait sortir
            // les objets de la boîte le 21 août.
            assert!(offset.x >= -2.5 && offset.x <= 2.5, "Offset X hors cavité : {}", offset.x);
            assert!(offset.y >= -2.0 && offset.y <= 2.0, "Offset Y hors cavité : {}", offset.y);
            assert!(scale > 0.1 && scale <= 1.25);
        }
    }

    /// **LA RÈGLE QUI REMPLACE DEUX RÉGLAGES RATÉS : remplir la cavité sans jamais en sortir.**
    ///
    /// Ce test a pris la place d'une « non-régression à 38 objets » qui figeait `-3,8`. Elle
    /// gardait un réglage qui **débordait** : 3,8 dépasse la demi-cavité de 2,5 au plan où les
    /// objets sont posés. Un test de non-régression qui protège un défaut est pire qu'aucun test,
    /// parce qu'il interdit de le corriger.
    ///
    /// Il MORD des deux côtés, et c'est ce qui compte — les deux échecs de la journée sont chacun
    /// attrapés par une moitié :
    ///   - trop écarté → un objet sort du carton (plainte du matin) ;
    ///   - trop serré  → la moitié de la cavité reste vide (plainte de l'après-midi).
    #[test]
    fn la_grille_remplit_la_cavite_sans_jamais_en_sortir() {
        const DEMI_L: f32 = 2.5;
        const DEMI_H: f32 = 2.0;
        for total in 1..=45 {
            let mut bord_x: f32 = 0.0;
            for i in 0..total {
                let (p, s) = compute_box_item_offset(i, total);
                // Le ×1.25 du survol compte : c'est l'état le plus encombrant qui décide.
                let demi = s * 1.25 * 0.5;
                assert!(
                    p.x.abs() + demi <= DEMI_L + 1e-3,
                    "à {total} objets, le n°{i} sort par le côté : {:.2} > {DEMI_L}",
                    p.x.abs() + demi
                );
                assert!(
                    p.y.abs() + demi <= DEMI_H + 1e-3,
                    "à {total} objets, le n°{i} sort en haut ou en bas : {:.2} > {DEMI_H}",
                    p.y.abs() + demi
                );
                bord_x = bord_x.max(p.x.abs() + demi);
            }
            // ET l'autre moitié : la grille doit VRAIMENT occuper la cavité. À 4 objets, le
            // réglage borné du matin n'atteignait que 1,82 sur 2,5 — d'où « beaucoup de vide ».
            if total > 1 {
                assert!(
                    bord_x >= DEMI_L * 0.9,
                    "à {total} objets la grille n'occupe que {bord_x:.2} sur {DEMI_L} : trop serrée"
                );
            }
        }
    }

    /// **LE DÉFAUT QU'IL A VU EN JOUANT : seul, les objets partaient aux quatre coins.**
    ///
    /// Le tirage vaut « joueurs + 3 » : seul, on tire 4 objets, l'espacement valait alors 7,6 —
    /// toute la largeur du carton — et les quatre se plaquaient contre les parois.
    #[test]
    fn a_quatre_objets_ils_se_groupent_au_centre_au_lieu_de_fuir_aux_coins() {
        let mut ecart_max: f32 = 0.0;
        for i in 0..4 {
            let (p, _) = compute_box_item_offset(i, 4);
            ecart_max = ecart_max.max(p.x.abs());
        }
        // Avant la première correction ce chiffre valait 3,8 — hors de la cavité (demi 2,5).
        // Après elle, 1,2 — soit moins de la moitié de la cavité, d'où « beaucoup trop proches ».
        // La fourchette dit les DEUX défauts à la fois plutôt qu'un seul.
        assert!(
            (1.5..=2.0).contains(&ecart_max),
            "à 4 objets, le plus écarté est à {ecart_max:.2} : hors de la fourchette [1,5 ; 2,0] \
             (au-dessus ils sortent du carton, en dessous ils se tassent au centre)"
        );
    }

    /// **Aucun objet ne doit sortir du carton, quel que soit le nombre de joueurs.**
    ///
    /// ⚠ 4,7 est la demi-largeur de l'OUVERTURE (`bornes_carton` : modèle 0,8185 × échelle 11,5).
    /// Les objets étant posés au FOND, cette borne est bien trop permissive pour attraper quoi que
    /// ce soit — elle laissait passer le débordement qu'il a vu à l'écran. Elle est conservée comme
    /// garde-fou grossier ; c'est `la_grille_remplit_la_cavite_sans_jamais_en_sortir` qui mord.
    #[test]
    fn aucun_objet_ne_sort_du_carton_quel_que_soit_le_nombre_de_joueurs() {
        const DEMI_LARGEUR: f32 = 4.7;
        for total in 1..=45 {
            for i in 0..total {
                let (p, s) = compute_box_item_offset(i, total);
                let bord = p.x.abs() + s * 0.5;
                assert!(
                    bord <= DEMI_LARGEUR,
                    "avec {total} objets, le n°{i} déborde : bord à {bord:.2} > {DEMI_LARGEUR}"
                );
            }
        }
    }

    /// La grille reste CENTRÉE : le premier et le dernier objet d'une rangée sont à égale distance
    /// du milieu. Sans ça, un tirage pair paraîtrait décalé dans la boîte.
    #[test]
    fn la_grille_est_symetrique_autour_du_centre() {
        for total in [4usize, 6, 12, 20, 30, 38] {
            let cols = match total {
                1..=4 => 2,
                5..=6 => 3,
                7..=12 => 4,
                13..=20 => 5,
                21..=30 => 6,
                _ => 7,
            };
            let (premier, _) = compute_box_item_offset(0, total);
            let (dernier, _) = compute_box_item_offset(cols - 1, total);
            assert!(
                (premier.x + dernier.x).abs() < 1e-4,
                "à {total} objets la rangée n'est pas centrée : {} et {}",
                premier.x,
                dernier.x
            );
        }
    }
}
