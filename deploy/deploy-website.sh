#!/usr/bin/env bash
# 构建并发布 website（landing page）到 demo.plotgram.dev 根路径
#
# 产物：
#   - demo 站根目录:  /var/www/plotgram/  （index.html + assets/brand/）
#   - CDN:            /website/assets/    （打包 js / css）
#
# 重要：根目录 rsync 使用 --delete，但必须排除 playground/ showcase/ agent/ 等其他站点子目录，
#       否则会误删其他站点（历史上 agent/ 曾因此被清空）。
#
# 用法:
#   ./deploy/deploy-website.sh              # 构建 + 同步
#   ./deploy/deploy-website.sh --skip-build # 跳过构建，用已有 dist 同步
#   ./deploy/deploy-website.sh --setup-nginx # 同步 nginx 配置

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_BUILD=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-website.sh [选项]

构建 website（landing page）并同步到 demo 站根路径与 CDN。

选项:
  --skip-build    跳过 vite build，用已有 dist 同步
  --setup-nginx   同步 nginx 配置（demo 站 + CDN）
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

WEBSITE_DIR="$ROOT_DIR/website"
WEBSITE_CDN_BASE="${CDN_BASE}website/"
ROOT_REMOTE="$DEPLOY_HOST:$REMOTE_DIR/"
ROOT_MIRROR_REMOTE="$SITE_MIRROR_HOST:$SITE_MIRROR_DIR/"
CDN_WEBSITE_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/website/"

# ─── 构建 ───────────────────────────────────────────────
build() {
  log "构建 website (cdn=${WEBSITE_CDN_BASE})"
  require_cmd npm
  (
    cd "$WEBSITE_DIR"
    npm ci --silent
    VITE_CDN_BASE="$WEBSITE_CDN_BASE" npm run build
  )
}

# ─── 打包暂存 ───────────────────────────────────────────
# ICP 备案号注入逻辑见 lib/common.sh 的 inject_icp_badge（仅 plotgram.cn 镜像站）。
stage_artifacts() {
  STAGING_DIR="$(new_staging_dir)"
  mkdir -p "$STAGING_DIR/demo-root" "$STAGING_DIR/demo-root-cn" "$STAGING_DIR/cdn-website"

  # 根目录：website dist（不含打包 assets，走 CDN）
  rsync -a --delete \
    --exclude='assets/' \
    "$WEBSITE_DIR/dist/" "$STAGING_DIR/demo-root/"

  # 根目录：brand 资源（不走 CDN，直接放 demo 站 /assets/brand/）
  mkdir -p "$STAGING_DIR/demo-root/assets/brand"
  rsync -a "$ROOT_DIR/assets/brand/" "$STAGING_DIR/demo-root/assets/brand/"

  # 根目录：其他静态资源（svg 等，不走 CDN）
  rsync -a --include='*.svg' --exclude='*' \
    "$WEBSITE_DIR/dist/assets/" "$STAGING_DIR/demo-root/assets/"

  # 镜像站副本（demo-root-cn）：复制主站版本后注入 ICP 备案号
  rsync -a "$STAGING_DIR/demo-root/" "$STAGING_DIR/demo-root-cn/"
  if [[ "$MIRROR_ENABLED" == "true" ]]; then
    log "注入 ICP 备案号 → plotgram.cn index.html"
    inject_icp_badge "$STAGING_DIR/demo-root-cn/index.html"
  fi

  # CDN：website 打包 assets（js / css），保留 assets/ 子目录层级
  mkdir -p "$STAGING_DIR/cdn-website/assets"
  rsync -a --delete \
    "$WEBSITE_DIR/dist/assets/" "$STAGING_DIR/cdn-website/assets/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync
  # 主站（plotgram.dev）：不含 ICP badge
  log "同步 demo 根目录 → 主站 (plotgram.dev)"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR'"
  rsync -avz --delete \
    --exclude='playground/' \
    --exclude='showcase/' \
    --exclude='agent/' \
    "$STAGING_DIR/demo-root/" "$ROOT_REMOTE"

  # 镜像站（plotgram.cn）：含静态 ICP badge
  if [[ "$MIRROR_ENABLED" == "true" ]]; then
    log "同步 demo 根目录 → 镜像站 (plotgram.cn，含 ICP badge)"
    ssh "$SITE_MIRROR_HOST" "mkdir -p '$SITE_MIRROR_DIR'"
    rsync -avz --delete \
      --exclude='playground/' \
      --exclude='showcase/' \
      --exclude='agent/' \
      "$STAGING_DIR/demo-root-cn/" "$ROOT_MIRROR_REMOTE"
  else
    log "跳过镜像同步（MIRROR_ENABLED=false）"
  fi

  log "同步 CDN → $CDN_WEBSITE_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/website'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn-website/" "$CDN_WEBSITE_REMOTE"
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 website（landing page）==="
  log "  访问地址: https://demo.plotgram.dev/"

  setup_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"

  if [[ "$SETUP_NGINX" == true ]]; then
    sync_nginx "$DEPLOY_HOST" nginx/demo.plotgram.dev.conf
    sync_nginx "$ASSET_HOST" nginx/assets.pg.agcli.cn.conf nginx/plotgram.cn.conf
  fi

  if [[ "$SKIP_BUILD" == false ]]; then
    build
  else
    log "跳过构建（使用已有 website/dist）"
    [[ -d "$WEBSITE_DIR/dist" ]] || die "缺少 website/dist，请先去掉 --skip-build 运行一次"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Website (plotgram.dev): https://demo.plotgram.dev/"
  echo "   Website (plotgram.cn):  https://www.plotgram.cn/"
  echo "   CDN:                    ${CDN_BASE}website/assets/"
  echo ""
}

main "$@"
