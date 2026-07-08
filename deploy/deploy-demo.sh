#!/usr/bin/env bash
# 一键构建并同步 website(landing) + playground + showcase 到 demo.plotgram.dev，大资源走 assets.plotgram.cn
#
# 用法:
#   ./deploy/deploy-demo.sh
#   ./deploy/deploy-demo.sh --skip-showcase-render
#
# 环境变量:
#   DEPLOY_HOST       demo 站 SSH 目标（默认 plotgram.dev）
#   ASSET_HOST        资源 CDN SSH 目标（默认 shanxun）
#   REMOTE_DIR        demo 站部署目录（默认 /var/www/plotgram）
#   ASSET_REMOTE_DIR  CDN 部署目录（默认 /var/www/assets.plotgram.cn）
#   CDN_BASE          CDN 根 URL（默认 https://assets.plotgram.cn/）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

DEPLOY_HOST="${DEPLOY_HOST:-plotgram.dev}"
ASSET_HOST="${ASSET_HOST:-shanxun}"
REMOTE_DIR="${REMOTE_DIR:-/var/www/plotgram}"
ASSET_REMOTE_DIR="${ASSET_REMOTE_DIR:-/var/www/assets.plotgram.cn}"
CDN_BASE="${CDN_BASE:-https://assets.plotgram.cn/}"
DOMAIN="demo.plotgram.dev"
PLAYGROUND_BASE="/playground/"
PLAYGROUND_CDN_BASE="${CDN_BASE}playground/"
STAGING_DIR=""

SKIP_SHOWCASE_RENDER=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-demo.sh [选项]

构建 website（landing page）+ playground（含 WASM）与 showcase，同步到 demo 站与 assets CDN。

选项:
  --skip-showcase-render  跳过 showcase SVG 渲染（使用已有产物）
  -h, --help              显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-showcase-render) SKIP_SHOWCASE_RENDER=true; shift ;;
    -h|--help) usage; exit 0 ;;
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

patch_showcase_cdn() {
  local index_html="$1"
  python3 - "$CDN_BASE" "$index_html" <<'PY'
import re
import sys
from pathlib import Path

cdn_base, path = sys.argv[1], sys.argv[2]
text = Path(path).read_text(encoding="utf-8")
pattern = re.compile(
    r'(// cdn:start\s*\n\s*const CDN_BASE = )"[^"]*"(;\s*\n\s*// cdn:end)',
    re.MULTILINE,
)
updated, count = pattern.subn(rf'\1"{cdn_base}"\2', text, count=1)
if count != 1:
    raise SystemExit("showcase/index.html: 未找到 CDN 标记")
Path(path).write_text(updated, encoding="utf-8")
PY
}

patch_build_hash() {
  local index_html="$1"
  local build_hash

  build_hash=$(cd "$ROOT_DIR/showcase" && find . -name '*.svg' -type f -not -path '*/.history/*' | sort | xargs cat | shasum -a 256 | cut -d' ' -f1 | head -c 8)

  if [[ -z "$build_hash" ]]; then
    die "无法计算 BUILD_HASH（showcase 目录下没有 SVG 文件？）"
  fi

  log "BUILD_HASH=${build_hash}"
  sed -i '' "s/{{BUILD_HASH}}/${build_hash}/g" "$index_html"
}

build_website() {
  log "构建 website (Landing Page)…"
  require_cmd npm
  (
    cd "$ROOT_DIR/website"
    npm ci --silent
    npm run build
  )
}

build_playground() {
  log "构建 plotgram-wasm…"
  require_cmd wasm-pack
  local wasm_out="$ROOT_DIR/playground/plotgram-wasm"
  wasm-pack build "$ROOT_DIR/crates/plotgram-wasm" --target web --release --out-dir "$wasm_out"

  local wasm_bin="$wasm_out/plotgram_wasm_bg.wasm"
  [[ -f "$wasm_bin" ]] || die "WASM 产物未生成: $wasm_bin"
  local wasm_md5
  wasm_md5=$(md5 -q "$wasm_bin" 2>/dev/null || md5sum "$wasm_bin" | awk '{print $1}')
  log "plotgram-wasm md5=${wasm_md5}"

  if [[ -d "$ROOT_DIR/playground/public/plotgram-wasm" ]]; then
    log "删除过期的 public/plotgram-wasm（避免 vite build 写入 dist 旧副本）"
    rm -rf "$ROOT_DIR/playground/public/plotgram-wasm"
  fi

  log "构建 playground（base=${PLAYGROUND_BASE}, cdn=${PLAYGROUND_CDN_BASE}）…"
  require_cmd npm
  (
    cd "$ROOT_DIR/playground"
    npm ci --silent
    VITE_BASE_PATH="$PLAYGROUND_BASE" \
      VITE_CDN_BASE="$PLAYGROUND_CDN_BASE" \
      VITE_WASM_BUILD_STAMP="$wasm_md5" \
      npm run build
  )
}

