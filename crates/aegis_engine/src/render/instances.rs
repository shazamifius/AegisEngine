//! # LES INSTANCES — dire une fois ce qu'on répète mille fois
//!
//! Né le 30 août 2026, sur un chiffre qui ne laissait pas le choix.
//!
//! ## Le chiffre
//!
//! **3 458 appels de dessin par image, pour 42 374 triangles.** Soit douze triangles par appel :
//! exactement un cube. Le moteur demandait donc à la carte trois mille quatre cent cinquante-huit
//! fois « dessine-moi un cube », là où un seul appel dessine la même chose.
//!
//! Meta recommande de rester **sous 150 appels par image** sur un Quest 2 — et il en faut deux
//! jeux, un par œil. On était **vingt-trois fois au-dessus**, sur la machine qui est la référence
//! déclarée du projet. *Ce n'était pas un défaut de rendu : c'était le jeu qui ne pouvait pas
//! tourner là où il doit tourner.*
//!
//! ## Ce que ça change, et pourquoi c'est aussi une simplification
//!
//! Ce qui distingue un cube d'un autre — sa matrice, sa teinte, ses réglages — voyageait en
//! **constantes poussées**, un envoi par objet. Ces 96 octets deviennent des **attributs de
//! sommet par instance** : un seul tampon, écrit une fois par image, et un appel par maillage.
//!
//! ⭐ Et la contrainte qui a coûté un chantier entier **disparaît avec** : les constantes poussées
//! sont plafonnées à 128 octets garantis par Vulkan, ce plafond avait déjà été dépassé une fois
//! sans que rien ne le signale. Un tampon d'instances n'a pas cette limite. *La constante
//! arbitraire ne rétrécit pas, elle cesse d'exister.*
//!
//! ## Ce que ça ne change pas
//!
//! Rien à l'écran. C'est la même géométrie, les mêmes couleurs, le même éclairage — seule la
//! façon de le demander change. Une différence visible signalerait une faute, pas un progrès.

use crate::core::math::{Mat4, Vec4};
use crate::core::memory::MemoryManager;
use crate::GpuContext;
use ash::vk;
use std::cell::Cell;

/// Ce qui distingue un objet d'un autre : 96 octets, les mêmes qu'avant.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Instance {
    pub modele: Mat4,
    pub teinte: Vec4,
    pub params: Vec4,
}

/// Le tampon qui porte les instances d'une image.
///
/// ⚠ **Une seule image est en vol dans ce moteur** (une barrière unique), et c'est ce qui rend sûr
/// de réécrire ce tampon à chaque image sans double tampon ni synchronisation. Le jour où deux
/// images voleront en parallèle, cette hypothèse tombe — et elle est écrite ici pour qu'on la
/// retrouve plutôt que de la redécouvrir par un scintillement qu'on ne s'explique pas.
pub struct Instances {
    tampon: vk::Buffer,
    memoire: vk::DeviceMemory,
    adresse: *mut Instance,
    capacite: usize,
    /// Où en est le remplissage de l'image courante.
    curseur: Cell<usize>,
    /// Combien d'instances ont été refusées faute de place, depuis le dernier relevé.
    perdues: Cell<usize>,
}

