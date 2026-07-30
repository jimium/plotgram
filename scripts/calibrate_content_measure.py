#!/usr/bin/env python3
"""Calibrate / verify plotgram-content width heuristics against real fonts.

Eval-only side channel (ADR-005 §3): the Rust main path never opens font
files; this script measures true advance widths with Pillow and the fonts the
theme *assumes*, so the heuristic em-factor table in
`crates/plotgram-content/src/measure.rs` can be data-driven instead of
guessed. Char classes here MUST mirror `char_class()` in measure.rs.

Subcommands:
  calibrate                     Print per-class em-factor table (regular /
                                bold / mono) measured from the real fonts.
  gen-cases [--font-size N]     Emit a corpus of mixed-text cases as JSON for
                                the Rust estimator to fill in.
  compare --estimates FILE      Compare Rust estimates against Pillow truth;
                                report ratio distribution and overflow risks.
  verify-layout LAYOUT...       Check every RunBox in ContentLayout JSON dumps
                                against real advance widths (overflow -> exit 1).
  render-layout LAYOUT...       Draw ContentLayout with real fonts to PNG
                                (red boxes = estimated run boxes) for eyeballing.

Typical loop:
  python3 scripts/calibrate_content_measure.py calibrate
  python3 scripts/calibrate_content_measure.py gen-cases > /tmp/cases.json
  cargo run -p plotgram-content --example dump_estimates -- /tmp/cases.json > /tmp/estimates.json
  python3 scripts/calibrate_content_measure.py compare --estimates /tmp/estimates.json
"""
import argparse
import json
import statistics
import sys
import unicodedata
from pathlib import Path

from PIL import ImageFont

REPO = Path(__file__).resolve().parent.parent
FONT_REGULAR = REPO / "fonts" / "NotoSansCJKsc-Regular.otf"
FONT_BOLD = REPO / "fonts" / "NotoSansCJKsc-Bold.otf"
FONT_MONO = "/System/Library/Fonts/Menlo.ttc"  # code-run assumption

# Measure at a large size to minimize hinting/rounding noise.
PROBE_SIZE = 100

# ── char classes: MUST mirror char_class() in measure.rs ──────────────────

def char_class(ch: str) -> str:
    if unicodedata.east_asian_width(ch) in ("W", "F"):
        return "wide"
    # EAW-ambiguous but full-width (~1em) in Noto Sans CJK; mirror measure.rs.
    if ch in "·—‘’“”•…@%" or 0x2018 <= ord(ch) <= 0x201F or 0x2190 <= ord(ch) <= 0x21FF:
        return "wide"
    if ch == " ":
        return "space"
    if "A" <= ch <= "Z":
        return "upper"
    if "a" <= ch <= "z":
        return "lower"
    if "0" <= ch <= "9":
        return "digit"
    if ch in ".,:;!'|":
        return "punct_narrow"
    return "other"


CORPUS = {
    "wide": "布局引擎内容块度量估宽常用汉字样本流程架构服务数据（，。：；！？）「」…アイウエオ가나다",
    "upper": "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "lower": "abcdefghijklmnopqrstuvwxyz",
    "digit": "0123456789",
    "punct_narrow": ".,:;!'|",
    "space": " ",
    "other": "-_+=/\\<>?@#$%&*()[]{}\"~^`",
}

STYLE_FONT = {"plain": "regular", "emph": "regular", "strong": "bold", "code": "mono"}


def load_fonts(args):
    return {
        "regular": ImageFont.truetype(str(args.font_regular), PROBE_SIZE),
        "bold": ImageFont.truetype(str(args.font_bold), PROBE_SIZE),
        "mono": ImageFont.truetype(str(args.font_mono), PROBE_SIZE),
    }


def true_width(font, text: str, font_size: float) -> float:
    """Real advance width at font_size (scaled from the probe size)."""
    return font.getlength(text) * font_size / PROBE_SIZE


def true_width_fallback(mono, cjk, text: str, font_size: float) -> float:
    """Code runs: mono font lacks CJK glyphs; real renderers fall back to the
    CJK font per glyph. Mirror that so truth is not a notdef width."""
    total = 0.0
    for ch in text:
        font = cjk if char_class(ch) == "wide" else mono
        total += font.getlength(ch)
    return total * font_size / PROBE_SIZE


def run_truth(fonts, style: str, text: str, font_size: float) -> float:
    if style == "code":
        return true_width_fallback(fonts["mono"], fonts["regular"], text, font_size)
    return true_width(fonts[STYLE_FONT[style]], text, font_size)


