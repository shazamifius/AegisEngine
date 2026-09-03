//! **CE QUE LE MOTEUR DEMANDE À UNE CARTE GRAPHIQUE — et la garde qui l'empêche d'en demander plus.**
//!
//! ## Pourquoi ce fichier existe (3 septembre 2026)
//!
//! Le moteur exigeait **quatre** fonctionnalités Vulkan et n'en utilisait **qu'une**. Les trois
//! autres étaient entrées le 11 août par un commit de *gameplay* (`84bfeb2`, « semi-texturing des
//! blocs, brins d'herbe 3D ») — personne ne les avait demandées, personne ne s'en servait, et
//! elles refusaient des machines pour rien :
//!
//! | Exigée avant ce fichier | Utilisée par le code vivant ? |
//! |---|---|
//! | `dynamicRendering` | ✅ `cmd_begin_rendering` — `ecran.rs`, `verre.rs` |
//! | `synchronization2` | ❌ **zéro** barrière `…Barrier2` hors des fichiers endormis ; les 10 barrières réelles sont en API 1.0 |
//! | `bufferDeviceAddress` | ❌ aucun usage, nulle part |
//! | `descriptorIndexing` | ❌ seulement `_bindless.rs`, **endormi** donc non compilé |
//!
//! **C'est exactement la faute corrigée le 1ᵉʳ septembre sur le numéro de version** (« Vulkan 1.4
//! déclaré sans jamais s'en servir ») — sauf qu'on avait corrigé la version et laissé les
//! fonctionnalités. *Une prétention gratuite ne disparaît pas parce qu'on a corrigé sa voisine.*
//!
//! ## Ce que la spécification garantit, et pourquoi une seule des trois était vraiment dangereuse
//!
//! Vérifié dans la spec Vulkan (chapitre *Feature Requirements*), pas de seconde main :
//!
//! - **Sur un appareil qui annonce 1.3**, `dynamicRendering`, `synchronization2` et
//!   `bufferDeviceAddress` sont **obligatoires**. Les demander ne pouvait donc pas échouer.
//! - **`descriptorIndexing`, lui, n'est obligatoire que si `VK_EXT_descriptor_indexing` est
//!   supporté.** C'était donc la seule des quatre capable de faire échouer `vkCreateDevice` sur une
//!   machine 1.3 parfaitement valide — et c'était celle qui ne servait à rien.
//!
//! ## Ce que ça change pour la suite
//!
//! Il ne reste qu'une fonctionnalité, et elle existe aussi en extension `VK_KHR_dynamic_rendering`
//! **dès Vulkan 1.2**. Le jour où une machine de référence s'avère être en 1.2 — c'est une vraie
//! possibilité pour le Meta Quest 2, dont la version n'a **jamais** été vérifiée — la descente est
//! une extension à demander, pas un chantier. *La contrainte matérielle n'a pas rétréci : elle a
//! presque disparu.*
//!
//! ⚠ **Et le moteur DIT désormais ce qui manque.** Avant, `create_device` échouait sans message
//! utile sur une machine insuffisante : c'était la dette n° 4 des quatre pièges du moteur — « pour
//! un moteur qui vise tous les appareils du monde, c'est la vraie dette, pas le numéro de version ».

use ash::vk;

/// La version de Vulkan que le moteur demande à l'instance **et** exige du périphérique.
///
/// ⚠ **Les deux chemins (avec écran, sans écran) lisent cette constante.** Ils en portaient chacun
/// une copie littérale jusqu'au 3 septembre 2026 : *un banc qui tournerait sous une version
/// différente du jeu ne mesurerait pas le jeu.*
pub const VERSION_EXIGEE: u32 = vk::make_api_version(0, 1, 3, 0);

/// Le nom que le moteur donne de lui-même à Vulkan. Il apparaît dans les outils de diagnostic.
///
/// ⚠ Il annonçait `"AegisEngine Pure Vulkan 1.4"` jusqu'au 3 septembre 2026, **deux jours après**
/// que le moteur soit descendu en 1.3 — comme les deux lignes de journal et le README public.
/// *Une chaîne de caractères qui affirme une version est un commentaire comme un autre : elle
/// vieillit, et elle ment avec l'autorité d'une mesure quand quelqu'un diagnostique sur mobile.*
pub const NOM: &str = "AegisEngine";

