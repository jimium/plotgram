#!/usr/bin/env bash
# 共线方案基线快照：质量（collinear-baseline）+ 性能（bench-phases）+ 确定性
#
# 用法（仓库根目录）:
#   ./benchmark-data/snapshot-collinear.sh
#   ./benchmark-data/snapshot-collinear.sh --runs 5
#   ./benchmark-data/snapshot-collinear.sh --set benchmark-data/product-regression-set.txt
#   ./benchmark-data/snapshot-collinear.sh --set benchmark-data/product-regression-set.txt \
#                                         --set benchmark-data/stress-probe-set.txt
#
# 默认采集 product-gate + stress-probe（角色感知 baseline）。
# 每条样本在 JSON 内补 role 字段（取文件名第一段：product/stress/demo/mech/smoke）。
#
# 输出:
#   benchmark-data/collinear-baseline-YYYY-MM-DD.json
#   benchmark-data/collinear-baseline-YYYY-MM-DD.md
#   benchmark-data/collinear-baseline-latest.{json,md}

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUNS=5
DEFAULT_SET_FILES=("$ROOT/benchmark-data/product-regression-set.txt" "$ROOT/benchmark-data/stress-probe-set.txt")
SET_FILES=()
DATE="$(date +%Y-%m-%d)"
JSON_OUT="$ROOT/benchmark-data/collinear-baseline-${DATE}.json"
MD_OUT="$ROOT/benchmark-data/collinear-baseline-${DATE}.md"
JSON_LATEST="$ROOT/benchmark-data/collinear-baseline-latest.json"
MD_LATEST="$ROOT/benchmark-data/collinear-baseline-latest.md"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --runs) RUNS="$2"; shift 2 ;;
    --set)
      SET_FILES+=("$2")
      shift 2
      ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

