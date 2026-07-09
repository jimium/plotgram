#!/usr/bin/env bash
# 一键构建并发布 Plotgram Agent API 服务
#
# 流程：rsync 源码 → shanxun:/opt/plotgram → cargo build → 复制二进制 → 重启服务
# （shanxun 已装 Rust 1.96 + rsproxy.cn 镜像，2 核 1.8G 编译约 4-5 分钟）
#
# 用法:
#   ./deploy/deploy-agent-api.sh              # 同步代码 + 编译 + 重启
#   ./deploy/deploy-agent-api.sh --skip-sync  # 跳过 rsync，用服务器上已有代码编译
#   ./deploy/deploy-agent-api.sh --skip-build # 跳过编译，仅重启（用已有二进制）
#   ./deploy/deploy-agent-api.sh --setup-nginx # 同步 nginx 配置
#   ./deploy/deploy-agent-api.sh --dry-run     # 只同步代码不上传/重启
#
# 环境变量:
#   DEPLOY_HOST   SSH 目标（默认 shanxun）
#   REMOTE_SRC    服务器源码目录（默认 /opt/plotgram）
#   REMOTE_DIR    部署目录（默认 /opt/plotgram-agent-api）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

DEPLOY_HOST="${DEPLOY_HOST:-shanxun}"
REMOTE_SRC="${REMOTE_SRC:-/opt/plotgram}"
REMOTE_DIR="${REMOTE_DIR:-/opt/plotgram-agent-api}"

SKIP_SYNC=false
SKIP_BUILD=false
SETUP_NGINX=false
DRY_RUN=false

usage() {
  cat <<'EOF'
用法: deploy/deploy-agent-api.sh [选项]

同步源码到 shanxun:/opt/plotgram，本地编译，部署到 /opt/plotgram-agent-api 并重启。

选项:
  --skip-sync    跳过 rsync 同步，用服务器上已有代码编译
  --skip-build   跳过编译，仅重启（使用已有二进制）
  --setup-nginx  同步 nginx 配置到 shanxun（首次部署或配置变更时使用）
  --dry-run      只同步代码，不编译/不重启
  -h, --help     显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-sync)    SKIP_SYNC=true; shift ;;
    --skip-build)   SKIP_BUILD=true; shift ;;
    --setup-nginx)  SETUP_NGINX=true; shift ;;
    --dry-run)      DRY_RUN=true; shift ;;
    -h|--help)      usage; exit 0 ;;
    *) echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

log()  { echo "▸ $*"; }
die()  { echo "✗ $*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "未找到命令: $1"
}

# ─── 同步源码 ───────────────────────────────────────────
sync_source() {
  require_cmd rsync
  log "同步源码 → $DEPLOY_HOST:$REMOTE_SRC ..."
  rsync -az --delete \
    --exclude='target' \
    --exclude='node_modules' \
    --exclude='agent-demo/plotgram-wasm' \
    --exclude='playground/plotgram-wasm' \
    --exclude='studio/plotgram-wasm' \
    --exclude='agent-demo/dist' \
    --exclude='playground/dist' \
    --exclude='website/dist' \
    --exclude='.git' \
    --exclude='showcase/.history' \
    "$ROOT_DIR/" "$DEPLOY_HOST:$REMOTE_SRC/"
  log "源码同步完成"
}

# ─── 远程编译 ───────────────────────────────────────────
build_remote() {
  require_cmd ssh
  log "在 $DEPLOY_HOST 上编译 (cargo build --release -p plotgram-server)..."
  log "  目录: $REMOTE_SRC"
  log "  镜像: rsproxy.cn (配置在 ~/.cargo/config.toml)"
  ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=10 "$DEPLOY_HOST" \
    "cd $REMOTE_SRC && source ~/.cargo/env 2>/dev/null; cargo build --release -p plotgram-server 2>&1 | tail -15"

  # 验证二进制存在
  if ! ssh "$DEPLOY_HOST" "test -f $REMOTE_SRC/target/release/plotgram-server"; then
    die "编译失败：二进制不存在 $REMOTE_SRC/target/release/plotgram-server"
  fi

  local size
  size=$(ssh "$DEPLOY_HOST" "du -h $REMOTE_SRC/target/release/plotgram-server | cut -f1")
  log "编译完成: $REMOTE_SRC/target/release/plotgram-server ($size)"
}

