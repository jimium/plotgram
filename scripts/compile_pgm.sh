#!/usr/bin/env bash
# 将单个 .pgm 构建为 SVG（plotgram-cli → plotgram-compile 全链路）
#
# 用法:
#   scripts/compile_pgm.sh                          # 默认示例图
#   scripts/compile_pgm.sh path/to/diagram.pgm      # 输出到同目录 .svg
#   scripts/compile_pgm.sh in.pgm -o out.svg        # 指定输出路径
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_INPUT="$ROOT/showcase/hierarchical/smoke.fan-out-four.pgm"

INPUT="${1:-$DEFAULT_INPUT}"
OUTPUT=""

shift || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--output)
      OUTPUT="${2:?missing path after $1}"
      shift 2
      ;;
    -h|--help)
      sed -n '2,6p' "$0"
      exit 0
      ;;
    *)
      echo "未知选项: $1" >&2
      exit 1
      ;;
  esac
done

if [[ ! -f "$INPUT" ]]; then
  echo "找不到输入文件: $INPUT" >&2
  exit 1
fi

if [[ -z "$OUTPUT" ]]; then
  OUTPUT="${INPUT%.pgm}.svg"
fi

mkdir -p "$(dirname "$OUTPUT")"

echo "输入:  $INPUT"
echo "输出:  $OUTPUT"
echo

(cd "$ROOT" && cargo run -q -p plotgram-cli -- "$INPUT" -o "$OUTPUT")

echo
echo "完成: $OUTPUT ($(wc -c < "$OUTPUT" | tr -d ' ') bytes)"

if command -v open >/dev/null 2>&1; then
  open "$OUTPUT"
fi