# ── calibrate ──────────────────────────────────────────────────────────────

def cmd_calibrate(args):
    fonts = load_fonts(args)
    print(f"# em factors (advance / font_size), probe={PROBE_SIZE}px")
    print(f"# regular/bold: {args.font_regular.name}/{args.font_bold.name}, "
          f"mono: {Path(str(args.font_mono)).name}")
    print()
    print("| class | regular mean (min–max) | bold mean | mono mean |")
    print("|-------|------------------------|-----------|-----------|")
    table = {}
    for cls, corpus in CORPUS.items():
        row = {}
        for name, font in fonts.items():
            factors = [font.getlength(ch) / PROBE_SIZE for ch in corpus]
            row[name] = {
                "mean": statistics.mean(factors),
                "min": min(factors),
                "max": max(factors),
            }
        table[cls] = row
        r, b, m = row["regular"], row["bold"], row["mono"]
        print(f"| {cls} | {r['mean']:.3f} ({r['min']:.3f}–{r['max']:.3f})"
              f" | {b['mean']:.3f} | {m['mean']:.3f} |")
    print()
    print("→ 把 mean 值（宽度安全起见可取 mean 与 max 之间）填进"
          " measure.rs 的 em_factor() 占位表。")
    if args.json:
        Path(args.json).write_text(json.dumps(table, indent=2, ensure_ascii=False))
        print(f"written: {args.json}")


# ── gen-cases ──────────────────────────────────────────────────────────────

MIXED_TEXTS = [
    "校验库存",
    "落库并发消息",
    "处理下单主链路：",
    "用户登录 API 网关",
    "Check inventory levels",
    "HTTP 504 Gateway Timeout",
    "重试 3 次后进入死信队列 DLQ",
    "order.created",
    "kafka: topic=order.created, partition=3",
    "幂等键去重（Redis SETNX, TTL 24h）",
    "责任：鉴权、限流、路由",
    "μs 级延迟 — 99.9% SLA",
]


def cmd_gen_cases(args):
    cases = []
    for text in MIXED_TEXTS:
        for style in ("plain", "strong", "emph", "code"):
            cases.append({"text": text, "style": style, "font_size": args.font_size})
    json.dump(cases, sys.stdout, ensure_ascii=False, indent=2)
    print()


# ── compare ────────────────────────────────────────────────────────────────

def cmd_compare(args):
    fonts = load_fonts(args)
    entries = json.loads(Path(args.estimates).read_text())
    rows = []
    for e in entries:
        est = e["est_width"]
        truth = run_truth(fonts, e["style"], e["text"], e["font_size"])
        rows.append((est / truth if truth > 0 else 1.0, est, truth, e))

    ratios = sorted(r[0] for r in rows)
    n = len(ratios)
    p = lambda q: ratios[min(n - 1, int(q * n))]
    eps = 1e-6
    overflow = [r for r in rows if r[0] < args.min_ratio - eps]
    fat = [r for r in rows if r[0] > args.max_ratio + eps]

    print(f"cases: {n}   ratio = est/truth   target: [{args.min_ratio}, {args.max_ratio}]")
    print(f"p50 {p(0.50):.3f}   p05 {p(0.05):.3f}   p95 {p(0.95):.3f}   "
          f"min {ratios[0]:.3f}   max {ratios[-1]:.3f}")
    print(f"underestimate (<{args.min_ratio}, 溢出风险): {len(overflow)}   "
          f"overestimate (>{args.max_ratio}, 过肥): {len(fat)}")
    for tag, bad in (("UNDER", overflow), ("OVER", fat)):
        for ratio, est, truth, e in sorted(bad, key=lambda r: r[0])[:10]:
            print(f"  [{tag}] {ratio:.3f} est={est:.1f} truth={truth:.1f} "
                  f"style={e['style']:6s} text={e['text']!r}")
    sys.exit(1 if overflow else 0)


# ── verify-layout / render-layout ────────────────────────────────────────────

