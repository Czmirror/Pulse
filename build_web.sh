#!/usr/bin/env bash
# Build Pulse for WebAssembly and copy artifacts to web/
set -euo pipefail

PACKAGE="pulse"
OUT_DIR="web"

echo "=== Installing wasm32 target (if needed) ==="
rustup target add wasm32-unknown-unknown

echo "=== Installing wasm-bindgen-cli (if needed) ==="
if ! command -v wasm-bindgen &>/dev/null; then
  cargo install wasm-bindgen-cli --locked
fi

echo "=== Building release WASM ==="
cargo build --release --target wasm32-unknown-unknown

WASM_FILE="target/wasm32-unknown-unknown/release/${PACKAGE}.wasm"

echo "=== Running wasm-bindgen ==="
wasm-bindgen \
  --out-dir "${OUT_DIR}" \
  --target web \
  --no-typescript \
  "${WASM_FILE}"

echo ""
echo "Build complete!  Serve the '${OUT_DIR}/' directory with any static file server."
echo "Example:"
echo "  python3 -m http.server 8080 --directory ${OUT_DIR}"
