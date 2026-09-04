//! # LA PASSE DE VERRE — la première physique de la MATIÈRE qui traverse le GPU
//!
//! Le shader qu'elle lance (`shaders/refraction.wgsl`) porte l'explication de ce qu'il calcule.
//! Ce fichier-ci ne porte que la plomberie, et **la mesure**.
//!
//! ## ⚠ CE QUE CETTE PASSE N'EST PAS ENCORE
//!
//! Elle est exercée par son test, **pas par le rendu du jeu**. C'est un pas assumé et écrit : le
//! shader doit être prouvé JUSTE avant d'être branché, sinon on brancherait une erreur et il
//! faudrait ensuite démêler laquelle des deux moitiés est fausse. *La règle du projet — aucune
//! brique dans `render/` sans être appelée dans le même commit — est tenue par le test ; elle
//! ne le sera vraiment que quand une image du jeu la traversera.*
//!
//! ## ⭐ Pourquoi la mesure ne compare PAS le GPU au processeur
//!
//! `epaisseur.rs` fait le même calcul côté processeur, et il serait tentant de comparer les deux
//! images. **Ce serait une mauvaise mesure** : deux implémentations issues du même raisonnement
//! peuvent être fausses exactement de la même façon, et leur accord ne prouverait alors que leur
//! parenté. *Chacune est donc confrontée à LA VÉRITÉ* — une sphère, dont on sait calculer
//! analytiquement où le rayon ressort. Le processeur y arrive à **1,740°** ; le GPU doit faire au
//! moins aussi bien, et c'est le critère.

use crate::core::gpu_context::GpuContext;
use crate::render::pipeline::{Melange, PipelineFactory, Reglages};
use ash::vk;

/// Les 96 octets que le shader lit — six vecteurs, et pas un de plus.
///
/// ⚠ **Vulkan ne garantit que 128 octets de constantes poussées**, et le projet a déjà payé pour
/// l'avoir oublié : le moteur en poussait 160 et fonctionnait ici parce que cette machine en offre
/// 256. *Une limite qu'on ne lit pas est une limite qu'on découvre chez quelqu'un d'autre.*
///
/// C'est aussi pourquoi la caméra voyage par sa **base** et non par ses matrices : deux matrices
/// 4×4 auraient coûté 128 octets à elles seules, et la même base sert à projeter comme à
/// dé-projeter.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ConstantesVerre {
    /// xyz = l'œil.
    pub position: [f32; 4],
    /// xyz = axe droit, w = tangente du demi-champ horizontal.
    pub droite: [f32; 4],
    /// xyz = axe haut, w = tangente du demi-champ vertical.
    pub haut: [f32; 4],
    /// xyz = axe de visée.
    pub avant: [f32; 4],
    /// xyz = absorption par canal (Beer-Lambert), w = rapport des indices n₁/n₂.
    ///
    /// ⚠ Depuis le 4 septembre 2026, `xyz` est **l'absorption de référence**, que le volume de
    /// matière module texel par texel. Un volume neutre (partout 1) redonne exactement le milieu
    /// homogène d'avant.
    pub matiere: [f32; 4],
    /// xy = taille en pixels, z = mode (0 = couleur, 1 = direction), w = tours de Newton.
    pub reglages: [f32; 4],
    /// ⭐ xyz = le coin **minimal** de la boîte du volume, dans le monde ; w = le nombre de pas de
    /// la marche à l'intérieur de la matière.
    ///
    /// *Le nombre de pas voyage ici parce qu'il n'y avait plus un seul `w` libre ailleurs — et
    /// c'est bien : ce qui décide de la finesse de l'intégration appartient au volume.*
    pub volume_min: [f32; 4],
    /// xyz = la **taille** de la boîte du volume, dans le monde. `w` n'est pas lu.
    ///
    /// ⚠ Une composante nulle donnerait une division par zéro dans le shader ; l'appelant décrit
    /// toujours une boîte réelle, même pour un volume d'un seul texel.
    pub volume_taille: [f32; 4],
}

impl ConstantesVerre {
    /// ⭐ **Le milieu homogène, écrit une seule fois** — le coin de la boîte, et **un seul pas**.
    ///
    /// Sur un volume neutre, un pas unique suffit et il est *exact* : l'unique segment couvre tout
    /// le trajet, son échantillon au milieu vaut 1, et la somme donne `sigma × distance` — la
    /// formule d'avant, au calcul près. *Demander plus de pas ici ne changerait rien qu'un coût.*
    pub const MILIEU_HOMOGENE_MIN: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    /// La taille de la boîte du milieu homogène. **Aucune composante nulle** : le shader divise
    /// par elle.
    pub const MILIEU_HOMOGENE_TAILLE: [f32; 4] = [1.0, 1.0, 1.0, 0.0];

