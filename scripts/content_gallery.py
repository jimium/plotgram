#!/usr/bin/env python3
"""One-page gallery for plotgram-content: rendered results vs estimates.

Rebuilds every sample (SVG + ContentLayout JSON via the Rust example, true-font
PNG via calibrate_content_measure.py render-layout), computes per-run
estimate/truth ratios with the real fonts, and writes a single self-contained
HTML file (PNGs inlined as data URLs, no server needed) that shows all samples
side by side:

  left  — engine SVG as the browser renders it, with the estimated content box
          (green) and run boxes (blue, toggleable) overlaid;
  right — true-font raster (Pillow + Noto CJK / Menlo, red boxes = estimated
          run boxes): the ground truth the heuristic is calibrated against.

Usage:
  python3 scripts/content_gallery.py            # rebuild + generate + open
  python3 scripts/content_gallery.py --no-build # regenerate HTML only
  python3 scripts/content_gallery.py --no-open  # do not open the browser
Output:
  target/content-samples/index.html  (single file, opens via file://)
"""
import argparse
import base64
import html
import json
import subprocess
import sys
import webbrowser
from pathlib import Path
from types import SimpleNamespace

REPO = Path(__file__).resolve().parent.parent
OUT_DIR = REPO / "target" / "content-samples"

sys.path.insert(0, str(REPO / "scripts"))
import calibrate_content_measure as cal  # noqa: E402  (shared truth helpers)

PNG_SCALE = 2  # render-layout supersampling; displayed at logical px


def rebuild() -> None:
    subprocess.run(
        ["cargo", "run", "-q", "-p", "plotgram-content", "--example", "render_samples"],
        cwd=REPO, check=True,
    )
    layouts = sorted(str(p) for p in OUT_DIR.glob("*.layout.json"))
    subprocess.run(
        [sys.executable, str(REPO / "scripts" / "calibrate_content_measure.py"),
         "render-layout", *layouts, "--scale", str(PNG_SCALE)],
        cwd=REPO, check=True,
    )


def sample_order() -> list[str]:
    """Sample order as authored in render_samples.rs (meta files carry no
    index, so ask the example itself — it prints names in order)."""
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "plotgram-content", "--example", "render_samples"],
        cwd=REPO, check=True, capture_output=True, text=True,
    ).stdout
    return [line.split(":")[0] for line in out.splitlines() if ": " in line and " x " in line]


def run_ratios(fonts, lay) -> list[float]:
    fs = lay["font_size"]
    return [
        run["width"] / t if (t := cal.run_truth(fonts, run["style"], run["text"], fs)) > 0 else 1.0
        for line in lay["lines"]
        for run in line["runs"]
    ]


def overlay_divs(lay, pad: float) -> str:
    parts = [
        f'<div class="ov est" style="left:{pad}px;top:{pad}px;'
        f'width:{lay["width"]:.1f}px;height:{lay["height"]:.1f}px"></div>'
    ]
    for line in lay["lines"]:
        for run in line["runs"]:
            parts.append(
                f'<div class="ov run" style="left:{pad + run["x"]:.1f}px;'
                f'top:{pad + line["top"]:.1f}px;width:{run["width"]:.1f}px;'
                f'height:{line["height"]:.1f}px"></div>'
            )
    return "".join(parts)


CSS = """
  :root { --ink:#1f2430; --mut:#6b7280; --line:#e3e6ec; --card:#ffffff; }
  * { box-sizing: border-box; }
  body { margin:0; padding:28px 36px 64px; background:#f4f5f8; color:var(--ink);
         font:14px/1.6 -apple-system,"PingFang SC","Noto Sans CJK SC",sans-serif; }
  h1 { font-size:21px; margin:0 0 4px; }
  .sub { color:var(--mut); margin-bottom:10px; }
  .legend { display:flex; gap:18px; align-items:center; flex-wrap:wrap;
            font-size:13px; color:var(--mut); margin-bottom:24px; }
  .sw { display:inline-block; width:14px; height:14px; border-radius:3px;
        vertical-align:-2px; margin-right:6px; }
  .sw.est { border:2px solid #2f9e63; } .sw.run { border:1.5px dashed #4c7dd4; }
  .sw.png { border:1.5px solid #e68c8c; }
  section.sample { background:var(--card); border:1px solid var(--line); border-radius:10px;
                   padding:18px 22px; margin-bottom:26px; }
  .head { display:flex; justify-content:space-between; align-items:baseline;
          gap:16px; flex-wrap:wrap; }
  .head h2 { font-size:16px; margin:0; font-family:Menlo,monospace; }
  .stats { font-size:13px; color:var(--mut); }
  .stats b { color:var(--ink); font-weight:600; }
  .stats .ok { color:#2f9e63; font-weight:600; } .stats .bad { color:#c0392b; font-weight:700; }
  pre.src { background:#f7f8fa; border:1px solid var(--line); border-radius:6px;
            padding:10px 14px; font:12px/1.7 Menlo,monospace; white-space:pre-wrap;
            margin:12px 0 14px; color:#444a57; }
  .panels { display:flex; gap:28px; flex-wrap:wrap; align-items:flex-start; }
  figure { margin:0; } figcaption { font-size:12px; color:var(--mut); margin-bottom:8px; }
  .svgwrap { position:relative; display:inline-block; }
  .svgwrap svg { display:block; }
  .ov { position:absolute; pointer-events:none; }
  .ov.est { outline:2px solid rgba(47,158,99,.75); }
  .ov.run { outline:1.5px dashed rgba(76,125,212,.65); display:none; }
  body.show-runs .ov.run { display:block; }
  img.truth { display:block; image-rendering:auto; }
  label.toggle { user-select:none; cursor:pointer; }
"""


