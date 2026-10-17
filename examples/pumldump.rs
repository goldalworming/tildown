//! `cargo run --example pumldump -- in.puml`: cetak primitif (debug).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&a[1]).unwrap();
    tildown::puml_render::dump(&src);
}
