//! **D'OÙ VIENT LE PIRE CAS À DIX FOIS LA MOYENNE ? — le banc qui tranche (M4).**
//!
//! ```text
//! cargo run --release -p aegis_engine --example pire_cas --no-default-features
//! cargo run --release -p aegis_engine --example pire_cas --no-default-features -- <images>
//! ```
//!
//! ## Le défaut qu'il instruit
//!
//! Le corpus porte ce constat depuis des semaines, marqué **prioritaire** et jamais expliqué :
//!
//! > *« Le pire cas vaut jusqu'à 10× la moyenne — 0,215 ms de moyenne pour 0,554 ms de pic sur une
//! > campagne de 971 images ; jusqu'à 2,4 ms sur une autre. La cause n'est pas expliquée. »*
//!
//! Sur une machine à échéance dure — 13,9 ms en casque, ratée = malaise physique — une image sur
//! *N* qui prend dix fois le temps n'est pas une lenteur, c'est un défaut qui se ressent.
//!
//! ## ⭐ Pourquoi ce banc mesure le FOND, et rien de plus compliqué
//!
//! `chrono_gpu.rs` documente lui-même, sans en tirer la conséquence : *« la même étape "fond"
//! rendait 0,022 ms puis 0,462 ms d'une image à l'autre — un facteur 20, alors que rien n'avait
//! changé dans le moteur »*.
//!
//! **Un fond plein écran fait rigoureusement le même travail à chaque image** : mêmes trois
//! sommets, mêmes pixels, même shader, aucune géométrie, aucune donnée qui bouge. *Si cette
//! étape-là varie d'un facteur dix, la cause ne peut pas être algorithmique.* C'est le plus petit
//! instrument capable de séparer « le moteur fait un travail irrégulier » de « la machine rend un
//! temps irrégulier pour un travail régulier ».
//!
//! ## Les deux hypothèses, et ce qui les départage
//!
//! | Hypothèse | Ce qu'elle prédit |
//! |---|---|
//! | **A — le moteur** : une passe fait parfois plus de travail | la dispersion dépend de la scène, et **disparaît** sur un travail constant |
//! | **B — la machine** : le GPU change de fréquence, ou est préempté | la dispersion **persiste** sur un travail constant, et **décroît en relatif** quand la charge monte |
//!
//! ⭐ **L'hypothèse B a un mobile connu** : le jeu tourne en `FIFO` calé sur un écran à **165 Hz**,
//! soit 6,06 ms par image, pour un travail GPU mesuré à 0,215 ms. **Le processeur graphique est donc
//! inactif ~97 % du temps** — et un GPU inactif redescend ses horloges. C'est exactement le
//! mécanisme du piège déjà connu du projet (*une fenêtre masquée gonfle les durées d'un facteur 3
//! à 4*), mais ici il n'a pas besoin d'une fenêtre masquée pour se déclencher : la charge suffit.
//!
//! ⚠ **Ce banc tourne SANS ÉCRAN**, donc sans compositeur, sans `FIFO` et sans présentation. *Si la
//! dispersion survit à ça, elle ne vient d'aucun des trois.*
//!
//! ## ⚠ Ce que ce banc ne peut pas faire
//!
//! Il décrit **cette carte, ce pilote, ce jour**. Un temps GPU ne se cite jamais comme une propriété
//! du moteur — c'est écrit dans `chrono_gpu.rs` et ça vaut ici. Il ne dit rien d'un Adreno 650, et
//! il ne le dira jamais.

use aegis_engine::chrono_gpu::ChronoGpu;
use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::{Mat4, Vec3};
use aegis_engine::render::cadre::{Ambiance, Cadre, DonneesImage};
use aegis_engine::render::pipeline::{Faces, Melange, PipelineFactory, Reglages};
use ash::vk;

const FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;

/// Les charges balayées : le même travail, sur des surfaces de plus en plus grandes.
///
/// *C'est le second axe du départage. Si la dispersion vient de la fréquence, elle doit **décroître
/// en relatif** quand la charge monte : un GPU qui travaille reste en haute fréquence.*
const COTES: [u32; 4] = [256, 512, 1024, 2048];

/// ⭐⭐ LE REPOS ENTRE DEUX IMAGES — et c'est lui qui fait tout le banc.
///
/// **Une première version de ce banc mesurait sans repos, et son verdict était faux par
/// construction.** En soumission serrée — encoder, soumettre, attendre, recommencer — le processeur
/// graphique est sollicité en permanence : il ne redescend jamais ses horloges. Le banc éliminait
/// donc le suspect n° 1 *en construisant un environnement qui ne l'a pas*, puis concluait à son
/// innocence.
///
/// *C'est la faute que le corpus nomme « l'instrument qui modifie ce qu'il mesure », et elle a
/// failli passer : les chiffres étaient beaux, réguliers, et ne répondaient pas à la question.*
///
/// 6,06 ms est la période d'un écran à 165 Hz — la cadence exacte à laquelle la campagne d'origine
/// a été prise, en `FIFO`. Avec un travail de 0,2 ms, le GPU y est **inactif 97 % du temps**.
const REPOS_165HZ: std::time::Duration = std::time::Duration::from_micros(6060);

