#!/usr/bin/env python3
"""本地服务：可视化 collinear baseline 变化。

用法（仓库根或本目录均可）:
  ./benchmark-data/serve-viewer.py
  ./benchmark-data/serve-viewer.py --port 8765

浏览器打开 http://127.0.0.1:8765 （趋势）与 /readme（说明文档）
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

DIR = Path(__file__).resolve().parent
VIEWER = DIR / "viewer"
BASELINE_RE = re.compile(r"^collinear-baseline-.+\.json$")


def list_baselines() -> list[dict]:
    items: list[dict] = []
    for path in sorted(DIR.glob("collinear-baseline-*.json")):
        if not BASELINE_RE.match(path.name):
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            items.append(
                {
                    "file": path.name,
                    "error": str(e),
                    "is_latest": path.name == "collinear-baseline-latest.json",
                }
            )
            continue
        samples = data.get("samples") or []
        items.append(
            {
                "file": path.name,
                "date": data.get("date"),
                "note": data.get("note"),
                "perf_runs": data.get("perf_runs"),
                "sample_count": len(samples),
                "is_latest": path.name == "collinear-baseline-latest.json",
            }
        )
    # dated first by date, latest last as alias
    dated = [x for x in items if not x.get("is_latest")]
    latest = [x for x in items if x.get("is_latest")]
    dated.sort(key=lambda x: (x.get("date") or "", x["file"]))
    return dated + latest


def load_baseline(name: str) -> dict | None:
    name = Path(name).name
    if not BASELINE_RE.match(name):
        return None
    path = DIR / name
    if not path.is_file() or not path.resolve().is_relative_to(DIR.resolve()):
        return None
    return json.loads(path.read_text(encoding="utf-8"))


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
            readme = DIR / "README.html"
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
            # Aggregate time series across dated snapshots (skip latest alias)
            series = []
            for meta in list_baselines():
                if meta.get("is_latest") or meta.get("error"):
                    continue
                data = load_baseline(meta["file"])
                if not data:
                    continue
                samples = data.get("samples") or []
                totals = {
                    "exact_sev": 0.0,
                    "tight_sev": 0.0,
                    "lint_err": 0,
                    "through": 0,
                    "group_interior": 0,
                    "median_ms": 0.0,
                    "ms_count": 0,
                }
                per_file = {}
                for s in samples:
                    name = (s.get("file") or "").rsplit("/", 1)[-1]
                    lint = s.get("lint") or {}
                    exact = float(s.get("exact_sev") or 0)
                    tight = float(s.get("tight_sev") or 0)
                    err = int(lint.get("error_count") or 0)
                    through = int(lint.get("edge_through_node") or 0)
                    gi = int(lint.get("edge_crosses_group_interior") or 0)
                    ms = s.get("median_ms")
                    totals["exact_sev"] += exact
                    totals["tight_sev"] += tight
                    totals["lint_err"] += err
                    totals["through"] += through
                    totals["group_interior"] += gi
                    if isinstance(ms, (int, float)):
                        totals["median_ms"] += float(ms)
                        totals["ms_count"] += 1
                    per_file[name] = {
                        "exact_sev": exact,
                        "tight_sev": tight,
                        "lint_err": err,
                        "through": through,
                        "group_interior": gi,
                        "median_ms": ms,
                        "node_fp": (s.get("node_fp") or "")[:12],
                        "det": s.get("det"),
                    }
                avg_ms = (
                    totals["median_ms"] / totals["ms_count"]
                    if totals["ms_count"]
                    else None
                )
                series.append(
                    {
                        "file": meta["file"],
                        "date": data.get("date") or meta.get("date"),
                        "note": data.get("note") or "",
                        "totals": {
                            "exact_sev": round(totals["exact_sev"], 2),
                            "tight_sev": round(totals["tight_sev"], 2),
                            "lint_err": totals["lint_err"],
                            "through": totals["through"],
                            "group_interior": totals["group_interior"],
                            "avg_median_ms": round(avg_ms, 2) if avg_ms is not None else None,
                            "samples": len(samples),
                        },
                        "per_file": per_file,
                    }
                )
            self._json(200, {"series": series})
            return

        self._json(404, {"error": "not found", "path": path})


def main() -> None:
    ap = argparse.ArgumentParser(description="Serve collinear baseline viewer")
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
    print(f"data dir: {DIR}")
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
