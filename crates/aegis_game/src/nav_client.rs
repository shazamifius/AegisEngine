//! nav_client.rs — le pont vers le launcher web3 (le « régisseur » de bascule entre jeux).
//!
//! Le launcher web3 est SERVEUR sur `127.0.0.1:47820` ; les jeux sont CLIENTS. Ce module est notre
//! côté du contrat (`prive/launcher/CONTRAT_NAV.md` du dépôt web3game, et `launcher/src/nav.rs` qui en est
//! l'implémentation de référence). Il ne dépend de RIEN d'autre que la bibliothèque standard : une
//! socket TCP locale, quatre types de messages, aucune bibliothèque tierce.
//!
//! Le contrat en une phrase : on s'ANNONCE (`REGISTER`) dès le démarrage, on dit quand on a affiché
//! une VRAIE image (`READY`), et on écoute l'ordre de partir (`QUIT`). C'est le `READY` qui compte le
//! plus : le régisseur ne tue jamais le jeu précédent avant qu'un nouveau ait prouvé une frame, donc
//! tant qu'on ne l'envoie pas, la bascule reste en suspens sur l'écran de chargement.
//!
//! ⚠ RÈGLE DE CONCEPTION, non négociable : **l'absence du launcher n'est pas une erreur.** Lancé
//! seul (double-clic, `cargo run`, une machine sans web3), le jeu doit tourner exactement pareil.
//! Toute méthode d'ici est donc sans effet quand la connexion n'a pas pu se faire — jamais un
//! `unwrap`, jamais un blocage.
//!
//! ⚠ Et l'échec est JOURNALISÉ, jamais avalé. Un `let _ =` sur une socket est précisément l'endroit
//! où un mécanisme meurt sans témoin : on imprime les trois issues (relié / pas de launcher / erreur
//! d'écriture), pour qu'un lancement muet ne puisse jamais se faire passer pour un lancement réussi.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// L'adresse du régisseur. En dur des DEUX côtés, comme le port du sidecar : ce n'est pas un réglage,
/// c'est la définition du rendez-vous local.
const ADRESSE: &str = "127.0.0.1:47820";

/// Version du contrat nav parlée par ce jeu (le launcher la lit dans le `REGISTER`).
const NAV_PROTO: u16 = 1;

// jeu -> launcher (types < 128)
const REGISTER: u8 = 1;
#[allow(dead_code)] // le portail interne au jeu l'utilisera : « emmène-moi vers tel autre jeu »
const SWITCH_REQUEST: u8 = 2;
const PROGRESS: u8 = 3;
const READY: u8 = 4;
// launcher -> jeu (types >= 128)
const QUIT: u8 = 131;

/// Délai d'établissement. Le launcher écoute en local : s'il est là, il répond immédiatement. Ce
/// délai n'existe donc que pour borner le cas « personne n'écoute » sur une pile réseau lente — il
/// ne doit jamais retarder visiblement le démarrage du jeu.
const DELAI_CONNEXION: Duration = Duration::from_millis(300);

/// Notre côté du pont. Inerte et sans effet quand le launcher n'est pas là.
pub struct NavClient {
    /// La socket d'écriture. `None` = pas de launcher : toutes les méthodes deviennent des no-op.
    ecrit: Option<TcpStream>,
    /// Posé par le fil lecteur quand le launcher demande à ce jeu de partir.
    quitter: Arc<AtomicBool>,
    /// `READY` est un ONE-SHOT : le renvoyer à chaque image inonderait la socket pour rien.
    pret_envoye: bool,
}

impl NavClient {
    /// Se connecte au régisseur et s'annonce sous `game_id` (l'identifiant que le launcher utilise
    /// pour nous lancer — « aegis » pour ce jeu). Renvoie toujours un client utilisable : s'il n'y a
    /// pas de launcher, il est simplement inerte.
    pub fn connecter(game_id: &str) -> Self {
        Self::connecter_a(ADRESSE, game_id)
    }

