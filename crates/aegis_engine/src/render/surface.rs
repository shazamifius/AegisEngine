//! **LA MÉMOIRE DE SURFACE — l'étage 0 de la thèse, dans sa version la plus petite qui soit vraie.**
//!
//! > *Tout vit sur la SURFACE. L'écran ne fait que la lire.*
//!
//! Ce fichier porte le premier des trois chantiers de l'étage 0 : **l'adressage**. Donner à chaque
//! triangle une plage de mémoire indexée par $(T, u, v)$, et prouver que cette adresse se calcule
//! **sans aucune recherche spatiale** — ni table de hachage, ni grille, ni arbre.
//!
//! ## Ce qui est ici, et ce qui n'y est pas
//!
//! | | État |
//! |---|---|
//! | **0.1 L'adressage** $(T,u,v)$ | ✅ ici, et vérifié par une bijection testée |
//! | **0.2 L'allocation** — quelle plage, quand, que faire quand la mémoire manque | ⛔ **pas ici.** La subdivision $k$ est **uniforme**, et c'est dit plus bas |
//! | **0.3 Lire depuis l'écran** | ⛔ pas encore — la mémoire est écrite et relue par le processeur, pas échantillonnée par un pixel |
//!
//! *Un fichier qui porte le nom d'une technique ne l'implémente pas : voici donc la liste de ce
//! qu'il ne fait pas, avant celle de ce qu'il fait.*
//!
//! ## ⭐ Le format d'une entrée : 8 octets, et le chiffre vient d'un budget
//!
//! `On-Surface Caches` (HPG 2024) paie **1 134 octets par entrée**. La décomposition, lue dans leur
//! § 3.2, tombe à l'octet près :
//!
//! | Poste | Octets | Part |
//! |---|---|---|
//! | hémisphère directionnel 8×8, **en double** (fp16 RGBA) | 1 024 | **90,3 %** |
//! | harmoniques sphériques L2 (9 coef × 3 canaux, fp16) | 54 | 4,8 % |
//! | 4 pointeurs 64 bits vers les entrées voisines | 32 | 2,8 % |
//! | position + normale en pleine précision | 24 | 2,1 % |
//!
//! ⚠⚠ **Et c'est un résultat négatif pour l'argument « l'adresse barycentrique supprime leurs
//! pointeurs ».** Elle les supprime bel et bien — pointeurs, position et normale — mais cela pèse
//! **56 octets sur 1 134, soit 4,9 %.** Les 90 % restants sont l'hémisphère, qu'une adresse
//! barycentrique paie **exactement au même prix**. *L'adressage est une innovation de qualité et de
//! simplicité ; ce n'en est pas une de budget, et le prétendre serait faux.*
//!
//! **Le budget, lui, tranche autrement.** L'Adreno 650 du Quest 2 offre ~44 Go/s, soit **611 Mo de
//! trafic mémoire par image** à 72 Hz pour 7,03 M pixels (deux yeux) — les 87 o/pixel de
//! `recherche/02-BUDGET.md`. En accordant 15 % de ce trafic à la lecture de la mémoire de surface,
//! **une entrée lue une fois par pixel ne peut pas dépasser ≈ 13 octets.**
//!
//! D'où le format retenu ici : **deux `u32`, quatre demi-flottants** — R, G, B, et un quatrième
//! réservé. 8 octets, aucune extension Vulkan requise (`pack2x16float` est du WGSL de base), et de
//! la vraie précision flottante. *Le moteur stocke de la **lumière**, jamais une couleur : un format
//! normalisé sur [0,1] aurait écrasé le haut de la dynamique.*
//!
//! ⚠ **Ce qui reste à gagner, et qui n'est pas fait :** `R11G11B10` descendrait à 4 octets sans
//! extension non plus, au prix d'un empaquetage écrit à la main. *À mesurer avant de le faire —
//! l'écart entre 8 et 4 octets vaut 5,7 points de budget, ce qui n'est pas rien mais ne se décide
//! pas sans la mesure.*
//!
//! ## L'adresse, et pourquoi elle ne cherche rien
//!
//! Un triangle subdivisé $k$ fois porte $n = 2^k$ segments par arête, donc
//! $(n+1)(n+2)/2$ micro-sommets rangés en triangle. Le micro-sommet $(i,j)$ vit à la coordonnée
//! barycentrique $(u,v) = (i/n,\ j/n)$, et son rang dans le triangle vaut
//!
//! ```text
//! rang(i, j) = j·(2n + 3 − j)/2 + i
//! ```
//!
//! *C'est une somme de rangées décroissantes, rien de plus.* L'adresse complète est
//! `base(T) + rang(i,j)`, et `base(T) = T · (n+1)(n+2)/2` tant que $k$ est uniforme.
//!
//! ⭐ **Le sens de tout ça : aucune de ces lignes ne consulte la scène.** Là où un nuage de surfels
//! doit interroger une table de hachage pour trouver le voisin d'un échantillon — 32 octets par
//! entrée chez OSC-GI, rien que pour éviter cette requête — l'adjacence est ici de l'arithmétique.
//!
//! ## ⚠ La limite qui compte : $k$ est UNIFORME
//!
//! Tout ce fichier suppose la **même** subdivision pour tous les triangles. C'est faux dans une
//! vraie scène, et le banc `topologie` a chiffré à quel point : **les aires des triangles d'un
//! `.glb` Blender ordinaire varient de 20 610 ×**. Une subdivision uniforme donne donc une densité
//! d'échantillons absurde sur un grand mur et famélique sur une petite pièce.
//!
//! *C'est exactement le chantier 0.2, et il n'est pas commencé.* Le dire ici évite qu'un lecteur
//! croie l'allocation résolue parce que l'adressage l'est.

