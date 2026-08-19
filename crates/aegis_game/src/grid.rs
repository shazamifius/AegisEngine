use aegis_engine::math::{Vec2, Vec4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Air,
    SolidBlock,
    GrassBlock,
    MetalBlock,
    WoodPlank,
    CloudPlatform,
    Lava,
    Ice,
    StickyHoney,
    Portal,
    AntiGravityBubble,
    BlackHole,
    SpikesUp,
    SpikesDown,
    SpikesLeft,
    SpikesRight,
    StartPoint,
    FinishFlag,
}

impl TileType {
    pub fn is_solid(&self) -> bool {
        matches!(
            self,
            TileType::SolidBlock
                | TileType::GrassBlock
                | TileType::MetalBlock
                | TileType::WoodPlank
                | TileType::CloudPlatform
                | TileType::Ice
                | TileType::StickyHoney
        )
    }

    pub fn is_hazard(&self) -> bool {
        matches!(
            self,
            TileType::Lava
                | TileType::SpikesUp
                | TileType::SpikesDown
                | TileType::SpikesLeft
                | TileType::SpikesRight
        )
    }

    pub fn is_ice(&self) -> bool {
        matches!(self, TileType::Ice)
    }

    pub fn is_honey(&self) -> bool {
        matches!(self, TileType::StickyHoney)
    }

    pub fn color(&self) -> Vec4 {
        match self {
            TileType::Air => Vec4::new(0.0, 0.0, 0.0, 0.0),
            TileType::SolidBlock => Vec4::new(0.42, 0.28, 0.18, 1.0),     // Earth dirt brown
            TileType::GrassBlock => Vec4::new(0.25, 0.75, 0.25, 1.0),     // Grass green
            TileType::MetalBlock => Vec4::new(0.55, 0.58, 0.65, 1.0),     // Metal grey
            TileType::WoodPlank => Vec4::new(0.68, 0.45, 0.22, 1.0),      // Oak wood plank
            TileType::CloudPlatform => Vec4::new(0.90, 0.95, 1.0, 0.85),   // Fluffy white cloud
            TileType::Lava => Vec4::new(0.98, 0.35, 0.05, 1.0),           // Glowing lava
            TileType::Ice => Vec4::new(0.45, 0.88, 0.98, 0.85),           // Ice cyan
            TileType::StickyHoney => Vec4::new(0.98, 0.75, 0.10, 0.92),   // Golden honey
            TileType::Portal => Vec4::new(0.65, 0.20, 0.95, 1.0),         // Cosmic portal purple
            TileType::AntiGravityBubble => Vec4::new(0.20, 0.90, 0.95, 0.75), // Anti-gravity bubble
            TileType::BlackHole => Vec4::new(0.08, 0.05, 0.15, 1.0),       // Black hole singularity
            TileType::SpikesUp | TileType::SpikesDown | TileType::SpikesLeft | TileType::SpikesRight => {
                Vec4::new(0.85, 0.15, 0.15, 1.0)                          // Red spikes
            }
            TileType::StartPoint => Vec4::new(0.2, 0.45, 0.95, 1.0),       // Start point blue
            TileType::FinishFlag => Vec4::new(0.98, 0.85, 0.1, 1.0),      // Finish flag gold
        }
    }
}

impl TileType {
    pub fn to_u8(&self) -> u8 {
        match self {
            TileType::Air => 0,
            TileType::SolidBlock => 1,
            TileType::GrassBlock => 2,
            TileType::MetalBlock => 3,
            TileType::StartPoint => 4,
            TileType::FinishFlag => 5,
            _ => 0,
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => TileType::SolidBlock,
            2 => TileType::GrassBlock,
            3 => TileType::MetalBlock,
            4 => TileType::StartPoint,
            5 => TileType::FinishFlag,
            _ => TileType::Air,
        }
    }
}

