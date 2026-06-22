#!/usr/bin/env python3
"""Generate the gtl-viewer app/tray icon (single source of truth).

Renders the "diff lines" concept: a dark-slate squircle holding stacked
code lines -- red removed, green added -- with a +/- gutter. Drawn at 4x
supersampling and downsampled with Lanczos for crisp antialiased edges.

Requires Pillow (`pip install --user pillow`). Run:
    python3 generate_icon.py            # writes icon.png (1024x1024) next to this script
"""
from __future__ import annotations
import os
from PIL import Image, ImageDraw

# Geometry is authored in a 512x512 design space (matching the approved mockup).
DESIGN = 512
FINAL = 1024          # emitted icon edge in px
SS = 4                # supersample factor
S = FINAL * SS        # working canvas edge
K = S / DESIGN        # design-space -> canvas scale

GREEN = (34, 197, 94)
RED = (239, 68, 68)
SLATE_TOP = (30, 41, 59)      # #1e293b
SLATE_BOT = (15, 23, 42)      # #0f172a
BORDER = (71, 85, 105)        # #475569


def sx(v: float) -> float:
    return v * K


def rrect(draw: ImageDraw.ImageDraw, box, radius, **kw):
    draw.rounded_rectangle(
        [sx(box[0]), sx(box[1]), sx(box[2]), sx(box[3])],
        radius=sx(radius), **kw,
    )


def vgradient(w: int, h: int, top, bot) -> Image.Image:
    """Vertical gradient via a 1xN strip resized up (smooth, no numpy)."""
    n = 256
    strip = Image.new("RGB", (1, n))
    px = strip.load()
    for y in range(n):
        t = y / (n - 1)
        px[0, y] = tuple(round(top[i] + (bot[i] - top[i]) * t) for i in range(3))
    return strip.resize((w, h), Image.BILINEAR)


def main() -> None:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # --- tile: rounded-square with vertical slate gradient -------------------
    tile_box = (40, 40, 472, 472)  # 432x432 within 512
    radius = 96
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [sx(tile_box[0]), sx(tile_box[1]), sx(tile_box[2]), sx(tile_box[3])],
        radius=sx(radius), fill=255,
    )
    tw = int(sx(tile_box[2]) - sx(tile_box[0]))
    th = int(sx(tile_box[3]) - sx(tile_box[1]))
    grad = vgradient(tw, th, SLATE_TOP, SLATE_BOT).convert("RGBA")
    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    tile.paste(grad, (int(sx(tile_box[0])), int(sx(tile_box[1]))))
    img = Image.composite(tile, img, mask)
    draw = ImageDraw.Draw(img)

    # subtle top sheen (white 10% -> 0 over the upper half)
    sheen = Image.new("L", (1, 256), 0)
    sp = sheen.load()
    for y in range(256):
        t = y / 255
        sp[0, y] = round(26 * max(0.0, 1 - t * 2))  # ~10% alpha peak
    sheen = sheen.resize((tw, th), Image.BILINEAR)
    white = Image.new("RGBA", (S, S), (255, 255, 255, 0))
    sheen_full = Image.new("L", (S, S), 0)
    sheen_full.paste(sheen, (int(sx(tile_box[0])), int(sx(tile_box[1]))))
    sheen_alpha = Image.composite(sheen_full, Image.new("L", (S, S), 0), mask)
    white.putalpha(sheen_alpha)
    img = Image.alpha_composite(img, white)
    draw = ImageDraw.Draw(img)

    # border
    rrect(draw, tile_box, radius, outline=BORDER + (140,), width=int(sx(2)))

    # --- diff rows -----------------------------------------------------------
    def row(y: float, color, w: float, sign: str):
        # faint full-width row highlight
        rrect(draw, (78, y - 12, 434, y + 32), 10, fill=color + (30,))
        # gutter sign
        rrect(draw, (92, y + 7, 120, y + 13), 3, fill=color)       # horizontal bar
        if sign == "+":
            rrect(draw, (103, y - 4, 109, y + 24), 3, fill=color)  # vertical bar
        # code line
        rrect(draw, (150, y + 1, 150 + w, y + 19), 9, fill=color)

    row(150, RED, 210, "-")
    row(208, RED, 150, "-")
    row(284, GREEN, 250, "+")
    row(342, GREEN, 180, "+")

    out = img.resize((FINAL, FINAL), Image.LANCZOS)
    dest = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon.png")
    out.save(dest)
    print(f"wrote {dest} ({FINAL}x{FINAL})")


if __name__ == "__main__":
    main()
