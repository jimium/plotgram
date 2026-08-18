#!/usr/bin/env bash
# G5 / Stage 7：CoordinateProblem 生产构造点棘轮。
# 允许：CoordinateProblem::from_contract / Atlas solve_from_contract。
# 禁止：生产路径内联 `CoordinateProblem {`。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-builder-entry.sh"

# 门面存在性
if ! rg -q 'fn from_contract\(' crates/tautcore-core/src/layout/kernel/coordinate/layout_contract.rs \
  && ! rg -q 'fn from_contract\(' crates/tautcore-core/src/layout/kernel/coordinate/model.rs; then
  echo "FAIL: CoordinateProblem::from_contract missing"
  exit 1
fi

if ! rg -q 'pub fn solve_from_contract\(' crates/tautcore-core/src/layout/atlas/solve.rs; then
  echo "FAIL: atlas solve_from_contract missing (Atlas metric entry)"
  exit 1
fi

# 内联结构体字面量（排除 struct/impl 定义与测试）
inline_hits="$(
  rg -n 'CoordinateProblem\s*\{' crates/tautcore-core/src/layout -g '*.rs' \
    | rg -v 'pub struct CoordinateProblem' \
    | rg -v 'impl CoordinateProblem' \
    | rg -v '_tests\.rs:' \
    | rg -v '/tests\.rs:' \
    | rg -v '^\S+:\s*//' \
    || true
)"

# 允许：cfg(test) / kernel 单测辅助
prod_only="$(
  echo "$inline_hits" \
    | rg -v 'kernel/coordinate/(auditor|analysis|optimizer|group_ir)\.rs:' \
    | rg -v 'kernel/coordinator\.rs:' \
    || true
)"

MAX_PROD_INLINE="${BUILDER_INLINE_MAX:-0}"
prod_count="$(echo "$prod_only" | sed '/^$/d' | wc -l | tr -d ' ')"

echo "OK: CoordinateProblem::from_contract + atlas::solve_from_contract are production facades"
echo "    production inline CoordinateProblem {{ count=${prod_count} (≤${MAX_PROD_INLINE})"

if [[ -n "${prod_only}" && "${prod_count}" -gt "${MAX_PROD_INLINE}" ]]; then
  echo "FAIL: production CoordinateProblem {{ outside tests/kernel helpers:"
  echo "$prod_only"
  exit 1
fi

echo "OK: builder entry (Atlas solve_from_contract / from_contract; inline≤${MAX_PROD_INLINE})"
