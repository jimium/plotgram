# shellcheck shell=bash
# Plotgram 部署公共库
#
# 各站点发布脚本 source 本文件后即可使用：
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"
#
# 提供：
#   - 基础工具：log / die / require_cmd / usage_die
#   - 路径常量：ROOT_DIR / SCRIPT_DIR / DEPLOY_LIB_DIR
#   - SSH 连接复用：开启 ControlMaster，避免短时间内多次 rsync 触发服务器连接限流
#   - rsync 封装：rsync_to <src/> <host:path/> [exclude args...]
#   - nginx 配置同步：sync_nginx <host> <conf1> [conf2...] → nginx -t && reload
#   - staging 临时目录：new_staging_dir / cleanup_staging
#
# 约定：
#   - 本文件只定义函数与少量常量，不执行副作用；source 时不会产生输出。
#   - 调用方负责在 main 中调用 setup_ssh_multiplexing（如需）与 cleanup_staging。

# ─── 路径常量 ───────────────────────────────────────────
DEPLOY_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "$DEPLOY_LIB_DIR/.." && pwd)"
ROOT_DIR="$(cd "$DEPLOY_DIR/.." && pwd)"
NGINX_CONF_DIR="$DEPLOY_DIR/nginx"

# ─── 默认部署目标（适用于 website / playground / showcase / agent-demo）────────
# 注意：deploy-agent-api.sh 部署在 shanxun 而非 plotgram.dev，
#       它在 source 本文件后会直接覆盖 DEPLOY_HOST / REMOTE_DIR。
DEPLOY_HOST="${DEPLOY_HOST:-plotgram.dev}"
ASSET_HOST="${ASSET_HOST:-shanxun}"
REMOTE_DIR="${REMOTE_DIR:-/var/www/plotgram}"
ASSET_REMOTE_DIR="${ASSET_REMOTE_DIR:-/var/www/assets.pg.agcli.cn}"
CDN_BASE="${CDN_BASE:-https://assets.pg.agcli.cn/}"

# SSH ControlMaster socket 路径（按 host+user 区分，%C 哈希）
_DEPLOY_SSH_CONTROL_PATH="/tmp/plotgram-deploy-ssh-%C"
_DEPLOY_SSH_OPTS=(-o ControlMaster=auto -o ControlPath="$_DEPLOY_SSH_CONTROL_PATH" -o ControlPersist=120)

# ─── 基础工具 ───────────────────────────────────────────
log() { echo "▸ $*" >&2; }
die() { echo "✗ $*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "未找到命令: $1"
}

# usage_die <message>：打印错误 + 用法后退出
usage_die() {
  echo "✗ $*" >&2
  exit 1
}

# ─── SSH 连接复用 ───────────────────────────────────────
# 启用 ControlMaster，让同一台 host 的多个 rsync/scp/ssh 复用一条 TCP 连接。
# 解决短时间多次 rsync 触发服务器 sshd 连接限流导致 "Connection closed" 的问题。
#
# 调用：setup_ssh_multiplexing <host> [<host>...]
# 退出时会自动关闭通过 RSYNC_RSH 开启的 master 连接（trap EXIT）。
setup_ssh_multiplexing() {
  local hosts=("$@")
  local ssh_opts_str="${_DEPLOY_SSH_OPTS[*]}"
  # 通过 RSYNC_RSH 让 rsync 也走复用连接
  export RSYNC_RSH="ssh $ssh_opts_str"
  # 预热：对每个 host 建立一条 master 连接
  local h
  for h in "${hosts[@]}"; do
    ssh "${_DEPLOY_SSH_OPTS[@]}" -o ConnectTimeout=10 "$h" 'true' 2>/dev/null || true
  done
}

# 关闭某个 host 的 master 连接（一般在脚本结束时调用，或交由 trap）
close_ssh_multiplexing() {
  local h
  for h in "$@"; do
    ssh -O exit -o ControlPath="$_DEPLOY_SSH_CONTROL_PATH" "$h" 2>/dev/null || true
  done
}

# ─── rsync 封装 ────────────────────────────────────────
# rsync_to <src_dir/> <host:path/> [--exclude=xxx ...]
# src 必须以 / 结尾。删除目标端多余文件（--delete），除非通过环境变量关闭。
rsync_to() {
  require_cmd rsync
  local src="$1"
  local dst="$2"
  shift 2
  [[ "$src" == */ ]] || die "rsync_to: src 必须以 / 结尾，实际: $src"
  [[ -d "$src" ]] || die "rsync_to: 源目录不存在: $src"

  local args=(--delete)
  # 透传额外的 --exclude / --include 等参数
  args+=("$@")

  rsync -avz "${args[@]}" "$src" "$dst"
}

# ─── nginx 配置同步 ────────────────────────────────────
# sync_nginx <host> <conf_file> [conf_file...] → nginx -t && systemctl reload
# conf_file 用绝对路径或相对 DEPLOY_DIR 的路径均可。
sync_nginx() {
  local host="$1"
  shift
  local confs=()
  local c
  for c in "$@"; do
    # 支持相对 deploy/ 的路径
    if [[ ! -f "$c" ]]; then
      c="$DEPLOY_DIR/$c"
    fi
    [[ -f "$c" ]] || die "nginx 配置不存在: $c"
    confs+=("$c")
  done

  log "同步 nginx 配置 → $host ..."
  scp "${confs[@]}" "$host:/etc/nginx/conf.d/"
  ssh "$host" 'nginx -t && systemctl reload nginx && echo "✅ nginx 已重载"'
}

# ─── staging 临时目录 ──────────────────────────────────
# 用于在打包阶段把多个来源的文件汇集到一个临时目录，再统一 rsync。
_STAGING_DIR=""

new_staging_dir() {
  STAGING_DIR="$(mktemp -d)"
  _STAGING_DIR="$STAGING_DIR"
  log "staging: $STAGING_DIR"
  echo "$STAGING_DIR"
}

cleanup_staging() {
  if [[ -n "$_STAGING_DIR" && -d "$_STAGING_DIR" ]]; then
    rm -rf "$_STAGING_DIR"
    _STAGING_DIR=""
  fi
}

# ─── 通用参数解析 ──────────────────────────────────────
# parse_common_flags：解析 --skip-build / --setup-nginx / --help
# 各脚本如需站点专属 flag，可在调用本函数后自行扩展。
# 返回值通过全局变量：COMMON_SKIP_BUILD / COMMON_SETUP_NGINX
COMMON_SKIP_BUILD=false
COMMON_SETUP_NGINX=false

parse_common_flags() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --skip-build)   COMMON_SKIP_BUILD=true; shift ;;
      --setup-nginx)  COMMON_SETUP_NGINX=true; shift ;;
      -h|--help)      return 1 ;;  # 让调用方打印 usage
      *)              return 0 ;;  # 遇到非通用 flag，交回调用方处理（shift 由调用方负责）
    esac
  done
  return 0
}
