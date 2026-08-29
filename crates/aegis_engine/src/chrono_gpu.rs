//! # LE CHRONOMÈTRE GPU — mesurer le temps EXÉCUTÉ, pas le travail soumis
//!
//! Né le 29 août 2026, en tête du chantier du rendu, et l'ordre n'est pas négociable : sans
//! instrument, on optimise à l'aveugle. Son jumeau [`crate::mesure`] compte le travail **soumis**
//! (appels de dessin, triangles) et **dit lui-même ce qu'il ne voit pas** : le remplissage, et le
//! temps que le GPU passe réellement dans chaque partie de l'image. Ce fichier comble exactement ce
//! trou-là.
//!
//! ## Pourquoi les deux instruments coexistent au lieu d'en fusionner un
//!
//! Ils ne mesurent pas la même nature de grandeur, et les confondre est précisément l'erreur que
//! le projet a déjà payée sur le globe.
//!
//! - Le **travail soumis** est *déterministe et portable* : même scène, même compte, sur n'importe
//!   quelle machine. Il se compare d'une version à l'autre et d'un appareil à l'autre.
//! - Le **temps GPU** est *vrai mais local* : il décrit ce processeur graphique, ce pilote, ce jour.
//!   Il ne se cite jamais comme une propriété du moteur.
//!
//! Le second est indispensable malgré ça, parce que **l'éclairage est un travail de GPU** : il ne
//! se voit pas du tout dans le nombre d'appels de dessin. Une passe d'ombres coûteuse et une passe
//! d'ombres gratuite soumettent exactement le même travail.
//!
//! ## Le budget qui donne un sens aux chiffres rendus ici
//!
//! La machine de référence du projet est le **Meta Quest 2** : deux yeux, 72 Hz, soit **13,9 ms
//! pour l'image entière**. Ce n'est pas une intuition mais un calcul (`prive/aegis/PLAN-RENDU.md`) :
//! par pixel, ce casque dispose d'environ 2,3× moins de calcul qu'un téléphone à 99 $. Un chiffre
//! rendu par ce chronomètre se lit donc toujours contre ce budget-là — *jamais contre les 16,6 ms
//! de la machine de développement, qui sont une facilité, pas une cible.*
//!
//! ## Les trois pièges de cette mesure, et comment ils sont fermés ici
//!
//! 1. **Le GPU travaille en différé.** Interroger un compteur juste après avoir encodé les
//!    commandes ne mesure que la file d'attente. Ici, la lecture porte toujours sur l'image
//!    **précédente**, dont [`crate::core::gpu_context::GpuContext::begin_frame`] vient d'attendre
//!    la barrière (`wait_for_fences`) — donc elle est terminée, et le résultat est disponible sans
//!    aucune attente supplémentaire. *Cette architecture à une seule image en vol est ce qui rend
//!    l'instrument simple ; si elle changeait, ce fichier devrait changer avec elle.*
//! 2. **Le compteur ne fait pas 64 bits sur toutes les machines.** `timestampValidBits` peut valoir
//!    30 ou 36 : au-delà, le compteur repasse à zéro et une soustraction naïve rend une durée
//!    absurde — souvent énorme, parfois négative. Le calcul masque donc les bits non significatifs.
//! 3. **Certaines files ne savent pas horodater du tout** (`timestampValidBits == 0`). Dans ce cas
//!    l'instrument **refuse de naître** plutôt que de rendre des zéros : un banc qui répond « 0 ms »
//!    quand il ne sait pas mesurer est pire qu'un banc absent, parce qu'on le croit.
//!
//! ## Ce qu'il ne mesure toujours PAS
//!
//! Le **remplissage** (combien de pixels sont réellement peints) reste invisible : il demande des
//! requêtes de statistiques de pipeline, une autre famille de compteurs. C'est pourtant lui qui
//! étrangle un casque, où la même scène se dessine deux fois. *Une mesure dont on connaît l'angle
//! mort vaut mieux qu'une mesure qu'on croit complète.*

