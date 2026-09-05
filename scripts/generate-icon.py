#!/usr/bin/env python3
"""Generate the app icon (crates/aliexpress-cli/icons/icon.png).

The icon is generated rather than drawn by hand so that the macOS geometry lives
in the source as named constants instead of being re-measured every time
somebody edits a PNG. Run this after changing anything below:

    python3 scripts/generate-icon.py

It only uses the standard library (zlib for the PNG stream). This repository has
no image toolchain, and a single asset does not justify adding one. The output
is deterministic: the same source produces byte-identical bytes.

macOS icon geometry
-------------------
The widely used macOS app icon template (Big Sur and later) draws on a 1024x1024
canvas but does not fill it. The icon body is an 824x824 rounded rectangle
centred on the canvas, leaving a 100px transparent margin on every side, and the
corner radius is 185.4 - 22.5% of the body.

The margin is what keeps icons looking the same size as each other: an icon that
fills its canvas edge to edge is drawn larger than every icon following the
template beside it, which is why a square, full-bleed icon looks out of place
next to them. It also leaves room for the shadow and for badges to sit over.

Apple's real shape is a continuous-curvature "squircle" rather than a circular
arc, but the circular approximation at this radius is what the template grid
uses and is indistinguishable at the sizes an icon is actually seen.

Everything below those three constants - the gradient, the highlight, the magnifying
glass - is this app's own identity and can be changed freely.
"""

import struct
import zlib
from pathlib import Path

CANVAS = 1024
# See the module docstring: the body is deliberately smaller than the canvas.
BODY = 824
MARGIN = (CANVAS - BODY) / 2  # 100.0
CORNER_RADIUS = BODY * 0.225  # 185.4

# A vertical orange gradient (Tailwind orange-400 to orange-600): AliExpress'
# colour, so the icon says what it searches. Flat fills read as cheap next to the
# shaded icons around them.
TOP_COLOR = (0xFB, 0x92, 0x3C)
BOTTOM_COLOR = (0xEA, 0x58, 0x0C)

# A faint highlight along the top edge, for material rather than for contrast.
# Turning it up makes the icon look plastic.
HIGHLIGHT_ALPHA = 0.16
HIGHLIGHT_SPAN = 0.45  # fraction of the body height over which it fades out

MARK_COLOR = (0xFF, 0xFF, 0xFF)
# The magnifying glass: a ring and a handle at 45 degrees, sized as fractions of
# the body so that the mark has room to breathe inside the rounded rectangle.
LENS_RADIUS = BODY * 0.19
STROKE = BODY * 0.06
HANDLE_LENGTH = BODY * 0.2
# The ring sits up and left of centre so that ring plus handle are centred.
LENS_CENTER = (CANVAS / 2 - BODY * 0.06, CANVAS / 2 - BODY * 0.06)

OUT_PATH = Path(__file__).resolve().parent.parent / "crates/aliexpress-cli/icons/icon.png"


def rounded_rect_distance(x: float, y: float) -> float:
    """Signed distance to the icon body, negative inside.

    Shrink the rectangle by the corner radius, measure the distance to that, and
    subtract the radius back off; the corners fall out of it for free.
    """
    half = BODY / 2 - CORNER_RADIUS
    dx = abs(x - CANVAS / 2) - half
    dy = abs(y - CANVAS / 2) - half
    outside = (max(dx, 0.0) ** 2 + max(dy, 0.0) ** 2) ** 0.5
    inside = min(max(dx, dy), 0.0)
    return outside + inside - CORNER_RADIUS


def polygon_distance(x: float, y: float, points) -> float:
    """Signed distance to a convex polygon, negative inside.

    The distance is taken to the nearest *segment*, not to the nearest edge line.
    Taking the largest of the per-edge half-plane distances would be simpler, but
    outside a vertex it measures along an edge's normal rather than to the vertex
    itself, which widens the antialiased edge unevenly around sharp corners.

    The sign comes from a separate inside test, which is why the polygon has to
    be convex: every point is inside exactly when it is behind all of the edges.
    """
    nearest = float("inf")
    inside = True
    for i in range(len(points)):
        ax, ay = points[i]
        bx, by = points[(i + 1) % len(points)]
        ex, ey = bx - ax, by - ay
        px, py = x - ax, y - ay
        length_squared = ex * ex + ey * ey
        t = min(max((px * ex + py * ey) / length_squared, 0.0), 1.0)
        nearest = min(nearest, ((px - ex * t) ** 2 + (py - ey * t) ** 2) ** 0.5)
        # Points are given clockwise, so the outward normal is (ey, -ex).
        if px * ey - py * ex > 0.0:
            inside = False
    return -nearest if inside else nearest


def coverage(distance: float) -> float:
    """Antialiased coverage from a signed distance, over a one-pixel transition."""
    return min(max(0.5 - distance, 0.0), 1.0)


def ring_distance(x: float, y: float) -> float:
    """Signed distance to the lens ring, negative inside the stroke."""
    cx, cy = LENS_CENTER
    radial = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
    return abs(radial - LENS_RADIUS) - STROKE / 2


def handle_points():
    """The handle as a rectangle running from the ring down-right at 45 degrees."""
    cx, cy = LENS_CENTER
    d = 2 ** -0.5
    start = LENS_RADIUS
    end = LENS_RADIUS + HANDLE_LENGTH
    half = STROKE * 0.6
    # Points clockwise: along the handle axis (d, d), across it (d, -d).
    return [
        (cx + start * d + half * d, cy + start * d - half * d),
        (cx + end * d + half * d, cy + end * d - half * d),
        (cx + end * d - half * d, cy + end * d + half * d),
        (cx + start * d - half * d, cy + start * d + half * d),
    ]


def build_pixels():
    handle = handle_points()
    rows = []
    for py in range(CANVAS):
        y = py + 0.5
        # The gradient and the highlight only vary down the icon, so they are
        # resolved once per row rather than once per pixel.
        t = min(max((y - MARGIN) / BODY, 0.0), 1.0)
        base = [TOP_COLOR[i] + (BOTTOM_COLOR[i] - TOP_COLOR[i]) * t for i in range(3)]
        highlight = max(0.0, 1.0 - t / HIGHLIGHT_SPAN) ** 2 * HIGHLIGHT_ALPHA
        body_color = [c + (255 - c) * highlight for c in base]

        row = bytearray()
        for px in range(CANVAS):
            x = px + 0.5
            body_alpha = coverage(rounded_rect_distance(x, y))
            if body_alpha <= 0.0:
                row += b"\x00\x00\x00\x00"
                continue
            mark_alpha = max(
                coverage(ring_distance(x, y)),
                coverage(polygon_distance(x, y, handle)),
            )
            color = [
                body_color[i] + (MARK_COLOR[i] - body_color[i]) * mark_alpha
                for i in range(3)
            ]
            row += bytes(
                (
                    int(round(color[0])),
                    int(round(color[1])),
                    int(round(color[2])),
                    int(round(body_alpha * 255)),
                )
            )
        rows.append(bytes(row))
    return rows


def write_png(path: Path, rows) -> None:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    # Filter 0 (None) throughout. With nothing in the image but a rounded
    # rectangle and a triangle, choosing filters per row buys almost nothing.
    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", CANVAS, CANVAS, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def main() -> None:
    write_png(OUT_PATH, build_pixels())
    print(f"wrote {OUT_PATH} ({OUT_PATH.stat().st_size} bytes, {CANVAS}x{CANVAS})")


if __name__ == "__main__":
    main()
