#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = tildown::run() {
        eprintln!("TilDown: {e}");
        std::process::exit(1);
    }
}