use ash::vk;
use std::cell::{Cell, RefCell};

/// Une étape mesurée : le temps écoulé entre son jalon et le jalon précédent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Etape {
    pub nom: &'static str,
    pub millisecondes: f32,
}

/// Convertit des horodatages bruts en durées nommées.
///
/// **Séparée de Vulkan exprès, et c'est tout l'intérêt du fichier** : c'est ici que vivent les
/// deux calculs qui peuvent être faux (le masque de bits et la conversion de période), et cette
/// fonction se teste sans GPU, sans fenêtre et sans pilote. Le reste du fichier n'est que de la
/// plomberie que le compilateur vérifie.
///
/// `noms` porte l'étiquette de chaque jalon *à partir du second* : le premier jalon marque le début
/// de l'image et ne clôt aucune étape. On attend donc `ticks.len() == noms.len() + 1`.
///
/// - `periode_ns` : nanosecondes par unité de compteur (`VkPhysicalDeviceLimits::timestampPeriod`).
/// - `bits_valides` : `timestampValidBits` de la file employée. `0` rend une liste vide — la file
///   ne sait pas horodater, et rendre `0.0 ms` serait mentir.
pub fn durees(
    noms: &[&'static str],
    ticks: &[u64],
    periode_ns: f32,
    bits_valides: u32,
) -> Vec<Etape> {
    if bits_valides == 0 || noms.is_empty() || ticks.len() < 2 {
        return Vec::new();
    }

    // Au-delà de `bits_valides`, les bits hauts n'ont aucun sens : le compteur repasse à zéro
    // dessous. On soustrait donc en anneau, puis on ne garde que la partie significative — ce qui
    // rend le calcul correct AUSSI quand le compteur a débordé pendant l'image.
    let masque = if bits_valides >= 64 {
        u64::MAX
    } else {
        (1u64 << bits_valides) - 1
    };

    let combien = noms.len().min(ticks.len() - 1);
    let mut etapes = Vec::with_capacity(combien);
    for i in 0..combien {
        let delta = ticks[i + 1].wrapping_sub(ticks[i]) & masque;
        // f64 pour la conversion : un compteur 64 bits dépasse largement la précision d'un f32,
        // et l'erreur se verrait sur les durées courtes — celles qui nous intéressent le plus.
        let ms = delta as f64 * f64::from(periode_ns) / 1_000_000.0;
        etapes.push(Etape {
            nom: noms[i],
            millisecondes: ms as f32,
        });
    }
    etapes
}

/// Ce qu'une étape a coûté sur les images observées : sa moyenne, son pire cas, et le nombre
/// d'images qui l'ont vue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cumul {
    pub nom: &'static str,
    pub moyenne_ms: f32,
    pub pic_ms: f32,
    pub images: u32,
}

