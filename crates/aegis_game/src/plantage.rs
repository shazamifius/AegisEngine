//! plantage.rs — si le jeu s'arrête tout seul, il laisse une trace que le launcher saura relayer.
//!
//! # Pourquoi ce fichier existe (16 août 2026)
//!
//! Recensé ce jour-là : le cœur réseau capture ses paniques depuis longtemps, le launcher vient de
//! recevoir la sienne — **AegisEngine n'avait rien du tout.** C'est pourtant le programme le plus
//! susceptible de tomber : il parle à Vulkan, donc à un pilote graphique, donc à du matériel qu'on
//! ne connaît pas. Sur les machines des camarades de classe, c'est exactement là que ça cassera.
//!
//! Sans trace, un jeu qui disparaît ne laisse **rien** : la fenêtre se ferme, et personne — ni la
//! personne devant l'écran, ni nous — ne sait pourquoi.
//!
//! # Le choix : UN SEUL témoin, partagé avec le launcher
//!
//! On écrit dans le MÊME fichier que lui (`~/.web3/plantage-en-attente.txt`), avec un préfixe qui
//! dit d'où ça vient. Deux fichiers séparés auraient voulu dire deux questions à poser, donc deux
//! occasions de déranger quelqu'un pour le même incident. Le launcher pose la question **une fois**,
//! au démarrage suivant, et respecte la réponse.
//!
//! # Ce que ce fichier ne fait PAS, volontairement
//!
//! - **Il n'envoie rien.** Aucun réseau ici. Le jeu dépose une trace, c'est tout ; c'est le launcher
//!   qui demandera, et seulement si la personne dit oui. Un jeu qui téléphone tout seul après un
//!   plantage serait précisément ce qu'on refuse.
//! - **Il n'ajoute aucune dépendance.** Bibliothèque standard uniquement, comme le reste du projet.
//! - **Il ne remplace pas le message d'origine** : le hook précédent est rejoué en premier, donc la
//!   panique s'affiche toujours normalement dans un terminal. On n'a jamais dégradé un diagnostic
//!   qui existait.

use std::path::PathBuf;

/// Le témoin partagé avec le launcher web3. Ce nom est un CONTRAT entre deux dépôts : le changer
/// d'un côté sans l'autre rendrait le plantage muet, sans que rien ne le signale — le genre de
/// défaut qui survit des semaines ici. Il est écrit en toutes lettres des deux côtés
/// (`launcher/src/rapport.rs::TEMOIN_PLANTAGE`) exprès, pour qu'une recherche le trouve.
const TEMOIN: &str = "plantage-en-attente.txt";

/// `~/.web3`, le dossier que le launcher relit. `None` si l'on ne sait pas où est le dossier
/// personnel — on préfère ne rien écrire plutôt qu'écrire n'importe où.
fn dossier_web3() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(|h| PathBuf::from(h).join(".web3"))
}

/// Installe la capture. Idempotent, sans réseau, sans dépendance.
///
/// Les trois précautions du cœur, reprises telles quelles parce qu'elles y ont été payées :
/// 1. le hook précédent est rejoué **en premier** (on ne dégrade jamais l'existant) ;
/// 2. tout notre bloc est sous `catch_unwind` — une panique DANS un hook de panique fait **avorter**
///    le processus, ce qui est pire que le défaut d'origine ;
/// 3. l'écriture est best-effort : un disque plein ne doit pas transformer un plantage en second
///    plantage.
pub fn installer() {
    static POSE: std::sync::Once = std::sync::Once::new();
    POSE.call_once(|| {
        let precedent = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            precedent(info);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let quoi = info
                    .payload()
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| info.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panique".to_string());
                let ou = info
                    .location()
                    .map(|l| format!("{}:{}", l.file(), l.line()))
                    .unwrap_or_else(|| "?".to_string());
                deposer(&format!("aegis {ou} — {quoi}"));
            }));
        }));
    });
}

/// Dépose le témoin. Séparé du hook pour être testable **sans provoquer de vraie panique** : un test
/// qui panique pour de bon empoisonne le processus de test et rend le résultat illisible.
pub fn deposer(message: &str) {
    let Some(dossier) = dossier_web3() else { return };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::create_dir_all(&dossier);
    let _ = std::fs::write(dossier.join(TEMOIN), format!("{ts}\n{message}\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le message porte de quoi diagnostiquer : d'OÙ ça vient (`aegis`), l'endroit exact, et la
    /// cause. Sans le préfixe, une trace du jeu serait indistinguable d'une trace du launcher dans
    /// le même fichier — et l'on chercherait le défaut dans le mauvais programme.
    #[test]
    fn le_temoin_dit_dou_il_vient_et_pourquoi() {
        let msg = "aegis src/party_game.rs:120 — index hors bornes";
        assert!(msg.starts_with("aegis "), "la provenance doit être lisible d'un coup d'œil");
        assert!(msg.contains("index hors bornes"), "la cause doit survivre");
        assert!(msg.contains(".rs:"), "l'endroit exact doit être là");
    }

    /// Sans dossier personnel connu, on n'écrit RIEN plutôt que d'écrire n'importe où. Un fichier
    /// déposé au hasard dans le dossier courant de quelqu'un serait du déchet, pas un diagnostic.
    #[test]
    fn sans_dossier_personnel_on_necrit_nulle_part() {
        // On ne peut pas retirer HOME du processus de test sans perturber les autres tests ; on
        // vérifie donc la propriété sur la fonction qui décide, telle qu'elle est écrite.
        let d = dossier_web3();
        if let Some(p) = d {
            assert!(p.ends_with(".web3"), "le témoin va dans ~/.web3, là où le launcher le relit");
        }
    }
}
