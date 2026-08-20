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

GRID = 11
SCALE = 96

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

# 9 wide and 7 tall on an 11 grid: the only offsets that centre it exactly on both
# axes. A pixel off-centre is visible at this scale and reads as a mistake.
CRAB_AT = ((GRID - 9) // 2, (GRID - 7) // 2)


def plate(px, radius=3):
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
            if ch != ".":
                px[oy + j][ox + i] = ch


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

    side = GRID * SCALE
    rows = [
        [PALETTE[px[y // SCALE][x // SCALE]] for x in range(side)]
        for y in range(side)
    ]
    out = HERE / "icon.png"
    write_png(out, rows)
    print(f"wrote {out.name}: {side}x{side}, {out.stat().st_size} bytes")


if __name__ == "__main__":
    main()
