#!/usr/bin/env bash
# 对比两份 collinear baseline JSON；按正确性轨 / 质量轨分层报告。
#
# 用法:
#   ./benchmark-data/compare-collinear.sh baseline.json current.json
#   ./benchmark-data/compare-collinear.sh --allow-node-fp baseline.json current.json
#   ./benchmark-data/compare-collinear.sh --allow-quality-debt baseline.json current.json
#
# 门禁分层（C4 / P4）:
#   正确性轨（硬）：edge_crosses_group_interior 不升；det 必须为 true
#   质量轨（软/可债）：exact_sev / tight_sev / error_count / through /
#                     unrelated_trunk / median_ms；node_fp（可用 --allow-node-fp）
#   默认：两轨任一 FAIL → exit 1
#   --allow-quality-debt：仅正确性轨硬失败；质量轨 FAIL 打印为债（exit 0）
#
# 读法示例（group↓、sev↑）:
#   正确性轨 PASS: lint.edge_crosses_group_interior 2 → 1
#   质量轨 FAIL（债）: exact_sev 上升 … → 须 note 残余边后显式抬基线

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALLOW_NODE_FP=0
ALLOW_QUALITY_DEBT=0
ARGS=()
for a in "$@"; do
  case "$a" in
    --allow-node-fp) ALLOW_NODE_FP=1 ;;
    --allow-quality-debt) ALLOW_QUALITY_DEBT=1 ;;
    *) ARGS+=("$a") ;;
  esac
done

BASE="${ARGS[0]:-$ROOT/benchmark-data/collinear-baseline-latest.json}"
CUR="${ARGS[1]:-}"

if [[ -z "$CUR" ]]; then
  echo "用法: $0 [--allow-node-fp] [--allow-quality-debt] <baseline.json> <current.json>" >&2
  exit 2
fi

python3 - "$BASE" "$CUR" "$ALLOW_NODE_FP" "$ALLOW_QUALITY_DEBT" <<'PY'
import json, sys

PERF_BUDGET = 0.10
EPS = 1.0  # 严重度浮点容差（px·加权）
ALLOW_NODE_FP = sys.argv[3] == "1"
ALLOW_QUALITY_DEBT = sys.argv[4] == "1"

with open(sys.argv[1]) as fh:
    base = json.load(fh)
with open(sys.argv[2]) as fh:
    cur = json.load(fh)

base_map = {s["file"]: s for s in base["samples"]}
cur_map = {s["file"]: s for s in cur["samples"]}

correctness = []
quality = []
warns = []

only_base = sorted(set(base_map) - set(cur_map))
only_cur = sorted(set(cur_map) - set(base_map))
if only_base:
    correctness.append(f"current 缺少样例: {only_base}")
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
            quality.append(msg)

    for key in ("exact_sev", "tight_sev"):
        bv, cv = float(b.get(key, 0)), float(c.get(key, 0))
        if cv > bv + EPS:
            quality.append(f"{name}: {key} 上升 {bv:.3f} → {cv:.3f}")

    bl = b.get("lint") or {}
    cl = c.get("lint") or {}

    # 正确性轨：穿组
    bv, cv = int(bl.get("edge_crosses_group_interior", 0)), int(
        cl.get("edge_crosses_group_interior", 0)
    )
    if cv > bv:
        correctness.append(
            f"{name}: lint.edge_crosses_group_interior 上升 {bv} → {cv}"
        )
    elif cv < bv:
        warns.append(
            f"{name}: lint.edge_crosses_group_interior 下降 {bv} → {cv}（正确性改进）"
        )

    for key in ("error_count", "unrelated_edge_trunk_merge", "edge_through_node"):
        bv, cv = int(bl.get(key, 0)), int(cl.get(key, 0))
        if cv > bv:
            quality.append(f"{name}: lint.{key} 上升 {bv} → {cv}")

    bm, cm = b.get("median_ms"), c.get("median_ms")
    if bm is not None and cm is not None and bm > 0:
        if bm < 10.0:
            if cm > bm + 5.0:
                quality.append(
                    f"{name}: median_ms 退化 {bm} → {cm} (绝对 +{cm - bm:.2f}ms > +5ms，基线 <10ms)"
                )
        else:
            ratio = cm / bm
            if ratio > 1.0 + PERF_BUDGET + 1e-9:
                quality.append(
                    f"{name}: median_ms 退化 {bm} → {cm} ({ratio:.2%} > +{PERF_BUDGET:.0%})"
                )

    if c.get("det") is False:
        correctness.append(f"{name}: det=false（非确定）")

print(f"对比: {sys.argv[1]}  vs  {sys.argv[2]}")
print(f"共同样例: {len(set(base_map)&set(cur_map))}")
for w in warns:
    print(f"WARN: {w}")

print("--- 正确性轨（硬：穿组 / 确定性）---")
if correctness:
    print("FAIL:")
    for e in correctness:
        print(f"  - {e}")
else:
    print("PASS")

print("--- 质量轨（软/可债：sev / through / trunk / perf / node_fp）---")
if quality:
    label = "FAIL（债）" if ALLOW_QUALITY_DEBT else "FAIL"
    print(f"{label}:")
    for e in quality:
        print(f"  - {e}")
else:
    print("PASS")

if correctness:
    sys.exit(1)
if quality and not ALLOW_QUALITY_DEBT:
    sys.exit(1)
if quality and ALLOW_QUALITY_DEBT:
    print("PASS: 正确性轨通过；质量轨债已显式允许（--allow-quality-debt）")
    sys.exit(0)
print("PASS: 正确性与质量门禁通过")
sys.exit(0)
PY
