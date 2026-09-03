//! **L'ÉTAT DU MOTEUR, CALCULÉ — pour qu'il cesse d'être RACONTÉ.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example etat --no-default-features
//! ```
//!
//! ## Pourquoi cet outil existe (3 septembre 2026)
//!
//! Sa remarque, et elle est juste : *« comment je vais faire pour travailler plus tard si tout est
//! en bordel et qu'il faut cette compréhension-là juste pour COMMENCER une session ? »*
//!
//! Mesuré le jour même : une ouverture de session lisait **331 Ko** avant d'écrire une ligne, et
//! `prive/moteur/` pesait **205 Ko après trois jours d'existence** — plus que trois mois de fiches
//! de méthode. Et le pire : **trois de ces documents m'ont menti dans la même heure**. Ils juraient
//! que `descriptorIndexing` n'était pas demandé à la carte ; il l'était depuis trois semaines.
//!
//! ## Ce que cet outil sépare, et c'est toute l'idée
//!
//! Un document de projet mélange deux natures de choses qui n'ont pas la même durée de vie :
//!
//! - **Les GARDES** — un piège, une leçon, un *pourquoi*, une décision et sa raison. Ça ne se périme
//!   jamais. Ça vaut son poids, et **on n'y touche pas**.
//! - **L'ÉTAT** — « le moteur demande X », « 85 tests », « 13 337 lignes », « sync2 est utilisé ».
//!   **C'est faux dès le lendemain**, et ça occupe l'essentiel du volume.
//!
//! *Un texte se recopie, donc il diverge ; une commande, non.* Le projet a déjà eu deux fois cette
//! idée — le hook de démarrage qui **calcule** l'état des dépôts (22 août), et `voir.rs` qui
//! **calcule** la planche des preuves (2 septembre). Ceci est la troisième, appliquée au moteur.
//!
//! ⚠ **Ce que cet outil ne fera JAMAIS :** remplacer une garde. Il ne dit pas *pourquoi* le moteur
//! est en 1.3, ni que `naga` retourne l'axe Y, ni qu'une fenêtre masquée fausse une mesure de 3 à
//! 4×. Ça, ça reste écrit — c'est ce qui ne se calcule pas.
//!
//! ⚠ **Et il ne compte pas les tests.** Le seul compte honnête vient de `cargo test`, qui les
//! *exécute* : un `#[test]` désarmé par un doc-comment mal placé se compte encore et ne tourne plus.
//! *Un compteur de tests qui monte n'a jamais prouvé que les tests tournent.*

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    let racine = racine_du_depot();

    titre("AEGIS — L'ÉTAT CALCULÉ");
    println!("  Rien de ce qui suit n'est écrit à la main. Tout est relu sur le disque à l'instant.");
    println!("  Racine : {}", racine.display());

    ce_que_le_moteur_exige(&racine);
    le_poids_du_code(&racine);
    les_shaders_compiles(&racine);
    ce_qui_dort(&racine);
    ce_que_personne_n_appelle(&racine);

    titre("CE QUI NE SE CALCULE PAS, ET QUI RESTE ÉCRIT");
    println!("  Les tests               → nix-shell shell.nix --run \"cargo test --release\"");
    println!("  Les warnings            → cargo clippy --release --all-targets -p <crate>");
    println!("                            ⚠ toujours dire AVEC QUELLE COMMANDE : le moteur seul et");
    println!("                              l'espace de travail entier ne donnent pas le même compte.");
    println!("  Les pièges du terrain   → prive/moteur/00-MOTEUR.md");
    println!("  Le cap et l'ordre       → prive/moteur/CAP.md, 01-DECOUPAGE.md");
    println!("  Ce que l'œil juge       → personne d'autre que lui");
}

/// ⭐ Ce que le moteur demande à une carte graphique, **lu dans le code** et non recopié.
///
/// C'est l'affirmation qui vivait dans trois documents privés, dont deux la disaient fausse.
fn ce_que_le_moteur_exige(racine: &Path) {
    titre("CE QUE LE MOTEUR DEMANDE À UNE CARTE");

    let capacites = lire(&racine.join("crates/aegis_engine/src/core/capacites.rs"));

    let version = capacites
        .lines()
        .find(|l| l.contains("pub const VERSION_EXIGEE"))
        .and_then(|l| l.split_once("make_api_version(0, "))
        .map(|(_, reste)| reste.trim_end_matches(");").replace(", ", "."))
        .unwrap_or_else(|| "?".into());
    println!("  Vulkan {version}");

    // Les `.nom(true)` de la fabrique de fonctionnalités — la seule liste qui existe.
    let fabrique = entre(&capacites, "pub fn fonctionnalites_13", "\n}");
    let mut aucune = true;
    for morceau in fabrique.split('.').skip(1) {
        if let Some(fin) = morceau.find('(') {
            if morceau[fin..].starts_with("(true)") {
                println!("  · {}", &morceau[..fin]);
                aucune = false;
            }
        }
    }
    if aucune {
        println!("  · (aucune fonctionnalité optionnelle)");
    }
    println!("  Garde : `le_moteur_ne_demande_que_ce_qu_il_utilise` échoue si l'une d'elles ne sert pas.");
}

