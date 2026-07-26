#!/usr/bin/env bash
# Stage 7：相 II/III 公开 API 不变量——Ink / materialize 只读 Plan，无 DiagramType。
# 禁止 ink.rs 与 pipeline 落笔段出现 `&mut Plan` / `mut plan: Plan` / DiagramType。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-atlas-phase-api.sh"

fail=0
INK="crates/plotgram-core/src/layout/atlas/ink.rs"
PIPE="crates/plotgram-core/src/layout/atlas/pipeline.rs"

# 1) ink 不得出现 DiagramType
if rg -n 'DiagramType' "$INK" | rg -v '^\S+:\s*//' | rg -v '^\S+:\s*//!' | rg -v '^\S+:\s*///' | rg -q .; then
  echo "FAIL: DiagramType in atlas/ink.rs (phase III must not branch on diagram type):"
  rg -n 'DiagramType' "$INK" || true
  fail=1
fi

# 2) ink 公开 API 不得持有 &mut Plan / mut Plan
mut_plan="$(
  rg -n '&mut\s+Plan|mut\s+plan:\s*Plan|mut\s+plan:\s*&mut' "$INK" || true
)"
if [[ -n "${mut_plan}" ]]; then
  echo "FAIL: mutable Plan in atlas/ink.rs (Ink must take &Plan only):"
  echo "$mut_plan"
  fail=1
fi

# 3) materialize_edges 签名必须是 &Plan（只读）
if ! rg -q 'pub fn materialize_edges\(' "$INK"; then
  echo "FAIL: materialize_edges missing in ink.rs"
  fail=1
elif ! rg -n 'pub fn materialize_edges\(' -A6 "$INK" | rg -q 'plan:\s*&Plan'; then
  echo "FAIL: materialize_edges must take plan: &Plan"
  rg -n 'pub fn materialize_edges\(' -A8 "$INK" || true
  fail=1
fi

# 4) pipeline 落笔调用 materialize_edges 前须跑 provenance 覆盖
if ! rg -q 'assert_channel_provenance_coverage' "$PIPE"; then
  echo "FAIL: pipeline must call assert_channel_provenance_coverage before ink"
  fail=1
fi

# 5) solve 公开入口：solve_from_contract 存在（度量相契约入口，非内联 CoordinateProblem）
if ! rg -q 'pub fn solve_from_contract\(' crates/plotgram-core/src/layout/atlas/solve.rs; then
  echo "FAIL: solve_from_contract missing"
  fail=1
fi

if (( fail != 0 )); then
  exit 1
fi

echo "OK: atlas phase API (ink &Plan-only, no DiagramType; provenance gate wired)"
