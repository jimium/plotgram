#!/usr/bin/env python3
"""Assemble _out/manifest.json for the showcase gallery and gate.

Two modes:

  backup   Copy _out/manifest.json -> _out/manifest.prev.json (idempotent;
           no-op if no current manifest). Call BEFORE rendering.

  write    Read fresh render results (results.jsonl, one JSON per line) +
           the full discovered sample list (all-list) + the previous manifest,
           and write a new _out/manifest.json. Samples in all-list but absent
           from results are carried over from the previous manifest (with
           changed=false). Samples absent from both are marked render-error.

Manifest schema:

  {
    "generated_at": "ISO-8601 UTC",
    "binary_hash": "...",
    "samples": [
      {path, layout, facet, role, status, svg, error, elapsed_ms, hash, changed}
    ]
  }

Path shape: `{layout}/[{facet}/]{role}.{slug}.taut`
`changed` = (this hash != prev hash) OR (sample not in prev).
SVG mirrors source: `_out/{path without .taut}.svg`.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
from pathlib import Path


def role_of(path: str) -> str:
    name = path.rsplit("/", 1)[-1]
    return name.split(".", 1)[0]


def layout_of(path: str) -> str:
    return path.split("/", 1)[0]


def facet_of(path: str) -> str | None:
    """Second path segment when it is a directory (not the filename)."""
    parts = path.split("/")
    if len(parts) >= 3:
        return parts[1]
    return None


def svg_rel_for(path: str) -> str:
    stem = path[:-4] if path.endswith(".taut") else path
    return f"_out/{stem}.svg"


def load_prev(prev_path: Path) -> dict[str, dict]:
    if not prev_path.exists():
        return {}
    try:
        prev = json.loads(prev_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        print(f"write_manifest: bad prev manifest {prev_path}: {e}", file=sys.stderr)
        return {}
    return {s["path"]: s for s in prev.get("samples", [])}


def load_results(results_path: Path) -> dict[str, dict]:
    out: dict[str, dict] = {}
    if not results_path.exists():
        return out
    for line in results_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as e:
            print(f"write_manifest: skipping bad results line: {e}", file=sys.stderr)
            continue
        out[entry["path"]] = entry
    return out


def load_all_list(all_list_path: Path) -> list[str]:
    paths: list[str] = []
    for line in all_list_path.read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if s and not s.startswith("#"):
            paths.append(s)
    return paths


def cmd_backup(showcase_dir: Path) -> int:
    out_dir = showcase_dir / "_out"
    cur = out_dir / "manifest.json"
    prev = out_dir / "manifest.prev.json"
    if not cur.exists():
        return 0
    out_dir.mkdir(parents=True, exist_ok=True)
    prev.write_bytes(cur.read_bytes())
    print(f"write_manifest: backed up manifest -> {prev.name}", file=sys.stderr)
    return 0


def cmd_write(
    showcase_dir: Path,
    binary_hash: str,
    results_path: Path,
    all_list_path: Path,
) -> int:
    out_dir = showcase_dir / "_out"
    out_dir.mkdir(parents=True, exist_ok=True)
    prev = load_prev(out_dir / "manifest.prev.json")
    fresh = load_results(results_path)
    all_paths = load_all_list(all_list_path)

    samples: list[dict] = []
    ok = parse_err = render_err = missing = carried = changed_count = 0
    for path in all_paths:
        prev_entry = prev.get(path)
        if path in fresh:
            entry = dict(fresh[path])
            source = "fresh"
        elif prev_entry is not None:
            entry = dict(prev_entry)
            entry["changed"] = False
            source = "prev"
            carried += 1
        else:
            entry = {
                "status": "render-error",
                "svg": svg_rel_for(path),
                "error": "missing render result",
                "elapsed_ms": 0,
                "hash": None,
                "changed": True,
            }
            source = "missing"
            missing += 1

        entry["path"] = path
        entry["layout"] = layout_of(path)
        entry["facet"] = facet_of(path)
        entry["role"] = role_of(path)
        if entry.get("status") == "ok" and not entry.get("svg"):
            entry["svg"] = svg_rel_for(path)

        if source == "fresh":
            prev_hash = prev_entry.get("hash") if prev_entry else None
            entry["changed"] = entry.get("hash") != prev_hash

        st = entry.get("status", "render-error")
        if st == "ok":
            ok += 1
        elif st == "parse-error":
            parse_err += 1
        else:
            render_err += 1
        if entry.get("changed"):
            changed_count += 1

        samples.append(
            {
                "path": entry["path"],
                "layout": entry["layout"],
                "facet": entry["facet"],
                "role": entry["role"],
                "status": st,
                "svg": entry.get("svg"),
                "error": entry.get("error"),
                "elapsed_ms": entry.get("elapsed_ms", 0),
                "hash": entry.get("hash"),
                "changed": bool(entry.get("changed", False)),
            }
        )

    samples.sort(key=lambda s: s["path"])

    manifest = {
        "generated_at": dt.datetime.now(dt.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "binary_hash": binary_hash,
        "samples": samples,
    }
    out_path = out_dir / "manifest.json"
    out_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"write_manifest: wrote {out_path.name}: "
        f"{ok} ok, {parse_err} parse-error, {render_err} render-error, "
        f"{missing} missing, {carried} carried, {changed_count} changed",
        file=sys.stderr,
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)

    p_backup = sub.add_parser("backup", help="copy manifest.json -> manifest.prev.json")
    p_backup.add_argument("--showcase-dir", required=True, type=Path)

    p_write = sub.add_parser("write", help="write new manifest.json")
    p_write.add_argument("--showcase-dir", required=True, type=Path)
    p_write.add_argument("--binary-hash", required=True)
    p_write.add_argument(
        "--results", required=True, type=Path, help="results.jsonl (fresh renders)"
    )
    p_write.add_argument(
        "--all-list", required=True, type=Path, help="full discovered sample list"
    )

    args = parser.parse_args()
    if args.mode == "backup":
        return cmd_backup(args.showcase_dir.resolve())
    if args.mode == "write":
        return cmd_write(
            args.showcase_dir.resolve(),
            args.binary_hash,
            args.results.resolve(),
            args.all_list.resolve(),
        )
    parser.error(f"unknown mode: {args.mode}")
    return 2


if __name__ == "__main__":
    sys.exit(main())
