//! **ÉCLAIRER UNE VRAIE SCÈNE 3D — la question que tout le corpus posait sans pouvoir y répondre.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example eclairer --no-default-features
//! cargo run --release -p aegis_engine --example eclairer --no-default-features -- <fichier.glb> [sortie.png]
//! ```
//!
//! ## Pourquoi cet outil existe (5 septembre 2026)
//!
//! Le moteur porte **onze shaders et une chaîne de rendu complète** — lumière directe, ombres,
//! halo, occlusion ambiante, MSAA, chaîne HDR et courbe de tonalité. **Aucun n'avait jamais vu
//! autre chose qu'une scène 2.5D voxel générée en code.** La question était écrite en dernière
//! ligne de deux documents du corpus, recommandée deux fois, et jamais tranchée :
//!
//! > *« Tout a été écrit pour une scène 2.5D voxel — qu'est-ce qui tient sur une vraie scène 3D ? »*
//!
//! Elle ne pouvait pas l'être : il manquait le moyen de charger une scène. `GlbLoader::charger_scene`
//! l'a ouvert le même jour.
//!
//! ## ⭐ Ce que cet outil a révélé avant même de rendre une image
//!
//! **`party_2d5.wgsl` — le shader d'éclairage — ne contient aucune trace de voxel.** Pas un cube,
//! pas une grille, pas un bloc. Tout le spécifique au jeu vit dans les 1 907 lignes de
//! `party_render_pass.rs`, **côté jeu**. *La frontière tenait, et personne ne l'avait vérifiée sur
//! autre chose que la parole du code.*
//!
//! ## Ce que cet outil est, et ce qu'il n'est pas
//!
//! Il **orchestre les briques du moteur** — `Cibles`, `Ombre`, `File`, `Cadre`, `Ecran` — sur une
//! scène quelconque, sans rien emprunter au jeu. C'est le plus petit assemblage qui produise une
//! image éclairée honnête, courbe de tonalité comprise.
//!
//! ⚠ **Il ne remplace pas le rendu du jeu**, et il ne le doit pas : pas de particules, pas de
//! halo, pas d'interface, pas d'occlusion ambiante. *Ajouter ces passes ici en ferait un second
//! moteur — celui que personne ne teste.* Ce qui manque est nommé plus bas, à mesure que ça se
//! branchera.
//!
//! ⚠ **Et il ne juge rien.** Il écrit une image ; le juge du rendu perçu est un œil humain.

use aegis_engine::core::gpu_context::GpuContext;
use aegis_engine::core::math::{Mat4, Vec3, Vec4};
use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use aegis_engine::geometry::gpu_mesh::GpuMesh;
use aegis_engine::render::cadre::{Ambiance, Cadre, DonneesImage};
use aegis_engine::render::cibles::Cibles;
use aegis_engine::render::ecran::Ecran;
use aegis_engine::render::file::{Dessin, File};
use aegis_engine::render::instances::Instances;
use aegis_engine::render::ombre::{matrice_lumiere, Ombre};
use aegis_engine::render::pipeline::{Faces, Melange, PipelineFactory, Reglages};
use aegis_engine::scene::light::GpuLight;
use ash::vk;
use std::path::PathBuf;

/// Le modèle par défaut : la scène de test qu'il a préparée.
const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";
/// La cible finale est une image d'écran ordinaire : c'est là que la courbe de tonalité aboutit.
const FORMAT_ECRAN: vk::Format = vk::Format::B8G8R8A8_UNORM;

