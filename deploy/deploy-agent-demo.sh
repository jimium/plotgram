#!/usr/bin/env bash
# 构建并发布 Agent Demo 到 plotgram.cn/agent/
#
# 产物：
#   - 主站:  /var/www/plotgram.cn/agent/  （index.html、logo，不含 assets/、plotgram-wasm/）
#   - CDN:   /agent/assets/                （打包 js / css）
#
# 前置条件：agent-demo/plotgram-wasm/ 必须存在（由 deploy-wasm.sh 构建）。
# 本脚本不构建 wasm，只负责 agent-demo 自身的 vite build 与同步。
#
# 用法:
#   ./deploy/deploy-agent-demo.sh              # 构建 + 同步
#   ./deploy/deploy-agent-demo.sh --skip-build # 跳过构建，用已有 dist 同步
#   ./deploy/deploy-agent-demo.sh --setup-nginx # 同步 nginx 配置

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_BUILD=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-agent-demo.sh [选项]

构建 Agent Demo（vite build）并同步到 plotgram.cn/agent/ 与 CDN。

前置：需先运行 ./deploy/deploy-wasm.sh 生成 agent-demo/plotgram-wasm/。

选项:
  --skip-build    跳过 vite build，用已有 dist 同步
  --setup-nginx   同步 nginx 配置
  -h, --help      显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)   SKIP_BUILD=true; shift ;;
    --setup-nginx)  SETUP_NGINX=true; shift ;;
    -h|--help)      usage; exit 0 ;;
    *)              echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

trap cleanup_staging EXIT
trap 'close_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"' EXIT

AGENT_DIR="$ROOT_DIR/agent-demo"
AGENT_BASE="/agent/"
AGENT_CDN_BASE="${CDN_BASE}agent/"
AGENT_API="https://api.pg.agcli.cn/agent/chat"
AGENT_REMOTE="$DEPLOY_HOST:$REMOTE_DIR/agent/"
CDN_AGENT_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/agent/"

# ─── 构建 ───────────────────────────────────────────────
build() {
  # vite build 时 alias 需要 plotgram-wasm/ 存在（开发中间件源，生产走 CDN）
  if [[ ! -f "$AGENT_DIR/plotgram-wasm/plotgram_wasm_bg.wasm" ]]; then
    die "缺少 agent-demo/plotgram-wasm/，请先运行 ./deploy/deploy-wasm.sh"
  fi

  log "构建 agent-demo (base=${AGENT_BASE}, cdn=${AGENT_CDN_BASE})"
  require_cmd npm
  (
    cd "$AGENT_DIR"
    npm ci --silent
    VITE_BASE_PATH="$AGENT_BASE" \
      VITE_CDN_BASE="$AGENT_CDN_BASE" \
      VITE_AGENT_API="$AGENT_API" \
      npm run build
  )
}

# ─── 打包暂存 ───────────────────────────────────────────
stage_artifacts() {
  STAGING_DIR="$(new_staging_dir)"
  mkdir -p "$STAGING_DIR/agent" "$STAGING_DIR/cdn-agent"

  # 主站：agent 页面（不含 wasm / 打包 assets，走 CDN）
  rsync -a --delete \
    --exclude='plotgram-wasm/' \
    --exclude='assets/' \
    "$AGENT_DIR/dist/" "$STAGING_DIR/agent/"

  # 注入 ICP 备案号
  log "注入 ICP 备案号 → agent/index.html"
  inject_icp_badge "$STAGING_DIR/agent/index.html"

  # CDN：agent 打包 assets（js / css），保留 assets/ 子目录层级
  mkdir -p "$STAGING_DIR/cdn-agent/assets"
  rsync -a --delete \
    "$AGENT_DIR/dist/assets/" "$STAGING_DIR/cdn-agent/assets/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync
  # 主站（plotgram.cn）
  log "同步 agent → plotgram.cn"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR/agent'"
  rsync -avz --delete \
    "$STAGING_DIR/agent/" "$AGENT_REMOTE"

  log "同步 CDN → $CDN_AGENT_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/agent'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn-agent/" "$CDN_AGENT_REMOTE"
}

# ─── 验证 ───────────────────────────────────────────────
verify() {
  log "验证..."
  local code

  code=$(curl -s -o /dev/null -w '%{http_code}' "https://www.plotgram.cn/agent/" 2>/dev/null || echo "000")
  if [[ "$code" == "200" ]]; then
    log "✅ Agent 页面: https://www.plotgram.cn/agent/"
  else
    echo "⚠ Agent 页面返回 HTTP $code"
  fi

  code=$(curl -s -o /dev/null -w '%{http_code}' "https://api.pg.agcli.cn/health" 2>/dev/null || echo "000")
  if [[ "$code" == "200" ]]; then
    log "✅ Agent API: https://api.pg.agcli.cn/health"
  else
    echo "⚠ Agent API 返回 HTTP $code（需先运行 deploy-agent-api.sh）"
  fi
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 Agent Demo ==="
  log "  访问路径: https://www.plotgram.cn/agent/"
  log "  CDN wasm: ${CDN_BASE}plotgram-wasm/（common，由 deploy-wasm.sh 维护）"

  setup_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"

  if [[ "$SETUP_NGINX" == true ]]; then
    sync_nginx "$DEPLOY_HOST" nginx/plotgram.cn.conf
    sync_nginx "$ASSET_HOST" nginx/assets.pg.agcli.cn.conf
  fi

  if [[ "$SKIP_BUILD" == false ]]; then
    build
  else
    log "跳过构建（使用已有 agent-demo/dist）"
    [[ -d "$AGENT_DIR/dist" ]] || die "缺少 agent-demo/dist，请先去掉 --skip-build 运行一次"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Agent Demo: https://www.plotgram.cn/agent/"
  echo "   CDN assets: ${CDN_BASE}agent/assets/"
  echo "   Agent API:  $AGENT_API"
  echo "   CDN WASM:   ${CDN_BASE}plotgram-wasm/（由 deploy-wasm.sh 维护）"
  echo ""

  verify
}

main "$@"
