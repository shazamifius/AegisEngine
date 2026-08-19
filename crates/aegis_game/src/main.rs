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

    fn render_frame(&mut self) {
        let (window, engine, party_render_pass) = match (self.window.as_ref(), self.engine.as_mut(), self.party_render_pass.as_mut()) {
            (Some(w), Some(e), Some(r)) => (w, e, r),
            _ => return,
        };

        let dt = engine.delta_time();
        self.party_game.update(dt, &self.input_state);
        self.input_state.jump_pressed_this_frame = false;

        // On pousse NOTRE position au cœur, et rien d'autre : lui seul décide ce que les autres
        // en voient. La cadence est plafonnée dans le client, l'appelant n'a pas à la connaître.
        let moi = self.party_game.human_player().position;
        self.sidecar.pousser_ma_pose(moi.x, moi.y, 0.0, 0.0, 0.0);

        let distants = self.sidecar.avatars();
        let (envoyes, recus, avatars) = self.sidecar.compteurs();
        let etat_pont = hud::EtatPont {
            relie: self.sidecar.relie(),
            envoyes,
            recus,
            avatars,
        };

        match engine.gpu.begin_frame(window) {
            Ok((cmd, image_index)) => {
                party_render_pass.render_party_scene(&engine.gpu, cmd, image_index, &self.party_game, &etat_pont, &distants);
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