/// D'où vient la carte en mémoire — et donc si l'on a le droit de réécrire par-dessus.
///
/// Cette distinction n'est pas une subtilité : elle est la seule chose qui sépare « ma carte » de
/// « un terrain plat qui a pris sa place ». Le 12 août 2026, lancer le jeu depuis un autre dossier
/// a fait apparaître le terrain par défaut, et l'éditeur aurait enregistré CE terrain sur le premier
/// bloc posé. La carte a survécu par chance — parce que le dossier de lancement n'était pas celui
/// de la carte. Ce n'est pas une garantie qu'on peut garder.
#[derive(Debug, Clone, PartialEq)]
enum SourceCarte {
    /// Chargée depuis ce fichier : on y réécrit sans crainte, c'est bien la même carte.
    Fichier(std::path::PathBuf),
    /// Aucun fichier de carte nulle part : on joue le terrain par défaut, et l'éditeur a le droit
    /// de CRÉER le fichier ici (rien à écraser).
    Neuve(std::path::PathBuf),
    /// ⛔ Un fichier de carte EXISTE mais n'a pas pu être lu (corrompu, droits, disque). Écrire
    /// par-dessus détruirait un travail qu'on n'a pas su ouvrir. On refuse, et on le dit fort.
    Illisible(std::path::PathBuf),
}

/// Le chemin `lu` — relatif ou absolu — désigne-t-il un fichier posé directement dans `dossier` ?
/// `cwd` sert à résoudre les relatifs, et il est PASSÉ en paramètre plutôt que lu ici : c'est ce qui
/// rend cette règle testable sans toucher au dossier courant du processus.
///
/// ⚠ C'est le cas RELATIF qui compte, et c'est lui qui manquait. Le régisseur lance le jeu avec le
/// répertoire courant déjà placé dans le paquet : le chemin lu est alors le simple
/// « custom_map.lvl », dont le `parent()` est la chaîne vide et n'égalera jamais un dossier absolu.
/// Mesuré le 12 août 2026 : la règle ne se déclenchait pas du tout dans le seul cas pour lequel elle
/// existe, pendant que le journal affichait « carte chargée » avec l'air d'aller très bien.
fn loge_dans(lu: &std::path::Path, dossier: &std::path::Path, cwd: &std::path::Path) -> bool {
    let absolu = if lu.is_absolute() { lu.to_path_buf() } else { cwd.join(lu) };
    absolu.parent() == Some(dossier)
}

/// Le dossier où vivent les cartes ÉDITÉES par la personne qui joue. Sous l'arborescence `~/.web3`
/// du projet, pour que tout ce qui appartient à quelqu'un reste au même endroit chez lui.
fn dossier_joueur() -> Option<std::path::PathBuf> {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(std::path::Path::new(&base).join(".web3").join("aegis"))
}

/// Le dossier du binaire — celui du PAQUET quand le jeu est installé. ⚠ Il est **remplacé à chaque
/// mise à jour** : ce qu'on y écrit disparaît sans prévenir. On y LIT, on n'y écrit jamais.
fn dossier_binaire() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

/// Les endroits où une carte peut vivre, dans l'ordre où on les essaie.
///
/// ⭐ LA DISTINCTION QUI SUPPRIME LE PROBLÈME (12 août 2026). Il n'existe pas « deux copies de la
/// carte à synchroniser » — il existe **deux choses différentes** qu'on avait confondues :
///
/// - **la carte LIVRÉE avec le jeu** (dossier du binaire, ou dépôt en développement) : un asset,
///   versionné, identique chez tout le monde, remplacé à chaque mise à jour ;
/// - **la carte du JOUEUR** (`~/.web3/aegis/`) : son travail à lui, que rien ne doit jamais écraser.
///
/// D'où l'ordre de lecture : la sienne d'abord — s'il a édité, c'est ça qu'il veut revoir — puis le
/// dossier courant (c'est là que vit la carte de l'auteur quand il lance par `./run.sh` depuis la
/// racine du projet), puis le dossier du binaire (le seul repère stable quand le jeu est lancé PAR
/// QUELQU'UN D'AUTRE : le launcher place son répertoire courant dans le dossier du paquet, et sur
/// les machines d'une classe personne n'aura jamais le « bon » dossier courant).
fn chemins_carte() -> Vec<std::path::PathBuf> {
    let mut candidats = Vec::new();
    // Porte de secours explicite, pour les bancs et les tests : jamais deviner quand on peut dire.
    if let Ok(p) = std::env::var("AEGIS_MAP") {
        candidats.push(std::path::PathBuf::from(p));
    }
    if let Some(d) = dossier_joueur() {
        candidats.push(d.join("custom_map.lvl"));
    }
    candidats.push(std::path::PathBuf::from("custom_map.lvl"));
    if let Some(d) = dossier_binaire() {
        candidats.push(d.join("custom_map.lvl"));
    }
    candidats
}