use crate::core::memory::MemoryManager;
use crate::render::compute_pipeline::ComputePipelineManager;
use ash::vk;

/// Les octets d'une entrée : deux `u32`, soit quatre demi-flottants (R, G, B, réservé).
///
/// *Voir l'en-tête du module pour la dérivation — ce n'est pas un choix esthétique, c'est le
/// plafond que le budget du Quest 2 laisse à une entrée lue une fois par pixel.*
pub const OCTETS_PAR_ENTREE: u64 = 8;

/// Le nombre de micro-sommets d'un triangle subdivisé `k` fois.
///
/// $n = 2^k$ segments par arête, donc $(n+1)(n+2)/2$ sommets rangés en triangle.
pub fn micro_sommets(k: u32) -> u32 {
    let n = 1u32 << k;
    (n + 1) * (n + 2) / 2
}

/// Le rang du micro-sommet `(i, j)` dans son triangle, pour `n` segments par arête.
///
/// Les rangées se parcourent à `j` croissant ; la rangée `j` en contient `n − j + 1`. Le rang est
/// donc la somme des rangées précédentes, plus `i` :
///
/// $$\mathrm{rang}(i,j) = \sum_{m<j} (n - m + 1) + i = \frac{j(2n + 3 - j)}{2} + i$$
///
/// ⚠ Aucune vérification que `i + j ≤ n` : hors du triangle, le rang empiéterait sur la rangée
/// suivante. C'est à l'appelant de rester dans le domaine, et le shader le fait.
pub fn rang(i: u32, j: u32, n: u32) -> u32 {
    j * (2 * n + 3 - j) / 2 + i
}

