//! Lapisan (lihat docs/PRD.md §6):
//! - `doc`, `md`, `store` — murni, tanpa winit/wgpu. Shell mana pun memanggil
//!   perintahnya, bukan event platform mentah.
//! - `text` — font, atlas glyph (sisi CPU) dan layout teks.
//! - `gpu`  — wgpu: surface, pipeline quad, tekstur.
//! - `app`  — shell winit dan seluruh UI.

mod app;
pub mod doc;
mod gpu;
mod icons;
pub mod md;
mod mermaid;
mod plantuml;
pub mod puml_render;
pub mod store;
mod sync;
mod text;
mod vector;
#[cfg(target_arch = "wasm32")]
mod web;

pub use app::run;
