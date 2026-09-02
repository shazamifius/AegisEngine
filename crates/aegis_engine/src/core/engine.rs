use ash::vk;
use crate::core::math::{Mat4, Vec3, Vec4};
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::core::gpu_context::GpuContext;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GlassPushConstants {
    pub mvp_matrix: Mat4,
    pub model_matrix: Mat4,
    pub normal_matrix: Mat4,
    pub glass_tint: Vec4,
    pub params: Vec4,
}

#[derive(Clone, Debug)]
pub struct GlassSlabInstance {
    pub position: Vec3,
    pub rotation_z: f32,
    pub rotation_x: f32,
    pub tint: Vec4,
    pub rugosite: f32,
}

/// Moteur de Rendu 3D Principal AegisEngine (Pure Vulkan 1.4 Native).
pub struct Engine {
    pub gpu: GpuContext,
    pub frame_count: u64,
    pub last_update: Instant,
}

impl Engine {
    pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let gpu = GpuContext::new(window)?;

        Ok(Self {
            gpu,
            frame_count: 0,
            last_update: Instant::now(),
        })
    }

    pub fn delta_time(&mut self) -> f32 {
        let dt = self.last_update.elapsed().as_secs_f32().min(0.033);
        self.last_update = Instant::now();
        dt
    }

    pub fn on_resize(&mut self, window: &Window) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.gpu.resize(window);
    }

    /// Rapatrie l'image affichée depuis la carte, en RVB (trois octets par pixel).
    ///
    /// ⚠ Extraite de la capture le 30 août 2026 parce que **deux choses en ont besoin** : écrire
    /// un PNG, et mesurer ce que l'image fait à l'œil. Deux copies de ce transfert auraient
    /// divergé — et la mesure aurait alors porté sur une image qui n'est pas celle qu'on regarde.
    ///
    /// ⚠⚠ **Le transfert lui-même a déménagé dans `GpuContext::relire_image` le 2 septembre 2026.**
    /// Il vivait ici, donc dans la structure qui exige une fenêtre, et il supposait en dur que
    /// l'image venait d'une chaîne de présentation. *Résultat : le contexte sans écran savait
    /// rendre, et rien ne savait relire ce qu'il rendait.* Ce qui reste ici est ce qui appartient
    /// vraiment à `Engine` : **savoir QUELLE image regarder.**
    pub fn lire_image(&self) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
        let extent = self.gpu.swapchain_extent;
        // ⚠ L'image RÉELLEMENT rendue, pas la première de la liste. Lire `[0]` marchait par
        // accident tant qu'on ne regardait pas de près ; dès qu'on demande une mesure exacte, ça
        // devient une photographie d'une image vieille de plusieurs trames.
        let image = self.gpu.swapchain_images[self.gpu.derniere_image];
        let rvb = self.gpu.relire_image(
            image,
            vk::ImageLayout::PRESENT_SRC_KHR,
            extent,
            self.gpu.swapchain_format,
        )?;
        Ok((extent.width, extent.height, rvb))
    }

    /// Mesure la tonalité de ce qui est actuellement affiché.
    ///
    /// ⚠ Ce que ça rend est un INDICATEUR, jamais un verdict : le juge du rendu perçu reste un
    /// œil humain. L'instrument sert à rendre décidable ce qui ne l'était pas — comparer deux
    /// réglages autrement que de mémoire, et nommer ce qui manque.
    pub fn mesurer_tonalite(
        &self,
    ) -> Result<crate::image::tonalite::Analyse, Box<dyn std::error::Error>> {
        let (_, _, rvb) = self.lire_image()?;
        Ok(crate::image::tonalite::analyser(&rvb))
    }


    pub fn capture_screenshot(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (largeur, hauteur, rvb) = self.lire_image()?;
        let extent = vk::Extent2D { width: largeur, height: hauteur };

        // ⚠⚠ ICI VIVAIT UN SCRIPT PYTHON, ÉCRIT SUR LE DISQUE PUIS EXÉCUTÉ (30 août 2026).
        //
        // Trois fautes en une : la règle la plus ferme du projet est « QUE du Rust, aucun autre
        // langage » ; la capture échouait chez quiconque n'a pas `python3` ; et **cet échec était
        // avalé** — le code ne journalisait que le succès, sans branche `else`. Le mécanisme
        // serait donc mort chez quelqu'un d'autre, en silence.
        //
        // L'encodeur est maintenant dans le moteur, en Rust, et prouvé par aller-retour (il porte
        // son propre décodeur de test : ce qu'il écrit doit se relire octet pour octet).
        let png = crate::image::png::encoder(extent.width, extent.height, &rvb)
            .map_err(|e| format!("encodage PNG impossible : {e}"))?;
        std::fs::write(path, &png)?;

        log::info!(
            "Capture ecrite : {path} ({}x{}, {} Ko)",
            extent.width,
            extent.height,
            png.len() / 1024
        );

        Ok(())
    }
}
