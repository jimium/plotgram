#!/usr/bin/env bash
# Showcase render orchestrator (showcase-redesign-2026-08.md §6).
# Thin shell: cargo build + drive plotgram validate/render + call scripts/.
# Non-trivial logic (discover / incremental / manifest) lives in scripts/*.py.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_DIR="$ROOT_DIR/target"
SCRIPTS="$SCRIPT_DIR/scripts"

PROFILE=release
LAYOUT_FILTER=""
FORCE=false

usage() {
  cat <<EOF
用法: render.sh [选项]

增量渲染激活布局目录下的 .pgm -> _out/{path 去 .pgm}.svg（镜像 facet），并写 manifest。

渲染完成后通过 ./apps/serve.sh 提供的 HTTP 服务访问画廊。

选项:
  --layout NAME    只渲一个布局族（如 hierarchical / tree）
  --force          全量重渲（跳过 mtime 增量判定）
  --debug          用 debug 二进制（默认 release）
  -h, --help       显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --layout) LAYOUT_FILTER="$2"; shift 2 ;;
    --force) FORCE=true; shift ;;
    --debug) PROFILE=debug; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

BIN="$TARGET_DIR/$PROFILE/plotgram"

sha256_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

# 1. build
echo "构建 plotgram-cli ($PROFILE)..."
if [[ "$PROFILE" == "release" ]]; then
  (cd "$ROOT_DIR" && cargo build --release -p plotgram-cli)
else
  (cd "$ROOT_DIR" && cargo build -p plotgram-cli)
fi
[[ -x "$BIN" ]] || { echo "未找到二进制: $BIN" >&2; exit 1; }
echo

BINARY_HASH="$(sha256_file "$BIN")"
OUT_DIR="$SCRIPT_DIR/_out"
mkdir -p "$OUT_DIR"

# tmp files
ALL_LIST="$(mktemp)"
RENDER_LIST="$(mktemp)"
RESULTS_FILE="$(mktemp)"
ERR_FILE="$(mktemp)"
trap 'rm -f "$ALL_LIST" "$RENDER_LIST" "$RESULTS_FILE" "$ERR_FILE"' EXIT
: > "$RESULTS_FILE"

# 2. backup prev manifest
python3 "$SCRIPTS/write_manifest.py" backup --showcase-dir "$SCRIPT_DIR"

# 3. discover all active samples
DISCOVER_ARGS=(--showcase-dir "$SCRIPT_DIR")
if [[ -n "$LAYOUT_FILTER" ]]; then
  DISCOVER_ARGS+=(--layout "$LAYOUT_FILTER")
fi
python3 "$SCRIPTS/discover.py" "${DISCOVER_ARGS[@]}" > "$ALL_LIST"

ALL_COUNT=$(grep -c '' "$ALL_LIST" || true)
if [[ "$ALL_COUNT" -eq 0 ]]; then
  echo "未发现 .pgm 样例" >&2
  exit 1
fi

# 4. incremental: which need render
INCR_ARGS=(--showcase-dir "$SCRIPT_DIR" --binary "$BIN")
if $FORCE; then INCR_ARGS+=(--force); fi
python3 "$SCRIPTS/incremental.py" "${INCR_ARGS[@]}" < "$ALL_LIST" > "$RENDER_LIST"

RENDER_COUNT=$(grep -c '' "$RENDER_LIST" || true)
echo "发现 $ALL_COUNT 个样例；$RENDER_COUNT 个需重渲$($FORCE && echo '（--force）' || echo '')"
echo

# 5. render loop (only samples needing render)
emit_result() {
  # args: path status svg error elapsed_ms hash  (use "-" for null)
  python3 - "$RESULTS_FILE" "$@" <<'PYEOF'
import json, sys
results_file, path, status, svg, error, elapsed_ms, h = sys.argv[1:8]
entry = {
    "path": path,
    "status": status,
    "svg": svg if svg != "-" else None,
    "error": error if error != "-" else None,
    "elapsed_ms": int(elapsed_ms),
    "hash": h if h != "-" else None,
}
with open(results_file, "a", encoding="utf-8") as f:
    f.write(json.dumps(entry, ensure_ascii=False) + "\n")
PYEOF
}

