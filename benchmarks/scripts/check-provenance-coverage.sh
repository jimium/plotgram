#!/usr/bin/env bash
# Stage 7：Atlas Plan 边级 Provenance 覆盖率门禁。
# 跑 provenance_check 单测（含 product flowchart 覆盖率 = 100%）。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-provenance-coverage.sh"

out="$(
  cargo test -p tautcore-core --lib layout::atlas::provenance_check -- --nocapture 2>&1
)" || {
  echo "$out" | tail -60
  echo "FAIL: provenance_check tests failed"
  exit 1
}

if echo "$out" | rg -q "test result: FAILED|FAILED\."; then
  echo "$out" | tail -60
  echo "FAIL: provenance coverage tests reported failures"
  exit 1
fi

echo "OK: atlas channel provenance coverage (provenance_check)"
