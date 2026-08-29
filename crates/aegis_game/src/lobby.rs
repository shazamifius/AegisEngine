//! # LE LOBBY — ouvrir une partie, en trouver une, attendre qu'elle commence. DANS LE JEU.
//!
//! ## Pourquoi ici et pas dans le launcher
//!
//! Ça y a été construit d'abord, et c'était une erreur. Ses mots, le 22 août 2026 :
//! *« MAIS SURTOUT JE PARLAIS SURTOUT PAS DU LAUNCHER […] on clique sur le bouton play, tok on est
//! dans le jeu en Aegis, et là un système où on peut voir les lobbys des gens — pas DANS le
//! launcher »*.
//!
//! Le partage est plus net qu'un choix d'écran : **le launcher installe, vérifie et lance ; le jeu
//! joue.** Une salle d'attente est un moment de JEU — on y regarde qui arrive, on décide, on lance.
//! La sortir du jeu, c'est demander de quitter le monde pour attendre d'y entrer.
//!
//! ## Ce qui a été porté, et ce qui a été refait
//!
//! Le **modèle** vient du launcher (bornes, défauts, règles d'admission) : il était déjà éprouvé,
//! et une règle de jeu ne change pas parce qu'on change de dessin. Le **rendu** est entièrement
//! refait avec le `Pinceau` du jeu et sa police 5×7 — rien n'est repris de l'autre interface.
//!
//! ⚠ **La police du jeu ne connaît que les MAJUSCULES ASCII.** Tout caractère inconnu — un accent
//! compris — se dessine en cadre plein, exprès (« se voir, jamais s'effacer », cf. `hud::glyphe`).
//! Les libellés d'ici s'écrivent donc sans accent : « DUREE », pas « DURÉE ». Ce n'est pas une
//! négligence d'orthographe, c'est la contrainte de la fonte, et un test l'épingle.

use crate::hud::{couleurs, largeur_texte, Pinceau};

/// Où l'on est dans le lobby.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VueLobby {
    /// On joue : le lobby ne dessine rien.
    #[default]
    Fermee,
    Liste,
    Creer,
    Attente,
}

/// Un réglage de partie, avec ses bornes et son défaut.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Reglage {
    /// Le numéro que le cœur transporte, et le SEUL lien avec le réseau. Il est stable : changer
    /// l'ordre d'affichage ne doit jamais changer ce que comprend un autre joueur.
    pub numero: u8,
    pub nom: &'static str,
    pub unite: &'static str,
    pub valeur: i32,
    pub defaut: i32,
    pub min: i32,
    pub max: i32,
    pub pas: i32,
}

impl Reglage {
    pub fn modifie(&self) -> bool {
        self.valeur != self.defaut
    }

    /// Déplace la valeur de `n` pas, **en restant dans les bornes**. Le `clamp` vit ici et pas dans
    /// l'écran : sinon chaque endroit qui incrémente devrait y penser, et le jour où l'on ajoutera
    /// la manette ou le clavier, l'un des trois oubliera.
    pub fn bouger(&mut self, n: i32) {
        self.valeur = (self.valeur + n * self.pas).clamp(self.min, self.max);
    }

    pub fn remettre_defaut(&mut self) {
        self.valeur = self.defaut;
    }

    pub fn affichage(&self) -> String {
        if self.unite.is_empty() {
            format!("{}", self.valeur)
        } else {
            format!("{} {}", self.valeur, self.unite)
        }
    }

    /// Le plus large de tous les affichages que ce réglage puisse prendre.
    ///
    /// Sert à figer la taille du texte de la valeur : la calculer sur la valeur COURANTE la ferait
    /// changer sous le doigt en cliquant « + », ce qui se lit comme un défaut d'affichage. On
    /// dimensionne donc une fois pour toutes sur le pire cas — les bornes sont connues, autant s'en
    /// servir. `min` autant que `max` : un malus part vers le bas, et « -300 PTS » est plus large
    /// que « 0 PTS ».
    pub fn affichage_le_plus_large(&self) -> String {
        let candidat = |v: i32| Reglage { valeur: v, ..*self }.affichage();
        let (a, b) = (candidat(self.min), candidat(self.max));
        if a.chars().count() >= b.chars().count() { a } else { b }
    }
}

/// ⚠ Les numéros 1..=5 sont un CONTRAT avec le cœur : ils voyagent tels quels dans l'annonce.
/// Les réordonner ici ne casserait rien ; les renuméroter casserait tout, silencieusement.
pub fn reglages_par_defaut() -> Vec<Reglage> {
    vec![
        Reglage { numero: 1, nom: "DUREE D'UNE COURSE", unite: "S", valeur: 150, defaut: 150, min: 30, max: 600, pas: 15 },
        Reglage { numero: 2, nom: "CHOIX D'UN OBJET", unite: "S", valeur: 20, defaut: 20, min: 5, max: 90, pas: 5 },
        Reglage { numero: 3, nom: "MANCHES", unite: "", valeur: 3, defaut: 3, min: 1, max: 15, pas: 1 },
        Reglage { numero: 4, nom: "POINTS PAR ARRIVEE", unite: "PTS", valeur: 100, defaut: 100, min: 0, max: 1000, pas: 25 },
        Reglage { numero: 5, nom: "MALUS SI PERSONNE N'ARRIVE", unite: "PTS", valeur: -30, defaut: -30, min: -300, max: 0, pas: 10 },
    ]
}

pub const JOUEURS_MIN: u16 = 2;
pub const JOUEURS_MAX: u16 = 35;
pub const JOUEURS_DEFAUT: u16 = 8;
pub const CODE_LEN: usize = 5;
/// Longueur maximale du nom d'une partie — au-delà, il mord sur la colonne des places.
pub const NOM_MAX: usize = 22;

#[derive(Clone, Debug)]
pub struct Creation {
    pub nom: String,
    pub joueurs: u16,
    pub code: Option<String>,
    pub reglages: Vec<Reglage>,
}

impl Default for Creation {
    fn default() -> Self {
        Creation { nom: String::new(), joueurs: JOUEURS_DEFAUT, code: None, reglages: reglages_par_defaut() }
    }
}

impl Creation {
    pub fn touches(&self) -> usize {
        self.reglages.iter().filter(|r| r.modifie()).count()
    }

    pub fn tout_remettre(&mut self) {
        for r in &mut self.reglages {
            r.remettre_defaut();
        }
    }

    /// Peut-on ouvrir ? Un nom vide est refusé — une liste de « partie sans nom » est illisible dès
    /// qu'il y en a trois.
    pub fn prete(&self) -> bool {
        !self.nom.trim().is_empty()
            && self.code.as_ref().is_none_or(|c| c.chars().count() == CODE_LEN)
    }

    /// Une frappe au clavier. **Filtre à la source** : la police ne sait dessiner que l'ASCII
    /// imprimable, donc accepter un accent afficherait un cadre plein à la place de ce qu'on tape.
    /// Mieux vaut que la touche ne fasse rien que d'écrire un carré.
    pub fn taper(&mut self, c: char) {
        if self.nom.chars().count() >= NOM_MAX {
            return;
        }
        let c = c.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() || c == ' ' || c == '\'' || c == '-' {
            self.nom.push(c);
        }
    }

    pub fn effacer(&mut self) {
        self.nom.pop();
    }
}

/// Une partie vue dans la liste, telle que l'annonce la décrit.
#[derive(Clone, Debug)]
pub struct PartieVue {
    pub nom: String,
    pub hote: String,
    pub presents: u16,
    pub capacite: u16,
    pub code_exige: bool,
    pub reglages: Vec<Reglage>,
}

impl PartieVue {
    pub fn complete(&self) -> bool {
        self.presents >= self.capacite
    }
}

