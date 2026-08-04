#!/usr/bin/env bash
# 在 apps/ 目录下启动静态文件服务（无缓存，便于开发时刷新看到最新 SVG）
# 监听端口: 8030
# 端口占用时: 先杀掉占用进程，再启动

set -euo pipefail

PORT=8030
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVING_DIR="$SCRIPT_DIR"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "错误: 未找到命令 '$1'，请先安装"; exit 1; }
}

kill_port_occupant() {
  local port="$1"
  require_cmd lsof
  require_cmd kill

  # lsof -ti 返回监听该端口的 PID 列表（没有则空）
  local pids
  pids="$(lsof -ti:"$port" -sTCP:LISTEN 2>/dev/null || true)"

  if [[ -z "$pids" ]]; then
    echo "端口 $port 当前空闲"
    return 0
  fi

  echo "发现端口 $port 被占用，占用 PID(s): $pids"
  echo "先尝试优雅终止 (SIGTERM)..."
  kill $pids 2>/dev/null || true

  # 等待最多 3 秒让进程退出
  local waited=0
  while [[ $waited -lt 3 ]]; do
    sleep 1
    waited=$((waited + 1))
    local remaining
    remaining="$(lsof -ti:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -z "$remaining" ]]; then
      echo "端口 $port 已释放"
      return 0
    fi
  done

  # 仍未退出，强制 kill -9
  local remaining
  remaining="$(lsof -ti:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$remaining" ]]; then
    echo "进程仍未退出，执行强制终止 (SIGKILL) PID(s): $remaining"
    kill -9 $remaining 2>/dev/null || true
    sleep 1
  fi
}

main() {
  require_cmd python3

  echo "=============================================="
  echo " Serving: $SERVING_DIR"
  echo "   Port : http://localhost:$PORT"
  echo "=============================================="
  echo ""

  kill_port_occupant "$PORT"

  echo ""
  echo "🚀 启动静态文件服务 (无缓存, Ctrl+C 退出)..."
  echo ""
  cd "$SERVING_DIR"
  exec python3 "$SCRIPT_DIR/serve.py" "$PORT" -d .
}

main "$@"
