#!/usr/bin/env bash
# Phase 6 / A1：路由软罚项 max/min ≤ 100；G6 项数 ≤ MAX_SOFT_ITEMS。
# 真源：crates/plotgram-core/src/layout/routing/objectives.rs 的 SOFT_RANKING。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

out="$(cargo test -p plotgram-core --lib layout::routing::objectives::tests -- --nocapture 2>&1)"
if echo "$out" | rg -q "test result: FAILED|FAILED\."; then
  echo "$out" | tail -40
  echo "FAIL: soft ranking ratio/items exceed caps (see routing/objectives.rs)"
  exit 1
fi

# 禁止在 objectives 之外再定义新的 *_PENALTY 常量（散落即债）
# 允许：objectives.rs 自身；use/re-export 行
violations="$(
  rg -n '^\s*(pub(\([^)]*\))?\s+)?const\s+\w*PENALTY\w*\s*:' \
    crates/plotgram-core/src/layout/routing \
    --glob '!**/objectives.rs' \
    || true
)"
if [[ -n "${violations}" ]]; then
  echo "FAIL: penalty consts outside routing/objectives.rs:"
  echo "$violations"
  exit 1
fi

item_count="$(
  rg -n '^\s*\("[A-Z_]+_PENALTY"' crates/plotgram-core/src/layout/routing/objectives.rs | wc -l | tr -d ' '
)"
MAX_ITEMS="${PENALTY_ITEM_MAX:-13}"
if (( item_count > MAX_ITEMS )); then
  echo "FAIL: SOFT_RANKING item_count=${item_count} > ${MAX_ITEMS}"
  exit 1
fi

echo "OK: penalty ratio + item_count=${item_count} (≤${MAX_ITEMS}) + single registry"
