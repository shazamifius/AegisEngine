//! # La planche du lobby — photographier des écrans que personne n'a jamais vus
//!
//! Le 29 août 2026, le constat qui a fait naître ce programme, dans ses mots : *« tout le système
//! de lobby, de création de lobby privé, de code, et même interaction leaderboard : pour l'instant
//! RIEN n'a jamais été testé, vérifié, et surtout j'ai jamais vu de toute ma vie le GUI pour les
//! lobby »*.
//!
//! Ce n'était pas une négligence. Le lobby se pilote **à la souris**, et la console de pilotage ne
//! savait qu'appuyer sur des touches : aucun scénario ne pouvait donc y entrer, et ses trois
//! écrans — complets, dessinés, neuf tests de logique au vert — n'avaient jamais été affichés une
//! seule fois. *Un mécanisme jamais exercé est mort, et rien ne le dit.*
//!
//! ## Ce que ce programme fait, et pourquoi ainsi
//!
//! Il lance le jeu, ouvre le lobby, traverse ses écrans **par les mêmes gestes qu'une main** (le
//! clic passe par les zones, la frappe par le champ de saisie), et photographie chaque étape.
//!
//! ⚠ **Il ne code AUCUNE coordonnée.** Il demande au jeu où sont ses boutons (`cliquer <action>`
//! vise le centre de la zone publiée) : une position écrite à la main se périmerait au premier
//! ajustement de pixel, et le scénario continuerait de « réussir » en cliquant dans le vide.
//! C'est aussi ce qui lui permet de PROUVER qu'un bouton dessiné est atteignable — s'il ne l'est
//! plus, l'étape échoue bruyamment au lieu de ne rien faire.
//!
//! ⚠ **Il ne coche rien qu'il n'ait vu.** Chaque étape vérifie l'écran obtenu avant de continuer,
//! et s'arrête en le disant si ce n'est pas celui attendu.
//!
//! ```text
//! nix-shell shell.nix --run "cargo run --release --example planche_lobby -- /tmp/planche"
//! ```
//!
//! *(Écrit en Rust, comme tout ce dépôt. Deux outils Python traînent encore dans `tools/` — ils
//! contredisent la règle « que du Rust, aucun autre langage » et attendent d'être remplacés.)*

use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Le port de la console du jeu.
///
/// ⚠ **Pas 47820** : le régisseur de bascule du launcher y est SERVEUR. Ce programme s'y est
/// connecté deux fois en croyant parler au jeu, a reçu le silence d'un interlocuteur qui ne dit
/// pas bonjour, et a conclu que les cinq écrans du lobby étaient introuvables. Le lobby n'y était
/// pour rien. *Une connexion réussie ne prouve pas qu'on parle à qui l'on croit.*
const PORT: u16 = 47830;

/// Le temps qu'on accepte d'attendre qu'un écran s'établisse.
///
/// ⚠ **Une borne, pas une temporisation.** La première version dormait 250 ms entre deux gestes et
/// continuait quoi qu'il arrive : les zones publiées étant celles de l'image PRÉCÉDENTE, le clic
/// sur le champ de nom tombait dans un écran pas encore dessiné, la réponse « absent de l'écran »
/// n'était lue par personne, et le nom n'était jamais tapé — après quoi « OUVRIR LE LOBBY »
/// refusait, à juste titre, et j'ai failli accuser le lobby. *On attend la CONDITION, jamais une
/// durée : une durée est un pari sur la vitesse de la machine.*
const PATIENCE_ECRAN: Duration = Duration::from_secs(5);

