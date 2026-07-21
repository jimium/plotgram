#!/usr/bin/env bash
# 构建并发布 playground 到 plotgram.cn/playground/
#
# 产物：
#   - 主站:  /var/www/plotgram.cn/playground/  （HTML、favicon、logo，不含 assets/、plotgram-wasm/）
#   - CDN:   /playground/assets/                （打包 js / css）
#
# 前置条件：playground/plotgram-wasm/ 必须存在（由 deploy-wasm.sh 构建）。
# 本脚本不构建 wasm，只负责 playground 自身的 vite build 与同步。
#
# 用法:
#   ./deploy/deploy-playground.sh              # 构建 + 同步
#   ./deploy/deploy-playground.sh --skip-build # 跳过构建，用已有 dist 同步
#   ./deploy/deploy-playground.sh --setup-nginx # 同步 nginx 配置

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_BUILD=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-playground.sh [选项]

构建 playground 并同步到 plotgram.cn/playground/ 与 CDN。

前置：需先运行 ./deploy/deploy-wasm.sh 生成 playground/plotgram-wasm/。

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

PLAYGROUND_DIR="$ROOT_DIR/playground"
PLAYGROUND_BASE="/playground/"
PLAYGROUND_CDN_BASE="${CDN_BASE}playground/"
PLAYGROUND_REMOTE="$DEPLOY_HOST:$REMOTE_DIR/playground/"
CDN_PLAYGROUND_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/playground/"

# ─── 构建 ───────────────────────────────────────────────
build() {
  # vite build 依赖 plotgram-wasm/（sync-wasm-dist.mjs 会复制到 dist）
  if [[ ! -f "$PLAYGROUND_DIR/plotgram-wasm/plotgram_wasm_bg.wasm" ]]; then
    die "缺少 playground/plotgram-wasm/，请先运行 ./deploy/deploy-wasm.sh"
  fi

  log "构建 playground (base=${PLAYGROUND_BASE}, cdn=${PLAYGROUND_CDN_BASE})"
  require_cmd npm
  (
    cd "$PLAYGROUND_DIR"
    npm ci --silent
    VITE_BASE_PATH="$PLAYGROUND_BASE" \
      VITE_CDN_BASE="$PLAYGROUND_CDN_BASE" \
      npm run build
  )
}

# ─── 打包暂存 ───────────────────────────────────────────
stage_artifacts() {
  STAGING_DIR="$(new_staging_dir)"
  mkdir -p "$STAGING_DIR/playground" "$STAGING_DIR/cdn-playground"

  # 主站：playground 不含 wasm / 打包 assets（走 CDN）
  rsync -a --delete \
    --exclude='plotgram-wasm/' \
    --exclude='assets/' \
    "$PLAYGROUND_DIR/dist/" "$STAGING_DIR/playground/"

  # 注入 ICP 备案号
  log "注入 ICP 备案号 → playground/index.html"
  inject_icp_badge "$STAGING_DIR/playground/index.html"

  # CDN：playground 打包 assets（js / css），保留 assets/ 子目录层级
  mkdir -p "$STAGING_DIR/cdn-playground/assets"
  rsync -a --delete \
    "$PLAYGROUND_DIR/dist/assets/" "$STAGING_DIR/cdn-playground/assets/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync
  # 主站（plotgram.cn）
  log "同步 playground → plotgram.cn"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR/playground'"
  rsync -avz --delete \
    "$STAGING_DIR/playground/" "$PLAYGROUND_REMOTE"

  log "同步 CDN → $CDN_PLAYGROUND_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/playground'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn-playground/" "$CDN_PLAYGROUND_REMOTE"
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 playground ==="
  log "  访问地址: https://www.plotgram.cn/playground/"

  setup_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"

  if [[ "$SETUP_NGINX" == true ]]; then
    sync_nginx "$DEPLOY_HOST" nginx/plotgram.cn.conf
    sync_nginx "$ASSET_HOST" nginx/assets.pg.agcli.cn.conf
  fi

  if [[ "$SKIP_BUILD" == false ]]; then
    build
  else
    log "跳过构建（使用已有 playground/dist）"
    [[ -d "$PLAYGROUND_DIR/dist" ]] || die "缺少 playground/dist，请先去掉 --skip-build 运行一次"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Playground: https://www.plotgram.cn/playground/"
  echo "   CDN:        ${CDN_BASE}playground/assets/"
  echo ""
}

main "$@"
