use std::env;
use std::fs;
use std::path::Path;

fn compile_wgsl(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL parse failed");
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let module_info = validator.validate(&module).expect("WGSL validation failed");
    let mut spv_options = naga::back::spv::Options::default();
    spv_options.flags.insert(naga::back::spv::WriterFlags::DEBUG);
    naga::back::spv::write_vec(&module, &module_info, &spv_options, None).expect("SPIR-V generation failed")
}

fn main() {
    println!("cargo:rerun-if-changed=src/shaders/");
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let shaders = [
        ("background.wgsl", "background.vert.spv", "background.frag.spv"),
        ("glass_dispersive.wgsl", "glass_dispersive.vert.spv", "glass_dispersive.frag.spv"),
        ("party_2d5.wgsl", "party_2d5.vert.spv", "party_2d5.frag.spv"),
    ];

    for (src_file, vert_out, frag_out) in shaders {
        let src_path = Path::new("src/shaders").join(src_file);
        let code = fs::read_to_string(&src_path).unwrap_or_else(|_| panic!("Impossible de lire {}", src_file));
        let spv = compile_wgsl(&code);
        let u8_bytes: Vec<u8> = spv.iter().flat_map(|w| w.to_le_bytes()).collect();

        fs::write(Path::new(&out_dir).join(vert_out), &u8_bytes).unwrap_or_else(|_| panic!("Échec de l'écriture du SPIR-V vert pour {}", vert_out));
        fs::write(Path::new(&out_dir).join(frag_out), &u8_bytes).unwrap_or_else(|_| panic!("Échec de l'écriture du SPIR-V frag pour {}", frag_out));
    }
}