fn main() {
    let mut args = std::env::args().skip(1);
    let chemin = match args.next() {
        Some(a) => PathBuf::from(a),
        None => racine_du_depot().join(MODELE_PAR_DEFAUT),
    };
    let sortie = args.next().unwrap_or_else(|| "target/preuves/scene-eclairee.png".to_string());

    titre("AEGIS — UNE VRAIE SCÈNE 3D, ÉCLAIRÉE");
    println!("  Fichier : {}", chemin.display());

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };
    println!(
        "  Scène   : {} parties, {} sommets, {} triangles",
        scene.parties.len(),
        scene.sommets.len(),
        scene.indices.len() / 3
    );
    for p in &scene.parties {
        println!("            · {} ({} triangles)", p.nom, p.nombre_indices / 3);
    }

    match rendre(&scene, 900, None, None) {
        Ok(Some(Rendu { octets, largeur, hauteur })) => {
            let png = match aegis_engine::image::png::encoder(largeur, hauteur, &octets) {
                Ok(p) => p,
                Err(e) => {
                    println!("  Encodage impossible : {e}");
                    return;
                }
            };
            if let Err(e) = std::fs::write(&sortie, &png) {
                println!("  Écriture impossible : {e}");
                return;
            }
            titre("L'IMAGE");
            println!("  {sortie} — {largeur}×{hauteur}, {} Ko", png.len() / 1024);
            let vivants = octets.chunks_exact(3).filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
            println!(
                "  {vivants} pixels non noirs sur {} ({:.1} %)",
                octets.len() / 3,
                100.0 * vivants as f64 / (octets.len() / 3) as f64
            );
            println!("\n  ⚠ Ce programme n'a pas jugé cette image, et il ne le peut pas.");
            println!("     Ce qui n'y est PAS, et qu'il ne faut pas croire absent du moteur :");
            println!("     l'occlusion ambiante, le halo, les particules, l'interface.");
        }
        Ok(None) => println!("\n  Aucun Vulkan joignable — rien n'a été rendu, et rien n'est conclu."),
        Err(e) => println!("\n  Le rendu a échoué : {e}"),
    }
}

/// Ce qu'un rendu rapporte : les octets RVB de l'image finale, et ses dimensions.
struct Rendu {
    octets: Vec<u8>,
    largeur: u32,
    hauteur: u32,
}

