#!/usr/bin/env python3
"""Generate the DevClean menu bar icon — the one source of truth for it.

Why this exists: the tray icon shipped once as a PNG written in greyscale with
**no alpha channel**. macOS draws a template image from its alpha, so the whole
36×36 frame rendered as a solid white square in the menu bar (G-UI-04). The PNG
also drifted from `assets/devclean-tray.svg`, because the SVG was never the input.

So the mark is described **once**, here, as geometry. This script rasterises it
to a greyscale+alpha template PNG *and* emits the matching SVG from the same
control points. They cannot disagree.

    python3 tools/make_tray_icon.py

Outputs:
    apps/desktop/src-tauri/icons/tray.png   36×36, colour type 4, black + alpha
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


# --- the mark ----------------------------------------------------------------
# A sprout, simplified and thickened for 18pt: a central stem and two leaves.
# Written in canvas proportions; `fit` re-centres and scales the result, so
# these numbers describe the shape rather than the final pixels.


def leaf(base, tip, half_width, t1=0.25, t2=0.75):
    """A leaf as two chained cubic Béziers: base → tip along one edge, then
    tip → base along the other. `half_width` is the half-thickness at the
    middle of the spine."""
    (bx, by), (tx, ty) = base, tip
    dx, dy = tx - bx, ty - by
    length = math.hypot(dx, dy)
    px, py = -dy / length, dx / length  # unit perpendicular to the spine

    def ctrl(t, side):
        return (bx + dx * t + px * half_width * side, by + dy * t + py * half_width * side)

    return {
        "kind": "leaf",
        "points": [base, ctrl(t1, 1), ctrl(t2, 1), tip, ctrl(t2, -1), ctrl(t1, -1), base],
    }


# Thin leaves, not fat ones: at 18pt the menu bar has room for a silhouette,
# not for detail. Fat leaves merge with the stem into one unreadable mass — this
# set was picked by rendering candidates as ASCII and looking at them.
MARK = [
    leaf((19.0, 18.0), (30.0, 9.5), 1.8),
    leaf((17.0, 22.5), (6.0, 13.0), 1.8),
    {"kind": "stem", "a": (18.0, 16.0), "b": (18.0, 31.0), "r": 1.55},
]


# --- geometry helpers --------------------------------------------------------


def cubic(p0, p1, p2, p3, t):
    m = 1 - t
    return (
        m**3 * p0[0] + 3 * m * m * t * p1[0] + 3 * m * t * t * p2[0] + t**3 * p3[0],
        m**3 * p0[1] + 3 * m * m * t * p1[1] + 3 * m * t * t * p2[1] + t**3 * p3[1],
    )


def flatten(shape, steps=40):
    """Outline of a shape as a polygon, for hit testing and bounds."""
    if shape["kind"] == "stem":
        return None
    pts = shape["points"]
    poly = [pts[0]]
    poly += [cubic(*pts[0:4], i / steps) for i in range(1, steps + 1)]
    poly += [cubic(*pts[3:7], i / steps) for i in range(1, steps + 1)]
    return poly


def point_in_poly(x, y, poly):
    inside = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def dist_to_segment(px, py, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    if dx == 0 and dy == 0:
        return math.hypot(px - a[0], py - a[1])
    t = max(0.0, min(1.0, ((px - a[0]) * dx + (py - a[1]) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (a[0] + t * dx), py - (a[1] + t * dy))


def inside(shape, x, y):
    if shape["kind"] == "stem":
        return dist_to_segment(x, y, shape["a"], shape["b"]) <= shape["r"]
    return point_in_poly(x, y, shape["poly"])


def bounds(shapes):
    minx, miny, maxx, maxy = 1e9, 1e9, -1e9, -1e9
    for s in shapes:
        if s["kind"] == "stem":
            for p in (s["a"], s["b"]):
                minx, maxx = min(minx, p[0] - s["r"]), max(maxx, p[0] + s["r"])
                miny, maxy = min(miny, p[1] - s["r"]), max(maxy, p[1] + s["r"])
        else:
            for x, y in s["poly"]:
                minx, maxx = min(minx, x), max(maxx, x)
                miny, maxy = min(miny, y), max(maxy, y)
    return minx, miny, maxx, maxy


def fit(shapes):
    """Uniform scale + translation so the mark is centred and fills FILL."""
    minx, miny, maxx, maxy = bounds(shapes)
    w, h = maxx - minx, maxy - miny
    scale = (SIZE * FILL) / max(w, h)
    ox = (SIZE - w * scale) / 2 - minx * scale
    oy = (SIZE - h * scale) / 2 - miny * scale
    tx = lambda p: (p[0] * scale + ox, p[1] * scale + oy)  # noqa: E731

    out = []
    for s in shapes:
        f = dict(s)
        if f["kind"] == "stem":
            f["a"], f["b"], f["r"] = tx(f["a"]), tx(f["b"]), f["r"] * scale
        else:
            f["points"] = [tx(p) for p in f["points"]]
            f["poly"] = [tx(p) for p in s["poly"]]
        out.append(f)
    return out


def rasterise(shapes):
    """Supersample SS×SS per pixel, then box-filter in both axes."""
    hi = []
    for py in range(SIZE * SS):
        y = (py + 0.5) / SS
        row = bytearray(SIZE * SS)
        for px in range(SIZE * SS):
            x = (px + 0.5) / SS
            row[px] = 255 if any(inside(s, x, y) for s in shapes) else 0
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


def write_svg(path, shapes):
    parts = []
    for s in shapes:
        if s["kind"] == "stem":
            a, b, r = s["a"], s["b"], s["r"]
            parts.append(
                f'    <line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}"'
                f' stroke="#000" stroke-width="{2 * r:.2f}" stroke-linecap="round"/>'
            )
        else:
            p = s["points"]
            parts.append(
                f'    <path d="M{p[0][0]:.2f} {p[0][1]:.2f}'
                f"C{p[1][0]:.2f} {p[1][1]:.2f} {p[2][0]:.2f} {p[2][1]:.2f} {p[3][0]:.2f} {p[3][1]:.2f}"
                f"C{p[4][0]:.2f} {p[4][1]:.2f} {p[5][0]:.2f} {p[5][1]:.2f} {p[0][0]:.2f} {p[0][1]:.2f}Z\"/>"
            )
    with open(path, "w") as f:
        f.write(
            '<svg xmlns="http://www.w3.org/2000/svg" '
            f'viewBox="0 0 {SIZE} {SIZE}" width="{SIZE}" height="{SIZE}">\n'
            "  <!-- macOS menu-bar template image: BLACK + alpha only, so the\n"
            "       system recolours it for light/dark menu bars. GENERATED by\n"
            "       tools/make_tray_icon.py from the same geometry as\n"
            "       apps/desktop/src-tauri/icons/tray.png — edit the script,\n"
            "       not this file. -->\n"
            '  <g fill="#000">\n' + "\n".join(parts) + "\n  </g>\n</svg>\n"
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
        s["poly"] = flatten(s)
        shapes.append(s)
    fitted = fit(shapes)
    rows = rasterise(fitted)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    write_png(os.path.join(root, "apps/desktop/src-tauri/icons/tray.png"), rows)
    write_svg(os.path.join(root, "assets/devclean-tray.svg"), fitted)
    report(rows)


if __name__ == "__main__":
    main()