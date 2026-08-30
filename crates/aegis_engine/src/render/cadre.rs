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
    /// Combien la lumière DIRECTE est forte, par rapport à l'ambiante.
    ///
    /// ## ⭐ Le réglage qui décide si une image a du relief
    ///
    /// Ciel et sol ne font pas que peindre le fond : ce sont eux qui éclairent les faces qu'aucune
    /// lampe n'atteint. Les baisser pour assombrir le fond assombrit donc **aussi les objets**, et
    /// l'image reste aussi plate — plus sombre, mais plate.
    ///
    /// Ce qui donne du relief est le RAPPORT entre les deux. Une scène ensoleillée a une ambiante
    /// faible et un soleil dur : les faces éclairées sont franches, les autres tombent dans une
    /// ombre colorée. Une scène par temps couvert a l'inverse — beaucoup d'ambiante, pas de
    /// direct — et **rien ne s'y détache de rien**.
    ///
    /// *Mesuré sur la scène du jeu : étendue tonale 20 points sur 100, 92 % de l'image perçue
    /// comme grise. C'est exactement le portrait d'un temps couvert, et c'est ce qu'on décrivait
    /// comme « tout triste » sans pouvoir le nommer.*
    ///
    /// ⚠ Comme `rugosite` et `reflectance`, ce champ est ici **faute d'un système de lumières que
    /// le jeu puisse régler**. Le jour où les lumières se posent comme des objets de la scène, il
    /// déménage avec elles et cette ligne disparaît.
    pub intensite_soleil: f32,
}

impl Ambiance {
    /// Les champs réglables, avec le nombre de valeurs que chacun attend.
    ///
    /// ⚠ **Une seule liste**, et c'est elle qui gouverne à la fois le réglage, l'aide et le test.
    /// Ajouter un champ à `Ambiance` sans l'inscrire ici le rendrait invisible au laboratoire —
    /// donc un test compare cette liste au texte de la structure, plutôt que de compter sur la
    /// mémoire de qui ajoutera le prochain.
    pub const CHAMPS: [(&'static str, usize); 7] = [
        ("ciel", 3),
        ("sol", 3),
        ("exposition", 1),
        ("point_blanc", 1),
        ("rugosite", 1),
        ("reflectance", 1),
        ("intensite_soleil", 1),
    ];

    /// Change un réglage désigné par son nom.
    ///
    /// ## ⭐ Pourquoi cette fonction existe, et pourquoi elle est ICI
    ///
    /// Le juge du rendu perçu est son œil, jamais une métrique — et un œil ne peut trancher que ce
    /// qu'il voit. Tant qu'un changement de couleur demandait une recompilation, l'aller-retour
    /// était trop long pour comparer, donc le réglage se faisait de mémoire, donc mal.
    ///
    /// *Ce n'est pas « une commande de debug » : c'est l'instrument qui rend une décision
    /// artistique décidable.* Il règle, et ce qui lui plaît devient le défaut du jeu.
    ///
    /// Elle vit dans le moteur parce que le moteur seul sait quels champs une ambiance possède ;
    /// la console ne fait que transporter du texte. Et étant **pure**, elle se teste entièrement
    /// sans fenêtre, sans GPU, et sans qu'aucune image ne soit dessinée.
    pub fn regler(&mut self, champ: &str, valeurs: &[f32]) -> Result<(), String> {
        let Some((_, attendues)) = Self::CHAMPS.iter().find(|(nom, _)| *nom == champ) else {
            let connus: Vec<&str> = Self::CHAMPS.iter().map(|(n, _)| *n).collect();
            return Err(format!("champ inconnu : {champ:?} — connus : {}", connus.join(", ")));
        };
        if valeurs.len() != *attendues {
            return Err(format!(
                "{champ} attend {attendues} valeur(s), {} recue(s)",
                valeurs.len()
            ));
        }
        // ⚠ Aucune borne haute : une exposition de 12 ou un ciel a 3,0 sont laids, pas invalides,
        // et c'est SON oeil qui doit le constater. Brider un laboratoire, c'est lui interdire de
        // trouver ce qu'on n'avait pas prevu. Seules les valeurs qui n'ont pas de SENS sont
        // refusees — une couleur negative, un point blanc nul qui diviserait par zero.
        if valeurs.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("une valeur doit etre finie et positive".to_string());
        }
        match champ {
            "ciel" => self.ciel = [valeurs[0], valeurs[1], valeurs[2]],
            "sol" => self.sol = [valeurs[0], valeurs[1], valeurs[2]],
            "exposition" => self.exposition = valeurs[0],
            "point_blanc" => {
                if valeurs[0] <= 0.0 {
                    return Err("le point blanc divise la courbe : il doit etre > 0".to_string());
                }
                self.point_blanc = valeurs[0];
            }
            "rugosite" => self.rugosite = valeurs[0],
            "reflectance" => self.reflectance = valeurs[0],
            "intensite_soleil" => self.intensite_soleil = valeurs[0],
            _ => unreachable!("la liste des champs a deja tranche"),
        }
        Ok(())
    }

