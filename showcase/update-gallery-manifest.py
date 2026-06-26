#!/usr/bin/env python3
"""
Update the inline SAMPLE_PATHS manifest inside showcase/index.html.

This keeps the single-file gallery page easy to maintain while still allowing
new .dfy examples to be discovered from disk automatically.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


MANIFEST_START = "// manifest:start"
MANIFEST_END = "// manifest:end"


def collect_dfy_paths(showcase_dir: Path) -> list[str]:
    """Return sorted .dfy paths relative to the showcase directory."""
    paths = [
        path.relative_to(showcase_dir).as_posix()
        for path in showcase_dir.rglob("*.dfy")
        if path.is_file()
    ]
    return sorted(paths)


def build_manifest_block(paths: list[str]) -> str:
    """Render the inline JavaScript block used by index.html."""
    lines = [MANIFEST_START, "    const SAMPLE_PATHS = ["]
    lines.extend(f'      "{path}",' for path in paths[:-1])
    if paths:
        lines.append(f'      "{paths[-1]}"')
    lines.append("    ];")
    lines.append(MANIFEST_END)
    return "\n".join(lines)


def update_index_html(index_html: Path, manifest_block: str) -> bool:
    """Replace the manifest block between the two sentinel markers."""
    content = index_html.read_text(encoding="utf-8")
    pattern = re.compile(
        rf"{re.escape(MANIFEST_START)}[\s\S]*?{re.escape(MANIFEST_END)}",
        re.MULTILINE,
    )
    updated, count = pattern.subn(manifest_block, content, count=1)
    if count != 1:
      raise RuntimeError("Could not find manifest markers in showcase/index.html")
    if updated == content:
        return False
    index_html.write_text(updated, encoding="utf-8")
    return True


def main() -> int:
    script_path = Path(__file__).resolve()
    showcase_dir = script_path.parent
    index_html = showcase_dir / "index.html"

    if not index_html.exists():
        raise FileNotFoundError(f"Missing index file: {index_html}")

    paths = collect_dfy_paths(showcase_dir)
    manifest_block = build_manifest_block(paths)
    changed = update_index_html(index_html, manifest_block)

    status = "updated" if changed else "already up to date"
    print(f"Gallery manifest {status}. {len(paths)} samples indexed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
