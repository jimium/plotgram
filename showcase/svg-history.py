#!/usr/bin/env python3
"""
Manage SVG render history for showcase.

When render-all produces a changed SVG, the previous version is archived under
showcase/.history/ and manifest.json is updated for the gallery page.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

HISTORY_DIR = ".history"
MANIFEST_NAME = "manifest.json"
MANIFEST_VERSION = 1


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def manifest_path(showcase_dir: Path) -> Path:
    return showcase_dir / HISTORY_DIR / MANIFEST_NAME


def load_manifest(showcase_dir: Path) -> dict:
    path = manifest_path(showcase_dir)
    if not path.exists():
        return {"version": MANIFEST_VERSION, "updated": None, "charts": {}}
    return json.loads(path.read_text(encoding="utf-8"))


def save_manifest(showcase_dir: Path, manifest: dict) -> None:
    manifest["version"] = MANIFEST_VERSION
    manifest["updated"] = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    path = manifest_path(showcase_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def chart_key(svg_rel: str) -> str:
    return svg_rel.replace("\\", "/")


def history_rel_path(showcase_dir: Path, archive_file: Path) -> str:
    return archive_file.relative_to(showcase_dir).as_posix()


def archive_dir_for(showcase_dir: Path, svg_rel: str) -> Path:
    rel = Path(svg_rel)
    return showcase_dir / HISTORY_DIR / rel.parent / rel.stem


def append_history_entry(
    showcase_dir: Path,
    manifest: dict,
    svg_rel: str,
    dfy_rel: str,
    archive_file: Path,
    entry_id: str,
    saved_at: str,
) -> None:
    key = chart_key(svg_rel)
    charts = manifest.setdefault("charts", {})
    chart = charts.setdefault(
        key,
        {
            "dfy": dfy_rel.replace("\\", "/"),
            "entries": [],
        },
    )
    chart["dfy"] = dfy_rel.replace("\\", "/")
    chart["entries"].append(
        {
            "id": entry_id,
            "file": history_rel_path(showcase_dir, archive_file),
            "savedAt": saved_at,
        }
    )
    chart["entries"].sort(key=lambda item: item["savedAt"], reverse=True)


def commit_svg(showcase_dir: Path, dfy_rel: str, tmp_svg: Path, final_svg: Path) -> str:
    """
    Promote a freshly rendered SVG into place, archiving the previous version if it changed.

    Returns: unchanged | created | archived
    """
    if not tmp_svg.exists():
        raise FileNotFoundError(f"Missing rendered SVG: {tmp_svg}")

    new_bytes = tmp_svg.read_bytes()
    new_hash = sha256_bytes(new_bytes)
    svg_rel = final_svg.relative_to(showcase_dir).as_posix()

    if final_svg.exists():
        old_hash = sha256_file(final_svg)
        if old_hash == new_hash:
            tmp_svg.unlink(missing_ok=True)
            return "unchanged"

        stamp = datetime.now(timezone.utc)
        entry_id = stamp.strftime("%Y%m%d-%H%M%S")
        saved_at = stamp.replace(microsecond=0).isoformat()
        archive_dir = archive_dir_for(showcase_dir, svg_rel)
        archive_dir.mkdir(parents=True, exist_ok=True)
        archive_file = archive_dir / f"{entry_id}.svg"
        shutil.copy2(final_svg, archive_file)

        manifest = load_manifest(showcase_dir)
        append_history_entry(
            showcase_dir,
            manifest,
            svg_rel,
            dfy_rel,
            archive_file,
            entry_id,
            saved_at,
        )
        save_manifest(showcase_dir, manifest)

        final_svg.write_bytes(new_bytes)
        tmp_svg.unlink(missing_ok=True)
        return "archived"

    final_svg.parent.mkdir(parents=True, exist_ok=True)
    tmp_svg.replace(final_svg)
    return "created"


def main() -> int:
    if len(sys.argv) != 5:
        print(
            "用法: svg-history.py commit <dfy-rel> <tmp-svg> <final-svg>",
            file=sys.stderr,
        )
        return 2

    _, command, dfy_rel, tmp_raw, final_raw = sys.argv
    if command != "commit":
        print("仅支持 commit 子命令", file=sys.stderr)
        return 2

    tmp_svg = Path(tmp_raw).resolve()
    final_svg = Path(final_raw).resolve()
    script_dir = Path(__file__).resolve().parent

    if not final_svg.is_relative_to(script_dir):
        print("final-svg must live under showcase/", file=sys.stderr)
        return 1

    result = commit_svg(script_dir, dfy_rel, tmp_svg, final_svg)
    print(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
