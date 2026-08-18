#!/usr/bin/env bash
# v2 门禁快照：批量调 `tautcore measure --json` 采集指标，归档到 baselines/。
#
# 用法（仓库根目录）:
#   ./benchmarks/snapshot.sh
#   ./benchmarks/snapshot.sh --tag v2-initial
#   ./benchmarks/snapshot.sh --set benchmarks/sets/smoke-set.txt
#   ./benchmarks/snapshot.sh --out benchmarks/baselines/current --set ... --set ...
#
# 默认遍历所有 benchmarks/sets/*.txt；每行路径相对 showcase/，必须含 layout 前缀。
# 输出（同日多次不互相覆盖）:
#   benchmarks/baselines/YYYY-MM-DD-HHMMSS[-tag].json
#   benchmarks/baselines/YYYY-MM-DD-HHMMSS[-tag].md
#   benchmarks/baselines/latest.{json,md}            # 始终指向最近一次
#
# --out BASE  CI 模式：只写 BASE.json，不写 .md，不 sync latest（避免覆盖已提交基线）
#
# 单样例 `tautcore measure` 在硬失败（parse-error / render-error / det=false /
# 任何 overlap > 0）时 exit ≠ 0；本脚本捕获 stdout JSON 不被 pipefail 杀掉。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BM="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$BM/.." && pwd)"
SHOWCASE="$ROOT/apps/showcase"

TAG=""
SET_FILES=()
OUT_BASE=""   # 自定义输出路径前缀（不含 .json/.md）；默认 baselines/{stamp}
STAMP="$(date +%Y-%m-%d-%H%M%S)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"; shift 2 ;;
    --set)
      SET_FILES+=("$2"); shift 2 ;;
    --out)
      OUT_BASE="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