def cmd_verify_layout(args):
    fonts = load_fonts(args)
    eps = 1e-6
    total, under = 0, 0
    for path in args.layouts:
        lay = json.loads(Path(path).read_text())
        fs = lay["font_size"]
        worst = 99.0
        for line in lay["lines"]:
            for run in line["runs"]:
                truth = run_truth(fonts, run["style"], run["text"], fs)
                ratio = run["width"] / truth if truth > 0 else 1.0
                total += 1
                worst = min(worst, ratio)
                if ratio < 1.0 - eps:
                    under += 1
                    print(f"  UNDER {Path(path).stem} {ratio:.3f} "
                          f"{run['style']:6s} {run['text']!r}")
        # Containment: every run box must sit inside the declared size.
        right = max((r["x"] + r["width"] for l in lay["lines"] for r in l["runs"]), default=0.0)
        bottom = max((l["top"] + l["height"] for l in lay["lines"]), default=0.0)
        ok_box = right <= lay["width"] + eps and bottom <= lay["height"] + eps
        if not ok_box:
            under += 1
            print(f"  BOX {Path(path).stem}: runs exceed declared size")
        print(f"{Path(path).stem}: worst run ratio {worst:.3f}  box {'ok' if ok_box else 'FAIL'}")
    print(f"runs checked: {total}   underestimates: {under}")
    sys.exit(1 if under else 0)


def cmd_render_layout(args):
    from PIL import Image, ImageDraw
    from PIL import ImageFont as IF

    s, pad = args.scale, args.pad
    ink = (31, 36, 48)
    for path in args.layouts:
        lay = json.loads(Path(path).read_text())
        size = round(lay["font_size"] * s)
        f_reg = IF.truetype(str(args.font_regular), size)
        f_bold = IF.truetype(str(args.font_bold), size)
        f_mono = IF.truetype(str(args.font_mono), size)

        w = int((lay["width"] + 2 * pad) * s)
        h = int((lay["height"] + 2 * pad) * s)
        img = Image.new("RGB", (w, h), (255, 255, 255))
        d = ImageDraw.Draw(img)
        d.rectangle([0, 0, w - 1, h - 1], outline=(138, 147, 166), width=s)

        for line in lay["lines"]:
            base_y = (line["baseline"] + pad) * s
            top = (line["top"] + pad) * s
            bot = (line["top"] + line["height"] + pad) * s
            for run in line["runs"]:
                x = (run["x"] + pad) * s
                # Estimated run box in red: real ink must stay inside.
                d.rectangle([x, top, x + run["width"] * s, bot], outline=(230, 140, 140))
                if run["style"] == "code":
                    cx = x  # per-glyph mono/CJK fallback, like real renderers
                    for ch in run["text"]:
                        f = f_reg if char_class(ch) == "wide" else f_mono
                        d.text((cx, base_y), ch, font=f, fill=ink, anchor="ls")
                        cx += f.getlength(ch)
                else:
                    f = f_bold if run["style"] == "strong" else f_reg
                    d.text((x, base_y), run["text"], font=f, fill=ink, anchor="ls")
        for y in lay["rules"]:
            yy = (y + pad) * s
            d.line([(pad * s, yy), ((lay["width"] + pad) * s, yy)],
                   fill=(195, 201, 212), width=max(1, int(lay["rule_thickness"] * s)))

        out = str(path).replace(".layout.json", ".png")
        img.save(out)
        print(f"rendered: {out}")


# ── main ───────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--font-regular", type=Path, default=FONT_REGULAR)
    ap.add_argument("--font-bold", type=Path, default=FONT_BOLD)
    ap.add_argument("--font-mono", default=FONT_MONO)
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("calibrate", help="print per-class em-factor table")
    s.add_argument("--json", help="also write the table as JSON")
    s.set_defaults(func=cmd_calibrate)

    s = sub.add_parser("gen-cases", help="emit corpus cases JSON to stdout")
    s.add_argument("--font-size", type=float, default=14.0)
    s.set_defaults(func=cmd_gen_cases)

    s = sub.add_parser("compare", help="compare Rust estimates vs font truth")
    s.add_argument("--estimates", required=True)
    s.add_argument("--min-ratio", type=float, default=1.0)
    s.add_argument("--max-ratio", type=float, default=1.2)
    s.set_defaults(func=cmd_compare)

    s = sub.add_parser("verify-layout", help="check RunBox widths vs font truth")
    s.add_argument("layouts", nargs="+")
    s.set_defaults(func=cmd_verify_layout)

    s = sub.add_parser("render-layout", help="draw ContentLayout to PNG with real fonts")
    s.add_argument("layouts", nargs="+")
    s.add_argument("--scale", type=int, default=2)
    s.add_argument("--pad", type=float, default=16.0)
    s.set_defaults(func=cmd_render_layout)

    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
