#!/usr/bin/env bash
# 全局门禁开关。
#
# 现状：`GATES_DEFAULT=on` —— Stage 7 收口后恢复棘轮（AGENTS.md §10 已失效）。
# 临时关闭单次运行：
#   PLOTGRAM_GATES=off ./benchmarks/scripts/check-semantic-isolation.sh

GATES_DEFAULT="on"

gate_enabled() {
  [[ "${PLOTGRAM_GATES:-$GATES_DEFAULT}" == "on" ]]
}

# 棘轮脚本：门禁关闭时直接跳过并以 0 退出。
gate_skip_unless_enabled() {
  if ! gate_enabled; then
    echo "SKIP: ${1:-gate} —— 门禁已关闭（PLOTGRAM_GATES=off）"
    exit 0
  fi
}
