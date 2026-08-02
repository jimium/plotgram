#!/usr/bin/env bash
# Generate routing visualization and open in browser.
# Usage: ./scripts/viz.sh [algorithm]   (default: orthogonal)
set -euo pipefail

ALGO="${1:-orthogonal}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cargo run -q -p plotgram-router --example viz -- "$ALGO"
open "$WORKSPACE_ROOT/target/router-viz.html"