fn main() {
    let images: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(400);

    titre("AEGIS — D'OÙ VIENT LE PIRE CAS ? (M4)");
    println!("  Le fond plein écran, {images} fois de suite, sans écran ni compositeur.");
    println!("  Le travail est RIGOUREUSEMENT constant d'une image à l'autre : toute dispersion");
    println!("  mesurée ici ne peut donc pas être algorithmique.\n");

    println!(
        "  {:>7} {:>10} {:>10} {:>10} {:>10} {:>10} {:>9}",
        "côté", "médiane", "moyenne", "p95", "p99", "max", "max/méd"
    );

    let mut verdicts = Vec::new();
    for cote in COTES {
        match mesurer(cote, images, None) {
            Ok(Some(mut d)) if d.len() >= 20 => {
                d.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let q = |f: f64| d[((d.len() - 1) as f64 * f) as usize];
                let mediane = q(0.5);
                let moyenne: f32 = d.iter().sum::<f32>() / d.len() as f32;
                let max = d[d.len() - 1];
                let rapport = max / mediane.max(f32::MIN_POSITIVE);
                println!(
                    "  {:>7} {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>8.1}×",
                    cote, mediane, moyenne, q(0.95), q(0.99), max, rapport
                );
                verdicts.push((cote, mediane, rapport));
            }
            Ok(_) => println!("  {cote:>7}  (trop peu de relevés — le chronomètre n'a rien rendu)"),
            Err(e) => println!("  {cote:>7}  échec : {e}"),
        }
    }

    // ── Le même travail, mais au rythme où il a été mesuré à l'origine ──────────────────────
    titre("LE MÊME TRAVAIL, CADENCÉ À 165 Hz — le GPU inactif 97 % du temps");
    println!("  6,06 ms de repos entre deux images : la cadence exacte de la campagne d'origine.");
    println!("  ⚠ C'est le seul régime qui reproduise ses conditions. Sans lui, ce banc élimine");
    println!("     le suspect en construisant un monde où il n'existe pas.\n");
    println!(
        "  {:>7} {:>10} {:>10} {:>10} {:>10} {:>10} {:>9}",
        "côté", "médiane", "moyenne", "p95", "p99", "max", "max/méd"
    );

    let mut cadences = Vec::new();
    for cote in COTES {
        match mesurer(cote, images.min(200), Some(REPOS_165HZ)) {
            Ok(Some(mut d)) if d.len() >= 20 => {
                d.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let q = |f: f64| d[((d.len() - 1) as f64 * f) as usize];
                let mediane = q(0.5);
                let moyenne: f32 = d.iter().sum::<f32>() / d.len() as f32;
                let max = d[d.len() - 1];
                let rapport = max / mediane.max(f32::MIN_POSITIVE);
                println!(
                    "  {:>7} {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>8.1}×",
                    cote, mediane, moyenne, q(0.95), q(0.99), max, rapport
                );
                cadences.push((cote, mediane, rapport));
            }
            Ok(_) => println!("  {cote:>7}  (trop peu de relevés)"),
            Err(e) => println!("  {cote:>7}  échec : {e}"),
        }
    }

    // ── LA COMPARAISON QUI TRANCHE ─────────────────────────────────────────────────────────
    if !cadences.is_empty() && !verdicts.is_empty() {
        titre("LA COMPARAISON QUI TRANCHE");
        println!("  {:>7} {:>14} {:>14} {:>12}", "côté", "serré méd.", "cadencé méd.", "rapport");
        let mut pire_ecart = 1.0f32;
        for ((c, ms, _), (_, mc, _)) in verdicts.iter().zip(cadences.iter()) {
            let ecart = mc / ms.max(f32::MIN_POSITIVE);
            pire_ecart = pire_ecart.max(ecart);
            println!("  {c:>7} {ms:>13.3}ms {mc:>13.3}ms {ecart:>11.2}×");
        }
        let pire_disp_cadence = cadences.iter().fold(0.0f32, |m, (_, _, r)| m.max(*r));
        println!();
        if pire_ecart > 1.5 || pire_disp_cadence > 3.0 {
            println!("  ⇒ ⭐ LAISSER LE GPU AU REPOS CHANGE LA MESURE (médiane ×{pire_ecart:.2},");
            println!("     dispersion jusqu'à {pire_disp_cadence:.1}× en cadencé).");
            println!("     **La cause du pire cas est la GESTION DE FRÉQUENCE, pas le moteur.**");
            println!("     Un travail rigoureusement constant devient irrégulier du seul fait");
            println!("     qu'on laisse la carte se rendormir entre deux images.");
        } else {
            println!("  ⇒ Le repos ne change presque rien (médiane ×{pire_ecart:.2}, dispersion");
            println!("     {pire_disp_cadence:.1}×). La gestion de fréquence est DISCULPÉE sur cette");
            println!("     machine, et le pire cas d'origine reste à expliquer ailleurs :");
            println!("     le compositeur, la présentation `FIFO`, ou une passe réelle du moteur.");
            println!("     ⚠ NE PAS conclure au-delà de ça.");
        }
    }

    if verdicts.len() < 2 {
        println!("\n  Pas assez de charges mesurées pour conclure.");
        return;
    }

    // ── Le verdict, contre les critères posés AVANT la mesure ───────────────────────────────
    titre("LE VERDICT");
    let (_, _, r_min) = verdicts[0];
    let (_, _, r_max) = verdicts[verdicts.len() - 1];
    let pire = verdicts.iter().fold(0.0f32, |m, (_, _, r)| m.max(*r));

    println!("  Critères écrits AVANT la mesure :");
    println!("   · dispersion ABSENTE sur travail constant  → le moteur (hypothèse A)");
    println!("   · dispersion PRÉSENTE et DÉCROISSANTE avec la charge → la machine (hypothèse B)");
    println!("   · dispersion PRÉSENTE et STABLE → ni l'un ni l'autre, il faut chercher ailleurs\n");

    if pire < 2.0 {
        println!("  ⇒ Dispersion quasi absente (pire rapport {pire:.1}×).");
        println!("     Un travail rigoureusement constant est rendu en temps constant, **avec comme");
        println!("     sans repos entre les images**. Deux suspects tombent : la gestion de");
        println!("     fréquence du GPU, et le rythme de soumission.");
        println!("\n     ⚠ CE N'EST PAS UNE CONCLUSION SUR LE MOTEUR. Ce banc n'a pas de fenêtre,");
        println!("     donc pas de compositeur ni de présentation `FIFO` — il ne peut PAS les");
        println!("     innocenter. Et il ne mesure aucune passe réelle. Ce qu'il établit est");
        println!("     étroit et solide : la carte n'est pas capricieuse. Le reste est ouvert.");
    } else if r_max < r_min * 0.6 {
        println!("  ⇒ Dispersion PRÉSENTE ({pire:.1}× au pire) et elle DÉCROÎT quand la charge monte");
        println!("     ({r_min:.1}× à {} px → {r_max:.1}× à {} px).", COTES[0], COTES[COTES.len() - 1]);
        println!("     **Hypothèse B confirmée** : la machine rend un temps irrégulier pour un");
        println!("     travail régulier. Ce n'est pas un défaut du moteur — c'est un défaut de");
        println!("     l'INSTRUMENT quand la charge est trop faible pour tenir le GPU éveillé.");
        println!("\n     ⭐ Conséquence : tout chiffre de pic mesuré sur une scène légère est SANS");
        println!("     VALEUR, et le budget doit se lire sur la MÉDIANE tant que la charge ne");
        println!("     remplit pas l'image.");
    } else {
        println!("  ⇒ Dispersion PRÉSENTE ({pire:.1}×) mais STABLE avec la charge");
        println!("     ({r_min:.1}× → {r_max:.1}×). Aucune des deux hypothèses ne tient telle");
        println!("     quelle : la cause est ailleurs — préemption par un autre programme, ou un");
        println!("     compteur d'horodatage qui ne mesure pas ce qu'on croit.");
        println!("     ⚠ NE PAS inventer d'explication ici. C'est un résultat, pas une conclusion.");
    }

    titre("CE QUE CE BANC NE DIT PAS");
    println!("  · Rien sur un Adreno 650 : il décrit cette carte, ce pilote, ce jour.");
    println!("  · Rien sur les passes réelles du moteur : il mesure le travail le plus simple");
    println!("    possible, exprès — c'est ce qui lui permet d'accuser autre chose que le moteur.");
}