    /// Le vrai corps, avec l'adresse en paramètre. Il n'existe QUE pour que les tests puissent parler
    /// à un launcher fictif sur un port éphémère : deux tests qui se disputeraient le port 47820
    /// s'échoueraient l'un l'autre au hasard, et un test instable est pire qu'un test absent — il
    /// fait douter des vrais échecs.
    fn connecter_a(adresse: &str, game_id: &str) -> Self {
        let quitter = Arc::new(AtomicBool::new(false));

        let adresse = match adresse.parse() {
            Ok(a) => a,
            Err(_) => return Self::inerte(quitter),
        };
        let flux = match TcpStream::connect_timeout(&adresse, DELAI_CONNEXION) {
            Ok(f) => f,
            Err(e) => {
                // ISSUE 1 — pas de launcher. Ce n'est PAS une panne : c'est le cas « lancé tout seul ».
                println!("[nav] aucun launcher web3 sur {adresse} ({e}) — on joue en autonome.");
                return Self::inerte(quitter);
            }
        };
        let _ = flux.set_nodelay(true);

        // Le fil lecteur : il ne fait qu'attendre QUIT. Il possède sa propre poignée sur la socket,
        // pour que la boucle de rendu n'ait jamais à lire (donc jamais à bloquer sur le réseau).
        let lecture = match flux.try_clone() {
            Ok(l) => l,
            Err(e) => {
                println!("[nav] socket non clonable ({e}) — on joue en autonome.");
                return Self::inerte(quitter);
            }
        };
        {
            let drapeau = Arc::clone(&quitter);
            std::thread::spawn(move || ecouter_le_launcher(lecture, drapeau));
        }

        let mut client = Self { ecrit: Some(flux), quitter, pret_envoye: false };

        // REGISTER : proto:u16 | pid:u32 | hwnd:u64 | game_id:str
        // `hwnd` est un indice de fenêtre propre à Windows ; sous Linux on envoie 0, et le contrat
        // dit alors au launcher de retrouver la fenêtre par le PID. Ce n'est pas un trou : c'est
        // le cas prévu par le protocole.
        let mut charge = Vec::with_capacity(32);
        charge.extend_from_slice(&NAV_PROTO.to_le_bytes());
        charge.extend_from_slice(&std::process::id().to_le_bytes());
        charge.extend_from_slice(&0u64.to_le_bytes());
        ecrire_chaine(&mut charge, game_id);
        if client.envoyer(REGISTER, &charge) {
            // ISSUE 2 — relié pour de vrai. La trace nomme le game_id : c'est elle qu'on ira
            // chercher dans le journal du régisseur pour prouver que les deux côtés se sont parlé.
            println!("[nav] relié au launcher web3 — annoncé « {game_id} » (pid {}).", std::process::id());
        }
        client
    }

    fn inerte(quitter: Arc<AtomicBool>) -> Self {
        Self { ecrit: None, quitter, pret_envoye: false }
    }

    /// Le launcher nous demande-t-il de partir ? À consulter une fois par image ; quand c'est vrai,
    /// le jeu ferme sa fenêtre et sort normalement (le régisseur a un repli dur, mais un jeu qui
    /// quitte proprement rend ses ressources et ne laisse pas de fenêtre fantôme).
    pub fn doit_quitter(&self) -> bool {
        self.quitter.load(Ordering::Relaxed)
    }

    /// Progression du chargement, en pourcents (borné 0..=100). Purement cosmétique : le régisseur
    /// s'en sert pour animer sa barre pendant que le jeu ouvre ses ressources.
    pub fn progression(&mut self, pct: u8) {
        self.envoyer(PROGRESS, &[pct.min(100)]);
    }

