//! # La console de pilotage — tester le jeu sans écran ni main humaine
//!
//! Sa demande, née d'un blocage réel : *« tu ne peux pas démarrer de fenêtre graphique chez un
//! SSH, mais tu peux envoyer des commandes pour piloter à distance et avoir des réponses… et ça on
//! en a besoin pour tous les tests »*.
//!
//! Le problème est exact. Un jeu Vulkan lancé par SSH n'a pas de session graphique, donc chaque
//! vérification demandait qu'un humain soit devant l'écran, sur chaque machine, en même temps. À
//! deux machines c'est pénible ; à trente-cinq c'est impossible. Et surtout : **on ne peut pas
//! rejouer un test**, donc on ne peut rien prouver deux fois.
//!
//! ## Ce que cette console N'EST PAS
//!
//! Ce n'est pas un mode de triche ni une porte dérobée. Elle **n'existe pas** dans un jeu lancé
//! normalement : il faut `AEGIS_CONSOLE=<port>` dans l'environnement pour qu'elle s'ouvre. Un
//! binaire distribué n'en porte que du code inerte.
//!
//! ⚠ **Portée honnête** : elle écoute sur la **loopback uniquement**, et n'authentifie personne.
//! Elle ferme donc l'accès depuis le réseau, PAS un autre programme tournant sous le même compte —
//! exactement la limite déjà documentée pour le jeton de contrôle du cœur. On ne prétend pas plus.
//!
//! ## Pourquoi une file, et pas un appel direct
//!
//! La console tourne dans son propre fil. Si elle touchait l'état du jeu directement, elle
//! prendrait des verrous pendant que la boucle de rendu enregistre ses commandes graphiques — la
//! saccade garantie, pour une fonctionnalité qui ne sert qu'aux tests.
//!
//! Elle **dépose** donc ses ordres dans une file que la boucle consomme quand ça l'arrange, et
//! **lit** un instantané que la boucle publie une fois par image. Les deux côtés ne se croisent
//! jamais plus longtemps qu'un `lock` sur un `Vec` court.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// Un ordre déposé par la console, consommé par la boucle de jeu.
#[derive(Debug, Clone, PartialEq)]
pub enum Ordre {
    /// Maintenir ou relâcher une touche : `gauche`, `droite`, `saut`, `bas`.
    Touche { nom: String, enfoncee: bool },
    /// Un appui bref : enfoncée puis relâchée sur l'image suivante. C'est ce qu'on veut presque
    /// toujours pour sauter — maintenir `saut` ne rebondit pas, le front montant est unique.
    Appui { nom: String },
    /// Voter : `o` ou `n`.
    Voter { pour: bool },
    /// Écrire une capture d'écran.
    Capture { chemin: String },
    /// Terminer le jeu proprement.
    Quitter,
}

/// L'instantané que la boucle publie pour la console. Volontairement plat et textuel : ce qui se
/// lit dans un terminal se compare aussi dans un test, sans analyseur.
#[derive(Debug, Clone, Default)]
pub struct Etat {
    pub phase: String,
    pub manche: u32,
    pub minuteur: f32,
    pub joueurs: Vec<(String, f32, bool)>, // nom, score, a fini
    pub avatars_distants: usize,
    pub envoyes: u64,
    pub recus: u64,
    pub carte: String,
    pub bouchon: String,
    pub vote: Option<(usize, usize, usize, usize, f32)>, // x, y, pour, seuil, reste
    pub position: (f32, f32),
    pub demonstration: bool,
}

/// Le point de rencontre entre la console et la boucle de jeu.
#[derive(Default)]
pub struct Pupitre {
    ordres: Mutex<VecDeque<Ordre>>,
    etat: Mutex<Etat>,
}

impl Pupitre {
    /// Appelé par la boucle de jeu : récupère tout ce qui a été demandé depuis la dernière image.
    pub fn prendre_les_ordres(&self) -> Vec<Ordre> {
        self.ordres.lock().map(|mut f| f.drain(..).collect()).unwrap_or_default()
    }

