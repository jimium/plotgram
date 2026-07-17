#!/usr/bin/env bash
# 一次性渲染 showcase 目录下所有 .pgm 文件

set -euo pipefail

#PLOTGRAM_PROFILE=debug

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# 默认 release：计时反映真实渲染性能；开发时可 PLOTGRAM_PROFILE=debug
PLOTGRAM_PROFILE="${PLOTGRAM_PROFILE:-release}"
PLOTGRAM_BIN="$ROOT_DIR/target/$PLOTGRAM_PROFILE/plotgram"
export PLOTGRAM_FONTS_DIR="${PLOTGRAM_FONTS_DIR:-$ROOT_DIR/fonts}"

FORMATS=("svg")
VALIDATE=false
SERVE=false
SERVE_PORT=4173
TRANSPARENT_BG=true
SHOW_TITLE=false

usage() {
  cat <<'EOF'
用法: render-all.sh [选项]

扫描 showcase 各类型目录（flowchart/、sequence/ 等）下的 .pgm 文件并批量渲染。
输出与源文件同目录、同名换后缀（如 flowchart/s.linear-chain.svg）。

选项:
  -f, --format FORMAT   输出格式: svg | png | webp | ascii | json（默认 svg）
  -a, --all             同时渲染 svg 和 png（便于与 Mermaid 截图对比）
      --validate        渲染前先执行语法验证
  -s, --serve [PORT]    渲染完成后启动 HTTP 服务（默认 4173），便于在浏览器中查看 index.html
      --opaque          保留画布背景色（默认输出透明背景，便于嵌入 showcase 预览）
      --title           在画布顶部绘制 DSL title（默认不绘制）
  -h, --help            显示此帮助

示例:
  ./showcase/render-all.sh
  ./showcase/render-all.sh -a
  ./showcase/render-all.sh -f png
  ./showcase/render-all.sh --validate -a
  ./showcase/render-all.sh -s
  ./showcase/render-all.sh --serve 8080
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -f|--format)
      FORMATS=("$2")
      shift 2
      ;;
    -a|--all)
      FORMATS=("svg" "png")
      shift
      ;;
    --validate)
      VALIDATE=true
      shift
      ;;
    --opaque)
      TRANSPARENT_BG=false
      shift
      ;;
    --title)
      SHOW_TITLE=true
      shift
      ;;
    -s|--serve)
      SERVE=true
      if [[ "${2:-}" =~ ^[0-9]+$ ]]; then
        SERVE_PORT="$2"
        shift 2
      else
        shift
      fi
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知选项: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

build_plotgram() {
  export CARGO_TARGET_DIR="$ROOT_DIR/target"
  echo "构建 plotgram-cli ($PLOTGRAM_PROFILE)..."
  if [[ "$PLOTGRAM_PROFILE" == "release" ]]; then
    (cd "$ROOT_DIR" && cargo build --release -p plotgram-cli)
  else
    (cd "$ROOT_DIR" && cargo build -p plotgram-cli)
  fi
  echo
}

run_plotgram() {
  if [[ ! -x "$PLOTGRAM_BIN" ]]; then
    echo "未找到二进制: $PLOTGRAM_BIN" >&2
    exit 1
  fi
  "$PLOTGRAM_BIN" "$@"
}

format_duration_ms() {
  awk -v ms="$1" 'BEGIN { printf "%.3fs", ms / 1000 }'
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

lookup_hash() {
  awk -F'\t' -v key="$1" '$2 == key { print $1; found = 1; exit } END { if (!found) print "" }' "$HASH_DB"
}

change_note_for() {
  local out="$1"
  local old_hash new_hash
  old_hash="$(lookup_hash "$out")"
  if [[ ! -f "$out" ]]; then
    echo ""
    return
  fi
  new_hash="$(sha256_file "$out")"
  if [[ -z "$old_hash" ]]; then
    echo " [新建]"
  elif [[ "$old_hash" == "$new_hash" ]]; then
    echo " [无变化]"
  else
    echo " [已变化]"
  fi
}

# 仅统计 plotgram render 墙钟耗时（毫秒精度）。
# 子进程 stdout/stderr 重定向到 /dev/null，避免 [perf] 等日志污染输出。
run_timed_render() {
  local render_args=(render "$1" -f "$2" -o "$3")
  if $TRANSPARENT_BG; then
    render_args+=(--transparent-background)
  fi
  if $SHOW_TITLE; then
    render_args+=(--title)
  fi
  perl -MTime::HiRes=time -e '
    use strict;
    my $start = time();
    my $pid = fork();
    if (!defined $pid) { exit 1; }
    if ($pid == 0) {
      open(STDOUT, ">", "/dev/null");
      open(STDERR, ">", "/dev/null");
      exec @ARGV or exit 127;
    }
    waitpid($pid, 0);
    my $rc = $? >> 8;
    print int((time() - $start) * 1000 + 0.5), "\n";
    exit($rc);
  ' -- "$PLOTGRAM_BIN" "${render_args[@]}"
}

output_ext() {
  case "$1" in
    svg)   echo "svg" ;;
    png)   echo "png" ;;
    webp)  echo "webp" ;;
    ascii) echo "ascii" ;;
    json)  echo "json" ;;
    *)
      echo "不支持的格式: $1（可选: svg png webp ascii json）" >&2
      exit 1
      ;;
  esac
}

