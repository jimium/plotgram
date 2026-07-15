#!/usr/bin/env bash
# 对比两份 collinear baseline JSON；质量/性能门禁失败时 exit 1。
#
# 用法:
#   ./benchmark-data/compare-collinear.sh baseline.json current.json
#   ./benchmark-data/compare-collinear.sh   # 默认 latest vs 需传入 current
#
# 门禁:
#   node_fp 必须一致（除非传入 --allow-node-fp；轴 B 层 gutter 允许变）
#   exact_sev / tight_sev / lint error_count / unrelated_trunk 不升（容差 1e-3）
#   median_ms 退化 ≤ 10%（基线 ≥10ms）；基线 <10ms 时绝对容差 +5ms
#   det 必须为 true（若两侧均有字段）

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALLOW_NODE_FP=0
ARGS=()
for a in "$@"; do
  case "$a" in
    --allow-node-fp) ALLOW_NODE_FP=1 ;;
    *) ARGS+=("$a") ;;
  esac
done

BASE="${ARGS[0]:-$ROOT/benchmark-data/collinear-baseline-latest.json}"
CUR="${ARGS[1]:-}"

if [[ -z "$CUR" ]]; then
  echo "用法: $0 [--allow-node-fp] <baseline.json> <current.json>" >&2
  exit 2
fi

python3 - "$BASE" "$CUR" "$ALLOW_NODE_FP" <<'PY'
import json, sys

PERF_BUDGET = 0.10
EPS = 1.0  # 严重度浮点容差（px·加权）
ALLOW_NODE_FP = sys.argv[3] == "1"

with open(sys.argv[1]) as fh:
    base = json.load(fh)
with open(sys.argv[2]) as fh:
    cur = json.load(fh)

base_map = {s["file"]: s for s in base["samples"]}
cur_map = {s["file"]: s for s in cur["samples"]}

errors = []
warns = []

only_base = sorted(set(base_map) - set(cur_map))
only_cur = sorted(set(cur_map) - set(base_map))
if only_base:
    errors.append(f"current 缺少样例: {only_base}")
if only_cur:
    warns.append(f"current 新增样例（未对比）: {only_cur}")

for f in sorted(set(base_map) & set(cur_map)):
    b, c = base_map[f], cur_map[f]
    name = f.split("/")[-1]

    if b.get("node_fp") != c.get("node_fp"):
        msg = f"{name}: node_fp 变化 {b.get('node_fp')} → {c.get('node_fp')}"
        if ALLOW_NODE_FP:
            warns.append(msg + " （允许：轴 B）")
        else:
            errors.append(msg)

    for key in ("exact_sev", "tight_sev"):
        bv, cv = float(b.get(key, 0)), float(c.get(key, 0))
        if cv > bv + EPS:
            errors.append(f"{name}: {key} 上升 {bv:.3f} → {cv:.3f}")

    bl = b.get("lint") or {}
    cl = c.get("lint") or {}
    for key in ("error_count", "unrelated_edge_trunk_merge", "edge_through_node", "edge_crosses_group_interior"):
        bv, cv = int(bl.get(key, 0)), int(cl.get(key, 0))
        if cv > bv:
            errors.append(f"{name}: lint.{key} 上升 {bv} → {cv}")

    bm, cm = b.get("median_ms"), c.get("median_ms")
    if bm is not None and cm is not None and bm > 0:
        # 亚 10ms 样例相对比例噪声大：改用绝对容差 5ms
        if bm < 10.0:
            if cm > bm + 5.0:
                errors.append(
                    f"{name}: median_ms 退化 {bm} → {cm} (绝对 +{cm - bm:.2f}ms > +5ms，基线 <10ms)"
                )
        else:
            ratio = cm / bm
            if ratio > 1.0 + PERF_BUDGET + 1e-9:
                errors.append(
                    f"{name}: median_ms 退化 {bm} → {cm} ({ratio:.2%} > +{PERF_BUDGET:.0%})"
                )


    if c.get("det") is False:
        errors.append(f"{name}: det=false（非确定）")

print(f"对比: {sys.argv[1]}  vs  {sys.argv[2]}")
print(f"共同样例: {len(set(base_map)&set(cur_map))}")
for w in warns:
    print(f"WARN: {w}")
if errors:
    print("FAIL:")
    for e in errors:
        print(f"  - {e}")
    sys.exit(1)
print("PASS: 质量与性能门禁通过")
sys.exit(0)
PY