    /// Le réglage courant, écrit **tel qu'il se recolle dans le code du jeu**.
    ///
    /// ⚠ La forme compte autant que le contenu : quand un réglage lui plaît, il doit devenir le
    /// défaut du jeu **sans être retranscrit**. Une transcription à la main, c'est une virgule
    /// perdue et un rendu qu'on ne retrouve plus — et on ne saurait même pas que c'est ça.
    pub fn decrire(&self) -> String {
        format!(
            "Ambiance {{\n    \
             ciel: [{:.3}, {:.3}, {:.3}],\n    \
             sol: [{:.3}, {:.3}, {:.3}],\n    \
             exposition: {:.3},\n    \
             point_blanc: {:.3},\n    \
             rugosite: {:.3},\n    \
             reflectance: {:.3},\n    \
             intensite_soleil: {:.3},\n}}",
            self.ciel[0],
            self.ciel[1],
            self.ciel[2],
            self.sol[0],
            self.sol[1],
            self.sol[2],
            self.exposition,
            self.point_blanc,
            self.rugosite,
            self.reflectance,
            self.intensite_soleil,
        )
    }
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
            // 2,45 = l'eclairement d'un soleil a 0,75, ramene par la conservation d'energie
            // (0,75 x pi / 0,96). C'est la valeur qui etait ecrite en dur dans le jeu.
            intensite_soleil: 2.45,
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
    /// Vue × projection **depuis la lumière** : elle sert deux fois, à dessiner la carte d'ombre
    /// et à savoir, pour chaque pixel, où le regarder dedans.
    ///
    /// ⚠ L'ordre des champs de cette structure est celui de `commun.wgsl`, et d'un seul autre
    /// endroit désormais. *Il était copié dans deux shaders jusqu'au 29 août 2026, sous un
    /// commentaire qui disait lui-même que les faire diverger « décalerait les ombres sans
    /// qu'aucune ligne ne paraisse fausse » — un commentaire qui demande de ne pas diverger note
    /// la faute à venir, il ne l'empêche pas.*
    pub light_view_proj: Mat4,
    /// L'inverse de `view_proj`, calculée une fois par image.
    ///
    /// Elle n'existe que pour le fond, et c'est ce qui la justifie : un fond est un triangle sans
    /// géométrie, il n'a aucun sommet dont hériter une direction. Sans elle, il ne pourrait
    /// qu'inventer un dégradé — c'est-à-dire choisir une couleur, donc franchir la frontière que
    /// tout ce fichier existe pour tenir. *64 octets par image contre une décision d'artiste
    /// gravée dans le moteur : le calcul est vite fait.*
    pub inv_view_proj: Mat4,
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
        light_view_proj: Mat4,
        camera_monde: [f32; 3],
        ambiance: Ambiance,
        lumieres: &[GpuLight],
    ) -> Self {
        let retenues = lumieres.len().min(MAX_LUMIERES);
        let mut tableau = [GpuLight::default(); MAX_LUMIERES];
        tableau[..retenues].copy_from_slice(&lumieres[..retenues]);
        Self {
            view_proj,
            light_view_proj,
            // Calculée ici, une fois par image, et jamais dans le shader : une inversion de
            // matrice par pixel serait ~60 opérations répétées deux millions de fois pour un
            // résultat rigoureusement identique partout. *Jamais d'excédent.*
            inv_view_proj: view_proj.inverse(),
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

        let liaisons = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                // Le sommet en a besoin pour `view_proj`, le fragment pour les lumières.
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            // La carte d'ombre, et l'échantillonneur qui sait la comparer. Deux liaisons plutôt
            // qu'une combinée : c'est ce que WGSL produit, et aligner le code sur ce que le shader
            // déclare vraiment évite un décalage silencieux entre les deux.
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let layout_descripteur = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&liaisons),
                None,
            )?
        };

        let tailles = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1),
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

    /// Branche la carte d'ombre sur ce descripteur.
    ///
    /// ⚠ **Appelée APRÈS la création de l'ombre, et c'est forcé par une dépendance circulaire** :
    /// l'ombre a besoin du layout de pipeline, qui a besoin du layout de ce descripteur, qui est
    /// créé ici. Séparer la *déclaration* du descripteur de son *remplissage* est ce qui casse le
    /// cercle — et c'est plus honnête qu'un ordre d'appel à respecter de mémoire.
    ///
    /// ⚠⚠ Sans cet appel, le descripteur porte des liaisons déclarées mais vides : le pilote
    /// l'accepte souvent en silence et rend des ombres absentes ou aléatoires. C'est exactement la
    /// forme « mécanisme branché d'un seul côté » — d'où la journalisation, pour que le geste
    /// laisse une trace au lieu d'être supposé.
    pub fn brancher_la_carte_d_ombre(
        &self,
        device: &ash::Device,
        vue: vk::ImageView,
        echantillonneur: vk::Sampler,
    ) {
        let image = vk::DescriptorImageInfo::default()
            .image_view(vue)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL);
        let sampler = vk::DescriptorImageInfo::default().sampler(echantillonneur);

        let ecritures = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image)),
            vk::WriteDescriptorSet::default()
                .dst_set(self.ensemble)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&sampler)),
        ];
        unsafe { device.update_descriptor_sets(&ecritures, &[]) };
        log::info!("Cadre : carte d'ombre branchee sur les liaisons 1 et 2");
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

    /// ⚠⚠ LA GARDE QUI REMPLACE LA PRECEDENTE : plus rien ne DOIT pousser de constantes.
    ///
    /// Le test ci-dessus veillait a ce qu'elles tiennent sous 128 octets. Depuis
    /// l'instanciation (30 aout 2026), il n'y en a plus une seule : la description d'un objet
    /// voyage dans un tampon d'instances, qui n'a pas de plafond.
    ///
    /// *Un test qui garde une propriete devenue vide est un test mort* — il reste vert quoi qu'il
    /// arrive et donne l'illusion d'une surveillance. Celui-ci garde ce qui est vrai maintenant :
    /// qu'aucun chemin de rendu ne rouvre cette porte par commodite, et ne ramene avec elle la
    /// limite qui a failli rendre le moteur incapable de demarrer sur un Quest 2.
    #[test]
    fn plus_aucun_chemin_de_rendu_ne_pousse_de_constantes() {
        let fichiers: [(&str, &str); 4] = [
            ("render/file.rs", include_str!("file.rs")),
            ("render/ombre.rs", include_str!("ombre.rs")),
            ("render/instances.rs", include_str!("instances.rs")),
            ("ui/mod.rs", include_str!("../ui/mod.rs")),
        ];

        let mut coupables = Vec::new();
        for (nom, source) in fichiers {
            for (numero, ligne) in source.lines().enumerate() {
                let code = ligne.split("//").next().unwrap_or("");
                if code.contains("cmd_push_constants") {
                    coupables.push(format!("{nom} ligne {}", numero + 1));
                }
            }
        }

        assert!(
            coupables.is_empty(),
            "ces chemins poussent encore des constantes, alors que tout passe par les \
             instances :\n  {}\nUne seconde facon de decrire un objet, c'est un second format a \
             tenir — et le retour du plafond de 128 octets.",
            coupables.join("\n  ")
        );
    }

    /// Tous les fichiers WGSL du moteur — les shaders compilés **et** les préambules qu'ils
    /// incluent, puisque la faute peut vivre dans un préambule aussi bien que dans un shader.
    ///
    /// ⚠⚠ **UNE SEULE LISTE, et c'est le point.** Elle existait en DEUX exemplaires jusqu'au
    /// 30 août 2026 — la garde des couleurs et celle de la gamma en tenaient chacune la sienne, à
    /// jour d'accord par la seule vigilance. Ajouter le halo en aurait fait deux listes de dix
    /// lignes à maintenir en parallèle, c'est-à-dire, tôt ou tard, une seule des deux corrigée.
    ///
    /// *C'est exactement la faute que `commun.wgsl` a fermée côté shaders, retrouvée côté tests.*
    /// Un troisième test confronte celle-ci à `build.rs`, qui décide de ce qui est réellement
    /// compilé : une liste ne peut donc plus oublier un shader neuf.
    const SHADERS: &[(&str, &str)] = &[
        ("commun.wgsl", include_str!("../shaders/commun.wgsl")),
        ("objet.wgsl", include_str!("../shaders/objet.wgsl")),
        ("plein_ecran.wgsl", include_str!("../shaders/plein_ecran.wgsl")),
        ("party_2d5.wgsl", include_str!("../shaders/party_2d5.wgsl")),
        ("background.wgsl", include_str!("../shaders/background.wgsl")),
        ("ombre.wgsl", include_str!("../shaders/ombre.wgsl")),
        ("composition.wgsl", include_str!("../shaders/composition.wgsl")),
        ("halo.wgsl", include_str!("../shaders/halo.wgsl")),
        ("halo_extraction.wgsl", include_str!("../shaders/halo_extraction.wgsl")),
        ("halo_descente.wgsl", include_str!("../shaders/halo_descente.wgsl")),
        ("halo_montee.wgsl", include_str!("../shaders/halo_montee.wgsl")),
        ("occlusion.wgsl", include_str!("../shaders/occlusion.wgsl")),
        ("copie.wgsl", include_str!("../shaders/copie.wgsl")),
    ];

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
    /// ⚠ **La garde couvre TOUS les shaders du moteur, et pas seulement l'éclairage.** Elle n'en
    /// couvrait qu'un seul jusqu'au 29 août 2026, et la dette laissée dehors — `background.wgsl`
    /// et son « blanc pur studio » écrit en dur — est précisément ce qui rendait l'image
    /// désagréable : des objets éclairés à `0,17` posés sur un fond peint à `0,97`, deux mondes
    /// dans la même image qui ne se parlaient pas. *Une garde posée sur UN chemin n'est pas une
    /// garde ; ici la preuve est venue de l'œil avant de venir du test.*
    #[test]
    fn aucun_shader_du_moteur_ne_choisit_de_couleur() {
        let mut coupables = Vec::new();

        for &(nom, source) in SHADERS {
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
                                coupables.push(format!(
                                    "{nom} ligne {} : vec3<f32>({arguments})",
                                    numero + 1
                                ));
                            }
                        }
                    }
                    reste = &apres[fin..];
                }
            }
        }

        assert!(
            coupables.is_empty(),
            "le moteur choisit des couleurs, ce qui est le role du jeu :\n  {}\n\
             Ces valeurs doivent passer par `Ambiance`, pose par l'appelant.",
            coupables.join("\n  ")
        );
    }

    /// ⚠ La garde ci-dessus vaut ce que vaut sa liste — et une liste tenue à la main se périme.
    ///
    /// Ce test la compare à celle de `build.rs`, qui est la source de vérité (c'est elle qui décide
    /// ce qui est réellement compilé dans le moteur). Ajouter un shader sans l'inscrire dans la
    /// garde devient donc impossible sans qu'un test tombe. *Le seul remède qui tienne à
    /// « une liste oublie toujours quelque chose » est une seconde liste qui la contredit.*
    #[test]
    fn la_garde_des_couleurs_couvre_tous_les_shaders_compiles() {
        let build = include_str!("../../build.rs");
        let garde = include_str!("cadre.rs");

        let mut manquants = Vec::new();
        for ligne in build.lines() {
            // ⚠ Seules les lignes du TABLEAU comptent — celles qui nomment aussi un `.vert.spv`.
            // Sans ce filtre la sonde ramassait `format!("{}.wgsl", …)`, du texte de `build.rs`
            // qui *parle* de shaders sans en déclarer aucun, et accusait un fichier nommé
            // `{}.wgsl`. *Une sonde qui compte son propre vocabulaire ne mesure rien ;* c'est le
            // même piège que le `grep` qui trouve les formules qu'il cite lui-même.
            if !ligne.contains(".vert.spv\"") {
                continue;
            }
            let Some(debut) = ligne.find("(\"") else { continue };
            let apres = &ligne[debut + 2..];
            let Some(fin) = apres.find(".wgsl\"") else { continue };
            let nom = format!("{}.wgsl", &apres[..fin]);
            // ⚠ On cherche dans le TEXTE de ce fichier, pas dans la constante : c'est ce qui rend
            // la sonde indépendante de la façon dont la liste est écrite, et ce qui la ferait
            // encore tomber si quelqu'un remplaçait `include_str!` par autre chose sans y penser.
            if !garde.contains(&format!("(\"{nom}\", include_str!")) {
                manquants.push(nom);
            }
        }

        assert!(
            manquants.is_empty(),
            "ces shaders sont compiles dans le moteur mais echappent a la garde des couleurs : \
             {manquants:?}\nLes ajouter a la liste de `aucun_shader_du_moteur_ne_choisit_de_couleur`."
        );
    }

    /// Le laboratoire règle vraiment ce qu'il annonce régler.
    #[test]
    fn regler_une_ambiance_change_ce_qu_il_faut_et_rien_d_autre() {
        let mut a = Ambiance::default();
        let temoin = a.sol;

        a.regler("ciel", &[0.1, 0.2, 0.4]).expect("le ciel doit se regler");
        assert_eq!(a.ciel, [0.1, 0.2, 0.4]);
        assert_eq!(a.sol, temoin, "regler le ciel ne doit pas toucher au sol");

        a.regler("exposition", &[1.6]).expect("l'exposition doit se regler");
        assert!((a.exposition - 1.6).abs() < 1e-6);
    }

    /// ⚠ Ce qui est refusé l'est parce que ça n'a pas de SENS, jamais parce que c'est laid.
    #[test]
    fn le_laboratoire_refuse_l_absurde_et_accepte_le_laid() {
        let mut a = Ambiance::default();

        assert!(a.regler("cieI", &[0.1, 0.2, 0.3]).is_err(), "un nom fautif doit se voir");
        assert!(a.regler("ciel", &[0.1, 0.2]).is_err(), "une couleur a trois composantes");
        assert!(a.regler("ciel", &[-0.1, 0.2, 0.3]).is_err(), "une couleur negative n'existe pas");
        assert!(a.regler("exposition", &[f32::NAN]).is_err(), "un NaN contaminerait toute l'image");
        assert!(a.regler("point_blanc", &[0.0]).is_err(), "il divise la courbe de tonalite");

        // Laid, mais parfaitement valide : c'est son œil qui juge, pas ce code.
        assert!(a.regler("exposition", &[14.0]).is_ok());
        assert!(a.regler("ciel", &[3.0, 0.0, 3.0]).is_ok());
    }

    /// ⚠⚠ La garde qui rend le laboratoire complet : un champ ajouté à `Ambiance` sans être
    /// inscrit dans `CHAMPS` serait **invisible au réglage**, sans que rien ne le signale — le
    /// laboratoire aurait l'air de fonctionner en ne montrant qu'une partie des leviers.
    #[test]
    fn le_laboratoire_atteint_tous_les_champs_de_l_ambiance() {
        let source = include_str!("cadre.rs");
        // ⚠ On démarre APRÈS l'accolade : la ligne `pub struct Ambiance {` commence elle aussi par
        // `pub `, et une sonde qui la ramasse accuse un champ nommé « struct Ambiance { ». *Une
        // sonde qui lit le texte parlant de la chose au lieu de la chose ne mesure rien* — c'est
        // le même piège que le `grep` qui trouve les formules qu'il cite lui-même.
        const ENTETE: &str = "pub struct Ambiance {";
        let debut = source.find(ENTETE).expect("la structure existe") + ENTETE.len();
        let fin = debut + source[debut..].find("\n}").expect("elle se ferme");
        let corps = &source[debut..fin];

        let mut absents = Vec::new();
        for ligne in corps.lines() {
            let Some(reste) = ligne.trim().strip_prefix("pub ") else { continue };
            let Some(nom) = reste.split(':').next() else { continue };
            if !Ambiance::CHAMPS.iter().any(|(c, _)| *c == nom) {
                absents.push(nom.to_string());
            }
        }

        assert!(
            absents.is_empty(),
            "ces reglages existent dans Ambiance et sont hors d'atteinte du laboratoire : \
             {absents:?}\nLes ajouter a Ambiance::CHAMPS."
        );
    }

    /// Ce que le laboratoire affiche doit se recoller dans le code sans retouche.
    #[test]
    fn la_description_se_recolle_telle_quelle_dans_le_jeu() {
        let mut a = Ambiance::default();
        a.regler("ciel", &[0.12, 0.18, 0.31]).unwrap();
        let texte = a.decrire();

        assert!(texte.starts_with("Ambiance {"), "ce doit etre une valeur Rust, pas un rapport");
        assert!(texte.ends_with('}'));
        assert!(texte.contains("ciel: [0.120, 0.180, 0.310],"));
        // Chaque champ réglable doit apparaître : un réglage qu'on ne peut pas relire est perdu.
        for (nom, _) in Ambiance::CHAMPS {
            assert!(texte.contains(&format!("{nom}: ")), "{nom} manque dans la description");
        }
    }

    /// ⚠⚠ LA GARDE QUI FERME LA DOUBLE GAMMA — la faute qui rendait toute l'image terne.
    ///
    /// Elle ne fixe pas une valeur : elle vérifie une **cohérence entre deux endroits qui doivent
    /// s'accorder** et qui ne se voient pas l'un l'autre. Si la surface de présentation est
    /// demandée en `_SRGB`, elle encode déjà la gamma à l'écriture ; un shader qui en encode une
    /// seconde délave tout, sans qu'aucune erreur ne soit levée nulle part.
    ///
    /// *C'était le cas, et c'est ce qui a été mesuré sur une capture : le fond des panneaux du
    /// HUD, demandé à (13, 15, 20), sortait à (63, 69, 80).*
    ///
    /// Et la garde vaut dans les DEUX sens : si quelqu'un repasse un jour la surface en `UNORM`,
    /// ce test tombera aussi — parce qu'il faudra alors réintroduire l'encodage dans les shaders.
    #[test]
    fn la_gamma_n_est_encodee_qu_une_seule_fois() {
        let contexte = include_str!("../core/gpu_context.rs");
        let surface_encode = contexte.contains("vk::Format::B8G8R8A8_SRGB");

        let mut encodeurs = Vec::new();
        for &(nom, source) in SHADERS {
            for (numero, ligne) in source.lines().enumerate() {
                let code = ligne.split("//").next().unwrap_or("");
                // La signature d'un encodage de gamma en sortie : élever à 1/2,2 ou à 1/2,4.
                if code.contains("1.0 / 2.2") || code.contains("1.0/2.2") || code.contains("1.0 / 2.4")
                {
                    encodeurs.push(format!("{nom} ligne {}", numero + 1));
                }
            }
        }

        if surface_encode {
            assert!(
                encodeurs.is_empty(),
                "la surface de presentation est en _SRGB, elle encode DEJA la gamma — ces shaders \
                 en encodent une seconde et delavent toute l'image :\n  {}\n\
                 Ecrire du lineaire, et laisser la surface encoder.",
                encodeurs.join("\n  ")
            );
        } else {
            assert!(
                !encodeurs.is_empty(),
                "la surface n'est plus demandee en _SRGB : elle n'encode donc plus rien, et les \
                 shaders doivent reprendre cet encodage a leur charge — sinon toute l'image sort \
                 beaucoup trop sombre."
            );
        }
    }

    /// La conversion sRGB → linéaire doit exister, sinon les couleurs du jeu sont mal interprétées.
    ///
    /// ⚠ Un albédo sRGB traité comme linéaire fausse **tout** le calcul d'énergie du PBR, et le
    /// fausse *silencieusement* : l'image reste plausible, simplement délavée et fausse.
    #[test]
    fn les_couleurs_demandees_sont_converties_avant_le_calcul() {
        let commun = include_str!("../shaders/commun.wgsl");
        let eclairage = include_str!("../shaders/party_2d5.wgsl");

        assert!(
            commun.contains("fn vers_lineaire("),
            "la conversion sRGB vers lineaire doit vivre dans le preambule partage"
        );
        assert!(
            commun.contains("12.92") && commun.contains("0.04045"),
            "la vraie courbe sRGB, avec son segment droit pres du noir — pas un simple 2,2, \
             sans quoi les tons les plus sombres remontent visiblement"
        );
        assert!(
            eclairage.contains("vers_lineaire(in.color.rgb)"),
            "la couleur demandee par le jeu doit etre convertie avant d'entrer dans l'eclairage"
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
            64 * 3 + 16 + 16 * 3 + 48 * MAX_LUMIERES,
            "aucun remplissage ne doit s'etre glisse entre les champs"
        );
        assert_eq!(std::mem::align_of::<DonneesImage>(), 4);
    }

    /// Le compte de lumières est ce que le shader va parcourir — il doit suivre la réalité.
    #[test]
    fn le_compte_de_lumieres_suit_ce_qui_est_donne() {
        let soleil = GpuLight::new_directional(Vec3::new(0.4, 0.9, 0.7), Vec3::new(1.0, 1.0, 1.0), 3.0);
        let d = DonneesImage::nouvelle(Mat4::IDENTITY, Mat4::IDENTITY, [0.0, 0.0, 0.0], Ambiance::default(), &[soleil, soleil, soleil]);
        assert_eq!(d.lumieres_actives(), 3);
        // Les emplacements non utilises sont neutres, pas des restes d'une image precedente.
        assert_eq!(d.lumieres[3], GpuLight::default());

        let aucune = DonneesImage::nouvelle(Mat4::IDENTITY, Mat4::IDENTITY, [0.0, 0.0, 0.0], Ambiance::default(), &[]);
        assert_eq!(aucune.lumieres_actives(), 0);
    }

    /// ⚠ Un debordement doit etre BORNE, jamais une ecriture hors du tableau.
    #[test]
    fn trop_de_lumieres_sont_bornees_et_le_compte_le_dit() {
        let l = GpuLight::new_point(Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 0.5, 0.2), 100.0);
        let trop = vec![l; MAX_LUMIERES + 7];
        let d = DonneesImage::nouvelle(Mat4::IDENTITY, Mat4::IDENTITY, [1.0, 2.0, 3.0], Ambiance::default(), &trop);
        assert_eq!(
            d.lumieres_actives(),
            MAX_LUMIERES,
            "le compte doit dire la verite sur ce qui sera reellement eclaire"
        );
        assert_eq!(d.camera_et_compte.x, 1.0, "la position camera ne doit pas etre ecrasee");
    }
}
