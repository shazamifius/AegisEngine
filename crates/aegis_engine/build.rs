use std::env;
use std::fs;
use std::path::Path;

/// Colle en tête d'un shader les fichiers qu'il déclare inclure.
///
/// ## Pourquoi ce mécanisme existe
///
/// WGSL n'a pas d'inclusion, et naga n'en fournit pas. La conséquence se lisait dans le dépôt : la
/// structure `Cadre` était **recopiée dans deux shaders**, sous un commentaire qui disait lui-même
/// que les faire diverger « décalerait les ombres sans qu'aucune ligne ne paraisse fausse ». Le
/// fond devait en produire une troisième copie.
///
/// *Un commentaire qui demande de ne pas diverger ne protège de rien ; il note la faute à venir.*
/// Douze lignes de Rust suffisent à la rendre impossible — et c'est du Rust, donc rien n'entre
/// dans la chaîne du projet.
///
/// La syntaxe est une ligne seule : `//!inclure commun`. Elle est remplacée sur place, ce qui
/// garde l'ordre naturel (le préambule avant son usage).
///
/// ⚠ Les numéros de ligne rapportés par naga portent alors sur le fichier **assemblé**, pas sur
/// celui qu'on édite. C'est le prix, il est connu, et le message d'erreur le rappelle.
fn assembler(source: &str, dossier: &Path) -> String {
    let mut sortie = String::with_capacity(source.len() * 2);
    for ligne in source.lines() {
        match ligne.trim().strip_prefix("//!inclure ") {
            Some(nom) => {
                let chemin = dossier.join(format!("{}.wgsl", nom.trim()));
                let inclus = fs::read_to_string(&chemin)
                    .unwrap_or_else(|_| panic!("shader inclus introuvable : {}", chemin.display()));
                // Récursif : un préambule peut lui-même en inclure un autre.
                sortie.push_str(&assembler(&inclus, dossier));
            }
            None => sortie.push_str(ligne),
        }
        sortie.push('\n');
    }
    sortie
}

fn compile_wgsl(source: &str, nom: &str) -> Vec<u32> {
    // ⚠ Les numéros de ligne ci-dessous portent sur le shader ASSEMBLÉ (préambules collés en
    // tête), pas sur le fichier tel qu'on l'édite. Le dire ici évite de chercher une erreur à une
    // ligne qui n'existe pas — c'est le seul coût du mécanisme d'inclusion.
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| {
        panic!("{nom} : WGSL illisible (lignes comptees APRES inclusion des preambules)\n{e:?}")
    });
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let module_info = validator.validate(&module).unwrap_or_else(|e| {
        panic!("{nom} : WGSL refuse a la validation (lignes comptees APRES inclusion)\n{e:?}")
    });
    let mut spv_options = naga::back::spv::Options::default();
    spv_options.flags.insert(naga::back::spv::WriterFlags::DEBUG);
    naga::back::spv::write_vec(&module, &module_info, &spv_options, None).expect("SPIR-V generation failed")
}

fn main() {
    println!("cargo:rerun-if-changed=src/shaders/");
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let shaders = [
        ("background.wgsl", "background.vert.spv", "background.frag.spv"),
        ("party_2d5.wgsl", "party_2d5.vert.spv", "party_2d5.frag.spv"),
        ("ombre.wgsl", "ombre.vert.spv", "ombre.frag.spv"),
        ("composition.wgsl", "composition.vert.spv", "composition.frag.spv"),
        ("halo_extraction.wgsl", "halo_extraction.vert.spv", "halo_extraction.frag.spv"),
        ("halo_descente.wgsl", "halo_descente.vert.spv", "halo_descente.frag.spv"),
        ("halo_montee.wgsl", "halo_montee.vert.spv", "halo_montee.frag.spv"),
        ("occlusion.wgsl", "occlusion.vert.spv", "occlusion.frag.spv"),
        ("copie.wgsl", "copie.vert.spv", "copie.frag.spv"),
        ("refraction.wgsl", "refraction.vert.spv", "refraction.frag.spv"),
        ("cartes.wgsl", "cartes.vert.spv", "cartes.frag.spv"),
    ];

    for (src_file, vert_out, frag_out) in shaders {
        let dossier = Path::new("src/shaders");
        let src_path = dossier.join(src_file);
        let code = fs::read_to_string(&src_path).unwrap_or_else(|_| panic!("Impossible de lire {}", src_file));
        let spv = compile_wgsl(&assembler(&code, dossier), src_file);
        let u8_bytes: Vec<u8> = spv.iter().flat_map(|w| w.to_le_bytes()).collect();

        fs::write(Path::new(&out_dir).join(vert_out), &u8_bytes).unwrap_or_else(|_| panic!("Échec de l'écriture du SPIR-V vert pour {}", vert_out));
        fs::write(Path::new(&out_dir).join(frag_out), &u8_bytes).unwrap_or_else(|_| panic!("Échec de l'écriture du SPIR-V frag pour {}", frag_out));
    }
}
