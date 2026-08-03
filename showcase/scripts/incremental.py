#!/usr/bin/env python3
"""Decide which samples need re-rendering (mtime-based incremental).

Reads candidate .pgm paths (one per line, posix-relative to showcase dir) on
stdin. For each, compares mtime of the .pgm source and the plotgram binary
against the SVG at _out/{path without .pgm}.svg (mirrors source tree, including
optional facet dirs). If the SVG is missing or older than either the source or
the binary, the sample needs re-rendering.

With --force, all inputs are emitted (full re-render).

Output: paths needing render, one per line (posix-relative to showcase dir).
Summary stats go to stderr.

showcase-redesign-2026-08.md §6.1 / §6.2.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def svg_rel_for(pgm_rel: str) -> str:
    """Mirror source tree: hierarchical/flat/smoke.x.pgm -> _out/hierarchical/flat/smoke.x.svg"""
    stem = pgm_rel[:-4] if pgm_rel.endswith(".pgm") else pgm_rel
    return f"_out/{stem}.svg"


def needs_render(pgm_abs: Path, svg_abs: Path, binary_abs: Path) -> bool:
    if not svg_abs.exists():
        return True
    svg_mtime = svg_abs.stat().st_mtime
    if pgm_abs.stat().st_mtime > svg_mtime:
        return True
    if binary_abs.stat().st_mtime > svg_mtime:
        return True
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--showcase-dir", required=True, type=Path)
    parser.add_argument(
        "--binary", required=True, type=Path, help="plotgram binary path"
    )
    parser.add_argument(
        "--force", action="store_true", help="emit all inputs (skip mtime check)"
    )
    args = parser.parse_args()

    showcase = args.showcase_dir.resolve()
    binary = args.binary.resolve()
    if not binary.exists():
        print(f"incremental: binary not found: {binary}", file=sys.stderr)
        return 1

    total = 0
    rendered = 0
    out_lines: list[str] = []
    for line in sys.stdin:
        pgm_rel = line.strip()
        if not pgm_rel or pgm_rel.startswith("#"):
            continue
        total += 1
        pgm_abs = showcase / pgm_rel
        svg_abs = showcase / svg_rel_for(pgm_rel)
        if args.force or needs_render(pgm_abs, svg_abs, binary):
            out_lines.append(pgm_rel)
            rendered += 1

    sys.stdout.write("\n".join(out_lines))
    if out_lines:
        sys.stdout.write("\n")
    sys.stdout.flush()
    print(
        f"incremental: {rendered}/{total} need render"
        + (" (--force)" if args.force else ""),
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
