#!/usr/bin/env python3
"""Generate the DevClean menu bar icon — the one source of truth for it.

Why this exists: the tray icon shipped once as a PNG written in greyscale with
**no alpha channel**. macOS draws a template image from its alpha, so the whole
36×36 frame rendered as a solid white square in the menu bar (G-UI-04). The PNG
also drifted from `assets/devclean-tray.svg`, because the SVG was never the input.

So the mark is described **once**, here, as geometry. This script rasterises it
to a greyscale+alpha template PNG *and* emits the matching SVG from the same
control points. They cannot disagree.

The mark is the DevClean leaf outline (see `assets/devclean-leaf.svg`): three
open strokes — a left leaf edge, a right leaf edge and the stem. Because the
mark is an outline rather than a filled silhouette, the rasteriser tests the
distance from each supersample to the stroke centreline instead of testing
point-in-polygon.

    python3 tools/make_tray_icon.py

Outputs:
    apps/desktop/src-tauri/icons/tray.png   36×36, colour type 4, black + alpha
    assets/devclean-tray.png                the same bitmap, kept beside the source
    assets/devclean-tray.svg                the same mark, as vectors
"""

from __future__ import annotations

import math
import os
import struct
import zlib

SIZE = 36        # 18pt @2x — what the macOS menu bar samples
FILL = 0.90      # glyph size relative to the frame
SS = 4           # supersampling factor for smooth edges
STROKE = 18.0    # outline width in canvas units (the 512-viewBox leaf)


# --- the mark ----------------------------------------------------------------
# The DevClean leaf outline, in 512-viewBox canvas proportions. `fit`
# re-centres and scales the result, so these numbers describe the shape rather
# than the final pixels. Straight line segments are given as `line`; curved
# strokes as chained cubics (`segs`). The stroke is centred on the path.

MARK = [
    {
        "kind": "curve",
        "segs": [
            ((256.0, 410.0), (224.0, 398.0), (184.0, 372.0), (155.0, 334.0)),
            ((155.0, 334.0), (121.0, 290.0), (108.0, 235.0), (108.0, 174.0)),
            ((108.0, 174.0), (162.0, 190.0), (207.0, 221.0), (234.0, 266.0)),
            ((234.0, 266.0), (250.0, 292.0), (256.0, 326.0), (256.0, 359.0)),
        ],
    },
    {
        "kind": "curve",
        "segs": [
            ((256.0, 410.0), (288.0, 395.0), (330.0, 367.0), (360.0, 327.0)),
            ((360.0, 327.0), (392.0, 284.0), (407.0, 230.0), (411.0, 194.0)),
            ((411.0, 194.0), (360.0, 208.0), (316.0, 237.0), (285.0, 278.0)),
            ((285.0, 278.0), (265.0, 305.0), (256.0, 337.0), (256.0, 370.0)),
        ],
    },
    {"kind": "line", "a": (256.0, 282.0), "b": (256.0, 410.0)},
]


# --- geometry helpers --------------------------------------------------------


def cubic(p0, p1, p2, p3, t):
    m = 1 - t
    return (
        m**3 * p0[0] + 3 * m * m * t * p1[0] + 3 * m * t * t * p2[0] + t**3 * p3[0],
        m**3 * p0[1] + 3 * m * m * t * p1[1] + 3 * m * t * t * p2[1] + t**3 * p3[1],
    )


def polyline(shape, steps=32):
    """The stroke centreline as a polyline, for bounds and hit testing."""
    if shape["kind"] == "line":
        return [shape["a"], shape["b"]]
    pts = []
    for p0, p1, p2, p3 in shape["segs"]:
        pts += [cubic(p0, p1, p2, p3, i / steps) for i in range(steps)]
    pts.append(shape["segs"][-1][3])
    return pts


