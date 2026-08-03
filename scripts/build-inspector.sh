#!/usr/bin/env bash
# Build the debug inspector wasm module into showcase/debug/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-pack.
set -euo pipefail
cd "$(dirname "$0")/.."
wasm-pack build crates/plotgram-wasm --target web \
  --out-dir ../../showcase/debug/pkg --release
echo "done. serve with: python3 -m http.server -d showcase/debug 8090"
