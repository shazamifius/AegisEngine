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

    // Dimensions intérieures maximales du carton mystère (Width = 7.6, Height = 4.4)
    let width_span = 7.6f32;
    let height_span = 4.4f32;

    // ⚠ LES DEUX BORNES QUI CORRIGENT LE DÉFAUT. Elles sont choisies pour être SANS EFFET sur une
    // grille pleine — à 7 colonnes le pas vaut 1,27 et à 6 rangées 0,88, tous deux en dessous —
    // et pour ne mordre que sur les tirages peu nombreux, ceux qui s'éparpillaient.
    const PAS_MAX_X: f32 = 2.4;
    const PAS_MAX_Y: f32 = 1.6;

    let dx = if cols > 1 { (width_span / (cols - 1) as f32).min(PAS_MAX_X) } else { 0.0 };
    let dy = if rows > 1 { (height_span / (rows - 1) as f32).min(PAS_MAX_Y) } else { 0.0 };

    // On centre sur l'étendue RÉELLEMENT occupée, et non sur celle du carton : c'est ce qui fait
    // qu'un petit tirage se groupe au milieu au lieu de coller aux parois.
    let start_x = -dx * (cols - 1) as f32 * 0.5;
    let start_y = -dy * (rows - 1) as f32 * 0.5 - 0.4;

    let off_x = start_x + (col as f32) * dx;
    let off_y = start_y + (row as f32) * dy;
    let off_z = 12.2; // Lancement légèrement devant le fond du carton (Z = 12.0) pour une visibilité 100% parfaite !

    // Échelle adaptative garantissant que TOUS les objets (GLB & blocs) restent TOUJOURS parfaitement dimensionnés
    let scale_mult = match total_items {
        1..=4 => 1.25,
        5..=9 => 0.95,
        10..=16 => 0.70,
        17..=25 => 0.55,
        26..=36 => 0.45,
        _ => 0.38,
    };

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
            // Vérifie que les offsets restent strictement dans les dimensions intérieures du carton (-2.7 <= X <= 2.7, -1.8 <= Y <= 1.8)
            assert!(offset.x >= -3.85 && offset.x <= 3.85, "Offset X out of box bounds: {}", offset.x);
            assert!(offset.y >= -2.65 && offset.y <= 2.65, "Offset Y out of box bounds: {}", offset.y);
            assert!(scale > 0.1 && scale <= 1.25);
        }
    }

    /// **NON-RÉGRESSION — le réglage à 38 objets ne doit pas bouger d'un millimètre.**
    ///
    /// C'est celui qu'il avait éprouvé à l'œil et qui rendait bien ; la correction ne vise QUE les
    /// petits tirages. Les bornes sont choisies pour rester sans effet ici, et ce test l'exige
    /// plutôt que de l'espérer : à 7 colonnes le pas vaut 1,267, à 6 rangées 0,88, tous deux sous
    /// les bornes — donc le centrage retombe exactement sur l'ancien `-width_span * 0.5`.
    #[test]
    fn a_trente_huit_objets_la_grille_est_identique_a_avant() {
        let (p0, s0) = compute_box_item_offset(0, 38);
        assert!((p0.x - (-3.8)).abs() < 1e-4, "premier objet en x = {} au lieu de -3.8", p0.x);
        assert!((p0.y - (-2.6)).abs() < 1e-4, "premier objet en y = {} au lieu de -2.6", p0.y);
        assert!((s0 - 0.38).abs() < 1e-6);

        // Le dernier de la première rangée doit toujours atteindre le bord opposé.
        let (p6, _) = compute_box_item_offset(6, 38);
        assert!((p6.x - 3.8).abs() < 1e-4, "septième objet en x = {} au lieu de 3.8", p6.x);
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
        // Avant la correction, ce chiffre valait 3,8 — la paroi elle-même.
        assert!(
            ecart_max < 2.0,
            "à 4 objets, le plus écarté est à {ecart_max:.2} du centre : ils s'éparpillent encore"
        );
    }

    /// **Aucun objet ne doit sortir du carton, quel que soit le nombre de joueurs.**
    ///
    /// La demi-largeur intérieure vaut 4,7 : mesurée, pas supposée — `cargo run --bin bornes_carton`
    /// donne une largeur de modèle de 0,8185, multipliée par l'échelle d'ouverture 11,5.
    /// Le test balaie toute la plage possible (4 objets pour un joueur seul, 45 au plafond).
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
