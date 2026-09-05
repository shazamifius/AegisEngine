use std::fs::File;
use std::io::Read;
use std::path::Path;
use crate::core::math::{Mat4, Vec2, Vec3, Vec4};
use crate::geometry::vertex::Vertex;

/// Un morceau de scène : la trace d'une primitive dans le maillage fusionné.
///
/// Elle permet de dessiner un objet seul — ou de le compter — **sans hiérarchie de scène**.
/// *`scene/_scene_graph.rs` dort pour une raison : rien n'en a encore besoin, et une brique
/// rallumée sans être exercée recrée exactement le défaut qu'on vient de corriger.*
#[derive(Debug, Clone)]
pub struct Partie {
    /// Le nom du nœud dans le fichier — celui que l'auteur voit dans Blender.
    pub nom: String,
    pub premier_indice: u32,
    pub nombre_indices: u32,
    /// La plage de sommets de cette partie dans le tampon fusionné. ⚠ Elle est nécessaire pour
    /// raisonner sur une primitive SEULE : deux objets distincts qui se touchent ne sont pas la
    /// même surface, et les confondre inventerait une adjacence qui n'existe pas.
    pub premier_sommet: u32,
    pub nombre_sommets: u32,
}

/// Une scène glTF entière, ses objets replacés puis fusionnés en un seul maillage.
///
/// Un seul tampon de sommets et un seul d'indices : c'est ce que le pipeline sait dessiner
/// aujourd'hui, et `parties` garde de quoi les séparer le jour où il saura faire mieux.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub sommets: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Une entrée par primitive lue, dans l'ordre de la fusion.
    pub parties: Vec<Partie>,
}

pub struct GlbLoader;

