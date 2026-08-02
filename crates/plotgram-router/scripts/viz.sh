#!/usr/bin/env bash
# Generate routing visualization and open in browser.
# Usage: ./scripts/viz.sh [algorithm]   (default: orthogonal)
#
# Uses --release: debug builds of visibility/search routers are 50–100× slower
# across the full fixture suite and look “stuck”.
set -euo pipefail

ALGO="${1:-orthogonal}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

echo "routing fixtures with $ALGO (release) ..."
cargo run --release -q -p plotgram-router --example viz -- "$ALGO"
open "$WORKSPACE_ROOT/target/router-viz.html"
