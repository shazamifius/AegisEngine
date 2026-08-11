use aegis_engine::geometry::glb_loader::GlbLoader;

fn main() {
    if let Ok((v_closed, _)) = GlbLoader::load_glb("/home/shaza/Documents/asset/boxfermer.glb") {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &v_closed {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        println!("BOX CLOSED bounds: min={:?}, max={:?}, size=[{}, {}, {}]", 
            min, max, max[0]-min[0], max[1]-min[1], max[2]-min[2]);
    }
    if let Ok((v_open, _)) = GlbLoader::load_glb("/home/shaza/Documents/asset/box.glb") {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &v_open {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        println!("BOX OPEN bounds: min={:?}, max={:?}, size=[{}, {}, {}]", 
            min, max, max[0]-min[0], max[1]-min[1], max[2]-min[2]);
    }
}
