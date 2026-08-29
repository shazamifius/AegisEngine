//! # LE CADRE — ce qui est vrai pour TOUTE une image : la caméra, et les lumières
//!
//! Né le 29 août 2026, étape 2 du chantier du rendu, et il règle **deux problèmes d'un coup** dont
//! un seul était cherché.
//!
//! ## 1. Le problème cherché : un shader ne peut pas éclairer ce qu'il ne connaît pas
//!
//! L'éclairage d'Aegis tenait en quatre lignes, avec **une direction de lumière écrite en dur dans
//! le shader**. `GpuLight` — la structure qui sait décrire directionnel, ponctuel et projecteur —
//! existait depuis le début et **aucun shader ne la lisait**. Pour qu'un shader lise des lumières,
//! il faut les lui donner ; c'est ce que fait ce fichier.
//!
//! ## 2. Le problème TROUVÉ en chemin, et il était plus grave
//!
//! Les constantes poussées du moteur pesaient **160 octets** : deux matrices, une teinte, quatre
//! réglages. Or **Vulkan ne garantit que 128 octets** (`maxPushConstantsSize`) — la machine de
//! développement en offre 256, beaucoup de GPU mobiles exactement 128.
//!
//! Autrement dit : **le moteur fonctionnait ici et aurait très probablement refusé de créer son
//! pipeline sur la machine de référence du projet**, un Meta Quest 2. Du « ça marche chez moi » sur
//! l'axe exact où le projet ne peut pas se le permettre — et parfaitement invisible sans aller lire
//! la limite.
//!
//! *Prouvé sur cette machine (`maxPushConstantsSize = 256`), **pas** vérifié sur un Quest : ce qui
//! est certain est que le moteur dépassait la garantie, pas qu'un appareil précis le refusait.*
//!
//! ## La correction, et pourquoi elle sert aussi l'élégance
//!
//! La matrice modèle-vue-projection était poussée **par objet**, alors que sa partie vue-projection
//! est **la même pour tous les objets d'une image**. On envoyait donc 64 octets identiques à chaque
//! appel de dessin : mesuré sur la scène de départ, ~2000 appels par image à 165 images par
//! seconde, soit **environ 21 Mo par seconde de données rigoureusement redondantes**.
//!
//! En remontant `view_proj` ici, les constantes poussées tombent à **96 octets** — sous la garantie
//! — et la redondance disparaît. *C'est « jamais d'excédent » au sens propre : on ne réduit pas un
//! gaspillage, on retire la raison qu'il avait d'exister.* Le shader fait un produit de matrices de
//! plus par sommet, ce que fait tout moteur, et il gagne au passage la position de la caméra dont
//! le spéculaire a besoin.

use crate::core::math::{Mat4, Vec4};
use crate::core::memory::MemoryManager;
use crate::scene::light::GpuLight;
use crate::GpuContext;
use ash::vk;

/// Combien de lumières une image peut porter.
///
/// ⚠ **Ce nombre est une borne, pas un objectif.** Il coûte de la mémoire constante (chaque
/// lumière pèse 48 octets, donc 16 lumières = 768 octets) et **rien en calcul tant qu'elles ne sont
/// pas allumées** : le shader ne parcourt que les lumières annoncées. Le jour où une scène en
/// demandera davantage, la bonne réponse ne sera pas de monter ce chiffre mais de passer à
/// l'échantillonnage par tuile (étape 3 du plan), dont le coût ne dépend plus du nombre de lumières.
pub const MAX_LUMIERES: usize = 16;