fn le_poids_du_code(racine: &Path) {
    titre("LE POIDS DU CODE");
    for (nom, dossier) in [
        ("moteur", "crates/aegis_engine/src"),
        ("jeu", "crates/aegis_game/src"),
    ] {
        let (vivant_l, vivant_f, dormant_l, dormant_f) = peser(&racine.join(dossier));
        println!("  {nom:8} {vivant_l:>6} lignes  {vivant_f:>3} fichiers");
        if dormant_f > 0 {
            println!(
                "  {:8} {dormant_l:>6} lignes  {dormant_f:>3} fichiers  ← endormis (préfixe `_`), non compilés",
                ""
            );
        }
    }
}

/// Les shaders **réellement compilés** — la source de vérité est `build.rs`, pas un dossier.
fn les_shaders_compiles(racine: &Path) {
    titre("LES SHADERS RÉELLEMENT COMPILÉS (lus dans build.rs)");
    let build = lire(&racine.join("crates/aegis_engine/build.rs"));
    let noms: Vec<&str> = build
        .lines()
        .filter(|l| l.contains(".vert.spv\""))
        .filter_map(|l| {
            let apres = &l[l.find("(\"")? + 2..];
            Some(&apres[..apres.find(".wgsl\"")?])
        })
        .collect();
    println!("  {} shaders : {}", noms.len(), noms.join(", "));
}

fn ce_qui_dort(racine: &Path) {
    titre("LES BRIQUES QUI DORMENT");
    println!("  ⚠ À lire AVANT de croire qu'une technique manque : de vraies formules testées");
    println!("    vivent là-dedans (réservoir ReSTIR, WBOIT/MBOIT, LEAN, Cauchy).");
    let mut trouves = Vec::new();
    parcourir(&racine.join("crates"), &mut |chemin| {
        let nom = fichier(chemin);
        if nom.starts_with('_') && nom.ends_with(".rs") || nom.starts_with('_') && nom.ends_with(".wgsl") {
            trouves.push(court(chemin, racine));
        }
    });
    trouves.sort();
    println!("  {} fichiers :", trouves.len());
    for t in &trouves {
        println!("    {t}");
    }
}

