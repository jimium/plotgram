#!/usr/bin/env bash
# 批量评估 showcase 目录下所有 .dfy 文件的布局与路由质量
#
# 用法:
#   ./eval-showcase.sh                     # 默认：同时对比布局和路由
#   ./eval-showcase.sh -c routing          # 仅对比路由
#   ./eval-showcase.sh -c layout           # 仅对比布局
#   ./eval-showcase.sh -f json             # 输出 JSON
#   ./eval-showcase.sh -o report.md        # 输出到文件

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
EVAL_BIN="$ROOT_DIR/target/debug/eval"
export DRAWIFY_FONTS_DIR="${DRAWIFY_FONTS_DIR:-$ROOT_DIR/fonts}"

COMPARE="auto"
FORMAT="markdown"
OUTPUT=""

usage() {
  cat <<'EOF'
用法: eval-showcase.sh [选项]

批量评估 showcase 目录下所有 .dfy 文件的布局与边路由质量。

选项:
  -c, --compare <模式>   对比模式: auto(默认) | routing | layout | full
  -f, --format <格式>    输出格式: markdown(默认) | json
  -o, --output <文件>    输出到文件（默认 stdout）
  -h, --help             显示此帮助

对比模式:
  auto       按图表类型自动选择适用的算法（默认）
  routing   对比 orthogonal / bezier / spline 边路由
  layout    对比 sugiyama / sugiyama-v2 布局算法
  full      对比 sugiyama + 三种路由的完整组合

示例:
  ./eval-showcase.sh
  ./eval-showcase.sh -c layout -o layout-report.md
  ./eval-showcase.sh -c auto -o report.md
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -c|--compare)
      COMPARE="$2"
      shift 2
      ;;
    -f|--format)
      FORMAT="$2"
      shift 2
      ;;
    -o|--output)
      OUTPUT="$2"
      shift 2
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

# 构建二进制
if [[ ! -x "$EVAL_BIN" ]]; then
  echo "构建 drawify-eval..."
  (cd "$ROOT_DIR" && cargo build -p drawify-eval --bin eval 2>&1)
fi

# 运行评估
echo "评估 showcase 目录: $SCRIPT_DIR"
echo "对比模式: $COMPARE"
echo

ARGS=("$SCRIPT_DIR" -c "$COMPARE" -f "$FORMAT")
if [[ -n "$OUTPUT" ]]; then
  ARGS+=(-o "$OUTPUT")
fi

"$EVAL_BIN" batch "${ARGS[@]}"