fn main() {
    let sortie: PathBuf =
        std::env::args().nth(1).unwrap_or_else(|| "planche-lobby".into()).into();
    if let Err(e) = std::fs::create_dir_all(&sortie) {
        eprintln!("impossible de créer {} : {e}", sortie.display());
        std::process::exit(2);
    }

    // ── DEUX FORMATS, À CHAQUE EXÉCUTION ────────────────────────────────────────────────────
    // Sa règle du 29 août : *« peu importe la taille de la fenêtre, il faut que le texte ne parte
    // jamais sur les autres box »*. Une planche prise à UNE largeur ne peut pas en témoigner — et
    // c'est justement la largeur qui faisait basculer le défaut. On photographie donc l'écran d'un
    // joueur (plein écran) ET le cas étroit qu'un compositeur en mosaïque impose, dans deux
    // dossiers distincts, pour que la comparaison soit immédiate.
    let mut code = 0;
    for (nom, plein) in [("plein-ecran", true), ("fenetre", false)] {
        let dossier = sortie.join(nom);
        if let Err(e) = std::fs::create_dir_all(&dossier) {
            eprintln!("impossible de créer {} : {e}", dossier.display());
            std::process::exit(2);
        }
        println!("\n════ {nom} ════");
        let mut jeu = match lancer_le_jeu(plein) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("le jeu n'a pas démarré : {e}");
                std::process::exit(2);
            }
        };
        code |= match Console::attendre(PORT) {
            Ok(mut c) => derouler(&mut c, &dossier),
            Err(e) => {
                eprintln!("la console n'a jamais répondu sur {PORT} : {e}");
                2
            }
        };
        // Le jeu est tué quoi qu'il arrive : un scénario qui laisse une fenêtre Vulkan derrière
        // lui fausse la mesure suivante, et occupe l'écran de quelqu'un.
        let _ = jeu.kill();
        let _ = jeu.wait();
        // Le port doit être rendu avant la passe suivante.
        std::thread::sleep(Duration::from_millis(500));
    }
    std::process::exit(code);
}

/// Ce que la console du jeu répond en accueillant un client.
///
/// Recopié plutôt qu'importé : `aegis_game` est un binaire, pas une bibliothèque, donc un exemple
/// ne peut pas lire sa constante. La duplication est dite ici pour qu'elle se corrige des deux
/// côtés — et le contrôle reste large (un préfixe) afin qu'un changement de formulation ne fasse
/// pas échouer un scénario pour rien.
fn aegis_game_bonjour() -> &'static str {
    "aegis console"
}

fn lancer_le_jeu(plein_ecran: bool) -> std::io::Result<Child> {
    let binaire = std::env::var("AEGIS_BINAIRE")
        .unwrap_or_else(|_| "target/release/aegis_game".to_string());

    // ⚠ ON RECONSTRUIT LE JEU AVANT DE LE PHOTOGRAPHIER, ET CE N'EST PAS UNE PRÉCAUTION DE CONFORT.
    // `cargo run --example` compile l'EXEMPLE, pas le binaire qu'il lance. La planche a donc
    // photographié, en toute confiance, un jeu vieux d'une demi-heure : les corrections de layout
    // étaient dans le source, absentes de l'image, et j'ai conclu qu'elles n'avaient rien changé.
    // C'est le pire genre de panne — l'instrument répond, et il répond faux. Reconstruire ici rend
    // la faute inatteignable au lieu de demander à chacun d'y penser.
    println!("→ compilation du jeu (sinon la planche photographie l'ancien binaire)");
    let build = Command::new("cargo")
        .args(["build", "--release", "--bin", "aegis_game"])
        .stdout(Stdio::null())
        .status()?;
    if !build.success() {
        return Err(std::io::Error::other("la compilation du jeu a échoué"));
    }

    println!("→ lancement de {binaire} avec la console sur {PORT}");
    let mut cmd = Command::new(&binaire);
    cmd.env("AEGIS_CONSOLE", PORT.to_string()).stdout(Stdio::null()).stderr(Stdio::null());
    if plein_ecran {
        // Le format de l'écran d'un joueur. Sans lui, un compositeur en mosaïque impose le sien —
        // le jeu a été rendu en 1256×1356, un portrait que personne ne voit en jouant.
        cmd.env("AEGIS_PLEIN_ECRAN", "1");
    }
    cmd.spawn()
}