#[derive(Clone, Debug)]
pub struct Membre {
    pub nom: String,
    pub est_hote: bool,
    pub c_est_moi: bool,
}

#[derive(Clone, Debug)]
pub struct SalleAttente {
    pub nom: String,
    pub membres: Vec<Membre>,
    pub capacite: u16,
    /// Affiché **uniquement à l'hôte**. Il ne voyage jamais en clair : l'annonce dit seulement
    /// qu'un code est exigé, et la demande d'entrée le porte scellé.
    pub code: Option<String>,
    pub je_suis_hote: bool,
    pub reglages: Vec<Reglage>,
}

impl SalleAttente {
    /// Il faut au moins deux personnes — une partie à un joueur n'est pas une partie, et laisser
    /// le bouton actif ferait perdre à l'hôte le temps de comprendre pourquoi rien ne se passe.
    pub fn peut_lancer(&self) -> bool {
        self.je_suis_hote && self.membres.len() >= 2
    }
}

/// CE QU'UN CLIC VEUT DIRE.
///
/// Une action nommée par zone, jamais un ordre de rectangles : insérer un bouton un jour
/// décalerait la lecture sans qu'aucun test ne bronche, et « BANNIR » deviendrait « LANCER ».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    ChampNom,
    JoueursMoins,
    JoueursPlus,
    BasculeCode,
    ReglageMoins(usize),
    ReglagePlus(usize),
    ReglageDefaut(usize),
    ToutRemettre,
    Ouvrir,
    Rejoindre(usize),
    CreerLaMienne,
    Sortir(usize),
    Bannir(usize),
    Lancer,
    /// Revenir en arrière. **Présent sur CHAQUE écran**, et c'est une garde, pas une commodité :
    /// le 22 août, un écran de lobby sans retour l'a laissé « infiniment coincé » — ses mots.
    Retour,
}

/// Une zone cliquable, en coordonnées écran normalisées (x ∈ [0, aspect], y ∈ [0, 1]).
pub type Zone = (f32, f32, f32, f32);

/// Tout l'état du lobby, côté jeu.
#[derive(Clone, Debug, Default)]
pub struct Lobby {
    pub vue: VueLobby,
    pub creation: Creation,
    pub salle: Option<SalleAttente>,
    pub parties: Vec<PartieVue>,
    /// Les zones du dernier écran dessiné, avec le SENS de chacune.
    ///
    /// ⚠ Derrière un `RefCell` parce que le dessin se fait en `&self` : la passe de rendu passe
    /// son contexte en immuable (`&Exterieur`), et la traverser en mutable pour ce seul besoin
    /// aurait touché toute la chaîne d'appel. L'alternative — calculer le layout deux fois, une
    /// fois pour dessiner et une fois pour les zones — ferait diverger le dessin des clics au
    /// premier ajustement de pixel : c'est exactement le défaut qu'on veut éviter.
    zones: std::cell::RefCell<Vec<(Zone, Action)>>,
    /// Le champ de saisie a-t-il le focus ? Les frappes n'y vont que dans ce cas.
    pub saisie_active: bool,
}

impl Lobby {
    pub fn ouvert(&self) -> bool {
        self.vue != VueLobby::Fermee
    }

    /// Ouvre le lobby sur la liste des parties.
    pub fn ouvrir(&mut self) {
        self.vue = VueLobby::Liste;
    }

    pub fn fermer(&mut self) {
        self.vue = VueLobby::Fermee;
        self.saisie_active = false;
    }

    /// Les zones cliquables du dernier écran DESSINÉ, avec le sens de chacune.
    ///
    /// Existe pour la console de pilotage, et le détour a une raison. Un scénario de test pourrait
    /// appeler `agir` directement — c'est d'ailleurs ce que `agir` est fait pour. Mais il ne
    /// prouverait alors que la RÈGLE, jamais qu'un bouton dessiné est réellement atteignable : le
    /// jour où le dessin et les zones divergent d'un pixel, la règle continue de passer et
    /// personne ne peut plus cliquer. En passant par ici, un scénario dit « clique sur CRÉER »
    /// sans coder aucun rectangle en dur — la coordonnée est LUE au lieu d'être écrite, donc elle
    /// ne peut pas se périmer, et un bouton devenu inatteignable fait échouer le scénario.
    pub fn zones_visibles(&self) -> Vec<(Zone, Action)> {
        self.zones.borrow().clone()
    }

    /// Un clic aux coordonnées normalisées. Rend `true` s'il a été consommé.
    pub fn clic(&mut self, x: f32, y: f32) -> bool {
        if !self.ouvert() {
            return false;
        }
        let trouve = self
            .zones
            .borrow()
            .iter()
            .find(|((zx, zy, zw, zh), _)| x >= *zx && x < zx + zw && y >= *zy && y < zy + zh)
            .map(|(_, a)| *a);
        match trouve {
            Some(a) => {
                self.agir(a);
                true
            }
            // Un clic dans le vide RETIRE le focus de saisie : sinon on tape encore dans un champ
            // qu'on croit avoir quitté.
            None => {
                self.saisie_active = false;
                true
            }
        }
    }

    /// Ce qu'une action fait. Séparé du clic pour être éprouvable SANS coordonnées : un test qui
    /// doit viser un pixel teste le placement, pas la règle.
    pub fn agir(&mut self, a: Action) {
        match a {
            Action::ChampNom => self.saisie_active = true,
            Action::JoueursMoins => {
                self.creation.joueurs = self.creation.joueurs.saturating_sub(1).max(JOUEURS_MIN)
            }
            Action::JoueursPlus => {
                self.creation.joueurs = (self.creation.joueurs + 1).min(JOUEURS_MAX)
            }
            Action::BasculeCode => {
                self.creation.code = match self.creation.code {
                    None => Some(tirer_un_code()),
                    Some(_) => None,
                }
            }
            Action::ReglageMoins(i) => {
                if let Some(r) = self.creation.reglages.get_mut(i) {
                    r.bouger(-1)
                }
            }
            Action::ReglagePlus(i) => {
                if let Some(r) = self.creation.reglages.get_mut(i) {
                    r.bouger(1)
                }
            }
            Action::ReglageDefaut(i) => {
                if let Some(r) = self.creation.reglages.get_mut(i) {
                    r.remettre_defaut()
                }
            }
            Action::ToutRemettre => self.creation.tout_remettre(),
            Action::CreerLaMienne => {
                self.vue = VueLobby::Creer;
                self.saisie_active = true;
            }
            Action::Ouvrir => {
                if self.creation.prete() {
                    self.salle = Some(SalleAttente {
                        nom: self.creation.nom.trim().to_string(),
                        membres: vec![Membre { nom: "MOI".into(), est_hote: true, c_est_moi: true }],
                        capacite: self.creation.joueurs,
                        code: self.creation.code.clone(),
                        je_suis_hote: true,
                        reglages: self.creation.reglages.clone(),
                    });
                    self.vue = VueLobby::Attente;
                    self.saisie_active = false;
                }
            }
            Action::Rejoindre(_) => {
                // Le paquet part par le cœur ; l'écran attend le verdict de l'hôte. Basculer ici
                // ferait croire qu'on est entré alors qu'on a seulement frappé.
            }
            Action::Sortir(i) | Action::Bannir(i) => {
                if let Some(s) = self.salle.as_mut() {
                    // ⚠ La garde vit ICI, pas seulement dans le dessin : les boutons ne sont pas
                    // affichés chez un invité, mais une garde qui n'existe que dans le dessin
                    // n'est pas une garde.
                    if s.je_suis_hote && s.membres.get(i).is_some_and(|m| !m.est_hote) {
                        s.membres.remove(i);
                    }
                }
            }
            Action::Lancer => {}
            Action::Retour => match self.vue {
                VueLobby::Creer => self.vue = VueLobby::Liste,
                VueLobby::Attente => {
                    self.salle = None;
                    self.vue = VueLobby::Liste;
                }
                // Depuis la liste, on retourne au JEU. C'est la sortie qui manquait le 22 août.
                VueLobby::Liste | VueLobby::Fermee => self.fermer(),
            },
        }
    }