timed_capture() {
  # Run "$@"; child stdout -> /dev/null; caller redirects stderr via 2>$ERR_FILE.
  # Print elapsed ms to stdout; preserve exit code.
  perl -MTime::HiRes=time -e '
    my $start = time;
    my $pid = fork();
    if (!defined $pid) { exit 1; }
    if ($pid == 0) {
      open(STDOUT, ">", "/dev/null");
      exec @ARGV or exit 127;
    }
    waitpid($pid, 0);
    my $rc = $? >> 8;
    my $elapsed = int((time() - $start) * 1000 + 0.5);
    print "$elapsed\n";
    exit($rc);
  ' -- "$@"
}

render_one() {
  local path="$1"
  local pgm_abs="$SCRIPT_DIR/$path"
  local svg_rel="_out/${path%.pgm}.svg"
  local svg_abs="$SCRIPT_DIR/$svg_rel"

  # validate
  : > "$ERR_FILE"
  local v_ms v_rc=0
  set +e
  v_ms="$(timed_capture "$BIN" validate "$pgm_abs" 2>"$ERR_FILE")"
  v_rc=$?
  set -e
  if [[ $v_rc -ne 0 ]]; then
    emit_result "$path" "parse-error" "-" "$(head -c 2000 "$ERR_FILE")" "${v_ms:-0}" "-"
    echo "  ✗ parse-error: $path"
    return 0
  fi

  # render
  mkdir -p "$(dirname "$svg_abs")"
  : > "$ERR_FILE"
  local r_ms r_rc=0
  set +e
  r_ms="$(timed_capture "$BIN" render "$pgm_abs" -o "$svg_abs" 2>"$ERR_FILE")"
  r_rc=$?
  set -e
  if [[ $r_rc -ne 0 ]]; then
    emit_result "$path" "render-error" "-" "$(head -c 2000 "$ERR_FILE")" "$(( ${v_ms:-0} + ${r_ms:-0} ))" "-"
    echo "  ✗ render-error: $path"
    return 0
  fi

  local hash
  hash="$(sha256_file "$svg_abs")"
  emit_result "$path" "ok" "$svg_rel" "-" "$(( ${v_ms:-0} + ${r_ms:-0} ))" "$hash"
  echo "  ✓ $path ($(( ${v_ms:-0} + ${r_ms:-0} ))ms)"
}

if [[ "$RENDER_COUNT" -gt 0 ]]; then
  echo "渲染中..."
  while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    render_one "$path"
  done < "$RENDER_LIST"
  echo
fi

# 6. write manifest (fresh results + carried prev + full list)
python3 "$SCRIPTS/write_manifest.py" write \
  --showcase-dir "$SCRIPT_DIR" \
  --binary-hash "$BINARY_HASH" \
  --results "$RESULTS_FILE" \
  --all-list "$ALL_LIST"

# 7. summary
python3 - "$OUT_DIR/manifest.json" <<'PYEOF'
import json, sys
m = json.load(open(sys.argv[1], encoding="utf-8"))
samples = m["samples"]
ok = sum(1 for s in samples if s["status"] == "ok")
pe = sum(1 for s in samples if s["status"] == "parse-error")
re = sum(1 for s in samples if s["status"] == "render-error")
ch = sum(1 for s in samples if s.get("changed"))
print(f"manifest: {len(samples)} samples | {ok} ok, {pe} parse-error, {re} render-error | {ch} changed")
if pe or re:
    sys.exit(1)
PYEOF
SUMMARY_RC=$?
if [[ $SUMMARY_RC -ne 0 ]]; then
  echo "存在失败样例（见上方）" >&2
fi

# 8. 提示访问地址（HTTP 服务由 apps/serve.sh 统一提供，端口 8030）
echo
echo "渲染完成。"
echo "画廊访问地址（请先运行 ./apps/serve.sh）:"
echo "  http://localhost:8030/showcase/index.html"

exit $SUMMARY_RC