/// Les fonctionnalités du cœur Vulkan 1.3 que le moteur active — **la liste, à un seul endroit**.
///
/// L'appelant fait le `push_next` lui-même : la structure doit rester vivante jusqu'à
/// `create_device`, ce que le langage ne permet pas de cacher ici. Mais **aucune décision** ne vit
/// chez l'appelant, seulement la plomberie — et c'est ce qui compte : les deux chemins ne peuvent
/// plus diverger sur *ce qui est demandé*.
pub fn fonctionnalites_13<'a>() -> vk::PhysicalDeviceVulkan13Features<'a> {
    vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true)
}

/// Ce qui manque à une carte pour faire tourner ce moteur — **vide si elle convient**.
///
/// ⭐ **Fonction PURE** : elle ne reçoit que ce que la carte annonce, donc elle se teste **sans
/// GPU**. C'est le même patron que `render::cibles::format_hdr`, qui était jusqu'ici le seul
/// endroit du moteur à négocier quoi que ce soit — au lieu d'exiger et d'échouer sans un mot.
///
/// Les messages nomment **la fonctionnalité ET le chemin de repli**, parce qu'un diagnostic sur une
/// machine qu'on n'a pas sous la main ne vaut que par ce qu'il dit à celui qui la tient.
pub fn ce_qui_manque(version_annoncee: u32, dynamic_rendering: bool) -> Vec<String> {
    let mut manques = Vec::new();

    if version_annoncee < VERSION_EXIGEE {
        manques.push(format!(
            "Vulkan {}.{} — cette carte annonce {}.{}. Le moteur n'utilise qu'une seule \
             fonctionnalite de la 1.3 (`dynamicRendering`), disponible en extension \
             `VK_KHR_dynamic_rendering` des la 1.2 : la descente est une extension a demander, \
             pas une reecriture.",
            vk::api_version_major(VERSION_EXIGEE),
            vk::api_version_minor(VERSION_EXIGEE),
            vk::api_version_major(version_annoncee),
            vk::api_version_minor(version_annoncee),
        ));
    }

    // ⚠ Vérifié même quand la version suffit : sur un appareil 1.3 la spécification la rend
    // obligatoire, mais un pilote peut mentir sur sa version — et une garde qui fait confiance à
    // une déclaration n'est pas une garde. Le coût est d'un appel déjà fait par ailleurs.
    if !dynamic_rendering {
        manques.push(
            "la fonctionnalite `dynamicRendering` — la seule que ce moteur utilise. \
             Sans elle, aucune passe de rendu ne peut demarrer."
                .to_string(),
        );
    }

    manques
}

