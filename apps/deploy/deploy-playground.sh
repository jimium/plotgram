#!/usr/bin/env bash
# 构建并发布 Plotgram Editor（代码目录 apps/playground/）到 plotgram.cn/editor/
#
# 产物：
#   - 主站:  /var/www/plotgram.cn/editor/  （HTML、favicon、logo，不含 assets/、plotgram-wasm/）
#   - CDN:   /editor/assets/                （打包 js / css）
#
# 兼容：nginx 将 /playground/ 301 重定向到 /editor/
#
# 前置条件：apps/playground/plotgram-wasm/ 必须存在（由 deploy-wasm.sh 构建）。
#
# 用法:
#   ./apps/deploy/deploy-playground.sh              # 构建 + 同步
#   ./apps/deploy/deploy-playground.sh --skip-build # 跳过构建，用已有 dist 同步
#   ./apps/deploy/deploy-playground.sh --setup-nginx # 同步 nginx 配置

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_BUILD=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: apps/deploy/deploy-playground.sh [选项]

构建 Editor（apps/playground/）并同步到 plotgram.cn/editor/ 与 CDN。

前置：需先运行 ./apps/deploy/deploy-wasm.sh 生成 apps/playground/plotgram-wasm/。

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

PLAYGROUND_DIR="$ROOT_DIR/apps/playground"
EDITOR_BASE="/editor/"
EDITOR_CDN_BASE="${CDN_BASE}editor/"
EDITOR_REMOTE="$DEPLOY_HOST:$REMOTE_DIR/editor/"
CDN_EDITOR_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/editor/"

# ─── 构建 ───────────────────────────────────────────────
build() {
  if [[ ! -f "$PLAYGROUND_DIR/plotgram-wasm/plotgram_wasm_bg.wasm" ]]; then
    die "缺少 apps/playground/plotgram-wasm/，请先运行 ./apps/deploy/deploy-wasm.sh"
  fi

  log "构建 Editor (base=${EDITOR_BASE}, cdn=${EDITOR_CDN_BASE})"
  require_cmd npm
  (
    cd "$PLAYGROUND_DIR"
    npm ci --silent
    VITE_BASE_PATH="$EDITOR_BASE" \
      VITE_CDN_BASE="$EDITOR_CDN_BASE" \
      npm run build
  )
}

# ─── 打包暂存 ───────────────────────────────────────────
stage_artifacts() {
  STAGING_DIR="$(new_staging_dir)"
  mkdir -p "$STAGING_DIR/editor" "$STAGING_DIR/cdn-editor"

  rsync -a --delete \
    --exclude='plotgram-wasm/' \
    --exclude='assets/' \
    "$PLAYGROUND_DIR/dist/" "$STAGING_DIR/editor/"

  log "注入 ICP 备案号 → editor/index.html"
  inject_icp_badge "$STAGING_DIR/editor/index.html"

  mkdir -p "$STAGING_DIR/cdn-editor/assets"
  rsync -a --delete \
    "$PLAYGROUND_DIR/dist/assets/" "$STAGING_DIR/cdn-editor/assets/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync
  log "同步 editor → plotgram.cn"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR/editor'"
  rsync -avz --delete \
    "$STAGING_DIR/editor/" "$EDITOR_REMOTE"

  log "同步 CDN → $CDN_EDITOR_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/editor'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn-editor/" "$CDN_EDITOR_REMOTE"
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 Plotgram Editor ==="
  log "  访问地址: https://www.plotgram.cn/editor/"

  setup_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"

  if [[ "$SETUP_NGINX" == true ]]; then
    sync_nginx "$DEPLOY_HOST" nginx/plotgram.cn.conf
    sync_nginx "$ASSET_HOST" nginx/assets.pg.agcli.cn.conf
  fi

  if [[ "$SKIP_BUILD" == false ]]; then
    build
  else
    log "跳过构建（使用已有 apps/playground/dist）"
    [[ -d "$PLAYGROUND_DIR/dist" ]] || die "缺少 apps/playground/dist，请先去掉 --skip-build 运行一次"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Editor: https://www.plotgram.cn/editor/"
  echo "   CDN:    ${CDN_BASE}editor/assets/"
  echo "   兼容:   /playground/ → /editor/ (nginx 重定向)"
  echo ""
}

main "$@"