# ─── 部署二进制 + 脚本 ─────────────────────────────────
deploy_binary() {
  require_cmd ssh
  log "确保部署目录存在..."
  ssh "$DEPLOY_HOST" "mkdir -p '$REMOTE_DIR'"

  log "复制二进制 → $REMOTE_DIR/plotgram-server.new"
  ssh "$DEPLOY_HOST" "cp $REMOTE_SRC/target/release/plotgram-server $REMOTE_DIR/plotgram-server.new && chmod +x $REMOTE_DIR/plotgram-server.new"

  log "同步 start.sh / stop.sh / .env.example..."
  rsync -az \
    "$SCRIPT_DIR/agent-api/start.sh" \
    "$SCRIPT_DIR/agent-api/stop.sh" \
    "$SCRIPT_DIR/agent-api/.env.example" \
    "$DEPLOY_HOST:$REMOTE_DIR/"

  # 首次部署：创建 .env（如不存在）
  ssh "$DEPLOY_HOST" "cd '$REMOTE_DIR' && [ -f .env ] || cp .env.example .env"
  ssh "$DEPLOY_HOST" "chmod +x '$REMOTE_DIR/start.sh' '$REMOTE_DIR/stop.sh'"
}

# ─── 重启服务 ───────────────────────────────────────────
restart_service() {
  log "停止旧服务..."
  ssh "$DEPLOY_HOST" "cd '$REMOTE_DIR' && ./stop.sh || true"

  log "替换二进制..."
  ssh "$DEPLOY_HOST" "cd '$REMOTE_DIR' && mv -f plotgram-server.new plotgram-server && chmod +x plotgram-server"

  log "启动新服务..."
  ssh "$DEPLOY_HOST" "cd '$REMOTE_DIR' && ./start.sh"

  log "验证健康检查..."
  sleep 2
  local health
  health=$(ssh "$DEPLOY_HOST" "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:6080/health" 2>/dev/null || echo "000")
  if [[ "$health" == "200" ]]; then
    log "✅ 健康检查通过 (HTTP 200)"
  else
    echo "⚠ 健康检查异常 (HTTP $health)，检查日志: ssh $DEPLOY_HOST 'tail -50 $REMOTE_DIR/server.log'"
  fi
}

# ─── 同步 nginx 配置 ───────────────────────────────────
setup_nginx() {
  log "同步 nginx 配置到 $DEPLOY_HOST ..."
  scp "$SCRIPT_DIR/nginx/assets.pg.agcli.cn.conf" \
      "$SCRIPT_DIR/nginx/api.pg.agcli.cn.conf" \
      "$DEPLOY_HOST:/etc/nginx/conf.d/"
  ssh "$DEPLOY_HOST" 'nginx -t && systemctl reload nginx && echo "✅ nginx 已重载"'
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 发布 Plotgram Agent API ==="
  log "  部署服务器: $DEPLOY_HOST"
  log "  源码目录:   $REMOTE_SRC"
  log "  部署目录:   $REMOTE_DIR"

  if [[ "$SKIP_SYNC" == false ]]; then
    sync_source
  else
    log "跳过 rsync 同步"
  fi

  if [[ "$DRY_RUN" == true ]]; then
    log "--dry-run 模式，跳过编译/部署"
    exit 0
  fi

  if [[ "$SKIP_BUILD" == false ]]; then
    build_remote
  else
    log "跳过编译"
  fi

  if [[ "$SETUP_NGINX" == true ]]; then
    setup_nginx
  fi

  deploy_binary
  restart_service

  echo ""
  echo "✅ 发布完成"
  echo "   健康检查: https://api.pg.agcli.cn/health"
  echo "   API:      https://api.pg.agcli.cn/agent/chat"
  echo "   静态资源: https://assets.pg.agcli.cn/"
  echo "   远程目录: $DEPLOY_HOST:$REMOTE_DIR"
  echo "   查看日志: ssh $DEPLOY_HOST 'tail -f $REMOTE_DIR/server.log'"
  echo "   重启服务: ssh $DEPLOY_HOST 'cd $REMOTE_DIR && ./stop.sh && ./start.sh'"
}

main "$@"