# 用户未传 --set 时使用默认集（product + stress）
if [[ ${#SET_FILES[@]} -eq 0 ]]; then
  SET_FILES=("${DEFAULT_SET_FILES[@]}")
fi

export PLOTGRAM_FONTS_DIR="${PLOTGRAM_FONTS_DIR:-$ROOT/fonts}"
# 避免 Cursor/沙箱注入的 CARGO_TARGET_DIR 把产物写到临时目录
unset CARGO_TARGET_DIR || true

echo "▶ 构建 collinear-baseline / bench-phases / plotgram (release)..."
cargo build --release -p plotgram-core --bin collinear-baseline --bin bench-phases -p plotgram-cli 2>&1 | tail -5

COLLINEAR="$ROOT/target/release/collinear-baseline"
BENCH="$ROOT/target/release/bench-phases"
PLOTGRAM="$ROOT/target/release/plotgram"

if [[ ! -x "$COLLINEAR" || ! -x "$BENCH" || ! -x "$PLOTGRAM" ]]; then
  echo "error: 缺少 release 二进制（$COLLINEAR / $BENCH / $PLOTGRAM）" >&2
  exit 1
fi

# 合并多个 --set 文件，去重，保持顺序（product 优先）
TMP_LIST="$(mktemp)"
for sf in "${SET_FILES[@]}"; do
  [[ -f "$sf" ]] || { echo "warn: 缺失 set 文件 $sf，跳过" >&2; continue; }
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${line// }" ]] && continue
    echo "$line"
  done < "$sf"
done | awk '!seen[$0]++' > "$TMP_LIST"

FILES=()
while IFS= read -r f; do
  FILES+=("$f")
done < "$TMP_LIST"

echo "▶ 共 ${#FILES[@]} 个样例（来自 ${#SET_FILES[@]} 个 set 文件）"

echo "▶ 质量指标（collinear-baseline）..."
# collinear-baseline 接受 [file.pgm ...]，跳过 --set 自动扫描，直接传文件列表
"$COLLINEAR" --runs 1 --date "$DATE" "${FILES[@]}" >"$JSON_OUT"

echo "▶ 性能 + 确定性..."
TMP_PERF="$(mktemp)"
{
  echo "{"
  first=1
  for f in "${FILES[@]}"; do
    name="$(basename "$f" .pgm)"
    if ! out="$("$BENCH" "$f" "$RUNS" 2>/dev/null)"; then
      med="null"; minv="null"; maxv="null"
    else
      med=$(echo "$out" | grep '中位数:' | sed -E 's/.*中位数:[[:space:]]*([0-9.]+)ms.*/\1/')
      minv=$(echo "$out" | grep '最小值:' | sed -E 's/.*最小值:[[:space:]]*([0-9.]+)ms.*/\1/')
      maxv=$(echo "$out" | grep '最大值:' | sed -E 's/.*最大值:[[:space:]]*([0-9.]+)ms.*/\1/')
    fi
    h1=$("$PLOTGRAM" render "$f" -f svg 2>/dev/null | shasum -a 256 | awk '{print $1}')
    h2=$("$PLOTGRAM" render "$f" -f svg 2>/dev/null | shasum -a 256 | awk '{print $1}')
    if [[ "$h1" == "$h2" && -n "$h1" ]]; then det="true"; else det="false"; fi
    if [[ $first -eq 0 ]]; then echo ","; fi
    first=0
    printf '  "%s": {"median_ms": %s, "min_ms": %s, "max_ms": %s, "det": %s}' \
      "$f" "${med:-null}" "${minv:-null}" "${maxv:-null}" "$det"
    echo "  bench $name: ${med:-FAIL}ms det=$det" >&2
  done
  echo
  echo "}"
} >"$TMP_PERF"

python3 - "$JSON_OUT" "$TMP_PERF" "$RUNS" <<'PY'
import json, sys, os
with open(sys.argv[1]) as fh:
    snap = json.load(fh)
with open(sys.argv[2]) as fh:
    perf = json.load(fh)

ROLE_ORDER = ["smoke", "product", "demo", "stress", "mech"]
def derive_role(path):
    name = path.split("/")[-1]
    head = name.split(".")[0]
    return head if head in ROLE_ORDER else "product"

for s in snap["samples"]:
    p = perf.get(s["file"], {})
    s["median_ms"] = p.get("median_ms")
    s["min_ms"] = p.get("min_ms")
    s["max_ms"] = p.get("max_ms")
    s["det"] = p.get("det")
    s["role"] = derive_role(s["file"])
snap["perf_runs"] = int(sys.argv[3])
with open(sys.argv[1], "w") as fh:
    json.dump(snap, fh, indent=2, ensure_ascii=False)
    fh.write("\n")
PY
rm -f "$TMP_PERF" "$TMP_LIST"

python3 - "$JSON_OUT" "$MD_OUT" <<'PY'
import json, sys
from collections import defaultdict
with open(sys.argv[1]) as fh:
    snap = json.load(fh)
lines = []
lines.append(f"# Collinear baseline {snap.get('date')}")
lines.append("")
lines.append(f"- note: {snap.get('note')}")
lines.append(f"- perf_runs: {snap.get('perf_runs')}")
lines.append(f"- samples: {len(snap.get('samples', []))}")
by_role = defaultdict(list)
for s in snap.get("samples", []):
    by_role[s.get("role", "product")].append(s)
for role in ["smoke", "product", "demo", "stress", "mech"]:
    if role in by_role:
        lines.append(f"  - {role}: {len(by_role[role])}")
lines.append("")
lines.append("| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |")
lines.append("|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|")
for role in ["smoke", "product", "demo", "stress", "mech"]:
    for s in by_role.get(role, []):
        name = s["file"].split("/")[-1]
        lint = s.get("lint") or {}
        lines.append(
            f"| {role} | `{name}` | {s['nodes']} | {s['edges']} | {s['exact_sev']:.1f} | {s['tight_sev']:.1f} | "
            f"{s['exact_pairs']} | {lint.get('unrelated_edge_trunk_merge', 0)} | {lint.get('error_count', 0)} | "
            f"{s.get('median_ms')} | {s.get('det')} | `{s['node_fp'][:12]}` |"
        )
lines.append("")
lines.append("复跑: `./benchmark-data/snapshot-collinear.sh`")
lines.append("对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`")
with open(sys.argv[2], "w") as fh:
    fh.write("\n".join(lines) + "\n")
PY

cp "$JSON_OUT" "$JSON_LATEST"
cp "$MD_OUT" "$MD_LATEST"
echo
echo "✓ $JSON_OUT"
echo "✓ $MD_OUT"
echo "✓ synced latest"
