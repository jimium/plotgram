#!/usr/bin/env bash
# 一次性渲染 showcase 目录下所有 .dfy 文件

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# 默认 release：计时反映真实渲染性能；开发时可 DRAWIFY_PROFILE=debug
DRAWIFY_PROFILE="${DRAWIFY_PROFILE:-release}"
DRAWIFY_BIN="$ROOT_DIR/target/$DRAWIFY_PROFILE/drawify"
export DRAWIFY_FONTS_DIR="${DRAWIFY_FONTS_DIR:-$ROOT_DIR/fonts}"

FORMATS=("svg")
VALIDATE=false
SERVE=false
SERVE_PORT=4173

usage() {
  cat <<'EOF'
用法: render-all.sh [选项]

扫描 showcase 各类型目录（flowchart/、sequence/ 等）下的 .dfy 文件并批量渲染。
输出与源文件同目录、同名换后缀（如 flowchart/s.linear-chain.svg）。

选项:
  -f, --format FORMAT   输出格式: svg | png | webp | ascii | json（默认 svg）
  -a, --all             同时渲染 svg 和 png（便于与 Mermaid 截图对比）
      --validate        渲染前先执行语法验证
  -s, --serve [PORT]    渲染完成后启动 HTTP 服务（默认 4173），便于在浏览器中查看 index.html
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

build_drawify() {
  echo "构建 drawify-cli ($DRAWIFY_PROFILE)..."
  if [[ "$DRAWIFY_PROFILE" == "release" ]]; then
    (cd "$ROOT_DIR" && cargo build --release -p drawify-cli)
  else
    (cd "$ROOT_DIR" && cargo build -p drawify-cli)
  fi
  echo
}

run_drawify() {
  if [[ ! -x "$DRAWIFY_BIN" ]]; then
    echo "未找到二进制: $DRAWIFY_BIN" >&2
    exit 1
  fi
  "$DRAWIFY_BIN" "$@"
}

format_duration_ms() {
  awk -v ms="$1" 'BEGIN { printf "%.3fs", ms / 1000 }'
}

# 仅统计 drawify render 墙钟耗时（毫秒精度）；不含 svg-history 归档/写盘。
# 子进程 stdout/stderr 重定向到 /dev/null，避免 [perf] 等日志污染输出。
run_timed_render() {
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
  ' -- "$DRAWIFY_BIN" render "$1" -f "$2" -o "$3"
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

build_drawify

# 清理上次中断留下的临时渲染文件，避免污染 svg-history
while IFS= read -r -d '' stale; do
  rm -f "$stale"
done < <(find "$SCRIPT_DIR" -name '*.rendering.*' -not -path '*/.*' -print0 2>/dev/null || true)

total_files=0
while IFS= read -r -d '' _; do
  total_files=$((total_files + 1))
done < <(find "$SCRIPT_DIR" -name '*.dfy' -not -path '*/.*' -print0)

if [[ "$total_files" -eq 0 ]]; then
  echo "未找到 .dfy 文件" >&2
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

while IFS= read -r -d '' dfy_file; do
  rel="${dfy_file#"$SCRIPT_DIR"/}"
  base="${dfy_file%.dfy}"

  if $VALIDATE; then
    if ! run_drawify validate "$dfy_file" >/dev/null 2>&1; then
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
    render_out="$out"
    history_note=""

    if [[ "$format" == "svg" ]]; then
      render_out="${out}.rendering.$$"
    fi

    if elapsed_ms="$(run_timed_render "$dfy_file" "$format" "$render_out")"; then
      total_ms=$((total_ms + elapsed_ms))
      if [[ "$format" == "svg" ]]; then
        history_result="$(python3 "$SCRIPT_DIR/svg-history.py" commit "$rel" "$render_out" "$out")"
        case "$history_result" in
          archived) history_note=" [已归档旧版]" ;;
          created)  history_note=" [新建]" ;;
        esac
      fi
      success=$((success + 1))
      printf '[%d/%d] %s -> %s (%s)%s\n' \
        "$current" "$total_jobs" "$rel" "$(basename "$out")" "$(format_duration_ms "$elapsed_ms")" "$history_note"
    else
      failed=$((failed + 1))
      if [[ "$format" == "svg" && -f "$render_out" ]]; then
        rm -f "$render_out"
      fi
      printf '[%d/%d] %s -> %s\n' \
        "$current" "$total_jobs" "$rel" "$(basename "$out")" >&2
      echo "  ✗ 失败" >&2
    fi
  done
done < <(find "$SCRIPT_DIR" -name '*.dfy' -not -path '*/.*' -print0 | sort -z)

rendered=$((success + failed))

echo
echo "完成: 成功 ${success} 个，失败 ${failed} 个（共 ${total_jobs} 个输出）"
if [[ "$rendered" -gt 0 ]]; then
  awk -v rendered="$rendered" -v total_ms="$total_ms" '
    BEGIN {
      total = total_ms / 1000
      avg = total / rendered
      printf "渲染计算耗时: 总计 %.3fs，平均 %.3fs（%d 次 drawify render）\n", total, avg, rendered
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
