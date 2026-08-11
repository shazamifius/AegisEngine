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

    fn load_glb_internal(path: impl AsRef<Path>, normalize: bool) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

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

    fn parse_glb_json(json_str: &str, bin_data: &[u8], normalize: bool) -> Result<(Vec<Vertex>, Vec<u32>), Box<dyn std::error::Error>> {
        let parsed: serde_json::Value = serde_json::from_str(json_str)?;
        let accessors = parsed["accessors"].as_array().ok_or("Pas d'accessors dans le GLB")?;
        let buffer_views = parsed["bufferViews"].as_array().ok_or("Pas de bufferViews dans le GLB")?;

        let mut pos_accessor_idx = None;
        let mut ind_accessor_idx = None;

        if let Some(meshes) = parsed["meshes"].as_array() {
            if let Some(primitives) = meshes[0]["primitives"].as_array() {
                if let Some(pos) = primitives[0]["attributes"]["POSITION"].as_u64() {
                    pos_accessor_idx = Some(pos as usize);
                }
                if let Some(ind) = primitives[0]["indices"].as_u64() {
                    ind_accessor_idx = Some(ind as usize);
                }
            }
        }

        let pos_acc_idx = pos_accessor_idx.ok_or("Attribut POSITION non trouvé dans GLB")?;
        let pos_acc = &accessors[pos_acc_idx];
        let pos_bv_idx = pos_acc["bufferView"].as_u64().unwrap() as usize;
        let pos_count = pos_acc["count"].as_u64().unwrap() as usize;
        let pos_bv_offset = buffer_views[pos_bv_idx]["byteOffset"].as_u64().unwrap_or(0) as usize;

        let mut vertices = Vec::with_capacity(pos_count);

        // Read positions (3 x f32)
        let pos_ptr = bin_data[pos_bv_offset..].as_ptr() as *const f32;
        for i in 0..pos_count {
            let px = unsafe { *pos_ptr.add(i * 3 + 0) };
            let py = unsafe { *pos_ptr.add(i * 3 + 1) };
            let pz = unsafe { *pos_ptr.add(i * 3 + 2) };

            let pos = Vec3::new(px, py, pz);
            let normal = pos.normalize_or_zero();
            let tangent = Vec4::new(1.0, 0.0, 0.0, 1.0);

            vertices.push(Vertex::new(pos, normal, tangent, Vec2::ZERO, Vec2::ZERO));
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

        // Read indices
        let mut indices = Vec::new();
        if let Some(ind_acc_idx) = ind_accessor_idx {
            let ind_acc = &accessors[ind_acc_idx];
            let ind_count = ind_acc["count"].as_u64().unwrap() as usize;
            let ind_bv_idx = ind_acc["bufferView"].as_u64().unwrap() as usize;
            let ind_bv_offset = buffer_views[ind_bv_idx]["byteOffset"].as_u64().unwrap_or(0) as usize;
            let component_type = ind_acc["componentType"].as_u64().unwrap();

            if component_type == 5123 {
                // UNSIGNED_SHORT (u16)
                let ind_ptr = bin_data[ind_bv_offset..].as_ptr() as *const u16;
                for i in 0..ind_count {
                    indices.push(unsafe { *ind_ptr.add(i) } as u32);
                }
            } else if component_type == 5125 {
                // UNSIGNED_INT (u32)
                let ind_ptr = bin_data[ind_bv_offset..].as_ptr() as *const u32;
                for i in 0..ind_count {
                    indices.push(unsafe { *ind_ptr.add(i) });
                }
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
        let assets = [
            "saw_blade.glb",
            "cannon_turret.glb",
            "spike_trap.glb",
            "laser_emitter.glb",
            "flamethrower.glb",
            "map.glb",
            "plantedecendente.glb",
            "rockbasdroit.glb",
        ];

        for asset in assets {
            let path = format!("/home/shaza/Documents/asset/{}", asset);
            let res = GlbLoader::load_glb_raw(&path);
            assert!(res.is_ok(), "Échec du chargement de {}", path);
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
}
