#!/usr/bin/env bash
# Build Pulse for WebAssembly and copy artifacts to web/
set -euo pipefail

PACKAGE="pulse"
OUT_DIR="web"

# lockfileの解決済みバージョンとCLIを一致させる。
BINDGEN_VERSION=$(awk '
  /^\[\[package\]\]/ { package = "" }
  /^name = / { package = $3 }
  package == "\"wasm-bindgen\"" && /^version = / {
    gsub(/"/, "", $3)
    print $3
  }
' Cargo.lock)
if [[ ! "$BINDGEN_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Could not determine a single wasm-bindgen version from Cargo.lock." >&2
  exit 1
fi

echo "=== Installing wasm32 target (if needed) ==="
rustup target add wasm32-unknown-unknown

echo "=== Installing wasm-bindgen-cli (if needed) ==="
INSTALLED_BINDGEN_VERSION=""
if command -v wasm-bindgen &>/dev/null; then
  INSTALLED_BINDGEN_VERSION=$(wasm-bindgen --version)
fi
if [[ "$INSTALLED_BINDGEN_VERSION" != "wasm-bindgen $BINDGEN_VERSION" ]]; then
  cargo install wasm-bindgen-cli --version "=$BINDGEN_VERSION" --locked
fi

echo "=== Building release WASM ==="
cargo build --locked --release --target wasm32-unknown-unknown

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
