//! sidecar_client.rs — le pont vers le cœur réseau web3.
//!
//! Le cœur Rust (`jeu sidecar`) est SERVEUR sur `127.0.0.1:47800` ; les jeux sont CLIENTS. Ce
//! module est notre côté du contrat (`prive/CONTRAT_SIDECAR.md` du dépôt web3game, et
//! `src/net/sidecar.rs` qui en est l'implémentation de référence). Comme le pont vers le
//! launcher, il ne dépend de **rien** d'autre que la bibliothèque standard.
//!
//! # Le contrat en une phrase
//!
//! On pousse SA position, on lit celle des autres. **Rien de plus** : le jeu ne fait pas de
//! réseau, il ne connaît ni pairs, ni NAT, ni signatures. Le cœur garde toute l'autorité — il ne
//! nous transmet que des poses déjà validées, et nous ne pouvons injecter aucun avatar.
//!
//! # Le partage des rôles, et pourquoi il est dans ce sens
//!
//! Le cœur tourne **en continu**, que le jeu soit lancé ou non : c'est lui le nœud du réseau, il
//! ne doit pas s'éteindre quand une fenêtre 3D se ferme. Le jeu n'est qu'une session qui
//! s'attache et se détache.
//!
//! ⚠ RÈGLE DE CONCEPTION, la même que pour le launcher et pour la même raison : **l'absence du
//! cœur n'est pas une erreur.** Lancé seul, le jeu doit tourner exactement pareil — solo, sans
//! le moindre ralentissement. Toute méthode d'ici est sans effet quand la connexion n'a pas pu
//! se faire : jamais un `unwrap`, jamais un blocage.
//!
//! ⚠ Et l'échec est JOURNALISÉ, jamais avalé. Un `let _ =` sur une socket est précisément
//! l'endroit où un mécanisme meurt sans témoin — ce dépôt en a trouvé trois le même jour. On
//! imprime les trois issues : relié / pas de cœur / la liaison est tombée.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// L'adresse du cœur. En dur des deux côtés : ce n'est pas un réglage, c'est la définition du
/// rendez-vous local. (`SIDECAR_ADDR` la déplace, des deux côtés, pour les bancs d'essai.)
const ADRESSE: &str = "127.0.0.1:47800";

/// Version du contrat parlée par ce jeu, envoyée dans le `HELLO`.
const PROTO: u16 = 1;

// jeu -> cœur (types < 128)
const HELLO: u8 = 1;
const PUSH_SELF: u8 = 2;
// cœur -> jeu (types >= 128)
const WELCOME: u8 = 128;
const SNAPSHOT: u8 = 129;

/// Taille d'un avatar dans un `SNAPSHOT` : 32 octets d'identité + 11 `f32`.
///
/// ⚠ Ce nombre est écrit en dur **des deux côtés** (`AVATAR_REC` dans `src/net/sidecar.rs`).
/// Les deux doivent bouger ensemble : sinon tous les avatars se décalent d'un cran, et rien ne
/// le dit — l'écran montre des joueurs aux mauvais endroits, pas une erreur.
const AVATAR_REC: usize = 32 + 11 * 4;

/// Cadence maximale d'émission de notre pose. Le contrat plafonne à ~60 Hz ; on s'en tient à 30,
/// qui suffit largement au cœur (il ré-émet à 20 Hz) et laisse la boucle de rendu tranquille.
const ENVOI_HZ: f32 = 30.0;

/// Délai d'établissement. Le cœur écoute en local : s'il est là, il répond tout de suite. Ce
/// délai ne borne que le cas « personne n'écoute », et ne doit jamais retarder le démarrage.
const DELAI_CONNEXION: Duration = Duration::from_millis(300);

