#!/bin/sh
# Build web: hasil di web/ (index.html + pkg/). Sajikan dengan server statis apa pun,
# mis. `python -m http.server -d web 8080`. Butuh: target wasm32-unknown-unknown dan
# wasm-bindgen-cli dengan versi yang sama dengan Cargo.lock.
set -e
cd "$(dirname "$0")/.."
cargo build --release --target wasm32-unknown-unknown --bin tildown
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/tildown.wasm