/// L'inverse de [`rang`] : retrouve `(i, j)` depuis un rang.
///
/// ## Pourquoi une formule fermée plutôt qu'une boucle
///
/// Le shader dispatche un fil par micro-sommet ; il ne connaît que son rang et doit en déduire sa
/// coordonnée barycentrique. Une boucle sur les rangées coûterait $O(n)$ par fil — acceptable à
/// $n = 8$, absurde à $n = 64$.
///
/// On cherche le plus grand `j` tel que $\mathrm{rang}(0,j) \le r$, c'est-à-dire la plus petite
/// racine de $j^2 - (2n+3)j + 2r = 0$ :
///
/// $$j = \left\lfloor \frac{(2n+3) - \sqrt{(2n+3)^2 - 8r}}{2} \right\rfloor$$
///
/// ⚠ **La racine carrée est en virgule flottante, donc `j` peut tomber d'une unité à côté** au
/// voisinage exact d'un début de rangée. Les deux corrections qui suivent ferment le cas dans les
/// deux sens, et le test `l_adresse_est_une_bijection` le vérifie sur **tous** les rangs de
/// `k = 0..=6` — pas sur un échantillon.
pub fn depuis_rang(r: u32, n: u32) -> (u32, u32) {
    let b = (2 * n + 3) as f32;
    let disc = (b * b - 8.0 * r as f32).max(0.0);
    let mut j = ((b - disc.sqrt()) * 0.5) as u32;
    // Vers le haut : tant que la rangée suivante commence encore avant `r`, on y est déjà.
    while j < n && rang(0, j + 1, n) <= r {
        j += 1;
    }
    // Vers le bas : si la rangée retenue commence après `r`, on est monté trop haut.
    while j > 0 && rang(0, j, n) > r {
        j -= 1;
    }
    (r - rang(0, j, n), j)
}

/// La mémoire de surface elle-même : un tampon de stockage, et rien d'autre.
///
/// *Elle ne connaît ni la scène, ni la caméra, ni la lumière. Elle sait combien d'entrées elle
/// porte, et où elles sont.*
pub struct MemoireDeSurface {
    pub tampon: vk::Buffer,
    memoire: vk::DeviceMemory,
    /// Le nombre d'entrées — c'est-à-dire de micro-sommets, tous triangles confondus.
    pub entrees: u32,
    /// $n = 2^k$, le nombre de segments par arête.
    pub cote: u32,
    /// Les micro-sommets d'un seul triangle.
    pub par_triangle: u32,
}

impl MemoireDeSurface {
    /// Alloue la mémoire de surface d'un maillage de `triangles` triangles, subdivisés `k` fois.
    ///
    /// ⚠ **`HOST_VISIBLE` est un choix de BANC, pas d'architecture.** Il permet de relire la
    /// mémoire depuis le processeur pour prouver que le shader y a bien écrit. En production, ce
    /// tampon vivrait en `DEVICE_LOCAL` — *sur un GPU mobile à mémoire unifiée la distinction
    /// s'estompe, mais elle ne disparaît pas, et supposer qu'elle disparaît serait une conclusion
    /// sur une machine qu'on n'a pas.*
    pub fn allouer(
        device: &ash::Device,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        triangles: u32,
        k: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let par_triangle = micro_sommets(k);
        let entrees = triangles * par_triangle;
        let (tampon, memoire) = MemoryManager::create_buffer(
            device,
            memory_props,
            entrees as u64 * OCTETS_PAR_ENTREE,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        Ok(Self { tampon, memoire, entrees, cote: 1 << k, par_triangle })
    }

    /// Alloue la mémoire d'après un [`Plan`](crate::render::allocation::Plan) — la subdivision
    /// varie alors d'un triangle à l'autre.
    ///
    /// ⚠ **`cote` et `par_triangle` deviennent des valeurs de FAÇADE dans ce mode.** Ils ne
    /// décrivent plus tous les triangles, seulement le premier ; l'adresse réelle se lit dans la
    /// table du plan. *Ils sont laissés parce que le chemin uniforme les emploie encore, et un champ
    /// qui ne veut plus dire la même chose selon le chemin est exactement le genre de piège que ce
    /// projet paie — d'où cette phrase, faute de pouvoir les supprimer aujourd'hui.*
    pub fn allouer_selon(
        device: &ash::Device,
        memory_props: &vk::PhysicalDeviceMemoryProperties,
        plan: &crate::render::allocation::Plan,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (tampon, memoire) = MemoryManager::create_buffer(
            device,
            memory_props,
            plan.octets().max(OCTETS_PAR_ENTREE),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let premier = plan.par_triangle.first().map(|(_, c)| *c).unwrap_or(1);
        Ok(Self {
            tampon,
            memoire,
            entrees: plan.entrees,
            cote: premier,
            par_triangle: micro_sommets(premier.trailing_zeros()),
        })
    }

    /// Les octets réellement occupés.
    pub fn octets(&self) -> u64 {
        self.entrees as u64 * OCTETS_PAR_ENTREE
    }

    /// Relit la mémoire de surface depuis le processeur, entrée par entrée, en RVB.
    ///
    /// *C'est l'instrument de preuve du banc : sans lui, « le shader a écrit » resterait une
    /// affirmation sur un tampon que personne n'a ouvert.*
    pub fn relire(&self, device: &ash::Device) -> Result<Vec<[f32; 3]>, Box<dyn std::error::Error>> {
        let octets = self.octets();
        let ptr = unsafe {
            device.map_memory(self.memoire, 0, octets, vk::MemoryMapFlags::empty())? as *const u32
        };
        let mots = unsafe { std::slice::from_raw_parts(ptr, self.entrees as usize * 2) };
        let sortie = mots
            .chunks_exact(2)
            .map(|p| {
                let (r, v) = depack2x16float(p[0]);
                let (b, _) = depack2x16float(p[1]);
                [r, v, b]
            })
            .collect();
        unsafe { device.unmap_memory(self.memoire) };
        Ok(sortie)
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.tampon, None);
            device.free_memory(self.memoire, None);
        }
    }
}