def data_url(path: Path) -> str:
    """Inline a PNG so the page is one portable file (works from file://)."""
    return "data:image/png;base64," + base64.b64encode(path.read_bytes()).decode()


def build_page(names: list[str]) -> str:
    fonts = cal.load_fonts(SimpleNamespace(
        font_regular=cal.FONT_REGULAR, font_bold=cal.FONT_BOLD, font_mono=cal.FONT_MONO))
    sections, all_ratios = [], []
    for name in names:
        lay = json.loads((OUT_DIR / f"{name}.layout.json").read_text())
        meta = json.loads((OUT_DIR / f"{name}.meta.json").read_text())
        svg = (OUT_DIR / f"{name}.svg").read_text()
        pad = meta["pad"]
        ratios = run_ratios(fonts, lay)
        all_ratios += ratios
        under = sum(1 for r in ratios if r < 1.0 - 1e-6)
        mw = meta["max_width"]
        mw_txt = f'{mw:.0f}px' if mw is not None else "—（不换行）"
        extras = ""
        if meta.get("align", "left") != "left":
            extras += f' · align <b>{meta["align"]}</b>'
        if meta.get("max_lines") is not None:
            extras += f' · max_lines <b>{meta["max_lines"]}</b>（截断 + …）'
        ratio_cls = "bad" if under else "ok"
        disp_w = lay["width"] + 2 * pad
        stats = (
            f'预估 <b>{lay["width"]:.1f} × {lay["height"]:.1f}</b>'
            f' · {len(lay["lines"])} 行 / {len(lay["rules"])} 分隔线'
            f' · max_width {mw_txt}{extras}'
            f' · {len(ratios)} runs，宽度比 <span class="{ratio_cls}">'
            f'{min(ratios):.3f} – {max(ratios):.3f}</span>'
            + (f' · <span class="bad">低估 {under}!</span>' if under else '')
        )
        sections.append(f"""
<section class="sample" id="{name}">
  <div class="head"><h2>{name}</h2><div class="stats">{stats}</div></div>
  <pre class="src">{html.escape(meta["text"])}</pre>
  <div class="panels">
    <figure>
      <figcaption>引擎 SVG（浏览器字体渲染）＋ 预估内容盒 / run 盒</figcaption>
      <div class="svgwrap">{svg}{overlay_divs(lay, pad)}</div>
    </figure>
    <figure>
      <figcaption>真字体真值（Pillow · Noto CJK + Menlo，红框 = 预估 run 盒）</figcaption>
      <img class="truth" src="{data_url(OUT_DIR / f'{name}.png')}" width="{disp_w:.0f}" alt="{name} truth render">
    </figure>
  </div>
</section>""")

    n = len(all_ratios)
    total_under = sum(1 for r in all_ratios if r < 1.0 - 1e-6)
    verdict = ('<span class="ok" style="color:#2f9e63;font-weight:600">零低估 ✓</span>'
               if total_under == 0 else
               f'<span style="color:#c0392b;font-weight:700">低估 {total_under}</span>')
    return f"""<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>plotgram-content 渲染 vs 预估画廊</title>
<style>{CSS}</style></head>
<body class="show-runs">
<h1>plotgram-content：渲染结果 vs 尺寸预估</h1>
<div class="sub">measure 为唯一几何写者（ADR-005）：左侧为引擎 SVG 输出与预估几何叠加，
右侧为真字体光栅真值。全部 {len(names)} 个样例 · {n} 个 run · {verdict} ·
比率区间 {min(all_ratios):.3f} – {max(all_ratios):.3f}（硬约束：下界 ≥ 1.0 防溢出；
混排语料上界目标 1.15，标点等短 run 允许略肥）</div>
<div class="legend">
  <span><span class="sw est"></span>预估内容盒（width × height，engine 拿到的尺寸）</span>
  <span><span class="sw run"></span>预估 run 盒（度量相定死的行内几何）</span>
  <span><span class="sw png"></span>真值图红框 = 同一预估 run 盒，墨迹必须在框内</span>
  <label class="toggle"><input type="checkbox" checked
    onchange="document.body.classList.toggle('show-runs',this.checked)"> 显示 run 盒</label>
</div>
{"".join(sections)}
</body></html>
"""


def open_in_browser(path: Path) -> None:
    if sys.platform == "darwin":
        subprocess.run(["open", str(path)], check=False)  # Launch Services, no AppleScript
    else:
        webbrowser.open(path.as_uri())


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--no-build", action="store_true",
                    help="skip cargo/PNG rebuild, regenerate HTML only")
    ap.add_argument("--no-open", action="store_true",
                    help="do not open the page in the default browser")
    args = ap.parse_args()
    if not args.no_build:
        rebuild()
    names = sample_order()
    out = OUT_DIR / "index.html"
    out.write_text(build_page(names), encoding="utf-8")
    print(f"gallery: {out}  ({len(names)} samples)")
    if not args.no_open:
        open_in_browser(out)


if __name__ == "__main__":
    main()