    /// Appelé par la boucle de jeu, une fois par image.
    pub fn publier(&self, etat: Etat) {
        if let Ok(mut e) = self.etat.lock() {
            *e = etat;
        }
    }

    fn lire_etat(&self) -> Etat {
        self.etat.lock().map(|e| e.clone()).unwrap_or_default()
    }

    fn deposer(&self, o: Ordre) {
        if let Ok(mut f) = self.ordres.lock() {
            // Borne de sûreté : un client qui déverserait sans fin ne doit pas faire enfler la
            // mémoire d'un jeu qui, lui, ne consomme qu'une fois par image.
            if f.len() < 512 {
                f.push_back(o);
            }
        }
    }
}

/// Ouvre la console si `AEGIS_CONSOLE` est défini. Renvoie le pupitre partagé.
///
/// La valeur est le port ; `AEGIS_CONSOLE=1` prend le port par défaut 47820, choisi juste après le
/// sidecar (47800) et le contrôle (47810) pour que les trois se lisent ensemble dans un `ss`.
pub fn ouvrir() -> Arc<Pupitre> {
    let pupitre = Arc::new(Pupitre::default());
    let Ok(v) = std::env::var("AEGIS_CONSOLE") else {
        return pupitre; // absente : le jeu se comporte exactement comme avant
    };
    let port: u16 = match v.trim() {
        "1" | "" => 47820,
        autre => autre.parse().unwrap_or(47820),
    };

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            // Échec NON FATAL, et annoncé : une console qui n'a pas pu s'ouvrir en silence ferait
            // croire à un jeu muet alors qu'il joue très bien.
            println!("[console] impossible d'écouter sur 127.0.0.1:{port} ({e}) — jeu sans console.");
            return pupitre;
        }
    };
    println!("[console] à l'écoute sur 127.0.0.1:{port}");

    let partage = Arc::clone(&pupitre);
    std::thread::spawn(move || {
        for flux in listener.incoming().flatten() {
            let p = Arc::clone(&partage);
            // Un fil par client : une session qui réfléchit ne bloque pas les autres.
            std::thread::spawn(move || servir(flux, p));
        }
    });
    pupitre
}

fn servir(flux: TcpStream, pupitre: Arc<Pupitre>) {
    let lecture = match flux.try_clone() {
        Ok(l) => l,
        Err(_) => return,
    };
    let mut sortie = flux;
    let _ = writeln!(sortie, "aegis console — 'aide' pour la liste");
    for ligne in BufReader::new(lecture).lines().map_while(Result::ok) {
        let reponse = repondre(ligne.trim(), &pupitre);
        if writeln!(sortie, "{reponse}").is_err() {
            return;
        }
        let _ = sortie.flush();
        if ligne.trim() == "quitter" {
            return;
        }
    }
}

