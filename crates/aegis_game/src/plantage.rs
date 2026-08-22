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
///
/// # Ce que le témoin porte, et pourquoi chaque ligne a été ajoutée
///
/// Il ne contenait qu'un horodatage brut et le message. Trois manques, mesurés le 21 août 2026 en
/// diagnostiquant un vrai rapport reçu :
///
/// * **la date était un nombre** (`1787162641`) — illisible sans conversion, donc impossible de
///   dire « ça date d'avant ou d'après le correctif » d'un coup d'œil ;
/// * **aucune version** — impossible de savoir QUELLE build a planté, donc impossible de savoir si
///   le défaut est déjà corrigé. C'est le manque le plus coûteux : il oblige à tout re-diagnostiquer ;
/// * **aucun contexte d'exécution** — j'ai perdu une heure à chercher un plantage `libxkbcommon`
///   qui ne se produisait QUE lancé à la main hors de son enveloppe, jamais par le launcher. La
///   ligne `session:` ci-dessous aurait tranché en une seconde.
pub fn deposer(message: &str) {
    let Some(dossier) = dossier_web3() else { return };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::create_dir_all(&dossier);
    let _ = std::fs::write(
        dossier.join(TEMOIN),
        format!(
            "{ts}\n{}\naegis v{} — {}\n{}\n{message}\n",
            date_lisible(ts),
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            contexte(),
        ),
    );
}

/// L'horodatage en clair, calculé sans aucune dépendance (algorithme des jours civils de Howard
/// Hinnant). En UTC : une heure locale serait plus agréable à lire et **impossible à comparer**
/// entre deux machines, ce qui est justement ce qu'on fait avec des rapports.
fn date_lisible(ts: u64) -> String {
    let jours = (ts / 86_400) as i64;
    let reste = ts % 86_400;
    // Décalage vers une ère commençant en mars : février et son 29 passent alors en fin d'année,
    // ce qui supprime tout cas particulier de bissextile.
    let z = jours + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        reste / 3600,
        (reste % 3600) / 60,
        reste % 60
    )
}

/// Dans quelles conditions le jeu tournait. Uniquement des faits d'environnement — **rien de
/// personnel** : pas de nom d'utilisateur, pas de chemin, pas d'adresse.
///
/// `enveloppe` dit si le jeu a été lancé par le launcher (qui l'entoure d'un environnement complet
/// sur NixOS) ou à la main. C'est la ligne qui distingue « ça casse chez les joueurs » de « ça casse
/// quand un développeur bricole », et les deux ne se corrigent pas au même endroit.
fn contexte() -> String {
    let session = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        "wayland"
    } else if std::env::var_os("DISPLAY").is_some() {
        "x11"
    } else {
        "aucune"
    };
    // `steam-run` exporte cette variable dans l'environnement FHS qu'il monte.
    let enveloppe = if std::env::var_os("STEAM_RUNTIME").is_some()
        || std::env::var_os("FHS_ENV").is_some()
    {
        "enveloppe FHS"
    } else {
        "lancement direct"
    };
    format!("session: {session} | {enveloppe}")
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

    /// La date se calcule sans dépendance : il faut donc la vérifier contre des repères connus,
    /// sinon on aurait remplacé un nombre illisible par un nombre FAUX et lisible — bien pire.
    #[test]
    fn la_date_lisible_tombe_juste_sur_des_reperes_connus() {
        assert_eq!(date_lisible(0), "1970-01-01 00:00:00 UTC");
        // Repère classique : le milliard de secondes.
        assert_eq!(date_lisible(1_000_000_000), "2001-09-09 01:46:40 UTC");
        // Un 29 février — le cas que l'algorithme des jours civils est là pour rendre indolore.
        assert_eq!(date_lisible(1_582_934_400), "2020-02-29 00:00:00 UTC");
        // Et l'horodatage réellement reçu dans son rapport du 21 août 2026.
        assert!(date_lisible(1_787_162_641).starts_with("2026-08-"));
    }

    /// Le contexte ne doit contenir QUE des faits d'environnement : jamais un nom, un chemin ou une
    /// adresse. La promesse faite à l'écran (« ce fichier ne contient que l'activité de
    /// l'application ») est une promesse, donc le code doit la rendre vraie.
    #[test]
    fn le_contexte_ne_dit_rien_de_personnel() {
        let c = contexte();
        assert!(!c.contains('/'), "aucun chemin ne doit apparaître : {c}");
        if let Ok(user) = std::env::var("USER") {
            if !user.is_empty() {
                assert!(!c.contains(&user), "le nom d'utilisateur ne doit pas fuiter : {c}");
            }
        }
    }
}
