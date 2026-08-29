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

/// Le port de la console quand `AEGIS_CONSOLE=1`. **Pas 47820** : le régisseur du launcher y est.
pub const PORT_PAR_DEFAUT: u16 = 47830;

/// Ce que la console dit en accueillant un client.
///
/// Il ne sert pas à faire joli : c'est ce qui permet à un scénario de vérifier qu'il parle bien à
/// la console du JEU. Sans cette vérification, une connexion réussie vers un tout autre programme
/// occupant le port se lit comme un succès — c'est exactement ce qui est arrivé le 29 août avec le
/// régisseur du launcher, et le scénario a accusé le lobby pendant deux essais.
pub const BONJOUR: &str = "aegis console — 'aide' pour la liste";

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

    // ── LE LOBBY (29 août 2026) ─────────────────────────────────────────────────────────────
    // Ces quatre-là existent parce que le lobby se pilote à la SOURIS, et que la console ne
    // savait qu'appuyer sur des touches. Conséquence mesurée : aucun test ne pouvait entrer dans
    // le lobby, personne ne l'avait jamais vu, et ses trois écrans — complets, dessinés, avec
    // neuf tests de logique au vert — n'avaient jamais été affichés une seule fois.
    /// Ouvrir ou refermer le lobby : exactement ce que fait ÉCHAP sous une vraie main.
    Echap,
    /// Un clic aux coordonnées du HUD : `x ∈ [0, aspect]`, `y ∈ [0, 1]` du HAUT vers le bas.
    ///
    /// Ce repère-là et pas des pixels : il est celui dans lequel le lobby définit ses zones, donc
    /// un scénario reste juste à n'importe quelle taille de fenêtre. En pixels, il ne vaudrait que
    /// pour la résolution du jour où on l'a écrit.
    Clic { x: f32, y: f32 },
    /// Une suite de frappes, pour les champs de saisie (nom de la partie, code d'accès).
    Texte { texte: String },
    /// Retour arrière dans le champ de saisie.
    Effacer,
    /// Repartir de zero pour l'agregation du temps GPU.
    GpuZero,

    // ── LE LABORATOIRE D'AMBIANCE (29 août 2026) ────────────────────────────────────────────
    /// Régler un champ de l'`Ambiance` sans recompiler.
    ///
    /// ⚠ Le nom du champ n'est PAS validé ici, et c'est délibéré : c'est le moteur qui sait quels
    /// champs une ambiance possède (`Ambiance::CHAMPS`). Recopier cette liste dans la console en
    /// ferait une seconde vérité, qui se périmerait au premier champ ajouté — le défaut même que
    /// tout le travail du jour a consisté à fermer ailleurs.
    Ambiance { champ: String, valeurs: Vec<f32> },
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
    /// L'écran de lobby ouvert (`liste`, `creer`, `attente`), ou vide s'il est fermé.
    pub lobby: String,
    /// Les zones cliquables du dernier écran de lobby dessiné : le nom de l'action, puis
    /// `x, y, largeur, hauteur` dans le repère du HUD.
    ///
    /// Publiées plutôt que devinées : c'est ce qui permet à un scénario de dire « clique sur
    /// CRÉER » sans coder un rectangle qui se périmerait au premier ajustement de pixel — et de
    /// CONSTATER, quand la liste ne contient pas ce qu'on attend, qu'un bouton dessiné n'est pas
    /// atteignable.
    pub zones: Vec<(String, f32, f32, f32, f32)>,
    /// L'ambiance courante, ecrite telle qu'elle se recolle dans le code du jeu.
    ///
    /// Publiee en TEXTE et non en valeurs : ce qui se lit dans un terminal se compare aussi dans
    /// un test, et surtout se recolle sans etre retranscrit.
    pub ambiance: String,
    /// Le temps GPU par etape : nom, moyenne, pire cas, nombre d'images agregees.
    ///
    /// ⚠ Contrairement a `travail`, ce releve ne VOYAGE PAS : il decrit ce GPU, ce pilote, ce
    /// jour. Il se lit contre le budget du Quest 2 (13,9 ms), jamais comme une propriete du moteur.
    /// Vide tant qu'aucune image n'est finie, ou si la file ne sait pas horodater.
    ///
    /// La MOYENNE sert a comparer deux versions ; le PIRE CAS dit si le rendu saccade. Sur un
    /// casque, une seule image ratee se ressent — les deux se lisent ensemble.
    pub gpu: Vec<(String, f32, f32, u32)>,
    /// Les images par seconde observees. ⚠ **A lire AVANT le cout** : une cadence effondree
    /// signale une fenetre masquee, et le releve qui l'accompagne surestime alors le cout d'un
    /// facteur 4 environ (mesure le 29 aout : 11 images en 6 s, 0,841 ms au lieu de 0,222 ms).
    pub gpu_cadence: f32,
    /// Le releve de la derniere image seule, pour voir ce qui se passe a l'instant present.
    pub gpu_image: Vec<(String, f32)>,
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
/// La valeur est le port ; `AEGIS_CONSOLE=1` prend le port par défaut **47830**.
///
/// ⚠ **Ce fut 47820, et c'était une collision** (corrigée le 29 août 2026). Le raisonnement
/// d'alors — « juste après le sidecar (47800) et le contrôle (47810), pour que les trois se lisent
/// ensemble dans un `ss` » — était esthétique, et personne n'avait regardé si le port était libre :
/// **le launcher y est SERVEUR** (`launcher/src/nav.rs`, le régisseur de bascule), et c'est par là
/// que le jeu lui parle. Dès que le launcher tourne — le cas normal chez un joueur — la console
/// n'ouvrait donc pas, et qui s'y connectait dialoguait avec le RÉGISSEUR en croyant parler au jeu.
/// Mesuré : `ss -ltnp` donnait `47820 users:(("launcher"))`. On laisse un cran de plus.
pub fn ouvrir() -> Arc<Pupitre> {
    let pupitre = Arc::new(Pupitre::default());
    let Ok(v) = std::env::var("AEGIS_CONSOLE") else {
        return pupitre; // absente : le jeu se comporte exactement comme avant
    };
    let port: u16 = match v.trim() {
        "1" | "" => PORT_PAR_DEFAUT,
        autre => autre.parse().unwrap_or(PORT_PAR_DEFAUT),
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
    let _ = writeln!(sortie, "{BONJOUR}");
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
            "travail             — le banc : dessins et triangles par image",
            "travail zero        — repart de zero pour mesurer une phase",
            "gpu                 — le temps GPU par etape : moyenne/pire, contre le budget Quest 2",
            "gpu image           — le temps GPU de la derniere image seule",
            "gpu zero            — repart de zero pour l agregation GPU",
            "— le laboratoire d'ambiance : regle EN DIRECT, l'oeil tranche —",
            "ambiance            — le reglage courant, pret a recoller dans le code du jeu",
            "ambiance <champ> <valeurs...>",
            "                      ciel <r> <v> <b> | sol <r> <v> <b> | exposition <x>",
            "                      point_blanc <x>  | rugosite <x>    | reflectance <x>",
            "— le lobby —",
            "echap               — ouvre ou referme le lobby",
            "zones               — les boutons atteignables sur l'ecran courant",
            "cliquer <action>    — clique au centre de cette zone (nom donne par 'zones')",
            "clic <x> <y>        — clique a ces coordonnees du HUD (y de haut en bas)",
            "texte <mots>        — tape dans le champ actif",
            "effacer             — retour arriere dans le champ actif",
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
            let lobby = if e.lobby.is_empty() { "fermee" } else { &e.lobby };
            format!(
                "phase={} manche={} minuteur={:.1} joueurs={} distants={} envoyes={} recus={} \
                 carte={} bouchon={} {vote} pos=({:.2},{:.2}) demo={} lobby={lobby} zones={}",
                e.phase, e.manche, e.minuteur, e.joueurs.len(), e.avatars_distants,
                e.envoyes, e.recus, e.carte, e.bouchon, e.position.0, e.position.1,
                e.demonstration, e.zones.len()
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

        // ── LE BANC DU RENDU ────────────────────────────────────────────────────────────────
        // Deux mots seulement : `travail` lit, `travail zero` repart de zéro pour mesurer une
        // phase précise. Le relevé est déterministe — même scène, même compte, sur n'importe
        // quelle machine — ce qui est tout l'intérêt face à des millisecondes qui ne voyagent pas.
        ["travail"] => {
            let t = aegis_engine::mesure::releve();
            format!(
                "images={} dessins={} triangles={} | par image : dessins={:.1} triangles={:.0}",
                t.images, t.dessins, t.triangles,
                t.dessins_par_image(), t.triangles_par_image()
            )
        }

        // Le second banc : le temps EXECUTE, la ou `travail` compte le travail SOUMIS. Les deux
        // sont necessaires et ne se remplacent pas — l'eclairage ne se voit pas du tout dans le
        // nombre d'appels de dessin.
        ["gpu"] => {
            let e = pupitre.lire_etat();
            if e.gpu.is_empty() {
                return "(aucune mesure — pas encore d'image finie, ou file sans horodatage)".to_string();
            }
            let budget = aegis_engine::chrono_gpu::ChronoGpu::BUDGET_QUEST2_MS;
            let moyenne: f32 = e.gpu.iter().map(|(_, m, _, _)| m).sum();
            let pire: f32 = e.gpu.iter().map(|(_, _, p, _)| p).sum();
            let images = e.gpu.iter().map(|(_, _, _, i)| *i).max().unwrap_or(0);
            let detail = e
                .gpu
                .iter()
                .map(|(n, m, p, _)| format!("{n}={m:.3}/{p:.3}"))
                .collect::<Vec<_>>()
                .join(" ");
            // Le compte d'images est affiche EXPRES : une moyenne sur trois images n'est pas une
            // mesure, et c'est invisible si on ne le montre pas.
            // La cadence est dite EN PREMIER, et c'est deliberé : elle decide si le reste de la
            // ligne vaut quelque chose. Une fenetre masquee tombe a quelques images par seconde et
            // fait grossir toutes les durees — le chiffre se croirait sans elle.
            let alerte = if e.gpu_cadence > 0.0 && e.gpu_cadence < 20.0 {
                "  ⚠ CADENCE EFFONDREE — fenetre masquee ? ce releve surestime le cout"
            } else {
                ""
            };
            format!(
                "{:.0} img/s sur {images} images{alerte} | moy/pire par etape : {detail} | total moy={moyenne:.3} ms ({:.1}% du budget Quest 2 de {budget:.1} ms), pire={pire:.3} ms",
                e.gpu_cadence,
                moyenne / budget * 100.0
            )
        }

        ["gpu", "image"] => {
            let e = pupitre.lire_etat();
            if e.gpu_image.is_empty() {
                return "(aucune image finie)".to_string();
            }
            let total: f32 = e.gpu_image.iter().map(|(_, ms)| ms).sum();
            let detail = e
                .gpu_image
                .iter()
                .map(|(n, ms)| format!("{n}={ms:.3}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("derniere image : {detail} | total={total:.3} ms")
        }

        // ── LE LABORATOIRE D'AMBIANCE ───────────────────────────────────────────────────────
        //
        // Sans arguments il MONTRE, avec arguments il REGLE. Une seule porte pour tous les
        // reglages : ajouter un champ a `Ambiance` ne demandera aucune commande de plus.
        ["ambiance"] => {
            let e = pupitre.lire_etat();
            if e.ambiance.is_empty() {
                return "(aucune image dessinee — l'ambiance n'est pas encore publiee)".to_string();
            }
            e.ambiance
        }

        ["ambiance", champ, valeurs @ ..] => {
            let mut nombres = Vec::with_capacity(valeurs.len());
            for v in valeurs {
                match v.parse::<f32>() {
                    Ok(n) => nombres.push(n),
                    // ⚠ Dire QUEL mot est fautif, pas seulement qu'il y en a un. Une virgule
                    // decimale tapee a la francaise ("0,2") est l'erreur la plus probable ici, et
                    // un message vague la ferait chercher longtemps.
                    Err(_) => return format!("« {v} » n'est pas un nombre (le separateur est le POINT)"),
                }
            }
            if nombres.is_empty() {
                return format!("ambiance {champ} <valeurs...> — aucune valeur donnee");
            }
            pupitre.deposer(Ordre::Ambiance {
                champ: (*champ).to_string(),
                valeurs: nombres,
            });
            // ⚠ On ne peut pas dire « ok » : l'ordre n'est joue qu'a l'image suivante, et le
            // moteur peut le refuser (champ inconnu, valeur absurde). Annoncer un succes qu'on
            // n'a pas constate serait une victoire prematuree de trois mots.
            "ordre transmis — relire avec « ambiance » pour voir ce qui a ete retenu".to_string()
        }

        ["gpu", "zero"] => {
            pupitre.deposer(Ordre::GpuZero);
            "ok gpu zero".to_string()
        }

        ["travail", "zero"] => {
            aegis_engine::mesure::remettre_a_zero();
            "ok travail zero".to_string()
        }

        ["echap"] => {
            pupitre.deposer(Ordre::Echap);
            "ok echap".to_string()
        }

        ["zones"] => {
            let e = pupitre.lire_etat();
            if e.zones.is_empty() {
                return "(aucune — lobby ferme, ou aucune image encore dessinee)".to_string();
            }
            e.zones
                .iter()
                .map(|(n, x, y, w, h)| format!("{n} {x:.3} {y:.3} {w:.3} {h:.3}"))
                .collect::<Vec<_>>()
                .join("\n")
        }

        ["cliquer", quoi] => {
            let e = pupitre.lire_etat();
            let cible = quoi.to_ascii_lowercase();
            match e.zones.iter().find(|(nom, ..)| nom.to_ascii_lowercase() == cible) {
                Some((nom, x, y, w, h)) => {
                    // Le CENTRE, jamais un bord : viser un bord éprouverait l'arrondi du calcul
                    // de rectangle, pas l'intention du scénario.
                    pupitre.deposer(Ordre::Clic { x: x + w / 2.0, y: y + h / 2.0 });
                    format!("ok cliquer {nom}")
                }
                // ⚠ UN ÉCHEC ANNONCÉ, JAMAIS AVALÉ. Sans ce message, un scénario qui vise un
                // bouton disparu cliquerait dans le vide et « réussirait » en ne faisant rien :
                // c'est exactement ainsi qu'un mécanisme meurt sans témoin. On dit donc ce qui
                // est réellement à l'écran, pour que la faute se lise sans relancer le jeu.
                None => {
                    let dispo: Vec<&str> = e.zones.iter().map(|(n, ..)| n.as_str()).collect();
                    let vues =
                        if dispo.is_empty() { "aucune".to_string() } else { dispo.join(" ") };
                    format!("absent de l'ecran : {quoi} (a l'ecran : {vues})")
                }
            }
        }

        ["clic", x, y] => match (x.parse::<f32>(), y.parse::<f32>()) {
            (Ok(x), Ok(y)) => {
                pupitre.deposer(Ordre::Clic { x, y });
                format!("ok clic {x} {y}")
            }
            _ => format!("inconnu : clic veut deux nombres, recu '{x} {y}'"),
        },

        // La ligne BRUTE après le mot, pas les mots recollés : un nom de partie a le droit de
        // porter deux espaces, et un scénario doit pouvoir taper exactement ce qu'un humain
        // taperait — sinon on teste une saisie qui n'est pas la sienne. Les bords, eux, sont
        // rognés : ils viennent du transport (une ligne réseau finit par un retour), jamais de
        // l'intention de qui tape.
        ["texte", ..] => {
            let texte = ligne["texte".len()..].trim().to_string();
            if texte.is_empty() {
                return "inconnu : texte veut quelque chose a taper".to_string();
            }
            pupitre.deposer(Ordre::Texte { texte: texte.clone() });
            format!("ok texte {texte}")
        }

        ["effacer"] => {
            pupitre.deposer(Ordre::Effacer);
            "ok effacer".to_string()
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

    /// Une zone d'essai : « CRÉER » occupe le quart supérieur gauche.
    fn etat_avec_zones() -> Etat {
        Etat {
            lobby: "liste".into(),
            zones: vec![
                ("CreerLaMienne".into(), 0.10, 0.20, 0.20, 0.10),
                ("Retour".into(), 0.02, 0.90, 0.20, 0.05),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn cliquer_vise_le_centre_de_la_zone_publiee() {
        let p = Pupitre::default();
        p.publier(etat_avec_zones());
        assert_eq!(repondre("cliquer CreerLaMienne", &p), "ok cliquer CreerLaMienne");
        let pris = p.prendre_les_ordres();
        // centre = (0.10 + 0.20/2, 0.20 + 0.10/2)
        assert_eq!(pris, vec![Ordre::Clic { x: 0.20, y: 0.25 }]);
    }

    #[test]
    fn le_nom_de_zone_est_insensible_a_la_casse() {
        let p = Pupitre::default();
        p.publier(etat_avec_zones());
        assert_eq!(repondre("cliquer creerlamienne", &p), "ok cliquer CreerLaMienne");
        assert_eq!(p.prendre_les_ordres().len(), 1);
    }

    /// ⚠ LE TEST QUI COMPTE LE PLUS ICI. Un bouton absent de l'écran doit faire ÉCHOUER le
    /// scénario bruyamment. S'il déposait un clic « quelque part », le scénario cliquerait dans
    /// le vide, ne ferait rien, et passerait quand même — un mécanisme mort sans témoin.
    #[test]
    fn cliquer_une_zone_absente_ne_depose_rien_et_dit_ce_qui_est_a_l_ecran() {
        let p = Pupitre::default();
        p.publier(etat_avec_zones());
        let r = repondre("cliquer Lancer", &p);
        assert!(r.starts_with("absent de l'ecran"), "{r}");
        assert!(r.contains("CreerLaMienne"), "doit dire ce qui EST la : {r}");
        assert!(p.prendre_les_ordres().is_empty(), "aucun clic ne doit partir");
    }

    #[test]
    fn sans_aucune_zone_le_message_le_dit_au_lieu_de_mentir() {
        let p = Pupitre::default();
        let r = repondre("cliquer Retour", &p);
        assert!(r.contains("a l'ecran : aucune"), "{r}");
        assert!(p.prendre_les_ordres().is_empty());
        assert!(repondre("zones", &p).starts_with("(aucune"));
    }

    #[test]
    fn le_texte_garde_les_espaces_tels_qu_ils_sont_tapes() {
        let p = Pupitre::default();
        repondre("texte  la  partie de shaza ", &p);
        assert_eq!(
            p.prendre_les_ordres(),
            vec![Ordre::Texte { texte: "la  partie de shaza".into() }]
        );
        assert!(repondre("texte", &p).starts_with("inconnu"));
        assert!(p.prendre_les_ordres().is_empty());
    }

    #[test]
    fn echap_et_effacer_se_deposent() {
        let p = Pupitre::default();
        repondre("echap", &p);
        repondre("effacer", &p);
        assert_eq!(p.prendre_les_ordres(), vec![Ordre::Echap, Ordre::Effacer]);
    }

    #[test]
    fn un_clic_mal_forme_ne_depose_rien() {
        let p = Pupitre::default();
        assert!(repondre("clic gauche 0.5", &p).starts_with("inconnu"));
        assert!(p.prendre_les_ordres().is_empty());
        repondre("clic 0.5 0.25", &p);
        assert_eq!(p.prendre_les_ordres(), vec![Ordre::Clic { x: 0.5, y: 0.25 }]);
    }

    #[test]
    fn l_etat_dit_quel_ecran_de_lobby_est_ouvert() {
        let p = Pupitre::default();
        assert!(repondre("etat", &p).contains("lobby=fermee"), "ferme par defaut");
        p.publier(etat_avec_zones());
        let r = repondre("etat", &p);
        assert!(r.contains("lobby=liste"), "{r}");
        assert!(r.contains("zones=2"), "{r}");
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
