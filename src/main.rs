#![allow(dead_code, unused_variables)]

mod core;
mod geometry;
mod materials;
mod physics;
mod render;
mod scene;
mod vr;

use std::sync::Arc;
use core::engine::Engine;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::x11::EventLoopBuilderExtX11,
    window::{Window, WindowId},
};

struct AegisApp {
    window: Option<Arc<Window>>,
    engine: Option<Engine>,
    screenshot_mode: bool,
    screenshot_path: String,
}

impl ApplicationHandler for AegisApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("AegisEngine v1.0.0 - Pure Vulkan 1.4 Native From Scratch")
                .with_inner_size(winit::dpi::LogicalSize::new(720, 1280));

            let window = match event_loop.create_window(window_attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    log::error!("Impossible de créer la fenêtre winit: {:?}", e);
                    return;
                }
            };

            log::info!("Fenêtre créée avec succès. Initialisation du moteur Vulkan 1.4 Native...");
            let engine = Engine::new(window.clone()).expect("Échec de l'initialisation du moteur Vulkan 1.4");

            self.window = Some(window);
            self.engine = Some(engine);

            log::info!("Boucle de rendu Vulkan 1.4 démarrée avec succès.");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let (window, engine) = match (self.window.as_ref(), self.engine.as_mut()) {
            (Some(w), Some(e)) if w.id() == id => (w, e),
            _ => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Fermeture du moteur demandée par l'utilisateur.");
                event_loop.exit();
            }
            WindowEvent::Resized(_new_size) => {
                engine.on_resize(window);
            }
            WindowEvent::RedrawRequested => {
                engine.render_frame(window);

                if self.screenshot_mode && engine.frame_count >= 5 {
                    if let Err(e) = engine.capture_screenshot(&self.screenshot_path) {
                        log::error!("Erreur lors de la capture d'écran: {:?}", e);
                    }
                    event_loop.exit();
                } else {
                    window.request_redraw();
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
    let screenshot_path = "/home/shaza/.gemini/antigravity/brain/a583107a-47cc-411b-84a4-969fd78a0aa3/screenshot_glass_v4.png".to_string();

    log::info!("=== Démarrage d'AegisEngine v1.0.0 (Pure Vulkan 1.4 From Scratch) ===");
    if screenshot_mode {
        log::info!("Mode Capture d'Écran Autonome Activé : Exportation vers {}", screenshot_path);
    }

    let event_loop = EventLoop::builder()
        .with_x11()
        .build()?;

    let mut app = AegisApp {
        window: None,
        engine: None,
        screenshot_mode,
        screenshot_path,
    };

    event_loop.run_app(&mut app)?;

    Ok(())
}
