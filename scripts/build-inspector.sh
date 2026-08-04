#!/usr/bin/env bash
# Build the Layout Debug Inspector wasm module into apps/inspector/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-pack.
set -euo pipefail
cd "$(dirname "$0")/.."
wasm-pack build crates/plotgram-wasm --target web \
  --out-dir ../../apps/inspector/pkg --release
echo "done. serve with: python3 -m http.server -d apps/inspector 8090"
