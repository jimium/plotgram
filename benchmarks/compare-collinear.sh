#!/usr/bin/env bash
# 对比两份 collinear baseline JSON；按正确性轨 / 质量轨分层报告。
#
# 用法:
#   ./benchmark-data/compare-collinear.sh baseline.json current.json
#   ./benchmark-data/compare-collinear.sh --allow-node-fp baseline.json current.json
#   ./benchmark-data/compare-collinear.sh --allow-quality-debt baseline.json current.json
#   ./benchmark-data/compare-collinear.sh --strict-stress baseline.json current.json
#
# 产品验收语言（Allowed / NeedsSeparation / Degraded）见:
#   docs/architecture/方案计划/collinear-and-arrow-merge-comparison.md §2
# 成功标准是「严重度与可归类」，不是「共线计数归零」。
#
# 门禁分层（角色感知）:
#   正确性轨（硬，全角色）: edge_crosses_group_interior 不升；det 必须为 true
#   质量轨（按角色）:
#     product / smoke  → 硬 FAIL
#     stress           → 默认 WARN（观测，不挡）；--strict-stress 时改硬 FAIL
#     demo             → 默认 WARN（可债）
#     mech             → 默认不门禁（仅正确性）
#   观测（WARN，不单独失败）:
#     allowed_share_len 可升；若 allowed↑ 且 exact 未降 → 提示抽检误标
#   默认: 正确性 FAIL → exit 1；product 质量 FAIL → exit 1
#   --allow-quality-debt: 所有质量轨转 WARN（exit 0，显式债）
#   --strict-stress: stress 质量轨也走硬 FAIL
#
# 抬基线 note 强制带角色（手册 §1）:
#   raise product: …原因…；残余: c.foo
#   raise stress (expected): …探针可接受…；残余: x.layout-stress-nested
#
# 读法示例（group↓、sev↑）:
#   正确性轨 PASS: lint.edge_crosses_group_interior 2 → 1
#   质量轨 FAIL（product）: exact_sev 上升 … → 须 note 残余边后显式抬基线
#   质量轨 WARN（stress）: exact_sev 上升 … → 探针可接受；提示抽检

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALLOW_NODE_FP=0
ALLOW_QUALITY_DEBT=0
STRICT_STRESS=0
ARGS=()
for a in "$@"; do
  case "$a" in
    --allow-node-fp) ALLOW_NODE_FP=1 ;;
    --allow-quality-debt) ALLOW_QUALITY_DEBT=1 ;;
    --strict-stress) STRICT_STRESS=1 ;;
    *) ARGS+=("$a") ;;
  esac
done

BASE="${ARGS[0]:-$ROOT/benchmark-data/collinear-baseline-latest.json}"
CUR="${ARGS[1]:-}"

if [[ -z "$CUR" ]]; then
  echo "用法: $0 [--allow-node-fp] [--allow-quality-debt] [--strict-stress] <baseline.json> <current.json>" >&2
  exit 2
fi

python3 - "$BASE" "$CUR" "$ALLOW_NODE_FP" "$ALLOW_QUALITY_DEBT" "$STRICT_STRESS" <<'PY'
import json, sys

PERF_BUDGET = 0.10
EPS = 1.0  # 严重度浮点容差（px·加权）
ALLOW_NODE_FP = sys.argv[3] == "1"
ALLOW_QUALITY_DEBT = sys.argv[4] == "1"
STRICT_STRESS = sys.argv[5] == "1"

ROLE_ORDER = ["smoke", "product", "demo", "stress", "mech"]
def derive_role(path):
    name = path.split("/")[-1]
    head = name.split(".")[0]
    return head if head in ROLE_ORDER else "product"

# 质量轨是否硬 fail 的角色判定
def quality_is_hard(role):
    if ALLOW_QUALITY_DEBT:
        return False
    if role in ("product", "smoke"):
        return True
    if role == "stress" and STRICT_STRESS:
        return True
    return False

