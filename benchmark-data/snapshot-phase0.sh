#!/usr/bin/env bash
# Phase 0 基线快照：lint + bench-phases + 确定性 hash
#
# 用法（仓库根目录）:
#   ./benchmark-data/snapshot-phase0.sh
#   ./benchmark-data/snapshot-phase0.sh --runs 5
#
# 输出:
#   benchmark-data/phase0-YYYY-MM-DD.md
#   benchmark-data/phase0-latest.md

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUNS=3
SET_FILE="$ROOT/benchmark-data/phase0-regression-set.txt"
DATE="$(date +%Y-%m-%d)"
OUT="$ROOT/benchmark-data/phase0-${DATE}.md"
LATEST="$ROOT/benchmark-data/phase0-latest.md"
TMPDIR_SNAP="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_SNAP"' EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --runs) RUNS="$2"; shift 2 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

export PLOTGRAM_FONTS_DIR="${PLOTGRAM_FONTS_DIR:-$ROOT/fonts}"

echo "▶ 构建 plotgram / bench-phases (release)..."
cargo build --release -p plotgram-cli -p plotgram-core --bin bench-phases 2>&1 | tail -3

PLOTGRAM="$ROOT/target/release/plotgram"
BENCH="$ROOT/target/release/bench-phases"

FILES=()
while IFS= read -r line || [[ -n "$line" ]]; do
  [[ "$line" =~ ^[[:space:]]*# ]] && continue
  [[ -z "${line// }" ]] && continue
  FILES+=("$line")
done < "$SET_FILE"

{
  echo "# Phase 0 回归基线快照"
  echo
  echo "- 日期: ${DATE}"
  echo "- 默认行为: architecture \`group_frame\` = **Fit**（Equal 需显式 \`group_sizing: uniform\`）"
  echo "- I1–I3 / corridor A1+A2: 已落地（见 layout-routing-optimization-proposal-2026-07.md）"
  echo "- 样例集: \`benchmark-data/phase0-regression-set.txt\`"
  echo "- 复跑: \`./benchmark-data/snapshot-phase0.sh\`"
  echo
  echo "## Lint 摘要"
  echo
  echo "| 样例 | errors | warnings | edge_through_node | edge_crosses_group_interior | sibling_width_ratio |"
  echo "|------|-------:|---------:|------------------:|----------------------------:|--------------------:|"
} > "$OUT"

for f in "${FILES[@]}"; do
  name="$(basename "$f" .pgm)"
  jf="$TMPDIR_SNAP/${name}.lint.json"
  set +e
  "$PLOTGRAM" lint "$f" --format json >"$jf" 2>/dev/null
  set -e
  if [[ ! -s "$jf" ]]; then
    echo "| \`$name\` | FAIL | - | - | - | - |" >> "$OUT"
    continue
  fi
  read -r errors warnings etn ecgi swr < <(python3 - "$jf" <<'PY'
import json, sys
with open(sys.argv[1]) as fh:
    d = json.load(fh)
violations = d.get("violations") or []
errors = sum(1 for v in violations if str(v.get("severity", "")).lower() == "error")
warnings = sum(1 for v in violations if str(v.get("severity", "")).lower() != "error")

def count(rule):
    return sum(1 for v in violations if v.get("rule") == rule)

print(errors, warnings, count("edge_through_node"), count("edge_crosses_group_interior"), count("sibling_width_ratio"))
PY
)
  echo "| \`$name\` | $errors | $warnings | $etn | $ecgi | $swr |" >> "$OUT"
  echo "  lint $name: e=$errors w=$warnings"
done

{
  echo
  echo "## bench-phases（布局+路由中位数，${RUNS} 轮）"
  echo
  echo "| 样例 | nodes | edges | groups | median_ms | min_ms | max_ms |"
  echo "|------|-------:|------:|-------:|----------:|-------:|-------:|"
} >> "$OUT"

for f in "${FILES[@]}"; do
  name="$(basename "$f" .pgm)"
  if ! out="$("$BENCH" "$f" "$RUNS" 2>/dev/null)"; then
    echo "| \`$name\` | - | - | - | FAIL | - | - |" >> "$OUT"
    continue
  fi
  nodes=$(echo "$out" | grep -m1 '节点:' | sed -E 's/.*节点: ([0-9]+).*/\1/')
  edges=$(echo "$out" | grep -m1 '节点:' | sed -E 's/.*边: ([0-9]+).*/\1/')
  groups=$(echo "$out" | grep -m1 '节点:' | sed -E 's/.*分组: ([0-9]+).*/\1/')
  median=$(echo "$out" | grep '中位数:' | sed -E 's/.*中位数:[[:space:]]*([0-9.]+)ms.*/\1/')
  minv=$(echo "$out" | grep '最小值:' | sed -E 's/.*最小值:[[:space:]]*([0-9.]+)ms.*/\1/')
  maxv=$(echo "$out" | grep '最大值:' | sed -E 's/.*最大值:[[:space:]]*([0-9.]+)ms.*/\1/')
  echo "| \`$name\` | $nodes | $edges | $groups | $median | $minv | $maxv |" >> "$OUT"
  echo "  bench $name: ${median}ms"
done

{
  echo
  echo "## 确定性（连续两次 SVG hash）"
  echo
  echo "| 样例 | hash1 == hash2 |"
  echo "|------|:--------------:|"
} >> "$OUT"

for f in "${FILES[@]}"; do
  name="$(basename "$f" .pgm)"
  h1=$("$PLOTGRAM" render "$f" -f svg 2>/dev/null | shasum -a 256 | awk '{print $1}')
  h2=$("$PLOTGRAM" render "$f" -f svg 2>/dev/null | shasum -a 256 | awk '{print $1}')
  if [[ "$h1" == "$h2" && -n "$h1" ]]; then
    echo "| \`$name\` | yes |" >> "$OUT"
  else
    echo "| \`$name\` | NO |" >> "$OUT"
  fi
  echo "  det $name: $([[ "$h1" == "$h2" ]] && echo ok || echo FAIL)"
done

{
  echo
  echo "## 说明"
  echo
  echo "- 本快照为后续 Phase 1–5 对照基线；**不改变算法行为**。"
  echo "- 与历史单图基线 \`baseline.md\`（tenant-isolation）并存。"
  echo "- 回链: \`docs/architecture/重构方案/render-layout-routing-baseline-2026-07.md\`"
} >> "$OUT"

cp "$OUT" "$LATEST"
echo
echo "✓ 写入 $OUT"
echo "✓ 同步 $LATEST"
