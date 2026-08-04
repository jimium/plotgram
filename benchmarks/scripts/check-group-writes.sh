#!/usr/bin/env bash
# group 写权棘轮——解析 CLI `[group_write] main=N`，断言 main ≤ THRESHOLD。
# Stage 7 / Atlas：Hierarchical 路径仅 materialize 写组框（write_counter ≤ 1）；
# canvas 平移不计。可用 GROUP_WRITE_THRESHOLD 覆盖。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-group-writes.sh"

THRESHOLD="${GROUP_WRITE_THRESHOLD:-1}"
# Atlas：flowchart Cross materialize 写权 0；architecture StrongMacro materialize ≤1。
# 样本取 architecture，阈值 1 = materialize-only（canvas 平移不计）。
SAMPLE="${GROUP_WRITE_SAMPLE:-apps/showcase/architecture/product.cloud-native.pgm}"
# flowchart 路径另验：GROUP_WRITE_SAMPLE=apps/showcase/flowchart/product.user-auth.pgm GROUP_WRITE_THRESHOLD=0

if [[ ! -f "$SAMPLE" ]]; then
  echo "FAIL: sample not found: $SAMPLE"
  exit 1
fi

out="$(
  PLOTGRAM_PERF=1 cargo run -q -p plotgram-cli -- render "$SAMPLE" -o /tmp/plotgram-group-writes-check.svg 2>&1
)" || {
  echo "$out" | tail -40
  echo "FAIL: render failed for $SAMPLE"
  exit 1
}

# 取最后一次 main=（route 后 / canvas 最终摘要）
main="$(
  echo "$out" | rg -o '\[group_write\] main=[0-9]+' | tail -1 | rg -o '[0-9]+$' || true
)"

if [[ -z "${main}" ]]; then
  echo "$out" | tail -60
  echo "FAIL: no [group_write] main=N in CLI stderr"
  exit 1
fi

sites="$(
  echo "$out" | rg '\[group_write\] main=' | tail -1 || true
)"

echo "OK: group_writes main=${main} (threshold≤${THRESHOLD}) sample=${SAMPLE} (Atlas materialize-only)"
echo "    ${sites}"

if (( main > THRESHOLD )); then
  echo "FAIL: group_writes main=${main} > threshold=${THRESHOLD}"
  exit 1
fi
