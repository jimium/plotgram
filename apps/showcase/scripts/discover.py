#!/usr/bin/env python3
"""List active layout .taut samples under a showcase directory.

A "layout" is any top-level directory under showcase/ that does NOT start with
'_' and is not in KNOWN_NON_SAMPLE_DIRS (scripts/, assets/). Each layout dir
is scanned recursively for *.taut (including optional facet subdirs such as
hierarchical/flat/). _backup/ and _out/ are never scanned (they are
'_'-prefixed).

Output: one path per line, posix-relative to the showcase dir, sorted.
Designed to be piped into incremental.py.

showcase-redesign-2026-08.md §6.1 / §4.1.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

KNOWN_NON_SAMPLE_DIRS = {"scripts", "assets"}


def discover_layouts(showcase_dir: Path) -> list[Path]:
    """Top-level layout dirs under showcase_dir, sorted."""
    if not showcase_dir.is_dir():
        return []
    layouts: list[Path] = []
    for entry in sorted(showcase_dir.iterdir()):
        if not entry.is_dir():
            continue
        if entry.name.startswith("_"):
            continue
        if entry.name in KNOWN_NON_SAMPLE_DIRS:
            continue
        layouts.append(entry)
    return layouts


def discover_samples(showcase_dir: Path, layout: str | None) -> list[str]:
    """Return sorted posix-relative .taut paths under active layout dirs."""
    layouts = discover_layouts(showcase_dir)
    if layout is not None:
        layouts = [d for d in layouts if d.name == layout]
        if not layouts:
            return []
    samples: list[str] = []
    for layout_dir in layouts:
        for pgm in sorted(layout_dir.rglob("*.taut")):
            if not pgm.is_file():
                continue
            samples.append(pgm.relative_to(showcase_dir).as_posix())
    return samples


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--showcase-dir", required=True, type=Path)
    parser.add_argument(
        "--layout", help="only emit samples under this layout dir name"
    )
    args = parser.parse_args()
    for s in discover_samples(args.showcase_dir.resolve(), args.layout):
        print(s)
    return 0


if __name__ == "__main__":
    sys.exit(main())
