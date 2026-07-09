#!/usr/bin/env bash
# 一键构建并同步 Plotgram Agent Demo 到 demo.plotgram.dev/agent/，WASM 走 assets.pg.agcli.cn
#
# 用法:
#   ./deploy/deploy-agent-demo.sh
#   ./deploy/deploy-agent-demo.sh --skip-build   # 跳过 wasm-pack / vite build，用已有 dist
#   ./deploy/deploy-agent-demo.sh --setup-nginx  # 同步 nginx 配置
#
# 环境变量:
#   DEPLOY_HOST       Demo 站 SSH 目标（默认 plotgram.dev）
#   ASSET_HOST        资源 CDN SSH 目标（默认 shanxun）
#   REMOTE_DIR        Demo 站部署根目录（默认 /var/www/plotgram）
#   AGENT_PATH        Demo 站 agent 子目录（默认 agent，即 $REMOTE_DIR/agent）
#   ASSET_REMOTE_DIR  CDN 部署目录（默认 /var/www/assets.pg.agcli.cn）
#   CDN_BASE          CDN 根 URL（默认 https://assets.pg.agcli.cn/）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
AGENT_DIR="$ROOT_DIR/agent-demo"

DEPLOY_HOST="${DEPLOY_HOST:-plotgram.dev}"
ASSET_HOST="${ASSET_HOST:-shanxun}"
REMOTE_DIR="${REMOTE_DIR:-/var/www/plotgram}"
AGENT_PATH="${AGENT_PATH:-agent}"
AGENT_REMOTE_DIR="$REMOTE_DIR/$AGENT_PATH"
ASSET_REMOTE_DIR="${ASSET_REMOTE_DIR:-/var/www/assets.pg.agcli.cn}"
CDN_BASE="${CDN_BASE:-https://assets.pg.agcli.cn/}"

AGENT_BASE="/agent/"
AGENT_CDN_BASE="${CDN_BASE}agent/"
STAGING_DIR=""

SKIP_BUILD=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-agent-demo.sh [选项]

构建 Agent Demo（wasm-pack + vite build），同步到 demo 站 /agent/ 与 CDN /agent/。

选项:
  --skip-build   跳过 wasm-pack 与 vite build，直接用已有 dist 同步
  --setup-nginx  同步 nginx 配置到 demo 站与 CDN
  -h, --help     显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)  SKIP_BUILD=true; shift ;;
    --setup-nginx) SETUP_NGINX=true; shift ;;
    -h|--help)     usage; exit 0 ;;
    *) echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

cleanup() {
  if [[ -n "$STAGING_DIR" && -d "$STAGING_DIR" ]]; then
    rm -rf "$STAGING_DIR"
  fi
}
trap cleanup EXIT

log() { echo "▸ $*"; }
die() { echo "✗ $*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "未找到命令: $1"
}

# ─── 构建 ───────────────────────────────────────────────
build_wasm() {
  log "构建 plotgram-wasm → agent-demo/plotgram-wasm/"
  require_cmd wasm-pack
  wasm-pack build "$ROOT_DIR/crates/plotgram-wasm" \
    --target web --release \
    --out-dir "$AGENT_DIR/plotgram-wasm"

  local wasm_bin="$AGENT_DIR/plotgram-wasm/plotgram_wasm_bg.wasm"
  [[ -f "$wasm_bin" ]] || die "WASM 产物未生成: $wasm_bin"
  local wasm_md5
  wasm_md5=$(md5 -q "$wasm_bin" 2>/dev/null || md5sum "$wasm_bin" | awk '{print $1}')
  log "plotgram-wasm md5=${wasm_md5}"
  echo "$wasm_md5"
}

build_vite() {
  local wasm_md5="$1"
  log "构建 agent-demo（base=${AGENT_BASE}, cdn=${AGENT_CDN_BASE}）"
  require_cmd npm
  (
    cd "$AGENT_DIR"
    npm ci --silent
    VITE_BASE_PATH="$AGENT_BASE" \
      VITE_CDN_BASE="$AGENT_CDN_BASE" \
      VITE_AGENT_API="https://api.pg.agcli.cn/agent/chat" \
      VITE_WASM_BUILD_STAMP="$wasm_md5" \
      npm run build
  )
}