/// ⚠⚠ **UNE SONDE APPROXIMATIVE, ET ELLE LE DIT.**
///
/// Elle compte les mentions du nom d'un module **hors de son propre fichier**. Zéro mention = très
/// probablement personne ne l'appelle. Ce n'est **pas** une analyse sémantique : un module utilisé
/// via `use` puis par un nom de type seul serait mal compté.
///
/// *Elle est calibrée sur deux cas connus* — `epaisseur` et `verre` sont documentés comme n'étant
/// appelés par rien d'autre que leurs tests. Si la sonde cesse de les voir, c'est elle qui a changé,
/// pas le moteur.
fn ce_que_personne_n_appelle(racine: &Path) {
    titre("CE QUE PERSONNE N'APPELLE (sonde approximative — voir sa doc)");
    println!("  La famille de défauts n° 1 du projet : du code complet, correct, branché à rien.");

    let src = racine.join("crates/aegis_engine/src");
    // ⚠⚠ ON REGARDE LES DEUX CRATES. Ma première version ne lisait que le moteur, et elle a
    // accusé `camera`, `ecran`, `ombre`, `occlusion`, `voxel` — tous appelés **par le jeu**.
    // *Une sonde qui ne regarde qu'une moitié du monde trouve des orphelins qui n'en sont pas.*
    let tout = racine.join("crates");
    let mut modules: BTreeMap<String, PathBuf> = BTreeMap::new();
    parcourir(&src, &mut |chemin| {
        let nom = fichier(chemin);
        if let Some(base) = nom.strip_suffix(".rs") {
            if !base.starts_with('_') && !matches!(base, "mod" | "lib" | "main") {
                modules.insert(base.to_string(), chemin.to_path_buf());
            }
        }
    });

    let mut orphelins = Vec::new();
    for (nom, chemin) in &modules {
        let mut mentions = 0usize;
        parcourir(&tout, &mut |autre| {
            if autre == chemin || !fichier(autre).ends_with(".rs") {
                return;
            }
            // Les déclarations `pub mod nom;` ne comptent pas : déclarer n'est pas appeler.
            // C'est exactement la nuance qui a laissé 19 briques mortes passer pour vivantes.
            for ligne in lire(autre).lines() {
                let l = ligne.trim();
                if l.starts_with("pub mod ") || l.starts_with("mod ") {
                    continue;
                }
                if l.contains(&format!("{nom}::")) {
                    mentions += 1;
                }
            }
        });
        if mentions == 0 {
            orphelins.push(nom.clone());
        }
    }

    for o in &orphelins {
        println!("    {o}");
    }
    // ⭐ DEUX CALIBRAGES, ET LE SECOND EST LE PLUS IMPORTANT.
    //
    // Les cas POSITIFS vérifient que la sonde sait accuser ; les cas NÉGATIFS vérifient qu'elle
    // sait innocenter. Sans les seconds, une sonde qui accuse le monde entier afficherait un
    // calibrage parfait — c'est exactement ce qui s'est produit à sa première exécution.
    //
    // *Une absence n'est jamais une preuve tant que l'instrument n'a pas montré qu'il sait
    // produire une présence — et l'inverse est vrai aussi.*
    let doivent_apparaitre = ["epaisseur", "verre"];
    let ne_doivent_pas = ["camera", "ecran", "ombre", "glb_loader", "capacites"];

    let manquants: Vec<&&str> =
        doivent_apparaitre.iter().filter(|c| !orphelins.contains(&c.to_string())).collect();
    let intrus: Vec<&&str> =
        ne_doivent_pas.iter().filter(|c| orphelins.contains(&c.to_string())).collect();

    println!(
        "  Calibrage : {}/{} orphelins connus retrouvés · {}/{} modules vivants correctement innocentés.",
        doivent_apparaitre.len() - manquants.len(),
        doivent_apparaitre.len(),
        ne_doivent_pas.len() - intrus.len(),
        ne_doivent_pas.len()
    );
    if !manquants.is_empty() || !intrus.is_empty() {
        println!("  ⚠⚠ LA SONDE EST FAUSSE — ne pas croire la liste ci-dessus.");
        if !manquants.is_empty() {
            println!("     elle ne voit plus : {manquants:?}");
        }
        if !intrus.is_empty() {
            println!("     elle accuse à tort : {intrus:?}");
        }
    }
}

// ─────────────────────────── la plomberie ───────────────────────────

fn racine_du_depot() -> PathBuf {
    // `CARGO_MANIFEST_DIR` pointe la crate ; le dépôt est deux crans au-dessus. Déterminé à la
    // compilation, donc insensible au répertoire depuis lequel on lance la commande — le piège qui
    // avait fait chercher les images de preuve au mauvais endroit le 2 septembre.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("arborescence inattendue")
        .to_path_buf()
}

fn titre(t: &str) {
    println!("\n\x1b[1m{t}\x1b[0m");
    println!("{}", "─".repeat(t.chars().count()));
}

fn lire(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn fichier(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn court(p: &Path, racine: &Path) -> String {
    p.strip_prefix(racine).unwrap_or(p).display().to_string()
}

fn entre<'a>(texte: &'a str, debut: &str, fin: &str) -> &'a str {
    let Some(i) = texte.find(debut) else { return "" };
    let reste = &texte[i..];
    let j = reste.find(fin).unwrap_or(reste.len());
    &reste[..j]
}

/// Lignes et fichiers, en séparant ce qui est compilé de ce qui dort.
fn peser(dossier: &Path) -> (usize, usize, usize, usize) {
    let (mut vl, mut vf, mut dl, mut df) = (0, 0, 0, 0);
    parcourir(dossier, &mut |chemin| {
        let nom = fichier(chemin);
        if !(nom.ends_with(".rs") || nom.ends_with(".wgsl")) {
            return;
        }
        let lignes = lire(chemin).lines().count();
        if nom.starts_with('_') {
            dl += lignes;
            df += 1;
        } else {
            vl += lignes;
            vf += 1;
        }
    });
    (vl, vf, dl, df)
}

fn parcourir(dossier: &Path, faire: &mut impl FnMut(&Path)) {
    let Ok(entrees) = std::fs::read_dir(dossier) else { return };
    let mut chemins: Vec<PathBuf> = entrees.flatten().map(|e| e.path()).collect();
    chemins.sort();
    for chemin in chemins {
        if chemin.is_dir() {
            if fichier(&chemin) != "target" {
                parcourir(&chemin, faire);
            }
        } else {
            faire(&chemin);
        }
    }
}
