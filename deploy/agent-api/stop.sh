#!/usr/bin/env bash
# 停止 plotgram-agent-api
#
# 用法: ./stop.sh
#   先发 SIGTERM（优雅退出），5 秒后仍存活则 SIGKILL

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PID_FILE="$SCRIPT_DIR/plotgram-server.pid"

if [[ ! -f "$PID_FILE" ]]; then
    echo "▸ 未找到 PID 文件 ($PID_FILE)，进程可能已停止"
    # 兜底：按进程名清理残留
    PIDS=$(pgrep -f "plotgram-server" 2>/dev/null || true)
    if [[ -n "$PIDS" ]]; then
        echo "⚠ 发现残留进程: $PIDS，尝试清理..."
        echo "$PIDS" | xargs kill 2>/dev/null || true
        sleep 1
        echo "$PIDS" | xargs kill -9 2>/dev/null || true
    fi
    exit 0
fi

PID=$(cat "$PID_FILE" 2>/dev/null || true)
if [[ -z "$PID" ]]; then
    rm -f "$PID_FILE"
    echo "▸ PID 文件为空，已清理"
    exit 0
fi

if ! kill -0 "$PID" 2>/dev/null; then
    echo "▸ 进程 $PID 已不在运行，清理 PID 文件"
    rm -f "$PID_FILE"
    exit 0
fi

echo "▸ 发送 SIGTERM → PID=$PID..."
kill -TERM "$PID"

# 等待最多 5 秒优雅退出
for i in $(seq 1 5); do
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "✅ 已停止 (PID=$PID)"
        rm -f "$PID_FILE"
        exit 0
    fi
    sleep 1
done

# 强制杀死
echo "⚠ 优雅退出超时，发送 SIGKILL..."
kill -9 "$PID" 2>/dev/null || true
rm -f "$PID_FILE"
echo "✅ 已强制停止 (PID=$PID)"