/// L'agrégation des relevés image après image.
///
/// **Elle existe parce que le relevé d'une seule image ne permet de comparer RIEN.** Mesuré le
/// 29 août sur la scène de départ : la même étape « fond » rendait 0,022 ms puis 0,462 ms d'une
/// image à l'autre — un facteur 20, alors que rien n'avait changé dans le moteur. Un banc dont le
/// bruit dépasse l'effet qu'on cherche est un banc dont on ne peut tirer aucune conclusion, et
/// c'est exactement la faute du micro-banc du globe : des gains imaginaires, trois mécanismes
/// empilés pour rattraper une prémisse fausse.
///
/// ⚠ **Le pic est gardé à côté de la moyenne, et il n'est pas décoratif.** Sur un casque, une seule
/// image ratée se voit et se ressent ; une moyenne confortable qui cache un pic à 20 ms décrit un
/// rendu agréable qui donne la nausée. Les deux chiffres se lisent ensemble, jamais l'un sans l'autre.
///
/// ## ⚠⚠ LA SONDE QUI MENT, ET C'EST LUI QUI L'A VUE (29 août 2026)
///
/// Sa remarque : *« si un programme visible en fenêtre fait un calcul mais que je vais sur une
/// autre fenêtre, ça va complètement freeze le jeu »*. Mesuré, et c'est pire que ça :
///
/// | état de la fenêtre | images en 6 s | coût moyen mesuré |
/// |---|---|---|
/// | visible | 996 (165 img/s) | 0,222 ms |
/// | **masquée** (autre espace de travail) | **11 (≈2 img/s)** | **0,841 ms** |
///
/// Le compositeur cesse d'envoyer ses invitations à dessiner : le rendu s'arrête presque
/// complètement. **Et le piège n'est pas le gel — c'est que les durées GONFLENT d'un facteur 4.**
/// Entre deux images espacées d'une demi-seconde, le GPU redescend ses horloges et vide ses
/// caches ; chaque image repart à froid. Une mesure prise fenêtre masquée fait donc paraître le
/// rendu **quatre fois plus coûteux** qu'il ne l'est — elle ment dans le sens qui trompe le plus,
/// celui qui ferait « optimiser » un code qui n'a aucun problème.
///
/// **La garde n'est pas un avertissement à lire, c'est un chiffre qui rend le défaut visible :**
/// la cadence est rendue avec chaque relevé. Onze images en six secondes se repère d'un coup d'œil
/// là où « 0,841 ms » se croit sans hésiter. *(Une première tentative de ce test a été faussée
/// parce qu'il a regardé l'espace de travail où je venais d'envoyer le jeu — d'où le contrôle
/// explicite dans le scénario, et la mesure refaite.)*
pub struct Historique {
    /// nom, somme des durées, pire durée vue, nombre d'images
    lignes: Vec<(&'static str, f64, f32, u32)>,
    /// Depuis quand on agrège — pour rendre la cadence, seul témoin d'une fenêtre bridée.
    depuis: std::time::Instant,
}

impl Default for Historique {
    fn default() -> Self {
        Self { lignes: Vec::new(), depuis: std::time::Instant::now() }
    }
}

impl Historique {
    /// Verse le relevé d'une image. Les étapes sont reconnues par leur nom : une étape qui
    /// n'apparaît que dans certaines images (un écran qui s'ouvre) garde donc un compte propre,
    /// et sa moyenne n'est pas diluée par les images où elle n'existait pas.
    pub fn verser(&mut self, etapes: &[Etape]) {
        for e in etapes {
            match self.lignes.iter_mut().find(|l| l.0 == e.nom) {
                Some(l) => {
                    l.1 += f64::from(e.millisecondes);
                    l.2 = l.2.max(e.millisecondes);
                    l.3 += 1;
                }
                None => self.lignes.push((e.nom, f64::from(e.millisecondes), e.millisecondes, 1)),
            }
        }
    }

    /// Les cumuls, dans l'ordre où les étapes sont apparues pour la première fois.
    pub fn cumuls(&self) -> Vec<Cumul> {
        self.lignes
            .iter()
            .map(|&(nom, somme, pic, images)| Cumul {
                nom,
                moyenne_ms: if images == 0 { 0.0 } else { (somme / f64::from(images)) as f32 },
                pic_ms: pic,
                images,
            })
            .collect()
    }

    /// Le nombre d'images versées — celui de l'étape la plus vue.
    ///
    /// ⚠ **À lire AVANT toute conclusion.** Une moyenne sur trois images n'est pas une mesure ;
    /// la règle des trois N du projet vaut aussi ici, et il vaut mieux voir ce compte que le deviner.
    pub fn images(&self) -> u32 {
        self.lignes.iter().map(|l| l.3).max().unwrap_or(0)
    }

