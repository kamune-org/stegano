#!/usr/bin/env python3
"""Write the Stegano logo SVGs. Run from anywhere; files land next to this script.

The mark is a 4x4 grid of image pixels in shades of indigo, with one pixel
changed (mint): what Stegano does to a carrier, one barely visible change.
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
INDIGO = "#5B5BD6"
MINT = "#6EE7B7"
BG_TOP, BG_BOTTOM = "#1B1C26", "#0E0F13"
# Opacity of each indigo pixel; None marks the changed (mint) pixel.
GRID = [
    [0.35, 0.60, 0.45, 0.80],
    [0.70, 0.40, 0.90, 0.50],
    [0.50, 0.85, None, 0.35],
    [0.90, 0.45, 0.65, 0.40],
]


def grid(size, cell, gap, radius, mono=False):
    start = (size - (4 * cell + 3 * gap)) / 2
    out = []
    for r, row in enumerate(GRID):
        for c, alpha in enumerate(row):
            x, y = start + c * (cell + gap), start + r * (cell + gap)
            if alpha is None:
                fill, op = ("#FFFFFF" if mono else MINT), 1.0
            else:
                fill, op = ("#FFFFFF" if mono else INDIGO), (alpha * 0.6 if mono else alpha)
            out.append(
                f'<rect x="{x:g}" y="{y:g}" width="{cell}" height="{cell}" rx="{radius}" '
                f'fill="{fill}" fill-opacity="{op:g}"/>'
            )
    return "\n  ".join(out)


def gradient(id_):
    return (
        f'<linearGradient id="{id_}" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="{BG_TOP}"/><stop offset="1" stop-color="{BG_BOTTOM}"/>'
        "</linearGradient>"
    )


def svg(body, defs=""):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">\n'
        f"  <defs>{defs}</defs>\n  {body}\n</svg>\n"
    )


def tile(inset=0):
    """Full app tile. inset > 0 leaves transparent margin (macOS icon grid)."""
    side = 1024 - 2 * inset
    scale = side / 1024
    body = (
        f'<g transform="translate({inset} {inset}) scale({scale:g})">\n'
        f'  <rect width="1024" height="1024" rx="228" fill="url(#bg)"/>\n'
        f"  {grid(1024, 152, 40, 34)}\n  </g>"
    )
    return svg(body, gradient("bg"))


files = {
    # Windows, Linux, favicon, in-app mark, README.
    "icon.svg": tile(),
    # macOS: Apple's grid puts an 824 px tile in a 1024 px canvas.
    "icon-macos.svg": tile(inset=100),
    # Android adaptive icon layers (108 dp canvas, 66 dp safe circle): the grid
    # is 440/1024 = 43% of the canvas so its diagonal stays inside the circle.
    "android-fg.svg": svg(grid(1024, 92, 24, 21)),
    "android-bg.svg": svg('<rect width="1024" height="1024" fill="url(#bg)"/>', gradient("bg")),
    "android-mono.svg": svg(grid(1024, 92, 24, 21, mono=True)),
}
for name, text in files.items():
    (HERE / name).write_text(text)
    print("wrote", name)
