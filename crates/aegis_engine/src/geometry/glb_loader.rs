use std::fs::File;
use std::io::Read;
use std::path::Path;
use crate::core::math::{Vec2, Vec3, Vec4};
use crate::geometry::vertex::Vertex;

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
        let mut indices = Vec::new();
        if let Some(ind_acc_idx) = ind_accessor_idx {
            let ind_acc = &accessors[ind_acc_idx];
            let ind_count = ind_acc["count"].as_u64().unwrap_or(0) as usize;
            let ind_bv_idx = ind_acc["bufferView"].as_u64().unwrap_or(0) as usize;
            let debut = buffer_views[ind_bv_idx]["byteOffset"].as_u64().unwrap_or(0) as usize
                + ind_acc["byteOffset"].as_u64().unwrap_or(0) as usize;
            let component_type = ind_acc["componentType"].as_u64().unwrap_or(0);

            let taille = match component_type {
                5121 => 1, // UNSIGNED_BYTE
                5123 => 2, // UNSIGNED_SHORT
                5125 => 4, // UNSIGNED_INT
                _ => 0,
            };
            if taille == 0 {
                return Err(format!("glTF : type d'indice {component_type} inconnu.").into());
            }
            if debut + ind_count * taille > bin_data.len() {
                return Err("glTF : les indices débordent du tampon.".into());
            }

            indices.reserve(ind_count);
            for i in 0..ind_count {
                let o = debut + i * taille;
                indices.push(match taille {
                    1 => bin_data[o] as u32,
                    2 => u16::from_le_bytes([bin_data[o], bin_data[o + 1]]) as u32,
                    _ => u32::from_le_bytes([bin_data[o], bin_data[o + 1], bin_data[o + 2], bin_data[o + 3]]),
                });
            }
        }

        if indices.is_empty() {
            for i in 0..vertices.len() as u32 {
                indices.push(i);
            }
        }

        log::info!("Mesh GLB chargé (normalize={}) : {} sommets, {} indices.", normalize, vertices.len(), indices.len());
        Ok((vertices, indices))
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
        while bin.len() % 4 != 0 { bin.push(0); }

        let json = r#"{"asset":{"version":"2.0"},
"accessors":[
{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},
{"bufferView":0,"byteOffset":36,"componentType":5126,"count":3,"type":"VEC3"},
{"bufferView":1,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}],
"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":72},{"buffer":0,"byteOffset":72,"byteLength":6}],
"buffers":[{"byteLength":78}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"mode":4}]}]}"#;
        let mut json_octets = json.as_bytes().to_vec();
        while json_octets.len() % 4 != 0 { json_octets.push(b' '); }

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

    /// ⚠ Ce que le chargeur ne sait TOUJOURS pas faire, écrit comme un test qui l'affirme.
    ///
    /// Les dix modèles du dépôt n'ont qu'**un maillage et qu'une primitive** — c'est pourquoi la
    /// limite `meshes[0]["primitives"][0]` n'a jamais gêné. **Une vraie scène 3D exportée depuis
    /// Blender (plusieurs objets, ou un objet à plusieurs matériaux) perdra tout sauf son premier
    /// morceau, en silence.** Ce test grave le fait, pour que la prochaine session ne le
    /// redécouvre pas en croyant à un bug d'affichage.
    #[test]
    fn nos_modeles_n_ont_qu_une_primitive_et_c_est_pour_ca_que_la_limite_ne_se_voit_pas() {
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
                "{nom} porte plusieurs morceaux — le chargeur n'en lira qu'un, EN SILENCE. \
                 C'est le moment d'ouvrir la lecture multi-primitives."
            );
        }
    }
}
