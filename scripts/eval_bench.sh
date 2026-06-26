#!/usr/bin/env bash
# drawify-eval 一键评估看板
#
# 生成纯 HTML 文件，双击即可打开，无需 web 服务器。
#
# 用法:
#   ./scripts/eval_bench.sh              # 运行评估 + 生成看板 + 打开
#   ./scripts/eval_bench.sh --no-open    # 不打开浏览器
#   ./scripts/eval_bench.sh --only-dashboard  # 只重新生成看板（不重新跑评估）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$PROJECT_DIR/target/eval-dashboard"
JSON_PATH="$OUTPUT_DIR/bench_result.json"
HTML_PATH="$OUTPUT_DIR/index.html"

OPEN_BROWSER=true
ONLY_DASHBOARD=false

for arg in "$@"; do
  case "$arg" in
    --no-open) OPEN_BROWSER=false ;;
    --only-dashboard) ONLY_DASHBOARD=true ;;
    *) echo "未知参数: $arg"; exit 1 ;;
  esac
done

mkdir -p "$OUTPUT_DIR"

if [ "$ONLY_DASHBOARD" = false ]; then
  echo "▶ 运行算法基准测试（showcase 目录）..."
  cargo run -p drawify-eval --bin bench -- --showcase "$PROJECT_DIR/showcase" --output "$JSON_PATH"
  echo ""
fi

if [ ! -f "$JSON_PATH" ]; then
  echo "✗ 找不到评估数据: $JSON_PATH"
  echo "  请先运行不带 --only-dashboard 的命令"
  exit 1
fi

echo "▶ 生成可视化看板..."
python3 "$SCRIPT_DIR/eval_dashboard.py" "$JSON_PATH" -o "$HTML_PATH"

echo ""
echo "✓ 完成！"
echo "  数据: $JSON_PATH"
echo "  看板: $HTML_PATH"

if [ "$OPEN_BROWSER" = true ]; then
  open "$HTML_PATH" 2>/dev/null || xdg-open "$HTML_PATH" 2>/dev/null || true
fi
