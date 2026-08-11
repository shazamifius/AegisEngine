use std::path::Path;
use std::fs;

#[derive(Debug, Clone)]
pub struct Web3PlayerInfo {
    pub id: u32,
    pub name: String,
    pub wallet_address: Option<String>,
}

pub struct Web3Manager {
    pub players_file_path: String,
}

impl Web3Manager {
    pub fn new() -> Self {
        Self {
            players_file_path: "/home/shaza/Documents/projet web 3/players.json".to_string(),
        }
    }

    /// Charge les pseudonymes des joueurs Web3 depuis /home/shaza/Documents/projet web 3/players.json
    pub fn load_player_names(&self, player_count: usize) -> Vec<Web3PlayerInfo> {
        let path = Path::new(&self.players_file_path);
        let mut result = Vec::new();

        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(json_players) = serde_json_minimal_parse(&content) {
                    for (i, name) in json_players.into_iter().enumerate().take(player_count) {
                        result.push(Web3PlayerInfo {
                            id: i as u32,
                            name,
                            wallet_address: None,
                        });
                    }
                }
            }
        }

        // Compléter avec les pseudonymes par défaut si la liste Web3 a moins d'éléments que N joueurs
        for i in result.len()..player_count {
            let name = if i == 0 {
                "Joueur 1 (Toi)".to_string()
            } else {
                format!("Joueur Web3 {}", i + 1)
            };
            result.push(Web3PlayerInfo {
                id: i as u32,
                name,
                wallet_address: None,
            });
        }

        result
    }
}

/// Analyseur JSON simple et ultra-robuste sans dépendance lourde
fn serde_json_minimal_parse(json_str: &str) -> Result<Vec<String>, ()> {
    let mut names = Vec::new();
    let trimmed = json_str.trim();

    // Support du format JSON array: ["Joueur 1", "Joueur 2", ...]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        for part in inner.split(',') {
            let clean = part.trim().trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                names.push(clean.to_string());
            }
        }
        return Ok(names);
    }

    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web3_manager_default_fallback() {
        let mgr = Web3Manager::new();
        let players = mgr.load_player_names(4);
        assert_eq!(players.len(), 4);
        assert_eq!(players[0].name, "Joueur 1 (Toi)");
        assert_eq!(players[1].name, "Joueur Web3 2");
    }

    #[test]
    fn test_serde_json_minimal_parse() {
        let json = r#"["Alpha", "Beta", "Gamma"]"#;
        let parsed = serde_json_minimal_parse(json).unwrap();
        assert_eq!(parsed, vec!["Alpha", "Beta", "Gamma"]);
    }
}