build_showcase() {
  if [[ "$SKIP_SHOWCASE_RENDER" == true ]]; then
    log "跳过 showcase SVG 渲染"
    return
  fi

  log "编译 plotgram CLI…"
  require_cmd cargo
  (
    cd "$ROOT_DIR"
    cargo build --release -q
  )

  log "渲染 showcase SVG…"
  PLOTGRAM_PROFILE=release "$ROOT_DIR/showcase/render-all.sh"
}

stage_artifacts() {
  STAGING_DIR="$(mktemp -d)"
  log "打包到临时目录: $STAGING_DIR"

  mkdir -p \
    "$STAGING_DIR/demo/root" \
    "$STAGING_DIR/demo/playground" \
    "$STAGING_DIR/demo/showcase" \
    "$STAGING_DIR/demo/assets" \
    "$STAGING_DIR/cdn/playground/plotgram-wasm" \
    "$STAGING_DIR/cdn/showcase"

  # demo 根目录：website (Landing Page)
  rsync -a --delete \
    "$ROOT_DIR/website/dist/" \
    "$STAGING_DIR/demo/root/"

  # demo：brand 资源
  mkdir -p "$STAGING_DIR/demo/root/assets/brand"
  rsync -a "$ROOT_DIR/assets/brand/" "$STAGING_DIR/demo/root/assets/brand/"

  # demo：playground 不含 wasm / 打包 assets（走 CDN）
  rsync -a --delete \
    --exclude='plotgram-wasm/' \
    --exclude='assets/' \
    "$ROOT_DIR/playground/dist/" \
    "$STAGING_DIR/demo/playground/"

  # demo：showcase 不含 svg / 历史快照（走 CDN）
  rsync -a \
    --exclude='*.svg' \
    --include='.history/' \
    --include='.history/manifest.json' \
    --exclude='.history/**' \
    --exclude='*.py' \
    --exclude='*.sh' \
    --exclude='test.md' \
    --exclude='README.md' \
    --exclude='.gitignore' \
    "$ROOT_DIR/showcase/" \
    "$STAGING_DIR/demo/showcase/"

  patch_showcase_cdn "$STAGING_DIR/demo/showcase/index.html"

  # CDN：wasm
  rsync -a --delete \
    "$ROOT_DIR/playground/plotgram-wasm/" \
    "$STAGING_DIR/cdn/playground/plotgram-wasm/"

  # CDN：playground 打包 assets（js / css）
  rsync -a --delete \
    "$ROOT_DIR/playground/dist/assets/" \
    "$STAGING_DIR/cdn/playground/assets/"

  # CDN：showcase svg + 历史快照
  rsync -a \
    --include='*/' \
    --include='*.svg' \
    --exclude='*' \
    "$ROOT_DIR/showcase/" \
    "$STAGING_DIR/cdn/showcase/"
}

upload() {
  require_cmd rsync

  log "同步 demo → ${DEPLOY_HOST}:${REMOTE_DIR} …"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR'"

  # 根目录（landing page + assets），不删除已有的 playground/ 和 showcase/
  rsync -avz --delete \
    --exclude='playground/' \
    --exclude='showcase/' \
    "$STAGING_DIR/demo/root/" "$DEPLOY_HOST:$REMOTE_DIR/"

  rsync -avz --delete \
    "$STAGING_DIR/demo/playground/" "$DEPLOY_HOST:$REMOTE_DIR/playground/"
  rsync -avz --delete \
    "$STAGING_DIR/demo/showcase/" "$DEPLOY_HOST:$REMOTE_DIR/showcase/"

  log "同步 CDN → ${ASSET_HOST}:${ASSET_REMOTE_DIR} …"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn/playground/" "$ASSET_HOST:$ASSET_REMOTE_DIR/playground/"
  rsync -avz --delete \
    "$STAGING_DIR/cdn/showcase/" "$ASSET_HOST:$ASSET_REMOTE_DIR/showcase/"

  log "同步 nginx 配置 …"
  scp "$ROOT_DIR/deploy/nginx/demo.plotgram.dev.conf" "$DEPLOY_HOST:/etc/nginx/conf.d/demo.plotgram.dev.conf"
  ssh "$DEPLOY_HOST" 'nginx -t && systemctl reload nginx'
  scp "$ROOT_DIR/deploy/nginx/assets.plotgram.cn.conf" "$ASSET_HOST:/etc/nginx/conf.d/assets.plotgram.cn.conf"
  ssh "$ASSET_HOST" 'nginx -t && systemctl reload nginx'
}

main() {
  log "开始同步 → https://${DOMAIN}（CDN: ${CDN_BASE}）"
  build_showcase
  build_website
  build_playground
  stage_artifacts
  patch_build_hash "$STAGING_DIR/demo/showcase/index.html"
  upload

  echo ""
  echo "✅ 同步完成"
  echo "   Website:    https://${DOMAIN}/"
  echo "   Playground: https://${DOMAIN}/playground/"
  echo "   Showcase:   https://${DOMAIN}/showcase/"
  echo "   CDN:        ${CDN_BASE}"
}

main "$@"