with open(sys.argv[1]) as fh:
    base = json.load(fh)
with open(sys.argv[2]) as fh:
    cur = json.load(fh)

# 兼容旧 baseline：缺 role 字段时按文件名推导
for snap in (base, cur):
    for s in snap.get("samples", []):
        if "role" not in s:
            s["role"] = derive_role(s["file"])

base_map = {s["file"]: s for s in base["samples"]}
cur_map = {s["file"]: s for s in cur["samples"]}

correctness = []      # 硬 fail（全角色）
quality_hard = []     # 硬 fail（product/smoke；或 stress 当 --strict-stress）
quality_soft = []     # WARN（stress/demo；或 product 当 --allow-quality-debt）
warns = []

# 缺样例按角色归类
def role_of(f):
    return base_map.get(f, cur_map.get(f, {})).get("role", "product")

only_base = sorted(set(base_map) - set(cur_map))
only_cur = sorted(set(cur_map) - set(base_map))
if only_base:
    for f in only_base:
        correctness.append(f"{f}: current 缺少样例（role={role_of(f)}）")
if only_cur:
    for f in only_cur:
        warns.append(f"{f}: current 新增样例（未对比，role={role_of(f)}）")

for f in sorted(set(base_map) & set(cur_map)):
    b, c = base_map[f], cur_map[f]
    name = f.split("/")[-1]
    role = c.get("role", "product")

    if b.get("node_fp") != c.get("node_fp"):
        msg = f"{name}: node_fp 变化 {b.get('node_fp')} → {c.get('node_fp')}"
        if ALLOW_NODE_FP:
            warns.append(msg + " （允许：轴 B）")
        else:
            (quality_hard if role in ("product", "smoke") else quality_soft).append(msg)

    be = float(b.get("exact_sev", 0) or 0)
    ce = float(c.get("exact_sev", 0) or 0)
    bt = float(b.get("tight_sev", 0) or 0)
    ct = float(c.get("tight_sev", 0) or 0)
    if ce > be + EPS:
        msg = f"{name}: exact_sev 上升 {be:.3f} → {ce:.3f} (role={role})"
        (quality_hard if quality_is_hard(role) else quality_soft).append(msg)
    if ct > bt + EPS:
        msg = f"{name}: tight_sev 上升 {bt:.3f} → {ct:.3f} (role={role})"
        (quality_hard if quality_is_hard(role) else quality_soft).append(msg)

    # allowed_share_len：合法合流可升（观测）；与 exact 交叉提示误标风险
    ba = float(b.get("allowed_share_len", 0) or 0)
    ca = float(c.get("allowed_share_len", 0) or 0)
    if ca > ba + EPS:
        warns.append(
            f"{name}: allowed_share_len 上升 {ba:.3f} → {ca:.3f}"
            f"（Allowed 可升；对应产品「有意合流」，role={role}）"
        )
        if ce >= be - EPS:
            warns.append(
                f"{name}: allowed↑ 但 exact_sev 未降（{be:.3f} → {ce:.3f}）"
                f"— 抽检是否误标 Allowed 掩盖 NeedsSeparation"
            )

    # ortho.degraded_count：空间不够时的可解释残余；计数不升（尚无 reason 分布）
    # 基线无 ortho（null/{}）时视为「首次接通 A6 可见性」，记 WARN 不挡——避免 null→N 假回归。
    bo_raw = b.get("ortho")
    co_raw = c.get("ortho")
    bo = bo_raw or {}
    co = co_raw or {}
    if "degraded_count" in bo or "degraded_count" in co:
        bd = int(bo.get("degraded_count") or 0)
        cd = int(co.get("degraded_count") or 0)
        baseline_missing = bo_raw is None or (
            isinstance(bo_raw, dict) and "degraded_count" not in bo_raw and not bo_raw
        )
        if baseline_missing and cd > 0:
            warns.append(
                f"{name}: ortho.degraded_count 首次可见 → {cd} (role={role})（A6 接通，非质量上升）"
            )
        elif cd > bd:
            msg = f"{name}: ortho.degraded_count 上升 {bd} → {cd} (role={role})"
            (quality_hard if quality_is_hard(role) else quality_soft).append(msg + "（Degraded 可不归零，但不得无说明地变多）")
        elif cd < bd:
            warns.append(f"{name}: ortho.degraded_count 下降 {bd} → {cd}（收敛改进）")

    bl = b.get("lint") or {}
    cl = c.get("lint") or {}

    # 正确性轨：穿组（全角色硬）
    bv, cv = int(bl.get("edge_crosses_group_interior", 0)), int(
        cl.get("edge_crosses_group_interior", 0)
    )
    if cv > bv:
        correctness.append(f"{name}: lint.edge_crosses_group_interior 上升 {bv} → {cv} (role={role})")
    elif cv < bv:
        warns.append(f"{name}: lint.edge_crosses_group_interior 下降 {bv} → {cv}（正确性改进）")

    for key in ("error_count", "unrelated_edge_trunk_merge", "edge_through_node"):
        bv, cv = int(bl.get(key, 0)), int(cl.get(key, 0))
        if cv > bv:
            msg = f"{name}: lint.{key} 上升 {bv} → {cv} (role={role})"
            (quality_hard if quality_is_hard(role) else quality_soft).append(msg)

    bm, cm = b.get("median_ms"), c.get("median_ms")
    if bm is not None and cm is not None and bm > 0:
        if bm < 10.0:
            if cm > bm + 5.0:
                msg = f"{name}: median_ms 退化 {bm} → {cm} (绝对 +{cm - bm:.2f}ms > +5ms，基线 <10ms)"
                (quality_hard if quality_is_hard(role) else quality_soft).append(msg)
        else:
            ratio = cm / bm
            if ratio > 1.0 + PERF_BUDGET + 1e-9:
                msg = f"{name}: median_ms 退化 {bm} → {cm} ({ratio:.2%} > +{PERF_BUDGET:.0%})"
                (quality_hard if quality_is_hard(role) else quality_soft).append(msg)

    if c.get("det") is False:
        correctness.append(f"{name}: det=false（非确定，role={role}）")