/// Le déroulé. Rend le code de sortie : 0 si toutes les étapes ont été VUES.
fn derouler(c: &mut Console, sortie: &PathBuf) -> i32 {
    let mut vues = 0usize;
    let mut ratees: Vec<String> = Vec::new();
    let mut faire = |c: &mut Console, pas: &dyn Fn(&mut Console) -> Result<(), String>| match pas(c)
    {
        Ok(()) => vues += 1,
        Err(e) => ratees.push(e),
    };

    // ── 1. La liste des parties ────────────────────────────────────────────────────────────
    faire(c, &|c| {
        c.dire("echap");
        c.attendre_ecran("liste")?;
        etape(c, sortie, "liste", "1-liste.png")
    });

    // ── 2. L'écran de création ─────────────────────────────────────────────────────────────
    faire(c, &|c| {
        c.cliquer("CreerLaMienne")?;
        c.attendre_ecran("creer")?;
        etape(c, sortie, "creer", "2-creer.png")
    });

    // ── 3. Un nom tapé dans le champ, comme un humain le taperait ──────────────────────────
    faire(c, &|c| {
        c.cliquer("ChampNom")?;
        c.dire("texte LA PARTIE DE SHAZA");
        etape(c, sortie, "creer", "3-creer-nom.png")
    });

    // ── 4. Une partie PRIVÉE : le code d'accès (tiré au sort, pas saisi) ───────────────────
    faire(c, &|c| {
        c.cliquer("BasculeCode")?;
        etape(c, sortie, "creer", "4-creer-code.png")
    });

    // ── 5. On ouvre la partie → la salle d'attente ─────────────────────────────────────────
    faire(c, &|c| {
        c.cliquer("Ouvrir")?;
        c.attendre_ecran("attente")?;
        etape(c, sortie, "attente", "5-attente.png")
    });

    println!("\n── ce qui a été VU ──");
    println!("  {vues} écran(s) photographié(s) dans {}", sortie.display());
    if ratees.is_empty() {
        println!("  aucune étape ratée");
    } else {
        // ⚠ Dit, jamais avalé. Une étape ratée en silence ferait croire à une planche complète.
        println!("  ⚠ {} étape(s) RATÉE(S) :", ratees.len());
        for r in &ratees {
            println!("     - {r}");
        }
    }
    c.dire("arret");
    if ratees.is_empty() { 0 } else { 1 }
}

