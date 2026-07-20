#!/usr/bin/env python3
"""本地服务：可视化门禁 baseline 变化（角色感知）。

用法（仓库根或本目录均可）:
  ./benchmarks/serve-viewer.py
  ./benchmarks/serve-viewer.py --port 8765

浏览器打开 http://127.0.0.1:8765 （趋势）与 /readme（说明文档）

无退化主叙事看 product；stress/demo 为观测轨。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import webbrowser
from collections import defaultdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

SCRIPT_DIR = Path(__file__).resolve().parent
BM = SCRIPT_DIR.parent  # benchmarks/
VIEWER = BM / "viewer"
BASELINES = BM / "baselines"
# latest.json 或 YYYY-MM-DD.json
BASELINE_RE = re.compile(r"^(latest|\d{4}-\d{2}-\d{2})\.json$")
ROLE_ORDER = ("smoke", "product", "demo", "stress", "mech")


def derive_role(sample: dict) -> str:
    role = sample.get("role")
    if isinstance(role, str) and role in ROLE_ORDER:
        return role
    name = (sample.get("file") or "").rsplit("/", 1)[-1]
    head = name.split(".", 1)[0]
    return head if head in ROLE_ORDER else "product"


def empty_totals() -> dict:
    return {
        "exact_sev": 0.0,
        "tight_sev": 0.0,
        "lint_err": 0,
        "through": 0,
        "group_interior": 0,
        "median_ms": 0.0,
        "ms_count": 0,
        "samples": 0,
    }


def accumulate(totals: dict, sample: dict) -> None:
    lint = sample.get("lint") or {}
    exact = float(sample.get("exact_sev") or 0)
    tight = float(sample.get("tight_sev") or 0)
    err = int(lint.get("error_count") or 0)
    through = int(lint.get("edge_through_node") or 0)
    gi = int(lint.get("edge_crosses_group_interior") or 0)
    ms = sample.get("median_ms")
    totals["exact_sev"] += exact
    totals["tight_sev"] += tight
    totals["lint_err"] += err
    totals["through"] += through
    totals["group_interior"] += gi
    totals["samples"] += 1
    if isinstance(ms, (int, float)):
        totals["median_ms"] += float(ms)
        totals["ms_count"] += 1


def finalize_totals(totals: dict) -> dict:
    avg_ms = (
        totals["median_ms"] / totals["ms_count"] if totals["ms_count"] else None
    )
    return {
        "exact_sev": round(totals["exact_sev"], 2),
        "tight_sev": round(totals["tight_sev"], 2),
        "lint_err": totals["lint_err"],
        "through": totals["through"],
        "group_interior": totals["group_interior"],
        "avg_median_ms": round(avg_ms, 2) if avg_ms is not None else None,
        "samples": totals["samples"],
    }


def list_baselines() -> list[dict]:
    items: list[dict] = []
    if not BASELINES.is_dir():
        return items
    for path in sorted(BASELINES.glob("*.json")):
        if not BASELINE_RE.match(path.name):
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            items.append(
                {
                    "file": path.name,
                    "error": str(e),
                    "is_latest": path.name == "latest.json",
                }
            )
            continue
        samples = data.get("samples") or []
        role_counts: dict[str, int] = defaultdict(int)
        for s in samples:
            role_counts[derive_role(s)] += 1
        items.append(
            {
                "file": path.name,
                "date": data.get("date"),
                "note": data.get("note"),
                "perf_runs": data.get("perf_runs"),
                "sample_count": len(samples),
                "role_counts": dict(role_counts),
                "is_latest": path.name == "latest.json",
            }
        )
    dated = [x for x in items if not x.get("is_latest")]
    latest = [x for x in items if x.get("is_latest")]
    dated.sort(key=lambda x: (x.get("date") or "", x["file"]))
    return dated + latest


def load_baseline(name: str) -> dict | None:
    name = Path(name).name
    if not BASELINE_RE.match(name):
        return None
    path = BASELINES / name
    if not path.is_file() or not path.resolve().is_relative_to(BASELINES.resolve()):
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    # 兼容旧快照：补 role
    for s in data.get("samples") or []:
        if "role" not in s:
            s["role"] = derive_role(s)
    return data


def build_series_point(meta: dict, data: dict) -> dict:
    samples = data.get("samples") or []
    by_role_acc: dict[str, dict] = defaultdict(empty_totals)
    all_acc = empty_totals()
    per_file = {}
    for s in samples:
        role = derive_role(s)
        accumulate(by_role_acc[role], s)
        accumulate(all_acc, s)
        name = (s.get("file") or "").rsplit("/", 1)[-1]
        lint = s.get("lint") or {}
        per_file[name] = {
            "role": role,
            "exact_sev": float(s.get("exact_sev") or 0),
            "tight_sev": float(s.get("tight_sev") or 0),
            "lint_err": int(lint.get("error_count") or 0),
            "through": int(lint.get("edge_through_node") or 0),
            "group_interior": int(lint.get("edge_crosses_group_interior") or 0),
            "median_ms": s.get("median_ms"),
            "node_fp": (s.get("node_fp") or "")[:12],
            "det": s.get("det"),
        }
    by_role = {r: finalize_totals(t) for r, t in by_role_acc.items()}
    by_role["all"] = finalize_totals(all_acc)
    return {
        "file": meta["file"],
        "date": data.get("date") or meta.get("date"),
        "note": data.get("note") or "",
        "role_counts": {
            r: by_role[r]["samples"] for r in ROLE_ORDER if r in by_role
        },
        "by_role": by_role,
        # 兼容旧前端：totals = product（无 product 时退回 all）
        "totals": by_role.get("product") or by_role["all"],
        "per_file": per_file,
    }


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("[%s] %s\n" % (self.log_date_time_string(), fmt % args))

    def _send(self, code: int, body: bytes, content_type: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, code: int, obj: object) -> None:
        body = json.dumps(obj, ensure_ascii=False, indent=2).encode("utf-8")
        self._send(code, body, "application/json; charset=utf-8")

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        path = unquote(parsed.path)

        if path in ("/", "/index.html"):
            html = (VIEWER / "index.html").read_bytes()
            self._send(200, html, "text/html; charset=utf-8")
            return

        if path in ("/readme", "/readme.html", "/README.html"):
            readme = BM / "README.html"
            if not readme.is_file():
                self._json(404, {"error": "README.html missing"})
                return
            self._send(200, readme.read_bytes(), "text/html; charset=utf-8")
            return

        if path.startswith("/static/"):
            rel = path[len("/static/") :]
            fpath = (VIEWER / rel).resolve()
            if not fpath.is_file() or not fpath.is_relative_to(VIEWER.resolve()):
                self._json(404, {"error": "not found"})
                return
            ctype = {
                ".css": "text/css; charset=utf-8",
                ".js": "application/javascript; charset=utf-8",
                ".svg": "image/svg+xml",
            }.get(fpath.suffix, "application/octet-stream")
            self._send(200, fpath.read_bytes(), ctype)
            return

        if path == "/api/baselines":
            self._json(200, {"baselines": list_baselines()})
            return

        if path.startswith("/api/baselines/"):
            name = path[len("/api/baselines/") :]
            data = load_baseline(name)
            if data is None:
                self._json(404, {"error": f"baseline not found: {name}"})
                return
            self._json(200, data)
            return

        if path == "/api/series":
            series = []
            for meta in list_baselines():
                if meta.get("is_latest") or meta.get("error"):
                    continue
                data = load_baseline(meta["file"])
                if not data:
                    continue
                series.append(build_series_point(meta, data))
            self._json(200, {"series": series, "role_order": list(ROLE_ORDER)})
            return

        self._json(404, {"error": "not found", "path": path})


def main() -> None:
    ap = argparse.ArgumentParser(description="Serve gate baseline viewer")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--no-open", action="store_true", help="不要自动打开浏览器")
    args = ap.parse_args()

    if not (VIEWER / "index.html").is_file():
        print(f"error: missing {VIEWER / 'index.html'}", file=sys.stderr)
        sys.exit(1)

    httpd = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{args.port}/"
    print(f"baseline viewer → {url}")
    print(f"baselines: {BASELINES}")
    print("Ctrl+C to stop")
    if not args.no_open:
        try:
            webbrowser.open(url)
        except Exception:
            pass
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nbye")
        httpd.server_close()


if __name__ == "__main__":
    main()