    /// Une frappe clavier. Ne fait quelque chose que si un champ a le focus.
    pub fn taper(&mut self, c: char) {
        if self.saisie_active && self.vue == VueLobby::Creer {
            self.creation.taper(c);
        }
    }

    pub fn effacer(&mut self) {
        if self.saisie_active && self.vue == VueLobby::Creer {
            self.creation.effacer();
        }
    }
}

/// Cinq caractères tirés au sort, sans les quatre qui s'entendent pareil à l'oral.
///
/// `I`/`1` et `O`/`0` se confondent quand on dicte un code à voix haute — et c'est exactement
/// comme ça qu'un code de partie se transmet.
///
/// # ⚠ Où ce code DEVRA être tiré, et pourquoi pas ici
///
/// À terme, **c'est le cœur qui doit le produire** : c'est lui qui porte le secret du salon, lui
/// qui en dérive la clé, et lui qui a déjà `getrandom` (entropie de l'OS, dépendance auditée du
/// projet). Le jeu n'aura qu'à afficher ce qu'on lui donne. Quand le pont sidecar portera les
/// salons, cette fonction disparaîtra — elle ne doit pas devenir une seconde source de secrets.
///
/// En attendant, on lit `/dev/urandom` **en std pur** : ajouter une dépendance à ce crate se
/// valide avec l'auteur avant, jamais après (règle du projet), et il n'y en avait aucune pour
/// l'aléa. Sous Windows ce chemin n'existe pas — le repli est marqué et visible à l'écran plutôt
/// que silencieux : un code faible qu'on croit fort est pire qu'un code absent.
fn tirer_un_code() -> String {
    use std::io::Read;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut brut = [0u8; CODE_LEN];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut brut))
        .is_ok();
    if !ok {
        // Visible, et pas un code plausible : personne ne doit croire cette table protégée.
        return "-----".into();
    }
    brut.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
//  LE RENDU — la DA du jeu, pas celle du launcher
// ═══════════════════════════════════════════════════════════════════════════════════════════════
//
//  Tout est en coordonnées d'écran normalisées (x ∈ [0, aspect], y ∈ [0, 1], y vers le bas), comme
//  le reste du HUD. Rien n'est repris de l'autre interface : mêmes règles, dessin entièrement
//  refait avec le `Pinceau` et la fonte 5×7 du jeu.

/// Couche de dessin du lobby. Au-dessus du HUD ordinaire (3-5) : quand on choisit sa partie, rien
/// du jeu derrière n'a à passer devant.
const COUCHE: u8 = 8;

const H_TITRE: f32 = 0.046;
const H_TEXTE: f32 = 0.026;
const H_PETIT: f32 = 0.020;
const MARGE: f32 = 0.026;

/// Le côté d'un bouton rond « − » / « + ».
const BOUTON_PAS: f32 = 0.040;
/// La largeur du bouton « DEFAUT », et l'écart qui le sépare du « + ».
const W_DEFAUT: f32 = 0.13;
const ECART_DEFAUT: f32 = 0.02;
/// La colonne où s'écrit la valeur.
const W_VALEUR: f32 = 0.15;

/// Les deux boutons que l'hôte voit en face d'un membre (« SORTIR », « BANNIR »), et leur écart.
const W_BOUTON_MEMBRE: f32 = 0.15;
const ECART_BOUTONS: f32 = 0.012;
/// Ce qu'ils prennent ensemble : la place que le nom d'un membre ne doit jamais atteindre.
const LARGEUR_BOUTONS_HOTE: f32 = 2.0 * W_BOUTON_MEMBRE + ECART_BOUTONS;

/// Ce que les commandes d'une ligne de réglage prennent, à droite : `− valeur + DEFAUT`.
///
/// Écrit UNE fois et vérifié sur place (`debug_assert` dans `ligne_reglage`) : c'est ce chiffre qui
/// dit combien de place reste au libellé, et une copie qui dérive rendrait le calcul de taille faux
/// sans que rien ne le signale — le texte recommencerait à passer sous les boutons.
const LARGEUR_COMMANDES: f32 = W_DEFAUT + ECART_DEFAUT + BOUTON_PAS + W_VALEUR + BOUTON_PAS;
/// Hauteur d'une ligne de réglage — assez pour deux boutons au pouce, assez peu pour que les cinq
/// réglages tiennent sans faire défiler.
const H_LIGNE: f32 = 0.058;

impl Lobby {
    /// Dessine l'écran en cours. Ne fait rien si le lobby est fermé.
    ///
    /// # Sécurité
    /// Appelée depuis la passe de rendu, avec un `Pinceau` valide pour le command buffer courant.
    pub unsafe fn dessiner(&self, p: &Pinceau) {
        if !self.ouvert() {
            return;
        }
        self.zones.borrow_mut().clear();
        unsafe {
            // Voile plein écran : le lobby n'est pas une fenêtre posée sur le jeu, c'est un
            // moment à part. Le laisser transparent ferait lire le texte par-dessus la carte.
            p.quad(0.0, 0.0, p.aspect, 1.0, couleurs::FOND, COUCHE - 1);
            match self.vue {
                VueLobby::Fermee => {}
                VueLobby::Liste => self.dessiner_liste(p),
                VueLobby::Creer => self.dessiner_creation(p),
                VueLobby::Attente => self.dessiner_attente(p),
            }
            // ⚠ LE RETOUR EST DESSINÉ EN DERNIER ET SUR **TOUS** LES ÉCRANS, sans condition.
            // Le 22 août, un écran de lobby sans sortie l'a laissé « infiniment coincé » — ses
            // mots. Le poser ici plutôt que dans chaque écran rend l'oubli impossible : il n'y a
            // plus qu'un endroit où il peut manquer, et il ne manque pas.
            self.bouton_retour(p);
        }
    }

    unsafe fn bouton_retour(&self, p: &Pinceau) {
        let (w, h) = (0.20, 0.052);
        let (x, y) = (MARGE, 1.0 - h - MARGE);
        unsafe {
            p.quad(x, y, w, h, couleurs::LIGNE_MOI, COUCHE);
            let lbl = if self.vue == VueLobby::Liste { "< JOUER" } else { "< RETOUR" };
            let tx = x + (w - largeur_texte(lbl, H_TEXTE)) * 0.5;
            p.texte(tx, y + (h - H_TEXTE) * 0.5, H_TEXTE, couleurs::TEXTE, COUCHE + 1, lbl);
        }
        self.zones.borrow_mut().push(((x, y, w, h), Action::Retour));
    }

    /// Un bouton rond « − » ou « + », et sa zone.
    unsafe fn bouton_pas(&self, p: &Pinceau, x: f32, y: f32, plus: bool, actif: bool, a: Action) {
        let c = 0.040;
        let teinte = if actif { couleurs::LIGNE_MOI } else { couleurs::LIGNE };
        let encre = if actif { couleurs::TEXTE } else { couleurs::TEXTE_FAIBLE };
        let lbl = if plus { "+" } else { "-" };
        unsafe {
            p.quad(x, y, c, c, teinte, COUCHE);
            p.texte(
                x + (c - largeur_texte(lbl, H_TEXTE)) * 0.5,
                y + (c - H_TEXTE) * 0.5,
                H_TEXTE, encre, COUCHE + 1, lbl,
            );
        }
        if actif {
            self.zones.borrow_mut().push(((x, y, c, c), a));
        }
    }

