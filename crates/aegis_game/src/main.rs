#![allow(dead_code, unused_variables)]

mod grid;
mod traps;
mod particles;
mod player;
mod mls_mpm;
mod mystery_box;
mod party_game;
mod party_render_pass;
mod hud;
mod tas;
mod vote;
mod entraide;
mod console;
mod boite_noire;
mod objects;
mod nav_client;
mod sidecar_client;
mod plantage;
mod web3_integration;

use std::sync::Arc;
use aegis_engine::Engine;
use nav_client::NavClient;
use sidecar_client::SidecarClient;
use party_game::PartyGame;
use party_render_pass::PartyRenderPass;
use player::InputState;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

// X11 est une API de Linux : l'importer sans condition rendait le jeu INCOMPILABLE sous Windows —
// mesuré le 12 août 2026, à la première tentative de compilation croisée. Ce n'était pas un choix,
// personne n'avait encore essayé. Or c'est là que sont les 39 personnes qui doivent y jouer.
#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;

struct AegisApp {
    window: Option<Arc<Window>>,
    engine: Option<Engine>,
    party_render_pass: Option<PartyRenderPass>,
    party_game: PartyGame,
    input_state: InputState,
    screenshot_mode: bool,
    screenshot_path: String,
    /// L'image à laquelle la capture est prise (`--frame`).
    screenshot_frame: u64,
    /// Position de la souris en pixels (pour la sélection d'items par clic)
    mouse_pos: (f32, f32),
    /// Le pont vers le launcher web3 (régisseur de bascule). Inerte si le jeu est lancé tout seul.
    nav: NavClient,
    /// Le pont vers le cœur réseau web3 : on y pousse notre position, on en lit celle des autres.
    /// Inerte si le cœur ne tourne pas — le jeu reste alors exactement le jeu solo.
    sidecar: SidecarClient,
    /// Le solveur, qui vérifie en tâche de fond que la carte piégée reste franchissable.
    verificateur: tas::Verificateur,
    /// Le vote en cours sur le retrait d'un bloc qui bouche — `None` la plupart du temps.
    vote: Option<vote::Vote>,
    /// La console de pilotage (inerte sans `AEGIS_CONSOLE`).
    pupitre: std::sync::Arc<console::Pupitre>,
    /// L'enregistreur de parties (inerte sans `AEGIS_BOITE_NOIRE`).
    boite: boite_noire::BoiteNoire,
    /// Touches à relâcher à la prochaine image : un `appui` console est un FRONT, pas un maintien.
    a_relacher: Vec<String>,
    /// La phase du tour précédent, pour ne lancer la vérification qu'aux transitions.
    phase_precedente: party_game::GamePhase,
    /// La démonstration du parcours, montrée quand personne n'a franchi la ligne.
    demonstration: Option<tas::Demonstration>,
}

impl AegisApp {
    fn new(screenshot_mode: bool, screenshot_path: String, screenshot_frame: u64) -> Self {
        Self {
            window: None,
            engine: None,
            party_render_pass: None,
            party_game: PartyGame::new(48, 24),
            input_state: InputState::default(),
            screenshot_mode,
            screenshot_path,
            screenshot_frame,
            mouse_pos: (0.0, 0.0),
            // On s'annonce AVANT d'ouvrir la fenêtre et d'initialiser Vulkan : le régisseur tient son
            // rideau baissé tant qu'aucune image n'est prouvée, et il vaut mieux qu'il sache tout de
            // suite qu'on est en train de démarrer plutôt que de nous croire morts.
            nav: NavClient::connecter("aegis"),
            // Comme le régisseur : on se relie AVANT d'ouvrir la fenêtre. Le cœur tourne en
            // continu de son côté, il n'attend rien de nous — mais s'il est là, autant que la
            // première position poussée soit la toute première du jeu.
            sidecar: SidecarClient::connecter(),
            verificateur: tas::Verificateur::nouveau(),
            vote: None,
            pupitre: console::ouvrir(),
            boite: boite_noire::BoiteNoire::nouvelle(),
            a_relacher: Vec::new(),
            phase_precedente: party_game::GamePhase::Drafting,
            demonstration: None,
        }
    }