    /// **La trame qui compte** : on a affiché une VRAIE image. À appeler une seule fois, APRÈS la
    /// première présentation réussie — jamais à l'initialisation, jamais « parce que ça devrait être
    /// prêt ». Tant qu'elle n'est pas partie, le régisseur garde son rideau baissé et laisse vivre
    /// le jeu précédent : c'est ce qui garantit qu'on ne voit jamais d'écran noir entre deux mondes.
    pub fn pret(&mut self) {
        if self.pret_envoye {
            return;
        }
        self.pret_envoye = true;
        if self.envoyer(READY, &[]) {
            println!("[nav] READY envoyé — première image présentée.");
        }
    }

    /// Demande au régisseur de basculer vers un autre jeu (un portail franchi dans notre monde).
    /// Le launcher lance la cible, attend SON `READY`, puis nous enverra `QUIT`.
    #[allow(dead_code)] // branché le jour où le jeu aura une porte de sortie
    pub fn aller_vers(&mut self, cible: &str) {
        let mut charge = Vec::new();
        ecrire_chaine(&mut charge, cible);
        self.envoyer(SWITCH_REQUEST, &charge);
    }

    /// Écrit une trame. Renvoie `true` si elle est partie. Une écriture qui échoue signifie que le
    /// launcher est mort : on passe en autonome plutôt que de réessayer indéfiniment.
    fn envoyer(&mut self, ty: u8, charge: &[u8]) -> bool {
        let flux = match self.ecrit.as_mut() {
            Some(f) => f,
            None => return false,
        };
        // Trame : [u32 LE longueur = 1 + charge][u8 type][charge]
        let mut trame = Vec::with_capacity(5 + charge.len());
        trame.extend_from_slice(&((1 + charge.len()) as u32).to_le_bytes());
        trame.push(ty);
        trame.extend_from_slice(charge);

        match flux.write_all(&trame).and_then(|_| flux.flush()) {
            Ok(()) => true,
            Err(e) => {
                // ISSUE 3 — on ÉTAIT relié et on ne l'est plus. C'est la seule des trois qui soit
                // vraiment anormale, et c'est exactement celle qu'un `let _ =` aurait effacée.
                println!("[nav] écriture impossible ({e}) — le launcher a disparu, on continue seul.");
                self.ecrit = None;
                false
            }
        }
    }
}

/// Le fil lecteur : il n'attend qu'une chose, l'ordre de partir. Toute autre trame est ignorée sans
/// bruit (le protocole peut grandir côté launcher sans casser un jeu déjà distribué). La fin de la
/// socket termine simplement le fil : un launcher fermé ne doit pas tuer le jeu.
fn ecouter_le_launcher(mut flux: TcpStream, quitter: Arc<AtomicBool>) {
    let mut entete = [0u8; 4];
    loop {
        if flux.read_exact(&mut entete).is_err() {
            return; // socket fermée : le launcher est parti, on reste en autonome
        }
        let longueur = u32::from_le_bytes(entete) as usize;
        // Garde de taille : une longueur aberrante (socket désynchronisée, ou process local hostile
        // qui aurait pris le port) ne doit jamais nous faire allouer un tampon démesuré.
        if longueur == 0 || longueur > (1 << 20) {
            return;
        }
        let mut corps = vec![0u8; longueur];
        if flux.read_exact(&mut corps).is_err() {
            return;
        }
        if corps[0] == QUIT {
            println!("[nav] QUIT reçu du launcher — on ferme proprement.");
            quitter.store(true, Ordering::Relaxed);
            return;
        }
    }
}

