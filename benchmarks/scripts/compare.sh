#!/usr/bin/env bash
# v2 门禁比对：角色分轨棘轮（showcase-redesign-2026-08.md §8.1）。
#
# 用法:
#   ./benchmarks/compare.sh baseline.json current.json
#   ./benchmarks/compare.sh --strict-stress baseline.json current.json
#
# 门禁分层（按角色，基于 path 键比对）:
#   正确性轨（硬，全角色）:
#     - status != "ok"
#     - correctness.det == false
#     - node_overlap_count > 0
#     - edge_crosses_group_interior > 0
#     - label_overlap_count > 0
#   质量轨（按角色）:
#     product / smoke  → 硬 FAIL（任何质量指标恶化）
#     stress            → 默认 WARN（--strict-stress 改硬）
#     demo              → 默认 WARN
#     mech              → 跳过质量门禁（仅正确性）
#
# 恶化判定（棘轮：current 不得比 baseline 差）:
#   - edge_crossing_count: 整数计数上升（容差 0）
#   - total_edge_length:   浮点上升（容差 1.0 px）
#   - canvas_area:         浮点上升（容差 10.0 px²）
#   - aspect_ratio:        浮点上升（容差 0.05；>=1，越大越差）
#   改进（current 更好）只记 INFO，不阻门禁。
#
# Exit code:
#   0 = clean pass（无硬退化、无 WARN）
#   1 = 硬退化（任何角色）
#   2 = 仅 WARN（无硬退化；存在 soft WARN）
#   3 = 用法错误 / 文件读取失败
#
# 抬基线 note 强制带 layout/role（§8.2）:
#   raise hierarchical/smoke: …原因…；残余: hierarchical/smoke.foo

set -uo pipefail

STRICT_STRESS=0
ARGS=()
for a in "$@"; do
  case "$a" in
    --strict-stress) STRICT_STRESS=1 ;;
    -h|--help)
      sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) ARGS+=("$a") ;;
  esac
done

