#!/bin/bash

set -e

PORT=3000
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../" && pwd)"
WASM_CRATE_DIR="$ROOT_DIR/crates/tautcore-wasm"
WASM_OUT_DIR="$SCRIPT_DIR/tautcore-wasm"
WASM_BIN="$WASM_OUT_DIR/tautcore_wasm_bg.wasm"

echo "🔧 正在同步 WASM 产物..."
if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "❌ 未找到 wasm-pack，请先执行: cargo install wasm-pack"
  exit 1
fi

wasm-pack build "$WASM_CRATE_DIR" --target web --out-dir "$WASM_OUT_DIR"

if [ ! -f "$WASM_BIN" ]; then
  echo "❌ WASM 产物未生成: $WASM_BIN"
  exit 1
fi

WASM_MD5=$(md5 -q "$WASM_BIN" 2>/dev/null || md5sum "$WASM_BIN" | awk '{print $1}')
cat > "$SCRIPT_DIR/.env.local" <<EOF
# 由 start.sh 自动生成：wasm 内容 md5，用于 ?v= 缓存破坏
VITE_WASM_BUILD_STAMP=$WASM_MD5
EOF

echo "✅ WASM 产物已更新 ($(date -r "$WASM_BIN" '+%H:%M:%S' 2>/dev/null || echo "md5=$WASM_MD5"), md5=$WASM_MD5)"

# public/ 下的旧副本会劫持 /tautcore-wasm/，务必清理
if [ -d "$SCRIPT_DIR/public/tautcore-wasm" ]; then
  echo "⚠️  删除过期的 public/tautcore-wasm（Vite 会优先于源码目录提供该路径）"
  rm -rf "$SCRIPT_DIR/public/tautcore-wasm"
fi

# Vite 在 3000 被占用时会静默切到 3001，容易继续打开旧实例
for p in 3000 3001; do
  PIDS=$(lsof -ti :$p 2>/dev/null || true)
  if [ -n "$PIDS" ]; then
    echo "⚠️  端口 $p 被占用，正在关闭: $PIDS"
    kill -9 $PIDS
  fi
done

echo ""
echo "🚀 正在启动开发服务器 (http://localhost:$PORT)..."
cd "$SCRIPT_DIR"
npm run dev
