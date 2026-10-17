//! `cargo run --example puml -- in.puml out.png [skala]`: render diagram PlantUML ke PNG.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&a[1]).expect("baca berkas");
    let k = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    if let Err(e) = tildown::puml_render::render_png(&src, &a[2], k) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
