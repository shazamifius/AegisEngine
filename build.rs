use std::env;
use std::fs;
use std::path::Path;

fn compile_glsl(source: &str, stage: naga::ShaderStage) -> Vec<u32> {
    let mut parser = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options {
        stage,
        defines: Default::default(),
    };
    let module = parser.parse(&options, source).expect("GLSL parse failed");
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let module_info = validator.validate(&module).expect("GLSL validation failed");
    let mut spv_options = naga::back::spv::Options::default();
    spv_options.flags.insert(naga::back::spv::WriterFlags::DEBUG);
    naga::back::spv::write_vec(&module, &module_info, &spv_options, None).expect("SPIR-V generation failed")
}

fn main() {
    println!("cargo:rerun-if-changed=src/shaders/");
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let shaders = [
        ("background.vert", naga::ShaderStage::Vertex, "background.vert.spv"),
        ("background.frag", naga::ShaderStage::Fragment, "background.frag.spv"),
        ("glass_dispersive.vert", naga::ShaderStage::Vertex, "glass_dispersive.vert.spv"),
        ("glass_dispersive.frag", naga::ShaderStage::Fragment, "glass_dispersive.frag.spv"),
    ];

    for (src_file, stage, out_file) in shaders {
        let src_path = Path::new("src/shaders").join(src_file);
        let code = fs::read_to_string(&src_path).unwrap_or_else(|_| panic!("Impossible de lire {}", src_file));
        let spv = compile_glsl(&code, stage);
        let u8_bytes: Vec<u8> = spv.iter().flat_map(|w| w.to_le_bytes()).collect();
        let dest_path = Path::new(&out_dir).join(out_file);
        fs::write(&dest_path, u8_bytes).unwrap_or_else(|_| panic!("Échec de l'écriture du SPIR-V pour {}", out_file));
    }
}
