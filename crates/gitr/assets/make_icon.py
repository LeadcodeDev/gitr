#!/usr/bin/env python3
"""Generates gitr's app icon: the ferrislabs crab on a black plate.

The crab is not a redrawing. It is lifted pixel for pixel from
github.com/ferrislabs.png, which turns out to be a 9x9 grid in three colours. That
economy is why it reads at dock size: nine pixels across leaves room for two raised
claws, two eyes and a body, and nothing else survives anyway. Larger hand-drawn crabs
were tried first and each one read as a face, the claws flattening into the body
outline every time.

One pixel departs from the original. The whites of the eyes are transparent there,
because the avatar sits on GitHub's white plate; here the plate is black, so they are
set explicitly or the eyes close up.

The plate and the crab are drawn by different rules on purpose. The crab is hard-edged,
every block an exact multiple of an output pixel. The plate is a smooth superellipse
with antialiased edges, because macOS draws every icon beside it that way and a stepped
corner among them reads as a rendering fault rather than as a style. An earlier version
quantised the plate to the crab's grid and looked broken in the dock.

Sizing follows Apple's grid rather than the canvas. Measured on Mail, Messages, Music
and Slack, each puts its artwork on 824 of 1024 pixels and leaves the rest transparent;
the dock lays out the canvas, so an icon painted to its own edges sits on a different
grid from its neighbours and looks wrong whichever way it lands.

Deliberately standard library only. The generator this replaced shelled out to
`rsvg-convert`, so regenerating the icon needed a Homebrew package that nothing else in
the tree requires.

    python3 make_icon.py        # rewrites icon.png
"""

import pathlib
import struct
import zlib

HERE = pathlib.Path(__file__).parent

CANVAS = 1024
ART = 824
INSET = (CANVAS - ART) // 2

# Apple's icon silhouette is a superellipse, not a rounded rectangle: the curvature runs
# continuously into the straight edge instead of meeting it at a tangent. Five is the
# exponent that matches macOS closely enough that the difference is invisible at any
# size the dock draws.
EXPONENT = 5.0
SUBSAMPLES = 8

# 72 output pixels per crab pixel: 9 across is 648, which is 78.6% of the artwork
# against 81.8% before. Both this and the 7-row height leave a whole number of pixels on
# each side, so the crab centres exactly — a pixel off-centre is visible at this scale.
BLOCK = 72

PLATE = (0, 0, 0)
COLOURS = {
    "O": (242, 103, 15),
    "o": (168, 62, 8),
    "K": (28, 25, 23),
    "W": (255, 255, 255),
}

CRAB = [
    "..o...o..",
    ".oo...oo.",
    ".oWKWKWo.",
    ".oOOOOOo.",
    "OOOOOOOOO",
    ".OOOOOOO.",
    "o.OOOOO.o",
]


def plate_alpha():
    """Coverage per pixel for the superellipse, as a flat row-major list.

    Sampled by sub-row rather than by sub-pixel: the shape's half-width is analytic for
    any y, so each sub-row costs one power and a span of overlaps instead of sixty-four
    inside-tests per pixel. That is the difference between a second and a minute.
    """
    alpha = [0.0] * (CANVAS * CANVAS)
    half = ART / 2
    centre = CANVAS / 2
    for y in range(CANVAS):
        row = y * CANVAS
        for s in range(SUBSAMPLES):
            dy = (y + (s + 0.5) / SUBSAMPLES) - centre
            ratio = abs(dy) / half
            if ratio >= 1.0:
                continue
            span = half * (1.0 - ratio**EXPONENT) ** (1.0 / EXPONENT)
            left, right = centre - span, centre + span
            for x in range(max(0, int(left)), min(CANVAS, int(right) + 1)):
                covered = min(x + 1.0, right) - max(float(x), left)
                if covered > 0:
                    alpha[row + x] += covered / SUBSAMPLES
    return alpha


def crab_pixels():
    """Maps every output pixel the crab covers to its colour, keyed by (x, y)."""
    width, height = 9 * BLOCK, 7 * BLOCK
    ox = INSET + (ART - width) // 2
    oy = INSET + (ART - height) // 2
    painted = {}
    for j, line in enumerate(CRAB):
        for i, ch in enumerate(line):
            if ch == ".":
                continue
            colour = COLOURS[ch]
            for y in range(oy + j * BLOCK, oy + (j + 1) * BLOCK):
                for x in range(ox + i * BLOCK, ox + (i + 1) * BLOCK):
                    painted[(x, y)] = colour
    return painted


def write_png(path, rows):
    """Encodes RGBA without Pillow. Filter byte zero on every scanline: the image is
    flat colour over a single curved edge, so a filter would cost cycles and save
    nothing."""
    raw = b"".join(b"\x00" + bytes(row) for row in rows)

    def chunk(tag, payload):
        body = tag + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))

    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", CANVAS, CANVAS, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    alpha = plate_alpha()
    crab = crab_pixels()

    rows = []
    for y in range(CANVAS):
        row = bytearray()
        base = y * CANVAS
        for x in range(CANVAS):
            colour = crab.get((x, y))
            if colour is not None:
                row += bytes(colour) + b"\xff"
            else:
                a = alpha[base + x]
                row += bytes(PLATE) + bytes((round(min(a, 1.0) * 255),))
        rows.append(row)

    out = HERE / "icon.png"
    write_png(out, rows)
    print(
        f"wrote {out.name}: {CANVAS}x{CANVAS} canvas, {ART}px artwork "
        f"({ART / CANVAS:.1%}), crab {9 * BLOCK}px ({9 * BLOCK / ART:.1%} of artwork), "
        f"{out.stat().st_size} bytes"
    )


if __name__ == "__main__":
    main()
