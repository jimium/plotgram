#!/usr/bin/env bash
# 构建 plotgram-wasm 并同步到 CDN common 路径（website / playground / agent 三端共用）
#
# 产物：
#   - apps/playground/plotgram-wasm/    （playground 开发中间件源）
#   - apps/agent-demo/plotgram-wasm/    （agent-demo 开发中间件源）
#   - CDN: /plotgram-wasm/              （生产环境三端共用，靠 ETag 控制缓存）
#
# 用法:
#   ./apps/deploy/deploy-wasm.sh              # 构建 + 同步 CDN
#   ./apps/deploy/deploy-wasm.sh --skip-build # 跳过构建，用已有产物同步 CDN
#
# 说明：
#   - 本脚本不同步 nginx 配置（wasm 路由已在 assets.pg.agcli.cn.conf 中配置）。
#   - 本脚本不依赖 demo 站，只操作 CDN（shanxun）。

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_BUILD=false

usage() {
  cat <<'EOF'
用法: apps/deploy/deploy-wasm.sh [选项]

构建 plotgram-wasm（wasm-pack），同步到 playground / agent-demo 本地目录与 CDN common 路径。

选项:
  --skip-build   跳过 wasm-pack 构建，用已有 apps/playground/plotgram-wasm/ 同步
  -h, --help     显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=true; shift ;;
    -h|--help)    usage; exit 0 ;;
    *)            echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

trap cleanup_staging EXIT
trap 'close_ssh_multiplexing "$ASSET_HOST"' EXIT

WASM_CRATE_DIR="$ROOT_DIR/crates/plotgram-wasm"
PLAYGROUND_WASM_OUT="$ROOT_DIR/apps/playground/plotgram-wasm"
AGENT_DEMO_WASM_OUT="$ROOT_DIR/apps/agent-demo/plotgram-wasm"
CDN_WASM_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/plotgram-wasm/"

# ─── 构建 ───────────────────────────────────────────────
build_wasm() {
  log "构建 plotgram-wasm → apps/playground/plotgram-wasm/"
  require_cmd wasm-pack
  wasm-pack build "$WASM_CRATE_DIR" \
    --target web --release \
    --out-dir "$PLAYGROUND_WASM_OUT"

  local wasm_bin="$PLAYGROUND_WASM_OUT/plotgram_wasm_bg.wasm"
  [[ -f "$wasm_bin" ]] || die "WASM 产物未生成: $wasm_bin"

  # 同步一份到 agent-demo 开发目录
  log "同步 wasm 副本 → apps/agent-demo/plotgram-wasm/"
  rsync -a --delete \
    "$PLAYGROUND_WASM_OUT/" "$AGENT_DEMO_WASM_OUT/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync
  log "同步 wasm → CDN $CDN_WASM_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/plotgram-wasm'"
  rsync_to "$PLAYGROUND_WASM_OUT/" "$CDN_WASM_REMOTE"
}

# ─── 验证 ───────────────────────────────────────────────
verify() {
  log "验证..."
  local code
  code=$(curl -s -o /dev/null -w '%{http_code}' "${CDN_BASE}plotgram-wasm/plotgram_wasm.js" 2>/dev/null || echo "000")
  if [[ "$code" == "200" ]]; then
    log "✅ CDN WASM: ${CDN_BASE}plotgram-wasm/plotgram_wasm.js"
  else
    echo "⚠ CDN WASM 返回 HTTP $code"
  fi
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 plotgram-wasm（CDN common）==="
  log "  CDN: ${CDN_BASE}plotgram-wasm/"

  setup_ssh_multiplexing "$ASSET_HOST"

  if [[ "$SKIP_BUILD" == false ]]; then
    build_wasm
  else
    log "跳过构建（使用已有 apps/playground/plotgram-wasm/）"
    [[ -f "$PLAYGROUND_WASM_OUT/plotgram_wasm_bg.wasm" ]] \
      || die "缺少 wasm 产物，请先去掉 --skip-build 运行一次"
  fi

  upload

  echo ""
  echo "✅ 发布完成"
  echo "   CDN WASM: ${CDN_BASE}plotgram-wasm/（三端共用，ETag 控制缓存）"
  echo "   本地副本: apps/playground/plotgram-wasm/、apps/agent-demo/plotgram-wasm/"
  echo ""

  verify
}

main "$@"
