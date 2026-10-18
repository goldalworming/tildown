#!/bin/sh
# Build web: hasil di web/ (index.html + pkg/). Sajikan dengan server statis apa pun,
# mis. `python -m http.server -d web 8080`. Butuh: target wasm32-unknown-unknown dan
# wasm-bindgen-cli dengan versi yang sama dengan Cargo.lock.
set -e
cd "$(dirname "$0")/.."
# Font Cina (Noto Sans SC, OFL): tidak masuk wasm, diunduh aplikasi saat dibutuhkan.
[ -f web/fonts/NotoSansSC-Regular.otf ] || curl -fsSL -o web/fonts/NotoSansSC-Regular.otf \
  https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf
cargo build --release --target wasm32-unknown-unknown --bin tildown
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/tildown.wasm
