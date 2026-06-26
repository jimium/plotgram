#!/usr/bin/env bash
# 生成 Markdown 格式的算法评估报告
#
# 输出到 target/eval-report/report.md
# 包含：按图类型分组的布局对比 + 路由对比
#
# 用法:
#   ./scripts/eval_report.sh              # 生成报告
#   ./scripts/eval_report.sh --open       # 生成并打开

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$PROJECT_DIR/target/eval-report"
OUTPUT_FILE="$OUTPUT_DIR/report.md"

OPEN=false
for arg in "$@"; do
  case "$arg" in
    --open) OPEN=true ;;
  esac
done

mkdir -p "$OUTPUT_DIR"

echo "▶ 生成 Markdown 评估报告..."
cargo run -p drawify-eval --bin eval -- batch "$PROJECT_DIR/showcase" -o "$OUTPUT_FILE" 2>&1 | tail -5

echo ""
echo "✓ 报告已生成: $OUTPUT_FILE"
echo "  $(wc -l < "$OUTPUT_FILE") 行"

if [ "$OPEN" = true ]; then
  open "$OUTPUT_FILE" 2>/dev/null || true
fi
