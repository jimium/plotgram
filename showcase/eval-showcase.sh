#!/usr/bin/env bash
# Showcase 质量评估与基线回归检测
#
# 用法:
#   ./eval-showcase.sh baseline              # 生成/更新 eval-data/showcase-baseline.json
#   ./eval-showcase.sh check                 # 与基线对比，有回归则 exit 1
#   ./eval-showcase.sh check -o report.md    # 输出对比报告

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
EVAL_BIN="$ROOT_DIR/target/debug/eval"
BASELINE_FILE="$ROOT_DIR/eval-data/showcase-baseline.json"
export PLOTGRAM_FONTS_DIR="${PLOTGRAM_FONTS_DIR:-$ROOT_DIR/fonts}"

MODE="check"
OUTPUT=""
FORMAT="text"

usage() {
  cat <<'EOF'
用法: eval-showcase.sh <命令> [选项]

命令:
  baseline    生成 showcase 质量基线（写入 eval-data/showcase-baseline.json）
  check       与基线对比，检测指标回归（默认）

选项:
  -o, --output <文件>   输出报告到文件（仅 check 模式）
  -f, --format <格式>   输出格式: text(默认) | json
  -h, --help            显示帮助

示例:
  ./eval-showcase.sh baseline
  ./eval-showcase.sh check
  ./eval-showcase.sh check -o /tmp/regression.md
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    baseline|check)
      MODE="$1"
      shift
      ;;
    -o|--output)
      OUTPUT="$2"
      shift 2
      ;;
    -f|--format)
      FORMAT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知参数: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

echo "构建 plotgram-eval..."
(cd "$ROOT_DIR" && CARGO_TARGET_DIR="$ROOT_DIR/target" cargo build -p plotgram-eval --bin eval 2>&1)

ARGS=("$SCRIPT_DIR")
case "$MODE" in
  baseline)
    ARGS=(baseline "${ARGS[@]}" -o "$BASELINE_FILE")
    echo "生成 showcase 基线 → $BASELINE_FILE"
  ;;
  check)
    ARGS=(baseline-check "${ARGS[@]}" --baseline "$BASELINE_FILE" -f "$FORMAT")
    if [[ -n "$OUTPUT" ]]; then
      ARGS+=(-o "$OUTPUT")
    fi
    echo "对比 showcase 与基线: $BASELINE_FILE"
  ;;
esac

"$EVAL_BIN" "${ARGS[@]}"
