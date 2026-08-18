#!/usr/bin/env bash
# 构建并发布 showcase 到 plotgram.cn/showcase/
#
# 产物：
#   - 主站:  /var/www/plotgram.cn/showcase/  （页面、.taut、manifest，不含 .svg）
#   - CDN:   /showcase/                       （SVG 文件）
#
# 流程：cargo build tautcore-cli → 渲染 SVG → patch CDN/BUILD_HASH → 同步
#
# 用法:
#   ./apps/deploy/deploy-showcase.sh                  # 渲染 SVG + 同步
#   ./apps/deploy/deploy-showcase.sh --skip-render    # 跳过 SVG 渲染，用已有 SVG 同步
#   ./apps/deploy/deploy-showcase.sh --setup-nginx     # 同步 nginx 配置

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_RENDER=false
SETUP_NGINX=false

usage() {
  cat <<'EOF'
用法: apps/deploy/deploy-showcase.sh [选项]

渲染 showcase SVG（cargo build + render.sh）并同步到 plotgram.cn 与 CDN。

选项:
  --skip-render   跳过 SVG 渲染，使用已有 SVG 同步
  --setup-nginx   同步 nginx 配置
  -h, --help      显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-render)  SKIP_RENDER=true; shift ;;
    --setup-nginx)  SETUP_NGINX=true; shift ;;
    -h|--help)      usage; exit 0 ;;
    *)              echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

trap cleanup_staging EXIT
trap 'close_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"' EXIT

SHOWCASE_DIR="$ROOT_DIR/apps/showcase"
SHOWCASE_REMOTE="$DEPLOY_HOST:$REMOTE_DIR/showcase/"
CDN_SHOWCASE_REMOTE="$ASSET_HOST:$ASSET_REMOTE_DIR/showcase/"

# ─── 渲染 SVG ──────────────────────────────────────────
render_svgs() {
  log "编译 tautcore CLI"
  require_cmd cargo
  (cd "$ROOT_DIR" && cargo build --release -q -p tautcore-cli)

  log "渲染 showcase SVG"
  "$SHOWCASE_DIR/render.sh" --force
}

# ─── patch CDN_BASE 与 BUILD_HASH ──────────────────────
# 把 apps/showcase/index.html 里的 CDN_BASE 占位符替换为真实 CDN 地址
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
    raise SystemExit("apps/showcase/index.html: 未找到 CDN 标记")
Path(path).write_text(updated, encoding="utf-8")
PY
}

# 注入 BUILD_HASH（基于所有 SVG 内容的 sha256 前 8 位，用于前端缓存失效）
patch_build_hash() {
  local index_html="$1"
  local build_hash
  build_hash=$(cd "$SHOWCASE_DIR" && find . -name '*.svg' -type f | sort | xargs cat | shasum -a 256 | cut -d' ' -f1 | head -c 8)

  if [[ -z "$build_hash" ]]; then
    die "无法计算 BUILD_HASH（showcase 目录下没有 SVG 文件？）"
  fi

  log "BUILD_HASH=${build_hash}"
  # macOS sed 需要 -i ''，Linux sed 直接 -i；这里用兼容写法
  if [[ "$(uname)" == "Darwin" ]]; then
    sed -i '' "s/{{BUILD_HASH}}/${build_hash}/g" "$index_html"
  else
    sed -i "s/{{BUILD_HASH}}/${build_hash}/g" "$index_html"
  fi
}

# ─── 打包暂存 ───────────────────────────────────────────
stage_artifacts() {
  STAGING_DIR="$(new_staging_dir)"
  mkdir -p "$STAGING_DIR/showcase" "$STAGING_DIR/cdn-showcase"

  # 主站：showcase 不含 svg（走 CDN）
  # 例外：assets/brand/ 下的品牌 logo SVG 需跟随主站（相对路径引用）
  rsync -a \
    --include='assets/brand/' \
    --include='assets/brand/*.svg' \
    --exclude='*.svg' \
    --exclude='*.py' \
    --exclude='*.sh' \
    --exclude='test.md' \
    --exclude='README.md' \
    --exclude='.gitignore' \
    "$SHOWCASE_DIR/" "$STAGING_DIR/showcase/"

  patch_showcase_cdn "$STAGING_DIR/showcase/index.html"

  # CDN：showcase svg
  rsync -a \
    --include='*/' \
    --include='*.svg' \
    --exclude='*' \
    "$SHOWCASE_DIR/" "$STAGING_DIR/cdn-showcase/"
}

# ─── 上传 ───────────────────────────────────────────────
upload() {
  require_cmd rsync

  # BUILD_HASH 依赖 SVG 内容，必须在 stage 后计算
  patch_build_hash "$STAGING_DIR/showcase/index.html"
  # 注入 ICP 备案号
  log "注入 ICP 备案号 → apps/showcase/index.html"
  inject_icp_badge "$STAGING_DIR/showcase/index.html"

  # 主站（plotgram.cn）
  log "同步 showcase → plotgram.cn"
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR/showcase'"
  rsync -avz --delete \
    "$STAGING_DIR/showcase/" "$SHOWCASE_REMOTE"

  log "同步 CDN → $CDN_SHOWCASE_REMOTE"
  ssh "$ASSET_HOST" "mkdir -p '$ASSET_REMOTE_DIR/showcase'"
  rsync -avz --delete \
    "$STAGING_DIR/cdn-showcase/" "$CDN_SHOWCASE_REMOTE"
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 showcase ==="
  log "  访问地址: https://www.plotgram.cn/showcase/"

  setup_ssh_multiplexing "$DEPLOY_HOST" "$ASSET_HOST"

  if [[ "$SETUP_NGINX" == true ]]; then
    sync_nginx "$DEPLOY_HOST" nginx/plotgram.cn.conf
    sync_nginx "$ASSET_HOST" nginx/assets.pg.agcli.cn.conf
  fi

  if [[ "$SKIP_RENDER" == false ]]; then
    render_svgs
  else
    log "跳过 SVG 渲染（使用已有 SVG）"
  fi

  stage_artifacts
  upload

  echo ""
  echo "✅ 发布完成"
  echo "   Showcase:  https://www.plotgram.cn/showcase/"
  echo "   CDN SVG:   ${CDN_BASE}showcase/"
  echo ""
}

main "$@"
