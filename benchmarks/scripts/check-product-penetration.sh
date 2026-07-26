#!/usr/bin/env bash
# Post-S7：轻量 product 正确性门禁——对白名单样例跑 lint，断言穿组=0。
# 全量 compare.sh 仍建议本地/ nightly；本脚本适合 PR CI（debug、无 release）。
# 注意：lint 可能因其它规则（node_overlap 等）非 0 退出；本门禁只读穿组计数。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=benchmarks/scripts/gate-switch.sh
source "$(dirname "$0")/gate-switch.sh"
gate_skip_unless_enabled "check-product-penetration.sh"

SAMPLES=(
  showcase/flowchart/product.linear-chain.pgm
  showcase/flowchart/product.user-auth.pgm
  showcase/flowchart/product.swimlane-order-process.pgm
  showcase/architecture/product.ecommerce-platform.pgm
  showcase/architecture/product.cloud-native.pgm
)

fail=0
for s in "${SAMPLES[@]}"; do
  if [[ ! -f "$s" ]]; then
    echo "FAIL: missing $s"
    fail=1
    continue
  fi
  set +e
  out="$(cargo run -q -p plotgram-cli -- lint "$s" --profile strict --format json 2>/dev/null)"
  rc=$?
  set -e
  if [[ -z "${out}" ]]; then
    echo "FAIL: empty lint JSON for $s (exit=${rc})"
    fail=1
    continue
  fi
  crosses="$(
    python3 -c "
import json,sys
d=json.load(sys.stdin)
vs=d.get('violations', d if isinstance(d,list) else [])
print(sum(1 for v in vs if v.get('rule')=='edge_crosses_group_interior'))
" <<<"$out"
  )"
  if [[ "${crosses}" != "0" ]]; then
    echo "FAIL: $s edge_crosses_group_interior=${crosses}"
    fail=1
  else
    echo "OK: $s penetration=0"
  fi
done

if (( fail != 0 )); then
  exit 1
fi
echo "OK: product penetration gate (${#SAMPLES[@]} samples)"
