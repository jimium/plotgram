#!/usr/bin/env bash
# 运行 plotgram-render 的 preview example，生成汇总 HTML 并在浏览器打开。
# 用法: scripts/render_preview.sh
set -euo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="target/render-preview"

cargo run -p plotgram-render --example preview
cargo run -p plotgram-render --example preview_ascii

python3 - "$OUT_DIR" <<'PY'
import html
import sys
from pathlib import Path

out_dir = Path(sys.argv[1])
svgs = sorted(out_dir.glob("preview.*.svg"))
if not svgs:
    sys.exit(f"no svg found in {out_dir}")

cards = []
for svg in svgs:
    theme = svg.name.removeprefix("preview.").removesuffix(".svg")
    size = svg.stat().st_size
    # 用 <img> 引用而非内联：各 SVG 的 defs id（arrow-head / hatch-*）互相隔离，避免跨主题串色
    cards.append(f"""
    <section class="card">
      <header>
        <h2>{html.escape(theme)}</h2>
        <span class="meta">{svg.name} · {size / 1024:.1f} KB</span>
      </header>
      <div class="canvas"><img src="{svg.name}" alt="{html.escape(theme)}"></div>
    </section>""")

# ASCII 后端输出：等宽 <pre> 卡片
for txt in sorted(out_dir.glob("preview.*.txt")):
    name = txt.name.removeprefix("preview.").removesuffix(".txt")
    size = txt.stat().st_size
    content = txt.read_text(encoding="utf-8")
    cards.append(f"""
    <section class="card">
      <header>
        <h2>{html.escape(name)}</h2>
        <span class="meta">{txt.name} · {size / 1024:.1f} KB</span>
      </header>
      <div class="canvas"><pre>{html.escape(content)}</pre></div>
    </section>""")

page = f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>plotgram-render preview</title>
<style>
  body {{ margin: 0; padding: 24px; background: #e8e8ec; color: #1a1a2e;
         font-family: 'Inter', 'Noto Sans CJK SC', -apple-system, sans-serif; }}
  h1 {{ font-size: 20px; margin: 0 0 20px; }}
  .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(560px, 1fr)); gap: 20px; }}
  .card {{ background: #fff; border-radius: 10px; overflow: hidden;
           box-shadow: 0 1px 4px rgba(0,0,0,.12); }}
  .card header {{ display: flex; align-items: baseline; gap: 10px;
                  padding: 10px 14px; border-bottom: 1px solid #e2e2e8; }}
  .card h2 {{ font-size: 15px; margin: 0; }}
  .meta {{ font-size: 12px; color: #888; }}
  .canvas {{ padding: 8px; }}
  .canvas img {{ display: block; width: 100%; height: auto; }}
  .canvas pre {{ margin: 0; padding: 12px; overflow: auto; font-size: 13px; line-height: 1.25;
                 font-family: 'SF Mono', 'Sarasa Mono SC', 'Noto Sans Mono CJK SC', Menlo, monospace;
                 background: #1e1e2e; color: #d9e0ee; border-radius: 6px; }}
</style>
</head>
<body>
<h1>plotgram-render preview <small style="font-weight:400;color:#666">({len(cards)} cards)</small></h1>
<div class="grid">{"".join(cards)}
</div>
</body>
</html>
"""

index = out_dir / "index.html"
index.write_text(page, encoding="utf-8")
print(f"wrote {index} ({index.stat().st_size} bytes)")
PY

INDEX="$OUT_DIR/index.html"
if command -v open >/dev/null 2>&1; then
  open "$INDEX"
else
  xdg-open "$INDEX"
fi