/// Où écrire, sachant d'où l'on a lu. **On réécrit là où on a lu — sauf si c'est le dossier du
/// binaire**, qu'une mise à jour remplace : l'édition irait alors à la poubelle au prochain
/// déploiement, silencieusement. Dans ce seul cas, elle bascule vers le dossier du joueur.
///
/// Conséquence voulue, et c'est tout l'intérêt : en développement (lancement depuis le dépôt) la
/// carte de l'auteur reste dans le dépôt, donc **versionnée par git** — le filet qui l'a sauvée le
/// 12 août. Chez un joueur, l'édition atterrit chez lui et survit aux mises à jour. Personne ne
/// synchronise rien, parce qu'il n'y a rien à synchroniser.
///
/// La comparaison elle-même est isolée dans `loge_dans`, une fonction PURE : elle ne lit ni le
/// disque ni l'environnement, donc elle se teste exhaustivement sans qu'aucun test n'ait à toucher
/// à un état global (changer le dossier courant dans un test le rend instable pour tous les autres).
fn cible_ecriture(lu: &std::path::Path) -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let dans_le_paquet = dossier_binaire()
        .map(|d| loge_dans(lu, &d, &cwd))
        .unwrap_or(false);
    if !dans_le_paquet {
        return lu.to_path_buf();
    }
    match dossier_joueur() {
        Some(d) => d.join("custom_map.lvl"),
        None => lu.to_path_buf(), // pas de dossier utilisateur : mieux vaut écrire là que nulle part
    }
}

#[derive(Clone)]
pub struct TileGrid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<TileType>,
    pub start_pos: Vec2,
    pub finish_pos: Vec2,
    /// D'où vient cette carte. Détermine où `enregistrer` écrit — et s'il a le droit d'écrire.
    source: SourceCarte,
}