/// Rend la scène éclairée, et rapporte l'image finale en RVB 8 bits.
///
/// `Ok(None)` si aucun Vulkan n'est joignable : sur une machine sans carte, cet outil s'abstient au
/// lieu d'échouer — *il ne mesure alors rien, et il le dit.*
/// `cadrage` impose un centre et un rayon au lieu de les déduire de la scène.
///
/// ⚠ **Indispensable pour comparer deux rendus.** Cadrée sur elle-même, toute scène remplit
/// l'image : une table seule et une table garnie n'occuperaient alors pas les mêmes pixels, et la
/// comparaison ne mesurerait que le cadrage.
fn rendre(
    scene: &Scene,
    cote: u32,
    cadrage: Option<(Vec3, f32)>,
    vers_le_soleil: Option<Vec3>,
) -> Result<Option<Rendu>, Box<dyn std::error::Error>> {
    let gpu = match GpuContext::sans_ecran_format(cote, cote, 1, FORMAT_ECRAN) {
        Ok(c) => c,
        Err(e) => {
            println!("  ⚠ {e}");
            return Ok(None);
        }
    };
    let memory_props =
        unsafe { gpu.instance.get_physical_device_memory_properties(gpu.physical_device) };

    // ── Les cibles, le cadre, la carte d'ombre ──────────────────────────────────────────────
    let cibles = Cibles::nouvelles(&gpu, &memory_props)?;
    let mut cadre = Cadre::nouveau(&gpu, &memory_props)?;

    let layout = PipelineFactory::create_pipeline_layout(
        &gpu.device,
        std::slice::from_ref(&cadre.layout_descripteur),
        &[],
    )?;

    // 2048 pèse 8 Mo en 32 bits : le compromis que `ombre.rs` documente pour une scène de cette
    // taille. Un téléphone prendrait 1024.
    let mut ombre = Ombre::nouvelle(&gpu, &memory_props, layout, 2048)?;
    cadre.brancher_la_carte_d_ombre(&gpu.device, ombre.vue, ombre.echantillonneur);

    let ecran = Ecran::nouveau(&gpu, &memory_props, &cibles, cadre.layout_descripteur)?;
    ecran.brancher(&gpu.device, &cibles);

    // ── Le maillage, et de quoi le dessiner ─────────────────────────────────────────────────
    let maillage = GpuMesh::upload(&gpu, &memory_props, &scene.sommets, &scene.indices)?;
    // ⚠⚠ DEUX, ET PAS UN — le défaut qui a coûté la première image de ce programme.
    //
    // Le tampon d'instances est **partagé par toutes les passes d'une image**, et il n'est remis à
    // zéro qu'au `recommencer()` suivant. La carte d'ombre y dépose l'objet, puis la passe
    // d'éclairage veut y déposer le sien : avec une capacité de 1, `poser` rend `None`,
    // `dessiner_un` **ne dessine rien**, et aucune erreur n'est levée.
    //
    // *Le moteur a bien un compteur pour ça — `perdues` — mais il n'est journalisé qu'au
    // `recommencer()` d'après. Dans un rendu à IMAGE UNIQUE, ce message n'arrive jamais.* C'est
    // exactement le genre de garde qui protège le jeu et laisse un banc dans le noir.
    let passes_qui_deposent = 2; // la carte d'ombre, puis l'éclairage
    let instances = Instances::nouveau(&gpu, &memory_props, passes_qui_deposent)?;

    // ⚠ Le modèle est l'IDENTITÉ : `charger_scene` a déjà replacé chaque objet dans le monde.
    // *Ré-appliquer une transformation ici la compterait deux fois.*
    let mut file = File::nouvelle();
    file.ajouter(Dessin {
        maillage: 0,
        modele: Mat4::IDENTITY,
        // La teinte est BLANCHE, et c'est une position, pas un défaut : le moteur fournit ce qui
        // est VRAI, un artiste fournirait ce qui est BEAU. Une couleur choisie ici serait une
        // décision d'apparence prise par le moteur.
        teinte: Vec4::new(1.0, 1.0, 1.0, 1.0),
        params: Vec4::new(0.0, 0.0, 0.0, 0.0),
        porte_une_ombre: true,
    });

    // ── Le cadrage, calculé depuis la scène ─────────────────────────────────────────────────
    let (centre, rayon) = cadrage.unwrap_or_else(|| boite_englobante(scene));
    const MARGE: f32 = 1.3;
    let fov = 55_f32.to_radians();
    let recul = rayon * MARGE / (fov * 0.5).tan();
    let direction = Vec3::new(0.55, 0.40, -0.73).normalize();
    let oeil = centre + direction * recul;

    let mut camera = aegis_engine::scene::camera::Camera::new(oeil, centre, 1.0);
    camera.fov_y_radians = fov;
    // Les plans de coupe suivent la scène : un `z_near` trop petit devant une scène large ruine la
    // précision de profondeur, et la géométrie se met à clignoter par bandes.
    camera.z_near = (recul - rayon * MARGE).max(rayon * 0.01);
    camera.z_far = recul + rayon * 2.0 * MARGE;
    let view_proj = camera.compute_projection_matrix() * camera.compute_view_matrix();

    // ── La lumière ──────────────────────────────────────────────────────────────────────────
    //
    // Un seul soleil, et la carte d'ombre le suit : la matrice de lumière doit couvrir la scène,
    // sinon ce qui en sort ne projette rien — compromis de toute carte d'ombre unique.
    let ambiance = Ambiance::default();
    let vers_le_soleil = vers_le_soleil.unwrap_or(Vec3::new(0.62, 0.58, 0.33)).normalize();
    let soleil = GpuLight::new_directional(
        // ⚠⚠ Le vecteur pointe **VERS** la lumière, jamais dans le sens où elle voyage.
        //
        // *La première version de ce programme passait l'opposé, avec un commentaire affirmant que
        // c'était la bonne convention. L'image restait plausible — une table éclairée — mais son
        // plateau était plus sombre que ses pieds sous un soleil au zénith. La convention est
        // écrite dans `party_render_pass.rs`, et je l'avais supposée au lieu de la lire.*
        vers_le_soleil,
        Vec3::new(1.0, 0.96, 0.88),
        // ⭐ Elle vient de l'ambiance, et ce n'est pas un détail de câblage : c'est ce qui rend le
        // RAPPORT direct/ambiant réglable d'un seul endroit. La valeur par défaut du moteur n'est
        // pas tâtonnée — le diffus vaut albédo × (1−F) × I × N·L / π, donc pour qu'une face en
        // plein soleil rende ~0,75 avant l'ambiante il faut I ≈ 0,75 × π / 0,96.
        ambiance.intensite_soleil,
    );
    let lumiere = matrice_lumiere(vers_le_soleil, centre, rayon * MARGE);

    cadre.ecrire(&DonneesImage::nouvelle(
        view_proj,
        lumiere,
        [oeil.x, oeil.y, oeil.z],
        ambiance,
        &[soleil],
    ));

    // ── Le pipeline d'éclairage — le shader du jeu, sans une ligne du jeu ───────────────────
    let module_v = PipelineFactory::create_shader_module_from_bytes(
        &gpu.device,
        aegis_engine::shaders::PARTY_2D5_VERT_SPV,
    )?;
    let module_f = PipelineFactory::create_shader_module_from_bytes(
        &gpu.device,
        aegis_engine::shaders::PARTY_2D5_FRAG_SPV,
    )?;
    let pipeline = PipelineFactory::create_graphics_pipeline(
        &gpu.device,
        layout,
        module_v,
        module_f,
        Reglages {
            color_format: cibles.format_hdr,
            // ⚠ Le shader déclare `@location(1)` pour l'ambiante ; sans ce format la carte refuse
            // le pipeline. Les deux se décident ensemble.
            second_format: Some(cibles.format_hdr),
            depth_format: Some(cibles.format_profondeur),
            depth_write: true,
            melange: Melange::Aucun,
            use_vertex_input: true,
            faces: Faces::Toutes,
            echantillons: cibles.echantillons,
        },
    )?;
    unsafe {
        gpu.device.destroy_shader_module(module_v, None);
        gpu.device.destroy_shader_module(module_f, None);
    }

    // ── Le rendu ────────────────────────────────────────────────────────────────────────────
    let etendue = gpu.swapchain_extent;
    let msaa = cibles.echantillons != vk::SampleCountFlags::TYPE_1;
    let cmd = gpu.begin_single_time_commands()?;
    instances.recommencer();

    unsafe {
        // 1. La carte d'ombre, hors de toute passe et avant celle qui la lira.
        //
        // ⚠ LE CADRE SE LIE AVANT, parce qu'une passe qui LIT un descripteur doit l'avoir lié.
        //
        // `Ombre::dessiner` reçoit un `_layout` qu'elle n'utilise pas — son propre commentaire dit
        // qu'il servirait « si cette passe en avait besoin ». Or **elle en a besoin** : son shader
        // lit `cadre.light_view_proj` pour placer le monde du point de vue de la lumière. Elle
        // compte donc sur un descripteur lié par l'appelant.
        //
        // ⚠⚠ **CE QUE JE NE PEUX PAS AFFIRMER, ET QUE J'AI POURTANT ÉCRIT UNE FOIS :** que son
        // absence explique les ombres manquantes du premier essai. Elle les a fait apparaître, oui
        // — mais les retirer ensuite ne les fait pas disparaître. *Lire un descripteur non lié est
        // un comportement INDÉFINI : il peut rendre les bonnes données par accident, et changer
        // d'avis d'une exécution à l'autre.* La ligne reste parce qu'elle est juste, pas parce que
        // sa nécessité est démontrée. **La vraie cause du premier essai n'est pas établie.**
        cadre.lier(&gpu.device, cmd, layout);
        ombre.dessiner(&gpu.device, cmd, layout, &file, &instances, &[&maillage]);

        // 2. Les cibles passent en attachement.
        for image in [cibles.image_resolue(), cibles.image_ambiante_resolue()] {
            barriere_couleur(&gpu, cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }
        if let (Some(c), Some(a)) = (cibles.image_couleur(), cibles.image_ambiante()) {
            for image in [c, a] {
                barriere_couleur(&gpu, cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            }
        }

        // Deux attachements couleur : la lumière totale, et l'ambiante SEULE. *C'est ce qui rend
        // l'occlusion exacte le jour où elle se branchera — elle ne doit assombrir que l'ambiante.*
        let attache = |vue: vk::ImageView, resolue: vk::ImageView| {
            let mut a = vk::RenderingAttachmentInfo::default()
                .image_view(vue)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } });
            if msaa {
                a = a
                    .resolve_image_view(resolue)
                    .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .resolve_mode(vk::ResolveModeFlags::AVERAGE);
            }
            a
        };
        let couleurs = [
            attache(cibles.vue_couleur(), cibles.vue_resolue()),
            attache(cibles.vue_ambiante(), cibles.vue_ambiante_resolue()),
        ];
        let profondeur = vk::RenderingAttachmentInfo::default()
            .image_view(cibles.vue_profondeur())
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
            });

        gpu.device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                .layer_count(1)
                .color_attachments(&couleurs)
                .depth_attachment(&profondeur),
        );
        regler_vue(&gpu, cmd, etendue);
        gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
        cadre.lier(&gpu.device, cmd, layout);
        instances.lier(&gpu.device, cmd);
        instances.dessiner_un(
            &gpu.device,
            cmd,
            &maillage,
            Mat4::IDENTITY,
            Vec4::new(1.0, 1.0, 1.0, 1.0),
            Vec4::new(0.0, 0.0, 0.0, 0.0),
        );
        gpu.device.cmd_end_rendering(cmd);

        // 3. Les cibles deviennent des TEXTURES : c'est cette transition qui les rend lisibles
        //    par la composition.
        for image in [cibles.image_resolue(), cibles.image_ambiante_resolue()] {
            barriere_couleur(
                &gpu, cmd, image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }

        // 4. La courbe de tonalité — **la seule passe qui courbe**, et c'est tout ce qu'elle fait.
        let ecran_image = gpu.swapchain_images[0];
        barriere_couleur(&gpu, cmd, ecran_image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let attache_ecran = vk::RenderingAttachmentInfo::default()
            .image_view(gpu.swapchain_image_views[0])
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE);
        gpu.device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&attache_ecran)),
        );
        regler_vue(&gpu, cmd, etendue);
        // ⚠ Le cadre porte l'exposition et le point blanc : sans lui, la composition n'aurait
        // aucune courbe.
        cadre.lier(&gpu.device, cmd, ecran.layout_pipeline());
        ecran.composer(&gpu.device, cmd);
        gpu.device.cmd_end_rendering(cmd);
    }
    gpu.end_single_time_commands(cmd)?;

    // ⭐ LE DIAGNOSTIC QUI BISSECTE, et il vaut d'être permanent.
    //
    // Une image finale noire a deux causes possibles et rigoureusement indiscernables depuis la
    // sortie : *rien n'a été dessiné*, ou *la composition n'a rien lu*. Compter ce que porte la
    // cible HDR **avant** la courbe de tonalité tranche entre les deux en une exécution — sans
    // quoi on modifie au hasard une chaîne à deux étages.
    let hdr = gpu.relire_image_brute(
        cibles.image_resolue(),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        etendue,
        cibles.format_hdr,
    )?;
    let pixels_hdr = hdr.chunks_exact(4).filter(|p| p.iter().any(|o| *o != 0)).count();
    println!(
        "  Avant la courbe : {pixels_hdr} pixels non nuls dans la cible HDR sur {}",
        etendue.width as usize * etendue.height as usize
    );

    // ⭐ La garde que le moteur ne peut pas donner à temps : elle dit MAINTENANT ce que `perdues`
    // n'aurait dit qu'à l'image suivante — laquelle n'existe pas ici.
    if instances.deposees() < passes_qui_deposent {
        println!(
            "  ⚠⚠ {} instances déposées pour {passes_qui_deposent} passes : le tampon a débordé, \
             et une passe n'a RIEN dessiné sans le dire.",
            instances.deposees()
        );
    }

    let octets = gpu.relire_image(
        gpu.swapchain_images[0],
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        etendue,
        FORMAT_ECRAN,
    )?;

    // ── Le ménage ───────────────────────────────────────────────────────────────────────────
    unsafe {
        gpu.device.device_wait_idle().ok();
        gpu.device.destroy_pipeline(pipeline, None);
        gpu.device.destroy_pipeline_layout(layout, None);
    }
    ecran.detruire(&gpu.device);
    ombre.detruire(&gpu.device);
    cibles.detruire(&gpu.device);
    instances.detruire(&gpu.device);
    // ⚠ `GpuMesh` n'a **ni `detruire` ni `Drop`** : son tampon de sommets et son tampon d'indices
    // ne sont jamais rendus à la carte. Sans conséquence ici — ce programme rend une image et sort
    // — mais c'est une vraie fuite pour qui chargerait des maillages en boucle. *Constaté le
    // 5 septembre 2026, non corrigé : ce serait toucher au moteur pour un sujet qui n'est pas
    // celui-ci, et le corriger en passant l'enterrerait au lieu de l'inscrire.*
    cadre.detruire(&gpu.device);

    Ok(Some(Rendu { octets, largeur: etendue.width, hauteur: etendue.height }))
}