/// ⭐⭐ **La LECTURE, côté processeur — le pendant exact de `lire_surface` dans `lecture.wgsl`.**
///
/// Trouve le micro-triangle qui contient $(u,v)$ et interpole entre ses trois coins.
///
/// ## Pourquoi elle existe en double
///
/// Non pas pour le rendu — l'écran lit sur la carte — mais pour **mesurer ce qu'aucune image ne
/// tranche** : le saut d'une valeur de part et d'autre d'une arête partagée par deux triangles de
/// subdivisions différentes. *Une carte d'écart ne peut pas répondre à cette question : elle varie
/// par triangle dès que la densité varie, qu'il y ait couture ou non.*
///
/// ⚠ **C'est une ré-implémentation, avec ce que ça coûte :** une erreur de conception partagée par
/// les deux chemins passerait. Le test `les_deux_lectures_disent_la_meme_chose` la contraint au
/// moins à rester alignée sur l'arithmétique du shader.
pub fn lire_interpole(
    entrees: &[[f32; 3]],
    base: u32,
    n: u32,
    u: f32,
    v: f32,
) -> [f32; 3] {
    let uu = u.clamp(0.0, 1.0);
    let vv = v.clamp(0.0, 1.0 - uu);
    let (gu, gv) = (uu * n as f32, vv * n as f32);
    let mut i = gu.floor() as u32;
    let mut j = gv.floor() as u32;
    if i + j >= n {
        // Sur l'arête, on recule d'une cellule pour rester dans le domaine.
        if i > 0 {
            i = i.saturating_sub(1);
        } else {
            j = j.saturating_sub(1);
        }
    }
    let (fu, fv) = (gu - i as f32, gv - j as f32);
    let e = |di: u32, dj: u32| entrees[(base + rang(i + di, j + dj, n)) as usize];
    // ⚠⚠ `i + j + 2 <= n` N'EST PAS UNE PRÉCAUTION : sans lui, la branche « micro-triangle
    // inversé » atteint le coin (i+1, j+1), qui n'existe pas sur la dernière cellule.
    //
    // En théorie le cas ne peut pas se produire — sur la diagonale extérieure, `i + j = n − 1`
    // force `fu + fv ≤ 1`. **En virgule flottante, si.** Un `fu + fv` qui vaut 1,0000001 fait
    // basculer dans la mauvaise branche. *Ici ça panique, et c'est une chance : dans le shader, le
    // même dépassement lirait la plage du triangle SUIVANT et rendrait une image plausible.*
    let inverse = fu + fv > 1.0 && i + j + 2 <= n;
    let (p0, p1, p2, w0, w1, w2) = if !inverse {
        (e(0, 0), e(1, 0), e(0, 1), 1.0 - fu - fv, fu, fv)
    } else {
        (e(1, 0), e(0, 1), e(1, 1), 1.0 - fv, 1.0 - fu, fu + fv - 1.0)
    };
    std::array::from_fn(|c| p0[c] * w0 + p1[c] * w1 + p2[c] * w2)
}