impl TileGrid {
    /// Une grille VIDE aux dimensions demandées — aucun disque, aucune carte, rien d'implicite.
    ///
    /// Elle est séparée de `new` depuis le 12 août 2026, parce que `new` fait DEUX choses : bâtir
    /// une grille *et* aller chercher une carte. Tant qu'aucune carte n'existait, la différence ne
    /// se voyait pas ; le jour où la carte livrée a été embarquée, `new(32, 18)` s'est mis à rendre
    /// une grille 58×28 — et deux tests l'ont dit tout de suite. Ils avaient raison : un
    /// constructeur qui ignore ses propres paramètres est un piège. Les tests qui veulent une
    /// grille nue passent donc par ici.
    pub fn vide(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![TileType::Air; width * height],
            start_pos: Vec2::new(3.5, 1.0),
            // `saturating_sub` et pas `-` : sur une grille de moins de 4 colonnes, la soustraction
            // déborde. En debug elle panique, mais en RELEASE elle repartait par le haut et posait
            // l'arrivée à des milliards de cases — silencieusement. Trouvé le 12 août 2026 par un
            // test qui cherchait tout autre chose.
            finish_pos: Vec2::new(width.saturating_sub(4) as f32 + 0.5, 2.0),
            source: SourceCarte::Neuve(std::path::PathBuf::from("custom_map.lvl")),
        }
    }

    /// La grille DU JEU : une grille vide, puis la meilleure carte disponible — celle du joueur si
    /// elle existe, sinon celle livrée avec le jeu. Les dimensions passées ne sont donc qu'un
    /// repli : toute carte chargée impose les siennes.
    pub fn new(width: usize, height: usize) -> Self {
        let mut grid = Self::vide(width, height);

        let candidats = chemins_carte();
        // On distingue TROIS issues, et on les journalise : chargée / aucune carte / carte présente
        // mais illisible. Un `if load().is_err() { defaut() }` les confondait toutes les trois —
        // et c'est la troisième, la seule dangereuse, qui disparaissait dans le silence.
        let mut illisible: Option<std::path::PathBuf> = None;
        for chemin in &candidats {
            if !chemin.exists() {
                continue;
            }
            match grid.load_from_file(chemin) {
                Ok(()) => {
                    let cible = cible_ecriture(chemin);
                    if cible != *chemin {
                        log::info!(
                            "Carte chargée depuis {} (livrée avec le jeu) — tes modifications iront \
                             dans {}, pour qu'une mise à jour ne les efface pas.",
                            chemin.display(),
                            cible.display()
                        );
                    } else {
                        log::info!("Carte chargée depuis {}", chemin.display());
                    }
                    grid.source = SourceCarte::Fichier(cible);
                    return grid;
                }
                Err(e) => {
                    log::error!(
                        "Carte PRÉSENTE mais illisible : {} ({e}). L'éditeur refusera d'écrire ici \
                         pour ne pas la détruire.",
                        chemin.display()
                    );
                    illisible.get_or_insert(chemin.clone());
                }
            }
        }

        // Aucune carte sur le disque → LA CARTE LIVRÉE, embarquée dans le binaire. Le terrain nu ne
        // reste que le filet du filet : si même la carte embarquée est illisible, le jeu démarre
        // quand même plutôt que de refuser de se lancer.
        if grid.load_carte_livree().is_err() {
            log::error!("La carte livrée (embarquée) est illisible — terrain nu.");
            grid.load_default_stage();
        }
        grid.source = match illisible {
            Some(p) => SourceCarte::Illisible(p),
            None => {
                // Aucune carte nulle part : l'éditeur en créera une, et il la créera CHEZ LE JOUEUR
                // (via `cible_ecriture`), jamais dans le paquet qu'une mise à jour remplacera.
                let defaut = candidats.last().cloned().unwrap_or_else(|| "custom_map.lvl".into());
                let cible = cible_ecriture(&defaut);
                log::info!("Aucune carte trouvée — terrain par défaut. L'éditeur créera {}", cible.display());
                SourceCarte::Neuve(cible)
            }
        };
        grid
    }

    /// Enregistre la carte LÀ D'OÙ ELLE VIENT, de façon atomique, et dit ce qui s'est passé.
    ///
    /// Trois raisons à cette fonction, chacune correspondant à un défaut réel :
    /// 1. **Le chemin n'est plus deviné** : on réécrit le fichier qu'on a ouvert, jamais un
    ///    `"custom_map.lvl"` relatif au dossier d'où le jeu a été lancé.
    /// 2. **On n'écrase jamais une carte qu'on n'a pas su lire** (`Illisible`).
    /// 3. **L'écriture est atomique** : `File::create` TRONQUE le fichier avant d'écrire, donc une
    ///    interruption au mauvais moment (disque plein, coupure, crash) laissait une carte à moitié
    ///    écrite — c'est-à-dire perdue. On écrit à côté, puis on renomme : le renommage sur un même
    ///    système de fichiers est indivisible, la carte est donc soit l'ancienne, soit la nouvelle.
    pub fn enregistrer(&self) {
        let cible = match &self.source {
            SourceCarte::Fichier(p) | SourceCarte::Neuve(p) => p.clone(),
            SourceCarte::Illisible(p) => {
                log::error!(
                    "ENREGISTREMENT REFUSÉ : {} existe mais n'a pas pu être lu au démarrage. \
                     Écrire ici remplacerait ta carte par le terrain par défaut.",
                    p.display()
                );
                return;
            }
        };
        // Le dossier du joueur peut ne pas exister encore : c'est le cas normal du premier
        // enregistrement, pas une panne. On le crée, et si on n'y arrive pas on le DIT.
        if let Some(parent) = cible.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    log::error!("Carte NON enregistrée — dossier {} impossible à créer : {e}", parent.display());
                    return;
                }
            }
        }
        let temporaire = cible.with_extension("lvl.tmp");
        if let Err(e) = self.save_to_file(&temporaire) {
            log::error!("Carte NON enregistrée ({}) : {e}", temporaire.display());
            return;
        }
        match std::fs::rename(&temporaire, &cible) {
            Ok(()) => log::info!("Carte enregistrée : {}", cible.display()),
            Err(e) => {
                log::error!("Carte NON enregistrée — le renommage a échoué ({}) : {e}", cible.display());
                let _ = std::fs::remove_file(&temporaire);
            }
        }
    }

    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        writeln!(f, "{} {} {} {} {} {}", self.width, self.height, self.start_pos.x, self.start_pos.y, self.finish_pos.x, self.finish_pos.y)?;
        for tile in &self.tiles {
            write!(f, "{} ", tile.to_u8())?;
        }
        writeln!(f)?;
        log::info!("Carte enregistrée avec succès sur le disque !");
        Ok(())
    }

    pub fn load_from_file(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::open(path)?;
        self.load_from_reader(std::io::BufReader::new(file))
    }

    /// La carte LIVRÉE avec le jeu, embarquée dans le binaire (3 Ko).
    ///
    /// Elle est ici pour la même raison que les modèles 3D : un joueur qui télécharge le jeu doit
    /// recevoir LE terrain, pas un sol vide. Tant qu'elle vivait en fichier à côté de l'exécutable,
    /// publier le binaire seul aurait livré un jeu sans sa carte — et personne ne l'aurait vu avant
    /// que quelqu'un d'autre ne lance le jeu. Le fichier reste dans le dépôt : c'est lui que
    /// l'éditeur modifie, et c'est lui qui est embarqué à la compilation suivante.
    const CARTE_LIVREE: &'static [u8] = include_bytes!("../../../custom_map.lvl");

    /// Charge la carte embarquée. Ne touche AUCUN fichier : c'est le dernier recours quand aucune
    /// carte n'existe sur le disque, avant le terrain nu.
    pub fn load_carte_livree(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.load_from_reader(std::io::BufReader::new(Self::CARTE_LIVREE))
    }

    /// Le PARSEUR, séparé de sa source — c'est ce qui permet de lire indifféremment un fichier du
    /// disque ou les octets embarqués, sans deux copies du format qui divergeraient un jour.
    fn load_from_reader(&mut self, mut reader: impl std::io::BufRead) -> Result<(), Box<dyn std::error::Error>> {
        let mut line1 = String::new();
        reader.read_line(&mut line1)?;
        let parts: Vec<&str> = line1.split_whitespace().collect();
        if parts.len() >= 6 {
            self.width = parts[0].parse()?;
            self.height = parts[1].parse()?;
            self.start_pos.x = parts[2].parse()?;
            self.start_pos.y = parts[3].parse()?;
            self.finish_pos.x = parts[4].parse()?;
            self.finish_pos.y = parts[5].parse()?;
        }

        let mut line2 = String::new();
        reader.read_line(&mut line2)?;
        let tiles_str: Vec<&str> = line2.split_whitespace().collect();
        self.tiles = vec![TileType::Air; self.width * self.height];
        for (idx, val_str) in tiles_str.iter().enumerate() {
            if idx < self.tiles.len() {
                if let Ok(val) = val_str.parse::<u8>() {
                    self.tiles[idx] = TileType::from_u8(val);
                }
            }
        }
        log::info!("Carte chargée avec succès depuis le disque !");
        Ok(())
    }

    pub fn ensure_capacity(&mut self, required_w: usize, required_h: usize) {
        if required_w <= self.width && required_h <= self.height {
            return;
        }
        let new_w = self.width.max(required_w);
        let new_h = self.height.max(required_h);
        let mut new_tiles = vec![TileType::Air; new_w * new_h];

        for y in 0..self.height {
            for x in 0..self.width {
                new_tiles[y * new_w + x] = self.tiles[y * self.width + x];
            }
        }

        self.width = new_w;
        self.height = new_h;
        self.tiles = new_tiles;
    }

    pub fn set_tile(&mut self, x: usize, y: usize, tile: TileType) {
        self.ensure_capacity(x + 1, y + 1);
        self.tiles[y * self.width + x] = tile;
        if tile == TileType::StartPoint {
            self.start_pos = Vec2::new(x as f32 + 0.5, y as f32 + 1.0);
        } else if tile == TileType::FinishFlag {
            self.finish_pos = Vec2::new(x as f32 + 0.5, y as f32 + 1.0);
        }
    }

    pub fn get_tile(&self, x: i32, y: i32) -> TileType {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return TileType::Air; // Air en dehors des limites pour permettre la chute dans le vide !
        }
        self.tiles[y as usize * self.width + x as usize]
    }

    pub fn load_default_stage(&mut self) {
        for t in self.tiles.iter_mut() {
            *t = TileType::Air;
        }

        // Sol de base propre en bas de la carte (row y=0)
        for x in 0..self.width {
            self.tiles[0 * self.width + x] = TileType::GrassBlock;
        }

        self.start_pos = Vec2::new(3.5, 1.0);
        self.finish_pos = Vec2::new((self.width - 4) as f32 + 0.5, 2.0);
    }

    pub fn get_lowest_block_y(&self) -> f32 {
        let mut min_y = f32::MAX;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.tiles[y * self.width + x] != TileType::Air {
                    min_y = min_y.min(y as f32);
                }
            }
        }
        if min_y == f32::MAX {
            0.0
        } else {
            min_y
        }
    }

    pub fn get_void_kill_y(&self) -> f32 {
        // Toujours exactement 5 blocs en dessous du bloc le plus bas de la carte
        self.get_lowest_block_y() - 5.0
    }

    pub fn check_solid_collision(&self, pos: Vec2, size: Vec2) -> bool {
        let half_w = size.x / 2.0;
        let left = pos.x - half_w;
        let right = pos.x + half_w;
        let bottom = pos.y;
        let top = pos.y + size.y;

        // Grille de blocs uniquement (pas de barrières invisibles autour de la map!)
        let min_x = left.floor() as i32;
        let max_x = right.floor() as i32;
        let min_y = bottom.floor() as i32;
        let max_y = top.floor() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.get_tile(x, y).is_solid() {
                    return true;
                }
            }
        }
        false
    }

    pub fn check_hazard_collision(&self, pos: Vec2, size: Vec2) -> bool {
        // Mort immédiate si le joueur tombe au niveau de la ligne du vide (5 blocs sous le bloc le plus bas)
        if pos.y <= self.get_void_kill_y() {
            return true;
        }

        let half_w = size.x / 2.0;
        let min_x = (pos.x - half_w).floor() as i32;
        let max_x = (pos.x + half_w).floor() as i32;
        let min_y = pos.y.floor() as i32;
        let max_y = (pos.y + size.y).floor() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.get_tile(x, y).is_hazard() {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier de travail unique par test : ces tests touchent au DISQUE, et deux d'entre eux
    /// qui partageraient un fichier échoueraient l'un l'autre au hasard. Un test instable est pire
    /// qu'un test absent — il fait douter des vrais échecs.
    fn dossier_temporaire(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("aegis-test-{nom}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// LA RÈGLE CENTRALE : une carte lue dans le dossier du BINAIRE (donc livrée avec le jeu, et
    /// remplacée à chaque mise à jour) doit voir ses modifications partir CHEZ LE JOUEUR. Sinon
    /// l'édition disparaît au prochain déploiement, sans un mot.
    #[test]
    fn editer_la_carte_livree_ecrit_chez_le_joueur_pas_dans_le_paquet() {
        let dans_le_paquet = dossier_binaire().unwrap().join("custom_map.lvl");
        let cible = cible_ecriture(&dans_le_paquet);
        assert_ne!(cible, dans_le_paquet, "l'édition serait écrasée par la prochaine mise à jour");
        assert_eq!(cible, dossier_joueur().unwrap().join("custom_map.lvl"));
    }

    /// ⚠ LE CAS QUI ÉCHAPPAIT À LA RÈGLE, et c'est le seul qui se produit en vrai : le régisseur
    /// lance le jeu avec le répertoire courant DÉJÀ dans le paquet, donc le chemin lu est le simple
    /// relatif « custom_map.lvl ». Table exhaustive, sur une fonction pure — aucun état global
    /// touché, donc ce test ne peut faire échouer aucun autre.
    #[test]
    fn la_regle_reconnait_le_paquet_en_relatif_comme_en_absolu() {
        let p = std::path::Path::new;
        let paquet = p("/opt/jeu/Linux");
        let cas: &[(&str, &str, bool)] = &[
            // (chemin lu, dossier courant, doit-on considérer qu'il est DANS le paquet ?)
            ("custom_map.lvl", "/opt/jeu/Linux", true),   // le cas réel du régisseur
            ("/opt/jeu/Linux/custom_map.lvl", "/", true), // le même, en absolu
            ("custom_map.lvl", "/home/moi/depot", false), // l'auteur dans son dépôt
            ("/home/moi/depot/custom_map.lvl", "/", false),
            ("sous/custom_map.lvl", "/opt/jeu", false),   // un cran plus bas : pas « dans » le paquet
            ("/opt/jeu/Linux/sous/custom_map.lvl", "/", false),
        ];
        for (lu, cwd, attendu) in cas {
            assert_eq!(
                loge_dans(p(lu), paquet, p(cwd)),
                *attendu,
                "lu={lu} cwd={cwd}"
            );
        }
    }

    /// Le témoin de l'autre côté : partout AILLEURS que dans le paquet, on réécrit là où on a lu.
    /// C'est ce qui garde la carte de l'auteur dans son dépôt, donc versionnée par git — le filet
    /// qui l'a sauvée. Sans ce test, la règle ci-dessus pourrait tout rediriger, y compris ça.
    #[test]
    fn temoin_ailleurs_on_reecrit_la_ou_on_a_lu() {
        let dans_le_depot = std::path::PathBuf::from("/un/depot/a/moi/custom_map.lvl");
        assert_eq!(cible_ecriture(&dans_le_depot), dans_le_depot);
    }

    /// ⛔ LE TEST QUI COMPTE : une carte présente mais ILLISIBLE ne doit jamais être écrasée.
    /// C'est le scénario qui aurait coûté la carte de l'auteur — démarrer sur le terrain par défaut
    /// alors que la vraie carte existe, puis poser un bloc.
    #[test]
    fn une_carte_illisible_nest_jamais_ecrasee() {
        let d = dossier_temporaire("illisible");
        let carte = d.join("custom_map.lvl");
        let contenu_precieux = b"CECI EST LA CARTE DE L'AUTEUR, ILLISIBLE MAIS PRECIEUSE";
        std::fs::write(&carte, contenu_precieux).unwrap();

        let mut grille = TileGrid::vide(32, 18);
        grille.load_default_stage();
        grille.source = SourceCarte::Illisible(carte.clone());
        grille.set_tile(5, 5, TileType::GrassBlock); // le geste qui déclenchait l'enregistrement
        grille.enregistrer();

        assert_eq!(
            std::fs::read(&carte).unwrap(),
            contenu_precieux,
            "le fichier a été écrasé alors qu'il était marqué illisible"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// La preuve que la garde ci-dessus MORD : avec la même grille marquée `Fichier` au lieu
    /// d'`Illisible`, l'écriture DOIT avoir lieu. Sans ce témoin, le test précédent passerait tout
    /// aussi bien si `enregistrer` n'écrivait jamais rien.
    #[test]
    fn temoin_positif_une_carte_lisible_est_bien_reecrite() {
        let d = dossier_temporaire("lisible");
        let carte = d.join("custom_map.lvl");
        std::fs::write(&carte, b"ancien contenu").unwrap();

        let mut grille = TileGrid::vide(32, 18);
        grille.load_default_stage();
        grille.source = SourceCarte::Fichier(carte.clone());
        grille.set_tile(5, 5, TileType::GrassBlock);
        grille.enregistrer();

        let ecrit = std::fs::read_to_string(&carte).unwrap();
        assert_ne!(ecrit, "ancien contenu", "la grille aurait dû être réécrite");
        assert!(ecrit.starts_with("32 18 "), "en-tête de carte attendu, trouvé : {ecrit:.20}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// L'enregistrement passe par un fichier temporaire renommé : après coup, aucun `.tmp` ne doit
    /// traîner. Un `.lvl.tmp` oublié à côté d'une carte, c'est le prochain qui se demandera lequel
    /// des deux est le bon.
    #[test]
    fn lenregistrement_ne_laisse_aucun_fichier_temporaire() {
        let d = dossier_temporaire("atomique");
        let carte = d.join("custom_map.lvl");

        let mut grille = TileGrid::vide(32, 18);
        grille.load_default_stage();
        grille.source = SourceCarte::Neuve(carte.clone());
        grille.enregistrer();

        assert!(carte.exists(), "la carte neuve aurait dû être créée");
        let restes: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("tmp"))
            .collect();
        assert!(restes.is_empty(), "fichiers temporaires laissés derrière : {restes:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ⛔ LA CARTE EST-ELLE VRAIMENT DANS LE BINAIRE ?
    ///
    /// Le 12 août 2026, `custom_map.lvl` vivait en fichier À CÔTÉ de l'exécutable. Publier le
    /// binaire seul aurait donc livré le jeu SANS sa carte — un sol vide — et personne ne l'aurait
    /// vu avant qu'un camarade ne lance le jeu chez lui. Ce test regarde les octets embarqués :
    /// il tombe si quelqu'un retire le `include_bytes!`, ou si la carte livrée devient illisible.
    #[test]
    fn la_carte_livree_voyage_dans_le_binaire_pas_a_cote() {
        let mut grille = TileGrid::vide(1, 1);
        grille.load_carte_livree().expect("la carte livrée doit être lisible");

        assert!(grille.width > 8 && grille.height > 8, "carte livrée trop petite : {}x{}", grille.width, grille.height);
        // GARDE ANTI-TEST-CREUX : une carte de la bonne taille mais TOUTE VIDE passerait le test
        // ci-dessus en livrant un terrain nu — exactement le défaut qu'on cherche à empêcher.
        let posees = grille.tiles.iter().filter(|t| **t != TileType::Air).count();
        assert!(posees > 20, "la carte livrée ne contient que {posees} tuiles : c'est un terrain vide");
    }

    #[test]
    fn test_grid_initialization() {
        let grid = TileGrid::vide(32, 18);
        assert_eq!(grid.width, 32);
        assert_eq!(grid.height, 18);
    }

    #[test]
    fn test_map_saving_and_loading() {
        let mut grid = TileGrid::new(32, 18);
        grid.set_tile(5, 5, TileType::GrassBlock);
        grid.set_tile(6, 5, TileType::SolidBlock);
        grid.set_tile(7, 5, TileType::MetalBlock);
        grid.set_tile(8, 5, TileType::StartPoint);
        grid.set_tile(9, 5, TileType::FinishFlag);

        let test_path = "test_custom_map.lvl";
        assert!(grid.save_to_file(test_path).is_ok());

        let mut loaded_grid = TileGrid::new(32, 18);
        assert!(loaded_grid.load_from_file(test_path).is_ok());

        assert_eq!(loaded_grid.get_tile(5, 5), TileType::GrassBlock);
        assert_eq!(loaded_grid.get_tile(6, 5), TileType::SolidBlock);
        assert_eq!(loaded_grid.get_tile(7, 5), TileType::MetalBlock);
        assert_eq!(loaded_grid.get_tile(8, 5), TileType::StartPoint);
        assert_eq!(loaded_grid.get_tile(9, 5), TileType::FinishFlag);

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_dynamic_void_kill_plane() {
        let mut grid = TileGrid::new(32, 18);
        for t in grid.tiles.iter_mut() { *t = TileType::Air; }

        grid.set_tile(10, 4, TileType::GrassBlock);
        assert_eq!(grid.get_lowest_block_y(), 4.0);
        assert_eq!(grid.get_void_kill_y(), -1.0); // 4 - 5 = -1.0

        grid.set_tile(12, 2, TileType::SolidBlock);
        assert_eq!(grid.get_lowest_block_y(), 2.0);
        assert_eq!(grid.get_void_kill_y(), -3.0); // 2 - 5 = -3.0

        // Mort instantanée si le joueur tombe au niveau de la ligne noire du vide
        assert!(grid.check_hazard_collision(Vec2::new(12.0, -3.5), Vec2::new(0.6, 1.2)));
        assert!(!grid.check_hazard_collision(Vec2::new(12.0, 3.0), Vec2::new(0.6, 1.2)));
    }
}