/// Ce que le JEU décide et que le moteur applique sans le connaître.
///
/// ## ⭐ LA FRONTIÈRE, et pourquoi ce type existe
///
/// **Le moteur fournit ce qui est VRAI ; le jeu fournit ce qui est BEAU.** Le moteur sait comment
/// la lumière se comporte — Lambert, GGX, Fresnel, la conservation d'énergie. Il ne doit *jamais*
/// savoir de quelle couleur est le ciel, ni si les ombres tirent sur le bleu.
///
/// ⚠ **Cette règle a été enfreinte le jour même où le PBR est arrivé**, et c'est ce qui a fait
/// naître ce type. Quatre décisions d'artiste s'étaient gravées dans `party_2d5.wgsl`, donc dans
/// le moteur : la rugosité (`0.55`), l'ambiante (`0.15, 0.17, 0.20`), le point blanc (`2.0`) et la
/// réflectance (`0.04`). Un moteur qui porte une couleur en dur a déjà choisi le rendu de tous les
/// jeux qu'il portera — il n'est plus un moteur, il est le décor d'un seul jeu.
///
/// ## L'ambiante n'est pas une valeur, c'est un CIEL et un SOL
///
/// Une ambiante grise unique donne des ombres *éteintes* : de l'absence de lumière. Dans le monde
/// réel comme dans les images qu'on aime, une ombre est **la couleur de ce qui l'éclaire encore**
/// — le ciel au-dessus, le sol qui renvoie en dessous. Deux couleurs au lieu d'une, interpolées
/// selon l'orientation de la surface, et les ombres cessent d'être grises.
///
/// *C'est trois lignes de shader, et c'est l'écart entre « terne » et « bleu de nuit ».*
///
/// ⚠ Poser `ciel == sol` redonne **exactement** l'ambiante plate d'avant : la capacité s'ajoute
/// sans rien changer tant que personne ne s'en sert. C'était voulu — un changement de rendu et un
/// changement d'architecture ne se prouvent pas dans le même commit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ambiance {
    /// Ce qui tombe du ciel sur les faces tournées vers le haut.
    pub ciel: [f32; 3],
    /// Ce qui remonte du sol sur les faces tournées vers le bas.
    pub sol: [f32; 3],
    /// Multiplie tout l'éclairage avant la courbe de tonalité. C'est le diaphragme.
    pub exposition: f32,
    /// Le niveau de lumière qui devient blanc. Plus il est haut, plus l'image garde de contraste
    /// dans les hautes lumières — et plus il faut de lumière pour saturer.
    pub point_blanc: f32,
    /// ⚠ **Rugosité et réflectance sont des propriétés de MATIÈRE, pas d'ambiance.** Elles sont
    /// ici faute de matériaux, et c'est écrit pour que personne ne prenne ce provisoire pour un
    /// choix : le jour où les matériaux existent, elles déménagent et ces deux lignes disparaissent.
    pub rugosite: f32,
    /// Réflectance à incidence normale. 0,04 pour les diélectriques (bois, pierre, plastique).
    pub reflectance: f32,
}

impl Default for Ambiance {
    /// Reproduit **exactement** le rendu d'avant l'existence de ce type — ciel et sol identiques,
    /// donc ambiante plate. Un défaut qui change l'image serait un piège pour qui l'adopte.
    fn default() -> Self {
        Self {
            ciel: [0.15, 0.17, 0.20],
            sol: [0.15, 0.17, 0.20],
            exposition: 1.0,
            point_blanc: 2.0,
            rugosite: 0.55,
            reflectance: 0.04,
        }
    }
}

/// Ce que le shader doit savoir et qui ne change pas d'un objet à l'autre.
///
/// L'agencement suit les règles d'alignement des tampons uniformes : `Mat4` et `Vec4` sont alignés
/// sur 16 octets, `GpuLight` est trois `vec4` consécutifs. Aucun remplissage n'est donc nécessaire,
/// et un test le vérifie plutôt que de l'espérer.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DonneesImage {
    /// Vue × projection, commune à toute l'image.
    pub view_proj: Mat4,
    /// `xyz` = position de la caméra dans le monde (le spéculaire en a besoin),
    /// `w` = nombre de lumières réellement allumées.
    pub camera_et_compte: Vec4,
    /// `rgb` = couleur du ciel, `w` = exposition.
    pub ciel_exposition: Vec4,
    /// `rgb` = couleur du sol, `w` = point blanc.
    pub sol_point_blanc: Vec4,
    /// `x` = rugosité, `y` = réflectance, `zw` libres.
    pub matiere: Vec4,
    pub lumieres: [GpuLight; MAX_LUMIERES],
}

impl DonneesImage {
    /// Prépare les données d'une image à partir d'une caméra et d'une liste de lumières.
    ///
    /// ⚠ **Les lumières au-delà de [`MAX_LUMIERES`] sont IGNORÉES, silencieusement pour le GPU mais
    /// pas pour l'appelant** : le compte rendu dit combien ont réellement été retenues. Un
    /// dépassement muet ferait chercher un défaut d'éclairage là où il n'y a qu'un débordement.
    pub fn nouvelle(
        view_proj: Mat4,
        camera_monde: [f32; 3],
        ambiance: Ambiance,
        lumieres: &[GpuLight],
    ) -> Self {
        let retenues = lumieres.len().min(MAX_LUMIERES);
        let mut tableau = [GpuLight::default(); MAX_LUMIERES];
        tableau[..retenues].copy_from_slice(&lumieres[..retenues]);
        Self {
            view_proj,
            camera_et_compte: Vec4::new(
                camera_monde[0],
                camera_monde[1],
                camera_monde[2],
                retenues as f32,
            ),
            ciel_exposition: Vec4::new(
                ambiance.ciel[0],
                ambiance.ciel[1],
                ambiance.ciel[2],
                ambiance.exposition,
            ),
            sol_point_blanc: Vec4::new(
                ambiance.sol[0],
                ambiance.sol[1],
                ambiance.sol[2],
                ambiance.point_blanc,
            ),
            matiere: Vec4::new(ambiance.rugosite, ambiance.reflectance, 0.0, 0.0),
            lumieres: tableau,
        }
    }