/// Un joueur distant, tel que le cœur nous le donne. Aucune notion de réseau ne transparaît ici :
/// c'est déjà une pose validée, prête à afficher.
#[derive(Clone, Copy, Debug)]
pub struct Avatar {
    /// Clé publique du joueur = son identité stable. On en dérive un nom court à l'écran.
    pub id: [u8; 32],
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Vitesse réelle, en unités par seconde. **À utiliser pour interpoler** : les instantanés
    /// arrivent à 20 Hz, et le ressenti du mouvement se fabrique ici, pas sur le réseau.
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// Ce que le fil lecteur publie et que la boucle de rendu consulte.
struct Partage {
    /// Les avatars du dernier instantané, **et l'instant où il est arrivé**.
    ///
    /// L'horodatage n'est pas un détail : les instantanés arrivent à 20 Hz alors que le jeu
    /// dessine bien plus souvent. Sans lui, chaque avatar resterait figé puis sauterait d'un
    /// coup — le contrat est explicite là-dessus, *le ressenti du mouvement se fabrique ici*,
    /// pas sur le réseau.
    avatars: Mutex<(Vec<Avatar>, Instant)>,
    /// Notre propre identité, telle que le cœur nous l'annonce au `WELCOME`.
    ///
    /// La couleur de skin arrive dans le même message ; elle n'est pas retenue ici parce que
    /// **rien ne s'en sert encore** — elle servira quand les avatars distants s'afficheront. Un
    /// champ gardé « pour plus tard » est un champ mort, et ce dépôt en a trouvé trois le même
    /// jour. Elle est journalisée à la réception : observable, sans être stockée.
    moi: Mutex<Option<[u8; 32]>>,
    /// Notre pseudonyme, tel que le cœur le diffuse sur le réseau. Vide tant que le WELCOME
    /// n'est pas arrivé, ou si le cœur est plus ancien que ce pont.
    mon_nom: Mutex<String>,
    /// Combien de poses **nous** avons poussées, et combien d'instantanés **nous** avons reçus.
    ///
    /// Ces deux compteurs ne sont pas de la décoration : ils sont la seule façon de distinguer
    /// « le pont fonctionne » de « le pont est branché mais rien ne passe ». Un seul des deux
    /// qui monte désigne immédiatement le sens en panne.
    envoyes: AtomicU64,
    recus: AtomicU64,
}

/// Notre côté du pont. Inerte et sans effet quand le cœur n'est pas là.
pub struct SidecarClient {
    /// La socket d'écriture. `None` = pas de cœur : toutes les méthodes deviennent des no-op.
    ecrit: Option<TcpStream>,
    partage: Arc<Partage>,
    dernier_envoi: Instant,
}

impl SidecarClient {
    /// Se connecte au cœur et s'annonce. Renvoie **toujours** un client utilisable : s'il n'y a
    /// pas de cœur, il est simplement inerte et le jeu continue en solo.
    pub fn connecter() -> Self {
        let adresse = std::env::var("SIDECAR_ADDR").unwrap_or_else(|_| ADRESSE.to_string());
        Self::connecter_a(&adresse)
    }