def dist_to_segment(px, py, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    if dx == 0 and dy == 0:
        return math.hypot(px - a[0], py - a[1])
    t = max(0.0, min(1.0, ((px - a[0]) * dx + (py - a[1]) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (a[0] + t * dx), py - (a[1] + t * dy))


def dist_to_polyline(px, py, poly):
    return min(dist_to_segment(px, py, poly[i], poly[i + 1]) for i in range(len(poly) - 1))


def inside(shape, x, y, r):
    return dist_to_polyline(x, y, shape["poly"]) <= r


def bounds(shapes):
    """Stroke-expanded bounds: centreline extremes grown by half the stroke."""
    half = STROKE / 2
    minx, miny, maxx, maxy = 1e9, 1e9, -1e9, -1e9
    for s in shapes:
        for x, y in s["poly"]:
            minx, maxx = min(minx, x), max(maxx, x)
            miny, maxy = min(miny, y), max(maxy, y)
    return minx - half, miny - half, maxx + half, maxy + half


def fit(shapes):
    """Uniform scale + translation so the stroked mark is centred and fills FILL."""
    minx, miny, maxx, maxy = bounds(shapes)
    w, h = maxx - minx, maxy - miny
    scale = (SIZE * FILL) / max(w, h)
    ox = (SIZE - w * scale) / 2 - minx * scale
    oy = (SIZE - h * scale) / 2 - miny * scale
    tx = lambda p: (p[0] * scale + ox, p[1] * scale + oy)  # noqa: E731

    out = []
    for s in shapes:
        f = dict(s)
        if f["kind"] == "line":
            f["a"], f["b"] = tx(f["a"]), tx(f["b"])
        else:
            f["segs"] = [tuple(tx(p) for p in seg) for seg in s["segs"]]
        f["poly"] = [tx(p) for p in s["poly"]]
        out.append(f)
    return out, STROKE * scale / 2


def rasterise(shapes, r):
    """Supersample SS×SS per pixel, then box-filter in both axes."""
    hi = []
    for py in range(SIZE * SS):
        y = (py + 0.5) / SS
        row = bytearray(SIZE * SS)
        for px in range(SIZE * SS):
            x = (px + 0.5) / SS
            row[px] = 255 if any(inside(s, x, y, r) for s in shapes) else 0
        hi.append(row)

    rows = []
    for py in range(SIZE):
        row = []
        for px in range(SIZE):
            total = sum(hi[py * SS + ky][px * SS + kx] for ky in range(SS) for kx in range(SS))
            row.append(total // (SS * SS))
        rows.append(row)
    return rows


# --- output ------------------------------------------------------------------


def write_png(path, rows):
    """8-bit greyscale + alpha (colour type 4): black pixels with an alpha
    mask — exactly what macOS expects from a template image."""
    raw = bytearray()
    for r in rows:
        raw.append(0)  # filter: none
        for a in r:
            raw += bytes((0, a))  # grey 0 (macOS recolours), alpha = the mark

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 4, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def write_svg(path, shapes, r):
    width = 2 * r
    parts = []
    for s in shapes:
        if s["kind"] == "line":
            a, b = s["a"], s["b"]
            d = f"M{a[0]:.2f} {a[1]:.2f}L{b[0]:.2f} {b[1]:.2f}"
        else:
            p = s["segs"]
            d = f"M{p[0][0][0]:.2f} {p[0][0][1]:.2f}"
            for _, c1, c2, end in p:
                d += (
                    f"C{c1[0]:.2f} {c1[1]:.2f} {c2[0]:.2f} {c2[1]:.2f}"
                    f" {end[0]:.2f} {end[1]:.2f}"
                )
        parts.append(f'    <path d="{d}"/>')
    with open(path, "w") as f:
        f.write(
            '<svg xmlns="http://www.w3.org/2000/svg" '
            f'viewBox="0 0 {SIZE} {SIZE}" width="{SIZE}" height="{SIZE}">\n'
            "  <!-- macOS menu-bar template image: BLACK + alpha only, so the\n"
            "       system recolours it for light/dark menu bars. GENERATED by\n"
            "       tools/make_tray_icon.py from the same geometry as\n"
            "       apps/desktop/src-tauri/icons/tray.png — edit the script,\n"
            "       not this file. -->\n"
            f'  <g fill="none" stroke="#000" stroke-width="{width:.2f}"'
            ' stroke-linecap="round" stroke-linejoin="round">\n'
            + "\n".join(parts)
            + "\n  </g>\n</svg>\n"
        )


def report(rows):
    pts = [(x, y) for y, r in enumerate(rows) for x, a in enumerate(r) if a > 16]
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    print(
        "tray.png %dx%d · glyph %dx%d (%.0f%% x %.0f%%) · centre (%.1f, %.1f)px · opaque %.0f%%"
        % (
            SIZE, SIZE,
            max(xs) - min(xs) + 1, max(ys) - min(ys) + 1,
            (max(xs) - min(xs) + 1) / SIZE * 100, (max(ys) - min(ys) + 1) / SIZE * 100,
            (min(xs) + max(xs)) / 2 - (SIZE - 1) / 2, (min(ys) + max(ys)) / 2 - (SIZE - 1) / 2,
            len(pts) / (SIZE * SIZE) * 100,
        )
    )
    for r in rows:
        print("    " + "".join("#" if a > 128 else ("+" if a > 16 else ".") for a in r))


def main():
    shapes = []
    for s in MARK:
        s = dict(s)
        s["poly"] = polyline(s)
        shapes.append(s)
    fitted, r = fit(shapes)
    rows = rasterise(fitted, r)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    write_png(os.path.join(root, "apps/desktop/src-tauri/icons/tray.png"), rows)
    write_png(os.path.join(root, "assets/devclean-tray.png"), rows)
    write_svg(os.path.join(root, "assets/devclean-tray.svg"), fitted, r)
    report(rows)


if __name__ == "__main__":
    main()