/// Traduit une ligne en réponse. Séparée du réseau pour être **testable sans socket**.
pub fn repondre(ligne: &str, pupitre: &Pupitre) -> String {
    let mots: Vec<&str> = ligne.split_whitespace().collect();
    match mots.as_slice() {
        [] => String::new(),

        ["aide"] => [
            "etat                — phase, minuteur, scores, pont, verdict TAS, vote",
            "joueurs             — un par ligne : nom score fini",
            "touche <n> <on|off> — gauche droite saut bas",
            "appui <n>           — enfoncee puis relachee (le saut veut un FRONT)",
            "voter <o|n>         — bulletin du joueur local",
            "capture <fichier>   — ecrit une capture d'ecran",
            "quitter             — ferme la session (le jeu continue)",
            "arret               — arrete le JEU proprement",
        ]
        .join("\n"),

        ["etat"] => {
            let e = pupitre.lire_etat();
            let vote = match e.vote {
                Some((x, y, pour, seuil, reste)) => {
                    format!("vote=({x},{y}) {pour}/{seuil} reste={reste:.0}s")
                }
                None => "vote=aucun".to_string(),
            };
            format!(
                "phase={} manche={} minuteur={:.1} joueurs={} distants={} envoyes={} recus={} \
                 carte={} bouchon={} {vote} pos=({:.2},{:.2}) demo={}",
                e.phase, e.manche, e.minuteur, e.joueurs.len(), e.avatars_distants,
                e.envoyes, e.recus, e.carte, e.bouchon, e.position.0, e.position.1, e.demonstration
            )
        }

        ["joueurs"] => {
            let e = pupitre.lire_etat();
            if e.joueurs.is_empty() {
                return "(aucun)".to_string();
            }
            e.joueurs
                .iter()
                .map(|(n, s, f)| format!("{n} {s:.1} {}", if *f { "fini" } else { "-" }))
                .collect::<Vec<_>>()
                .join("\n")
        }

        ["touche", nom, etat] if matches!(*etat, "on" | "off") => {
            pupitre.deposer(Ordre::Touche { nom: nom.to_string(), enfoncee: *etat == "on" });
            format!("ok touche {nom} {etat}")
        }

        ["appui", nom] => {
            pupitre.deposer(Ordre::Appui { nom: nom.to_string() });
            format!("ok appui {nom}")
        }

        ["voter", b] if matches!(*b, "o" | "n") => {
            pupitre.deposer(Ordre::Voter { pour: *b == "o" });
            format!("ok voter {b}")
        }

        ["capture", chemin] => {
            pupitre.deposer(Ordre::Capture { chemin: chemin.to_string() });
            format!("ok capture {chemin}")
        }

        ["arret"] => {
            pupitre.deposer(Ordre::Quitter);
            "ok arret".to_string()
        }

        ["quitter"] => "au revoir".to_string(),

        // ⚠ On répond « inconnu » plutôt que de rester muet. Un silence se confond avec un jeu
        // figé, et ferait chercher un blocage là où il n'y a qu'une faute de frappe.
        _ => format!("inconnu : {ligne} (essayez 'aide')"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_commande_inconnue_repond_au_lieu_de_se_taire() {
        let p = Pupitre::default();
        assert!(repondre("kjhkjh", &p).starts_with("inconnu"));
        assert_eq!(repondre("", &p), "");
    }

    #[test]
    fn les_ordres_sont_deposes_puis_pris_une_seule_fois() {
        let p = Pupitre::default();
        repondre("touche droite on", &p);
        repondre("appui saut", &p);
        let pris = p.prendre_les_ordres();
        assert_eq!(pris.len(), 2);
        assert_eq!(pris[0], Ordre::Touche { nom: "droite".into(), enfoncee: true });
        assert_eq!(pris[1], Ordre::Appui { nom: "saut".into() });
        assert!(p.prendre_les_ordres().is_empty(), "une file videe reste vide");
    }

    #[test]
    fn une_touche_mal_formee_ne_depose_rien() {
        let p = Pupitre::default();
        assert!(repondre("touche droite peut-etre", &p).starts_with("inconnu"));
        assert!(p.prendre_les_ordres().is_empty());
    }

    /// Un client qui déverse sans fin ne doit pas faire enfler la mémoire du jeu.
    #[test]
    fn la_file_d_ordres_est_bornee() {
        let p = Pupitre::default();
        for _ in 0..2000 {
            repondre("appui saut", &p);
        }
        assert!(p.prendre_les_ordres().len() <= 512);
    }

    #[test]
    fn l_etat_publie_par_la_boucle_est_celui_qu_on_lit() {
        let p = Pupitre::default();
        p.publier(Etat {
            phase: "Running".into(),
            manche: 3,
            avatars_distants: 2,
            carte: "Franchissable".into(),
            ..Default::default()
        });
        let r = repondre("etat", &p);
        assert!(r.contains("phase=Running"), "{r}");
        assert!(r.contains("manche=3"), "{r}");
        assert!(r.contains("distants=2"), "{r}");
    }

    #[test]
    fn le_vote_se_lit_dans_l_etat() {
        let p = Pupitre::default();
        p.publier(Etat { vote: Some((12, 4, 8, 24, 9.0)), ..Default::default() });
        assert!(repondre("etat", &p).contains("vote=(12,4) 8/24"));
        p.publier(Etat::default());
        assert!(repondre("etat", &p).contains("vote=aucun"));
    }
}