if [[ -n "$TAG" ]]; then
  if [[ ! "$TAG" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "error: --tag 仅允许字母数字 / . _ -" >&2
    exit 1
  fi
  STAMP="${STAMP}-${TAG}"
fi

# 默认：所有 sets/*.txt（按文件名排序，确定性）
if [[ ${#SET_FILES[@]} -eq 0 ]]; then
  while IFS= read -r sf; do
    SET_FILES+=("$sf")
  done < <(find "$BM/sets" -maxdepth 1 -name '*.txt' -type f | sort)
fi

if [[ ${#SET_FILES[@]} -eq 0 ]]; then
  echo "error: 没有找到 benchmarks/sets/*.txt" >&2
  exit 1
fi

# 合并 set 文件：去重保序，跳过注释 / 空行
TMP_LIST="$(mktemp)"
trap 'rm -f "$TMP_LIST"' EXIT

for sf in "${SET_FILES[@]}"; do
  [[ -f "$sf" ]] || { echo "warn: 缺失 set 文件 $sf，跳过" >&2; continue; }
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    line="$(echo "$line" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
    [[ -z "$line" ]] && continue
    echo "$line"
  done < "$sf"
done | awk '!seen[$0]++' > "$TMP_LIST"

FILES=()
while IFS= read -r line; do
  [[ -n "$line" ]] && FILES+=("$line")
done < "$TMP_LIST"

if [[ ${#FILES[@]} -eq 0 ]]; then
  echo "error: set 文件中没有有效样例路径" >&2
  exit 1
fi

# 1. build
echo "▶ 构建 tautcore-cli (release)..."
# 避免沙箱注入 CARGO_TARGET_DIR 把产物写到临时目录
unset CARGO_TARGET_DIR || true
( cd "$ROOT" && cargo build --release -p tautcore-cli ) 2>&1 | tail -3
TAUTCORE="$ROOT/target/release/tautcore"
[[ -x "$TAUTCORE" ]] || { echo "error: tautcore 二进制缺失 ($TAUTCORE)" >&2; exit 1; }
echo

echo "▶ 共 ${#FILES[@]} 个样例（来自 ${#SET_FILES[@]} 个 set 文件）"
echo "▶ 归档名: ${STAMP}"
echo

JSON_OUT="$BM/baselines/${STAMP}.json"
MD_OUT="$BM/baselines/${STAMP}.md"
if [[ -n "$OUT_BASE" ]]; then
  # --out 模式：写自定义路径，不写 .md，不 sync latest（CI 使用）
  JSON_OUT="${OUT_BASE}.json"
  MD_OUT=""
fi

# 2. run measure for each sample, capture stdout regardless of exit code.
# argv layout: tautcore / showcase / json / stamp / n_sets / sets... / files...
python3 - "$TAUTCORE" "$SHOWCASE" "$JSON_OUT" "$STAMP" "${#SET_FILES[@]}" "${SET_FILES[@]}" "${FILES[@]}" <<'PY'
import json, subprocess, sys, os, time

tautcore  = sys.argv[1]
showcase  = sys.argv[2]
out_json  = sys.argv[3]
stamp     = sys.argv[4]
n_sets    = int(sys.argv[5])
set_files = sys.argv[6:6 + n_sets]
files     = sys.argv[6 + n_sets:]

def derive(path):
    layout = path.split("/")[0] if "/" in path else ""
    role = path.rsplit("/", 1)[-1].split(".")[0]
    return layout, role

reports = []
for i, f in enumerate(files, 1):
    layout, role = derive(f)
    t0 = time.time()
    try:
        proc = subprocess.run(
            [tautcore, "measure", f, "--json"],
            cwd=showcase,
            capture_output=True, text=True,
        )
        stdout = proc.stdout.strip()
    except Exception as e:
        reports.append({
            "schema_version": 1,
            "path": f, "layout": layout, "role": role,
            "status": "render-error",
            "error": f"snapshot invocation failed: {e}",
            "elapsed_ms": int((time.time() - t0) * 1000),
            "correctness": None, "quality": None, "observation": None,
        })
        print(f"  [{i}/{len(files)}] {f.split('/')[-1]}: INVOCATION-FAIL", file=sys.stderr)
        continue

    wall_ms = int((time.time() - t0) * 1000)
    if not stdout:
        # measure 没写 stdout（理论上不应该）：构造 render-error 占位
        reports.append({
            "schema_version": 1,
            "path": f, "layout": layout, "role": role,
            "status": "render-error",
            "error": f"no stdout from `tautcore measure` (rc={proc.returncode}, stderr={proc.stderr[:300]})",
            "elapsed_ms": wall_ms,
            "correctness": None, "quality": None, "observation": None,
        })
    else:
        try:
            report = json.loads(stdout)
            # 防御：CLI 输出的 path/layout/role 应该与 set 文件行一致；
            # 若不一致，用 set 文件的版本覆盖（保证 compare by path 可靠）。
            report["path"] = f
            report["layout"] = layout
            report["role"] = role
            reports.append(report)
        except json.JSONDecodeError as e:
            reports.append({
                "schema_version": 1,
                "path": f, "layout": layout, "role": role,
                "status": "render-error",
                "error": f"non-JSON stdout: {e}; first 200 chars: {stdout[:200]}",
                "elapsed_ms": wall_ms,
                "correctness": None, "quality": None, "observation": None,
            })
    name = f.split("/")[-1]
    status = reports[-1].get("status", "?")
    ms = reports[-1].get("elapsed_ms", wall_ms)
    print(f"  [{i}/{len(files)}] {name}: {status} (rc={proc.returncode}, {ms}ms)", file=sys.stderr)

snap = {
    "schema_version": 1,
    "date": stamp,
    "tag": None,  # filled below if --tag was given
    "set_files": [os.path.relpath(s, os.getcwd()) for s in set_files],
    "samples": reports,
}
with open(out_json, "w") as fh:
    json.dump(snap, fh, indent=2, ensure_ascii=False)
    fh.write("\n")
print(f"\n✓ wrote {out_json} ({len(reports)} samples)", file=sys.stderr)
PY

# Inject tag (if any) into the JSON
if [[ -n "$TAG" ]]; then
  python3 - "$JSON_OUT" "$TAG" <<'PY'
import json, sys
p, tag = sys.argv[1], sys.argv[2]
with open(p) as fh:
    snap = json.load(fh)
snap["tag"] = tag
with open(p, "w") as fh:
    json.dump(snap, fh, indent=2, ensure_ascii=False)
    fh.write("\n")
PY
fi

# 3. .md summary（--out 模式下跳过）
if [[ -n "$MD_OUT" ]]; then
python3 - "$JSON_OUT" "$MD_OUT" <<'PY'
import json, sys
from collections import defaultdict
with open(sys.argv[1]) as fh:
    snap = json.load(fh)
md = sys.argv[2]
samples = snap.get("samples", [])
lines = []
lines.append(f"# Snapshot {snap.get('date','?')}")
if snap.get("tag"):
    lines.append(f"tag: `{snap['tag']}`")
sf = snap.get("set_files") or []
if sf:
    lines.append("set_files: " + ", ".join(f"`{s}`" for s in sf))
lines.append("")
lines.append(f"- samples: {len(samples)}")
by_role = defaultdict(list)
for s in samples:
    by_role[s.get("role", "?")].append(s)
for role in ["smoke", "product", "demo", "stress", "mech"]:
    if role in by_role:
        lines.append(f"  - {role}: {len(by_role[role])}")
ok = sum(1 for s in samples if s.get("status") == "ok")
pe = sum(1 for s in samples if s.get("status") == "parse-error")
re = sum(1 for s in samples if s.get("status") == "render-error")
lines.append(f"- status: ok={ok}, parse-error={pe}, render-error={re}")
lines.append("")
lines.append("| role | file | status | det | node_ovl | edge_grp | label_ovl | crossings | edge_len | canvas | aspect | n/e | ms |")
lines.append("|---|---|---|:---:|---:|---:|---:|---:|---:|---:|---:|---|---:|")
def fmt(v):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.1f}"
    return str(v)
for role in ["smoke", "product", "demo", "stress", "mech"]:
    for s in by_role.get(role, []):
        name = s["path"].rsplit("/", 1)[-1]
        c = s.get("correctness") or {}
        q = s.get("quality") or {}
        o = s.get("observation") or {}
        det = c.get("det") if c else None
        det_s = "✓" if det is True else ("✗" if det is False else "-")
        lines.append(
            f"| {role} | `{name}` | {s.get('status','?')} | {det_s} | "
            f"{fmt(c.get('node_overlap_count'))} | {fmt(c.get('edge_crosses_group_interior'))} | "
            f"{fmt(c.get('label_overlap_count'))} | {fmt(q.get('edge_crossing_count'))} | "
            f"{fmt(q.get('total_edge_length'))} | {fmt(q.get('canvas_area'))} | "
            f"{fmt(q.get('aspect_ratio'))} | "
            f"{fmt(o.get('node_count'))}/{fmt(o.get('edge_count'))} | "
            f"{fmt(s.get('elapsed_ms'))} |"
        )
lines.append("")
lines.append("复跑: `./benchmarks/snapshot.sh`")
lines.append("对比: `./benchmarks/compare.sh <baseline.json> <current.json>`")
with open(md, "w") as fh:
    fh.write("\n".join(lines) + "\n")
PY
fi

# 4. sync latest（--out 模式下跳过）
if [[ -z "$OUT_BASE" ]]; then
  cp "$JSON_OUT" "$BM/baselines/latest.json"
  cp "$MD_OUT" "$BM/baselines/latest.md"
fi

echo
echo "✓ $JSON_OUT"
[[ -n "$MD_OUT" ]] && echo "✓ $MD_OUT"
[[ -z "$OUT_BASE" ]] && echo "✓ synced latest"