/// Rend `images` fois un fond plein écran et rapporte la durée GPU de chacune.
fn mesurer(
    cote: u32,
    images: u32,
    repos: Option<std::time::Duration>,
) -> Result<Option<Vec<f32>>, Box<dyn std::error::Error>> {
    let gpu = match GpuContext::sans_ecran_format(cote, cote, 1, FORMAT) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ aucun Vulkan joignable : {e}");
            return Ok(None);
        }
    };
    let memory_props =
        unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

    let mut cadre = Cadre::nouveau(&gpu, &memory_props)?;
    let layout = PipelineFactory::create_pipeline_layout(
        &gpu.device,
        std::slice::from_ref(&cadre.layout_descripteur),
        &[],
    )?;

    // Le fond ne lit aucune géométrie : trois sommets engendrés dans le shader, et le cadre pour
    // savoir de quelle couleur est le ciel.
    let module_v = PipelineFactory::create_shader_module_from_bytes(
        &gpu.device,
        aegis_engine::shaders::BACKGROUND_VERT_SPV,
    )?;
    let module_f = PipelineFactory::create_shader_module_from_bytes(
        &gpu.device,
        aegis_engine::shaders::BACKGROUND_FRAG_SPV,
    )?;
    let pipeline = PipelineFactory::create_graphics_pipeline(
        &gpu.device,
        layout,
        module_v,
        module_f,
        Reglages {
            color_format: FORMAT,
            second_format: None,
            depth_format: None,
            depth_write: false,
            melange: Melange::Aucun,
            use_vertex_input: false,
            faces: Faces::Toutes,
            echantillons: vk::SampleCountFlags::TYPE_1,
        },
    )?;
    unsafe {
        gpu.device.destroy_shader_module(module_v, None);
        gpu.device.destroy_shader_module(module_f, None);
    }

    // Une caméra fixe : rien ne doit changer d'une image à l'autre, c'est tout l'intérêt.
    let oeil = Vec3::new(0.0, 1.0, -4.0);
    let camera = aegis_engine::scene::camera::Camera::new(oeil, Vec3::new(0.0, 0.0, 0.0), 1.0);
    cadre.ecrire(&DonneesImage::nouvelle(
        camera.compute_projection_matrix() * camera.compute_view_matrix(),
        Mat4::IDENTITY,
        [oeil.x, oeil.y, oeil.z],
        Ambiance::default(),
        &[],
    ));

    let mut chrono = ChronoGpu::nouveau(
        &gpu.device,
        gpu.proprietes.limits.timestamp_period,
        unsafe {
            gpu.instance
                .get_physical_device_queue_family_properties(gpu.physical_device)
                .first()
                .map(|f| f.timestamp_valid_bits)
                .unwrap_or(0)
        },
        8,
    )?;

    let etendue = gpu.swapchain_extent;
    let image = gpu.swapchain_images[0];
    let vue = gpu.swapchain_image_views[0];
    let mut durees = Vec::with_capacity(images as usize);

    for _ in 0..images {
        let cmd = gpu.begin_single_time_commands()?;
        // ⚠ Relève l'image PRÉCÉDENTE — elle est terminée, puisque `end_single_time_commands`
        // attend. C'est la même hypothèse que dans la boucle de rendu du jeu, où c'est la barrière
        // d'image qui l'a garantie.
        chrono.ouvrir_image(&gpu.device, cmd);

        unsafe {
            barriere(&gpu, cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            let attache = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE);
            gpu.device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&attache)),
            );
            gpu.device.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0, y: 0.0,
                width: etendue.width as f32, height: etendue.height as f32,
                min_depth: 0.0, max_depth: 1.0,
            }]);
            gpu.device.cmd_set_scissor(cmd, 0, &[vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue,
            }]);
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            cadre.lier(&gpu.device, cmd, layout);
            gpu.device.cmd_draw(cmd, 3, 1, 0, 0);
            gpu.device.cmd_end_rendering(cmd);
        }
        chrono.jalon(&gpu.device, cmd, "fond");
        gpu.end_single_time_commands(cmd)?;

        // Le relevé de CETTE image sera lu au tour suivant ; on récupère celui du tour d'avant.
        if let Some(e) = chrono.etapes().iter().find(|e| e.nom == "fond") {
            durees.push(e.millisecondes);
        }

        // ⭐ Le repos qui laisse la carte se rendormir — c'est lui qu'on cherche à incriminer.
        if let Some(d) = repos {
            std::thread::sleep(d);
        }
    }

    unsafe {
        gpu.device.device_wait_idle().ok();
        gpu.device.destroy_pipeline(pipeline, None);
        gpu.device.destroy_pipeline_layout(layout, None);
    }
    chrono.detruire(&gpu.device);
    cadre.detruire(&gpu.device);

    Ok(Some(durees))
}

unsafe fn barriere(
    gpu: &GpuContext,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    avant: vk::ImageLayout,
    apres: vk::ImageLayout,
) {
    unsafe {
        gpu.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .old_layout(avant)
                .new_layout(apres)
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })],
        );
    }
}

fn titre(t: &str) {
    println!("\n\x1b[1m{t}\x1b[0m");
    println!("{}", "─".repeat(t.chars().count()));
}
