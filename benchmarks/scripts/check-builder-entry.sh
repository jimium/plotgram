#!/usr/bin/env bash
# G5：CoordinateProblem 生产构造点棘轮。
# 生产路径应经 CoordinateProblem::build（或等价门面）；内联 `CoordinateProblem {` 仅允许测试。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-builder-entry.sh"

# 内联结构体字面量（排除 struct/impl 定义与测试）
inline_hits="$(
  rg -n 'CoordinateProblem\s*\{' crates/plotgram-core/src/layout -g '*.rs' \
    | rg -v 'pub struct CoordinateProblem' \
    | rg -v 'impl CoordinateProblem' \
    | rg -v '_tests\.rs:' \
    | rg -v '/tests\.rs:' \
    | rg -v '^\S+:\s*//' \
    || true
)"

# 允许：cfg(test) 模块内；kernel 单测辅助（auditor/analysis/optimizer/coordinator）
# 生产路径：0 处内联 `{`（flat/arch/intra/mindmap/main_axis/group_ir 均走 ::build）
prod_hits="$(
  echo "$inline_hits" \
    | rg -v 'kernel/coordinate/(auditor|analysis|optimizer)\.rs:' \
    | rg -v 'kernel/coordinator\.rs:' \
    | rg -v 'group_ir\.rs:.*#\[cfg\(test\)\]' \
    || true
)"

# group_ir / 其它文件中的测试内联：用路径白名单再滤一遍（测试 fn 所在文件仍可能命中）
# 保守：仅允许下列「测试/诊断」文件保留内联；其余生产文件必须为 0。
allowed_inline="$(
  echo "$inline_hits" \
    | rg 'kernel/coordinate/(auditor|analysis|optimizer|group_ir)\.rs:|kernel/coordinator\.rs:' \
    || true
)"
prod_only="$(
  echo "$inline_hits" \
    | rg -v 'kernel/coordinate/(auditor|analysis|optimizer|group_ir)\.rs:' \
    | rg -v 'kernel/coordinator\.rs:' \
    || true
)"

MAX_PROD_INLINE="${BUILDER_INLINE_MAX:-0}"
prod_count="$(echo "$prod_only" | sed '/^$/d' | wc -l | tr -d ' ')"

echo "OK: CoordinateProblem::build is the production facade"
echo "    production inline CoordinateProblem {{ count=${prod_count} (≤${MAX_PROD_INLINE})"

if [[ -n "${prod_only}" && "${prod_count}" -gt "${MAX_PROD_INLINE}" ]]; then
  echo "FAIL: production CoordinateProblem {{ outside tests/kernel helpers:"
  echo "$prod_only"
  exit 1
fi

# 门面存在性
if ! rg -q 'fn build\(' crates/plotgram-core/src/layout/kernel/coordinate/model.rs; then
  echo "FAIL: CoordinateProblem::build missing"
  exit 1
fi

echo "OK: builder entry (inline≤${MAX_PROD_INLINE}; allowed kernel helpers may still inline)"
if [[ -n "${allowed_inline}" ]]; then
  echo "    note: kernel helper/test inlines:"
  echo "$allowed_inline" | head -20
fi