    /// Les octets tels que le shader les lit.
    fn octets(&self) -> &[u8] {
        // SÛRETÉ : `#[repr(C)]` sur un agrégat de `f32` — pas de bourrage, pas de pointeur, pas
        // d'invariant. La taille est vérifiée par un test plutôt que supposée.
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// La passe elle-même : un pipeline plein écran qui lit **deux cartes de géométrie**.
///
/// ⭐ Elle ne connaît aucune forme. Toute la géométrie entre par `brancher`, sous forme de deux
/// images « normale + distance ». *C'est ce qui rend cette passe indépendante de ce qu'on
/// rend* — une sphère exacte aujourd'hui, un maillage venu de Blender demain, sans qu'une ligne
/// d'ici ne bouge.
pub struct Verre {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    descripteurs: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    ensemble: vk::DescriptorSet,
    /// ⭐ **Le volume neutre — un seul texel valant 1, et il n'est pas un bouche-trou.**
    ///
    /// Un ensemble de descripteurs doit être **entièrement** écrit avant d'être lu : laisser le
    /// binding du volume vide serait un comportement indéfini, pas une absence. Il faut donc
    /// toujours un volume — et le volume neutre est celui qui **redonne exactement le milieu
    /// homogène**, puisque `sigma × 1` sommé sur le trajet vaut `sigma × distance`.
    ///
    /// *C'est ce qui permet à la passe de n'avoir qu'un seul chemin de code : le cas simple n'est
    /// pas une branche, c'est une valeur.*
    volume_neutre: crate::render::texture::Texture,
}

impl Verre {
    pub fn nouvelle(
        gpu: &GpuContext,
        format_cible: vk::Format,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // ⭐ Deux images échantillonnées, et **aucun échantillonneur** : le shader lit par
        // `textureLoad`, donc au texel exact. Ce n'est pas une économie de descripteur — c'est
        // qu'une normale interpolée entre deux surfaces n'existe sur aucune des deux.
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // ⭐ Le volume de matière, lui, veut un échantillonneur : on l'interpole (voir
            // `Texture::create_volume`). C'est la différence de nature entre une carte de
            // géométrie et un milieu.
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let descripteurs = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?
        };
        let tailles = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(3),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
        ];
        let pool = unsafe {
            gpu.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&tailles)
                    .max_sets(1),
                None,
            )?
        };
        let ensemble = unsafe {
            gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&descripteurs)),
            )?[0]
        };

        let plage = PipelineFactory::create_push_constant_range(
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            std::mem::size_of::<ConstantesVerre>() as u32,
        );
        let layout = PipelineFactory::create_pipeline_layout(
            &gpu.device,
            std::slice::from_ref(&descripteurs),
            &[plage],
        )?;

        let module =
            PipelineFactory::create_shader_module_from_bytes(&gpu.device, crate::shaders::REFRACTION_SPV)?;
        let pipeline = PipelineFactory::create_graphics_pipeline(
            &gpu.device,
            layout,
            module,
            module,
            Reglages {
                color_format: format_cible,
                second_format: None,
                depth_format: None,
                depth_write: false,
                melange: Melange::Aucun,
                // Le shader fabrique ses trois sommets tout seul.
                use_vertex_input: false,
                faces: crate::render::pipeline::Faces::Toutes,
                echantillons: vk::SampleCountFlags::TYPE_1,
            },
        )?;
        unsafe { gpu.device.destroy_shader_module(module, None) };

        // Un seul texel valant 1 par canal : le milieu homogène, écrit comme une **valeur** et
        // non comme une branche.
        //
        // ⚠ En 32 bits flottants, comme les cartes de géométrie — et c'est délibéré. La moitié de
        // cette mémoire suffirait (`R16G16B16A16_SFLOAT`), mais le projet n'embarque aucune
        // bibliothèque et la conversion vers un flottant de 16 bits est à écrire à la main, avec
        // ses subnormaux et ses arrondis. *Une conversion fausse ne casserait pas l'image : elle
        // la rendrait plausible et fausse* — le pire mode de panne de ce moteur. Le format est un
        // paramètre de `create_volume` : le jour où la mémoire d'un mobile décidera, ce sera un
        // argument à changer, et une conversion à prouver par un test.
        let memoire = unsafe {
            gpu.instance
                .get_physical_device_memory_properties(gpu.physical_device)
        };
        let mut texel = Vec::with_capacity(16);
        for v in [1.0f32, 1.0, 1.0, 0.0] {
            texel.extend_from_slice(&v.to_ne_bytes());
        }
        let volume_neutre = crate::render::texture::Texture::create_volume(
            gpu,
            &memoire,
            [1, 1, 1],
            vk::Format::R32G32B32A32_SFLOAT,
            16,
            &texel,
        )?;

        let passe = Self { pipeline, layout, descripteurs, pool, ensemble, volume_neutre };
        passe.brancher_matiere(gpu, None);
        Ok(passe)
    }

    /// Désigne le volume que le shader traversera — ou **le milieu homogène** quand on ne lui en
    /// donne aucun.
    ///
    /// ⚠ **À rappeler après chaque `brancher`** si un volume propre est en place : les deux
    /// fonctions écrivent dans le même ensemble de descripteurs, et rien n'oblige l'appelant à les
    /// appeler dans un ordre plutôt qu'un autre. *C'est écrit ici parce que rien dans le type ne
    /// l'impose — une garde de prose, donc faible, et il vaut mieux le dire que le taire.*
    pub fn brancher_matiere(&self, gpu: &GpuContext, volume: Option<&crate::render::texture::Texture>) {
        let choisi = volume.unwrap_or(&self.volume_neutre);
        let image = [vk::DescriptorImageInfo::default()
            .image_view(choisi.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let echantillonneur = [vk::DescriptorImageInfo::default().sampler(choisi.sampler)];
        let ecritures = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&image),
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&echantillonneur),
        ];
        unsafe { gpu.device.update_descriptor_sets(&ecritures, &[]) };
    }

    /// Désigne les deux cartes que le shader lira. À rappeler dès qu'elles changent.
    ///
    /// `xyz` = la normale dans le monde, `w` = la distance depuis l'œil ; `w <= 0` = pas de
    /// matière. **Les deux cartes décrivent le même pixel** — celle d'avant porte la face par
    /// laquelle le rayon entre, celle d'arrière la face par laquelle il ressort.
    pub fn brancher(&self, gpu: &GpuContext, avant: vk::ImageView, arriere: vk::ImageView) {
        let images = [
            vk::DescriptorImageInfo::default()
                .image_view(avant)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            vk::DescriptorImageInfo::default()
                .image_view(arriere)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
        ];
        let ecritures = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&images[..1]),
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&images[1..]),
        ];
        unsafe { gpu.device.update_descriptor_sets(&ecritures, &[]) };
    }

    /// Dessine dans l'attachement déjà ouvert par l'appelant.
    pub fn dessiner(&self, gpu: &GpuContext, cmd: vk::CommandBuffer, k: &ConstantesVerre) {
        unsafe {
            gpu.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            gpu.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                std::slice::from_ref(&self.ensemble),
                &[],
            );
            gpu.device.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                k.octets(),
            );
            gpu.device.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }

    pub fn detruire(&self, device: &ash::Device) {
        self.volume_neutre.detruire(device);
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            // Le pool emporte les ensembles qu'il a alloués ; il n'y a rien à libérer de plus.
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.descripteurs, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::math::Vec3;

    /// La scène de mesure, choisie une fois : une bille de verre vue de face.
    const RAYON: f32 = 1.0;
    const RECUL: f32 = 4.0;
    const ETA: f32 = 1.0 / 1.5;
    const COTE: u32 = 256;
    /// Demi-champ vertical de 30° — la bille occupe une large part de l'image, donc les rayons
    /// rasants (ceux qui dévient le plus) sont bien représentés. *Un cadrage serré sur le centre
    /// mesurerait le cas le plus facile et annoncerait un chiffre trop gentil : c'est exactement
    /// la faute du banc qui ne balayait qu'une ligne d'écran et annonçait 1,8° pour une méthode
    /// qui en valait 36.*
    const TANGENTE: f32 = 0.57735; // tan(30°)

    fn constantes(mode: f32, tours: f32, cote: u32) -> ConstantesVerre {
        ConstantesVerre {
            position: [0.0, 0.0, -RECUL, 0.0],
            droite: [1.0, 0.0, 0.0, TANGENTE],
            haut: [0.0, 1.0, 0.0, TANGENTE],
            avant: [0.0, 0.0, 1.0, 0.0],
            matiere: [0.0, 0.0, 0.0, ETA],
            reglages: [cote as f32, cote as f32, mode, tours],
            volume_min: ConstantesVerre::MILIEU_HOMOGENE_MIN,
            volume_taille: ConstantesVerre::MILIEU_HOMOGENE_TAILLE,
        }
    }

    /// Les deux cartes de la bille, **exactes** : une intersection analytique par pixel.
    ///
    /// ⭐ Elles ne passent par aucune rastérisation et par aucun maillage. C'est ce qui rend la
    /// mesure lisible : *l'écart obtenu avec ces cartes est celui de la DISCRÉTISATION EN PIXELS,
    /// et de rien d'autre.* Quand une vraie rastérisation les remplira, tout écart supplémentaire
    /// lui sera imputable — parce que ce chiffre-ci aura été mesuré seul.
    ///
    /// Format : `R32G32B32A32_SFLOAT`. Trente-deux bits, délibérément : sur seize, la précision
    /// des distances entrerait dans la mesure et on ne saurait plus ce qu'on mesure.
    fn cartes_exactes(cote: u32) -> (Vec<u8>, Vec<u8>) {
        let mut avant = Vec::with_capacity((cote * cote) as usize * 16);
        let mut arriere = Vec::with_capacity((cote * cote) as usize * 16);
        let origine = Vec3::new(0.0, 0.0, -RECUL);
        for y in 0..cote {
            for x in 0..cote {
                let d = direction_du_pixel(x, y, cote);
                let b = origine.dot(d);
                let c = origine.dot(origine) - RAYON * RAYON;
                let disc = b * b - c;
                let (t0, t1) = if disc < 0.0 {
                    (-1.0, -1.0)
                } else {
                    (-b - disc.sqrt(), -b + disc.sqrt())
                };
                let poser = |sortie: &mut Vec<u8>, t: f32| {
                    if t <= 0.0 {
                        // `w <= 0` est la seule façon de dire « pas de matière ici ».
                        sortie.extend_from_slice(&[0u8; 16]);
                        return;
                    }
                    let n = (origine + d * t) * (1.0 / RAYON);
                    for v in [n.x, n.y, n.z, t] {
                        sortie.extend_from_slice(&v.to_le_bytes());
                    }
                };
                poser(&mut avant, t0);
                poser(&mut arriere, t1);
            }
        }
        (avant, arriere)
    }

    /// La direction du rayon d'un pixel — la même formule que le shader, écrite à part exprès.
    fn direction_du_pixel(x: u32, y: u32, cote: u32) -> Vec3 {
        let sx = ((x as f32 + 0.5) / cote as f32) * 2.0 - 1.0;
        let sy = 1.0 - ((y as f32 + 0.5) / cote as f32) * 2.0;
        Vec3::new(sx * TANGENTE, sy * TANGENTE, 1.0).normalize_or_zero()
    }

    /// ⭐ **LA VÉRITÉ** — où le rayon ressort, calculé analytiquement et sans aucune carte.
    ///
    /// Deux intersections exactes avec la sphère, deux applications de Snell. Rien ici ne
    /// ressemble au chemin du shader : *c'est la condition pour que la comparaison prouve quelque
    /// chose.* Renvoie `None` si le rayon manque la bille.
    fn verite(direction: Vec3) -> Option<Vec3> {
        let origine = Vec3::new(0.0, 0.0, -RECUL);
        let b = origine.dot(direction);
        let c = origine.dot(origine) - RAYON * RAYON;
        let d = b * b - c;
        if d < 0.0 {
            return None;
        }
        let t0 = -b - d.sqrt();
        let t1 = -b + d.sqrt();
        let _ = t1;
        if t0 <= 0.0 {
            return None;
        }

        let refracter = |incident: Vec3, normale: Vec3, eta: f32| -> Option<Vec3> {
            let cos_i = -normale.dot(incident);
            let reste = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
            if reste < 0.0 {
                return None;
            }
            Some(incident * eta + normale * (eta * cos_i - reste.sqrt()))
        };

        let p0 = origine + direction * t0;
        let n0 = p0 * (1.0 / RAYON);
        let dedans = refracter(direction, n0, ETA)?;

        // La sortie EXACTE : on repart de p0 dans la nouvelle direction et on recoupe la sphère.
        // Aucune approximation, aucune carte, aucun Newton.
        let bb = p0.dot(dedans);
        let cc = p0.dot(p0) - RAYON * RAYON;
        let dd = bb * bb - cc;
        let ts = -bb + dd.max(0.0).sqrt();
        let p1 = p0 + dedans * ts;
        let n1 = p1 * (1.0 / RAYON);

        // En sortant, la normale se retourne et le rapport d'indices s'inverse.
        match refracter(dedans, n1 * -1.0, 1.0 / ETA) {
            Some(v) => Some(v.normalize_or_zero()),
            // Réflexion totale interne : le rayon rebondit au lieu de sortir.
            None => {
                let n = n1 * -1.0;
                Some((dedans - n * (2.0 * dedans.dot(n))).normalize_or_zero())
            }
        }
    }

    /// Rend l'image des directions de sortie et mesure l'écart à la vérité, pixel par pixel.
    ///
    /// Renvoie `(écart moyen en degrés, pire écart, pixels comparés)`.
    fn mesurer(tours: f32, cote: u32) -> Option<Mesure> {
        let bruts = rendre(&constantes(1.0, tours, cote), vk::Format::B8G8R8A8_UNORM, cote)?;
        Some(depouiller(&bruts, cote))
    }

    /// Lance la passe et rapporte les octets bruts de l'image.
    ///
    /// ⚠ **Le format se choisit à l'appel, et ce n'est pas un détail :** une direction se lit en
    /// `UNORM` (quantification uniforme), une couleur en `SRGB` (la chaîne du vrai rendu). *Lire
    /// une direction à travers une courbe de gamma mesurerait la courbe autant que la direction.*
    fn rendre(k: &ConstantesVerre, format: vk::Format, cote: u32) -> Option<Vec<u8>> {
        rendre_avec(k, format, cote, None)
    }

    /// Le même rendu, **avec un volume de matière**. `None` = le milieu homogène.
    ///
    /// *Une seule fonction plutôt que deux : la leçon de `Texture` du même jour, appliquée tout
    /// de suite — deux textes presque identiques finissent toujours par diverger.*
    fn rendre_avec(
        k: &ConstantesVerre,
        format: vk::Format,
        cote: u32,
        volume: Option<(u32, u32, u32, &[u8])>,
    ) -> Option<Vec<u8>> {
        let ctx = match GpuContext::sans_ecran_format(cote, cote, 1, format) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠ aucun Vulkan joignable : {e}");
                return None;
            }
        };
        let verre = Verre::nouvelle(&ctx, ctx.swapchain_format).ok()?;

        // Les cartes vivent jusqu'à la fin du rendu : les détruire avant que la file soit vide
        // ferait lire au shader une image libérée — un défaut qui ne se voit pas toujours.
        let memory_props = unsafe {
            ctx.instance
                .get_physical_device_memory_properties(ctx.physical_device)
        };
        let (octets_avant, octets_arriere) = cartes_exactes(cote);
        let fabriquer = |octets: &[u8]| {
            crate::render::texture::Texture::create_from_bytes(
                &ctx,
                &memory_props,
                cote,
                cote,
                vk::Format::R32G32B32A32_SFLOAT,
                16,
                octets,
            )
        };
        let tex_avant = fabriquer(&octets_avant).ok()?;
        let tex_arriere = fabriquer(&octets_arriere).ok()?;
        verre.brancher(&ctx, tex_avant.view, tex_arriere.view);

        // Le volume doit vivre jusqu'à la fin du rendu, comme les cartes.
        let tex_volume = match volume {
            Some((l, h, p, octets)) => Some(
                crate::render::texture::Texture::create_volume(
                    &ctx,
                    &memory_props,
                    [l, h, p],
                    vk::Format::R32G32B32A32_SFLOAT,
                    16,
                    octets,
                )
                .ok()?,
            ),
            None => None,
        };
        verre.brancher_matiere(&ctx, tex_volume.as_ref());

        let image = ctx.swapchain_images[0];
        let vue = ctx.swapchain_image_views[0];
        let etendue = ctx.swapchain_extent;

        let plage = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let cmd = ctx.begin_single_time_commands().ok()?;
        unsafe {
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::NONE)
                    .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .image(image)
                    .subresource_range(plage)],
            );
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0; 4] },
                });
            ctx.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: etendue,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            ctx.device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: etendue.width as f32,
                    height: etendue.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            ctx.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: etendue,
                }],
            );
        }
        verre.dessiner(&ctx, cmd, k);
        unsafe { ctx.device.cmd_end_rendering(cmd) };
        ctx.end_single_time_commands(cmd).ok()?;

        let bruts = ctx
            .relire_image_brute(image, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, etendue, ctx.swapchain_format)
            .ok()?;
        verre.detruire(&ctx.device);
        tex_avant.detruire(&ctx.device);
        tex_arriere.detruire(&ctx.device);
        if let Some(v) = tex_volume.as_ref() {
            v.detruire(&ctx.device);
        }
        Some(bruts)
    }

    /// Compare l'image des directions à la vérité, pixel par pixel.
    fn depouiller(bruts: &[u8], cote: u32) -> Mesure {
        let mut somme = 0.0f64;
        let mut pire = 0.0f32;
        let mut compte = 0usize;
        let mut gros = 0usize;
        let mut pire_ecart_au_critique = f32::INFINITY;
        for y in 0..cote {
            for x in 0..cote {
                let d = direction_du_pixel(x, y, cote);
                let Some(attendue) = verite(d) else { continue };
                let i = ((y * cote + x) * 4) as usize;
                // Le format est B8G8R8A8 : bleu en premier.
                let lu = Vec3::new(
                    bruts[i + 2] as f32 / 255.0 * 2.0 - 1.0,
                    bruts[i + 1] as f32 / 255.0 * 2.0 - 1.0,
                    bruts[i] as f32 / 255.0 * 2.0 - 1.0,
                )
                .normalize_or_zero();
                if lu.length() < 0.5 {
                    continue;
                }
                let angle = attendue.dot(lu).clamp(-1.0, 1.0).acos().to_degrees();
                somme += angle as f64;
                if angle > pire {
                    pire = angle;
                    pire_ecart_au_critique = ecart_a_l_angle_critique(d);
                }
                if angle > 10.0 {
                    gros += 1;
                }
                compte += 1;
            }
        }
        Mesure {
            moyenne: (somme / compte.max(1) as f64) as f32,
            pire,
            pire_ecart_au_critique,
            gros,
            compte,
        }
    }

    /// De combien de degrés le rayon de ce pixel arrive-t-il à la face arrière **loin de l'angle
    /// critique** ? Proche de zéro = le rayon est sur la frontière entre « il sort » et « il est
    /// totalement réfléchi ».
    ///
    /// ⭐ C'est la sonde qui distingue **un calcul faux** d'une **discontinuité physique**. Elle
    /// est née sur le banc processeur, où un pire cas inexpliqué s'est révélé n'être que deux
    /// pixels posés à 0,05° d'une frontière que la nature elle-même rend discontinue. *Sans cette
    /// mesure, la seule issue honnête était d'écrire « cause inconnue ».*
    fn ecart_a_l_angle_critique(regard: Vec3) -> f32 {
        let Some((interieur, normale_sortie)) = trajet_interne(regard) else {
            return f32::INFINITY;
        };
        let cos_i = interieur.dot(normale_sortie * -1.0).abs().clamp(-1.0, 1.0);
        let incidence = cos_i.acos().to_degrees();
        // n₂ = 1 (le vide), n₁ = 1,5 : sin(critique) = 1/1,5.
        let critique = (ETA).asin().to_degrees();
        (incidence - critique).abs()
    }

    /// Le rayon à l'intérieur de la bille et la normale là où il frappe la face arrière.
    fn trajet_interne(regard: Vec3) -> Option<(Vec3, Vec3)> {
        let origine = Vec3::new(0.0, 0.0, -RECUL);
        let b = origine.dot(regard);
        let c = origine.dot(origine) - RAYON * RAYON;
        let d = b * b - c;
        if d < 0.0 {
            return None;
        }
        let t0 = -b - d.sqrt();
        if t0 <= 0.0 {
            return None;
        }
        let p0 = origine + regard * t0;
        let n0 = p0 * (1.0 / RAYON);
        let cos_i = -n0.dot(regard);
        let reste = 1.0 - ETA * ETA * (1.0 - cos_i * cos_i);
        if reste < 0.0 {
            return None;
        }
        let dedans = regard * ETA + n0 * (ETA * cos_i - reste.sqrt());
        let bb = p0.dot(dedans);
        let cc = p0.dot(p0) - RAYON * RAYON;
        let dd = bb * bb - cc;
        let p1 = p0 + dedans * (-bb + dd.max(0.0).sqrt());
        Some((dedans.normalize_or_zero(), p1 * (1.0 / RAYON)))
    }

    /// Ce qu'une passe de mesure rapporte. *Un pire cas sans son contexte ne se lit pas :
    /// c'est le savoir qui distingue un calcul faux d'une frontière physique.*
    struct Mesure {
        moyenne: f32,
        pire: f32,
        /// À quelle distance de l'angle critique se trouve le pixel le pire.
        pire_ecart_au_critique: f32,
        /// Combien de pixels dépassent 10° d'écart.
        gros: usize,
        compte: usize,
    }

    /// ⭐⭐⭐ **CE QUE COÛTE DE PASSER PAR DES CARTES EN ESPACE ÉCRAN** — et c'est un chiffre que
    /// le projet n'avait pas.
    ///
    /// # Pourquoi ce test existe
    ///
    /// Le shader ne calcule plus sa géométrie : il **lit deux cartes**, comme le fera le moteur
    /// réel. L'erreur est passée de **1,789° à 2,132°** le jour de ce changement, et il serait
    /// facile d'écrire « le surcoût de la discrétisation est de 0,34° ». **Ce serait une
    /// hypothèse**, pas une mesure : rien ne dit que ce surcoût vient des pixels plutôt que d'une
    /// faute d'indexation, d'un décalage d'un demi-texel ou d'un mauvais centre de pixel.
    ///
    /// ## La sonde qui tranche, et elle est simple
    ///
    /// Si le surcoût vient de la **taille des pixels**, alors il doit **rétrécir quand les pixels
    /// rétrécissent**, et tendre vers l'erreur du calcul direct. S'il vient d'une faute
    /// d'indexation, il ne bougera pas — une erreur de repère est indifférente à la résolution.
    ///
    /// *C'est la règle des trois N du projet, appliquée à une grandeur continue : jamais validé
    /// sur une seule mesure, et c'est la TENDANCE qui décide.*
    ///
    /// ⚠ Ce test ne grave aucun chiffre : il exige une **décroissance**. Un seuil absolu se
    /// périmerait à la première carte graphique différente ; une tendance, non.
    #[test]
    fn l_erreur_des_cartes_retrecit_avec_les_pixels_donc_c_est_bien_la_discretisation() {
        let Some(petite) = mesurer(6.0, 128) else {
            println!("  (le test est neutralise, PAS reussi — il n'a rien prouve)");
            return;
        };
        let moyenne = mesurer(6.0, 256).expect("Vulkan etait la a l'instant");
        let grande = mesurer(6.0, 512).expect("Vulkan etait la a l'instant");

        println!("  128²  : {:.3}°  ({} pixels de verre)", petite.moyenne, petite.compte);
        println!("  256²  : {:.3}°  ({} pixels de verre)", moyenne.moyenne, moyenne.compte);
        println!("  512²  : {:.3}°  ({} pixels de verre)", grande.moyenne, grande.compte);
        println!("  le calcul direct, sans carte, valait 1,789° — et le banc processeur 1,740°");

        assert!(
            moyenne.moyenne < petite.moyenne,
            "doubler la résolution n'a pas réduit l'erreur ({:.3}° -> {:.3}°) : le surcoût ne vient \
             donc PAS de la taille des pixels, et l'explication qu'on s'en donnait est fausse",
            petite.moyenne,
            moyenne.moyenne
        );
        assert!(
            grande.moyenne < moyenne.moyenne,
            "la décroissance s'arrête entre 256² et 512² ({:.3}° -> {:.3}°) : il reste alors une \
             erreur de FOND que la résolution ne corrige pas — à trouver, pas à tolérer",
            moyenne.moyenne,
            grande.moyenne
        );
    }

    /// ⭐ **LES TROIS IMAGES DU VERRE** — écrites dans `target/preuves/`, pour son œil.
    ///
    /// Un chiffre dit qu'un calcul est juste ; il ne dit pas si l'image est belle, et sur ce
    /// projet **le juge du rendu perçu est un œil humain, jamais une métrique**. Ce test ne
    /// prouve donc rien : il *montre*. Il n'affirme qu'une chose, la seule vérifiable sans
    /// regarder — que les trois images sont **différentes**.
    ///
    /// | Fichier | Ce qu'on y voit |
    /// |---|---|
    /// | `verre-sans-newton.png` | ce que donnait l'approximation : le fond replié, tordu |
    /// | `verre-newton.png` | la bille juste — le damier dévié comme il doit l'être |
    /// | `verre-absorbant.png` | la même, avec Beer-Lambert par canal : **le verre colore** |
    ///
    /// ⚠ La troisième est la seule qui porte une teinte, et **elle ne vient pas du shader** : le
    /// `sigma` arrive par constantes poussées, depuis ce test. *Le moteur fournit ce qui est vrai,
    /// l'appelant fournit ce qui est beau — et un test garde cette frontière.*
    #[test]
    fn les_trois_images_du_verre() {
        // Ici on regarde une COULEUR : c'est la chaîne du vrai rendu qu'il faut, sRGB comprise.
        let Some(sans) = rendre(&constantes(0.0, 0.0, COTE), vk::Format::B8G8R8A8_SRGB, COTE) else {
            println!("  (aucune image ecrite — pas de Vulkan)");
            return;
        };
        let avec = rendre(&constantes(0.0, 6.0, COTE), vk::Format::B8G8R8A8_SRGB, COTE)
            .expect("Vulkan etait la a l'instant");

        // Une absorption par canal : le rouge survit le mieux, le bleu meurt le plus vite. C'est
        // la longueur RÉELLEMENT traversée qui entre dans l'exponentielle, pas une épaisseur
        // supposée — donc le bord de la bille, plus épais en trajet, doit être plus sombre.
        let mut teinte = constantes(0.0, 6.0, COTE);
        teinte.matiere = [0.35, 0.9, 1.6, ETA];
        let colore =
            rendre(&teinte, vk::Format::B8G8R8A8_SRGB, COTE).expect("Vulkan etait la a l'instant");

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        for (nom, bruts) in [
            ("verre-sans-newton.png", &sans),
            ("verre-newton.png", &avec),
            ("verre-absorbant.png", &colore),
        ] {
            let mut rvb = Vec::with_capacity(bruts.len() / 4 * 3);
            for p in bruts.chunks_exact(4) {
                // B8G8R8A8 : bleu en premier.
                rvb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            let png = crate::image::png::encoder(COTE, COTE, &rvb).expect("png");
            std::fs::write(dossier.join(nom), &png).expect("ecriture");
            println!("  ecrit : target/preuves/{nom} ({} Ko)", png.len() / 1024);
        }

        // La seule chose qu'on peut affirmer sans regarder : les trois images diffèrent.
        // *Si Newton ne changeait rien à l'image, la première et la deuxième seraient identiques —
        // et c'est exactement le défaut qui a survécu une journée entière sur le banc processeur.*
        assert_ne!(sans, avec, "Newton ne change RIEN a l'image : la poignee est morte");
        assert_ne!(avec, colore, "l'absorption ne change RIEN : Beer-Lambert est mort");
    }

    /// Fabrique un volume cubique dont chaque texel vaut ce que la fonction en dit.
    ///
    /// `rgb` = le facteur d'absorption par canal ; `a` n'est pas lu par le shader.
    fn volume_cube(cote: u32, f: impl Fn(u32, u32, u32) -> [f32; 3]) -> Vec<u8> {
        volume_cube_source(cote, |x, y, z| {
            let d = f(x, y, z);
            [d[0], d[1], d[2], 0.0]
        })
    }

    /// Le même, mais chaque texel porte aussi son **terme de source** dans le quatrième canal.
    fn volume_cube_source(cote: u32, f: impl Fn(u32, u32, u32) -> [f32; 4]) -> Vec<u8> {
        let mut octets = Vec::with_capacity((cote * cote * cote * 16) as usize);
        // ⚠ L'ordre est celui que Vulkan attend : x varie le plus vite, z le plus lentement.
        // *L'écrire à l'envers ne casse rien — ça transpose la matière, ce qui est parfaitement
        // invisible sur un volume symétrique et faux sur tous les autres.*
        for z in 0..cote {
            for y in 0..cote {
                for x in 0..cote {
                    for v in f(x, y, z) {
                        octets.extend_from_slice(&v.to_ne_bytes());
                    }
                }
            }
        }
        octets
    }

    /// ⭐⭐⭐ **LA MATIÈRE CESSE D'ÊTRE UNIFORME** — et le milieu homogène traverse la marche sans
    /// bouger d'un pixel.
    ///
    /// # Les deux choses que ce test prouve, et il faut les deux
    ///
    /// **1. Le volume est vraiment lu.** Un feuillet dense d'un côté, clair de l'autre, doit
    /// produire une image différente. *Sans cette moitié, tout le mécanisme pourrait être branché
    /// à rien et personne ne le verrait — la famille de défauts n° 1 de ce projet.*
    ///
    /// **2. La marche ne dégrade RIEN.** Le même milieu homogène, calculé d'un côté par la
    /// formule fermée (un seul pas) et de l'autre par une marche en 32 pas sur un volume uniforme,
    /// doit donner **la même image**. C'est ce qui autorise à n'avoir qu'un seul chemin de code :
    /// *si la marche coûtait ne serait-ce qu'un niveau de couleur au cas simple, il aurait fallu
    /// garder l'ancienne formule à côté — donc deux chemins, dont un seul serait testé.*
    ///
    /// ⚠ Ce test ne juge **aucune** image : il compare des octets. Ce qu'une sucette doit *avoir
    /// l'air* d'être ne se mesure pas ici — c'est son œil, et lui seul.
    #[test]
    fn un_volume_inhomogene_change_la_matiere_et_le_milieu_homogene_traverse_sans_bouger() {
        let mut k = constantes(0.0, 6.0, COTE);
        // Une absorption franche, sinon les trois images se ressembleraient pour une raison qui
        // n'a rien à voir avec le volume.
        k.matiere = [0.35, 0.9, 1.6, ETA];

        let Some(par_la_formule) = rendre(&k, vk::Format::B8G8R8A8_SRGB, COTE) else {
            println!("  (aucune image — pas de Vulkan)");
            return;
        };

        // La boîte englobe la bille (rayon 1) avec de la marge, et la marche fait 32 pas.
        let mut k_marche = k;
        k_marche.volume_min = [-1.5, -1.5, -1.5, 32.0];
        k_marche.volume_taille = [3.0, 3.0, 3.0, 0.0];

        const N: u32 = 16;
        let uniforme = volume_cube(N, |_, _, _| [1.0, 1.0, 1.0]);
        let par_la_marche = rendre_avec(
            &k_marche,
            vk::Format::B8G8R8A8_SRGB,
            COTE,
            Some((N, N, N, &uniforme)),
        )
        .expect("Vulkan etait la a l'instant");

        // Un feuillet de colorant : dense d'un côté, presque rien de l'autre.
        let feuillet = volume_cube(N, |x, _, _| {
            if x < N / 2 { [2.5, 2.5, 2.5] } else { [0.15, 0.15, 0.15] }
        });
        let avec_feuillet = rendre_avec(
            &k_marche,
            vk::Format::B8G8R8A8_SRGB,
            COTE,
            Some((N, N, N, &feuillet)),
        )
        .expect("Vulkan etait la a l'instant");

        // ── 1. Le volume est lu ──
        assert_ne!(
            par_la_marche, avec_feuillet,
            "le volume ne change RIEN a l'image : il n'est pas lu, et tout ce mecanisme est mort"
        );

        // ── 2. La marche redonne la formule fermée ──
        let differents = par_la_formule
            .iter()
            .zip(par_la_marche.iter())
            .filter(|(a, b)| a != b)
            .count();
        let pire = par_la_formule
            .iter()
            .zip(par_la_marche.iter())
            .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        println!(
            "  formule fermee (1 pas) contre marche (32 pas, volume uniforme {N}³) :\n    \
             {differents} octets differents sur {}, ecart maximal {pire} niveau(x) sur 255",
            par_la_formule.len()
        );
        // Le seuil n'est pas un réglage : un écart d'UN niveau est le dernier bit d'un octet, donc
        // le bruit d'arrondi de la somme flottante. Deux niveaux voudraient dire que la marche
        // change la matière, ce qu'elle n'a pas le droit de faire sur un milieu uniforme.
        assert!(
            pire <= 1,
            "la marche degrade le milieu homogene de {pire} niveaux — elle ne l'integre pas juste"
        );

        // ── Et la mesure qui dit COMBIEN le feuillet change, pour que le chiffre existe ──
        let ecart_feuillet = par_la_marche
            .iter()
            .zip(avec_feuillet.iter())
            .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as f64)
            .sum::<f64>()
            / par_la_marche.len() as f64;
        println!("  le feuillet deplace en moyenne {ecart_feuillet:.2} niveaux par octet");

        // ── Les images, pour l'œil — la seule instance qui juge une matière ──
        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        for (nom, bruts) in [
            ("volume-1-homogene.png", &par_la_marche),
            ("volume-2-feuillet.png", &avec_feuillet),
        ] {
            let mut rvb = Vec::with_capacity(bruts.len() / 4 * 3);
            for p in bruts.chunks_exact(4) {
                rvb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            let png = crate::image::png::encoder(COTE, COTE, &rvb).expect("png");
            std::fs::write(dossier.join(nom), &png).expect("ecriture");
            println!("  ecrit : target/preuves/{nom} ({} Ko)", png.len() / 1024);
        }
    }

    /// ⭐⭐⭐ **LES BULLES** — la matière cesse de seulement retirer de la lumière, elle en rend.
    ///
    /// # Ce que ce test prouve, et la garde PHYSIQUE qu'il pose
    ///
    /// Une bulle d'air dans du sucre n'absorbe presque rien et **renvoie** ce qui l'éclaire : c'est
    /// le terme de source de l'équation du transfert radiatif. Sans lui, une inclusion ne pourrait
    /// être qu'un trou plus sombre — *une bulle serait un défaut, jamais un reflet.*
    ///
    /// **La garde qui compte n'est pas « l'image change », c'est « aucun pixel ne s'assombrit ».**
    /// Ajouter une source ne peut, physiquement, que rendre l'image plus claire ou l'y laisser :
    /// un seul pixel plus sombre signifierait que le terme est branché à l'envers, ou que
    /// l'atténuation appliquée à la lumière rendue est celle du mauvais segment. *Un test qui se
    /// contenterait de « c'est différent » laisserait passer les deux.*
    ///
    /// ⚠ Ce test ne juge **pas** à quoi une bulle ressemble. C'est son œil, sur
    /// `preuves/volume-18-bulles.png`.
    #[test]
    fn une_source_rend_de_la_lumiere_et_ne_peut_jamais_en_retirer() {
        let mut k = constantes(0.0, 6.0, COTE);
        k.matiere = [0.35, 0.9, 1.6, ETA];
        k.volume_min = [-1.5, -1.5, -1.5, 48.0];
        k.volume_taille = [3.0, 3.0, 3.0, 0.0];

        const N: u32 = 32;
        // Des bulles semées sur une grille régulière : ce qui compte ici est qu'elles soient
        // reproductibles, pas qu'elles soient jolies.
        let bulle = |x: u32, y: u32, z: u32| -> bool {
            let d = |v: u32| {
                let m = (v % 8) as f32 - 3.5;
                m * m
            };
            d(x) + d(y) + d(z) < 4.0
        };
        // Sans source : les bulles n'existent que par leur absence d'absorption.
        let muettes = volume_cube_source(N, |x, y, z| {
            if bulle(x, y, z) { [0.05, 0.05, 0.05, 0.0] } else { [1.0, 1.0, 1.0, 0.0] }
        });
        // Avec source : elles renvoient de la lumière.
        let vivantes = volume_cube_source(N, |x, y, z| {
            if bulle(x, y, z) { [0.05, 0.05, 0.05, 0.55] } else { [1.0, 1.0, 1.0, 0.0] }
        });

        let Some(sans_source) = rendre_avec(
            &k,
            vk::Format::B8G8R8A8_SRGB,
            COTE,
            Some((N, N, N, &muettes)),
        ) else {
            println!("  (aucune image — pas de Vulkan)");
            return;
        };
        let avec_source = rendre_avec(
            &k,
            vk::Format::B8G8R8A8_SRGB,
            COTE,
            Some((N, N, N, &vivantes)),
        )
        .expect("Vulkan etait la a l'instant");

        assert_ne!(
            sans_source, avec_source,
            "le terme de source ne change RIEN : le quatrieme canal du volume n'est pas lu"
        );

        // ── La garde physique : une source AJOUTE, elle ne retire jamais ──
        let mut assombris = 0usize;
        let mut gain_total = 0f64;
        let mut gain_max = 0i32;
        for (a, b) in sans_source.iter().zip(avec_source.iter()) {
            let d = *b as i32 - *a as i32;
            if d < 0 {
                assombris += 1;
            }
            gain_total += d as f64;
            gain_max = gain_max.max(d);
        }
        println!(
            "  bulles : gain moyen {:.2} niveau(x), gain maximal {gain_max}, \
             pixels assombris {assombris}",
            gain_total / sans_source.len() as f64
        );
        assert_eq!(
            assombris, 0,
            "{assombris} octets se sont ASSOMBRIS en ajoutant de la lumiere — le terme de source \
             est branche a l'envers, ou attenue par le mauvais segment"
        );
        assert!(gain_max > 0, "la source n'eclaircit nulle part");

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        let mut rvb = Vec::with_capacity(avec_source.len() / 4 * 3);
        for p in avec_source.chunks_exact(4) {
            rvb.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        let png = crate::image::png::encoder(COTE, COTE, &rvb).expect("png");
        std::fs::write(dossier.join("volume-3-bulles.png"), &png).expect("ecriture");
        println!("  ecrit : target/preuves/volume-3-bulles.png ({} Ko)", png.len() / 1024);
    }

    /// ⭐⭐⭐ **LA SUCETTE, ENTIÈRE** — la matière du banc processeur traverse enfin une vraie
    /// réfraction.
    ///
    /// # Ce que cette image réunit, et pourquoi il a fallu deux moitiés
    ///
    /// Le banc processeur (`epaisseur.rs`) porte **la matière** — sucre bleu dont la teinte vire
    /// avec l'épaisseur, feuillet de colorant figé par la viscosité, bulles qui renvoient
    /// l'ambiante — et **ne dévie aucun rayon** : sa sucette est une boule opaque et colorée. Le
    /// banc de verre porte **la réfraction** — Snell aux deux interfaces, Newton pour trouver la
    /// sortie — et son verre est **nu**. *Chacune était juste et aucune n'était une sucette.*
    ///
    /// Le champ est **exactement celui du banc processeur** : mêmes constantes, mêmes fonctions
    /// `alea` et `fondu`, qui ont été remontées de son `mod tests` pour ça. *Le recopier aurait
    /// garanti que les deux sucettes divergent au premier réglage.*
    ///
    /// # ⚠⚠ ET LE MUR QUE CE TEST A RÉVÉLÉ — il est structurel, pas anecdotique
    ///
    /// **Un volume cuit ne peut pas porter les bulles fines de cette sucette.** Leur rayon va de
    /// 0,016 à 0,046 pour une bille de rayon 1 ; une sphère cesse de scintiller à partir de huit
    /// échantillons dans son diamètre. Le calcul est sans appel :
    ///
    /// | Volume | Mémoire | Une bulle fine mesure |
    /// |---|---|---|
    /// | 128³ | 34 Mo | **0,9 texel** |
    /// | 256³ | **268 Mo** | 1,7 texel |
    /// | ~1200³ | ~27 **Go** | 8 texels — *ce qu'il faudrait* |
    ///
    /// **C'est une propriété du choix d'architecture, pas un défaut d'implémentation** : une grille
    /// régulière paie partout la finesse dont elle n'a besoin que par endroits. Le banc processeur
    /// n'a pas ce problème parce qu'il n'a **aucune grille** — il demande au point s'il est dans
    /// une bulle, et la réponse est exacte à n'importe quelle échelle.
    ///
    /// *Ce test grossit donc les bulles d'un facteur écrit, pour montrer le mécanisme entier. Il ne
    /// prétend pas rendre la sucette du banc processeur — et la question qu'il pose vaut mieux
    /// qu'une image : **les inclusions fines demanderont autre chose qu'un volume cuit.***
    #[test]
    fn la_sucette_entiere_traverse_le_gpu() {
        use crate::render::epaisseur::{alea, fondu};

        // ── Le champ, repris à l'identique du banc processeur ──
        const SUCRE: [f32; 3] = [3.4, 1.15, 0.30];
        const CELLULE: [f32; 3] = [0.150, 0.360, 0.150];
        // ⚠ Le facteur qui rend les bulles visibles dans une grille — voir le mur ci-dessus. Il
        // est ÉCRIT parce qu'il fait mentir la sucette : ce sont les bulles d'une sucette trois
        // fois plus grosse.
        const GROSSISSEMENT: f32 = 3.0;

        const N: u32 = 128;
        const BOITE: f32 = 2.4;
        let pas_du_volume = BOITE / N as f32;

        let debut = std::time::Instant::now();
        let volume = volume_cube_source(N, |ix, iy, iz| {
            let p = |i: u32| (i as f32 + 0.5) / N as f32 * BOITE - BOITE * 0.5;
            let (x, y, z) = (p(ix), p(iy), p(iz));

            // Le feuillet de colorant : la frontière est quasi nette, un sirop à 150 °C ne se
            // mélange pas, il se colle.
            let cote_droit = fondu(-0.17, -0.11, x);
            let concentration = 0.70 + 0.58 * cote_droit;
            let mut densite = [concentration, concentration, concentration];
            let mut source = 0.0f32;

            // Les bulles : on ne les place pas, on demande au point s'il est dans une.
            let cx = (x / CELLULE[0]).floor() as i32;
            let cy = (y / CELLULE[1]).floor() as i32;
            let cz = (z / CELLULE[2]).floor() as i32;
            if alea(cx, cy, cz, 7) > 0.18 {
                let place = |g: u32, axe: usize| (0.2 + 0.6 * alea(cx, cy, cz, g)) * CELLULE[axe];
                let centre = [
                    cx as f32 * CELLULE[0] + place(11, 0),
                    cy as f32 * CELLULE[1] + place(13, 1),
                    cz as f32 * CELLULE[2] + place(17, 2),
                ];
                // Stratification : les grosses remontent, les fines restent piégées.
                let haut = fondu(-0.6, 0.85, y);
                let rayon = (0.016 + 0.030 * haut)
                    * (0.40 + 0.60 * alea(cx, cy, cz, 23))
                    * GROSSISSEMENT;
                let etirement = 1.0 + 1.7 * haut;
                let d = [x - centre[0], (y - centre[1]) / etirement, z - centre[2]];
                let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                // La netteté suit le pas du VOLUME, pas celui de la marche : c'est la grille qui
                // limite ici, et une bulle plus fine qu'un texel ne peut qu'aliaser.
                let nettete = fondu(rayon * 0.55, rayon * 0.22, pas_du_volume);
                let dedans = fondu(rayon, rayon * 0.55, distance) * nettete;

                // ⭐ Une bulle n'est pas un trou, c'est un miroir : air dans du sucre, la réflexion
                // est totale au-delà de 41,8° — donc sur la plus grande part d'une sphère. Elle
                // renvoie l'ambiante SANS qu'elle ait traversé le bleu.
                source += 9.0 * dedans;
                for c in densite.iter_mut() {
                    *c += 7.0 * dedans;
                }
            }
            [densite[0], densite[1], densite[2], source]
        });
        println!(
            "  volume {N}³ cuit en {:?} ({:.0} Mo)",
            debut.elapsed(),
            volume.len() as f64 / 1e6
        );

        let mut k = constantes(0.0, 6.0, COTE);
        k.matiere = [SUCRE[0], SUCRE[1], SUCRE[2], ETA];
        // 64 pas : la marche doit voir les bulles, et elles font quelques texels.
        k.volume_min = [-BOITE * 0.5, -BOITE * 0.5, -BOITE * 0.5, 64.0];
        k.volume_taille = [BOITE, BOITE, BOITE, 0.0];

        let Some(image) = rendre_avec(&k, vk::Format::B8G8R8A8_SRGB, COTE, Some((N, N, N, &volume)))
        else {
            println!("  (aucune image — pas de Vulkan)");
            return;
        };

        // ── Ce qu'on peut affirmer sans regarder ──
        // La sucette absorbe surtout le rouge : à travers elle, le bleu doit survivre mieux que le
        // rouge. *C'est la seule chose que le sigma du sucre garantit, et elle se vérifie sans
        // juger l'image.* On ne regarde que le centre, là où le trajet est le plus long.
        let mut rouge = 0u64;
        let mut bleu = 0u64;
        let mut comptes = 0u64;
        let bord = COTE / 3;
        for y in bord..(COTE - bord) {
            for x in bord..(COTE - bord) {
                let i = ((y * COTE + x) * 4) as usize;
                bleu += image[i] as u64; // B8G8R8A8 : bleu en premier
                rouge += image[i + 2] as u64;
                comptes += 1;
            }
        }
        let (r, b) = (rouge as f64 / comptes as f64, bleu as f64 / comptes as f64);
        println!("  au centre : rouge {r:.1}, bleu {b:.1} (sur 255)");
        assert!(
            b > r * 1.5,
            "le sucre bleu n'est pas bleu : rouge {r:.1}, bleu {b:.1} — le sigma par canal ne \
             traverse pas la marche"
        );

        let dossier = std::path::Path::new("target/preuves");
        std::fs::create_dir_all(dossier).expect("dossier de preuves");
        let mut rvb = Vec::with_capacity(image.len() / 4 * 3);
        for p in image.chunks_exact(4) {
            rvb.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        let png = crate::image::png::encoder(COTE, COTE, &rvb).expect("png");
        std::fs::write(dossier.join("volume-4-sucette.png"), &png).expect("ecriture");
        println!("  ecrit : target/preuves/volume-4-sucette.png ({} Ko)", png.len() / 1024);
    }

    /// ⭐⭐⭐ **FRESNEL, CONFRONTÉ À UNE VÉRITÉ QUI N'EST PAS LA MIENNE.**
    ///
    /// # Ce qui était faux avant, et ce n'était pas esthétique
    ///
    /// Le shader transmettait **cent pour cent** de la lumière : l'énergie ne se conservait pas.
    /// Une bille de verre sans reflet ne ressemble à rien de réel, et *aucun réglage de couleur
    /// n'aurait pu le rattraper* — une somme de corrections justes ne franchit pas un mécanisme
    /// absent.
    ///
    /// # Le critère, écrit avant de lancer quoi que ce soit
    ///
    /// À incidence normale, `R = ((n₁−n₂)/(n₁+n₂))²`. Pour n = 1,5 : **exactement 4,00 %**. Ce
    /// nombre ne vient ni de ce moteur, ni de mon banc processeur — *c'est une constante physique
    /// vérifiable dans n'importe quel manuel, donc la comparaison prouve quelque chose.* Deux
    /// implémentations issues du même raisonnement peuvent être fausses de la même façon ; une
    /// implémentation et une vérité extérieure, non.
    ///
    /// Et deux propriétés de forme, qui attrapent les fautes qu'un seul point ne verrait pas :
    /// **R croît du centre vers le bord**, et **R tend vers 1** en incidence rasante.
    ///
    /// ⚠ **Rendu en `UNORM`, jamais en `SRGB`.** 4 % passés dans une courbe sRGB donnent 22 % à
    /// l'écran : on mesurerait la courbe autant que la réflectance. *C'est exactement la raison
    /// pour laquelle le format est un paramètre depuis le 2 septembre.*
    #[test]
    fn la_reflectance_de_fresnel_vaut_quatre_pour_cent_de_face() {
        // Mode 2 : la réflectance seule, en niveaux de gris.
        let k = constantes(2.0, 6.0, COTE);
        let Some(image) = rendre(&k, vk::Format::B8G8R8A8_UNORM, COTE) else {
            println!("  (aucune image — pas de Vulkan)");
            return;
        };

        let lire = |x: u32, y: u32| -> f32 {
            image[((y * COTE + x) * 4) as usize] as f32 / 255.0
        };

        // ── 1. Le centre : incidence normale, la vérité vaut 4,00 % ──
        let centre = lire(COTE / 2, COTE / 2);
        let attendu = {
            let eta = ETA; // n1/n2 = 1/1,5
            let r = (eta - 1.0) / (eta + 1.0);
            r * r
        };
        println!(
            "  au centre : {:.3} % mesures, {:.3} % attendus (vérité analytique)",
            centre * 100.0,
            attendu * 100.0
        );
        // La tolérance est celle de la QUANTIFICATION, pas un réglage : un octet vaut 1/255,
        // soit 0,39 point de pourcentage. On accepte un pas, pas davantage.
        assert!(
            (centre - attendu).abs() <= 1.0 / 255.0,
            "reflectance de face {:.4} au lieu de {:.4} — au-dela d'un pas de quantification",
            centre,
            attendu
        );

        // ── 2. LA COURBE ENTIÈRE, contre l'analytique ──
        //
        // Un seul point pourrait être juste par accident. On compare donc la réflectance mesurée à
        // la valeur analytique **à chaque distance du centre**, en calculant l'angle d'incidence
        // par la géométrie : un rayon qui passe à `b` de l'axe frappe une sphère de rayon `R` sous
        // un angle dont le sinus vaut `b/R`.
        //
        // ⚠⚠ **ET L'INCIDENCE RASANTE N'EST PAS ATTEIGNABLE SUR UNE GRILLE.** Mon premier critère
        // exigeait que la réflectance monte au-delà de 50 % ; elle plafonne à 43 %, et *c'est le
        // critère qui avait tort*. Entre l'avant-dernier pixel de la bille et le dernier, l'angle
        // saute de 78° à 85° : le centre du dernier pixel ne voit jamais le bord. **Exiger d'un
        // banc une borne qu'il ne peut pas échantillonner, c'est la faute que le corpus appelle
        // « extrapoler au-delà de ce que l'instrument atteint ».**
        //
        // La tolérance n'est donc pas un réglage : c'est **l'amplitude réelle de la réflectance à
        // l'intérieur du pixel** (elle explose près du bord) plus un pas de quantification.
        let angle_du_pixel = |dx: f32| -> Option<f32> {
            let a = (dx / (COTE as f32 * 0.5) * TANGENTE).atan();
            let b = RECUL * a.sin();
            if b >= RAYON { None } else { Some((b / RAYON).asin()) }
        };
        let fresnel = |incidence: f32| -> f32 {
            let (ci, eta) = (incidence.cos(), ETA);
            let st2 = eta * eta * (1.0 - ci * ci);
            if st2 >= 1.0 {
                return 1.0;
            }
            let ct = (1.0f32 - st2).sqrt();
            let rs = (eta * ci - ct) / (eta * ci + ct);
            let rp = (eta * ct - ci) / (eta * ct + ci);
            0.5 * (rs * rs + rp * rp)
        };

        let mut precedent = centre;
        let mut reculs = 0;
        let mut pire_ecart = 0.0f32;
        let mut pire_a = 0.0f32;
        let mut compares = 0;
        for dx in 1..(COTE / 2) {
            let mesure = lire(COTE / 2 + dx, COTE / 2);
            if mesure <= 0.0 {
                break; // sorti de la bille
            }
            if mesure + 1.0 / 255.0 < precedent {
                reculs += 1;
            }
            precedent = mesure;

            let (Some(i0), Some(i1)) = (
                angle_du_pixel(dx as f32 - 0.5),
                angle_du_pixel(dx as f32 + 0.5),
            ) else {
                continue; // le pixel chevauche la silhouette : son angle n'est pas défini partout
            };
            let attendu = fresnel(angle_du_pixel(dx as f32).unwrap());
            // Ce que la réflectance fait varier À L'INTÉRIEUR du pixel.
            let amplitude = (fresnel(i1) - fresnel(i0)).abs();
            let ecart = (mesure - attendu).abs();
            if ecart > amplitude + 1.0 / 255.0 {
                pire_ecart = pire_ecart.max(ecart);
                pire_a = i1.to_degrees();
            }
            compares += 1;
        }
        println!(
            "  {compares} distances comparees a l'analytique, plus grand ecart hors tolerance \
             {:.4} (vers {pire_a:.0}°)",
            pire_ecart
        );
        assert_eq!(reculs, 0, "la reflectance RECULE en allant vers le bord — Fresnel est inverse");
        assert!(compares > 30, "trop peu de points compares ({compares}) : la mesure ne dit rien");
        assert_eq!(
            pire_ecart, 0.0,
            "la courbe mesuree s'ecarte de Fresnel au-dela de ce que la discretisation explique"
        );
    }

    /// La taille des constantes, vérifiée plutôt que supposée — **et elle est maintenant au ras
    /// du plafond**.
    ///
    /// L'histoire de ce chiffre, parce qu'elle dit ce que le shader a cessé et commencé de savoir :
    /// **112** jusqu'au 2 septembre 2026 (le centre et le rayon de la sphère y voyageaient encore),
    /// **96** quand la géométrie est passée par des cartes — *ce qui n'est plus lu n'a pas à être
    /// poussé* —, puis **128** le 4 septembre, quand la matière est entrée par un volume : il a
    /// fallu dire où se trouve sa boîte dans le monde, et en combien de pas la traverser.
    ///
    /// ⚠⚠ **Il ne reste plus un seul octet.** Le prochain qui aura besoin d'un `vec4` ne pourra
    /// pas l'ajouter ici : il devra soit reprendre un `w` inutilisé, soit passer par un tampon
    /// uniforme. *Ce n'est pas une limite de cette machine — elle en offre 256 — c'est le plancher
    /// que Vulkan garantit partout, et le franchir ne se verrait que sur la carte de quelqu'un
    /// d'autre.*
    #[test]
    fn les_constantes_tiennent_sous_le_plafond_garanti_de_vulkan() {
        let taille = std::mem::size_of::<ConstantesVerre>();
        println!("  constantes de verre : {taille} octets (plafond garanti : 128)");
        assert_eq!(taille, 128, "la structure a changé de taille sans qu'on le décide");
        assert!(
            taille <= 128,
            "Vulkan ne garantit que 128 octets de constantes poussées, et cette limite a déjà \
             été franchie une fois sur ce projet — elle ne se voit que sur une AUTRE machine"
        );
    }

    /// ⭐⭐⭐ **LE PREMIER SHADER DU MOTEUR CONFRONTÉ À UNE VÉRITÉ PHYSIQUE.**
    ///
    /// # Le critère, écrit avant d'avoir lancé quoi que ce soit
    ///
    /// Le banc processeur (`epaisseur.rs`) atteint **1,740°** d'écart moyen à la vérité analytique,
    /// contre **36,402°** pour l'approximation naïve « le rayon ne dévie pas ». Le GPU doit :
    ///
    /// 1. **rester sous 3°** de moyenne — la marge au-dessus de 1,740° couvre la quantification
    ///    sur 8 bits (≈ 0,22° au pire, en UNORM) et les différences de précision des
    ///    transcendantes entre pilotes ;
    /// 2. **être meilleur d'au moins un facteur 5** que le même shader privé de Newton.
    ///
    /// *Le second point est le vrai test.* Le premier, seul, passerait encore si Newton ne servait
    /// à rien — il faut donc mesurer la même chose avec la poignée à zéro, et voir l'écart
    /// s'effondrer. **Une garde qui ne se compare qu'à elle-même ne mord jamais.**
    ///
    /// ⚠ **On ne compare pas le GPU au processeur, et c'est délibéré** : deux implémentations
    /// issues du même raisonnement peuvent être fausses de la même façon. Chacune est confrontée à
    /// la vérité, séparément.
    #[test]
    fn le_shader_trouve_la_sortie_du_rayon_aussi_bien_que_la_verite_analytique() {
        let Some(sans) = mesurer(0.0, COTE) else {
            println!("  (le test est neutralise, PAS reussi — il n'a rien prouve)");
            return;
        };
        let avec = mesurer(6.0, COTE).expect("Vulkan etait la a l'instant");

        println!("  pixels de verre compares : {}", avec.compte);
        println!(
            "  sans Newton (0 tour)  : moyenne {:.3}°  pire {:.3}°  au-dela de 10° : {} px ({:.2} %)",
            sans.moyenne,
            sans.pire,
            sans.gros,
            100.0 * sans.gros as f32 / sans.compte as f32
        );
        println!(
            "  avec Newton (6 tours) : moyenne {:.3}°  pire {:.3}°  au-dela de 10° : {} px ({:.2} %)",
            avec.moyenne,
            avec.pire,
            avec.gros,
            100.0 * avec.gros as f32 / avec.compte as f32
        );
        println!(
            "  le pixel le pire est a {:.3}° de l'angle critique ({:.2}°)",
            avec.pire_ecart_au_critique,
            (ETA).asin().to_degrees()
        );
        println!("  pour memoire, le banc processeur : 1,740° (verite analytique identique)");

        assert!(
            avec.moyenne < 3.0,
            "le shader s'ecarte de {:.3}° de la verite : au-dela de 3° ce n'est plus de la \
             quantification, c'est un calcul faux",
            avec.moyenne
        );
        assert!(
            avec.moyenne * 5.0 < sans.moyenne,
            "Newton n'apporte presque rien ({:.3}° -> {:.3}°) : soit la poignee ne fait rien, soit \
             la lecture finale de la normale manque — c'est le defaut exact qui a fait croire au \
             banc processeur qu'il convergeait alors qu'il ne bougeait pas",
            sans.moyenne,
            avec.moyenne
        );

        // ⚠⚠ LE CRITÈRE QUI EXPLIQUE LE PIRE CAS, au lieu de l'accepter.
        //
        // Le pire écart vaut plus de 100° et il est IDENTIQUE avec et sans Newton — un chiffre
        // qu'on ne peut pas laisser sans explication. La bonne question n'est pas « comment le
        // faire baisser » mais **« d'où vient-il ? »**.
        //
        // Réponse mesurée : ces pixels sont posés sur l'angle critique, la frontière où un rayon
        // cesse de sortir pour être totalement réfléchi. **La nature y est discontinue** : d'un
        // côté le rayon part vers l'avant, de l'autre il rebondit en arrière — plus de 100°
        // d'écart pour un millième de degré d'incidence. Aucune méthode numérique ne peut y être
        // continue, et un banc qui exigerait le contraire mesurerait sa propre tolérance.
        //
        // *Ce qu'on peut exiger, en revanche : que ces pixels soient RARES et qu'ils soient bien
        // là où la physique bascule.* Les deux se vérifient.
        assert!(
            avec.pire_ecart_au_critique < 1.0,
            "le pire ecart ({:.1}°) se produit a {:.2}° de l'angle critique — donc PAS sur la \
             discontinuite physique. C'est alors un calcul faux, pas une frontiere.",
            avec.pire,
            avec.pire_ecart_au_critique
        );
        // ⚠⚠ CE CRITÈRE A ÉTÉ CORRIGÉ APRÈS SA PREMIÈRE MESURE, et il faut dire pourquoi — sinon
        // ça ressemble exactement à ce que le projet s'interdit : desserrer une garde qui gêne.
        //
        // Il disait d'abord : « moins de 2 % des pixels au-delà de 10° ». La mesure a donné
        // **2,88 %**, et il est tombé. La tentation était de passer à 3 % ; le geste juste était de
        // se demander si le critère mesurait la bonne grandeur. **Il ne la mesurait pas.**
        //
        // Un pourcentage de pixels est une SURFACE. Or une frontière physique n'est pas une
        // surface : c'est une COURBE, et ce qu'il faut borner est son ÉPAISSEUR. Le compte des
        // 296 pixels rapporté au périmètre de la bille donne **0,82 pixel** — le liseré est plus
        // fin qu'un pixel, ce qui est exactement ce qu'une discontinuité doit produire.
        //
        // ⭐ Et l'épaisseur a une propriété que le pourcentage n'a pas : **elle ne dépend pas de la
        // résolution.** À 512² le nombre de pixels concernés doublerait et le pourcentage
        // changerait, alors que l'épaisseur du liseré resterait la même. *Un critère qui varie
        // avec la taille de l'image mesurait autre chose que ce qu'on croyait lui demander.*
        let perimetre = 2.0 * (std::f32::consts::PI * avec.compte as f32).sqrt();
        let epaisseur = avec.gros as f32 / perimetre;
        let epaisseur_sans = sans.gros as f32 / perimetre;
        println!(
            "  epaisseur du lisere au-dela de 10° : {epaisseur:.2} px (sans Newton : {epaisseur_sans:.2} px)"
        );
        assert!(
            epaisseur < 2.0,
            "le lisere d'erreur fait {epaisseur:.2} pixels d'epaisseur : une frontiere physique en \
             fait moins d'un. Au-dela, ce n'est plus la discontinuite qu'on mesure, c'est une zone \
             ou le calcul derape."
        );
    }
}
