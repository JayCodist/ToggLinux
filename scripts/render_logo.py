#!/usr/bin/env python3
"""Render the stopwatch logo (the design in the toggl-linux-stopwatch SVG).

Produces two outputs:
  1. app-icon.png   full-color 1024x1024 tile. Feed to `npm run icons`.
  2. tray-mark.bin  a 48x48 coverage mask as raw alpha bytes for tray.rs
                    (include_bytes!). The tray tints this mask per state:
                    accent while running, light grey while idle. The chevron
                    and the underscore are carved out as negative space, so
                    the monochrome tray mark stays legible on any panel
                    color. The mask is cropped to the mark's bounding box
                    and rescaled to fill the slot: the panel scales the
                    whole image into its icon slot, so any margin baked
                    into the mask shrinks the stopwatch on the panel.

Geometry mirrors the SVG by hand. All coordinates are 200x200 viewBox
units, multiplied by one scale factor. Colors match styles.css:
body #dd3873 (--accent), glyph white (as on .play-btn).
"""

import os
import sys
from PIL import Image, ImageDraw

BODY = (221, 56, 115, 255)  # --accent
GLYPH = (255, 255, 255, 255)
BG_TILE = (247, 247, 248, 255)  # --bg light, for the full-color tile only
TILE_R = 44  # viewBox units, from the SVG rect rx

# The mark's bbox in viewBox units is (34, 21)-(166, 179): 132x158, centered
# on (100, 100) once the -3 nudge is folded in. SCALE_UP enlarges it about
# that center so the glyph fills 90% of the canvas height, matching the
# visual weight of neighboring launcher icons. Mirrors the scale() transform
# in trackfecta-logo.svg; keep the two in sync.
SCALE_UP = 180.0 / 158.0

# Geometry in viewBox units (see the source SVG).
CROWN = (88, 24, 24, 13, 4)  # x y w h r
STEM = (94, 35, 12, 14)  # x y w h
SIDE = (93, 36, 14, 12, 3, 46, 100, 116)  # x y w h r angle cx cy
CIRCLE = (100, 116, 66)  # cx cy r
CHEVRON = [(80, 98), (106, 116), (80, 134)]  # polyline
UNDER = [(114, 136), (136, 136)]  # segment
STROKE = 13  # glyph stroke width in viewBox units


def draw_polyline(d, pts, width, scale, off, color):
    """Stroke a polyline with round caps and joins, at the given transform."""
    ox, oy = off
    r = width * scale / 2.0
    wpx = int(round(width * scale))
    sp = [(x * scale + ox, y * scale + oy) for x, y in pts]
    for (x1, y1), (x2, y2) in zip(sp, sp[1:]):
        d.line([x1, y1, x2, y2], fill=color, width=wpx)
    for (x, y) in sp:
        d.ellipse([x - r, y - r, x + r, y + r], fill=color)


def rotated_rounded_rect(size, params, scale, off, color):
    """The angled side button, drawn on its own layer and rotated."""
    ox, oy = off
    x, y, w, h, r, angle, cx, cy = params
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    d.rounded_rectangle(
        [x * scale + ox, y * scale + oy, (x + w) * scale + ox, (y + h) * scale + oy],
        radius=r * scale,
        fill=color,
    )
    # PIL rotates counterclockwise; SVG rotate is clockwise. Uniform scale
    # commutes with rotation, so transforming the center is enough.
    return layer.rotate(
        -angle, center=(cx * scale + ox, cy * scale + oy), resample=Image.BICUBIC
    )


def logo_transform(size):
    """(scale, off_x, off_y) mirroring the SVG group transform exactly:
    translate(100 100) scale(SCALE_UP) translate(-100 -103) in viewBox units.
    The -103 folds in the original translate(0 -3) vertical nudge so the
    mark's optical center lands on the canvas center.
    """
    scale = size / 200.0 * SCALE_UP
    off_x = size / 2.0 - 100.0 * scale
    off_y = size / 2.0 - 103.0 * scale
    return scale, off_x, off_y