/// Le centre et le rayon de la sphère qui contient toute la scène.
fn boite_englobante(scene: &Scene) -> (Vec3, f32) {
    let mut mini = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut maxi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for s in &scene.sommets {
        let p = Vec3::from(s.position);
        mini = mini.min(p);
        maxi = maxi.max(p);
    }
    ((mini + maxi) * 0.5, ((maxi - mini) * 0.5).length().max(1e-3))
}

/// ⚠ La fenêtre et les ciseaux sont DYNAMIQUES dans ce moteur : un pipeline créé sans eux dessine
/// dans une zone vide, et la carte ne s'en plaint pas.
unsafe fn regler_vue(gpu: &GpuContext, cmd: vk::CommandBuffer, etendue: vk::Extent2D) {
    unsafe {
        gpu.device.cmd_set_viewport(
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
        gpu.device.cmd_set_scissor(
            cmd,
            0,
            &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: etendue }],
        );
    }
}

unsafe fn barriere_couleur(
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

fn racine_du_depot() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !p.join("assets").is_dir() {
        if !p.pop() {
            return PathBuf::from(".");
        }
    }
    p
}

/// Une scène réduite aux parties dont le nom **n'est pas** dans `sauf`.
///
/// Sert à comparer « la table seule » et « la table garnie » : les objets posés dessus
/// disparaissent, la table ne bouge pas d'un pixel.
#[cfg(test)]
fn scene_sans(scene: &Scene, sauf: &[&str]) -> Scene {
    let mut sortie = Scene::default();
    for p in &scene.parties {
        if sauf.contains(&p.nom.as_str()) {
            continue;
        }
        let s0 = p.premier_sommet as usize;
        let decalage = sortie.sommets.len() as u32;
        sortie
            .sommets
            .extend_from_slice(&scene.sommets[s0..s0 + p.nombre_sommets as usize]);
        let i0 = p.premier_indice as usize;
        let premier = sortie.indices.len() as u32;
        for i in &scene.indices[i0..i0 + p.nombre_indices as usize] {
            sortie.indices.push(i - p.premier_sommet + decalage);
        }
        sortie.parties.push(aegis_engine::geometry::glb_loader::Partie {
            nom: p.nom.clone(),
            premier_indice: premier,
            nombre_indices: p.nombre_indices,
            premier_sommet: decalage,
            nombre_sommets: p.nombre_sommets,
        });
    }
    sortie
}

