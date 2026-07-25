#!/usr/bin/env bash
# G0/G4：group 写权棘轮——解析 CLI `[group_write] main=N`，断言 main ≤ THRESHOLD。
# G4：cloud-native 目标 main≤2（compute_bounds + canvas_translate）；理想 1。
# 可用 GROUP_WRITE_THRESHOLD 覆盖。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

THRESHOLD="${GROUP_WRITE_THRESHOLD:-2}"
SAMPLE="${GROUP_WRITE_SAMPLE:-showcase/architecture/product.cloud-native.pgm}"

if [[ ! -f "$SAMPLE" ]]; then
  echo "FAIL: sample not found: $SAMPLE"
  exit 1
fi

# PLOTGRAM_PERF=1 为文档约定；当前 perf_log 在非 wasm 下始终打 stderr。
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

echo "OK: group_writes main=${main} (threshold≤${THRESHOLD}) sample=${SAMPLE}"
echo "    ${sites}"

if (( main > THRESHOLD )); then
  echo "FAIL: group_writes main=${main} > threshold=${THRESHOLD}"
  exit 1
fi