def draw_logo(size, tile):
    """Render the mark at `size` px. tile=True adds the rounded background."""
    base = size / 200.0
    scale, off_x, off_y = logo_transform(size)
    off = (off_x, off_y)
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if tile:
        d.rounded_rectangle([0, 0, size - 1, size - 1], radius=TILE_R * base, fill=BG_TILE)

    # Body: crown, stem, main dial.
    ox, oy = off
    x, y, w, h, r = CROWN
    d.rounded_rectangle(
        [x * scale + ox, y * scale + oy, (x + w) * scale + ox, (y + h) * scale + oy],
        radius=r * scale,
        fill=BODY,
    )
    x, y, w, h = STEM
    d.rectangle(
        [x * scale + ox, y * scale + oy, (x + w) * scale + ox, (y + h) * scale + oy],
        fill=BODY,
    )
    cx, cy, rr = CIRCLE
    d.ellipse(
        [
            (cx - rr) * scale + ox,
            (cy - rr) * scale + oy,
            (cx + rr) * scale + ox,
            (cy + rr) * scale + oy,
        ],
        fill=BODY,
    )

    # Angled side button.
    img.alpha_composite(rotated_rounded_rect((size, size), SIDE, scale, off, BODY))

    # Glyph on top.
    d = ImageDraw.Draw(img)
    draw_polyline(d, CHEVRON, STROKE, scale, off, GLYPH)
    draw_polyline(d, UNDER, STROKE, scale, off, GLYPH)
    return img


def glyph_alpha(size):
    """A grayscale layer with the glyph strokes at full opacity.

    Uses the same transform as draw_logo so the carve stays aligned.
    """
    scale, off_x, off_y = logo_transform(size)
    off = (off_x, off_y)
    layer = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(layer)
    draw_polyline(d, CHEVRON, STROKE, scale, off, 255)
    draw_polyline(d, UNDER, STROKE, scale, off, 255)
    return layer


def make_mask(size=48, pad=2):
    """Body-only coverage at `size`x`size`: dial minus the carved glyph.

    The result is cropped to the mark's bounding box and rescaled to fill
    the slot (plus `pad` px of breathing room on each side). The panel
    scales the whole image into its icon slot, so margins baked into the
    mask shrink the stopwatch on screen; cropping makes it render as large
    as the other panel icons.
    """
    big = size * 8  # supersample
    body = draw_logo(big, tile=False)
    alpha = body.getchannel("A")
    carve = glyph_alpha(big)
    knocked = Image.frombytes(
        "L", alpha.size, bytes(max(a - c, 0) for a, c in zip(alpha.tobytes(), carve.tobytes()))
    )
    bbox = knocked.point(lambda v: 255 if v > 8 else 0).getbbox()
    x0, y0, x1, y1 = bbox
    side = (x1 - x0) + (y1 - y0)
    side = max(x1 - x0, y1 - y0) + pad * 2 * 8
    cx, cy = (x0 + x1) / 2.0, (y0 + y1) / 2.0
    box = (cx - side / 2.0, cy - side / 2.0, cx + side / 2.0, cy + side / 2.0)
    return knocked.crop(box).resize((size, size), Image.LANCZOS)


# Sizes for the freedesktop hicolor icon ladder. The Linux bundles install
# one PNG per size so the launcher never upscales a smaller bitmap (which
# renders soft). 256x256@2 gets the 512px render. Keep in sync with the
# bundle.linux.*.files maps in tauri.conf.json.
HICOLOR_SIZES = [16, 22, 24, 32, 48, 64, 96, 128, 192, 256, 512]


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.join(here, "..")
    icon_path = os.path.join(root, "app-icon.png")
    bin_path = os.path.join(root, "src-tauri", "icons", "tray-mark.bin")
    # Full-color mark on a transparent background, matching the source SVG:
    # accent dial and crown with a white chevron and underscore.
    draw_logo(1024, tile=False).save(icon_path)
    print(f"wrote {icon_path}", file=sys.stderr)

    # Hicolor ladder: the mark on a transparent canvas, rendered natively at
    # each size (no upscale). Consumed by the deb/rpm/appimage files maps.
    hicolor_dir = os.path.join(root, "src-tauri", "icons", "hicolor")
    os.makedirs(hicolor_dir, exist_ok=True)
    for size in HICOLOR_SIZES:
        out = os.path.join(hicolor_dir, f"trackfecta-{size}.png")
        draw_logo(size, tile=False).save(out)
        print(f"wrote {out}", file=sys.stderr)

    data = make_mask().tobytes()
    with open(bin_path, "wb") as f:
        f.write(data)
    print(f"wrote {bin_path} ({len(data)} bytes)", file=sys.stderr)


if __name__ == "__main__":
    main()