    /// Les images par seconde observées depuis la dernière remise à zéro.
    ///
    /// **À regarder AVANT le coût.** Une cadence effondrée (quelques images par seconde) signale
    /// une fenêtre masquée ou minimisée : le relevé qui l'accompagne est alors sans valeur, et
    /// surestime le coût. Rend `0.0` tant que rien n'a été observé.
    pub fn cadence(&self) -> f32 {
        let s = self.depuis.elapsed().as_secs_f32();
        if s <= 0.0 { 0.0 } else { self.images() as f32 / s }
    }

    pub fn remettre_a_zero(&mut self) {
        self.lignes.clear();
        self.depuis = std::time::Instant::now();
    }
}

/// Le chronomètre attaché à un GPU : un jeu de compteurs, et les durées de la dernière image finie.
pub struct ChronoGpu {
    pool: vk::QueryPool,
    capacite: u32,
    periode_ns: f32,
    bits_valides: u32,
    // ⚠ MUTABILITÉ INTÉRIEURE, ET C'EST UNE DÉCISION DÉJÀ PRISE PAR CE PROJET.
    //
    // Poser un jalon ne demande qu'un `&self`, alors que ça modifie ces compteurs. L'alternative
    // — faire remonter un `&mut` de chronomètre à travers toute la chaîne de rendu — alourdirait
    // chaque signature pour une donnée qui n'intéresse que l'instrumentation. C'est mot pour mot
    // l'arbitrage que `mesure.rs` a tranché en choisissant des compteurs globaux ; ici le
    // chronomètre possède des ressources Vulkan, donc il ne peut pas être global, mais la raison
    // est la même et la réponse doit l'être aussi.
    //
    // Sûr parce que l'encodage des commandes d'une image est mono-fil et sans récursion. Si ce
    // moteur encodait un jour ses commandes depuis plusieurs fils, c'est CE commentaire qu'il
    // faudra venir contredire — pas le découvrir dans un `already borrowed`.
    /// Les étiquettes des jalons de l'image en cours d'encodage.
    noms_en_cours: RefCell<Vec<&'static str>>,
    /// Le nombre de jalons réellement écrits dans l'image en cours.
    ecrits: Cell<u32>,
    /// Vrai dès qu'une image complète attend d'être lue.
    lecture_en_attente: Cell<bool>,
    /// Compté et journalisé une seule fois : un avertissement par image noierait le journal.
    debordements: Cell<u32>,
    derniere_image: Vec<Etape>,
    historique: Historique,
}

impl ChronoGpu {
    /// Crée le chronomètre, ou explique pourquoi il ne peut pas exister.
    ///
    /// ⚠ Rend `Err` si la file ne sait pas horodater. **Ne pas transformer ce refus en instrument
    /// muet** : c'est le refus lui-même qui porte l'information.
    pub fn nouveau(
        device: &ash::Device,
        periode_ns: f32,
        bits_valides: u32,
        capacite: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if bits_valides == 0 {
            return Err("cette file de commandes ne sait pas horodater (timestampValidBits = 0)".into());
        }
        if periode_ns <= 0.0 {
            return Err("periode d'horodatage invalide (timestampPeriod <= 0)".into());
        }

        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(capacite);
        let pool = unsafe { device.create_query_pool(&info, None)? };

        log::info!(
            "Chronometre GPU : {capacite} jalons, {periode_ns} ns/unite, {bits_valides} bits utiles"
        );

        Ok(Self {
            pool,
            capacite,
            periode_ns,
            bits_valides,
            noms_en_cours: RefCell::new(Vec::with_capacity(capacite as usize)),
            ecrits: Cell::new(0),
            lecture_en_attente: Cell::new(false),
            debordements: Cell::new(0),
            derniere_image: Vec::new(),
            historique: Historique::default(),
        })
    }

