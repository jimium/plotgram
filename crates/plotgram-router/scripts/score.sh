#!/usr/bin/env bash
# Score baseline management: record, compare, update.
#
# Usage:
#   ./scripts/score.sh baseline [algo]   # Save current scores as baseline
#   ./scripts/score.sh compare [algo]    # Compare current vs baseline (default)
#   ./scripts/score.sh update [algo]     # Overwrite baseline with current scores
#
# Algorithm defaults to "orthogonal".
#
# Exit codes:
#   0 = no regression (or baseline saved)
#   1 = clearance regression detected
#   2 = no baseline file found
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BASELINE="$SCRIPT_DIR/baseline.json"

cmd="${1:-compare}"
ALGO="${2:-orthogonal}"

case "$cmd" in
  baseline|update)
    cargo run -q -p plotgram-router --example bench -- "$ALGO" --json > "$BASELINE"
    echo "baseline saved: $BASELINE ($ALGO, $(wc -c < "$BASELINE" | tr -d ' ') bytes)"
    ;;
  compare)
    if [[ ! -f "$BASELINE" ]]; then
      echo "error: no baseline found at $BASELINE" >&2
      echo "run: ./scripts/score.sh baseline" >&2
      exit 2
    fi
    cargo run -q -p plotgram-router --example bench -- "$ALGO" --baseline "$BASELINE"
    ;;
  *)
    echo "usage: score.sh {baseline|compare|update} [algorithm]" >&2
    exit 1
    ;;
esac