// ─────────────────────────────────────────────────────────────────────────────
// Les gardes — chacune tient un défaut réel, trouvé le 5 septembre 2026
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[u8] = include_bytes!("../../../assets/modeles/table de teste verre.glb");

    fn scene_de_test() -> Scene {
        GlbLoader::charger_scene_bytes(TABLE).expect("la table de test")
    }

    /// La luminosité perçue d'un pixel RVB — la pondération standard, pas une moyenne plate.
    fn luminance(p: &[u8]) -> i32 {
        (p[0] as i32 * 54 + p[1] as i32 * 183 + p[2] as i32 * 19) >> 8
    }

    /// ⭐⭐ LA GARDE DE L'OMBRE PORTÉE, et sa première version était CREUSE.
    ///
    /// Une scène peut sortir **sans aucune ombre** — pas une ombre fausse, pas une erreur : rien.
    /// C'est arrivé au premier essai de ce programme, et c'est le genre de manque qu'aucun test de
    /// compilation n'attrape. *Cette garde a été éprouvée en supprimant `Ombre::dessiner` : elle
    /// rend alors exactement 0 pixel déplacé, et elle échoue.*
    ///
    /// ## ⚠⚠ Pourquoi la première version de ce test ne valait rien
    ///
    /// Elle comparait la table garnie à la table nue et comptait les pixels **assombris**. Or un
    /// pixel où la bouteille *recouvre* le plateau s'assombrit aussi, sans qu'aucune ombre n'existe.
    /// **Elle mesurait la présence des objets, pas leur ombre** — et elle est passée avec le défaut
    /// réintroduit exprès. *Une garde qui n'a jamais dit non n'a pas été testée.*
    ///
    /// ## Ce que celle-ci mesure, et pourquoi elle ne peut pas se tromper de la même façon
    ///
    /// **Les pixels qu'un objet recouvre ne dépendent pas de la position du soleil ; son ombre, si.**
    /// On rend donc quatre images — la table garnie et la table nue, sous deux soleils opposés — et
    /// on compare les deux ensembles de pixels assombris. Sans ombre portée, ces ensembles sont
    /// **rigoureusement identiques** : ce sont les mêmes pixels recouverts, sous un éclairage
    /// différent. Avec ombre, ils diffèrent, parce que l'ombre a changé de côté.
    ///
    /// *C'est une comparaison d'ensembles, pas un seuil de qualité : aucune constante nouvelle.*
    #[test]
    fn les_objets_poses_sur_la_table_y_projettent_une_ombre() {
        let complete = scene_de_test();
        let nue = scene_sans(&complete, &["Circle", "Circle.001"]);
        assert_eq!(nue.parties.len(), 1, "il ne doit rester que le plateau et ses pieds");

        // ⚠ LE MÊME cadrage partout : sinon on comparerait des points de vue, pas des ombres.
        let cadrage = Some(boite_englobante(&complete));
        // Deux soleils rasants et opposés en X : les ombres tombent de part et d'autre.
        let couchant = Some(Vec3::new(0.80, 0.38, 0.22));
        let levant = Some(Vec3::new(-0.80, 0.38, 0.22));

        let mut empreintes = Vec::new();
        for soleil in [couchant, levant] {
            let Some(avec) = rendre(&complete, 384, cadrage, soleil).expect("garnie") else {
                println!("  (aucun Vulkan — ce test s'abstient et ne mesure rien)");
                return;
            };
            let Some(sans) = rendre(&nue, 384, cadrage, soleil).expect("nue") else { return };
            assert_eq!(avec.octets.len(), sans.octets.len());

            // Les pixels qui montraient la table et que la scène garnie assombrit : recouverts par
            // un objet, OU dans son ombre. On ne sait pas encore lesquels — c'est la comparaison
            // des deux soleils qui le dira.
            let assombris: Vec<bool> = avec
                .octets
                .chunks_exact(3)
                .zip(sans.octets.chunks_exact(3))
                .map(|(a, s)| {
                    let (la, ls) = (luminance(a), luminance(s));
                    ls > 8 && ls - la > 8
                })
                .collect();
            empreintes.push(assombris);
        }

        let bouges = empreintes[0]
            .iter()
            .zip(empreintes[1].iter())
            .filter(|(a, b)| a != b)
            .count();

        assert!(
            bouges > 200,
            "seulement {bouges} pixels changent quand le soleil passe d'un côté à l'autre : les \
             zones assombries sont les mêmes des deux côtés, donc ce sont les objets EUX-MÊMES et \
             non leur ombre. La carte d'ombre n'agit pas — vérifier que `Ombre::dessiner` est bien \
             appelée, que sa matrice de lumière couvre la scène, et que le descripteur du cadre \
             est lié."
        );
    }

    /// ⚠ Le tampon d'instances est partagé par toutes les passes d'une image. À capacité trop
    /// faible, `poser` rend `None`, `dessiner_un` ne dessine rien, et **aucune erreur n'est levée** :
    /// l'image sort entièrement noire. Le compteur du moteur ne le dirait qu'à l'image suivante,
    /// laquelle n'existe pas dans un rendu unique.
    #[test]
    fn une_scene_rendue_n_est_jamais_vide() {
        let scene = scene_de_test();
        let Some(rendu) = rendre(&scene, 512, None, None).expect("le rendu") else {
            println!("  (aucun Vulkan — ce test s'abstient et ne mesure rien)");
            return;
        };
        let dessines = rendu.octets.chunks_exact(3).filter(|p| luminance(p) > 8).count();
        let total = rendu.octets.len() / 3;
        assert!(
            dessines > total / 40,
            "{dessines} pixels dessinés sur {total} : la scène est vide ou presque"
        );
    }

    /// Retirer des objets doit retirer des pixels : garde élémentaire contre un rendu qui
    /// ignorerait la scène qu'on lui donne et rendrait toujours la même image.
    #[test]
    fn retirer_des_objets_reduit_ce_qui_est_dessine() {
        let complete = scene_de_test();
        let nue = scene_sans(&complete, &["Circle", "Circle.001"]);
        let cadrage = Some(boite_englobante(&complete));

        let Some(avec) = rendre(&complete, 384, cadrage, None).expect("garnie") else { return };
        let Some(sans) = rendre(&nue, 384, cadrage, None).expect("nue") else { return };

        let compte = |r: &Rendu| r.octets.chunks_exact(3).filter(|p| luminance(p) > 8).count();
        assert!(
            compte(&avec) > compte(&sans),
            "la scène garnie ({}) ne couvre pas plus que la table nue ({})",
            compte(&avec),
            compte(&sans)
        );
    }

    /// `scene_sans` doit produire des indices qui pointent tous dans ses propres sommets — une
    /// erreur de décalage donnerait une géométrie explosée, ou un plantage de la carte.
    #[test]
    fn une_sous_scene_garde_des_indices_valides() {
        let complete = scene_de_test();
        let nue = scene_sans(&complete, &["Circle", "Circle.001"]);
        assert!(!nue.indices.is_empty());
        let max = nue.indices.iter().copied().max().unwrap() as usize;
        assert!(max < nue.sommets.len(), "indice {max} pour {} sommets", nue.sommets.len());
        let somme: u32 = nue.parties.iter().map(|p| p.nombre_indices).sum();
        assert_eq!(somme as usize, nue.indices.len());
    }
}