/// Vérifie qu'on est bien sur l'écran attendu, puis photographie. Ne coche rien qu'elle n'ait vu.
fn etape(c: &mut Console, sortie: &PathBuf, ecran: &str, fichier: &str) -> Result<(), String> {
    let ou = c.ecran();
    if ou != ecran {
        return Err(format!("{fichier} : attendu « {ecran} », obtenu « {ou} »"));
    }
    let chemin = sortie.join(fichier);
    c.dire(&format!("capture {}", chemin.display()));
    // On attend que le fichier EXISTE, pas qu'un délai s'écoule. Une durée fixe est un pari sur
    // la vitesse de la machine : la première version dormait 250 ms et déclarait le cinquième
    // écran manquant alors qu'il s'écrivait juste après. Ici la condition est la bonne — et la
    // borne ne sert qu'à ne pas attendre l'éternité.
    let debut = Instant::now();
    while !chemin.exists() {
        if debut.elapsed() > Duration::from_secs(5) {
            return Err(format!("{fichier} : la capture n'a pas été écrite en 5 s"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let zones = c.dire_tout("zones");
    println!("✓ {ecran} → {fichier}  ({} bouton(s) atteignable(s))", zones.len());
    for z in &zones {
        println!("      {z}");
    }
    Ok(())
}

/// Le lien avec la console du jeu.
struct Console {
    ecriture: TcpStream,
    lecture: BufReader<TcpStream>,
}

impl Console {
    /// Attend que la console ouvre son port. Le jeu doit charger Vulkan et ses modèles d'abord.
    fn attendre(port: u16) -> std::io::Result<Self> {
        let debut = Instant::now();
        loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(flux) => {
                    flux.set_read_timeout(Some(Duration::from_millis(400)))?;
                    let lecture = BufReader::new(flux.try_clone()?);
                    let mut c = Console { ecriture: flux, lecture };
                    // ⚠ ON VÉRIFIE À QUI L'ON PARLE, et ce contrôle vaut son coût : une connexion
                    // qui aboutit dit seulement qu'un programme écoute là, jamais lequel.
                    let bonjour = c.ligne(Duration::from_secs(3));
                    if !bonjour.starts_with(aegis_game_bonjour()) {
                        return Err(std::io::Error::other(format!(
                            "sur {port}, quelqu'un répond « {bonjour} » au lieu de la console du \
                             jeu — un AUTRE programme occupe ce port"
                        )));
                    }
                    println!("→ console du jeu branchée sur {port}");
                    return Ok(c);
                }
                Err(e) if debut.elapsed() > Duration::from_secs(30) => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }

    /// Clique sur un bouton nommé, et **échoue si la console dit qu'il n'y est pas**.
    ///
    /// C'est le point qui manquait : la console répondait déjà « absent de l'ecran », et le
    /// déroulé jetait sa réponse. Un geste raté passait donc inaperçu, et son effet manquant
    /// était imputé deux étapes plus loin à un mécanisme parfaitement sain.
    fn cliquer(&mut self, action: &str) -> Result<(), String> {
        // Les zones publiées datent de l'image précédente : on laisse au bouton le temps
        // d'apparaître avant de conclure qu'il manque.
        let debut = Instant::now();
        loop {
            let r = self.dire(&format!("cliquer {action}"));
            if r.starts_with("ok ") {
                return Ok(());
            }
            if debut.elapsed() > PATIENCE_ECRAN {
                return Err(format!("clic sur « {action} » impossible : {r}"));
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    }

    /// Attend que l'écran demandé soit à l'affichage.
    fn attendre_ecran(&mut self, ecran: &str) -> Result<(), String> {
        let debut = Instant::now();
        loop {
            let ou = self.ecran();
            if ou == ecran {
                return Ok(());
            }
            if debut.elapsed() > PATIENCE_ECRAN {
                return Err(format!("écran « {ecran} » jamais venu (resté sur « {ou} »)"));
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    }

    /// L'écran de lobby affiché, tel que le jeu le publie.
    fn ecran(&mut self) -> String {
        self.dire("etat")
            .split_whitespace()
            .find_map(|m| m.strip_prefix("lobby="))
            .unwrap_or("?")
            .to_string()
    }

    /// Envoie une commande, rend la première ligne de réponse.
    fn dire(&mut self, commande: &str) -> String {
        let _ = writeln!(self.ecriture, "{commande}");
        let _ = self.ecriture.flush();
        self.ligne(Duration::from_secs(3))
    }

    /// Envoie une commande dont la réponse tient sur plusieurs lignes (`zones`).
    ///
    /// Le protocole de la console est en lignes et ne borne pas une réponse : on lit donc jusqu'au
    /// silence. C'est une limite du protocole, pas de ce programme — et elle est dite plutôt que
    /// contournée en devinant un nombre de lignes.
    fn dire_tout(&mut self, commande: &str) -> Vec<String> {
        let _ = writeln!(self.ecriture, "{commande}");
        let _ = self.ecriture.flush();
        let mut out = Vec::new();
        // La première a droit à la patience d'une réponse ; les suivantes au silence court, qui
        // EST le délimiteur — faute de mieux dans un protocole en lignes.
        let mut patience = Duration::from_secs(3);
        loop {
            let l = self.ligne(patience);
            patience = Duration::from_millis(300);
            if l.is_empty() {
                break;
            }
            let dernier = l.starts_with('('); // « (aucune — …) » : une réponse, pas une zone
            out.push(l);
            if dernier {
                break;
            }
        }
        out
    }

    /// Lit une ligne, en RÉESSAYANT jusqu'à `patience`.
    ///
    /// ⚠ Le premier jet rendait la main au premier dépassement de délai, et c'était un défaut
    /// sérieux : la ligne d'accueil de la console arrivait parfois après, restait dans le tampon,
    /// et **décalait toutes les réponses d'un cran**. Le scénario lisait alors « ok echap » là où
    /// il attendait l'état, ne trouvait aucun `lobby=`, et concluait « écran inconnu » — cinq
    /// étapes ratées pour une cause qui n'était pas dans le jeu. Le partiel déjà lu est conservé
    /// entre deux tentatives : le jeter reviendrait à couper une ligne en deux.
    fn ligne(&mut self, patience: Duration) -> String {
        let mut s = String::new();
        let debut = Instant::now();
        loop {
            match self.lecture.read_line(&mut s) {
                Ok(_) => return s.trim_end().to_string(),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    if debut.elapsed() > patience {
                        return s.trim_end().to_string();
                    }
                }
                Err(_) => return s.trim_end().to_string(),
            }
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        let _ = self.ecriture.shutdown(Shutdown::Both);
    }
}
