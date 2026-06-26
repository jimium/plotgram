#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$EXT_DIR/../.." && pwd)"
WASM_CRATE="$ROOT_DIR/crates/drawify-wasm"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "❌ 未找到 wasm-pack，请先执行: cargo install wasm-pack"
  exit 1
fi

echo "🔧 Building Drawify WASM (web) -> editors/vscode/media/wasm"
(
  cd "$WASM_CRATE"
  wasm-pack build --target web --out-dir "$EXT_DIR/media/wasm"
)

echo "🔧 Building Drawify WASM (nodejs) -> editors/vscode/media/node"
(
  cd "$WASM_CRATE"
  wasm-pack build --target nodejs --out-dir "$EXT_DIR/media/node"
)

echo "✅ WASM 产物已写入 editors/vscode/media/{wasm,node}"
