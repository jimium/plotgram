#!/usr/bin/env python3
"""本地静态文件服务（无缓存），供 showcase 画廊等开发预览使用。"""

from __future__ import annotations

import argparse
from functools import partial
from http.server import HTTPServer, SimpleHTTPRequestHandler


class NoCacheRequestHandler(SimpleHTTPRequestHandler):
    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()

    def do_GET(self) -> None:
        # 忽略条件请求，避免 304 导致浏览器复用旧 SVG
        if "If-Modified-Since" in self.headers:
            self.headers["If-Modified-Since"] = "Thu, 01 Jan 1970 00:00:00 GMT"
        super().do_GET()


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve apps/ with no caching")
    parser.add_argument("port", type=int, nargs="?", default=8030)
    parser.add_argument("-d", "--directory", default=".")
    args = parser.parse_args()

    handler = partial(NoCacheRequestHandler, directory=args.directory)
    with HTTPServer(("", args.port), handler) as httpd:
        httpd.serve_forever()


if __name__ == "__main__":
    main()