impl Instances {
    pub fn nouveau(
        gpu: &GpuContext,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        capacite: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let taille = (capacite * std::mem::size_of::<Instance>()) as vk::DeviceSize;

        let (tampon, memoire) = MemoryManager::create_buffer(
            &gpu.device,
            memory_props,
            taille,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let adresse = unsafe {
            gpu.device.map_memory(memoire, 0, taille, vk::MemoryMapFlags::empty())? as *mut Instance
        };

        log::info!(
            "Instances : {capacite} emplacements, {} Ko projetes une fois pour toutes",
            taille / 1024
        );

        Ok(Self {
            tampon,
            memoire,
            adresse,
            capacite,
            curseur: Cell::new(0),
            perdues: Cell::new(0),
        })
    }

    /// À appeler une fois par image, avant tout dépôt.
    pub fn recommencer(&self) {
        // ⚠ Le compte des instances perdues est rendu AVANT d'être remis à zéro, et journalisé par
        // l'appelant : un débordement silencieux ferait chercher un défaut de géométrie là où il
        // n'y a qu'un tampon trop petit. C'est le même raisonnement que le compte de lumières.
        let perdues = self.perdues.replace(0);
        if perdues > 0 {
            log::warn!(
                "instances : {perdues} objets non dessines a l'image precedente — capacite {} \
                 depassee",
                self.capacite
            );
        }
        self.curseur.set(0);
    }

    /// Dépose un lot d'instances et rend l'index de la première.
    ///
    /// Rend `None` quand le tampon est plein : l'appelant ne dessine alors rien plutôt que de
    /// dessiner n'importe quoi.
    pub fn poser(&self, lot: &[Instance]) -> Option<u32> {
        let debut = self.curseur.get();
        if debut + lot.len() > self.capacite {
            self.perdues.set(self.perdues.get() + lot.len());
            return None;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(lot.as_ptr(), self.adresse.add(debut), lot.len());
        }
        self.curseur.set(debut + lot.len());
        Some(debut as u32)
    }

    /// Attache le tampon d'instances au point de liaison 1.
    ///
    /// ⚠ Le point 0 porte les sommets du maillage, le point 1 les instances. Les intervertir
    /// donne une géométrie explosée sans qu'aucune erreur ne soit levée.
    pub fn lier(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        unsafe {
            device.cmd_bind_vertex_buffers(cmd, 1, &[self.tampon], &[0]);
        }
    }

    /// Dessine un maillage une seule fois, avec sa propre instance.
    ///
    /// C'est le chemin des objets uniques — un décor, un élément d'interface. Il passe par le même
    /// tampon que la foule de cubes : *deux façons de décrire un objet, ce serait deux formats à
    /// tenir, et un shader qui doit savoir lequel on lui parle.*
    pub fn dessiner_un(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        maillage: &crate::geometry::gpu_mesh::GpuMesh,
        modele: Mat4,
        teinte: Vec4,
        params: Vec4,
    ) {
        if let Some(premiere) = self.poser(&[Instance { modele, teinte, params }]) {
            maillage.dessiner_instances(device, cmd, premiere, 1);
        }
    }

    /// Dessine un maillage unique décrit par les anciennes constantes poussées.
    ///
    /// ⚠ Ce pont existe parce que `PushConstants` porte **exactement** les mêmes trois champs
    /// qu'une instance : c'est la même description d'objet, elle a seulement changé de chemin
    /// pour aller jusqu'à la carte. Le garder évite de réécrire vingt-trois sites d'appel — et
    /// surtout de risquer une faute dans l'un d'eux en les recopiant à la main.
    pub fn dessiner_avec(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        maillage: &crate::geometry::gpu_mesh::GpuMesh,
        push: &crate::render::push_constants::PushConstants,
    ) {
        self.dessiner_un(device, cmd, maillage, push.model_matrix, push.color_tint, push.params);
    }

    /// Combien d'instances ont été déposées dans l'image courante.
    pub fn deposees(&self) -> usize {
        self.curseur.get()
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.unmap_memory(self.memoire);
            device.destroy_buffer(self.tampon, None);
            device.free_memory(self.memoire, None);
        }
    }
}

/// Les descriptions Vulkan des attributs d'instance, pour le pipeline.
///
/// ⚠ Une matrice 4×4 n'existe pas comme attribut de sommet : elle se déclare en **quatre
/// `vec4` consécutifs**, que le shader recompose. C'est la façon dont Vulkan procède, et
/// l'oublier donne un pipeline qui se crée sans erreur et dessine des objets à des positions
/// absurdes.
pub fn attributs() -> Vec<vk::VertexInputAttributeDescription> {
    let mut sortie = Vec::with_capacity(6);
    for i in 0..4u32 {
        sortie.push(
            vk::VertexInputAttributeDescription::default()
                .binding(1)
                .location(5 + i)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(i * 16),
        );
    }
    sortie.push(
        vk::VertexInputAttributeDescription::default()
            .binding(1)
            .location(9)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(64),
    );
    sortie.push(
        vk::VertexInputAttributeDescription::default()
            .binding(1)
            .location(10)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(80),
    );
    sortie
}

/// La description du point de liaison des instances.
pub fn liaison() -> vk::VertexInputBindingDescription {
    vk::VertexInputBindingDescription::default()
        .binding(1)
        .stride(std::mem::size_of::<Instance>() as u32)
        // ⚠ `INSTANCE` et non `VERTEX` : c'est le mot qui change tout. En `VERTEX`, chaque sommet
        // lirait une instance différente et le maillage se disloquerait.
        .input_rate(vk::VertexInputRate::INSTANCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L'agencement doit être celui que le pipeline annonce, sans remplissage surprise.
    #[test]
    fn l_agencement_d_une_instance_est_celui_declare_au_pipeline() {
        assert_eq!(std::mem::size_of::<Instance>(), 96, "une matrice et deux vec4");
        assert_eq!(std::mem::size_of::<Mat4>(), 64);
        assert_eq!(std::mem::size_of::<Vec4>(), 16);

        // Les décalages déclarés dans `attributs()` doivent tomber sur les vrais champs.
        let attrs = attributs();
        assert_eq!(attrs.len(), 6, "quatre pour la matrice, un pour la teinte, un pour les params");
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[3].offset, 48, "la quatrieme colonne de la matrice");
        assert_eq!(attrs[4].offset, 64, "la teinte suit immediatement la matrice");
        assert_eq!(attrs[5].offset, 80);
        assert_eq!(liaison().stride, 96);
    }

    /// ⚠ Les emplacements ne doivent JAMAIS empiéter sur ceux des sommets (0 à 4).
    #[test]
    fn les_emplacements_d_instance_ne_recouvrent_pas_ceux_des_sommets() {
        for a in attributs() {
            assert!(
                a.location >= 5,
                "l'emplacement {} entre en conflit avec un attribut de sommet",
                a.location
            );
            assert_eq!(a.binding, 1, "les instances vivent au point de liaison 1");
        }
    }

    /// Le taux de lecture décide de tout : en `VERTEX`, le maillage se disloquerait.
    #[test]
    fn les_instances_se_lisent_par_instance_et_non_par_sommet() {
        assert_eq!(liaison().input_rate, vk::VertexInputRate::INSTANCE);
    }
}
