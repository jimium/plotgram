#!/usr/bin/env bash
# M7-0：跑 Ink dogleg repair 基线探针，写出 docs/altlas/31 报告。
# 不改布局算法；仅测量 hints.atlas_plan_distorted_edges。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

SET="${1:-benchmarks/sets/product-regression-set.txt}"
OUT="${2:-docs/新架构/31-Atlas-M7-0-ink-repair基线-2026-07.md}"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

# stdout=TSV；stderr 含 perf 噪声 + summary 行
cargo run -q -p plotgram-eval --bin atlas_repair_baseline -- "$SET" >"$TMP" 2>/tmp/m70-stderr.txt || {
  echo "FAIL: atlas_repair_baseline"
  cat /tmp/m70-stderr.txt >&2
  exit 1
}

SUMMARY="$(grep -E '^# summary:' /tmp/m70-stderr.txt | tail -1 || true)"
# summary: hier_measured=N repaired_diagrams=R total_distorted_edges=D
HIER="$(echo "$SUMMARY" | sed -n 's/.*hier_measured=\([0-9]*\).*/\1/p')"
REPAIRED="$(echo "$SUMMARY" | sed -n 's/.*repaired_diagrams=\([0-9]*\).*/\1/p')"
DISTORTED="$(echo "$SUMMARY" | sed -n 's/.*total_distorted_edges=\([0-9]*\).*/\1/p')"
HIER="${HIER:-0}"
REPAIRED="${REPAIRED:-0}"
DISTORTED="${DISTORTED:-0}"

DATE="$(date +%Y-%m-%d)"

{
  echo "# Atlas M7-0：Ink repair 基线（${DATE}）"
  echo
  echo "> 测量第三道 dogleg（\`repair_ink_group_pierces\` → \`atlas_plan_distorted_edges\`）。"
  echo "> 不改算法。图集：\`${SET}\` 中 flowchart + architecture。"
  echo
  echo "## 合计"
  echo
  echo "| 指标 | 值 |"
  echo "|------|----|"
  echo "| Hier 实测图数 | ${HIER} |"
  echo "| 触发 repair 的图 | ${REPAIRED} |"
  echo "| 失真边总数 | ${DISTORTED} |"
  echo
  echo "## 结论"
  echo
  if [[ "${DISTORTED}" == "0" ]]; then
    echo "**product Hier 子集总 distorted=0** → M7 中期（Ink 守 gate）可降级/延后；优先看 M8 / R3。"
  else
    echo "**仍有 ${DISTORTED} 条失真边（${REPAIRED} 张图）** → 建议开 M7 中期：Ink 落笔守 gate，靶图见下表 \`repaired/*\` 行；穿组门禁（repair 后）须继续为 0。"
  fi
  echo
  echo "## 明细（TSV）"
  echo
  echo '```'
  cat "$TMP"
  echo '```'
  echo
  echo "## 复现"
  echo
  echo '```bash'
  echo "./benchmarks/scripts/report-ink-repair-baseline.sh"
  echo "# 或"
  echo "cargo run -q -p plotgram-eval --bin atlas_repair_baseline -- ${SET}"
  echo '```'
} >"$OUT"

echo "Wrote $OUT"
echo "$SUMMARY"