/// Interroge la carte et refuse **en le disant** si elle ne convient pas.
///
/// # Safety
/// *(titre en anglais : clippy le reconnait comme un symbole, pas comme de la prose.)*
/// L'appelant garantit que `physical_device` provient bien de `instance`.
pub unsafe fn verifier(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<(), Box<dyn std::error::Error>> {
    let props = unsafe { instance.get_physical_device_properties(physical_device) };

    let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut f2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut f13);
    unsafe { instance.get_physical_device_features2(physical_device, &mut f2) };

    let manques = ce_qui_manque(props.api_version, f13.dynamic_rendering == vk::TRUE);
    if manques.is_empty() {
        return Ok(());
    }

    let nom = unsafe { std::ffi::CStr::from_ptr(props.device_name.as_ptr()) }.to_string_lossy();
    Err(format!(
        "cette carte ne peut pas faire tourner le moteur.\n  Carte : {nom}\n  Il manque :\n    - {}",
        manques.join("\n    - ")
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une carte 1.3 qui a `dynamicRendering` convient — et le message d'erreur ne se déclenche pas
    /// par excès de zèle.
    #[test]
    fn une_carte_suffisante_ne_manque_de_rien() {
        assert!(ce_qui_manque(vk::make_api_version(0, 1, 3, 0), true).is_empty());
        assert!(ce_qui_manque(vk::make_api_version(0, 1, 4, 0), true).is_empty());
    }

    /// ⚠ Le cas qui compte vraiment, et qui n'a **jamais** été vérifié sur la machine de référence :
    /// un appareil en 1.1 ou 1.2. Le moteur doit le dire, pas planter sans un mot.
    #[test]
    fn une_carte_trop_ancienne_est_refusee_en_disant_pourquoi() {
        let manques = ce_qui_manque(vk::make_api_version(0, 1, 1, 0), true);
        assert_eq!(manques.len(), 1, "un seul manque attendu : la version");
        assert!(manques[0].contains("1.1"), "le message doit nommer ce que la carte annonce");
        assert!(
            manques[0].contains("VK_KHR_dynamic_rendering"),
            "le message doit nommer le chemin de repli, sinon il constate sans aider"
        );
    }

    /// Une carte qui annonce 1.3 mais ne rend pas `dynamicRendering` est refusée aussi.
    /// *La spécification l'interdit ; un pilote n'est pas la spécification.*
    #[test]
    fn une_carte_sans_rendu_dynamique_est_refusee_meme_si_sa_version_suffit() {
        let manques = ce_qui_manque(vk::make_api_version(0, 1, 3, 0), false);
        assert_eq!(manques.len(), 1);
        assert!(manques[0].contains("dynamicRendering"));
    }

    /// ⭐⭐ **LA GARDE QUI REMPLACE UN PARAGRAPHE DE DOCUMENTATION PAR UN TEST.**
    ///
    /// « Le moteur ne demande que ce qu'il utilise » était écrit dans trois documents, et **deux
    /// d'entre eux étaient faux** — ils affirmaient que `descriptorIndexing` n'était pas demandé
    /// alors qu'il l'était depuis le 11 août. *Une affirmation d'état vérifiable n'a rien à faire
    /// dans de la prose : elle s'y périme sans que rien ne le dise.*
    ///
    /// La sonde : pour chaque fonctionnalité activée dans ce fichier, chercher dans le code
    /// **vivant** au moins un appel qui s'en sert. Les fichiers endormis (préfixe `_`) ne comptent
    /// pas — le compilateur ne les voit pas, donc leur usage n'existe pas.
    ///
    /// *C'est la leçon du 23 août 2026, appliquée ici : écrire la règle est la moitié du travail ;
    /// la rendre inatteignable est l'autre.*
    #[test]
    fn le_moteur_ne_demande_que_ce_qu_il_utilise() {
        let vivant = sources_vivantes();

        for fonctionnalite in fonctionnalites_activees() {
            let preuves = preuves_d_usage(&fonctionnalite).unwrap_or_else(|| {
                panic!(
                    "`{fonctionnalite}` est activee et ce test ne sait pas comment prouver son \
                     usage. Ajoute-la a PREUVES avec ce qu'on doit trouver dans le code — c'est \
                     le prix a payer pour exiger quelque chose d'une carte graphique."
                )
            });

            assert!(
                preuves.iter().any(|motif| vivant.contains(motif)),
                "`{fonctionnalite}` est demandee a la carte et **aucun code vivant ne s'en sert** \
                 (cherche : {preuves:?}).\n  Une fonctionnalite exigee sans usage refuse des \
                 machines pour rien. Soit on s'en sert, soit on ne la demande pas."
            );
        }
    }

    /// ⚠ La garde ci-dessus ne vaut que si elle regarde le bon code. Celui-ci le vérifie sur un
    /// cas connu-positif : `cmd_begin_rendering` **doit** s'y trouver.
    ///
    /// *Sans lui, une erreur de chemin rendrait `sources_vivantes()` vide — et le test au-dessus
    /// passerait en accusant tout le monde, ou en n'accusant personne. Une absence n'est jamais une
    /// preuve tant que l'instrument n'a pas montré qu'il sait produire une présence.*
    #[test]
    fn la_sonde_lit_bien_le_code_du_moteur() {
        let vivant = sources_vivantes();
        assert!(
            vivant.contains("cmd_begin_rendering"),
            "la sonde ne lit pas les sources du moteur : elle ne prouve donc rien"
        );
        assert!(
            !vivant.contains("PARTIALLY_BOUND"),
            "la sonde ramasse les fichiers endormis (`_bindless.rs`), qui ne sont pas compiles"
        );
    }

    /// Ce qu'on doit trouver dans le code pour qu'une fonctionnalité soit dite « utilisée ».
    ///
    /// ⚠ **Une fonctionnalité absente de cette table fait ÉCHOUER le test**, exprès : c'est le seul
    /// remède connu à « une liste oublie toujours quelque chose ». *Ajouter une exigence oblige à
    /// dire comment on prouve qu'elle sert.*
    const PREUVES: &[(&str, &[&str])] = &[
        ("dynamic_rendering", &["cmd_begin_rendering"]),
        ("synchronization2", &["cmd_pipeline_barrier2", "queue_submit2", "MemoryBarrier2"]),
        ("buffer_device_address", &["get_buffer_device_address", "SHADER_DEVICE_ADDRESS"]),
        (
            "descriptor_indexing",
            &["PARTIALLY_BOUND", "UPDATE_AFTER_BIND", "runtime_descriptor_array"],
        ),
        ("timeline_semaphore", &["SemaphoreTypeCreateInfo"]),
        ("multiview", &["RenderingInfo::default().view_mask", "view_mask("]),
    ];

    fn preuves_d_usage(nom: &str) -> Option<&'static [&'static str]> {
        PREUVES.iter().find(|(f, _)| *f == nom).map(|(_, p)| *p)
    }

    /// Les fonctionnalités réellement activées, **lues dans ce fichier** plutôt que recopiées.
    ///
    /// *Une seconde liste tenue à la main se serait périmée le jour où la première a changé — c'est
    /// précisément le défaut que ce chantier corrige.*
    fn fonctionnalites_activees() -> Vec<String> {
        let moi = include_str!("capacites.rs");
        let debut = moi
            .find("pub fn fonctionnalites_13")
            .expect("la fabrique de fonctionnalites a ete renommee : ce test ne sait plus quoi lire");
        let corps = &moi[debut..];
        let fin = corps.find("\n}").expect("corps de fonction non termine");

        extraire_activations(&corps[..fin]).collect()
    }

    /// Extrait les `.nom_de_fonctionnalite(true)` d'un morceau de code.
    fn extraire_activations(code: &str) -> impl Iterator<Item = String> + '_ {
        code.split('.').skip(1).filter_map(|morceau| {
            let fin = morceau.find('(')?;
            if !morceau[fin..].starts_with("(true)") {
                return None;
            }
            let nom = &morceau[..fin];
            nom.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                .then(|| nom.to_string())
        })
    }

    /// Tout le code **compilé** du moteur, concaténé. Les fichiers endormis sont écartés.
    fn sources_vivantes() -> String {
        let racine = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut tout = String::new();
        rassembler(&racine, &mut tout);
        tout
    }

    fn rassembler(dossier: &std::path::Path, dans: &mut String) {
        let Ok(entrees) = std::fs::read_dir(dossier) else { return };
        for entree in entrees.flatten() {
            let chemin = entree.path();
            let nom = entree.file_name();
            let nom = nom.to_string_lossy();
            // ⚠ La convention du projet : un fichier inactif porte le préfixe `_` et sa ligne
            // `mod` est retirée. Le compilateur ne le voit pas — cette sonde non plus.
            if nom.starts_with('_') {
                continue;
            }
            // ⚠⚠ ET CE FICHIER-CI S'EXCLUT LUI-MÊME, sans quoi la sonde compterait son propre
            // vocabulaire : la table `PREUVES` cite `PARTIALLY_BOUND`, `cmd_pipeline_barrier2` et
            // les autres motifs qu'elle cherche. Un `grep` qui trouve les mots qu'il vient
            // d'écrire répond « oui » à tout — le projet est déjà tombé dedans le 22 août 2026,
            // avec un contrôle qui comptait le texte citant les formules recherchées.
            if nom == "capacites.rs" {
                continue;
            }
            if chemin.is_dir() {
                rassembler(&chemin, dans);
            } else if nom.ends_with(".rs") {
                if let Ok(texte) = std::fs::read_to_string(&chemin) {
                    dans.push_str(&texte);
                }
            }
        }
    }
}
