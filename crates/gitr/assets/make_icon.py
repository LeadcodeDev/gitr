#!/usr/bin/env python3
"""Generates gitr's app icon: the ferrislabs crab on a black plate, as pixel art.

The crab is not a redrawing. It is the organisation's own mark, lifted pixel for
pixel from github.com/ferrislabs.png, which turns out to be a 9x9 grid in three
colours. That economy is the whole reason it reads: nine pixels across leaves room
for two raised claws, two eyes and a body, and nothing else survives at dock size
anyway. Every hand-drawn crab attempted before this one was larger, more detailed,
and read as a face.

One pixel differs from the original. The whites of the eyes are transparent there,
because the avatar sits on GitHub's white plate; here the plate is black, so they
are set to white explicitly or the eyes close up.

Nothing is rasterised. Pixel art is placed, not sampled — feeding curves to a
rasteriser at this size produces mush, which is how the previous vector icon could
not simply be converted.

Deliberately standard library only. The generator this replaced shelled out to
`rsvg-convert`, so regenerating the icon needed a Homebrew package; a PNG encoder for
flat colour is twenty lines, and no dependency beats a small one.

    python3 make_icon.py        # rewrites icon.png
"""

import pathlib
import struct
import zlib

HERE = pathlib.Path(__file__).parent

# macOS does not fill the icon canvas. Measured on Mail, Messages, Music and Slack,
# every one puts its artwork on 824 of 1024 pixels — 80.5% — and leaves the rest
# transparent; the dock lays out the canvas, so an icon that fills its own reads at the
# wrong size beside them. 22 grid units at 37 gives 814, inset by 105 on each side of a
# 1024 canvas: 79.5%, within a percent of Apple's grid and, unlike 824, a whole number
# of pixels per grid unit.
GRID = 22
SCALE = 37
CANVAS = 1024
PLATE_RADIUS = 5

# The crab keeps its 9x9 structure but occupies two grid units per pixel. Drawing the
# plate on the finer grid is what buys the corner radius: 5 of 22 is 22.7%, against
# Apple's 22.5%, where 11 units could only offer 18% or 27%.
CRAB_SCALE = 2

PALETTE = {
    ".": (0, 0, 0, 0),
    "N": (0, 0, 0, 255),
    "O": (242, 103, 15, 255),
    "o": (168, 62, 8, 255),
    "K": (28, 25, 23, 255),
    "W": (255, 255, 255, 255),
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

# 18 wide and 14 tall on a 22 grid, so both offsets land on whole units and the crab
# centres exactly. A pixel off-centre is visible at this scale and reads as a mistake.
CRAB_AT = ((GRID - 9 * CRAB_SCALE) // 2, (GRID - 7 * CRAB_SCALE) // 2)


def plate(px, radius=PLATE_RADIUS):
    """The rounded square macOS expects, corners stepped rather than antialiased —
    a smooth corner beside hard-edged art reads as an accident."""
    for y in range(GRID):
        for x in range(GRID):
            dx = max(radius - 1 - x, x - (GRID - radius), 0)
            dy = max(radius - 1 - y, y - (GRID - radius), 0)
            if dx * dx + dy * dy <= (radius - 1) * (radius - 1) + 1:
                px[y][x] = "N"


def crab(px):
    ox, oy = CRAB_AT
    for j, row in enumerate(CRAB):
        for i, ch in enumerate(row):
            if ch == ".":
                continue
            for dy in range(CRAB_SCALE):
                for dx in range(CRAB_SCALE):
                    px[oy + j * CRAB_SCALE + dy][ox + i * CRAB_SCALE + dx] = ch


def write_png(path, rows):
    """Encodes RGBA without Pillow. Filter byte zero on every scanline: the image is a
    handful of flat colours, so a filter would cost cycles and save nothing."""
    raw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in rows)

    def chunk(tag, payload):
        body = tag + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))

    side = len(rows)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", side, side, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    px = [["." for _ in range(GRID)] for _ in range(GRID)]
    plate(px)
    crab(px)

    art = GRID * SCALE
    inset = (CANVAS - art) // 2
    blank = PALETTE["."]
    rows = [
        [
            PALETTE[px[(y - inset) // SCALE][(x - inset) // SCALE]]
            if inset <= x < inset + art and inset <= y < inset + art
            else blank
            for x in range(CANVAS)
        ]
        for y in range(CANVAS)
    ]
    out = HERE / "icon.png"
    write_png(out, rows)
    print(
        f"wrote {out.name}: {CANVAS}x{CANVAS} canvas, {art}px artwork "
        f"({art / CANVAS:.1%}), inset {inset}, {out.stat().st_size} bytes"
    )


if __name__ == "__main__":
    main()