    /// Une ligne « NOM ……… − valeur + [DEFAUT] ». Le bouton défaut n'apparaît que si le réglage a
    /// bougé : toujours visible, il ajouterait cinq zones cliquables qui ne disent rien
    /// (« remettre 150 à 150 » n'est pas une action) ; conditionnel, il devient une INFORMATION.
    /// La hauteur de caractère des libellés de réglages, **commune à toutes les lignes**.
    ///
    /// Le plus long décide : sinon chaque ligne prendrait sa propre taille et la colonne des noms
    /// serait en dents de scie — ce qui se lit comme un défaut, pas comme une adaptation.
    ///
    /// C'est ici que sa règle du 29 août est tenue — *« peu importe la taille de la fenêtre, le
    /// texte ne doit jamais partir sur les autres box »*. La place disponible est CALCULÉE
    /// (la largeur utile, moins ce que les commandes prennent à droite, moins une marge), jamais
    /// supposée. Le débordement devient impossible par construction plutôt que par réglage — même
    /// patron que les bornes du carton mystère, où deux constantes réglées à l'œil ont disparu.
    fn hauteur_des_libelles(&self, premier: Reglage, larg: f32) -> f32 {
        let place = larg - LARGEUR_COMMANDES - MARGE;
        std::iter::once(premier.nom)
            .chain(self.creation.reglages.iter().map(|r| r.nom))
            .fold(H_TEXTE, |h, nom| h.min(crate::hud::hauteur_pour_tenir(nom, H_TEXTE, place)))
    }

    unsafe fn ligne_reglage(
        &self,
        p: &Pinceau,
        x0: f32,
        larg: f32,
        y: f32,
        r: Reglage,
        acts: (Action, Action, Option<Action>),
        h_libelle: f32,
    ) {
        let droite = x0 + larg;
        let c = BOUTON_PAS;
        // Les commandes sont calées à DROITE dans un ordre fixe : une colonne de valeurs qui
        // s'aligne se lit d'un coup d'œil, des valeurs qui suivent la longueur du nom obligent
        // l'œil à chercher à chaque ligne.
        let w_defaut = W_DEFAUT;
        let x_defaut = droite - w_defaut;
        let x_plus = x_defaut - ECART_DEFAUT - c;
        let w_val = W_VALEUR;
        let x_val = x_plus - w_val;
        let x_moins = x_val - c;
        debug_assert!(
            (x_moins - (droite - LARGEUR_COMMANDES)).abs() < 1e-6,
            "LARGEUR_COMMANDES ne decrit plus la place que les commandes prennent"
        );

        unsafe {
            // La hauteur vient de l'appelant : elle est COMMUNE à toutes les lignes, calculée sur
            // le libellé le plus long. Chaque ligne choisissant la sienne, la colonne des noms
            // aurait des tailles différentes d'une ligne à l'autre — ce qui se lit comme un défaut.
            p.texte(x0, y + (c - h_libelle) * 0.5, h_libelle, couleurs::TEXTE, COUCHE + 1, r.nom);
        }
        unsafe {
            self.bouton_pas(p, x_moins, y, false, r.valeur > r.min, acts.0);
            self.bouton_pas(p, x_plus, y, true, r.valeur < r.max, acts.1);
        }
        let aff = r.affichage();
        // La valeur modifiée passe en OR : la couleur dit « touché » avant même qu'on lise le
        // bouton à côté. La même information portée deux fois, à deux vitesses de lecture.
        let teinte = if r.modifie() { couleurs::OR } else { couleurs::TEXTE };
        // La valeur tient dans SA colonne, elle aussi : « -30 PTS » réclame 0,175 quand la colonne
        // en offre 0,150, et mordait donc sur les deux boutons qui l'encadrent. La hauteur est
        // calculée sur la valeur la plus large que ce réglage puisse ATTEINDRE, pas sur celle du
        // moment — sinon le texte changerait de taille à chaque clic, sous le doigt.
        let h_val = crate::hud::hauteur_pour_tenir(&r.affichage_le_plus_large(), H_TEXTE, w_val);
        unsafe {
            p.texte(
                x_val + (w_val - largeur_texte(&aff, h_val)) * 0.5,
                y + (c - h_val) * 0.5,
                h_val, teinte, COUCHE + 1, &aff,
            );
        }
        if let (true, Some(a)) = (r.modifie(), acts.2) {
            let hb = 0.034;
            let yb = y + (c - hb) * 0.5;
            unsafe {
                p.quad(x_defaut, yb, w_defaut, hb, couleurs::LIGNE_MOI, COUCHE);
                p.texte(
                    x_defaut + (w_defaut - largeur_texte("DEFAUT", H_PETIT)) * 0.5,
                    yb + (hb - H_PETIT) * 0.5,
                    H_PETIT, couleurs::OR, COUCHE + 1, "DEFAUT",
                );
            }
            self.zones.borrow_mut().push(((x_defaut, yb, w_defaut, hb), a));
        }
    }

    /// Un bouton plein pleine largeur — celui qui engage.
    ///
    /// La boîte est passée en un seul `(x, y, largeur)` : clippy comptait huit arguments, et il
    /// avait raison — trois flottants nus côte à côte s'inversent un jour sans que rien ne le dise.
    unsafe fn bouton_large(&self, p: &Pinceau, boite: (f32, f32, f32), lbl: &str, actif: bool, a: Action) {
        let (x, y, w) = boite;
        let h = 0.062;
        let fond = if actif { couleurs::LIGNE_MOI } else { couleurs::LIGNE };
        let encre = if actif { couleurs::TEXTE } else { couleurs::TEXTE_FAIBLE };
        unsafe {
            p.quad(x, y, w, h, fond, COUCHE);
            p.texte(
                x + (w - largeur_texte(lbl, H_TEXTE)) * 0.5,
                y + (h - H_TEXTE) * 0.5,
                H_TEXTE, encre, COUCHE + 1, lbl,
            );
        }
        if actif {
            self.zones.borrow_mut().push(((x, y, w, h), a));
        }
    }
}

