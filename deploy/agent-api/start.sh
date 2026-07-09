#!/usr/bin/env bash
# 启动 plotgram-agent-api（后台运行）
#
# 用法: ./start.sh
#
# 环境变量从同目录 .env 文件读取。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BINARY="$SCRIPT_DIR/plotgram-server"
ENV_FILE="$SCRIPT_DIR/.env"
PID_FILE="$SCRIPT_DIR/plotgram-server.pid"
LOG_FILE="$SCRIPT_DIR/server.log"

# ─── 前置检查 ────────────────────────────────────────────
if [[ ! -f "$BINARY" ]]; then
    echo "✗ 二进制文件不存在: $BINARY" >&2
    exit 1
fi

if [[ ! -x "$BINARY" ]]; then
    chmod +x "$BINARY"
fi

# 已在运行则跳过
if [[ -f "$PID_FILE" ]]; then
    OLD_PID=$(cat "$PID_FILE" 2>/dev/null || true)
    if [[ -n "$OLD_PID" ]] && kill -0 "$OLD_PID" 2>/dev/null; then
        echo "▸ plotgram-server 已在运行 (PID=$OLD_PID)，跳过启动"
        exit 0
    fi
    # PID 文件残留但进程已退出，清理
    rm -f "$PID_FILE"
fi

# ─── 加载 .env ───────────────────────────────────────────
if [[ -f "$ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +a
    echo "▸ 已加载 .env"
else
    echo "⚠ 未找到 .env 文件，使用默认配置（DEMO_ENABLED 可能无法工作）"
fi

# 强制绑定到 127.0.0.1，由 nginx 反向代理对外
export PLOTGRAM_SERVER_ADDR="${PLOTGRAM_SERVER_ADDR:-127.0.0.1:6080}"

# ─── 后台启动 ────────────────────────────────────────────
echo "▸ 启动 plotgram-server..."
echo "  二进制: $BINARY"
echo "  监听:   $PLOTGRAM_SERVER_ADDR"
echo "  日志:   $LOG_FILE"

nohup "$BINARY" >>"$LOG_FILE" 2>&1 &
SERVER_PID=$!
echo "$SERVER_PID" > "$PID_FILE"

# 等待进程启动，确认存活
sleep 1
if kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "✅ 启动成功 (PID=$SERVER_PID)"
    echo "   停止: ./stop.sh"
    echo "   日志: tail -f $LOG_FILE"
else
    echo "✗ 启动失败，进程已退出" >&2
    rm -f "$PID_FILE"
    echo "─── 最近日志 ───" >&2
    tail -20 "$LOG_FILE" >&2 || true
    exit 1
fi
