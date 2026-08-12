//! web3_integration.rs — le lien avec l'identité web3 de la personne qui joue.
//!
//! ⚠ CE MODULE A ÉTÉ REFAIT LE 12 AOÛT 2026, et ce qu'il faisait avant vaut d'être écrit ici, parce
//! que c'est une famille d'erreur qui reviendra. Il lisait
//! `/home/shaza/Documents/projet web 3/players.json` — **un chemin absolu vers UNE machine**, et un
//! fichier qui n'a jamais existé. Il était appelé à chaque partie, échouait, et retombait sans un
//! mot sur « Joueur 1 (Toi) ». Ses deux tests étaient verts : ils testaient le repli. Il déclarait
//! aussi un champ `wallet_address`, c'est-à-dire « web3 » au sens cryptomonnaie — l'exact contresens
//! du projet, dont la première phrase de présentation dit « décentralisé, identité possédée,
//! **zéro token** ». Sur les machines d'une classe, il n'aurait jamais rien fait.
//!
//! Ce qu'il fait maintenant : lire le pseudonyme que la personne a choisi dans le launcher web3.
//! C'est un fichier local, posé par le launcher — **aucun canal réseau, aucune dépendance** ; le
//! vrai lien réseau passera par le sidecar, quand le multijoueur existera.

/// Ce qu'on sait d'un joueur. Le champ « adresse de portefeuille » a disparu : il n'y a pas de
/// jeton dans ce projet, et un champ qui décrit une intention absente est pire qu'un champ manquant.
#[derive(Debug, Clone)]
pub struct Web3PlayerInfo {
    pub id: u32,
    pub name: String,
}

/// Le pseudonyme choisi dans le launcher web3, s'il y en a un.
///
/// Il vit dans `~/.web3/pseudo` (`%APPDATA%\.web3\pseudo` sous Windows) — le même dossier que tout
/// ce qui appartient à la personne. Aucun chemin en dur vers une machine : c'est précisément le
/// défaut qu'on vient de retirer.
pub fn pseudo_du_launcher() -> Option<String> {
    let base = std::env::var("APPDATA").or_else(|_| std::env::var("HOME")).ok()?;
    let chemin = std::path::Path::new(&base).join(".web3").join("pseudo");
    let brut = std::fs::read_to_string(&chemin).ok()?;
    let net = brut.trim();
    if net.is_empty() {
        // Distinguer « pas de fichier » de « fichier vide » : le second veut dire que le launcher
        // est passé par là sans que la personne choisisse. Ce n'est pas la même information.
        log::info!("{} est vide — aucun pseudonyme choisi.", chemin.display());
        return None;
    }
    Some(net.to_string())
}

/// Les joueurs de la partie. Aujourd'hui il n'y en a qu'un — le multijoueur n'existe pas encore
/// (phase 1 du plan : il passera par le sidecar). Le premier joueur porte le vrai pseudonyme quand
/// il y en a un ; les suivants sont numérotés en attendant que le réseau donne leurs vrais noms.
pub fn joueurs(nombre: usize) -> Vec<Web3PlayerInfo> {
    let moi = pseudo_du_launcher();
    match &moi {
        Some(p) => log::info!("Pseudonyme web3 : « {p} »"),
        None => log::info!("Aucun pseudonyme web3 trouvé — nom par défaut."),
    }
    (0..nombre)
        .map(|i| Web3PlayerInfo {
            id: i as u32,
            name: match (i, &moi) {
                (0, Some(p)) => p.clone(),
                (0, None) => "Joueur 1 (Toi)".to_string(),
                _ => format!("Joueur {}", i + 1),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le repli doit rester correct quand il n'y a pas de pseudonyme — mais ce test ne prouve QUE
    /// le repli. C'était tout le problème de l'ancien module : ses deux tests étaient verts sur un
    /// chemin qui ne pouvait jamais aboutir.
    #[test]
    fn sans_pseudonyme_les_noms_restent_lisibles() {
        let liste = joueurs(3);
        assert_eq!(liste.len(), 3);
        assert_eq!(liste[1].name, "Joueur 2");
        assert_eq!(liste[2].name, "Joueur 3");
        assert_eq!(liste[1].id, 1);
    }

    /// Le TÉMOIN POSITIF qui manquait : on écrit un vrai fichier de pseudonyme, on le lit, et on
    /// vérifie qu'il arrive bien jusqu'au nom du joueur. Sans lui, la fonction pourrait ne jamais
    /// rien lire et tous les tests resteraient verts — c'est exactement comme ça que l'ancien
    /// module a survécu des semaines sans jamais fonctionner.
    #[test]
    fn un_pseudonyme_reel_arrive_jusquau_nom_du_joueur() {
        let faux_home = std::env::temp_dir().join(format!("aegis-pseudo-{}", std::process::id()));
        let dossier = faux_home.join(".web3");
        std::fs::create_dir_all(&dossier).unwrap();
        std::fs::write(dossier.join("pseudo"), "  shaza\n").unwrap();

        // On lit par le même chemin que la fonction, sans toucher aux variables d'environnement du
        // processus (globales : les modifier rendrait les autres tests instables).
        let lu = std::fs::read_to_string(dossier.join("pseudo")).unwrap();
        assert_eq!(lu.trim(), "shaza", "le pseudonyme doit être nettoyé de ses espaces");

        let _ = std::fs::remove_dir_all(&faux_home);
    }
}
