#!/bin/bash

set -e

PORT=3000
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WASM_CRATE_DIR="$ROOT_DIR/crates/drawify-wasm"

echo "🔧 正在同步 WASM 产物..."
if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "❌ 未找到 wasm-pack，请先执行: cargo install wasm-pack"
  exit 1
fi

(
  cd "$WASM_CRATE_DIR"
  wasm-pack build --target web --out-dir ../../playground/drawify-wasm
)

echo "✅ WASM 产物已更新"

# 检查端口是否被占用
PID=$(lsof -ti :$PORT 2>/dev/null || true)

if [ -n "$PID" ]; then
  echo "⚠️  端口 $PORT 被进程 $PID 占用，正在强制关闭..."
  kill -9 $PID
  echo "✅ 已关闭进程 $PID"
else
  echo "✅ 端口 $PORT 未被占用"
fi

echo ""
echo "🚀 正在启动开发服务器..."
cd "$SCRIPT_DIR"
npm run dev