impl GlbLoader {
    /// Charge un fichier .glb (glTF Binary) sans modifier son échelle ni son pivot d'origine Blender.
    pub fn load_glb_raw(path: impl AsRef<Path>) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        Self::load_glb_internal(path, false)
    }

    /// Charge un fichier .glb (glTF Binary) et normalise sa taille à 1.5 unités.
    pub fn load_glb(path: impl AsRef<Path>) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        Self::load_glb_internal(path, true)
    }

    /// Charge un modèle depuis des octets DÉJÀ EN MÉMOIRE — typiquement `include_bytes!`, donc un
    /// modèle embarqué dans le binaire.
    ///
    /// ⚠ C'EST LA VOIE NORMALE DEPUIS LE 12 AOÛT 2026, et voici pourquoi. Les treize modèles du jeu
    /// étaient chargés depuis `/home/shaza/Documents/asset/…` — un chemin absolu vers UNE machine.
    /// Sur n'importe quel autre ordinateur, aucun décor, aucun piège, aucune tourelle : rien ne se
    /// serait affiché, et rien ne l'aurait dit. Ils n'étaient même pas dans le dépôt, donc ni
    /// distribuables ni sauvegardés.
    ///
    /// Embarquer coûte 316 Ko dans un binaire de 4 Mo, et supprime le problème au lieu de le
    /// déplacer : plus de chemin à résoudre, plus de dossier à installer à côté, plus de « fichier
    /// manquant » possible. Un seul fichier à distribuer — la même règle que la police et les
    /// données pays/villes du launcher web3.
    pub fn load_glb_bytes(bytes: &[u8]) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        Self::parse_glb(bytes.to_vec(), true)
    }

    /// Comme `load_glb_bytes`, mais SANS normaliser la taille : pour les modèles dont les dimensions
    /// réelles comptent (le décor, qui doit s'accorder à la grille du jeu).
    pub fn load_glb_raw_bytes(bytes: &[u8]) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        Self::parse_glb(bytes.to_vec(), false)
    }

    fn load_glb_internal(path: impl AsRef<Path>, normalize: bool) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Self::parse_glb(buffer, normalize)
    }

    fn parse_glb(buffer: Vec<u8>, normalize: bool) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        if buffer.len() < 20 {
            return Err("Fichier GLB trop court.".into());
        }

        // 1. Header (12 octets)
        let magic = &buffer[0..4];
        if magic != b"glTF" {
            return Err("Format GLB invalide (magic != glTF).".into());
        }
        let _version = u32::from_le_bytes(buffer[4..8].try_into()?);
        let _length = u32::from_le_bytes(buffer[8..12].try_into()?);

        // 2. Chunk 0 (JSON)
        let json_len = u32::from_le_bytes(buffer[12..16].try_into()?) as usize;
        let json_type = u32::from_le_bytes(buffer[16..20].try_into()?);
        if json_type != 0x4E4F534A {
            return Err("Chunk 0 GLB invalide (type != JSON).".into());
        }

        let json_bytes = &buffer[20..20 + json_len];
        let json_str = std::str::from_utf8(json_bytes)?;

        // 3. Chunk 1 (BIN)
        let bin_offset = 20 + json_len;
        if buffer.len() < bin_offset + 8 {
            return Err("Pas de chunk BIN dans le fichier GLB.".into());
        }

        let bin_len = u32::from_le_bytes(buffer[bin_offset..bin_offset + 4].try_into()?) as usize;
        let bin_type = u32::from_le_bytes(buffer[bin_offset + 4..bin_offset + 8].try_into()?);
        if bin_type != 0x004E4942 && bin_type != 0x0414E4942 {
            // Some exporters use 0x004E4942 (BIN\0)
        }

        let bin_data = &buffer[bin_offset + 8..bin_offset + 8 + bin_len];

        log::info!("GLB Chargé avec succès : JSON len={}, BIN len={}", json_len, bin_len);

        // Parse JSON accessor & bufferView offsets dynamically
        let (v_out, i_out) = Self::parse_glb_json(json_str, bin_data, normalize)?;
        Ok((v_out, i_out))
    }

    /// Lit un accesseur glTF de flottants et rend ses composantes, sommet par sommet.
    ///
    /// ## ⚠⚠ POURQUOI CETTE FONCTION EXISTE, ET CE QU'ELLE A CORRIGÉ
    ///
    /// Ce chargeur ne lisait **que les positions**. Toutes les autres composantes du sommet étaient
    /// **inventées** — voir `normale_de_repli` ci-dessous. Le pipeline, lui, les attend depuis
    /// toujours (`Vertex` a ses champs `normal`, `tangent`, `uv0`), et le shader d'éclairage lit
    /// `in.normal` pour en tirer `N`, dont dépendent **Lambert, GGX et les ombres**.
    /// *Autrement dit : tout modèle importé était éclairé avec des normales fausses, et rien ne le
    /// disait. Les dix `.glb` du dépôt portent tous un attribut `NORMAL` — il n'a jamais été lu.*
    ///
    /// **Deux pièges du format que l'ancienne lecture ignorait, et qui donnent des données fausses
    /// sans jamais échouer :**
    /// 1. **Le décalage existe à DEUX niveaux** — sur la `bufferView` *et* sur l'`accessor`. N'en
    ///    lire qu'un marche tant qu'un accesseur a sa vue à lui, et se met à lire les octets du
    ///    voisin dès que deux accesseurs partagent une vue. Blender les partage.
    /// 2. **`byteStride`** — une vue peut entrelacer plusieurs attributs. Sans lui, on lit du serré
    ///    et on obtient des valeurs plausibles et fausses.
    ///
    /// Elle rend `None` si l'attribut est absent : c'est au demandeur de décider quoi en faire, et
    /// de le **dire**.
    fn lire_flottants(
        accessors: &[serde_json::Value],
        buffer_views: &[serde_json::Value],
        bin: &[u8],
        indice_accesseur: usize,
        composantes: usize,
    ) -> Option<Vec<f32>> {
        let acc = accessors.get(indice_accesseur)?;
        // 5126 = FLOAT. Les autres types (entiers normalisés) existent dans le format mais aucun
        // de nos modèles ne les emploie : mieux vaut refuser que lire des octets au hasard.
        if acc["componentType"].as_u64()? != 5126 {
            log::warn!(
                "glTF : accesseur {} n'est pas en FLOAT (type {}) — attribut ignoré plutôt que mal lu.",
                indice_accesseur,
                acc["componentType"].as_u64().unwrap_or(0)
            );
            return None;
        }
        let nombre = acc["count"].as_u64()? as usize;
        let vue = &buffer_views[acc["bufferView"].as_u64()? as usize];

        let debut = vue["byteOffset"].as_u64().unwrap_or(0) as usize
            + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
        // Sans `byteStride`, les données sont serrées : le pas vaut la taille d'un élément.
        let pas = vue["byteStride"].as_u64().unwrap_or(0) as usize;
        let pas = if pas == 0 { composantes * 4 } else { pas };

        // Garde de bornes : un fichier tronqué ou mal décrit doit être refusé, jamais lu au-delà.
        if debut + (nombre - 1) * pas + composantes * 4 > bin.len() {
            log::warn!("glTF : accesseur {indice_accesseur} déborde du tampon — attribut ignoré.");
            return None;
        }

        let mut sortie = Vec::with_capacity(nombre * composantes);
        for i in 0..nombre {
            let base = debut + i * pas;
            for c in 0..composantes {
                let o = base + c * 4;
                sortie.push(f32::from_le_bytes([bin[o], bin[o + 1], bin[o + 2], bin[o + 3]]));
            }
        }
        Some(sortie)
    }

    /// La normale que ce chargeur fabriquait pour TOUS les sommets, faute de lire l'attribut.
    ///
    /// Elle n'est exacte que sur **une sphère centrée à l'origine** : partout ailleurs elle pointe
    /// « loin du centre de l'objet » au lieu de « perpendiculairement à la surface ». Sur un cube,
    /// les six faces reçoivent des normales en éventail ; sur un objet décentré, elles pointent
    /// toutes du même côté.
    ///
    /// **Elle reste ici comme dernier recours**, pour un fichier réellement dépourvu de `NORMAL` —
    /// mais son emploi est désormais **journalisé**, parce que le vrai défaut n'était pas la
    /// formule : c'était qu'elle s'appliquait en silence.
    fn normale_de_repli(position: Vec3) -> Vec3 {
        position.normalize_or_zero()
    }

    fn parse_glb_json(json_str: &str, bin_data: &[u8], normalize: bool) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        let parsed: serde_json::Value = serde_json::from_str(json_str)?;
        let accessors = parsed["accessors"].as_array().ok_or("Pas d'accessors dans le GLB")?;
        let buffer_views = parsed["bufferViews"].as_array().ok_or("Pas de bufferViews dans le GLB")?;

        let mut pos_accessor_idx = None;
        let mut ind_accessor_idx = None;
        let (mut nor_idx, mut tan_idx, mut uv0_idx, mut uv1_idx) = (None, None, None, None);

        if let Some(meshes) = parsed["meshes"].as_array() {
            if let Some(primitives) = meshes[0]["primitives"].as_array() {
                // ⚠ DETTE CONNUE, PAS TRAITÉE ICI : on ne lit que `meshes[0]`, `primitives[0]`.
                // Un fichier exporté depuis Blender avec plusieurs objets, ou un objet portant
                // plusieurs matériaux, perd donc tout sauf son premier morceau — **sans erreur**.
                // C'est le prochain défaut de ce fichier, et il se voit dès qu'on charge une VRAIE
                // scène plutôt qu'un objet unique.
                let attributs = &primitives[0]["attributes"];
                pos_accessor_idx = attributs["POSITION"].as_u64().map(|v| v as usize);
                nor_idx = attributs["NORMAL"].as_u64().map(|v| v as usize);
                tan_idx = attributs["TANGENT"].as_u64().map(|v| v as usize);
                uv0_idx = attributs["TEXCOORD_0"].as_u64().map(|v| v as usize);
                uv1_idx = attributs["TEXCOORD_1"].as_u64().map(|v| v as usize);
                ind_accessor_idx = primitives[0]["indices"].as_u64().map(|v| v as usize);

                // Le mode de primitive : 4 = TRIANGLES, et c'est le seul que ce moteur dessine.
                // Un ruban ou un éventail se chargerait ici sans bruit et s'afficherait en désordre.
                let mode = primitives[0]["mode"].as_u64().unwrap_or(4);
                if mode != 4 {
                    return Err(format!(
                        "glTF : mode de primitive {mode} non géré (seul TRIANGLES = 4 l'est)."
                    )
                    .into());
                }
            }
        }

        let pos_acc_idx = pos_accessor_idx.ok_or("Attribut POSITION non trouvé dans GLB")?;
        let positions = Self::lire_flottants(accessors, buffer_views, bin_data, pos_acc_idx, 3)
            .ok_or("Attribut POSITION illisible dans GLB")?;
        let pos_count = positions.len() / 3;

        let normales = nor_idx.and_then(|i| Self::lire_flottants(accessors, buffer_views, bin_data, i, 3));
        let tangentes = tan_idx.and_then(|i| Self::lire_flottants(accessors, buffer_views, bin_data, i, 4));
        let uv0s = uv0_idx.and_then(|i| Self::lire_flottants(accessors, buffer_views, bin_data, i, 2));
        let uv1s = uv1_idx.and_then(|i| Self::lire_flottants(accessors, buffer_views, bin_data, i, 2));

        if normales.is_none() {
            // Ce message est la moitié qui manquait : un repli silencieux est un mécanisme mort.
            log::warn!(
                "glTF : aucune normale dans ce modèle ({pos_count} sommets) — repli sur une normale \
                 DÉDUITE DE LA POSITION. Elle n'est exacte que sur une sphère centrée : l'éclairage \
                 de cet objet sera faux."
            );
        }

        let mut vertices = Vec::with_capacity(pos_count);
        for i in 0..pos_count {
            let pos = Vec3::new(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);

            let normal = match &normales {
                Some(n) => Vec3::new(n[i * 3], n[i * 3 + 1], n[i * 3 + 2]),
                None => Self::normale_de_repli(pos),
            };
            let tangent = match &tangentes {
                Some(t) => Vec4::new(t[i * 4], t[i * 4 + 1], t[i * 4 + 2], t[i * 4 + 3]),
                // Sans tangente, pas de repère pour une carte de normales — mais la valeur doit
                // rester unitaire, sans quoi un shader qui l'emploierait produirait des NaN.
                None => Vec4::new(1.0, 0.0, 0.0, 1.0),
            };
            let uv0 = match &uv0s {
                Some(u) => Vec2::new(u[i * 2], u[i * 2 + 1]),
                None => Vec2::ZERO,
            };
            let uv1 = match &uv1s {
                Some(u) => Vec2::new(u[i * 2], u[i * 2 + 1]),
                None => Vec2::ZERO,
            };

            vertices.push(Vertex::new(pos, normal, tangent, uv0, uv1));
        }

        if normalize {
            let mut min_p = Vec3::splat(f32::MAX);
            let mut max_p = Vec3::splat(f32::MIN);
            for v in &vertices {
                let p = Vec3::from(v.position);
                min_p = min_p.min(p);
                max_p = max_p.max(p);
            }

            let center = (min_p + max_p) * 0.5;
            let size = (max_p - min_p).max_element();
            let scale = if size > 0.001 { 1.5 / size } else { 1.0 };

            for v in &mut vertices {
                v.position[0] = (v.position[0] - center.x) * scale;
                v.position[1] = (v.position[1] - center.y) * scale;
                v.position[2] = (v.position[2] - center.z) * scale;
            }
        }

        // ── Les indices ────────────────────────────────────────────────────────────────────────
        // ⚠ Le décalage de l'ACCESSEUR est lu ici aussi : il manquait, comme pour les attributs.
        // Et le type 5121 (octet non signé) est désormais accepté — un modèle de moins de 256
        // sommets l'emploie légitimement, et il était jusqu'ici ignoré **en silence**, ce qui
        // produisait un maillage sans aucun indice, donc un objet invisible.
        // ⭐ Même lecture que la voie « scène », et c'est voulu : deux copies d'un décodeur
        // divergent, et c'est toujours celle qu'on ne relit plus qui se met à mentir.
        let mut indices = match ind_accessor_idx {
            Some(i) => Self::lire_indices(accessors, buffer_views, bin_data, i)?,
            None => Vec::new(),
        };

        if indices.is_empty() {
            for i in 0..vertices.len() as u32 {
                indices.push(i);
            }
        }

        log::info!("Mesh GLB chargé (normalize={}) : {} sommets, {} indices.", normalize, vertices.len(), indices.len());
        Ok((vertices, indices))
    }

    // ═════════════════════════════════════════════════════════════════════════════════════════
    // LA SCÈNE COMPLÈTE — la voie ouverte le 5 septembre 2026
    // ═════════════════════════════════════════════════════════════════════════════════════════

    /// Charge une **scène entière** : tous les maillages, toutes leurs primitives, chacune replacée
    /// par la transformation de son nœud.
    ///
    /// ## ⚠⚠ POURQUOI C'EST UNE FONCTION NOUVELLE ET NON UNE CORRECTION DE `load_glb`
    ///
    /// **Les dix modèles du jeu portent TOUS une transformation de nœud non triviale** — mesuré, pas
    /// supposé : translation et échelle sur huit d'entre eux, rotation sur `plantedecendente` et
    /// `spike_trap`. `load_glb*` les ignore depuis toujours, et le jeu a été bâti par-dessus cet
    /// oubli : ses objets sont placés par le jeu lui-même.
    ///
    /// **Donc appliquer les transformations dans `load_glb` aurait déplacé et retourné tout le
    /// décor existant, sans qu'aucun test ne tombe** — un défaut d'image, invisible à la
    /// compilation. C'est le patron déjà employé trois fois côté réseau : *on ajoute un chemin à
    /// côté de l'ancien, jamais à sa place.*
    ///
    /// ## Ce que cette voie fait de plus, et que l'ancienne ne fera jamais
    ///
    /// - **Toutes les primitives**, pas `meshes[0].primitives[0]`. Un fichier Blender à plusieurs
    ///   objets — ou un objet à plusieurs matériaux — arrivait jusqu'ici amputé **en silence**.
    /// - **La hiérarchie des nœuds**, composée : un objet parenté suit son parent.
    /// - **Les normales transformées correctement**, par la comatrice — et non par la matrice
    ///   elle-même, qui les fausserait dès qu'une échelle est non uniforme.
    /// - **⚠ L'orientation rétablie quand une échelle est négative.** Le nœud « Cube » du modèle de
    ///   test vaut −2,95 sur deux axes : c'est un miroir, il inverse le sens de parcours des
    ///   triangles. Sans compensation, l'élimination des faces arrière jetterait exactement les
    ///   faces qu'il faut garder — l'objet apparaîtrait retourné, ou creux.
    ///
    /// ⚠ **Elle ne normalise jamais la taille.** Une scène a ses dimensions ; les changer n'aurait
    /// aucun sens pour un ensemble d'objets, et cacherait les échelles relatives qu'on veut voir.
    pub fn charger_scene(path: impl AsRef<Path>) -> Result<Scene, Box<dyn std::error::Error>> {
        let mut fichier = File::open(path)?;
        let mut tampon = Vec::new();
        fichier.read_to_end(&mut tampon)?;
        Self::charger_scene_bytes(&tampon)
    }

    /// Comme [`GlbLoader::charger_scene`], depuis des octets déjà en mémoire.
    pub fn charger_scene_bytes(octets: &[u8]) -> Result<Scene, Box<dyn std::error::Error>> {
        let (json_str, bin) = Self::decouper_glb(octets)?;
        let json: serde_json::Value = serde_json::from_str(json_str)?;

        let vide = Vec::new();
        let accessors = json["accessors"].as_array().unwrap_or(&vide);
        let vues = json["bufferViews"].as_array().unwrap_or(&vide);
        let maillages = json["meshes"].as_array().unwrap_or(&vide);
        let noeuds = json["nodes"].as_array().unwrap_or(&vide);

        let mut scene = Scene::default();

        // Les racines : celles de la scène désignée, ou tous les nœuds si le fichier n'en nomme
        // aucune. ⚠ Un fichier sans `scenes` est légal, et l'ignorer donnerait une scène vide.
        let racines: Vec<usize> = json["scenes"]
            .as_array()
            .and_then(|s| {
                let i = json["scene"].as_u64().unwrap_or(0) as usize;
                s.get(i)?["nodes"].as_array().map(|n| {
                    n.iter().filter_map(|v| v.as_u64().map(|x| x as usize)).collect()
                })
            })
            .unwrap_or_else(|| (0..noeuds.len()).collect());

        // Parcours en profondeur, en composant les transformations de parent en enfant.
        let mut pile: Vec<(usize, Mat4)> = racines.iter().rev().map(|i| (*i, Mat4::IDENTITY)).collect();
        let mut vus = vec![false; noeuds.len()];

        while let Some((idx, parent)) = pile.pop() {
            let Some(noeud) = noeuds.get(idx) else { continue };
            // Une hiérarchie glTF est un arbre ; un fichier abîmé pourrait en faire un cycle, et un
            // parcours qui y entrerait ne s'arrêterait jamais.
            if vus[idx] {
                log::warn!("glTF : le nœud {idx} est atteint deux fois — hiérarchie cyclique, branche ignorée.");
                continue;
            }
            vus[idx] = true;

            let monde = parent * Self::transformation_du_noeud(noeud);

            if let Some(enfants) = noeud["children"].as_array() {
                for e in enfants.iter().rev() {
                    if let Some(e) = e.as_u64() {
                        pile.push((e as usize, monde));
                    }
                }
            }

            let Some(im) = noeud["mesh"].as_u64().map(|v| v as usize) else { continue };
            let Some(maillage) = maillages.get(im) else { continue };
            let nom_objet = noeud["name"].as_str()
                .or_else(|| maillage["name"].as_str())
                .unwrap_or("(sans nom)");

            for (ip, prim) in maillage["primitives"].as_array().unwrap_or(&vide).iter().enumerate() {
                // 4 = TRIANGLES. Un ruban ou un éventail chargé comme des triangles s'afficherait
                // en désordre : on le nomme et on le laisse, plutôt que de le dessiner faux.
                let mode = prim["mode"].as_u64().unwrap_or(4);
                if mode != 4 {
                    log::warn!("glTF : « {nom_objet} » primitive {ip} en mode {mode} — ignorée (seul TRIANGLES = 4 est dessiné).");
                    continue;
                }
                let attributs = &prim["attributes"];
                let Some(ipos) = attributs["POSITION"].as_u64().map(|v| v as usize) else {
                    log::warn!("glTF : « {nom_objet} » primitive {ip} sans POSITION — ignorée.");
                    continue;
                };
                let Some(positions) = Self::lire_flottants(accessors, vues, bin, ipos, 3) else {
                    continue;
                };
                let nombre = positions.len() / 3;

                let normales = attributs["NORMAL"].as_u64()
                    .and_then(|i| Self::lire_flottants(accessors, vues, bin, i as usize, 3));
                let tangentes = attributs["TANGENT"].as_u64()
                    .and_then(|i| Self::lire_flottants(accessors, vues, bin, i as usize, 4));
                let uv0s = attributs["TEXCOORD_0"].as_u64()
                    .and_then(|i| Self::lire_flottants(accessors, vues, bin, i as usize, 2));
                let uv1s = attributs["TEXCOORD_1"].as_u64()
                    .and_then(|i| Self::lire_flottants(accessors, vues, bin, i as usize, 2));

                if normales.is_none() {
                    log::warn!(
                        "glTF : « {nom_objet} » n'a pas de normales — repli DÉDUIT DE LA POSITION, \
                         exact seulement sur une sphère centrée. Son éclairage sera faux."
                    );
                }

                // La comatrice transforme les normales sans les fausser sous échelle non uniforme,
                // et le signe du déterminant dit si la transformation est un miroir.
                let (comatrice, miroir) = Self::comatrice_et_miroir(&monde);
                let decalage = scene.sommets.len() as u32;

                for i in 0..nombre {
                    let p = Vec3::new(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);
                    let pm = monde * Vec4::new(p.x, p.y, p.z, 1.0);

                    let n = match &normales {
                        Some(n) => Vec3::new(n[i * 3], n[i * 3 + 1], n[i * 3 + 2]),
                        None => Self::normale_de_repli(p),
                    };
                    let mut nm = Self::appliquer_3x3(&comatrice, n).normalize_or_zero();
                    if miroir {
                        nm = -nm;
                    }

                    // La tangente garde sa quatrième composante : c'est le signe du bitangent, une
                    // convention d'orientation — la transformer comme un vecteur la détruirait.
                    let t = match &tangentes {
                        Some(t) => {
                            let v = Self::appliquer_3x3_direct(&monde, Vec3::new(t[i * 4], t[i * 4 + 1], t[i * 4 + 2]))
                                .normalize_or_zero();
                            Vec4::new(v.x, v.y, v.z, t[i * 4 + 3])
                        }
                        None => Vec4::new(1.0, 0.0, 0.0, 1.0),
                    };
                    let uv0 = match &uv0s {
                        Some(u) => Vec2::new(u[i * 2], u[i * 2 + 1]),
                        None => Vec2::ZERO,
                    };
                    let uv1 = match &uv1s {
                        Some(u) => Vec2::new(u[i * 2], u[i * 2 + 1]),
                        None => Vec2::ZERO,
                    };

                    scene.sommets.push(Vertex::new(Vec3::new(pm.x, pm.y, pm.z), nm, t, uv0, uv1));
                }

                let premier = scene.indices.len() as u32;
                let bruts = match prim["indices"].as_u64() {
                    Some(i) => Self::lire_indices(accessors, vues, bin, i as usize)?,
                    None => (0..nombre as u32).collect(),
                };
                for tri in bruts.chunks(3) {
                    if tri.len() < 3 {
                        break;
                    }
                    // ⚠ Le miroir inverse le sens de parcours : on le rétablit ici, sans quoi
                    // l'élimination des faces arrière garderait l'intérieur et jetterait l'extérieur.
                    let (a, b, c) = if miroir {
                        (tri[0], tri[2], tri[1])
                    } else {
                        (tri[0], tri[1], tri[2])
                    };
                    scene.indices.extend_from_slice(&[a + decalage, b + decalage, c + decalage]);
                }

                scene.parties.push(Partie {
                    nom: nom_objet.to_string(),
                    premier_indice: premier,
                    nombre_indices: scene.indices.len() as u32 - premier,
                    premier_sommet: decalage,
                    nombre_sommets: nombre as u32,
                });
            }
        }

        if scene.parties.is_empty() {
            return Err("glTF : aucune primitive triangulaire dans ce fichier.".into());
        }

        log::info!(
            "Scène GLB chargée : {} parties, {} sommets, {} indices.",
            scene.parties.len(),
            scene.sommets.len(),
            scene.indices.len()
        );
        Ok(scene)
    }

    /// Découpe un GLB en son morceau JSON et son morceau binaire.
    fn decouper_glb(buffer: &[u8]) -> Result<(&str, &[u8]), Box<dyn std::error::Error>> {
        if buffer.len() < 20 || &buffer[0..4] != b"glTF" {
            return Err("Format GLB invalide (magic != glTF).".into());
        }
        let json_len = u32::from_le_bytes(buffer[12..16].try_into()?) as usize;
        if u32::from_le_bytes(buffer[16..20].try_into()?) != 0x4E4F534A {
            return Err("Chunk 0 GLB invalide (type != JSON).".into());
        }
        if buffer.len() < 20 + json_len + 8 {
            return Err("Pas de chunk BIN dans le fichier GLB.".into());
        }
        let json_str = std::str::from_utf8(&buffer[20..20 + json_len])?;
        let bin_offset = 20 + json_len;
        let bin_len = u32::from_le_bytes(buffer[bin_offset..bin_offset + 4].try_into()?) as usize;
        let fin = (bin_offset + 8 + bin_len).min(buffer.len());
        Ok((json_str, &buffer[bin_offset + 8..fin]))
    }

    /// La transformation d'un nœud glTF : soit sa `matrix`, soit la composition `T · R · S`.
    fn transformation_du_noeud(noeud: &serde_json::Value) -> Mat4 {
        // `matrix` est prioritaire et exclut T/R/S — c'est la spécification glTF, et un fichier qui
        // porterait les deux serait déjà invalide.
        if let Some(m) = noeud["matrix"].as_array() {
            if m.len() == 16 {
                let v: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
                // glTF écrit ses matrices en colonnes, comme `Mat4`.
                return Mat4::from_cols(
                    Vec4::new(v[0], v[1], v[2], v[3]),
                    Vec4::new(v[4], v[5], v[6], v[7]),
                    Vec4::new(v[8], v[9], v[10], v[11]),
                    Vec4::new(v[12], v[13], v[14], v[15]),
                );
            }
        }

        let lire3 = |cle: &str, defaut: Vec3| -> Vec3 {
            match noeud[cle].as_array() {
                Some(a) if a.len() == 3 => Vec3::new(
                    a[0].as_f64().unwrap_or(0.0) as f32,
                    a[1].as_f64().unwrap_or(0.0) as f32,
                    a[2].as_f64().unwrap_or(0.0) as f32,
                ),
                _ => defaut,
            }
        };
        let t = lire3("translation", Vec3::ZERO);
        let s = lire3("scale", Vec3::new(1.0, 1.0, 1.0));
        let r = match noeud["rotation"].as_array() {
            Some(a) if a.len() == 4 => [
                a[0].as_f64().unwrap_or(0.0) as f32,
                a[1].as_f64().unwrap_or(0.0) as f32,
                a[2].as_f64().unwrap_or(0.0) as f32,
                a[3].as_f64().unwrap_or(1.0) as f32,
            ],
            _ => [0.0, 0.0, 0.0, 1.0],
        };

        Mat4::from_translation(t) * Self::matrice_du_quaternion(r) * Mat4::from_scale(s)
    }

    /// Un quaternion glTF `[x, y, z, w]` en matrice de rotation.
    fn matrice_du_quaternion(q: [f32; 4]) -> Mat4 {
        let [x, y, z, w] = q;
        let (x2, y2, z2) = (x + x, y + y, z + z);
        let (xx, xy, xz) = (x * x2, x * y2, x * z2);
        let (yy, yz, zz) = (y * y2, y * z2, z * z2);
        let (wx, wy, wz) = (w * x2, w * y2, w * z2);
        Mat4::from_cols(
            Vec4::new(1.0 - (yy + zz), xy + wz, xz - wy, 0.0),
            Vec4::new(xy - wz, 1.0 - (xx + zz), yz + wx, 0.0),
            Vec4::new(xz + wy, yz - wx, 1.0 - (xx + yy), 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// La comatrice de la partie 3×3, et le fait que la transformation soit un **miroir**.
    ///
    /// ⚠ **Une normale ne se transforme pas comme un point.** Sous une échelle non uniforme, la
    /// matrice elle-même la fait pencher du mauvais côté : il faut la transposée de l'inverse.
    /// La comatrice lui est proportionnelle, et le facteur disparaît à la normalisation — donc
    /// **aucune division, aucun cas dégénéré à traiter**. Ses lignes sont trois produits vectoriels
    /// des colonnes.
    ///
    /// Le déterminant se lit alors gratuitement : c'est le produit scalaire de la première colonne
    /// par la première ligne de la comatrice. **Négatif, la transformation retourne l'espace.**
    fn comatrice_et_miroir(m: &Mat4) -> ([Vec3; 3], bool) {
        let c0 = Vec3::new(m.cols[0].x, m.cols[0].y, m.cols[0].z);
        let c1 = Vec3::new(m.cols[1].x, m.cols[1].y, m.cols[1].z);
        let c2 = Vec3::new(m.cols[2].x, m.cols[2].y, m.cols[2].z);
        let lignes = [c1.cross(c2), c2.cross(c0), c0.cross(c1)];
        let determinant = c0.dot(lignes[0]);
        (lignes, determinant < 0.0)
    }

    /// Applique une matrice donnée par ses trois **lignes** à un vecteur.
    fn appliquer_3x3(lignes: &[Vec3; 3], v: Vec3) -> Vec3 {
        Vec3::new(lignes[0].dot(v), lignes[1].dot(v), lignes[2].dot(v))
    }

    /// Applique la partie 3×3 d'une `Mat4` à une direction — sans translation, sans correction.
    /// C'est ce qu'il faut pour une **tangente**, qui suit la surface au lieu de lui être normale.
    fn appliquer_3x3_direct(m: &Mat4, v: Vec3) -> Vec3 {
        let r = *m * Vec4::new(v.x, v.y, v.z, 0.0);
        Vec3::new(r.x, r.y, r.z)
    }

    /// Lit un accesseur d'indices, quel que soit son type entier.
    fn lire_indices(
        accessors: &[serde_json::Value],
        buffer_views: &[serde_json::Value],
        bin: &[u8],
        indice_accesseur: usize,
    ) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let acc = accessors.get(indice_accesseur).ok_or("glTF : accesseur d'indices absent.")?;
        let nombre = acc["count"].as_u64().unwrap_or(0) as usize;
        let vue = &buffer_views[acc["bufferView"].as_u64().unwrap_or(0) as usize];
        let debut = vue["byteOffset"].as_u64().unwrap_or(0) as usize
            + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
        let genre = acc["componentType"].as_u64().unwrap_or(0);

        // 5121 = octet, 5123 = court, 5125 = entier. Un modèle de moins de 256 sommets emploie
        // légitimement le premier ; l'ignorer produirait un objet sans indices, donc invisible.
        let taille = match genre {
            5121 => 1,
            5123 => 2,
            5125 => 4,
            _ => return Err(format!("glTF : type d'indice {genre} inconnu.").into()),
        };
        if debut + nombre * taille > bin.len() {
            return Err("glTF : les indices débordent du tampon.".into());
        }

        let mut sortie = Vec::with_capacity(nombre);
        for i in 0..nombre {
            let o = debut + i * taille;
            sortie.push(match taille {
                1 => bin[o] as u32,
                2 => u16::from_le_bytes([bin[o], bin[o + 1]]) as u32,
                _ => u32::from_le_bytes([bin[o], bin[o + 1], bin[o + 2], bin[o + 3]]),
            });
        }
        Ok(sortie)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_blender_glb_assets() {
        let assets: [(&str, &[u8]); 8] = [
            ("saw_blade.glb", include_bytes!("../../../../assets/modeles/saw_blade.glb")),
            ("cannon_turret.glb", include_bytes!("../../../../assets/modeles/cannon_turret.glb")),
            ("spike_trap.glb", include_bytes!("../../../../assets/modeles/spike_trap.glb")),
            ("laser_emitter.glb", include_bytes!("../../../../assets/modeles/laser_emitter.glb")),
            ("flamethrower.glb", include_bytes!("../../../../assets/modeles/flamethrower.glb")),
            ("map.glb", include_bytes!("../../../../assets/modeles/map.glb")),
            ("plantedecendente.glb", include_bytes!("../../../../assets/modeles/plantedecendente.glb")),
            ("rockbasdroit.glb", include_bytes!("../../../../assets/modeles/rockbasdroit.glb")),
        ];

        // ⚠ Ce test lisait `/home/shaza/Documents/asset/…` : il ne pouvait donc passer QUE sur la
        // machine de l'auteur, et aurait échoué sur toute autre — y compris en intégration continue.
        // Un test qui ne s'exécute que chez une personne ne prouve rien sur ce qu'on distribue.
        // Il travaille désormais sur les modèles EMBARQUÉS, ceux qui partiront réellement.
        for (asset, octets) in assets {
            let res = GlbLoader::load_glb_raw_bytes(octets);
            assert!(res.is_ok(), "Échec du chargement de {}", asset);
            let (vertices, indices) = res.unwrap();
            let mut min_p = Vec3::splat(f32::MAX);
            let mut max_p = Vec3::splat(f32::MIN);
            for v in &vertices {
                let p = Vec3::from(v.position);
                min_p = min_p.min(p);
                max_p = max_p.max(p);
            }
            println!("Asset {}: {} sommets, {} indices | Min: {:?}, Max: {:?}", asset, vertices.len(), indices.len(), min_p, max_p);
        }
    }

    /// ⭐ LA MESURE QUI A PROUVÉ LE DÉFAUT — elle vaut plus que le correctif.
    ///
    /// Le chargeur ne lisait pas l'attribut `NORMAL` : il fabriquait `position.normalize()`, ce qui
    /// n'est exact que sur une sphère centrée. Le shader d'éclairage lit `in.normal` pour en tirer
    /// `N`, dont dépendent Lambert, GGX et les ombres — **donc tout modèle importé était éclairé de
    /// travers, sans que rien ne le signale.**
    ///
    /// Ce test compare, sur les vrais modèles du jeu, la normale LUE et celle qu'on INVENTAIT. Il
    /// échoue si l'écart redevient petit — c'est-à-dire si quelqu'un remettait le repli en place :
    /// *une garde qui n'a jamais dit non n'a pas été testée.*
    #[test]
    fn les_normales_viennent_du_fichier_et_pas_de_la_position() {
        let modeles: [(&str, &[u8]); 4] = [
            ("cannon_turret.glb", include_bytes!("../../../../assets/modeles/cannon_turret.glb")),
            ("spike_trap.glb", include_bytes!("../../../../assets/modeles/spike_trap.glb")),
            ("map.glb", include_bytes!("../../../../assets/modeles/map.glb")),
            ("rockbasdroit.glb", include_bytes!("../../../../assets/modeles/rockbasdroit.glb")),
        ];

        let mut pire_ecart_moyen: f32 = 0.0;
        for (nom, octets) in modeles {
            let (sommets, _) = GlbLoader::load_glb_raw_bytes(octets).expect(nom);

            // Une normale lue est unitaire ; une normale inventée à partir d'une position nulle ne
            // l'est pas. C'est déjà un témoin, avant même de comparer les directions.
            let non_unitaires = sommets
                .iter()
                .filter(|v| (Vec3::from(v.normal).length() - 1.0).abs() > 0.01)
                .count();
            assert_eq!(
                non_unitaires, 0,
                "{nom} : {non_unitaires} normales non unitaires — l'attribut NORMAL n'est pas lu"
            );

            // L'écart angulaire moyen entre ce que le fichier dit et ce qu'on inventait.
            let mut somme_degres = 0.0f32;
            for v in &sommets {
                let lue = Vec3::from(v.normal);
                let inventee = GlbLoader::normale_de_repli(Vec3::from(v.position));
                let cos = lue.dot(inventee).clamp(-1.0, 1.0);
                somme_degres += cos.acos().to_degrees();
            }
            let moyen = somme_degres / sommets.len() as f32;
            println!("{nom} : écart moyen normale lue ↔ normale inventée = {moyen:.1}°");
            pire_ecart_moyen = pire_ecart_moyen.max(moyen);

            // 15° est très en dessous de ce qui a été mesuré : le seuil doit dire « ce n'est pas la
            // même donnée », pas reproduire le chiffre du jour.
            assert!(
                moyen > 15.0,
                "{nom} : écart de seulement {moyen:.1}° — le repli est-il revenu ?"
            );
        }
        println!("Pire écart moyen sur les quatre modèles : {pire_ecart_moyen:.1}°");
    }

    /// Fabrique un `.glb` minimal où POSITION et NORMAL **partagent une seule `bufferView`**, les
    /// normales étant atteintes par le `byteOffset` de leur **accesseur**.
    ///
    /// ⚠⚠ **CE CONSTRUCTEUR EXISTE PARCE QUE MA PREMIÈRE GARDE ÉTAIT CREUSE.** Je l'avais écrite
    /// sur `map.glb`, et la mutation l'a démasquée : en remettant le défaut (ignorer le décalage de
    /// l'accesseur), **elle restait verte**. Mesure faite ensuite sur les dix modèles du dépôt :
    /// *aucun* n'a de `byteOffset` d'accesseur, *aucun* ne partage une vue, *aucun* n'a de
    /// `byteStride`, et tous n'ont qu'un maillage et qu'une primitive. **Le cas n'existait
    /// simplement pas dans nos fichiers** — donc aucun test bâti sur eux ne pouvait mordre.
    ///
    /// *C'est la garde anti-test-creux du projet : exiger que l'instrument sache produire une
    /// PRÉSENCE avant de conclure d'une absence.* Ici, la présence se fabrique.
    fn glb_a_vue_partagee() -> Vec<u8> {
        // Trois sommets dans le plan XY, et des normales qui ne ressemblent à AUCUNE position :
        // toutes en +Z. Si le décalage de l'accesseur est ignoré, on relira les positions à leur
        // place — et la première, nulle, ne sera pas unitaire.
        let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let normales: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for v in positions { bin.extend_from_slice(&v.to_le_bytes()); }   // 0..36
        for v in normales { bin.extend_from_slice(&v.to_le_bytes()); }    // 36..72
        for v in indices { bin.extend_from_slice(&v.to_le_bytes()); }     // 72..78
        while !bin.len().is_multiple_of(4) { bin.push(0); }

        let json = r#"{"asset":{"version":"2.0"},
"accessors":[
{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},
{"bufferView":0,"byteOffset":36,"componentType":5126,"count":3,"type":"VEC3"},
{"bufferView":1,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}],
"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":72},{"buffer":0,"byteOffset":72,"byteLength":6}],
"buffers":[{"byteLength":78}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"mode":4}]}]}"#;
        let mut json_octets = json.as_bytes().to_vec();
        while !json_octets.len().is_multiple_of(4) { json_octets.push(b' '); }

        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((12 + 8 + json_octets.len() + 8 + bin.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json_octets.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
        glb.extend_from_slice(&json_octets);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
        glb.extend_from_slice(&bin);
        glb
    }

    /// Le second défaut du même geste : **le décalage de l'ACCESSEUR était ignoré**.
    ///
    /// L'ancien code ne lisait que celui de la `bufferView`. Tant qu'un accesseur a sa vue à lui ça
    /// marche — et c'est exactement le cas de nos dix modèles, ce qui explique que personne ne l'ait
    /// vu. Dès que deux accesseurs partagent une vue, on lit les octets du voisin **sans erreur**.
    #[test]
    fn le_decalage_de_l_accesseur_est_lu_quand_deux_attributs_partagent_une_vue() {
        let (sommets, indices) =
            GlbLoader::load_glb_raw_bytes(&glb_a_vue_partagee()).expect("le glb fabriqué");

        assert_eq!(sommets.len(), 3);
        assert_eq!(indices, vec![0, 1, 2]);

        for (i, v) in sommets.iter().enumerate() {
            let n = Vec3::from(v.normal);
            assert!(
                (n.length() - 1.0).abs() < 1e-5,
                "sommet {i} : normale de longueur {:.3} — les positions ont été relues à la place \
                 des normales, donc le décalage de l'accesseur est ignoré",
                n.length()
            );
            assert!(
                (n.z - 1.0).abs() < 1e-5,
                "sommet {i} : normale {n:?} au lieu de +Z — mauvais octets lus"
            );
        }
    }

    /// Les dix modèles du jeu n'ont qu'**un maillage et qu'une primitive** — c'est pourquoi la
    /// limite `meshes[0].primitives[0]` de l'ancienne voie n'a jamais gêné. Ce test garde le fait :
    /// le jour où l'un d'eux gagnera un second morceau, `load_glb*` l'amputera **en silence**, et
    /// il faudra le faire passer par [`GlbLoader::charger_scene`].
    #[test]
    fn les_modeles_du_jeu_restent_mono_primitive_donc_l_ancienne_voie_leur_suffit() {
        let modeles: [(&str, &[u8]); 3] = [
            ("map.glb", include_bytes!("../../../../assets/modeles/map.glb")),
            ("box.glb", include_bytes!("../../../../assets/modeles/box.glb")),
            ("saw_blade.glb", include_bytes!("../../../../assets/modeles/saw_blade.glb")),
        ];
        for (nom, octets) in modeles {
            let json_len = u32::from_le_bytes(octets[12..16].try_into().unwrap()) as usize;
            let json: serde_json::Value =
                serde_json::from_slice(&octets[20..20 + json_len]).expect(nom);
            let maillages = json["meshes"].as_array().expect(nom);
            let primitives: usize =
                maillages.iter().map(|m| m["primitives"].as_array().map_or(0, |p| p.len())).sum();
            assert_eq!(
                (maillages.len(), primitives),
                (1, 1),
                "{nom} porte plusieurs morceaux — `load_glb*` n'en lira qu'un, EN SILENCE. \
                 Ce modèle doit désormais passer par `charger_scene`."
            );
        }
    }

    // ═════════════════════════════════════════════════════════════════════════════════════════
    // LA VOIE « SCÈNE » — ouverte le 5 septembre 2026
    // ═════════════════════════════════════════════════════════════════════════════════════════

    const TABLE: &[u8] = include_bytes!("../../../../assets/modeles/table de teste verre.glb");

    /// ⭐ LA MESURE QUI A MOTIVÉ CE CHANTIER, gardée comme test.
    ///
    /// Sa scène de test porte **trois** objets. L'ancienne voie n'en rendait qu'un — un tiers du
    /// fichier — **sans erreur, sans avertissement**. C'est ce que ce test compare, dans les deux
    /// sens : il échouerait aussi bien si la nouvelle voie régressait que si l'ancienne se mettait
    /// à tout lire (auquel cas cette garde n'aurait plus lieu d'être).
    #[test]
    fn la_voie_scene_lit_les_trois_objets_que_l_ancienne_amputait() {
        let scene = GlbLoader::charger_scene_bytes(TABLE).expect("la table de test");
        assert_eq!(scene.parties.len(), 3, "trois objets dans le fichier, trois parties");

        let (anciens, _) = GlbLoader::load_glb_raw_bytes(TABLE).expect("l'ancienne voie");
        assert!(
            scene.sommets.len() > anciens.len(),
            "la scène ({}) doit porter plus que le premier objet seul ({})",
            scene.sommets.len(),
            anciens.len()
        );

        // Les parties couvrent exactement les indices, sans trou ni chevauchement.
        let somme: u32 = scene.parties.iter().map(|p| p.nombre_indices).sum();
        assert_eq!(somme as usize, scene.indices.len());
        for f in scene.parties.windows(2) {
            assert_eq!(f[0].premier_indice + f[0].nombre_indices, f[1].premier_indice);
        }

        // Tout indice pointe sur un sommet qui existe : une fusion mal décalée se verrait ici.
        let max = scene.indices.iter().copied().max().unwrap_or(0) as usize;
        assert!(max < scene.sommets.len(), "indice {max} hors des {} sommets", scene.sommets.len());
    }

    /// ⚠ Les nœuds de sa scène portent des translations proches de `y = 3,7`. Sans elles, les trois
    /// objets s'empileraient à l'origine — **une scène qui a l'air de marcher et qui est fausse.**
    #[test]
    fn les_transformations_de_noeud_sont_appliquees() {
        let scene = GlbLoader::charger_scene_bytes(TABLE).expect("la table de test");
        let y_moyen: f32 =
            scene.sommets.iter().map(|s| s.position[1]).sum::<f32>() / scene.sommets.len() as f32;
        assert!(
            y_moyen > 1.0,
            "les objets sont restés à l'origine (y moyen = {y_moyen}) : la transformation de nœud \
             n'a pas été appliquée"
        );

        // L'ancienne voie, elle, ne l'applique PAS — et c'est ce qui rend son remplacement
        // impossible sans déplacer le décor du jeu.
        let (anciens, _) = GlbLoader::load_glb_raw_bytes(TABLE).expect("l'ancienne voie");
        let y_ancien: f32 =
            anciens.iter().map(|s| s.position[1]).sum::<f32>() / anciens.len() as f32;
        assert!(
            y_ancien.abs() < 1.0,
            "l'ancienne voie s'est mise à transformer (y = {y_ancien}) — le décor du jeu vient de bouger"
        );
    }

    /// Un GLB d'un seul triangle, sous la transformation demandée, portant la normale demandée.
    ///
    /// *La normale est un paramètre et non un octet à corriger après coup : une première version de
    /// ce fabricant patchait le binaire à la main et visait deux octets à côté — le test échouait
    /// alors pour une raison étrangère à ce qu'il mesure, ce qui est la pire espèce de test.*
    fn glb_a_noeud(
        translation: [f32; 3],
        rotation: [f32; 4],
        echelle: [f32; 3],
        normale: [f32; 3],
    ) -> Vec<u8> {
        let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let normales: [f32; 9] = [
            normale[0], normale[1], normale[2],
            normale[0], normale[1], normale[2],
            normale[0], normale[1], normale[2],
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for v in positions { bin.extend_from_slice(&v.to_le_bytes()); }
        for v in normales { bin.extend_from_slice(&v.to_le_bytes()); }
        for v in indices { bin.extend_from_slice(&v.to_le_bytes()); }
        while !bin.len().is_multiple_of(4) { bin.push(0); }

        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},
"scene":0,"scenes":[{{"nodes":[0]}}],
"nodes":[{{"name":"essai","mesh":0,"translation":[{},{},{}],"rotation":[{},{},{},{}],"scale":[{},{},{}]}}],
"accessors":[
{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},
{{"bufferView":0,"byteOffset":36,"componentType":5126,"count":3,"type":"VEC3"}},
{{"bufferView":1,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":72}},{{"buffer":0,"byteOffset":72,"byteLength":6}}],
"buffers":[{{"byteLength":78}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"mode":4}}]}}]}}"#,
            translation[0], translation[1], translation[2],
            rotation[0], rotation[1], rotation[2], rotation[3],
            echelle[0], echelle[1], echelle[2]
        );
        let mut json_octets = json.into_bytes();
        while !json_octets.len().is_multiple_of(4) { json_octets.push(b' '); }

        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((12 + 8 + json_octets.len() + 8 + bin.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json_octets.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_octets);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin);
        glb
    }

    /// ⭐⭐ LA GARDE LA PLUS SUBTILE DE CE CHANTIER, et elle vient de sa scène.
    ///
    /// Son nœud « Cube » porte une échelle de **−2,95 sur deux axes** : un miroir. Un miroir inverse
    /// le sens de parcours des triangles — donc, sans compensation, l'élimination des faces arrière
    /// **garde l'intérieur et jette l'extérieur**. L'objet apparaît creux ou retourné, et rien dans
    /// la compilation, les types ou les autres tests ne le dit.
    ///
    /// *Ce test échouerait si l'on retirait la compensation, ce qu'aucune relecture ne garantit.*
    #[test]
    fn une_echelle_negative_retourne_le_sens_de_parcours_et_la_normale() {
        let droit = GlbLoader::charger_scene_bytes(&glb_a_noeud(
            [0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0],
        )).expect("sans miroir");
        assert_eq!(droit.indices, vec![0, 1, 2], "sans miroir, l'ordre ne bouge pas");

        let miroir = GlbLoader::charger_scene_bytes(&glb_a_noeud(
            [0.0; 3], [0.0, 0.0, 0.0, 1.0], [-1.0, 1.0, 1.0], [0.0, 0.0, 1.0],
        )).expect("avec miroir");
        assert_eq!(
            miroir.indices, vec![0, 2, 1],
            "un miroir doit inverser le sens de parcours, sinon la face visible est jetée"
        );

        // La normale de ce triangle est en +Z ; un miroir sur X ne doit PAS la retourner.
        let n = miroir.sommets[0].normal;
        assert!(
            (n[2] - 1.0).abs() < 1e-5,
            "la normale devrait rester (0,0,1) sous un miroir en X, elle vaut {n:?}"
        );
    }

    /// Une échelle **non uniforme** fait pencher une normale si on la transforme comme un point.
    /// La comatrice est là pour ça, et voici la vérité analytique qui le prouve : sur un plan
    /// incliné à 45°, aplatir Y de moitié doit **redresser** la normale, pas la coucher.
    #[test]
    fn la_normale_suit_la_comatrice_et_non_la_matrice() {
        // Normale à 45° dans le plan XY, sur un nœud qui aplatit Y de moitié.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let glb = glb_a_noeud([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0, 0.5, 1.0], [s, s, 0.0]);

        let scene = GlbLoader::charger_scene_bytes(&glb).expect("échelle non uniforme");
        let n = scene.sommets[0].normal;

        // Aplatir Y de moitié rend la pente DEUX FOIS plus raide, donc la normale se couche vers X :
        // (M⁻¹)ᵀ·(1,1,0) ∝ (1, 2, 0). Transformer par M donnerait (1, 0.5, 0) — l'inverse exact.
        let attendu = Vec3::new(1.0, 2.0, 0.0).normalize();
        assert!(
            (n[0] - attendu.x).abs() < 1e-4 && (n[1] - attendu.y).abs() < 1e-4,
            "normale {n:?}, attendue {attendu:?} — la matrice a été employée à la place de la comatrice"
        );
    }

    /// Une hiérarchie doit composer : un enfant translaté dans un parent translaté cumule les deux.
    #[test]
    fn la_hierarchie_des_noeuds_se_compose() {
        let scene = GlbLoader::charger_scene_bytes(TABLE).expect("la table");
        // Sa scène est plate ; ce qui se garde ici, c'est qu'aucun objet n'a été perdu ni doublé.
        assert_eq!(scene.parties.len(), 3);
        let noms: Vec<&str> = scene.parties.iter().map(|p| p.nom.as_str()).collect();
        assert!(noms.contains(&"Circle"), "noms lus : {noms:?}");
        assert!(noms.contains(&"Cube"), "noms lus : {noms:?}");
    }

    /// ⚠ GARDE ANTI-RÉGRESSION : l'ancienne voie ne doit RIEN changer pour le jeu.
    ///
    /// Les dix modèles portent tous une transformation de nœud que `load_glb*` ignore, et le jeu
    /// place ses objets lui-même. *Si un jour quelqu'un « corrige » l'ancienne voie pour appliquer
    /// ces transformations, tout le décor bougera d'un coup — et seul ce test le dira.*
    #[test]
    fn l_ancienne_voie_ignore_toujours_les_transformations_de_noeud() {
        let modeles: [(&str, &[u8]); 3] = [
            ("map.glb", include_bytes!("../../../../assets/modeles/map.glb")),
            ("saw_blade.glb", include_bytes!("../../../../assets/modeles/saw_blade.glb")),
            ("spike_trap.glb", include_bytes!("../../../../assets/modeles/spike_trap.glb")),
        ];
        for (nom, octets) in modeles {
            let (bruts, _) = GlbLoader::load_glb_raw_bytes(octets).expect(nom);
            let json_len = u32::from_le_bytes(octets[12..16].try_into().unwrap()) as usize;
            let json: serde_json::Value =
                serde_json::from_slice(&octets[20..20 + json_len]).expect(nom);
            let t = &json["nodes"][0]["translation"];
            if let Some(a) = t.as_array() {
                let ty = a[1].as_f64().unwrap_or(0.0) as f32;
                if ty.abs() > 0.5 {
                    let y: f32 = bruts.iter().map(|s| s.position[1]).sum::<f32>() / bruts.len() as f32;
                    assert!(
                        (y - ty).abs() > 0.1,
                        "{nom} : l'ancienne voie a appliqué la translation ({y} ≈ {ty}) — le jeu vient de bouger"
                    );
                }
            }
        }
    }
}