if [[ ${#ARGS[@]} -lt 2 ]]; then
  echo "用法: $0 [--strict-stress] <baseline.json> <current.json>" >&2
  exit 3
fi

BASE="${ARGS[0]}"
CUR="${ARGS[1]}"

[[ -f "$BASE" ]] || { echo "error: baseline 不存在: $BASE" >&2; exit 3; }
[[ -f "$CUR" ]]  || { echo "error: current 不存在: $CUR"  >&2; exit 3; }

python3 - "$BASE" "$CUR" "$STRICT_STRESS" <<'PY'
import json, sys

base_path, cur_path, strict_stress = sys.argv[1], sys.argv[2], sys.argv[3] == "1"

with open(base_path) as fh:
    base = json.load(fh)
with open(cur_path) as fh:
    cur = json.load(fh)

# schema_version 校验（双方都要有，且相等）
bv = base.get("schema_version")
cv = cur.get("schema_version")
if bv is None or cv is None:
    print("WARN: 缺 schema_version 字段（旧 v1 baseline？）", file=sys.stderr)
elif bv != cv:
    print(f"error: schema_version 不匹配 (baseline={bv}, current={cv})", file=sys.stderr)
    sys.exit(3)

base_samples = base.get("samples", [])
cur_samples  = cur.get("samples", [])

# 按 path 键索引（path 是 layout-prefixed showcase-relative 路径，唯一）
base_map = {}
for s in base_samples:
    p = s.get("path")
    if p is not None:
        base_map[p] = s
cur_map = {}
for s in cur_samples:
    p = s.get("path")
    if p is not None:
        cur_map[p] = s

ROLE_ORDER = ["smoke", "product", "demo", "stress", "mech"]
def role_of(s):
    r = s.get("role")
    if r in ROLE_ORDER:
        return r
    # 兜底：按文件名推导
    name = (s.get("path") or "").rsplit("/", 1)[-1]
    head = name.split(".")[0]
    return head if head in ROLE_ORDER else "product"

# 棘轮容差
EPS_LEN     = 1.0    # total_edge_length
EPS_AREA    = 10.0   # canvas_area
EPS_ASPECT  = 0.05   # aspect_ratio

# 质量轨是否硬 fail
def quality_is_hard(role):
    if role in ("product", "smoke"):
        return True
    if role == "stress" and strict_stress:
        return True
    return False

# 收集结果
hard_fail = []     # 硬退化（exit 1）
soft_warn = []     # 软退化（exit 2）
infos = []         # 改进 / 信息
new_samples = []   # current 新增
missing_samples = []  # current 缺失

only_base = sorted(set(base_map) - set(cur_map))
only_cur  = sorted(set(cur_map) - set(base_map))

for p in only_base:
    s = base_map[p]
    role = role_of(s)
    missing_samples.append((p, role))
for p in only_cur:
    s = cur_map[p]
    role = role_of(s)
    new_samples.append((p, role))

# 缺失/新增样例 = INFO（CI 可能只跑 smoke+product 子集；本地全量 snapshot 才会真正丢覆盖）
# 不进 WARN/FAIL，避免 subset CI 假阳性。
for p, role in missing_samples:
    infos.append(f"{p}: current 缺失 (role={role})（CI 子集跑属正常；本地全量跑若仍缺，请显式删除）")
for p, role in new_samples:
    infos.append(f"{p}: current 新增 (role={role})（首次基线后纳入下次比对）")

# 比对公共样例
for p in sorted(set(base_map) & set(cur_map)):
    b, c = base_map[p], cur_map[p]
    name = p.rsplit("/", 1)[-1]
    role = role_of(c)

    # ── 正确性轨（全角色硬）──
    bs = b.get("status", "ok")
    cs = c.get("status", "ok")
    if cs != "ok":
        hard_fail.append(f"{name}: status={cs} (role={role})" +
                          (f" (baseline={bs})" if bs != cs else ""))
    else:
        bc = b.get("correctness") or {}
        cc = c.get("correctness") or {}
        # det 必须 true
        bd = bc.get("det")
        cd = cc.get("det")
        if cd is not True:
            hard_fail.append(f"{name}: det={cd} (role={role})")
        # overlap 计数：棘轮，current 不得 > baseline；且 baseline=0 时 current 必须仍 0
        for key in ("node_overlap_count", "edge_crosses_group_interior", "label_overlap_count"):
            bv = int(bc.get(key) or 0)
            cv = int(cc.get(key) or 0)
            if cv > bv:
                hard_fail.append(f"{name}: {key} 上升 {bv} → {cv} (role={role})")
            elif cv < bv:
                infos.append(f"{name}: {key} 下降 {bv} → {cv}（正确性改进）")

    # ── 质量轨（按角色分级）──
    # mech 跳过
    if role == "mech":
        continue
    bq = b.get("quality") or {}
    cq = c.get("quality") or {}
    # 若 current 解析/渲染错，quality 可能为 null；上面正确性轨已硬 fail，这里跳过
    if cq is None:
        continue

    def check_float(key, eps, label):
        bv = float(bq.get(key) or 0) if bq.get(key) is not None else None
        cv = float(cq.get(key) or 0) if cq.get(key) is not None else None
        if cv is None or bv is None:
            return
        if cv > bv + eps:
            msg = f"{name}: {label} 上升 {bv:.3f} → {cv:.3f} (role={role}, +{cv-bv:.3f})"
            if quality_is_hard(role):
                hard_fail.append(msg)
            else:
                soft_warn.append(msg)
        elif cv < bv - eps:
            infos.append(f"{name}: {label} 下降 {bv:.3f} → {cv:.3f}（改进）")

    def check_int(key, label):
        bv = int(bq.get(key) or 0) if bq.get(key) is not None else 0
        cv = int(cq.get(key) or 0) if cq.get(key) is not None else 0
        if cv > bv:
            msg = f"{name}: {label} 上升 {bv} → {cv} (role={role})"
            if quality_is_hard(role):
                hard_fail.append(msg)
            else:
                soft_warn.append(msg)
        elif cv < bv:
            infos.append(f"{name}: {label} 下降 {bv} → {cv}（改进）")

    check_int("edge_crossing_count", "edge_crossing_count")
    check_float("total_edge_length", EPS_LEN, "total_edge_length")
    check_float("canvas_area",       EPS_AREA, "canvas_area")
    check_float("aspect_ratio",      EPS_ASPECT, "aspect_ratio")

# ── 报告 ──
print(f"对比: {base_path}")
print(f"  vs: {cur_path}")
common = len(set(base_map) & set(cur_map))
print(f"共同样例: {common}; baseline 独有: {len(missing_samples)}; current 独有: {len(new_samples)}")

# 角色分布
from collections import Counter
role_counts = Counter(role_of(s) for s in cur_samples)
roles_present = [r for r in ROLE_ORDER if r in role_counts]
print(f"角色分布 (current): " + ", ".join(f"{r}={role_counts[r]}" for r in roles_present))
print()

if infos:
    print("--- INFO（改进 / 信息）---")
    for s in infos:
        print(f"  - {s}")
    print()

if soft_warn:
    print("--- WARN（软退化，非阻断）---")
    for s in soft_warn:
        print(f"  WARN: {s}")
    print()

print("--- 正确性轨（硬：status / det / overlap，全角色）---")
if hard_fail:
    print("FAIL:")
    for e in hard_fail:
        print(f"  - {e}")
else:
    print("PASS")

# ── Exit code ──
if hard_fail:
    sys.exit(1)
if soft_warn:
    print()
    print("RESULT: WARN-only（exit 2）")
    sys.exit(2)
print()
print("RESULT: clean PASS（exit 0）")
sys.exit(0)
PY
