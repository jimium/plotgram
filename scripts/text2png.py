#!/usr/bin/env python3
"""Render a Unicode text file (box-drawing art) to PNG, simulating a terminal.

Usage: python3 scripts/text2png.py input.txt [-o output.png] [--scale 2]

Uses Menlo for ASCII/box-drawing and Noto Sans CJK for wide characters,
placing each glyph on a fixed cell grid (like a real terminal emulator).
"""
import argparse
import sys
import unicodedata
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# ── Fonts ──────────────────────────────────────────────────────────────────
MENLO_PATH = "/System/Library/Fonts/Menlo.ttc"
NOTO_CJK_PATH = str(Path(__file__).resolve().parent.parent / "fonts" / "NotoSansCJKsc-Regular.otf")

# ── Cell metrics (at font_size=16) ────────────────────────────────────────
FONT_SIZE = 16
CELL_W = 10  # Menlo char width at 16px
CELL_H = 20  # line height
PAD_X = 20   # image padding
PAD_Y = 16


def char_width(ch: str) -> int:
    """Display width: 2 for Wide/Fullwidth East Asian, 1 otherwise."""
    eaw = unicodedata.east_asian_width(ch)
    return 2 if eaw in ('W', 'F') else 1


def is_cjk(ch: str) -> bool:
    """Whether to use the CJK font for this character."""
    cp = ord(ch)
    return (
        0x4E00 <= cp <= 0x9FFF or   # CJK Unified
        0x3400 <= cp <= 0x4DBF or   # Extension A
        0x3000 <= cp <= 0x303F or   # CJK Symbols
        0xFF00 <= cp <= 0xFFEF or   # Fullwidth
        0x2E80 <= cp <= 0x2EFF or   # Radicals
        0xF900 <= cp <= 0xFAFF      # Compatibility
    )


def render_text_to_png(text: str, out_path: str, scale: int = 2):
    lines = text.rstrip('\n').split('\n')

    # Compute image dimensions
    max_cols = 0
    for line in lines:
        cols = sum(char_width(ch) for ch in line)
        max_cols = max(max_cols, cols)

    img_w = PAD_X * 2 + max_cols * CELL_W
    img_h = PAD_Y * 2 + len(lines) * CELL_H

    # Create image (dark background like a terminal)
    bg_color = (30, 30, 30)
    fg_color = (220, 220, 220)
    img = Image.new('RGB', (img_w, img_h), bg_color)
    draw = ImageDraw.Draw(img)

    # Load fonts
    font_mono = ImageFont.truetype(MENLO_PATH, FONT_SIZE)
    font_cjk = ImageFont.truetype(NOTO_CJK_PATH, FONT_SIZE)

    # Render each character at its grid position
    for row_idx, line in enumerate(lines):
        col = 0
        for ch in line:
            if ch == ' ':
                col += 1
                continue
            x = PAD_X + col * CELL_W
            y = PAD_Y + row_idx * CELL_H
            font = font_cjk if is_cjk(ch) else font_mono
            # Color accents for box-drawing vs text
            color = fg_color
            cp = ord(ch)
            if ch in '╌╎·¦':
                color = (160, 140, 200)  # purple for dashed
            elif 0x2500 <= cp <= 0x257F:  # Box Drawing block
                color = (140, 180, 220)  # blue-ish for lines
            elif ch in '▶▼◀▲':
                color = (240, 180, 80)   # orange for arrows
            draw.text((x, y), ch, font=font, fill=color)
            col += char_width(ch)

    # Scale up for clarity
    if scale > 1:
        img = img.resize((img_w * scale, img_h * scale), Image.NEAREST)

    img.save(out_path)
    print(f"Saved: {out_path} ({img.width}×{img.height})")


def main():
    parser = argparse.ArgumentParser(description="Text art → PNG")
    parser.add_argument("input", help="Input .txt file")
    parser.add_argument("-o", "--output", help="Output .png path")
    parser.add_argument("--scale", type=int, default=2, help="Upscale factor")
    args = parser.parse_args()

    text = Path(args.input).read_text(encoding="utf-8")
    out = args.output or str(Path(args.input).with_suffix('.png'))
    render_text_to_png(text, out, scale=args.scale)


if __name__ == "__main__":
    main()