    /// Traite un clic gauche en phase Draft ou Placement.
    fn handle_left_click(&mut self) {
        match self.party_game.phase {
            party_game::GamePhase::Drafting => {
                if let (Some(engine), Some(render_pass)) =
                    (self.engine.as_ref(), self.party_render_pass.as_ref())
                {
                    let w = engine.gpu.swapchain_extent.width as f32;
                    let h = engine.gpu.swapchain_extent.height as f32;
                    let (mx, my) = self.mouse_pos;
                    let total = self.party_game.mystery_box.available_items.len();
                    if total == 0 { return; }

                    let aspect = w / h;
                    let view = aegis_engine::math::Mat4::look_at_rh(
                        render_pass.camera_pos,
                        render_pass.camera_target,
                        aegis_engine::math::Vec3::Y,
                    );
                    let proj = aegis_engine::math::Mat4::perspective_rh(
                        38.0f32.to_radians(), aspect, 0.1, 500.0,
                    );
                    let vp = proj * view;

                    let box_pos = aegis_engine::math::Vec3::new(
                        self.party_game.grid.width as f32 / 2.0,
                        self.party_game.grid.height as f32 / 2.0,
                        0.0,
                    );
                    let t = self.party_game.round_timer;

                    let mut best_idx = None;
                    let mut min_dist_sq = 90.0 * 90.0; // Rayon de clic généreux de 90 pixels

                    for i in 0..total {
                        let (offset_vec, _) = crate::mystery_box::compute_box_item_offset(i, total);
                        let item_world = box_pos + offset_vec;

                        let clip = vp * aegis_engine::math::Vec4::new(item_world.x, item_world.y, item_world.z, 1.0);
                        if clip.w > 0.0 {
                            let ndc_x = clip.x / clip.w;
                            let ndc_y = clip.y / clip.w;
                            let sx = (ndc_x + 1.0) * 0.5 * w;
                            let sy = (1.0 - ndc_y) * 0.5 * h;
                            let dx = mx - sx;
                            let dy = my - sy;
                            let dist_sq = dx * dx + dy * dy;
                            if dist_sq < min_dist_sq {
                                min_dist_sq = dist_sq;
                                best_idx = Some(i);
                            }
                        }
                    }

                    if let Some(idx) = best_idx {
                        self.party_game.mystery_box.select_item(idx);
                        // Validation directe de l'objet par clic -> Passage immédiat en phase Placement !
                        self.party_game.phase = party_game::GamePhase::Placement;
                        let (gw, gh) = (self.party_game.grid.width, self.party_game.grid.height);
                        self.party_game.editor.cursor = ((gw / 2) as i32, (gh / 2) as i32);
                        log::info!("🖱️ Clic 3D → Item {} ({}) CHOISI ! Passage immédiat en Phase Placement.", idx, self.party_game.mystery_box.available_items[idx].name());
                    }
                }
            }
            party_game::GamePhase::Placement => {
                // Convertir la position souris en coordonnées grille
                if let (Some(engine), Some(render_pass)) =
                    (self.engine.as_ref(), self.party_render_pass.as_ref())
                {
                    let (mx, my) = self.mouse_pos;
                    let w = engine.gpu.swapchain_extent.width as f32;
                    let h = engine.gpu.swapchain_extent.height as f32;
                    // Coordonnées NDC [-1, 1]
                    let ndc_x = (mx / w) * 2.0 - 1.0;
                    let ndc_y = 1.0 - (my / h) * 2.0;
                    // Matrice VP inverse pour retrouver la position monde
                    let aspect = w / h;
                    let view = aegis_engine::math::Mat4::look_at_rh(
                        render_pass.camera_pos,
                        render_pass.camera_target,
                        aegis_engine::math::Vec3::Y,
                    );
                    let proj = aegis_engine::math::Mat4::perspective_rh(
                        38.0f32.to_radians(), aspect, 0.1, 500.0,
                    );
                    let vp = proj * view;
                    let inv_vp = vp.inverse();
                    {
                        // Ray depuis NDC (Z=0 = near plane)
                        let near = inv_vp * aegis_engine::math::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
                        let far  = inv_vp * aegis_engine::math::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
                        let near_w = aegis_engine::math::Vec3::new(near.x / near.w, near.y / near.w, near.z / near.w);
                        let far_w  = aegis_engine::math::Vec3::new(far.x  / far.w,  far.y  / far.w,  far.z  / far.w);
                        // Intersection avec le plan Z=0 (la map)
                        let dz = far_w.z - near_w.z;
                        if dz.abs() > 1e-6 {
                            let t = -near_w.z / dz;
                            let world_x = near_w.x + t * (far_w.x - near_w.x);
                            let world_y = near_w.y + t * (far_w.y - near_w.y);
                            let gx = world_x.floor() as usize;
                            let gy = world_y.floor() as usize;
                            if gx < self.party_game.grid.width && gy < self.party_game.grid.height {
                                self.party_game.editor.cursor = (gx as i32, gy as i32);
                                self.party_game.placement_place_request = true;
                                log::info!("🖱️ Clic Placement → case ({}, {})", gx, gy);
                            }

                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Traduit une touche nommée par la console en entrée de jeu.
    ///
    /// Les noms sont ceux qu'on tape dans un terminal, pas ceux du clavier : `gauche` plutôt que
    /// `KeyQ`. Une console de test qui exigerait des noms de touches physiques serait inutilisable
    /// sur un clavier différent — et le projet vise déjà AZERTY, QWERTY et les flèches.
    fn touche_console(&mut self, nom: &str, enfoncee: bool) {
        match nom {
            "gauche" => self.input_state.left = enfoncee,
            "droite" => self.input_state.right = enfoncee,
            "bas" => {
                self.input_state.down = enfoncee;
                self.input_state.crouch = enfoncee;
            }
            "saut" | "haut" => {
                self.input_state.up = enfoncee;
                // Le front montant, exactement comme au clavier : `jump_pressed_this_frame` est
                // remis à faux par la boucle après chaque image.
                if enfoncee {
                    self.input_state.jump_pressed_this_frame = true;
                }
            }
            _ => log::warn!("[console] touche inconnue : {nom}"),
        }
    }

}

/// L'instantané que la console lit. Construit une fois par image, jamais à la demande : une
/// lecture qui traverserait le jeu pendant qu'il travaille prendrait des verrous au mauvais moment.
///
/// Fonction LIBRE et non méthode : Rust autorise les emprunts de champs disjoints d'une struct,
/// mais pas à travers un `&self` quand un autre champ est déjà emprunté en `&mut` (le moteur de
/// rendu, ici). Passer les morceaux explicitement dit d'ailleurs mieux ce dont l'état dépend.
#[allow(clippy::too_many_arguments)]
fn publier_etat_console(
    pupitre: &console::Pupitre,
    g: &party_game::PartyGame,
    sidecar: &SidecarClient,
    verificateur: &tas::Verificateur,
    vote: Option<&vote::Vote>,
    demonstration: bool,
) {
    {
        let moi = g.players.iter().find(|p| p.is_human);
        pupitre.publier(console::Etat {
            phase: format!("{:?}", g.phase),
            manche: g.round_number,
            minuteur: g.minuteur_de_phase().0,
            joueurs: g
                .players
                .iter()
                .map(|p| (p.name.clone(), p.total_score, p.has_finished))
                .collect(),
            avatars_distants: sidecar.avatars().len(),
            envoyes: sidecar.compteurs().0,
            recus: sidecar.compteurs().1,
            carte: format!("{:?}", verificateur.etat()),
            bouchon: format!("{:?}", verificateur.bouchon()),
            vote: vote.map(|v| (v.bloc.0, v.bloc.1, v.pour(), v.seuil(), v.reste)),
            position: moi.map(|p| (p.player.position.x, p.player.position.y)).unwrap_or((0.0, 0.0)),
            demonstration,
        });
    }
}

impl AegisApp {

    fn render_frame(&mut self) {
        // ── LA CONSOLE PARLE ICI, ET NULLE PART AILLEURS ────────────────────────────────────
        // Un seul point d'entrée, juste avant la mise à jour : ses ordres suivent exactement le
        // même chemin qu'un appui clavier. Sans quoi on testerait un jeu qui n'est pas celui
        // qu'on joue.
        //
        // Les touches d'un `appui` sont relâchées ICI, une image après avoir été enfoncées : le
        // saut ne se déclenche qu'au FRONT montant, et une touche laissée enfoncée par la console
        // ne sauterait qu'une fois puis resterait morte.
        for nom in std::mem::take(&mut self.a_relacher) {
            self.touche_console(&nom, false);
        }
        for ordre in self.pupitre.prendre_les_ordres() {
            match ordre {
                console::Ordre::Touche { nom, enfoncee } => self.touche_console(&nom, enfoncee),
                console::Ordre::Appui { nom } => {
                    self.touche_console(&nom, true);
                    self.a_relacher.push(nom);
                }
                console::Ordre::Voter { pour } => {
                    if let (Some(v), Some(moi)) = (
                        self.vote.as_mut(),
                        self.party_game.players.iter().find(|p| p.is_human),
                    ) {
                        let b = if pour { vote::Bulletin::Pour } else { vote::Bulletin::Contre };
                        v.voter(moi.id, b);
                    }
                }
                console::Ordre::Capture { chemin } => {
                    self.screenshot_mode = true;
                    self.screenshot_path = chemin;
                    self.screenshot_frame = 0;
                }
                console::Ordre::Quitter => {
                    log::info!("[console] arrêt demandé.");
                    std::process::exit(0);
                }
            }
        }

        let (window, engine, party_render_pass) = match (self.window.as_ref(), self.engine.as_mut(), self.party_render_pass.as_mut()) {
            (Some(w), Some(e), Some(r)) => (w, e, r),
            _ => return,
        };

        let dt = engine.delta_time();

        self.party_game.update(dt, &self.input_state);
        if let Some(moi) = self.party_game.players.iter().find(|p| p.is_human) {
            self.boite.noter(dt, &self.input_state, &moi.player);
        }
        self.input_state.jump_pressed_this_frame = false;
        publier_etat_console(
            &self.pupitre,
            &self.party_game,
            &self.sidecar,
            &self.verificateur,
            self.vote.as_ref(),
            self.demonstration.is_some(),
        );

        // On pousse NOTRE position au cœur, et rien d'autre : lui seul décide ce que les autres
        // en voient. La cadence est plafonnée dans le client, l'appelant n'a pas à la connaître.
        //
        // ⚠ Mais SEULEMENT pendant la course, et seulement vivant. Entre les manches (choix,
        // placement, tableau des scores) le personnage n'est nulle part, et un mort non plus.
        // Se taire n'est pas une économie : c'est ce qui empêche le retour au point de départ de
        // la manche suivante de ressembler à une téléportation. L'anti-triche du cœur n'accepte
        // que 2,5 unités entre deux états à 20 Hz — un revenant était puni et son avatar effacé
        // chez tous les autres (observé en réel le 19 août avec deux jeux côte à côte). Le
        // silence, lui, élargit la borne d'autant, sans rien affaiblir.
        let en_course = self.party_game.phase == party_game::GamePhase::Running;
        let vivant = !self.party_game.players[0].is_dead;
        if en_course && vivant {
            let moi = self.party_game.human_player().position;
            self.sidecar.pousser_ma_pose(moi.x, moi.y, 0.0, 0.0, 0.0);
        }

        // La carte vient d'être piégée : on demande au solveur si elle reste franchissable.
        // C'est au début de la COURSE qu'on le fait, pour que le verdict soit là si personne
        // n'arrive — et qu'on sache alors si c'était la carte ou les joueurs.
        let phase = self.party_game.phase;
        if phase != self.phase_precedente {
            match phase {
                party_game::GamePhase::Running => {
                    self.verificateur
                        .lancer(&self.party_game.grid, &self.party_game.traps);
                    // La carte est figée AU COUP D'ENVOI : les pièges bougent ensuite, et une
                    // carte relevée après coup ne serait plus celle que le joueur a franchie.
                    self.boite.ouvrir_manche(
                        self.party_game.round_number,
                        &self.party_game.grid,
                        &self.party_game.traps,
                    );
                }
                party_game::GamePhase::Drafting => {
                    self.verificateur.oublier();
                    self.demonstration = None;
                    // Une manche neuve : le vote de la précédente n'a plus d'objet.
                    self.vote = None;
                }
                party_game::GamePhase::Leaderboard => {
                    // Fin de course : on écrit, avec le couple qui fait tout l'intérêt du fichier
                    // — l'humain est-il arrivé, et qu'en disait le TAS ?
                    let arrive = self.party_game.players.iter().any(|p| p.is_human && p.has_finished);
                    let verdict = format!("{:?}", self.verificateur.etat());
                    self.boite.fermer_manche(arrive, &verdict);
                    // Personne n'a franchi la ligne : on montre que c'était possible, et
                    // comment. C'est le moment exact où la question se pose.
                    let personne = !self.party_game.players.iter().any(|p| p.has_finished);
                    let a_montrer = if personne { self.verificateur.solution() } else { None };
                    if let Some(solution) = a_montrer {
                        log::info!("🎬 Personne n'a réussi — démonstration du parcours trouvé.");
                        self.demonstration = Some(tas::Demonstration::nouvelle(
                            &self.party_game.grid,
                            solution,
                        ));
                    }
                }
                _ => {}
            }
            self.phase_precedente = phase;
        }

        if let Some(demo) = self.demonstration.as_mut() {
            demo.avancer(dt, &self.party_game.grid, &self.party_game.traps);
        }

        let distants = self.sidecar.avatars();
        let (envoyes, recus, avatars) = self.sidecar.compteurs();
        // ⚠ Lu AVANT la passe de rendu, pas pendant. Prendre ce verrou au milieu de
        // l'enregistrement des commandes graphiques ferait attendre la boucle d'affichage
        // derrière le fil du solveur — une saccade visible, pour une information qui ne change
        // qu'une fois par manche.
        let bouchon = self.verificateur.bouchon();

        // ⚠ VÉRIFICATION VISUELLE. Un vote ne s'ouvre que sur une carte bouchée, ce qui est
        // difficile à provoquer pour regarder le bandeau. Cette variable en ouvre un factice.
        //
        // Elle existe parce que, sur ce projet, **la capture d'écran est la seule sonde qui
        // tranche pour du HUD** : les onze tests du tableau des scores étaient verts pendant qu'il
        // s'affichait à l'envers. Un test prouve la cohérence avec la convention qu'on lui donne,
        // jamais que cette convention est celle du moteur.
        //     AEGIS_DEMO_VOTE=1 aegis_game --screenshot vote.png
        if self.vote.is_none() && std::env::var("AEGIS_DEMO_VOTE").is_ok() {
            self.vote = Some(vote::Vote::ouvrir((12, 4), 35));
        }

        // ── LE VOTE S'OUVRE DÈS QUE LE TAS DIT « BOUCHÉ » ────────────────────────────────────
        //
        // Sa règle : « dès que le TAS dit bouché, pour ne pas perdre de temps ». On n'attend donc
        // pas la fin de la manche — la carte est déjà infranchissable, et chaque seconde passée
        // dessus est prise à une manche que personne ne peut gagner.
        //
        // ⚠ On n'ouvre QUE sur un `Bloc` désigné. Sur `AucunSeul`, il n'y a rien à proposer :
        // ouvrir un vote sans proposition demanderait aux joueurs de deviner, et un vote qu'on ne
        // sait pas formuler use le mécanisme pour les fois où il compte.
        if self.vote.is_none()
            && self.party_game.phase == party_game::GamePhase::Running
            && matches!(self.verificateur.etat(), tas::EtatCarte::PasTrouvee)
        {
            if let tas::Bouchon::Bloc { x, y } = bouchon {
                // Tout le monde vote — y compris qui a fini, et y compris qui a posé le bloc.
                let inscrits = self.party_game.players.len();
                log::info!("🗳 Carte bouchée : vote sur le retrait du bloc ({x},{y}) — {inscrits} inscrits.");
                self.vote = Some(vote::Vote::ouvrir((x, y), inscrits));
            }
        }

        if let Some(v) = self.vote.as_mut() {
            match v.update(dt) {
                vote::Issue::EnCours => {}
                vote::Issue::Adopte => {
                    let (x, y) = v.bloc;
                    log::info!("🗳 Adopté ({}/{}) — le bloc ({x},{y}) est retiré.", v.pour(), v.inscrits());
                    self.party_game.grid.set_tile(x, y, grid::TileType::Air);
                    // La carte a changé : le verdict précédent ne vaut plus rien. On redemande,
                    // plutôt que de laisser à l'écran un « bouché » que le vote vient de démentir.
                    self.verificateur.oublier();
                    self.verificateur.lancer(&self.party_game.grid, &self.party_game.traps);
                    self.vote = None;
                }
                vote::Issue::Rejete => {
                    log::info!("🗳 Rejeté ({}/{} requis) — le bloc reste.", v.pour(), v.seuil());
                    self.vote = None;
                }
            }
        }
        let etat_pont = hud::EtatPont {
            relie: self.sidecar.relie(),
            envoyes,
            recus,
            avatars,
        };

        match engine.gpu.begin_frame(window) {
            Ok((cmd, image_index)) => {
                party_render_pass.render_party_scene(
                    &engine.gpu,
                    cmd,
                    image_index,
                    &self.party_game,
                    &party_render_pass::Exterieur {
                        pont: &etat_pont,
                        distants: &distants,
                        carte: self.verificateur.etat(),
                        bouchon: &bouchon,
                        vote: self.vote.as_ref(),
                        demonstration: self.demonstration.as_ref().map(|d| d.position()),
                    },
                );
                if engine.gpu.end_frame(cmd, image_index, window).is_err() {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        engine.gpu.resize(window);
                        let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
                        party_render_pass.recreate_framebuffer_resources(&engine.gpu, &memory_props);
                    }
                }
                engine.frame_count += 1;
                // LA trame qui compte, et elle est ICI et nulle part ailleurs : `end_frame` a
                // présenté une image RÉELLE à l'écran. L'envoyer plus tôt (à la création de la
                // fenêtre, ou après l'init Vulkan) ferait exactement le mensonge que le contrat
                // interdit — le régisseur tuerait le jeu précédent avant que celui-ci n'affiche
                // quoi que ce soit, et l'on verrait un écran noir. `pret()` est un one-shot : le
                // laisser dans la boucle de rendu ne coûte rien après la première image.
                self.nav.pret();
            }
            Err(_) => {
                let size = window.inner_size();
                if size.width > 0 && size.height > 0 {
                    engine.gpu.resize(window);
                    let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
                    party_render_pass.recreate_framebuffer_resources(&engine.gpu, &memory_props);
                }
            }
        }
    }
}

impl ApplicationHandler for AegisApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("AegisEngine v1.0.0 - Party Platformer 2.5D Game")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

            let window = match event_loop.create_window(window_attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    log::error!("Impossible de créer la fenêtre winit: {:?}", e);
                    return;
                }
            };

            log::info!("Fenêtre créée avec succès. Initialisation du moteur AegisEngine...");
            // Ces trois jalons ne sont PAS décoratifs : l'initialisation Vulkan et la compilation des
            // pipelines peuvent durer plusieurs secondes au premier lancement. Sans eux, le régisseur
            // affiche une barre qui n'avance pas et l'on ne sait pas distinguer « ça charge » de
            // « c'est planté ».
            self.nav.progression(20);
            let engine = Engine::new(window.clone()).expect("Échec de l'initialisation du moteur Vulkan 1.4");
            self.nav.progression(60);
            let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
            let party_render_pass = PartyRenderPass::new(&engine.gpu, &memory_props).expect("Échec de création du RenderPass Party");
            self.nav.progression(90);

            self.window = Some(window);
            self.engine = Some(engine);
            self.party_render_pass = Some(party_render_pass);

            log::info!("=== Boucle de Jeu Party Platformer Démarrée (Séparation Moteur / Jeu Active) ===");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window = match self.window.as_ref() {
            Some(w) if w.id() == id => w.clone(),
            _ => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Fermeture du jeu demandée par l'utilisateur.");
                event_loop.exit();
            }
            WindowEvent::Resized(_new_size) => {
                if let (Some(engine), Some(party_pass)) = (self.engine.as_mut(), self.party_render_pass.as_mut()) {
                    engine.on_resize(&window);
                    let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
                    party_pass.recreate_framebuffer_resources(&engine.gpu, &memory_props);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed {
                    if button == MouseButton::Left {
                        self.handle_left_click();
                    } else if button == MouseButton::Right && self.party_game.phase == crate::party_game::GamePhase::Placement {
                        self.party_game.placement_dir = self.party_game.placement_dir.rotate_cw();
                        log::info!("🔄 Clic Droit → Rotation du Piège en Direction {:?}", self.party_game.placement_dir);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_y = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.05,
                };
                if scroll_y != 0.0 {
                    if let Some(party_pass) = self.party_render_pass.as_mut() {
                        party_pass.zoom_level = (party_pass.zoom_level - scroll_y * 0.15).clamp(0.4, 2.8);
                    }
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(key_code), state, .. }, .. } => {
                let is_pressed = state == ElementState::Pressed;

                match key_code {
                    // Contrôles de déplacement ZQSD (AZERTY) / WASD (QWERTY) / Flèches
                    KeyCode::KeyZ | KeyCode::KeyW | KeyCode::ArrowUp => {
                        self.input_state.up = is_pressed;
                    }
                    KeyCode::KeyS | KeyCode::ArrowDown => {
                        self.input_state.down = is_pressed;
                        self.input_state.crouch = is_pressed;
                    }
                    KeyCode::KeyA | KeyCode::KeyQ | KeyCode::ArrowLeft => {
                        self.input_state.left = is_pressed;
                    }
                    KeyCode::KeyD | KeyCode::ArrowRight => {
                        self.input_state.right = is_pressed;
                    }
                    KeyCode::Space => {
                        self.input_state.jump = is_pressed;
                        if is_pressed {
                            self.input_state.jump_pressed_this_frame = true;
                        }
                    }
                    KeyCode::Enter => {}

                    // ── VOTER : O pour retirer le bloc, N pour le garder ──────────────────
                    //
                    // ⚠ Deux touches DÉDIÉES, et pas Entrée/Échap ni une réutilisation d'une
                    // touche de jeu : le vote s'ouvre PENDANT la course, doigts sur les
                    // déplacements. Une touche partagée ferait voter par accident quelqu'un qui
                    // essayait de sauter — et un bulletin ne se reprend pas.
                    //
                    // O et N parce que le jeu parle français. Le joueur local est le premier
                    // humain de la table ; sur le réseau, chacun émettra le sien.
                    KeyCode::KeyO | KeyCode::KeyN if is_pressed => {
                        if let Some(v) = self.vote.as_mut() {
                            let bulletin = if key_code == KeyCode::KeyO {
                                vote::Bulletin::Pour
                            } else {
                                vote::Bulletin::Contre
                            };
                            if let Some(moi) = self.party_game.players.iter().find(|p| p.is_human) {
                                if v.voter(moi.id, bulletin) {
                                    log::info!(
                                        "🗳 Bulletin enregistré : {bulletin:?} — {}/{} pour, seuil {}",
                                        v.pour(), v.inscrits(), v.seuil()
                                    );
                                }
                            }
                        }
                    }

                    // Sélection directe d'un item du carton par touche 1-0 en phase Draft
                    KeyCode::Digit1 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(0); log::info!("Item 1 sélectionné"); },
                    KeyCode::Digit2 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(1); log::info!("Item 2 sélectionné"); },
                    KeyCode::Digit3 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(2); log::info!("Item 3 sélectionné"); },
                    KeyCode::Digit4 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(3); log::info!("Item 4 sélectionné"); },
                    KeyCode::Digit5 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(4); log::info!("Item 5 sélectionné"); },
                    KeyCode::Digit6 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(5); log::info!("Item 6 sélectionné"); },
                    KeyCode::Digit7 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(6); log::info!("Item 7 sélectionné"); },
                    KeyCode::Digit8 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(7); log::info!("Item 8 sélectionné"); },
                    KeyCode::Digit9 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(8); log::info!("Item 9 sélectionné"); },
                    KeyCode::Digit0 => if is_pressed && self.party_game.phase == crate::party_game::GamePhase::Drafting { self.party_game.mystery_box.select_item(9); log::info!("Item 10 sélectionné"); },

                    // Rotation du piège sur R (Phase de Placement) ou Reset du joueur (Phase Running)
                    KeyCode::KeyR => {
                        if is_pressed {
                            if self.party_game.phase == crate::party_game::GamePhase::Placement {
                                self.party_game.placement_dir = self.party_game.placement_dir.rotate_cw();
                                log::info!("🔄 Touche R → Rotation du Piège en Direction {:?}", self.party_game.placement_dir);
                            } else {
                                self.party_game.reset_player_to_spawn();
                            }
                        }
                    }


                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();

                if let Some(engine) = self.engine.as_ref() {
                    if self.screenshot_mode && engine.frame_count >= self.screenshot_frame {
                        if let Err(e) = engine.capture_screenshot(&self.screenshot_path) {
                            log::error!("Erreur lors de la capture d'écran: {:?}", e);
                        }
                        event_loop.exit();
                    } else {
                        window.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Le launcher nous demande de partir (il a basculé vers un autre monde et l'autre a prouvé
        // son image). On sort proprement : le régisseur possède bien un repli dur, mais un jeu qui
        // quitte de lui-même rend son contexte Vulkan et ne laisse pas de fenêtre fantôme derrière.
        if self.nav.doit_quitter() {
            event_loop.exit();
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

/// ⚠ DEUX FAÇONS DE MOURIR, ET LE HOOK N'EN COUVRE QU'UNE (mesuré le 16 août 2026).
///
/// Le hook de panique attrape les `panic!`. Mais ce jeu s'arrête bien plus souvent par une `Err`
/// remontée jusqu'ici — et une `Err` rendue par `main` **n'est pas une panique** : le processus se
/// termine proprement, le hook ne voit rien.
///
/// Constaté en essayant pour de vrai, pas en le supposant : lancé sans affichage, le jeu rend
/// `Error: Os(... XNotSupported(XOpenDisplayFailed))` et **aucun témoin n'était déposé**. Or c'est
/// exactement le cas le plus probable chez quelqu'un d'autre — pas de pilote Vulkan, pas
/// d'affichage, matériel inconnu. Le mécanisme aurait donc raté précisément ce qu'il devait
/// attraper, tout en ayant l'air complet.
///
/// D'où cet enrobage : `main` ne fait que déléguer, et déposer le témoin si le jeu a refusé de
/// démarrer. Le message d'erreur d'origine reste affiché, on n'enlève rien.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── MODE ANALYSE : `aegis_game --analyser <fichier>` ─────────────────────────────────────
    // Aucune fenêtre, aucun Vulkan : il rejoue un enregistrement dans la physique du solveur et
    // rend son verdict. C'est ce qui permet de l'exécuter par SSH, sur n'importe quelle machine,
    // et surtout de rejouer dix fois la même partie — ce qu'un écran ne permet jamais.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--analyser") {
        let Some(chemin) = args.get(i + 1) else {
            eprintln!("usage : aegis_game --analyser <fichier de boite noire>");
            std::process::exit(2);
        };
        match boite_noire::analyser(std::path::Path::new(chemin)) {
            Ok(a) => {
                println!("instants        : {}", a.instants);
                println!("humain arrive   : {}", a.humain_arrive);
                println!("rejeu arrive    : {}", a.rejeu_arrive);
                println!("ecart max       : {:.3} unites", a.ecart_max);
                match a.divergence {
                    Some((n, e)) => println!("divergence      : image {n} (ecart {e:.3})"),
                    None => println!("divergence      : aucune au-dela d'une demi-tuile"),
                }
                println!();
                // Le diagnostic en clair : c'est lui qui dit où creuser.
                if a.humain_arrive && a.rejeu_arrive {
                    println!("=> LA PHYSIQUE EST FIDELE. La sequence gagnante existe dans le monde du");
                    println!("   solveur : s'il a dit « pas trouve », c'est sa RECHERCHE qui echoue");
                    println!("   (maille, budget, heuristique) — pas sa simulation.");
                } else if a.humain_arrive && !a.rejeu_arrive {
                    println!("=> LE SIMULATEUR DU TAS DIVERGE DU JEU. L'humain est arrive, ses entrees");
                    println!("   rejouees non. Tout ce que le solveur affirme devient suspect, y compris");
                    println!("   ses succes — c'est le plus grave des deux defauts.");
                } else {
                    println!("=> L'humain n'est pas arrive sur cette manche : rien a conclure du rejeu.");
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[analyse] {e}");
                std::process::exit(1);
            }
        }
    }

    plantage::installer();
    let resultat = executer();
    if let Err(e) = &resultat {
        plantage::deposer(&format!("aegis n'a pas pu démarrer — {e}"));
    }
    resultat
}

fn executer() -> Result<(), Box<dyn std::error::Error>> {
    // LA CAPTURE DES PLANTAGES EN TOUT PREMIER — avant même le journal. Ce jeu parle à Vulkan, donc
    // à un pilote graphique, donc à du matériel qu'on ne connaît pas : c'est le programme du projet
    // le plus susceptible de tomber sur la machine de quelqu'un d'autre. Sans trace, une fenêtre qui
    // se ferme ne laisse RIEN. Elle ne part nulle part : le launcher demandera au démarrage suivant.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let screenshot_mode = args.iter().any(|arg| arg == "--screenshot");
    // Où atterrit la capture : le dossier courant par défaut, ou l'argument qui suit `--screenshot`.
    // (Avant : un chemin absolu vers le dossier de travail d'un agent sur UNE machine — la capture
    // partait donc dans le vide partout ailleurs, sans qu'aucun message ne le dise.)
    let screenshot_path = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "aegis_screenshot.png".to_string());

    // À quelle image capturer. La cinquième suffit pour juger un décor, mais pas pour voir un
    // état qui met du temps à s'établir — une animation qui démarre, un compteur réseau qui
    // monte. Sans ce réglage on ne peut vérifier que le tout début d'une partie, et c'est
    // précisément ce qui a laissé passer un HUD entièrement à l'envers.
    let screenshot_frame: u64 = args
        .iter()
        .position(|a| a == "--frame")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    log::info!("=== Démarrage d'AegisEngine v1.0.0 (Party Platformer Game) ===");
    if screenshot_mode {
        log::info!("Mode Capture d'Écran Autonome Activé : Exportation vers {}", screenshot_path);
    }

    // Le choix du backend est une affaire de Linux (X11 plutôt que Wayland, qui interdit au jeu de
    // se placer lui-même à l'écran). Ailleurs, winit n'a qu'un seul backend : rien à choisir.
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "linux")]
    builder.with_x11();
    let event_loop = builder.build()?;

    let mut app = AegisApp::new(screenshot_mode, screenshot_path, screenshot_frame);
    event_loop.run_app(&mut app)?;

    Ok(())
}