/// Défait un `pack2x16float` du WGSL : deux demi-flottants IEEE-754 dans un `u32`.
///
/// *Écrit à la main plutôt que délégué : le projet ne prend pas de dépendance pour seize lignes
/// d'arithmétique de bits, et une conversion qu'on ne sait pas dériver est une boîte noire dans le
/// chemin de vérification.*
fn depack2x16float(mot: u32) -> (f32, f32) {
    (demi_vers_f32(mot as u16), demi_vers_f32((mot >> 16) as u16))
}

fn demi_vers_f32(h: u16) -> f32 {
    let signe = (h as u32 & 0x8000) << 16;
    let exposant = (h as u32 >> 10) & 0x1f;
    let mantisse = h as u32 & 0x3ff;
    match exposant {
        // Zéro et sous-normaux : reconstruits par le calcul plutôt que par un cas particulier.
        0 => f32::from_bits(signe) + f32::from_bits(signe | 0x3800_0000) * (mantisse as f32 / 1024.0)
            - f32::from_bits(signe | 0x3800_0000) * 0.0,
        // Infinis et NaN.
        0x1f => f32::from_bits(signe | 0x7f80_0000 | (mantisse << 13)),
        // Le cas ordinaire : on décale l'exposant de son biais (15 → 127).
        _ => f32::from_bits(signe | ((exposant + 112) << 23) | (mantisse << 13)),
    }
}

/// Les réglages passés au shader, en constantes poussées.
///
/// ⚠ `repr(C)` et l'ordre des champs comptent : ils doivent correspondre exactement à la structure
/// `Reglages` de `surface.wgsl`. *Rien ne vérifie cette correspondance à la compilation — c'est la
/// couture la plus fragile de ce fichier, et le banc la teste de bout en bout.*
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reglages {
    pub triangles: u32,
    pub cote: u32,
    pub par_triangle: u32,
    pub _pad: u32,
    /// La direction dans laquelle le soleil VOYAGE (de la lumière vers la surface).
    ///
    /// ⚠ Le sens est écrit ici parce qu'il a déjà coûté : `examples/eclairer.rs` documente une
    /// convention de direction *supposée au lieu d'être lue*. Un vecteur d'éclairage sans sa
    /// convention est un piège à demi armé.
    pub soleil: [f32; 4],
    /// `xyz` = la teinte du signal, `w` = sa fréquence spatiale.
    ///
    /// ⚠⚠ **Elles viennent de l'appelant, et c'est la frontière du projet, pas une commodité.**
    /// Le moteur fournit ce qui est VRAI ; choisir une couleur est le rôle du jeu, et un test échoue
    /// si un shader du moteur en porte une. *La première version de `surface.wgsl` portait une
    /// teinte en dur, et la garde ne l'a pas vue — elle ne regardait pas les shaders de calcul.*
    pub signal: [f32; 4],
}

