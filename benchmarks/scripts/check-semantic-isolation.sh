#!/usr/bin/env bash
# Phase 6：语义隔离 + mindmap builder 白名单 + EdgeLayout 可变签名上限。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

fail=0

# 仅匹配「代码中的 DiagramType 标识符」，忽略注释/文档行。
code_diagram_type() {
  local path="$1"
  rg -n 'DiagramType' "$path" -g '*.rs' \
    --glob '!**/*test*' \
    --glob '!**/orthogonal_tests.rs' \
    | rg -v '^\S+:\s*//' \
    | rg -v '^\S+:\s*//!' \
    | rg -v '^\S+:\s*#' \
    | rg -v '^\S+:\s*\*' \
    | rg -v '^\S+:\s*///' \
    || true
}

# 1) kernel：除文档外不得出现 DiagramType
kernel_code="$(code_diagram_type crates/plotgram-core/src/layout/kernel | rg -v 'mod\.rs:' || true)"
# 单测内 DiagramType：bounds/edge_gutter 的 #[cfg(test)] 块（生产路径已无）
kernel_code="$(echo "$kernel_code" | rg -v 'kernel/(common/edge_gutter|group/bounds)\.rs:' || true)"
if [[ -n "${kernel_code}" ]]; then
  echo "FAIL: DiagramType in layout/kernel:"
  echo "$kernel_code"
  fail=1
fi

# 2) edge_routing_orthogonal 生产代码
ortho_code="$(code_diagram_type crates/plotgram-core/src/layout/routing/edge_routing_orthogonal)"
# feedback_side：仅允许 cfg(test) 内
if echo "$ortho_code" | rg -q 'feedback_side\.rs:'; then
  prod_fb="$(
    awk '
      /^#\[cfg\(test\)\]/ {test=1}
      test==0 && /DiagramType/ && $0 !~ /^[[:space:]]*\/\// && $0 !~ /^[[:space:]]*\/\*/ {print NR":"$0}
    ' crates/plotgram-core/src/layout/routing/edge_routing_orthogonal/feedback_side.rs
  )"
  if [[ -n "${prod_fb}" ]]; then
    echo "FAIL: DiagramType in feedback_side production:"
    echo "$prod_fb"
    fail=1
  fi
  ortho_code="$(echo "$ortho_code" | rg -v 'feedback_side\.rs:' || true)"
fi
if [[ -n "${ortho_code}" ]]; then
  echo "FAIL: DiagramType in edge_routing_orthogonal production:"
  echo "$ortho_code"
  fail=1
fi

# 3) algo == "architecture" 仅允许 frame_spec
algo_arch_bad="$(
  rg -n 'algo == "architecture"' crates/plotgram-core/src/layout -g '*.rs' \
    | rg -v 'recipes/frame_spec/' \
    | rg -v '^\S+:\s*//' \
    || true
)"
if [[ -n "${algo_arch_bad}" ]]; then
  echo "FAIL: algo == \"architecture\" outside recipes/frame_spec:"
  echo "$algo_arch_bad"
  fail=1
fi

# 4) G5：生产路径禁止内联 `CoordinateProblem {`（须经 ::build；kernel 测试辅助白名单）
cp_bad="$(
  rg -n 'CoordinateProblem\s*\{' crates/plotgram-core/src/layout -g '*.rs' \
    | rg -v 'pub struct CoordinateProblem' \
    | rg -v 'impl CoordinateProblem' \
    | rg -v 'kernel/coordinate/(auditor|analysis|optimizer|group_ir)\.rs:' \
    | rg -v 'kernel/coordinator\.rs:' \
    | rg -v '_tests\.rs:' \
    | rg -v '/tests\.rs:' \
    | rg -v '^\S+:\s*//' \
    | rg -v '^\S+:\s*//!' \
    | rg -v '^\S+:\s*///' \
    || true
)"
if [[ -n "${cp_bad}" ]]; then
  echo "FAIL: CoordinateProblem {{ outside G5 whitelist (use CoordinateProblem::build):"
  echo "$cp_bad"
  fail=1
fi

# 5b) G-pre：architecture 默认 Fit+None；resolve_group_frame_spec 不调 DSL
spec_fn="$(rg -A8 '^pub fn resolve_group_frame_spec' crates/plotgram-core/src/layout/recipes/frame_spec/spec.rs | head -9)"
if echo "$spec_fn" | rg -q 'resolve_from_group_frame_config\('; then
  echo "FAIL: resolve_group_frame_spec still calls resolve_from_group_frame_config"
  fail=1
fi
arch_fn="$(rg -A15 '^fn resolve_architecture' crates/plotgram-core/src/layout/recipes/frame_spec/spec.rs | head -16)"
if ! echo "$arch_fn" | rg -q 'TrackSizing::Fit'; then
  echo "FAIL: resolve_architecture must default TrackSizing::Fit"
  fail=1
fi
if ! echo "$arch_fn" | rg -q 'BorderAlign::None'; then
  echo "FAIL: resolve_architecture must default BorderAlign::None"
  fail=1
fi
if echo "$arch_fn" | rg -q 'TrackSizing::Equal|BorderAlign::SharedLines'; then
  echo "FAIL: resolve_architecture must not default Equal/SharedLines"
  fail=1
fi

# 5c) G-pre：parse_group_sizing 恒 Fit；无 subnet id 启发式
gs_fn="$(rg -A5 '^pub fn parse_group_sizing' crates/plotgram-core/src/layout/recipes/architecture/group_sizing.rs | head -6)"
if ! echo "$gs_fn" | rg -q 'GroupSizingPolicy::Fit'; then
  echo "FAIL: parse_group_sizing must return Fit"
  fail=1
fi
if rg -n 'fn architecture_subnet_layout_hint' crates/plotgram-core/src/layout/recipes/architecture/group_layout_hint.rs \
  | rg -v '^\S+:\s*//' | rg -q .; then
  echo "FAIL: architecture_subnet_layout_hint must be removed (G-pre)"
  fail=1
fi

mut_sigs="$(
  rg -n 'pub(\([^)]*\))?\s+fn\s+\w+[^{]*&mut\s*\[EdgeLayout\]' \
    crates/plotgram-core/src/layout -g '*.rs' \
    || true
)"
mut_count="$(echo "$mut_sigs" | sed '/^$/d' | wc -l | tr -d ' ')"
MAX_MUT=3
if [[ "${mut_count}" -gt "${MAX_MUT}" ]]; then
  echo "FAIL: pub fn &mut [EdgeLayout] count=${mut_count} > ${MAX_MUT}"
  echo "$mut_sigs"
  fail=1
else
  echo "OK: pub fn &mut [EdgeLayout] count=${mut_count} (≤${MAX_MUT})"
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "OK: semantic isolation"