# ─── 打包暂存 ───────────────────────────────────────────
stage_artifacts() {
  STAGING_DIR="$(mktemp -d)"
  log "打包到临时目录: $STAGING_DIR"

  mkdir -p \
    "$STAGING_DIR/demo/agent" \
    "$STAGING_DIR/cdn/agent/plotgram-wasm" \
    "$STAGING_DIR/cdn/agent/assets"

  # demo 站：agent 页面（不含 wasm / 打包 assets，走 CDN）
  rsync -a --delete \
    --exclude='plotgram-wasm/' \
    --exclude='assets/' \
    "$AGENT_DIR/dist/" \
    "$STAGING_DIR/demo/agent/"

  # CDN：wasm
  rsync -a --delete \
    "$AGENT_DIR/plotgram-wasm/" \
    "$STAGING_DIR/cdn/agent/plotgram-wasm/"

  # CDN：vite 打包 assets（js / css）
  rsync -a --delete \
    "$AGENT_DIR/dist/assets/" \
    "$STAGING_DIR/cdn/agent/assets/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync

  log "同步 demo → ${DEPLOY_HOST}:${AGENT_REMOTE_DIR} …"
  ssh "$DEPLOY_HOST" "mkdir -p '$AGENT_REMOTE_DIR'"
  rsync -avz --delete \
    "$STAGING_DIR/demo/agent/" "$DEPLOY_HOST:$AGENT_REMOTE_DIR/"

  log "同步 CDN → ${ASSET_HOST}:${ASSET_REMOTE_DIR}/agent/ …"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/agent'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn/agent/" "$ASSET_HOST:$ASSET_REMOTE_DIR/agent/"
}

# ─── nginx 配置 ─────────────────────────────────────────
setup_nginx() {
  log "同步 nginx 配置 …"
  scp "$ROOT_DIR/deploy/nginx/demo.plotgram.dev.conf" \
      "$DEPLOY_HOST:/etc/nginx/conf.d/demo.plotgram.dev.conf"
  ssh "$DEPLOY_HOST" 'nginx -t && systemctl reload nginx && echo "✅ demo 站 nginx 已重载"'
  scp "$ROOT_DIR/deploy/nginx/assets.pg.agcli.cn.conf" \
      "$ASSET_HOST:/etc/nginx/conf.d/assets.pg.agcli.cn.conf"
  ssh "$ASSET_HOST" 'nginx -t && systemctl reload nginx && echo "✅ CDN nginx 已重载"'
}

# ─── 验证 ───────────────────────────────────────────────
verify() {
  log "验证部署..."
  local code

  code=$(curl -s -o /dev/null -w '%{http_code}' "https://demo.plotgram.dev/agent/" 2>/dev/null || echo "000")
  if [[ "$code" == "200" ]]; then
    log "✅ Agent 页面: https://demo.plotgram.dev/agent/"
  else
    echo "⚠ Agent 页面返回 HTTP $code"
  fi

  code=$(curl -s -o /dev/null -w '%{http_code}' "https://assets.pg.agcli.cn/agent/plotgram-wasm/plotgram_wasm.js" 2>/dev/null || echo "000")
  if [[ "$code" == "200" ]]; then
    log "✅ CDN WASM JS: https://assets.pg.agcli.cn/agent/plotgram-wasm/plotgram_wasm.js"
  else
    echo "⚠ CDN WASM JS 返回 HTTP $code"
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
  log "=== 发布 Plotgram Agent Demo ==="
  log "  部署服务器: $DEPLOY_HOST"
  log "  访问路径:   https://demo.plotgram.dev/agent/"
  log "  CDN:        ${CDN_BASE}agent/"

  if [[ "$SETUP_NGINX" == true ]]; then
    setup_nginx
  fi

  local wasm_md5="0"
  if [[ "$SKIP_BUILD" == false ]]; then
    wasm_md5=$(build_wasm)
    build_vite "$wasm_md5"
  else
    log "跳过构建（使用已有 agent-demo/dist）"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Agent Demo: https://demo.plotgram.dev/agent/"
  echo "   CDN WASM:   ${CDN_BASE}agent/plotgram-wasm/"
  echo "   CDN assets: ${CDN_BASE}agent/assets/"
  echo "   Agent API:  https://api.pg.agcli.cn/agent/chat"
  echo ""

  verify
}

main "$@"