    /// À appeler une fois par image, **après** l'attente de la barrière et **avant** tout jalon.
    ///
    /// Fait trois choses dans cet ordre, et l'ordre compte : elle relève l'image précédente (le GPU
    /// l'a finie, donc les compteurs sont lisibles sans attendre), elle remet les compteurs à zéro
    /// pour l'image qui commence, puis elle pose le jalon d'ouverture.
    pub fn ouvrir_image(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        if self.lecture_en_attente.get() {
            self.relever(device);
        }

        unsafe {
            device.cmd_reset_query_pool(cmd, self.pool, 0, self.capacite);
        }
        self.noms_en_cours.borrow_mut().clear();
        self.ecrits.set(0);
        self.lecture_en_attente.set(false);

        self.ecrire(device, cmd);
    }

    /// Referme l'étape courante et lui donne son nom.
    ///
    /// Le découpage est donc *séquentiel*, pas imbriqué : chaque jalon clôt ce qui le précède
    /// depuis le jalon d'avant. C'est un choix — des zones imbriquées demanderaient deux compteurs
    /// par zone et une pile, pour une information que ce moteur n'a pas encore besoin de lire.
    pub fn jalon(&self, device: &ash::Device, cmd: vk::CommandBuffer, nom: &'static str) {
        if self.ecrits.get() >= self.capacite {
            self.debordements.set(self.debordements.get() + 1);
            if self.debordements.get() == 1 {
                log::warn!(
                    "Chronometre GPU : plus de {} jalons dans une image, les suivants sont ignores",
                    self.capacite
                );
            }
            return;
        }
        self.noms_en_cours.borrow_mut().push(nom);
        self.ecrire(device, cmd);
        self.lecture_en_attente.set(true);
    }

    /// Les durées de la dernière image entièrement terminée. Vide tant qu'aucune ne l'est.
    ///
    /// ⚠ **Une image ne prouve rien** — voir [`Historique`] pour ce qui se compare.
    pub fn etapes(&self) -> &[Etape] {
        &self.derniere_image
    }

    /// Ce que chaque étape coûte en moyenne, et son pire cas, depuis la dernière remise à zéro.
    /// **C'est ce relevé-ci qui sert à comparer deux versions du moteur.**
    pub fn cumuls(&self) -> Vec<Cumul> {
        self.historique.cumuls()
    }

    /// Le nombre d'images agrégées — à regarder avant de conclure quoi que ce soit.
    pub fn images_agregees(&self) -> u32 {
        self.historique.images()
    }

    /// Les images par seconde observées. Voir [`Historique::cadence`] : c'est le témoin qui dit si
    /// le relevé vaut quelque chose.
    pub fn cadence(&self) -> f32 {
        self.historique.cadence()
    }

    /// Repart de zéro, pour mesurer une phase précise plutôt que depuis le lancement.
    pub fn remettre_a_zero(&mut self) {
        self.historique.remettre_a_zero();
    }

    /// Le total des étapes relevées — le coût GPU de l'image, tel que ce découpage le voit.
    pub fn total_ms(&self) -> f32 {
        self.derniere_image.iter().map(|e| e.millisecondes).sum()
    }

    /// Le budget du Meta Quest 2 à 72 Hz, pour deux yeux : la seule échéance qui décide sur ce
    /// projet. Rendue ici pour qu'aucun appelant n'ait à la recopier — une constante recopiée
    /// diverge, c'est une leçon déjà payée.
    pub const BUDGET_QUEST2_MS: f32 = 1000.0 / 72.0;

    /// La part du budget Quest 2 que consomme la dernière image mesurée, en pourcentage.
    ///
    /// ⚠ **C'est une mise en perspective, pas une prédiction.** Ce chiffre est mesuré sur CETTE
    /// machine : il ne dit pas ce que coûterait la même image sur un casque, seulement combien de
    /// l'échéance elle mangerait si le matériel était identique — ce qu'il n'est pas.
    pub fn part_du_budget_quest2(&self) -> f32 {
        self.total_ms() / Self::BUDGET_QUEST2_MS * 100.0
    }

    /// Libère le jeu de compteurs. À appeler avant de détruire le device.
    pub fn detruire(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_query_pool(self.pool, None);
        }
    }