/// Chaîne du protocole nav : longueur sur 2 octets (petit-boutiste) puis les octets UTF-8.
fn ecrire_chaine(tampon: &mut Vec<u8>, s: &str) {
    let octets = s.as_bytes();
    tampon.extend_from_slice(&(octets.len() as u16).to_le_bytes());
    tampon.extend_from_slice(octets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// L'encodage d'une chaîne doit être byte-pour-byte celui que `nav.rs::lire_chaine` attend.
    #[test]
    fn une_chaine_sencode_en_longueur_puis_octets() {
        let mut b = Vec::new();
        ecrire_chaine(&mut b, "aegis");
        assert_eq!(b, vec![5, 0, b'a', b'e', b'g', b'i', b's']);
    }

    /// Sans launcher à l'écoute, la construction doit RÉUSSIR et rendre un client inerte : c'est le
    /// cas « le jeu est lancé tout seul », qui doit rester parfaitement normal. On vise un port qu'on
    /// vient de fermer : personne n'écoute là, avec certitude et sans dépendre de l'état de la machine.
    #[test]
    fn sans_launcher_le_client_est_inerte_et_le_jeu_continue() {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let mort = ecoute.local_addr().unwrap().to_string();
        drop(ecoute); // le port redevient libre : plus personne n'écoute

        let mut c = NavClient::connecter_a(&mort, "aegis-test");
        assert!(c.ecrit.is_none());
        assert!(!c.doit_quitter());
        c.pret(); // ne doit rien faire, et surtout pas paniquer
        c.progression(50);
    }

    /// Le vrai contrat, vérifié contre une socket réelle : un launcher fictif doit recevoir un
    /// REGISTER bien formé, puis un READY — et un seul, même si `pret()` est appelé à chaque image.
    #[test]
    fn le_register_et_le_ready_partent_dans_le_bon_format() {
        let ecoute = TcpListener::bind("127.0.0.1:0").expect("port éphémère");
        let adresse = ecoute.local_addr().unwrap().to_string();
        let fil = std::thread::spawn(move || {
            let (mut flux, _) = ecoute.accept().unwrap();
            let mut recu = Vec::new();
            let mut tampon = [0u8; 256];
            // On lit jusqu'à avoir les deux trames : REGISTER (4 + 22 = 26 o) puis READY (4 + 1 = 5 o).
            while recu.len() < 31 {
                match flux.read(&mut tampon) {
                    Ok(0) => break,
                    Ok(n) => recu.extend_from_slice(&tampon[..n]),
                    Err(_) => break,
                }
            }
            recu
        });

        let mut c = NavClient::connecter_a(&adresse, "aegis");
        assert!(c.ecrit.is_some(), "le client aurait dû se relier au launcher fictif");
        c.pret();
        c.pret(); // deuxième appel : doit être sans effet
        drop(c);

        let recu = fil.join().unwrap();
        // Les offsets sont ceux que `nav.rs::gerer_connexion` lit : la charge commence à 4 (longueur)
        // + 1 (type) = 5, et il y cherche `proto|pid|hwnd` sur 14 octets puis la chaîne à `p[14..]`,
        // soit 5 + 14 = 19 dans la trame complète.
        // Trame 1 : longueur = 1 (type) + 2 (proto) + 4 (pid) + 8 (hwnd) + 2 + 5 ("aegis") = 22.
        assert_eq!(u32::from_le_bytes([recu[0], recu[1], recu[2], recu[3]]), 22);
        assert_eq!(recu[4], REGISTER);
        assert_eq!(u16::from_le_bytes([recu[5], recu[6]]), NAV_PROTO);
        assert_eq!(u32::from_le_bytes([recu[7], recu[8], recu[9], recu[10]]), std::process::id());
        assert_eq!(u16::from_le_bytes([recu[19], recu[20]]), 5, "longueur de la chaîne game_id");
        assert_eq!(&recu[21..26], b"aegis");
        // Trame 2 : READY, charge vide → longueur 1. Et RIEN derrière : le second `pret()` n'a pas écrit.
        assert_eq!(u32::from_le_bytes([recu[26], recu[27], recu[28], recu[29]]), 1);
        assert_eq!(recu[30], READY);
        assert_eq!(recu.len(), 31, "un seul READY doit partir, même sur plusieurs appels");
    }
}