build_plotgram

HASH_DB="$(mktemp)"
trap 'rm -f "$HASH_DB"' EXIT

total_files=0
while IFS= read -r -d '' _; do
  total_files=$((total_files + 1))
done < <(find "$SCRIPT_DIR" -name '*.pgm' -not -path '*/.*' -print0)

if [[ "$total_files" -eq 0 ]]; then
  echo "未找到 .pgm 文件" >&2
  exit 1
fi

total_jobs=$((total_files * ${#FORMATS[@]}))
current=0
success=0
failed=0
total_ms=0

echo "开始渲染: $total_files 个文件 × ${#FORMATS[@]} 种格式 = $total_jobs 个输出"
echo "格式: ${FORMATS[*]}"
echo

echo "记录现有输出 hash..."
while IFS= read -r -d '' dfy_file; do
  base="${dfy_file%.pgm}"
  for format in "${FORMATS[@]}"; do
    ext="$(output_ext "$format")"
    out="${base}.${ext}"
    if [[ -f "$out" ]]; then
      printf '%s\t%s\n' "$(sha256_file "$out")" "$out" >> "$HASH_DB"
    fi
  done
done < <(find "$SCRIPT_DIR" -name '*.pgm' -not -path '*/.*' -print0 | sort -z)
echo

while IFS= read -r -d '' dfy_file; do
  rel="${dfy_file#"$SCRIPT_DIR"/}"
  base="${dfy_file%.pgm}"

  if $VALIDATE; then
    if ! run_plotgram validate "$dfy_file" >/dev/null 2>&1; then
      echo "✗ 验证失败: $rel" >&2
      failed=$((failed + ${#FORMATS[@]}))
      current=$((current + ${#FORMATS[@]}))
      continue
    fi
  fi

  for format in "${FORMATS[@]}"; do
    current=$((current + 1))
    ext="$(output_ext "$format")"
    out="${base}.${ext}"

    if elapsed_ms="$(run_timed_render "$dfy_file" "$format" "$out")"; then
      total_ms=$((total_ms + elapsed_ms))
      success=$((success + 1))
      printf '[%d/%d] %s -> %s (%s)%s\n' \
        "$current" "$total_jobs" "$rel" "$(basename "$out")" "$(format_duration_ms "$elapsed_ms")" "$(change_note_for "$out")"
    else
      failed=$((failed + 1))
      printf '[%d/%d] %s -> %s\n' \
        "$current" "$total_jobs" "$rel" "$(basename "$out")" >&2
      echo "  ✗ 失败" >&2
    fi
  done
done < <(find "$SCRIPT_DIR" -name '*.pgm' -not -path '*/.*' -print0 | sort -z)

rendered=$((success + failed))

echo
echo "完成: 成功 ${success} 个，失败 ${failed} 个（共 ${total_jobs} 个输出）"
if [[ "$rendered" -gt 0 ]]; then
  awk -v rendered="$rendered" -v total_ms="$total_ms" '
    BEGIN {
      total = total_ms / 1000
      avg = total / rendered
      printf "渲染计算耗时: 总计 %.3fs，平均 %.3fs（%d 次 plotgram render）\n", total, avg, rendered
    }
  '
fi

echo
echo "更新 showcase/index.html 的样例 manifest..."
python3 "$SCRIPT_DIR/update-gallery-manifest.py"

[[ "${failed}" -eq 0 ]]

if $SERVE; then
  echo
  echo "启动 HTTP 服务: http://localhost:${SERVE_PORT}/index.html"
  echo "按 Ctrl+C 停止。"
  python3 -m http.server --directory "$SCRIPT_DIR" "$SERVE_PORT"
fi