/// Les trois tampons que la passe LIT : la géométrie, et le plan d'allocation.
///
/// *Regroupés parce qu'ils voyagent toujours ensemble — et parce qu'une fonction à huit paramètres
/// dont six sont des paires est une fonction où l'on finit par intervertir deux arguments du même
/// type sans qu'aucun compilateur ne le dise.*
pub struct EntreesGeometrie {
    /// `(tampon, octets)` des sommets, en flottants bruts.
    pub sommets: (vk::Buffer, u64),
    /// `(tampon, octets)` des indices.
    pub indices: (vk::Buffer, u64),
    /// `(tampon, octets)` du plan d'allocation : deux `u32` par triangle.
    pub plan: (vk::Buffer, u64),
}

/// La passe de calcul qui remplit la mémoire de surface.
pub struct PasseDeSurface {
    layout_descripteur: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}

impl PasseDeSurface {
    /// Construit la passe et la branche sur les trois tampons qu'elle lit et écrit.
    pub fn nouvelle(
        device: &ash::Device,
        entrees: &EntreesGeometrie,
        memoire: &MemoireDeSurface,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (sommets, octets_sommets) = entrees.sommets;
        let (indices, octets_indices) = entrees.indices;
        let (plan, octets_plan) = entrees.plan;
        let liaisons: [vk::DescriptorSetLayoutBinding; 4] = std::array::from_fn(|i| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        });
        let layout_descripteur = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&liaisons),
                None,
            )?
        };

        let tailles = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(4)];
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default().pool_sizes(&tailles).max_sets(1),
                None,
            )?
        };
        let set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&layout_descripteur)),
            )?[0]
        };

        let infos = [
            vk::DescriptorBufferInfo::default().buffer(sommets).offset(0).range(octets_sommets),
            vk::DescriptorBufferInfo::default().buffer(indices).offset(0).range(octets_indices),
            vk::DescriptorBufferInfo::default().buffer(memoire.tampon).offset(0).range(memoire.octets()),
            vk::DescriptorBufferInfo::default().buffer(plan).offset(0).range(octets_plan),
        ];
        let ecritures: Vec<vk::WriteDescriptorSet> = (0..4)
            .map(|i| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(&infos[i]))
            })
            .collect();
        unsafe { device.update_descriptor_sets(&ecritures, &[]) };

        let plage = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<Reglages>() as u32);
        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(std::slice::from_ref(&layout_descripteur))
                    .push_constant_ranges(std::slice::from_ref(&plage)),
                None,
            )?
        };

        let code: Vec<u32> = crate::shaders::SURFACE_COMP_SPV
            .chunks_exact(4)
            .map(|o| u32::from_le_bytes([o[0], o[1], o[2], o[3]]))
            .collect();
        let module = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?
        };
        let pipeline = ComputePipelineManager::create_compute_pipeline(
            device,
            module,
            layout,
            c"main",
        )?;
        unsafe { device.destroy_shader_module(module, None) };

        Ok(Self { layout_descripteur, pool, set, layout, pipeline })
    }

    /// Encode le remplissage de la mémoire de surface, puis la barrière qui rend l'écriture
    /// visible à qui lira ensuite.
    ///
    /// ## ⚠⚠ Ce que la barrière fait, et ce qu'AUCUN test ne prouve aujourd'hui
    ///
    /// Elle rend l'écriture du shader visible à qui lira ensuite. Elle est exprimée dans la même
    /// fonction que le travail qu'elle protège, pour qu'on ne puisse pas encoder l'un sans
    /// l'autre — c'est la famille de défauts qu'`examples/eclairer.rs` documente trois fois : *une
    /// dépendance entre passes que rien n'exprime.*
    ///
    /// **Mais le banc `surface` ne la prouve pas, et c'est mesuré :** en la retirant, les 147 330
    /// entrées restent justes au bit près. La raison est que le banc attend la fin des commandes
    /// avant de relire — cette attente rend l'écriture visible de toute façon, et la barrière y est
    /// donc *redondante*.
    ///
    /// ⚠ **Elle cessera de l'être au geste suivant**, quand un shader de fragment lira cette
    /// mémoire dans la MÊME soumission : là, rien n'attendra plus, et son absence donnerait une
    /// image à moitié écrite sans qu'aucune erreur ne soit levée. *Elle est donc gardée pour ce
    /// jour-là — mais écrire qu'elle « corrige un défaut » aujourd'hui serait une garantie que le
    /// code ne tient pas, et c'est une mutation qui l'a montré, pas une relecture.*
    pub fn encoder(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        reglages: &Reglages,
        memoire: &MemoireDeSurface,
        rangs_max: u32,
    ) {
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                std::slice::from_ref(&self.set),
                &[],
            );
            let octets = std::slice::from_raw_parts(
                (reglages as *const Reglages) as *const u8,
                std::mem::size_of::<Reglages>(),
            );
            device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::COMPUTE, 0, octets);
            // Un fil par micro-sommet en X, un triangle en Y. Le groupe fait 64 en X.
            //
            // ⚠ `rangs_max` est le nombre de micro-sommets du triangle **le plus subdivisé** — pas
            // une moyenne. Les fils en trop d'un triangle grossier sortent dans le shader, qui relit
            // la subdivision réelle dans le plan. *C'est le gaspillage assumé d'un dispatch
            // rectangulaire sur une allocation qui ne l'est pas ; le mesurer est le chantier
            // suivant, l'ignorer serait le vrai défaut.*
            device.cmd_dispatch(
                cmd,
                ComputePipelineManager::calculate_workgroup_count(rangs_max, 64),
                reglages.triangles,
                1,
            );
            let barriere = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ | vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(memoire.tampon)
                .size(memoire.octets());
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&barriere),
                &[],
            );
        }
    }

    pub fn detruire(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout_descripteur, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_compte_des_micro_sommets_suit_la_subdivision() {
        assert_eq!(micro_sommets(0), 3, "un triangle non subdivisé a ses trois coins");
        assert_eq!(micro_sommets(1), 6);
        assert_eq!(micro_sommets(2), 15);
        assert_eq!(micro_sommets(3), 45);
    }

    /// ⭐ **La garde qui compte : l'adresse doit être une BIJECTION.**
    ///
    /// Deux micro-sommets qui partagent un rang s'écraseraient l'un l'autre en silence ; un rang que
    /// personne n'atteint serait de la mémoire allouée pour rien. *Une adresse fausse ne produit pas
    /// d'erreur — elle produit une image presque juste, ce qui est le pire cas.*
    ///
    /// Le test parcourt **tous** les rangs de `k = 0..=6`, pas un échantillon : à `k = 6` un
    /// triangle porte 2 145 micro-sommets, et c'est là que la racine carrée en virgule flottante
    /// de [`depuis_rang`] a le plus d'occasions de tomber à côté.
    #[test]
    fn l_adresse_est_une_bijection() {
        for k in 0..=6 {
            let n = 1u32 << k;
            let total = micro_sommets(k);
            let mut vus = vec![false; total as usize];
            for j in 0..=n {
                for i in 0..=(n - j) {
                    let r = rang(i, j, n);
                    assert!(r < total, "k={k} : le rang ({i},{j}) = {r} sort de la plage {total}");
                    assert!(!vus[r as usize], "k={k} : le rang {r} est atteint deux fois");
                    vus[r as usize] = true;
                    assert_eq!(depuis_rang(r, n), (i, j), "k={k} : l'inverse du rang {r} est faux");
                }
            }
            assert!(vus.iter().all(|v| *v), "k={k} : un rang n'est atteint par aucun (i,j)");
        }
    }

    /// La conversion demi-flottant → f32, contre des valeurs dont on connaît le motif binaire.
    ///
    /// *Sans ce test, une erreur de dépaquetage ferait accuser le shader — on chercherait le défaut
    /// dans le GPU alors qu'il serait dans l'instrument qui le lit.*
    #[test]
    fn le_depaquetage_des_demi_flottants_est_juste() {
        assert_eq!(demi_vers_f32(0x0000), 0.0);
        assert_eq!(demi_vers_f32(0x3c00), 1.0);
        assert_eq!(demi_vers_f32(0x4000), 2.0);
        assert_eq!(demi_vers_f32(0x3800), 0.5);
        assert_eq!(demi_vers_f32(0xbc00), -1.0);
        assert!((demi_vers_f32(0x3555) - 1.0 / 3.0).abs() < 1e-3);
    }

    /// ⭐ La lecture processeur doit rendre exactement la valeur stockée AUX micro-sommets.
    ///
    /// *Si elle ne le fait pas, toute mesure de couture bâtie dessus accuserait l'allocation d'un
    /// défaut qui viendrait de l'instrument.*
    #[test]
    fn la_lecture_rend_les_valeurs_aux_micro_sommets() {
        let n = 4u32;
        let total = micro_sommets(2) as usize;
        // Une valeur reconnaissable par micro-sommet : son propre rang.
        let entrees: Vec<[f32; 3]> = (0..total).map(|r| [r as f32, 0.0, 0.0]).collect();
        for j in 0..=n {
            for i in 0..=(n - j) {
                let lu = lire_interpole(&entrees, 0, n, i as f32 / n as f32, j as f32 / n as f32);
                assert!(
                    (lu[0] - rang(i, j, n) as f32).abs() < 1e-3,
                    "au micro-sommet ({i},{j}) la lecture rend {} au lieu de {}",
                    lu[0],
                    rang(i, j, n)
                );
            }
        }
    }

    /// Entre deux micro-sommets, la lecture doit rester DANS l'intervalle de ses trois coins.
    ///
    /// *Une interpolation dont les poids ne somment pas à 1, ou qui piocherait hors du
    /// micro-triangle, sortirait de cet encadrement — et rendrait une image plausible.*
    #[test]
    fn la_lecture_n_invente_jamais_de_valeur_hors_des_coins() {
        let n = 8u32;
        let total = micro_sommets(3) as usize;
        let entrees: Vec<[f32; 3]> = (0..total).map(|r| [(r % 17) as f32, 0.0, 0.0]).collect();
        let (mut mini, mut maxi) = (f32::MAX, f32::MIN);
        for e in &entrees {
            mini = mini.min(e[0]);
            maxi = maxi.max(e[0]);
        }
        for a in 0..=40 {
            for b in 0..=40 {
                let (u, v) = (a as f32 / 40.0, b as f32 / 40.0);
                if u + v > 1.0 {
                    continue;
                }
                let lu = lire_interpole(&entrees, 0, n, u, v)[0];
                assert!(lu >= mini - 1e-3 && lu <= maxi + 1e-3, "({u},{v}) rend {lu}");
            }
        }
    }

    /// Le format d'une entrée est un poste de budget : le figer ici rend tout changement visible.
    ///
    /// *8 octets × 7,03 M pixels = 56 Mo lus par image, soit 9,2 % des 611 Mo que l'Adreno 650
    /// laisse passer à 72 Hz. Le jour où quelqu'un porte l'entrée à 16 octets, ce test tombe et
    /// l'oblige à refaire le calcul.*
    #[test]
    fn une_entree_tient_dans_le_budget_du_quest_2() {
        const PIXELS: u64 = 2 * 1832 * 1920;
        const TRAFIC_PAR_IMAGE: u64 = 44_000_000_000 / 72;
        let part = (OCTETS_PAR_ENTREE * PIXELS) as f64 / TRAFIC_PAR_IMAGE as f64;
        assert!(
            part < 0.15,
            "une entrée de {OCTETS_PAR_ENTREE} o lue une fois par pixel prendrait {:.1} % du \
             trafic du Quest 2 — le plafond posé est 15 %",
            part * 100.0
        );
    }
}