    /// Le vrai corps, avec l'adresse en paramètre. Il existe pour que les tests parlent à un cœur
    /// fictif sur un port éphémère : deux tests qui se disputeraient le port 47800 s'échoueraient
    /// l'un l'autre au hasard, et un test instable est pire qu'un test absent.
    fn connecter_a(adresse: &str) -> Self {
        let partage = Arc::new(Partage {
            avatars: Mutex::new((Vec::new(), Instant::now())),
            moi: Mutex::new(None),
            mon_nom: Mutex::new(String::new()),
            envoyes: AtomicU64::new(0),
            recus: AtomicU64::new(0),
        });

        let cible = match adresse.parse() {
            Ok(a) => a,
            Err(_) => return Self::inerte(partage),
        };
        let flux = match TcpStream::connect_timeout(&cible, DELAI_CONNEXION) {
            Ok(f) => f,
            Err(e) => {
                // ISSUE 1 — pas de cœur. Ce n'est PAS une panne : c'est le cas « jeu lancé seul ».
                println!("[sidecar] aucun cœur web3 sur {adresse} ({e}) — on joue en solo.");
                return Self::inerte(partage);
            }
        };
        let _ = flux.set_nodelay(true);

        let lecture = match flux.try_clone() {
            Ok(l) => l,
            Err(e) => {
                println!("[sidecar] socket non clonable ({e}) — on joue en solo.");
                return Self::inerte(partage);
            }
        };
        {
            let partage = Arc::clone(&partage);
            std::thread::spawn(move || ecouter_le_coeur(lecture, partage));
        }

        let mut client = Self {
            ecrit: Some(flux),
            partage,
            // Antidaté d'une période : la première pose part sans attendre.
            dernier_envoi: Instant::now() - Duration::from_secs(1),
        };

        // ⚠ ON N'ANNONCE PAS « relié ». `envoyer` ne prouve que l'ÉCRITURE TCP : elle réussit
        // même si le cœur rejette la trame et raccroche aussitôt. Le message qui vivait ici
        // affirmait « relié au cœur web3 » sur cette seule base — il aurait donc dit « relié » à
        // un joueur qui ne l'était pas, et c'est le genre de message qui fait chercher un défaut
        // ailleurs pendant des heures.
        //
        // La SEULE preuve d'acceptation est le WELCOME que le cœur renvoie ; il est journalisé
        // par `ecouter_le_coeur`. On dit ce qu'on a FAIT, pas ce qu'on espère.
        //
        // (Le 19 août, un jeton de contrôle a été ajouté ici par erreur : le port 47800 — le pont
        // du jeu — ne l'exige pas. Seul 47810, le canal de contrôle du LAUNCHER, le demande. Les
        // deux sockets portent des noms proches dans les journaux ; les confondre coûte cher.)
        if client.envoyer(HELLO, &PROTO.to_le_bytes()) {
            println!("[sidecar] HELLO envoyé à {adresse} — en attente du WELCOME du cœur.");
        }
        client
    }

    fn inerte(partage: Arc<Partage>) -> Self {
        Self { ecrit: None, partage, dernier_envoi: Instant::now() }
    }

    /// Sommes-nous reliés au cœur ?
    pub fn relie(&self) -> bool {
        self.ecrit.is_some()
    }

    /// Pousse notre position. À appeler à chaque image : la cadence est plafonnée ici, pour que
    /// l'appelant n'ait pas à connaître le contrat.
    pub fn pousser_ma_pose(&mut self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32) {
        if self.ecrit.is_none() {
            return;
        }
        if self.dernier_envoi.elapsed().as_secs_f32() < 1.0 / ENVOI_HZ {
            return;
        }
        self.dernier_envoi = Instant::now();

        let mut charge = Vec::with_capacity(20);
        for v in [x, y, z, yaw, pitch] {
            charge.extend_from_slice(&v.to_le_bytes());
        }
        if self.envoyer(PUSH_SELF, &charge) {
            self.partage.envoyes.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Les joueurs distants, **avancés jusqu'à maintenant** avec la vitesse que porte chacun.
    ///
    /// C'est l'extrapolation que demande le contrat : entre deux instantanés (50 ms), un avatar
    /// continue sur sa lancée plutôt que d'attendre, immobile, la prochaine nouvelle. On borne
    /// l'avance : au-delà, un joueur dont on n'a plus de nouvelles partirait tout droit à
    /// l'infini, ce qui est pire qu'un avatar arrêté.
    pub fn avatars(&self) -> Vec<Avatar> {
        /// Au-delà, on cesse d'inventer : mieux vaut un avatar figé qu'un avatar parti au loin.
        const AVANCE_MAX: f32 = 0.25;

        let Ok(garde) = self.partage.avatars.lock() else {
            return Vec::new();
        };
        let (liste, recu_a) = &*garde;
        let dt = recu_a.elapsed().as_secs_f32().min(AVANCE_MAX);

        liste
            .iter()
            .map(|a| Avatar {
                x: a.x + a.vx * dt,
                y: a.y + a.vy * dt,
                z: a.z + a.vz * dt,
                ..*a
            })
            .collect()
    }

    /// `(poses envoyées, instantanés reçus, avatars au dernier instantané)`.
    ///
    /// Le témoin du pont : c'est ce triplet qu'on affiche à l'écran pour prouver que les deux
    /// sens vivent. Un pont branché dont un seul compteur monte est un pont à moitié mort — et
    /// c'est exactement le genre de panne qu'aucun test ne voit.
    pub fn compteurs(&self) -> (u64, u64, usize) {
        (
            self.partage.envoyes.load(Ordering::Relaxed),
            self.partage.recus.load(Ordering::Relaxed),
            self.partage.avatars.lock().map(|a| a.0.len()).unwrap_or(0),
        )
    }

    /// Notre identité vue par le cœur, si le `WELCOME` est arrivé.
    pub fn mon_identite(&self) -> Option<[u8; 32]> {
        self.partage.moi.lock().ok().and_then(|m| *m)
    }

    /// Écrit une trame. Renvoie `true` si elle est partie. Une écriture qui échoue signifie que
    /// le cœur est mort : on repasse en solo plutôt que de réessayer indéfiniment.
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
                // ISSUE 3 — on ÉTAIT relié et on ne l'est plus. La seule des trois qui soit
                // vraiment anormale, et exactement celle qu'un `let _ =` aurait effacée.
                println!("[sidecar] écriture impossible ({e}) — le cœur a disparu, on continue seul.");
                self.ecrit = None;
                false
            }
        }
    }
}