    /// Combien de lumières le shader va réellement parcourir.
    pub fn lumieres_actives(&self) -> usize {
        self.camera_et_compte.w as usize
    }
}

/// Le tampon qui porte [`DonneesImage`] sur la carte, et son descripteur.
pub struct Cadre {
    tampon: vk::Buffer,
    memoire: vk::DeviceMemory,
    /// Mémoire projetée une fois pour toutes : ces données changent à chaque image, et
    /// projeter/déprojeter 165 fois par seconde serait un coût pur.
    adresse: *mut DonneesImage,
    pub layout_descripteur: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    ensemble: vk::DescriptorSet,
}

impl Cadre {
    pub fn nouveau(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let taille = std::mem::size_of::<DonneesImage>() as vk::DeviceSize;

        let (tampon, memoire) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            taille,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let adresse = unsafe {
            gpu.device
                .map_memory(memoire, 0, taille, vk::MemoryMapFlags::empty())?
                as *mut DonneesImage
        };

        let liaison = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            // Le sommet en a besoin pour `view_proj`, le fragment pour les lumières.
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);

        let layout_descripteur = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(std::slice::from_ref(&liaison)),
                None,
            )?
        };

        let tailles = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)];
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
                    .set_layouts(std::slice::from_ref(&layout_descripteur)),
            )?[0]
        };

        let info_tampon = vk::DescriptorBufferInfo::default()
            .buffer(tampon)
            .offset(0)
            .range(taille);
        let ecriture = vk::WriteDescriptorSet::default()
            .dst_set(ensemble)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&info_tampon));
        unsafe { gpu.device.update_descriptor_sets(&[ecriture], &[]) };

        log::info!(
            "Cadre : {taille} octets sur la carte, jusqu'a {MAX_LUMIERES} lumieres par image"
        );

        Ok(Self {
            tampon,
            memoire,
            adresse,
            layout_descripteur,
            pool,
            ensemble,
        })
    }

    /// Écrit les données de l'image à venir.
    ///
    /// ⚠ **Une seule image en vol.** L'écriture est directe, sans double tampon, parce que
    /// [`crate::core::gpu_context::GpuContext::begin_frame`] attend la barrière de l'image
    /// précédente avant d'arriver ici : le GPU a fini de lire quand on récrit. *Si le moteur
    /// passait un jour à plusieurs images en vol, ce fichier devrait changer avec lui — et c'est ce
    /// commentaire qu'il faudra venir contredire, pas un scintillement à diagnostiquer.*
    pub fn ecrire(&self, donnees: &DonneesImage) {
        unsafe { std::ptr::write(self.adresse, *donnees) };
    }

    /// Rend le descripteur disponible au pipeline pour les dessins qui suivent.
    pub fn lier(&self, device: &ash::Device, cmd: vk::CommandBuffer, layout: vk::PipelineLayout) {
        unsafe {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                std::slice::from_ref(&self.ensemble),
                &[],
            );
        }
    }

    pub fn detruire(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout_descripteur, None);
            device.unmap_memory(self.memoire);
            device.destroy_buffer(self.tampon, None);
            device.free_memory(self.memoire, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::math::Vec3;

    /// ⚠ LE TEST QUI PROTÈGE LA CORRECTION : les constantes poussées doivent tenir sous la
    /// garantie Vulkan de 128 octets.
    ///
    /// C'est exactement le défaut corrigé le 29 août — 160 octets poussés, 256 disponibles sur la
    /// machine de développement, 128 garantis par la spécification. Ce test échoue si quelqu'un
    /// rajoute une matrice « juste pour essayer » ; sans lui, la panne se manifesterait chez
    /// quelqu'un d'autre, sur un appareil qu'on n'a pas.
    #[test]
    fn les_constantes_poussees_tiennent_dans_la_garantie_vulkan() {
        const GARANTIE_VULKAN: usize = 128;
        let taille = std::mem::size_of::<crate::render::push_constants::PushConstants>();
        assert!(
            taille <= GARANTIE_VULKAN,
            "les constantes poussees pesent {taille} octets, or Vulkan n'en garantit que \
             {GARANTIE_VULKAN} : le pipeline serait refuse sur un GPU qui s'en tient au minimum, \
             ce qui est le cas de beaucoup de mobiles et donc de la machine de reference du projet"
        );
    }

    /// ⚠⚠ LE TEST QUI REND LA FRONTIÈRE INATTEIGNABLE, pas seulement écrite.
    ///
    /// Le shader d'éclairage du moteur ne doit contenir **aucune couleur**. Une couleur y est une
    /// décision d'artiste gravée du côté du moteur — c'est exactement ce qui s'était produit le
    /// jour où le PBR est arrivé, et ce que ce fichier existe pour empêcher.
    ///
    /// La sonde : un `vec3<f32>(a, b, c)` dont les trois composantes diffèrent **est** une
    /// couleur. Un `vec3<f32>(x)` ou trois valeurs identiques restent permis — c'est une grandeur
    /// scalaire répétée (une réflectance, un facteur), pas une teinte.
    ///
    /// *Écrire la règle est la moitié du travail ; la rendre inatteignable est l'autre.*
    ///
    /// ⚠ Dette connue et non couverte : `background.wgsl` porte, lui, un vrai dégradé de couleurs
    /// — le fond du jeu vit dans le moteur. C'est la même faute, à un autre endroit, et elle
    /// demande son propre chantier plutôt qu'une exception discrète accordée ici.
    #[test]
    fn le_shader_d_eclairage_du_moteur_ne_choisit_aucune_couleur() {
        let source = include_str!("../shaders/party_2d5.wgsl");
        let mut coupables = Vec::new();

        for (numero, ligne) in source.lines().enumerate() {
            let sans_commentaire = ligne.split("//").next().unwrap_or("");
            let mut reste = sans_commentaire;
            while let Some(debut) = reste.find("vec3<f32>(") {
                let apres = &reste[debut + "vec3<f32>(".len()..];
                let Some(fin) = apres.find(')') else { break };
                let arguments = &apres[..fin];
                let valeurs: Vec<&str> = arguments.split(',').map(str::trim).collect();
                if valeurs.len() == 3 {
                    let nombres: Vec<Option<f32>> =
                        valeurs.iter().map(|v| v.parse::<f32>().ok()).collect();
                    // Trois littéraux qui ne sont pas tous égaux : c'est une teinte.
                    if nombres.iter().all(Option::is_some) {
                        let n: Vec<f32> = nombres.into_iter().flatten().collect();
                        if n[0] != n[1] || n[1] != n[2] {
                            coupables.push(format!("ligne {} : vec3<f32>({arguments})", numero + 1));
                        }
                    }
                }
                reste = &apres[fin..];
            }
        }

        assert!(
            coupables.is_empty(),
            "le moteur choisit des couleurs, ce qui est le role du jeu :\n  {}\n\
             Ces valeurs doivent passer par `Ambiance`, pose par l'appelant.",
            coupables.join("\n  ")
        );
    }

    /// L'agencement du tampon doit être celui que le shader attend, sans remplissage surprise.
    #[test]
    fn l_agencement_des_donnees_d_image_est_celui_annonce() {
        assert_eq!(std::mem::size_of::<GpuLight>(), 48, "trois vec4 par lumiere");
        assert_eq!(std::mem::size_of::<Mat4>(), 64);
        assert_eq!(std::mem::size_of::<Vec4>(), 16);
        assert_eq!(
            std::mem::size_of::<DonneesImage>(),
            64 + 16 + 16 * 3 + 48 * MAX_LUMIERES,
            "aucun remplissage ne doit s'etre glisse entre les champs"
        );
        assert_eq!(std::mem::align_of::<DonneesImage>(), 4);
    }

    /// Le compte de lumières est ce que le shader va parcourir — il doit suivre la réalité.
    #[test]
    fn le_compte_de_lumieres_suit_ce_qui_est_donne() {
        let soleil = GpuLight::new_directional(Vec3::new(0.4, 0.9, 0.7), Vec3::new(1.0, 1.0, 1.0), 3.0);
        let d = DonneesImage::nouvelle(Mat4::IDENTITY, [0.0, 0.0, 0.0], Ambiance::default(), &[soleil, soleil, soleil]);
        assert_eq!(d.lumieres_actives(), 3);
        // Les emplacements non utilises sont neutres, pas des restes d'une image precedente.
        assert_eq!(d.lumieres[3], GpuLight::default());

        let aucune = DonneesImage::nouvelle(Mat4::IDENTITY, [0.0, 0.0, 0.0], Ambiance::default(), &[]);
        assert_eq!(aucune.lumieres_actives(), 0);
    }

    /// ⚠ Un debordement doit etre BORNE, jamais une ecriture hors du tableau.
    #[test]
    fn trop_de_lumieres_sont_bornees_et_le_compte_le_dit() {
        let l = GpuLight::new_point(Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 0.5, 0.2), 100.0);
        let trop = vec![l; MAX_LUMIERES + 7];
        let d = DonneesImage::nouvelle(Mat4::IDENTITY, [1.0, 2.0, 3.0], Ambiance::default(), &trop);
        assert_eq!(
            d.lumieres_actives(),
            MAX_LUMIERES,
            "le compte doit dire la verite sur ce qui sera reellement eclaire"
        );
        assert_eq!(d.camera_et_compte.x, 1.0, "la position camera ne doit pas etre ecrasee");
    }
}