    fn ecrire(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        unsafe {
            // BOTTOM_OF_PIPE : « tout ce qui précède est terminé ». C'est la seule sémantique qui
            // donne des étapes séquentielles justes ; un stage plus haut mesurerait l'entrée dans
            // le pipeline et non la fin du travail.
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.pool,
                self.ecrits.get(),
            );
        }
        self.ecrits.set(self.ecrits.get() + 1);
    }

    fn relever(&mut self, device: &ash::Device) {
        if self.ecrits.get() < 2 {
            return;
        }
        let mut bruts = vec![0u64; self.ecrits.get() as usize];
        // Sans `WAIT` : la barrière de l'image a déjà été attendue par l'appelant, donc les
        // résultats sont là. S'ils ne l'étaient pas, on garde le relevé précédent plutôt que de
        // bloquer le rendu pour une mesure — un instrument ne doit jamais dégrader ce qu'il observe.
        let issue = unsafe {
            device.get_query_pool_results(
                self.pool,
                0,
                &mut bruts,
                vk::QueryResultFlags::TYPE_64,
            )
        };
        if issue.is_ok() {
            self.derniere_image = durees(
                &self.noms_en_cours.borrow(),
                &bruts,
                self.periode_ns,
                self.bits_valides,
            );
            self.historique.verser(&self.derniere_image);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le cas nominal : trois jalons délimitent deux étapes.
    #[test]
    fn deux_jalons_encadrent_une_etape() {
        // 1 ns par unité : les unités valent directement des nanosecondes.
        let e = durees(&["fond", "monde"], &[0, 1_000_000, 3_500_000], 1.0, 64);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].nom, "fond");
        assert!((e[0].millisecondes - 1.0).abs() < 1e-4, "{:?}", e[0]);
        assert!((e[1].millisecondes - 2.5).abs() < 1e-4, "{:?}", e[1]);
    }

    /// La période n'est pas toujours de 1 ns — sur beaucoup de GPU elle vaut 38,4 ou 1,0.
    #[test]
    fn la_periode_du_gpu_est_appliquee() {
        let e = durees(&["passe"], &[0, 26_042], 38.4, 64);
        // 26 042 unites x 38,4 ns = 1 000 013 ns, soit 1 ms a 1,3 e-5 pres.
        assert!((e[0].millisecondes - 1.0).abs() < 0.01, "{:?}", e[0]);
    }

    /// ⚠ LE PIÈGE QUI JUSTIFIE CETTE FONCTION : le compteur repasse à zéro à `bits_valides`.
    ///
    /// Sans masque, la soustraction rendrait ici une durée gigantesque au lieu d'une microseconde —
    /// et le relevé désignerait comme coupable la passe la plus innocente de l'image.
    #[test]
    fn un_compteur_qui_deborde_ne_rend_pas_une_duree_absurde() {
        let bits = 32;
        let plafond = 1u64 << bits;
        // L'image commence juste avant le débordement et finit 1000 unités après.
        let debut = plafond - 400;
        let fin = 600; // le compteur est repassé à zéro entre les deux
        let e = durees(&["a_cheval"], &[debut, fin], 1.0, bits);
        assert!(
            (e[0].millisecondes - 0.001).abs() < 1e-6,
            "1000 ns attendues, obtenu {:?}",
            e[0]
        );

        // Et la garde mord : en lisant les mêmes valeurs sur 64 bits, la durée devient absurde.
        let sans_masque = durees(&["a_cheval"], &[debut, fin], 1.0, 64);
        assert!(
            sans_masque[0].millisecondes > 1e6,
            "le test ne prouverait rien si le cas naif ne cassait pas : {:?}",
            sans_masque[0]
        );
    }

    /// Une file qui ne sait pas horodater ne doit pas rendre « 0 ms » — ce serait un mensonge
    /// qu'on croirait.
    #[test]
    fn une_file_sans_horodatage_ne_rend_rien_du_tout() {
        assert!(durees(&["passe"], &[0, 1_000_000], 1.0, 0).is_empty());
    }

    /// Robustesse : moins d'horodatages que de noms (un jalon perdu, un pool trop petit) ne doit
    /// ni paniquer ni inventer une étape.
    #[test]
    fn des_noms_sans_horodatage_sont_ignores_sans_paniquer() {
        let e = durees(&["a", "b", "c"], &[0, 1_000_000], 1.0, 64);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].nom, "a");

        assert!(durees(&[], &[0, 1, 2], 1.0, 64).is_empty());
        assert!(durees(&["a"], &[7], 1.0, 64).is_empty());
        assert!(durees(&["a"], &[], 1.0, 64).is_empty());
    }

    /// L'agrégation moyenne, garde le pire cas, et compte ses images.
    #[test]
    fn l_historique_moyenne_et_retient_le_pire() {
        let mut h = Historique::default();
        assert_eq!(h.images(), 0);
        assert!(h.cumuls().is_empty());

        h.verser(&[
            Etape { nom: "fond", millisecondes: 1.0 },
            Etape { nom: "monde", millisecondes: 4.0 },
        ]);
        h.verser(&[
            Etape { nom: "fond", millisecondes: 3.0 },
            Etape { nom: "monde", millisecondes: 2.0 },
        ]);

        let c = h.cumuls();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].nom, "fond");
        assert!((c[0].moyenne_ms - 2.0).abs() < 1e-5, "{:?}", c[0]);
        assert!((c[0].pic_ms - 3.0).abs() < 1e-5, "le pire cas ne doit pas etre lisse : {:?}", c[0]);
        assert_eq!(c[0].images, 2);
        assert!((c[1].moyenne_ms - 3.0).abs() < 1e-5, "{:?}", c[1]);
        assert!((c[1].pic_ms - 4.0).abs() < 1e-5, "{:?}", c[1]);
        assert_eq!(h.images(), 2);

        // La cadence existe des qu'il y a des images, et repart a zero avec le reste.
        assert!(h.cadence() > 0.0, "deux images versees, la cadence ne peut pas etre nulle");

        h.remettre_a_zero();
        assert_eq!(h.images(), 0);
        assert!(h.cumuls().is_empty());
        assert_eq!(h.cadence(), 0.0, "sans image, pas de cadence — et surtout pas de division");
    }

    /// ⚠ Le cas qui justifie de compter les images PAR ÉTAPE : un écran qui s'ouvre à mi-parcours
    /// ne doit pas voir sa moyenne divisée par les images où il n'existait pas.
    #[test]
    fn une_etape_intermittente_garde_sa_propre_moyenne() {
        let mut h = Historique::default();
        h.verser(&[Etape { nom: "monde", millisecondes: 2.0 }]);
        h.verser(&[Etape { nom: "monde", millisecondes: 2.0 }]);
        h.verser(&[
            Etape { nom: "monde", millisecondes: 2.0 },
            Etape { nom: "lobby", millisecondes: 6.0 },
        ]);

        let c = h.cumuls();
        let lobby = c.iter().find(|c| c.nom == "lobby").expect("le lobby doit exister");
        assert_eq!(lobby.images, 1, "le lobby n'a ete vu qu'une fois");
        assert!(
            (lobby.moyenne_ms - 6.0).abs() < 1e-5,
            "6 ms sur 1 image, pas 2 ms sur 3 : {lobby:?}"
        );
        assert_eq!(h.images(), 3, "le compte global suit l'etape la plus vue");
    }

    /// Le budget de référence du projet est celui du casque, pas celui de l'écran de dev.
    #[test]
    fn le_budget_de_reference_est_celui_du_quest_2() {
        assert!(
            (ChronoGpu::BUDGET_QUEST2_MS - 13.888).abs() < 0.01,
            "72 Hz pour deux yeux = 13,9 ms, obtenu {}",
            ChronoGpu::BUDGET_QUEST2_MS
        );
    }
}
