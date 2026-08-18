#!/usr/bin/env bash
# Slice 3/4 行为不变验证：把 showcase 全量 .taut 渲染为 SVG 到指定目录（扁平命名）。
# 用法：route_snapshot.sh <out_dir>
# 需先 `cargo build -p tautcore-cli`（debug）。不改动 showcase 目录。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT_DIR/target/debug/tautcore"
export TAUTCORE_FONTS_DIR="${TAUTCORE_FONTS_DIR:-$ROOT_DIR/fonts}"

OUT_DIR="${1:?usage: route_snapshot.sh <out_dir>}"
mkdir -p "$OUT_DIR"

count=0
while IFS= read -r -d '' f; do
  rel="${f#"$ROOT_DIR"/showcase/}"
  flat="${rel//\//__}"
  flat="${flat%.taut}.svg"
  "$BIN" render "$f" -o "$OUT_DIR/$flat" >/dev/null 2>&1 || echo "FAIL: $rel" >&2
  count=$((count + 1))
done < <(find "$ROOT_DIR/showcase" -name '*.taut' -not -path '*/.*' -print0 | sort -z)
echo "rendered $count files -> $OUT_DIR"