impl Lobby {
    /// ÉCRAN « LES PARTIES OUVERTES ».
    unsafe fn dessiner_liste(&self, p: &Pinceau) {
        let larg = (p.aspect * 0.72).min(1.05);
        let x0 = (p.aspect - larg) * 0.5;
        unsafe {
            p.texte_centre(0.070, H_TITRE, couleurs::TEXTE, COUCHE + 1, "LES PARTIES OUVERTES");
        }

        // ── L'ÉTAT VIDE ──────────────────────────────────────────────────────────────────────
        // Il compte autant que les autres : c'est ce que voit le PREMIER arrivé. Lui dire « rien »
        // sans lui dire quoi faire le laisse croire que quelque chose est cassé.
        if self.parties.is_empty() {
            unsafe {
                p.texte_centre(0.30, H_TEXTE, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                    "PERSONNE N'A OUVERT DE PARTIE");
                p.texte_centre(0.35, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                    "LES TABLES S'ANNONCENT SANS SERVEUR - LA TIENNE AUSSI");
                self.bouton_large(p, (x0 + (larg - 0.46) * 0.5, 0.44, 0.46), "CREER LA MIENNE", true,
                    Action::CreerLaMienne);
            }
            return;
        }

        unsafe {
            let s = format!("{} TABLES", self.parties.len());
            p.texte_centre(0.126, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, &s);
        }

        let h_carte = 0.115;
        let mut y = 0.175;
        for (i, pa) in self.parties.iter().enumerate() {
            if y + h_carte > 0.86 {
                break;
            }
            let complete = pa.complete();
            // Une table pleine reste VISIBLE mais s'efface : la masquer ferait croire qu'elle
            // n'existe pas, alors qu'une place peut se libérer à la seconde d'après.
            let fond = if complete { couleurs::LIGNE } else { couleurs::LIGNE_MOI };
            let encre = if complete { couleurs::TEXTE_FAIBLE } else { couleurs::TEXTE };
            unsafe {
                p.quad(x0, y, larg, h_carte, fond, COUCHE);
                p.texte(x0 + MARGE, y + 0.018, H_TEXTE, encre, COUCHE + 1, &pa.nom);
                if pa.code_exige {
                    // Le cadenas est DESSINÉ, pas écrit : la fonte 5×7 n'a pas d'emoji, et un
                    // glyphe inconnu sortirait en cadre plein.
                    let cx = x0 + MARGE + largeur_texte(&pa.nom, H_TEXTE) + 0.018;
                    p.quad(cx, y + 0.026, 0.020, 0.014, couleurs::OR, COUCHE + 1);
                    p.quad(cx + 0.005, y + 0.018, 0.010, 0.008, couleurs::OR, COUCHE + 1);
                }
                let par = format!("PAR {}", pa.hote);
                p.texte(x0 + MARGE, y + 0.056, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, &par);

                // Le compte de places, calé à droite et gros : c'est l'information qui décide.
                let places = format!("{} / {}", pa.presents, pa.capacite);
                let teinte = if complete { couleurs::TEXTE_FAIBLE } else { couleurs::OR };
                p.texte(x0 + larg - MARGE - largeur_texte(&places, H_TEXTE), y + 0.018,
                    H_TEXTE, teinte, COUCHE + 1, &places);
                if complete {
                    let l = "COMPLET";
                    p.texte(x0 + larg - MARGE - largeur_texte(l, H_PETIT), y + 0.056,
                        H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, l);
                }

                // Les réglages en une ligne : ce sont eux qui font choisir entre deux tables.
                let resume: Vec<String> = pa.reglages.iter().take(3)
                    .map(|r| format!("{} {}", r.nom.split(' ').next_back().unwrap_or(r.nom), r.affichage()))
                    .collect();
                p.texte(x0 + MARGE, y + 0.086, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                    &resume.join("  "));
            }
            // Une table pleine n'est pas cliquable : proposer d'y frapper serait promettre une
            // place qui n'existe pas.
            if !complete {
                self.zones.borrow_mut().push(((x0, y, larg, h_carte), Action::Rejoindre(i)));
            }
            y += h_carte + 0.014;
        }
        unsafe {
            self.bouton_large(p, (x0 + larg - 0.46, 0.90, 0.46), "CREER LA MIENNE", true,
                Action::CreerLaMienne);
        }
    }

    /// ÉCRAN « CREER UNE PARTIE ».
    ///
    /// L'ordre des blocs suit l'ordre des décisions : d'abord comment ça s'appelle et qui peut
    /// entrer (ce qu'on décide en une seconde), ensuite les réglages fins. Mettre les cinq
    /// réglages en haut ferait croire qu'il faut les régler pour ouvrir une table.
    unsafe fn dessiner_creation(&self, p: &Pinceau) {
        let larg = (p.aspect * 0.76).min(1.15);
        let x0 = (p.aspect - larg) * 0.5;
        unsafe {
            // Bornés à la largeur de l'écran : ce sous-titre sortait des DEUX côtés en fenêtre
            // étroite (« …TIENS LA PORTE — TU CHOISIS QUI ENTRE ET QUAND CA COMME… »).
            let utile = p.aspect - 2.0 * MARGE;
            let titre = "CREER UNE PARTIE";
            let sous = "TU TIENS LA PORTE - TU CHOISIS QUI ENTRE ET QUAND CA COMMENCE";
            p.texte_centre(0.055, crate::hud::hauteur_pour_tenir(titre, H_TITRE, utile),
                couleurs::TEXTE, COUCHE + 1, titre);
            p.texte_centre(0.112, crate::hud::hauteur_pour_tenir(sous, H_PETIT, utile),
                couleurs::TEXTE_FAIBLE, COUCHE + 1, sous);
        }

        // ── LE NOM ───────────────────────────────────────────────────────────────────────────
        let (y_nom, h_nom) = (0.155, 0.064);
        let fond = if self.saisie_active { couleurs::LIGNE_MOI } else { couleurs::LIGNE };
        unsafe {
            p.quad(x0, y_nom, larg, h_nom, fond, COUCHE);
            if self.creation.nom.is_empty() {
                p.texte(x0 + MARGE, y_nom + (h_nom - H_TEXTE) * 0.5, H_TEXTE,
                    couleurs::TEXTE_FAIBLE, COUCHE + 1, "LE NOM DE TA PARTIE");
            } else {
                let l = p.texte(x0 + MARGE, y_nom + (h_nom - H_TEXTE) * 0.5, H_TEXTE,
                    couleurs::TEXTE, COUCHE + 1, &self.creation.nom);
                if self.saisie_active {
                    p.quad(x0 + MARGE + l + 0.006, y_nom + (h_nom - H_TEXTE) * 0.5, 0.006,
                        H_TEXTE, couleurs::TEXTE, COUCHE + 1);
                }
            }
        }
        self.zones.borrow_mut().push(((x0, y_nom, larg, h_nom), Action::ChampNom));

        // ── COMBIEN DE JOUEURS ───────────────────────────────────────────────────────────────
        let y_j = y_nom + h_nom + 0.022;
        let joueurs = Reglage {
            numero: 0, nom: "JOUEURS AU MAXIMUM", unite: "",
            valeur: self.creation.joueurs as i32, defaut: JOUEURS_DEFAUT as i32,
            min: JOUEURS_MIN as i32, max: JOUEURS_MAX as i32, pas: 1,
        };
        // La taille des libellés est DÉRIVÉE de la place qui leur reste vraiment, et le libellé le
        // plus long décide pour tous. Rien n'est réglé à l'œil ici : le texte ne peut plus
        // atteindre les commandes à AUCUNE largeur de fenêtre, parce que la place disponible entre
        // dans le calcul au lieu d'être supposée.
        let h_libelle = self.hauteur_des_libelles(joueurs, larg);
        unsafe {
            self.ligne_reglage(p, x0, larg, y_j, joueurs,
                (Action::JoueursMoins, Action::JoueursPlus, None), h_libelle);
        }

        // ── LE CODE D'ACCÈS ──────────────────────────────────────────────────────────────────
        // Une bascule, pas une case perdue : c'est la décision la plus lourde de l'écran, celle
        // qui sépare « n'importe qui entre » de « il faut que je le donne à quelqu'un ».
        let y_c = y_j + H_LIGNE;
        let ferme = self.creation.code.is_some();
        unsafe {
            // La MÊME taille que les libellés de réglages : c'est la même colonne, et deux
            // tailles côte à côte se lisent comme un défaut de rendu, pas comme une hiérarchie.
            p.texte(x0, y_c + 0.008, h_libelle, couleurs::TEXTE, COUCHE + 1, "CODE D'ACCES");
            let (bw, bh) = (0.09, 0.040);
            let bx = x0 + larg - bw;
            p.quad(bx, y_c, bw, bh, if ferme { couleurs::OR } else { couleurs::LIGNE }, COUCHE);
            let px = if ferme { bx + bw - 0.036 } else { bx + 0.004 };
            p.quad(px, y_c + 0.004, 0.032, bh - 0.008, couleurs::FOND, COUCHE + 1);
            self.zones.borrow_mut().push(((bx, y_c, bw, bh), Action::BasculeCode));

            match &self.creation.code {
                Some(code) => {
                    // Espacé et gros : il est fait pour être DIT à voix haute, pas relu par son
                    // auteur. Des caractères serrés se dictent mal.
                    let esp: String = code.chars().flat_map(|c| [c, ' ']).collect();
                    let e = esp.trim_end();
                    p.texte(bx - 0.03 - largeur_texte(e, H_TEXTE), y_c + 0.008, H_TEXTE,
                        couleurs::OR, COUCHE + 1, e);
                }
                None => {
                    p.texte(x0, y_c + 0.040, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                        "LA TABLE EST OUVERTE A TOUT LE MONDE");
                }
            }
        }

        // ── LES RÉGLAGES ─────────────────────────────────────────────────────────────────────
        let mut y = y_c + H_LIGNE + 0.020;
        unsafe {
            p.texte(x0, y, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, "REGLAGES");
            // « TOUT REMETTRE » n'apparaît qu'à partir de DEUX réglages touchés : pour un seul,
            // le bouton de la ligne fait exactement la même chose, plus près de la main.
            if self.creation.touches() >= 2 {
                let l = format!("TOUT REMETTRE ({})", self.creation.touches());
                let lw = largeur_texte(&l, H_PETIT);
                p.texte(x0 + larg - lw, y, H_PETIT, couleurs::OR, COUCHE + 1, &l);
                self.zones.borrow_mut().push(((x0 + larg - lw, y - 0.006, lw, 0.032), Action::ToutRemettre));
            }
        }
        y += 0.030;
        for (i, r) in self.creation.reglages.iter().enumerate() {
            unsafe {
                self.ligne_reglage(p, x0, larg, y, *r,
                    (Action::ReglageMoins(i), Action::ReglagePlus(i), Some(Action::ReglageDefaut(i))),
                    h_libelle);
            }
            y += H_LIGNE;
        }

        // ── OUVRIR ───────────────────────────────────────────────────────────────────────────
        let prete = self.creation.prete();
        unsafe {
            self.bouton_large(p, (x0, y + 0.020, larg), "OUVRIR LE LOBBY", prete, Action::Ouvrir);
            if !prete {
                let l = if self.creation.nom.trim().is_empty() {
                    "DONNE UN NOM A TA PARTIE POUR L'OUVRIR"
                } else {
                    "LE CODE DOIT FAIRE 5 CARACTERES"
                };
                p.texte_centre(y + 0.095, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, l);
            }
        }
    }