/// Le fil lecteur. Il ne bloque jamais la boucle de rendu : celle-ci ne lit rien du réseau, elle
/// consulte l'état partagé. Une trame inconnue est ignorée sans bruit — le protocole peut
/// grandir côté cœur sans casser un jeu déjà distribué.
fn ecouter_le_coeur(mut flux: TcpStream, partage: Arc<Partage>) {
    let mut entete = [0u8; 4];
    loop {
        if flux.read_exact(&mut entete).is_err() {
            return; // socket fermée : le cœur est parti, on reste en solo
        }
        let longueur = u32::from_le_bytes(entete) as usize;
        // Garde de taille : une longueur aberrante (socket désynchronisée, ou processus local
        // hostile qui aurait pris le port) ne doit jamais nous faire allouer un tampon démesuré.
        if longueur == 0 || longueur > (1 << 20) {
            return;
        }
        let mut corps = vec![0u8; longueur];
        if flux.read_exact(&mut corps).is_err() {
            return;
        }

        match corps[0] {
            WELCOME => {
                if let Some((id, (r, v, b), nom)) = decoder_welcome(&corps[1..]) {
                    let court = nom_court(&id);
                    // On affiche le pseudonyme QUAND IL Y EN A UN, et l'identité courte TOUJOURS.
                    // L'identité est ce qui ne se falsifie pas ; le nom, lui, est un confort — deux
                    // personnes peuvent porter le même, aucune ne peut porter la même clé.
                    if nom.is_empty() {
                        println!(
                            "[sidecar] WELCOME — le cœur nous connaît sous « {court} », \
                             couleur ({r:.2}, {v:.2}, {b:.2})."
                        );
                    } else {
                        println!(
                            "[sidecar] WELCOME — « {nom} » ({court}), couleur ({r:.2}, {v:.2}, {b:.2})."
                        );
                    }
                    if let Ok(mut m) = partage.moi.lock() {
                        *m = Some(id);
                    }
                    if let Ok(mut n) = partage.mon_nom.lock() {
                        *n = nom;
                    }
                }
            }
            SNAPSHOT => {
                let avatars = decoder_snapshot(&corps[1..]);
                if let Ok(mut a) = partage.avatars.lock() {
                    *a = (avatars, Instant::now());
                }
                partage.recus.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// `WELCOME` : 32 octets d'identité, `f32 r, g, b`, puis — depuis le 21 août 2026 — un octet de
/// longueur suivi du PSEUDONYME.
///
/// # Ce que ces 32 octets sont, et ce qu'ils ne sont pas
///
/// C'est notre clé publique, telle que le cœur nous l'annonce. Nous ne la CHOISISSONS pas : la clé
/// privée ne quitte jamais le cœur, et c'est lui qui signe tout ce qui part sur le réseau. Ce jeu
/// peut donc afficher ce qu'il veut à l'écran, il ne peut pas se faire passer pour quelqu'un
/// d'autre auprès des autres joueurs — il n'a pas de quoi signer à leur place.
///
/// ⚠ Le cœur a envoyé 32 ZÉROS ici jusqu'au 21 août 2026. Tout ce code existait déjà et ne recevait
/// rien : le pont était complet d'un seul côté. D'où la garde ci-dessous — une identité nulle n'est
/// pas une identité, et l'accepter en silence ramènerait exactement le même trou sans témoin.
///
/// La lecture reste tolérante sur la LONGUEUR (`>= 44`) : un cœur plus ancien qui n'enverrait pas
/// encore le pseudonyme continue d'être compris, il nous laisse simplement sans nom.
fn decoder_welcome(charge: &[u8]) -> Option<([u8; 32], (f32, f32, f32), String)> {
    if charge.len() < 32 + 12 {
        return None;
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&charge[..32]);
    if id == [0u8; 32] {
        eprintln!(
            "[sidecar] ⚠ le cœur annonce une identité NULLE — on ne sait pas qui l'on est. \
             Le cœur est-il plus ancien que le pont ?"
        );
        return None;
    }
    let f = |i: usize| f32::from_le_bytes(charge[i..i + 4].try_into().unwrap());
    let nom = match charge.get(44) {
        Some(&n) if charge.len() >= 45 + n as usize => {
            String::from_utf8_lossy(&charge[45..45 + n as usize]).to_string()
        }
        _ => String::new(),
    };
    Some((id, (f(32), f(36), f(40)), nom))
}

/// `SNAPSHOT` : `u16 count`, puis `count` enregistrements de 76 octets.
///
/// Une trame tronquée rend ce qu'elle contenait vraiment plutôt que de tout jeter : on préfère
/// afficher les avatars lisibles que faire disparaître tout le monde sur un octet manquant.
fn decoder_snapshot(charge: &[u8]) -> Vec<Avatar> {
    if charge.len() < 2 {
        return Vec::new();
    }
    let compte = u16::from_le_bytes([charge[0], charge[1]]) as usize;
    let mut avatars = Vec::with_capacity(compte.min(1024));

    for i in 0..compte {
        let debut = 2 + i * AVATAR_REC;
        let fin = debut + AVATAR_REC;
        if fin > charge.len() {
            break;
        }
        let rec = &charge[debut..fin];
        let mut id = [0u8; 32];
        id.copy_from_slice(&rec[..32]);
        let f = |n: usize| f32::from_le_bytes(rec[32 + n * 4..36 + n * 4].try_into().unwrap());
        avatars.push(Avatar {
            id,
            x: f(0), y: f(1), z: f(2),
            vx: f(3), vy: f(4), vz: f(5),
            yaw: f(6), pitch: f(7),
            r: f(8), g: f(9), b: f(10),
        });
    }
    avatars
}

/// Le nom court d'une identité : les huit premiers caractères hexadécimaux de la clé publique.
/// Assez pour distinguer les joueurs d'une classe à l'œil, assez court pour tenir dans le HUD.
pub fn nom_court(id: &[u8; 32]) -> String {
    id[..4].iter().map(|o| format!("{o:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Sans cœur à l'écoute, la construction doit RÉUSSIR et rendre un client inerte : c'est le
    /// cas « le jeu est lancé tout seul », qui doit rester parfaitement normal. On vise un port
    /// qu'on vient de fermer : personne n'écoute là, avec certitude.
    #[test]
    fn sans_coeur_le_client_est_inerte_et_le_jeu_continue() {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let mort = ecoute.local_addr().unwrap().to_string();
        drop(ecoute);

        let mut client = SidecarClient::connecter_a(&mort);
        assert!(!client.relie(), "sans coeur, le client doit etre inerte");

        // Et surtout : pousser une pose ne doit ni paniquer, ni bloquer, ni compter quoi que ce soit.
        for _ in 0..100 {
            client.pousser_ma_pose(1.0, 2.0, 3.0, 0.5, 0.1);
        }
        assert_eq!(client.compteurs(), (0, 0, 0));
        assert!(client.avatars().is_empty());
        assert!(client.mon_identite().is_none());
    }

    /// Un instantané se décode exactement comme le cœur l'écrit — 32 octets d'identité puis onze
    /// flottants dans cet ordre. C'est la frontière où une inversion de deux champs ne produit
    /// aucune erreur, seulement des joueurs aux mauvais endroits.
    #[test]
    fn un_instantane_se_decode_champ_pour_champ() {
        let mut charge = Vec::new();
        charge.extend_from_slice(&2u16.to_le_bytes()); // deux avatars
        for n in 0..2u8 {
            charge.extend_from_slice(&[n; 32]);
            for v in 0..11u8 {
                charge.extend_from_slice(&(n as f32 * 100.0 + v as f32).to_le_bytes());
            }
        }

        let a = decoder_snapshot(&charge);
        assert_eq!(a.len(), 2);

        assert_eq!(a[0].id, [0u8; 32]);
        assert_eq!((a[0].x, a[0].y, a[0].z), (0.0, 1.0, 2.0));
        assert_eq!((a[0].vx, a[0].vy, a[0].vz), (3.0, 4.0, 5.0));
        assert_eq!((a[0].yaw, a[0].pitch), (6.0, 7.0));
        assert_eq!((a[0].r, a[0].g, a[0].b), (8.0, 9.0, 10.0));

        assert_eq!(a[1].id, [1u8; 32]);
        assert_eq!((a[1].x, a[1].y, a[1].z), (100.0, 101.0, 102.0));
        assert_eq!(a[1].b, 110.0);
    }

    /// Un instantané tronqué (socket coupée en plein envoi) rend les avatars COMPLETS qu'il
    /// portait, sans paniquer et sans les faire tous disparaître.
    #[test]
    fn un_instantane_tronque_ne_fait_pas_tout_disparaitre() {
        let mut charge = Vec::new();
        charge.extend_from_slice(&3u16.to_le_bytes()); // il en annonce trois...
        charge.extend_from_slice(&[7u8; AVATAR_REC]); // ...il n'en porte qu'un et demi
        charge.extend_from_slice(&[7u8; AVATAR_REC / 2]);

        let a = decoder_snapshot(&charge);
        assert_eq!(a.len(), 1, "le seul avatar entier doit survivre");

        assert!(decoder_snapshot(&[]).is_empty());
        assert!(decoder_snapshot(&[5, 0]).is_empty(), "annonce cinq, n'en porte aucun");
    }

    /// La taille d'un avatar est écrite en dur des deux côtés du pont. Si quelqu'un la change
    /// ici sans la changer là-bas, tous les joueurs se décalent d'un cran **sans aucune erreur**.
    #[test]
    fn la_taille_d_un_avatar_est_celle_du_contrat() {
        assert_eq!(AVATAR_REC, 76, "CONTRAT_SIDECAR.md §3 : 32 octets d'identite + 11 f32");
    }

    #[test]
    fn le_nom_court_tient_en_huit_caracteres() {
        let mut id = [0u8; 32];
        id[0] = 0xDE;
        id[1] = 0xAD;
        id[2] = 0xBE;
        id[3] = 0xEF;
        assert_eq!(nom_court(&id), "DEADBEEF");
    }

    /// **Le témoin du pont** : un vrai échange, de bout en bout, avec un cœur fictif.
    ///
    /// Les autres tests vérifient des décodages — ils passeraient tous avec un client qui ne se
    /// connecte à rien. Celui-ci exige que les octets partent, arrivent, et reviennent : c'est le
    /// seul qui distingue « le code est juste » de « le pont fonctionne ».
    #[test]
    fn un_echange_complet_avec_le_coeur() {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = ecoute.local_addr().unwrap().to_string();

        // Le coeur fictif : il lit le HELLO, repond un WELCOME, puis pousse un SNAPSHOT.
        let coeur = std::thread::spawn(move || {
            let (mut flux, _) = ecoute.accept().unwrap();

            // 1. On doit recevoir le HELLO, avec la version du contrat.
            let mut entete = [0u8; 4];
            flux.read_exact(&mut entete).unwrap();
            let n = u32::from_le_bytes(entete) as usize;
            let mut corps = vec![0u8; n];
            flux.read_exact(&mut corps).unwrap();
            assert_eq!(corps[0], HELLO, "le jeu doit s'annoncer en premier");
            assert_eq!(u16::from_le_bytes([corps[1], corps[2]]), PROTO);

            // 2. WELCOME : notre identite et notre couleur.
            let mut charge = vec![0xABu8; 32];
            for v in [0.2f32, 0.4, 0.6] {
                charge.extend_from_slice(&v.to_le_bytes());
            }
            // Le pseudonyme, ajouté APRÈS les 44 octets d'origine (extension compatible).
            charge.push(5);
            charge.extend_from_slice(b"shaza");
            ecrire(&mut flux, WELCOME, &charge);

            // 3. SNAPSHOT : un joueur distant.
            let mut snap = Vec::new();
            snap.extend_from_slice(&1u16.to_le_bytes());
            snap.extend_from_slice(&[0x5Au8; 32]);
            for v in [11.0f32, 22.0, 33.0, 0.0, 0.0, 0.0, 1.5, 0.0, 1.0, 1.0, 1.0] {
                snap.extend_from_slice(&v.to_le_bytes());
            }
            ecrire(&mut flux, SNAPSHOT, &snap);

            // 4. Et on doit recevoir la pose que le jeu pousse.
            flux.read_exact(&mut entete).unwrap();
            let n = u32::from_le_bytes(entete) as usize;
            let mut corps = vec![0u8; n];
            flux.read_exact(&mut corps).unwrap();
            assert_eq!(corps[0], PUSH_SELF);
            let lire = |i: usize| f32::from_le_bytes(corps[1 + i * 4..5 + i * 4].try_into().unwrap());
            (lire(0), lire(1), lire(2), lire(3), lire(4))
        });

        let mut client = SidecarClient::connecter_a(&adresse);
        assert!(client.relie(), "le client doit etre relie au coeur fictif");

        client.pousser_ma_pose(7.0, 8.0, 9.0, 1.25, -0.5);

        // On attend l'arrivee du WELCOME et du SNAPSHOT plutot que de dormir un delai fixe : un
        // `sleep` arbitraire rend un test soit lent, soit instable selon la machine.
        let debut = Instant::now();
        while debut.elapsed() < Duration::from_secs(5) {
            if client.compteurs().1 > 0 && client.mon_identite().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let pose_recue = coeur.join().expect("le coeur fictif a panique");
        assert_eq!(pose_recue, (7.0, 8.0, 9.0, 1.25, -0.5), "la pose doit arriver telle quelle");

        let (envoyes, recus, combien) = client.compteurs();
        assert_eq!(envoyes, 1, "une pose poussee");
        assert_eq!(recus, 1, "un instantane recu");
        assert_eq!(combien, 1, "un joueur distant");

        assert_eq!(client.mon_identite(), Some([0xABu8; 32]), "le WELCOME nous nomme");
        assert_eq!(
            client.partage.mon_nom(),
            "shaza",
            "le pseudonyme doit suivre l'identité dans le WELCOME"
        );

        let a = client.avatars();
        assert_eq!(a[0].id, [0x5Au8; 32]);
        assert_eq!((a[0].x, a[0].y, a[0].z), (11.0, 22.0, 33.0));
        assert_eq!(a[0].yaw, 1.5);
    }

    /// Ecrit une trame au format du contrat, pour le coeur fictif des tests.
    fn ecrire(flux: &mut TcpStream, ty: u8, charge: &[u8]) {
        let mut trame = Vec::with_capacity(5 + charge.len());
        trame.extend_from_slice(&((1 + charge.len()) as u32).to_le_bytes());
        trame.push(ty);
        trame.extend_from_slice(charge);
        flux.write_all(&trame).unwrap();
        flux.flush().unwrap();
    }
}

impl Partage {
    /// Notre pseudonyme tel que le cœur le diffuse, ou une chaîne vide s'il ne nous l'a pas encore
    /// dit. À afficher À CÔTÉ de l'identité courte, jamais à sa place : c'est l'identité qui ne se
    /// falsifie pas.
    pub fn mon_nom(&self) -> String {
        self.mon_nom.lock().map(|n| n.clone()).unwrap_or_default()
    }
}