# 按角色汇总
from collections import defaultdict
roles_present = sorted({s.get("role", "product") for s in cur["samples"]})

print(f"对比: {sys.argv[1]}  vs  {sys.argv[2]}")
print(f"共同样例: {len(set(base_map)&set(cur_map))}")
print(f"角色分布 (current): {', '.join(f'{r}={sum(1 for s in cur[\"samples\"] if s.get(\"role\")==r)}' for r in roles_present)}")
print()
for w in warns:
    print(f"WARN: {w}")

print("--- 正确性轨（硬：穿组 / 确定性，全角色）---")
if correctness:
    print("FAIL:")
    for e in correctness:
        print(f"  - {e}")
else:
    print("PASS")

print("--- 质量轨（product/smoke 硬；stress/demo/mech 软）---")
if quality_hard:
    print("FAIL:")
    for e in quality_hard:
        print(f"  - {e}")
else:
    print("PASS (product/smoke)")
if quality_soft:
    label = "FAIL（债）" if STRICT_STRESS else "WARN（观测，不挡）"
    print(f"{label}:")
    for e in quality_soft:
        print(f"  - {e}")
else:
    print("PASS (stress/demo/mech)")

if correctness:
    sys.exit(1)
if quality_hard:
    sys.exit(1)
if quality_soft and STRICT_STRESS:
    sys.exit(1)
if quality_soft and ALLOW_QUALITY_DEBT:
    print("PASS: 正确性轨通过；质量轨债已显式允许（--allow-quality-debt）")
    sys.exit(0)
print("PASS: 正确性与 product 质量门禁通过")
sys.exit(0)
PY
