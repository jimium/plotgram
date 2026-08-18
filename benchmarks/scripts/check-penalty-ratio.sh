#!/usr/bin/env bash
# Stage 7：LexCost / 罚项门禁。
# 优先校验 Atlas 使用 LexCost（无跨层标量求和）；旧 routing/objectives 比例降为 WARN。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-penalty-ratio.sh"

fail=0

# ── Atlas / LexCost 硬轨 ──────────────────────────────────
# 1) LexCost 定义在中立 kernel/cost.rs
if ! rg -q 'pub struct LexCost' crates/tautcore-core/src/layout/kernel/cost.rs; then
  echo "FAIL: LexCost missing in layout/kernel/cost.rs"
  fail=1
fi

# 2) 禁止对 LexCost 做跨层字段求和的 Add / 标量折叠 API
bad_lex="$(
  rg -n 'impl\s+(Add|AddAssign).*LexCost|fn\s+\w*as_scalar\w*|fn\s+\w*total_cost\w*|fn\s+\w*sum_layers\w*' \
    crates/tautcore-core/src/layout/kernel/cost.rs \
    crates/tautcore-core/src/layout/atlas \
    || true
)"
if [[ -n "${bad_lex}" ]]; then
  echo "FAIL: LexCost must not expose cross-layer scalar sum / Add:"
  echo "$bad_lex"
  fail=1
fi

# 3) channel 搜索必须用 LexCost（非裸 f64 罚项表驱动）
if ! rg -q 'LexCost' crates/tautcore-core/src/layout/atlas/channel/search.rs; then
  echo "FAIL: atlas/channel/search.rs must use LexCost"
  fail=1
fi

# ── 旧 objectives 软轨（WARN，不阻断）──────────────────────
if [[ -f crates/tautcore-core/src/layout/routing/objectives.rs ]]; then
  out="$(cargo test -p tautcore-core --lib layout::routing::objectives::tests -- --nocapture 2>&1)" || true
  if echo "$out" | rg -q "test result: FAILED|FAILED\."; then
    echo "WARN: legacy soft ranking ratio/items exceed caps (routing/objectives.rs) — Atlas LexCost is the hard gate"
    echo "$out" | tail -20
  fi

  violations="$(
    rg -n '^\s*(pub(\([^)]*\))?\s+)?const\s+\w*PENALTY\w*\s*:' \
      crates/tautcore-core/src/layout/routing \
      --glob '!**/objectives.rs' \
      || true
  )"
  if [[ -n "${violations}" ]]; then
    echo "WARN: penalty consts outside routing/objectives.rs (legacy debt):"
    echo "$violations"
  fi
fi

if (( fail != 0 )); then
  exit 1
fi

echo "OK: LexCost API + atlas channel usage (legacy penalty ratio WARN-only)"
