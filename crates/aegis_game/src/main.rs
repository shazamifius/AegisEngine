#![allow(dead_code, unused_variables)]

mod grid;
mod traps;
mod particles;
mod player;
mod mls_mpm;
mod mystery_box;
mod party_game;
mod party_render_pass;
mod objects;

use std::sync::Arc;
use aegis_engine::Engine;
use party_game::PartyGame;
use party_render_pass::PartyRenderPass;
use player::InputState;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    platform::x11::EventLoopBuilderExtX11,
    window::{Window, WindowId},
};

struct AegisApp {
    window: Option<Arc<Window>>,
    engine: Option<Engine>,
    party_render_pass: Option<PartyRenderPass>,
    party_game: PartyGame,
    input_state: InputState,
    screenshot_mode: bool,
    screenshot_path: String,
}

impl AegisApp {
    fn new(screenshot_mode: bool, screenshot_path: String) -> Self {
        Self {
            window: None,
            engine: None,
            party_render_pass: None,
            party_game: PartyGame::new(48, 24),
            input_state: InputState::default(),
            screenshot_mode,
            screenshot_path,
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

        match engine.gpu.begin_frame(window) {
            Ok((cmd, image_index)) => {
                party_render_pass.render_party_scene(&engine.gpu, cmd, image_index, &self.party_game);
                if engine.gpu.end_frame(cmd, image_index, window).is_err() {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        engine.gpu.resize(window);
                        let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
                        party_render_pass.recreate_framebuffer_resources(&engine.gpu, &memory_props);
                    }
                }
                engine.frame_count += 1;
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
            let engine = Engine::new(window.clone()).expect("Échec de l'initialisation du moteur Vulkan 1.4");
            let memory_props = unsafe { engine.gpu.instance.get_physical_device_memory_properties(engine.gpu.physical_device) };
            let party_render_pass = PartyRenderPass::new(&engine.gpu, &memory_props).expect("Échec de création du RenderPass Party");

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

                    // Sélection du Type de Bloc Éditeur (Touches 1, 2, 3, 4, 5)
                    KeyCode::Digit1 => if is_pressed { self.party_game.editor.selected_block = crate::party_game::EditorBlockType::Grass; log::info!("Bloc Sélectionné : 1 - HERBE VERTE"); },
                    KeyCode::Digit2 => if is_pressed { self.party_game.editor.selected_block = crate::party_game::EditorBlockType::Dirt; log::info!("Bloc Sélectionné : 2 - TERRE MARRON"); },
                    KeyCode::Digit3 => if is_pressed { self.party_game.editor.selected_block = crate::party_game::EditorBlockType::Stone; log::info!("Bloc Sélectionné : 3 - PIERRE GRISE"); },
                    KeyCode::Digit4 => if is_pressed { self.party_game.editor.selected_block = crate::party_game::EditorBlockType::Start; log::info!("Bloc Sélectionné : 4 - POINT DE SPAWN"); },
                    KeyCode::Digit5 => if is_pressed { self.party_game.editor.selected_block = crate::party_game::EditorBlockType::Finish; log::info!("Bloc Sélectionné : 5 - POINT D'ARRIVÉE"); },

                    // Bascule de Mode (Mode Éditeur <-> Mode Jeu Direct) sur Tab ou F1
                    KeyCode::Tab | KeyCode::F1 => if is_pressed { self.party_game.toggle_mode(); },

                    // Re-spawn / Reset du Joueur sur R
                    KeyCode::KeyR => if is_pressed { self.party_game.reset_player_to_spawn(); },

                    // Suppression de bloc visé sur Suppr / Retour Arrière
                    KeyCode::Delete | KeyCode::Backspace => if is_pressed { self.party_game.delete_selected_block(); },

                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();

                if let Some(engine) = self.engine.as_ref() {
                    if self.screenshot_mode && engine.frame_count >= 5 {
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let screenshot_mode = args.iter().any(|arg| arg == "--screenshot");
    let screenshot_path = "/home/shaza/.gemini/antigravity/brain/e25d64b0-bc32-41b5-95c2-f822bf7c18e1/party_game_screenshot.png".to_string();

    log::info!("=== Démarrage d'AegisEngine v1.0.0 (Party Platformer Game) ===");
    if screenshot_mode {
        log::info!("Mode Capture d'Écran Autonome Activé : Exportation vers {}", screenshot_path);
    }

    let event_loop = EventLoop::builder()
        .with_x11()
        .build()?;

    let mut app = AegisApp::new(screenshot_mode, screenshot_path);
    event_loop.run_app(&mut app)?;

    Ok(())
}
