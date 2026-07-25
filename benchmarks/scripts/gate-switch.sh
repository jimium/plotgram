#!/usr/bin/env bash
# 全局门禁开关（新架构重大设计调整期）。
#
# 现状：`GATES_DEFAULT=off` —— 所有棘轮脚本默认跳过，compare 降为「只报告不阻断」。
# 依据：AGENTS.md §10（下一代布局与路由架构设计期门禁豁免）。
#
# 临时开启单次运行：
#   PLOTGRAM_GATES=on ./benchmarks/scripts/check-semantic-isolation.sh
# 期末恢复：把 GATES_DEFAULT 改回 on，并删除 AGENTS.md §10。

GATES_DEFAULT="off"

gate_enabled() {
  [[ "${PLOTGRAM_GATES:-$GATES_DEFAULT}" == "on" ]]
}

# 棘轮脚本：门禁关闭时直接跳过并以 0 退出。
gate_skip_unless_enabled() {
  if ! gate_enabled; then
    echo "SKIP: ${1:-gate} —— 新架构期门禁默认关闭（PLOTGRAM_GATES=on 可临时开启）"
    exit 0
  fi
}