    /// ÉCRAN « LA SALLE D'ATTENTE ».
    ///
    /// ⚠ **Le code n'est affiché QU'À L'HÔTE**, et c'est la seule règle de cet écran qui ne soit
    /// pas cosmétique. Il ne voyage pas dans l'annonce et la demande d'entrée le porte scellé :
    /// l'écran serait le seul endroit de tout le système où il pourrait fuiter.
    unsafe fn dessiner_attente(&self, p: &Pinceau) {
        // Pas de salle alors que la vue l'annonce : on ne peint rien plutôt que d'inventer un
        // écran vide. `agir(Retour)` reste disponible — le bouton est dessiné hors d'ici.
        let Some(s) = self.salle.clone() else { return };
        let larg = (p.aspect * 0.72).min(1.05);
        let x0 = (p.aspect - larg) * 0.5;

        unsafe {
            p.texte_centre(0.060, H_TITRE, couleurs::TEXTE, COUCHE + 1, &s.nom);
            let sous = format!("{} / {} - ON ATTEND", s.membres.len(), s.capacite);
            p.texte_centre(0.118, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, &sous);
        }

        let mut y = 0.165;
        if let (true, Some(code)) = (s.je_suis_hote, s.code.as_ref()) {
            let h = 0.086;
            unsafe {
                p.quad(x0, y, larg, h, couleurs::LIGNE_MOI, COUCHE);
                p.texte(x0 + MARGE, y + 0.012, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                    "LE CODE A DONNER");
                let esp: String = code.chars().flat_map(|c| [c, ' ']).collect();
                let esp = esp.trim_end();
                // Le code passe en premier et prend ce qu'il lui faut — c'est LUI qu'on vient
                // lire, et le lire de travers ferait entrer la mauvaise personne.
                let h_code = crate::hud::hauteur_pour_tenir(esp, H_TITRE, larg - 2.0 * MARGE);
                let l_code = p.texte(x0 + MARGE, y + 0.040, h_code, couleurs::OR, COUCHE + 1, esp);
                // L'avertissement se contente de CE QUI RESTE sur la ligne du code. Il était posé
                // au bord droit sans regarder où le code s'arrêtait : les deux s'écrivaient l'un
                // sur l'autre dès que la fenêtre se resserrait, et aucun des deux ne se lisait.
                let av = "PERSONNE D'AUTRE NE LE VOIT";
                let reste = larg - 2.0 * MARGE - l_code - 0.020;
                let h_av = crate::hud::hauteur_pour_tenir(av, H_PETIT, reste);
                p.texte(x0 + larg - MARGE - largeur_texte(av, h_av), y + 0.058, h_av,
                    couleurs::TEXTE_FAIBLE, COUCHE + 1, av);
            }
            y += h + 0.020;
        }

        // ⚠ LA LIGNE D'UN MEMBRE PORTE DEUX TEXTES EMPILÉS, et sa hauteur ne les contenait pas :
        // 0,010 de marge + le nom (0,026) + le nom de son rôle (0,020) + la marge du bas font
        // 0,068, quand la boîte n'en offrait que 0,054. « MOI » et « TIENT LA PORTE - TOI » se
        // chevauchaient donc, et le second sortait par le bas. La hauteur est maintenant DÉDUITE
        // de ce qu'on y met, au lieu d'être un chiffre rond posé à côté.
        const MARGE_LIGNE: f32 = 0.010;
        const ECART_ETIQ: f32 = 0.002;
        let h_boite = MARGE_LIGNE + H_TEXTE + ECART_ETIQ + H_PETIT + MARGE_LIGNE;
        let h_l = h_boite + 0.008;
        for (i, m) in s.membres.iter().enumerate() {
            if y + h_l > 0.84 {
                break;
            }
            let fond = if m.c_est_moi { couleurs::LIGNE_MOI } else { couleurs::LIGNE };
            unsafe {
                p.quad(x0, y, larg, h_boite, fond, COUCHE);
                // La place d'un texte s'arrête où commencent les boutons de l'hôte — sinon un nom
                // un peu long viendrait s'écrire dessus.
                let place = larg - 2.0 * MARGE
                    - if s.je_suis_hote && !m.est_hote { LARGEUR_BOUTONS_HOTE } else { 0.0 };
                p.texte(x0 + MARGE, y + MARGE_LIGNE,
                    crate::hud::hauteur_pour_tenir(&m.nom, H_TEXTE, place),
                    couleurs::TEXTE, COUCHE + 1, &m.nom);
                let mut etiq = Vec::new();
                if m.est_hote {
                    etiq.push("TIENT LA PORTE");
                }
                if m.c_est_moi {
                    etiq.push("TOI");
                }
                if !etiq.is_empty() {
                    let e = etiq.join(" - ");
                    p.texte(x0 + MARGE, y + MARGE_LIGNE + H_TEXTE + ECART_ETIQ,
                        crate::hud::hauteur_pour_tenir(&e, H_PETIT, place),
                        couleurs::TEXTE_FAIBLE, COUCHE + 1, &e);
                }
            }
            // ⚠ Les deux boutons n'apparaissent QUE pour l'hôte, ET jamais en face de lui-même :
            // montrer un bouton qui échouera toujours est une promesse qu'on ne tient pas.
            if s.je_suis_hote && !m.est_hote {
                let (bw, bh) = (W_BOUTON_MEMBRE, 0.038);
                let by = y + (h_boite - bh) * 0.5;
                let bx2 = x0 + larg - MARGE - bw;
                let bx1 = bx2 - bw - ECART_BOUTONS;
                debug_assert!(
                    (bx1 - (x0 + larg - MARGE - LARGEUR_BOUTONS_HOTE)).abs() < 1e-6,
                    "LARGEUR_BOUTONS_HOTE ne decrit plus la place que ces boutons prennent"
                );
                unsafe {
                    // « SORTIR » d'abord, « BANNIR » ensuite et plus loin du bord : le geste
                    // réparable est le plus à portée, le définitif demande de viser.
                    p.quad(bx1, by, bw, bh, couleurs::FOND, COUCHE + 1);
                    p.texte(bx1 + (bw - largeur_texte("SORTIR", H_PETIT)) * 0.5,
                        by + (bh - H_PETIT) * 0.5, H_PETIT, couleurs::TEXTE, COUCHE + 2, "SORTIR");
                    p.quad(bx2, by, bw, bh, couleurs::FOND, COUCHE + 1);
                    p.texte(bx2 + (bw - largeur_texte("BANNIR", H_PETIT)) * 0.5,
                        by + (bh - H_PETIT) * 0.5, H_PETIT, couleurs::URGENCE, COUCHE + 2, "BANNIR");
                }
                self.zones.borrow_mut().push(((bx1, by, bw, bh), Action::Sortir(i)));
                self.zones.borrow_mut().push(((bx2, by, bw, bh), Action::Bannir(i)));
            }
            y += h_l;
        }

        let libres = (s.capacite as usize).saturating_sub(s.membres.len());
        if libres > 0 && y < 0.84 {
            unsafe {
                let l = format!("{libres} PLACES LIBRES");
                p.texte(x0, y + 0.008, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1, &l);
            }
        }

        unsafe {
            if s.je_suis_hote {
                self.bouton_large(p, (x0, 0.88, larg), "LANCER LA PARTIE", s.peut_lancer(),
                    Action::Lancer);
                if !s.peut_lancer() {
                    p.texte_centre(0.955, H_PETIT, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                        "IL FAUT ETRE AU MOINS DEUX");
                }
            } else {
                p.texte_centre(0.90, H_TEXTE, couleurs::TEXTE_FAIBLE, COUCHE + 1,
                    "L'HOTE LANCERA LA PARTIE");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le réglage « JOUEURS AU MAXIMUM », qui n'est pas dans la liste mais partage sa colonne.
    fn ligne_joueurs() -> Reglage {
        Reglage {
            numero: 0, nom: "JOUEURS AU MAXIMUM", unite: "",
            valeur: JOUEURS_DEFAUT as i32, defaut: JOUEURS_DEFAUT as i32,
            min: JOUEURS_MIN as i32, max: JOUEURS_MAX as i32, pas: 1,
        }
    }

    /// ⚠ **SA RÈGLE DU 29 AOÛT 2026, ÉPROUVÉE SUR TOUTE LA PLAGE DES FORMATS.**
    ///
    /// Ses mots, devant des captures où les valeurs s'écrivaient par-dessus leurs libellés :
    /// *« peu importe la taille de la fenêtre, il faut que le texte ne parte jamais sur les autres
    /// box »*. Un test à une seule largeur ne prouverait rien — c'est justement la largeur qui
    /// faisait basculer le défaut : `MALUS SI PERSONNE N'ARRIVE` demande 0,58 quand une fenêtre
    /// étroite n'en laisse que 0,33.
    ///
    /// Vérifié par mutation : rendre `hauteur_des_libelles` constante (`H_TEXTE`) fait tomber ce
    /// test dès le format 0,40 — il mord donc bien des deux côtés.
    #[test]
    fn aucun_libelle_ne_deborde_sur_les_commandes_a_aucune_largeur() {
        let l = Lobby::default();
        let joueurs = ligne_joueurs();
        // Du portrait le plus étroit à l'ultra-large : 0,40 → 4,40.
        for i in 0..=200 {
            let aspect = 0.40 + i as f32 * 0.02;
            let larg = (aspect * 0.76).min(1.15);
            let h = l.hauteur_des_libelles(joueurs, larg);
            let place = (larg - LARGEUR_COMMANDES - MARGE).max(0.0);
            for nom in std::iter::once(joueurs.nom).chain(l.creation.reglages.iter().map(|r| r.nom))
            {
                let pris = largeur_texte(nom, h);
                assert!(
                    pris <= place + 1e-6,
                    "a l aspect {aspect:.2} (larg {larg:.3}), {nom} prend {pris:.4} \
                     pour une place de {place:.4}"
                );
            }
        }
    }

    /// La valeur d'un réglage tient dans SA colonne, à n'importe quelle valeur atteignable — et
    /// sans changer de taille quand on clique, ce qui se lirait comme un défaut.
    #[test]
    fn aucune_valeur_ne_deborde_de_sa_colonne_a_aucun_reglage() {
        for r in reglages_par_defaut().into_iter().chain([ligne_joueurs()]) {
            let h = crate::hud::hauteur_pour_tenir(&r.affichage_le_plus_large(), H_TEXTE, W_VALEUR);
            let mut v = r;
            // Toutes les valeurs que l'on peut réellement atteindre, du minimum au maximum.
            v.valeur = v.min;
            while v.valeur < v.max {
                let pris = largeur_texte(&v.affichage(), h);
                assert!(
                    pris <= W_VALEUR + 1e-6,
                    "{} a {} prend {pris:.4} pour une colonne de {W_VALEUR:.4}",
                    v.nom, v.valeur
                );
                v.bouger(1);
            }
        }
    }

    /// Un texte plus court que la place disponible garde sa taille : on rétrécit, on n'agrandit
    /// jamais — sinon la taille du texte deviendrait une fonction de la fenêtre, et deux écrans
    /// côte à côte ne se ressembleraient plus.
    #[test]
    fn un_libelle_qui_tient_garde_sa_taille() {
        assert_eq!(crate::hud::hauteur_pour_tenir("MANCHES", H_TEXTE, 10.0), H_TEXTE);
        assert_eq!(crate::hud::hauteur_pour_tenir("", H_TEXTE, 10.0), H_TEXTE);
        // Place nulle ou négative : on ne dessine rien plutôt que de déborder.
        assert_eq!(crate::hud::hauteur_pour_tenir("MANCHES", H_TEXTE, 0.0), 0.0);
        assert_eq!(crate::hud::hauteur_pour_tenir("MANCHES", H_TEXTE, -1.0), 0.0);
    }

    /// **AUCUN ACCENT DANS LES LIBELLÉS — la fonte du jeu n'en a pas.**
    ///
    /// `hud::glyphe` rend tout caractère inconnu en **cadre plein**, exprès (« se voir, jamais
    /// s'effacer »). Un « DURÉE » à l'écran s'afficherait donc « DUR■E ». Aucun test de
    /// comportement ne peut voir ça, et une relecture le laisse passer parce que le mot est
    /// correctement orthographié dans le code.
    ///
    /// Ce test lit donc le SOURCE : il extrait toutes les chaînes littérales du fichier et refuse
    /// tout caractère hors ASCII imprimable. Les commentaires sont ignorés — c'est de la prose,
    /// elle ne va pas à l'écran, et l'écrire sans accent la rendrait pénible à lire.
    #[test]
    fn aucun_libelle_ne_porte_de_caractere_que_la_fonte_ignore() {
        let src = include_str!("lobby.rs");
        // ⚠ On s'arrête au module de tests : ses messages d'assertion ne vont JAMAIS à l'écran du
        // jeu, et les écrire sans accent rendrait illisible ce qui s'affiche dans un terminal.
        // (Trouvé en écrivant ce test : il tombait sur son propre message d'erreur.)
        let src = src.split("#[cfg(test)]").next().unwrap_or(src);
        for (n, ligne) in src.lines().enumerate() {
            let t = ligne.trim_start();
            if t.starts_with("//") || t.starts_with("///") || t.starts_with("*") {
                continue;
            }
            // Les chaînes littérales de la ligne, grossièrement mais suffisamment.
            let mut dans = false;
            let mut courante = String::new();
            for c in ligne.chars() {
                if c == '"' {
                    if dans && !courante.is_empty() {
                        for ch in courante.chars() {
                            assert!(
                                ch.is_ascii(),
                                "ligne {} : « {} » contient « {ch} », que la fonte 5x7 rendra en \
                                 cadre plein. Écrire sans accent : DUREE, ACCES, CREER…",
                                n + 1, courante
                            );
                        }
                    }
                    dans = !dans;
                    courante.clear();
                } else if dans {
                    courante.push(c);
                }
            }
        }
    }

    /// **IL Y A TOUJOURS UNE SORTIE, DEPUIS N'IMPORTE QUEL ÉCRAN.**
    ///
    /// Le 22 août 2026, un écran de lobby sans retour l'a laissé « infiniment coincé » — ses mots,
    /// dans le launcher. Ici le bouton est dessiné hors des écrans, en un seul endroit, et ce test
    /// exige qu'aucune vue n'y échappe. Une sortie qui dépendrait de chaque écran finirait par
    /// manquer sur celui qu'on ajoutera demain.
    #[test]
    fn chaque_ecran_a_une_sortie() {
        for vue in [VueLobby::Liste, VueLobby::Creer, VueLobby::Attente] {
            let mut l = Lobby { vue, ..Default::default() };
            if vue == VueLobby::Attente {
                l.salle = Some(SalleAttente {
                    nom: "ESSAI".into(),
                    membres: vec![Membre { nom: "MOI".into(), est_hote: true, c_est_moi: true }],
                    capacite: 8, code: None, je_suis_hote: true,
                    reglages: reglages_par_defaut(),
                });
            }
            l.agir(Action::Retour);
            assert_ne!(l.vue, vue, "aucune sortie depuis {vue:?}");
        }
        // Et depuis la liste, on retourne AU JEU — pas dans une autre vue de lobby.
        let mut l = Lobby { vue: VueLobby::Liste, ..Default::default() };
        l.agir(Action::Retour);
        assert_eq!(l.vue, VueLobby::Fermee, "depuis la liste, le retour doit rendre au jeu");
    }

    #[test]
    fn un_reglage_ne_sort_jamais_de_ses_bornes() {
        let mut r = reglages_par_defaut()[0];
        for _ in 0..500 {
            r.bouger(1);
        }
        assert_eq!(r.valeur, r.max);
        for _ in 0..500 {
            r.bouger(-1);
        }
        assert_eq!(r.valeur, r.min);
    }

    /// Le bouton « DEFAUT » n'existe que s'il sert — et disparaît dès qu'on revient à la valeur
    /// d'origine à la main, sinon l'écran mentirait sur ce qui a été touché.
    #[test]
    fn la_marque_modifie_suit_exactement_la_valeur() {
        let mut c = Creation::default();
        assert_eq!(c.touches(), 0);
        c.reglages[0].bouger(2);
        assert_eq!(c.touches(), 1);
        c.reglages[0].bouger(-2);
        assert_eq!(c.touches(), 0, "de retour au défaut, la marque doit disparaître");
        c.reglages[1].bouger(1);
        c.reglages[2].bouger(1);
        assert_eq!(c.touches(), 2);
        c.tout_remettre();
        assert_eq!(c.touches(), 0);
    }

    /// La saisie refuse ce que la fonte ne sait pas dessiner : mieux vaut qu'une touche ne fasse
    /// rien que d'écrire un carré plein à l'écran.
    #[test]
    fn la_saisie_refuse_ce_que_la_fonte_ne_sait_pas_dessiner() {
        let mut c = Creation::default();
        for ch in "Éàç€\u{1F512}".chars() {
            c.taper(ch);
        }
        assert!(c.nom.is_empty(), "un caractère hors fonte est entré : {}", c.nom);
        for ch in "la partie d'ada-2".chars() {
            c.taper(ch);
        }
        assert_eq!(c.nom, "LA PARTIE D'ADA-2", "la saisie doit monter en majuscules");
        // Et le nom est borné : au-delà il mordrait sur la colonne des places.
        for _ in 0..100 {
            c.taper('X');
        }
        assert_eq!(c.nom.chars().count(), NOM_MAX);
    }

    #[test]
    fn une_partie_sans_nom_ou_a_code_incomplet_ne_s_ouvre_pas() {
        let mut l = Lobby { vue: VueLobby::Creer, ..Default::default() };
        l.agir(Action::Ouvrir);
        assert!(l.salle.is_none(), "une partie sans nom ne doit pas s'ouvrir");
        l.creation.nom = "ESSAI".into();
        l.creation.code = Some("AB".into());
        l.agir(Action::Ouvrir);
        assert!(l.salle.is_none(), "un code de 2 caractères ne doit pas passer");
        l.creation.code = None;
        l.agir(Action::Ouvrir);
        assert_eq!(l.vue, VueLobby::Attente);
        assert_eq!(l.salle.as_ref().unwrap().membres.len(), 1, "l'hôte s'assoit à sa table");
    }

    /// **Les gardes ne vivent pas que dans le dessin.** Les boutons ne sont pas affichés chez un
    /// invité — mais une garde qui n'existe que dans le dessin n'est pas une garde.
    #[test]
    fn un_invite_ne_peut_exclure_personne_et_l_hote_ne_s_exclut_pas() {
        let membres = || vec![
            Membre { nom: "ADA".into(), est_hote: true, c_est_moi: false },
            Membre { nom: "MOI".into(), est_hote: false, c_est_moi: true },
        ];
        let mut invite = Lobby {
            vue: VueLobby::Attente,
            salle: Some(SalleAttente {
                nom: "CHEZ ADA".into(), membres: membres(), capacite: 8, code: None,
                je_suis_hote: false, reglages: reglages_par_defaut(),
            }),
            ..Default::default()
        };
        invite.agir(Action::Bannir(1));
        assert_eq!(invite.salle.as_ref().unwrap().membres.len(), 2, "un invité a exclu quelqu'un");

        let mut hote = Lobby {
            vue: VueLobby::Attente,
            salle: Some(SalleAttente {
                nom: "CHEZ MOI".into(), membres: membres(), capacite: 8, code: None,
                je_suis_hote: true, reglages: reglages_par_defaut(),
            }),
            ..Default::default()
        };
        hote.agir(Action::Sortir(0)); // l'hôte, en position 0
        assert_eq!(hote.salle.as_ref().unwrap().membres.len(), 2, "l'hôte s'est exclu lui-même");
        hote.agir(Action::Sortir(1));
        assert_eq!(hote.salle.as_ref().unwrap().membres.len(), 1, "l'hôte doit pouvoir exclure");
    }

    /// Le code tiré au sort évite les caractères qui s'entendent pareil quand on le dicte.
    #[test]
    fn le_code_evite_les_caracteres_ambigus_a_l_oral() {
        let mut l = Lobby::default();
        let mut vus = std::collections::HashSet::new();
        for _ in 0..40 {
            l.agir(Action::BasculeCode);
            let c = l.creation.code.clone().expect("un code est tiré");
            assert_eq!(c.chars().count(), CODE_LEN);
            for ch in c.chars() {
                assert!(!"IO01".contains(ch), "« {ch} » s'entend comme un autre caractère");
            }
            vus.insert(c);
            l.agir(Action::BasculeCode);
            assert!(l.creation.code.is_none(), "la bascule doit refermer la table");
        }
        assert!(vus.len() > 35, "seulement {} codes distincts sur 40", vus.len());
    }

    /// Rejoindre n'invente PAS une entrée : tant que le pont réseau n'est pas là, basculer vers la
    /// salle ferait croire qu'on est entré alors qu'on a seulement frappé.
    #[test]
    fn rejoindre_n_invente_pas_une_entree() {
        let mut l = Lobby { vue: VueLobby::Liste, ..Default::default() };
        l.agir(Action::Rejoindre(0));
        assert_eq!(l.vue, VueLobby::Liste);
        assert!(l.salle.is_none());
    }
}
