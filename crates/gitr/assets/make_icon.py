#!/usr/bin/env python3
"""Generates gitr's app icon: two branches leaving a lane and merging back.

The mark is the product's own subject. A white lane runs the height of the plate; two
branches diverge from one commit and reconverge on another, each keeping its own colour
end to end — the rule `graph::layout` enforces. Their commits sit at different heights,
because in a real history they do, and because four nodes on the cardinal points of a
circle read as a life buoy rather than as a graph.
"""

import math
import pathlib
import subprocess

S = 1024
HERE = pathlib.Path(__file__).parent


def squircle(size, n=5.0, steps=720):
    """Apple-style superellipse, |x|^n + |y|^n = 1, rather than a rounded rect."""
    c = size / 2
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        x = c + c * math.copysign(abs(ct) ** (2 / n), ct)
        y = c + c * math.copysign(abs(st) ** (2 / n), st)
        pts.append(f"{x:.2f},{y:.2f}")
    return "M" + " L".join(pts) + " Z"


def cubic(p0, p1, p2, p3, t):
    """Point on a cubic bezier, so a node lands exactly on the branch it belongs to."""
    u = 1 - t
    return tuple(
        u**3 * a + 3 * u**2 * t * b + 3 * u * t**2 * c + t**3 * d
        for a, b, c, d in zip(p0, p1, p2, p3)
    )


TRUNK_X = 512
TOP, BOT = 112, 912
FORK_Y, MERGE_Y = 288, 736
REACH = 268
STROKE = 92
NODE = 68
RING = 16
PLATE = "#121215"

fork, merge = (TRUNK_X, FORK_Y), (TRUNK_X, MERGE_Y)
left = ((TRUNK_X - REACH, FORK_Y - 4), (TRUNK_X - REACH, MERGE_Y + 4))
right = ((TRUNK_X + REACH, FORK_Y - 4), (TRUNK_X + REACH, MERGE_Y + 4))
blue_node = cubic(fork, left[0], left[1], merge, 0.34)
green_node = cubic(fork, right[0], right[1], merge, 0.68)

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{S}" height="{S}" viewBox="0 0 {S} {S}">
  <defs>
    <linearGradient id="plate" x1="0" y1="0" x2="0.35" y2="1">
      <stop offset="0%" stop-color="#26262b"/>
      <stop offset="55%" stop-color="#141417"/>
      <stop offset="100%" stop-color="#0a0a0c"/>
    </linearGradient>
    <linearGradient id="lane" gradientUnits="userSpaceOnUse" x1="380" y1="128" x2="700" y2="896">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="55%" stop-color="#e9e9ee"/>
      <stop offset="100%" stop-color="#9c9ca6"/>
    </linearGradient>
    <linearGradient id="blue" gradientUnits="userSpaceOnUse" x1="512" y1="280" x2="248" y2="740">
      <stop offset="0%" stop-color="#8fb0ff"/>
      <stop offset="100%" stop-color="#3f6ed6"/>
    </linearGradient>
    <linearGradient id="green" gradientUnits="userSpaceOnUse" x1="512" y1="280" x2="790" y2="740">
      <stop offset="0%" stop-color="#7ee08a"/>
      <stop offset="100%" stop-color="#2ea043"/>
    </linearGradient>
    <filter id="grain" x="0" y="0" width="100%" height="100%">
      <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="3" seed="7" result="n"/>
      <feColorMatrix in="n" type="saturate" values="0" result="g"/>
      <feComponentTransfer in="g" result="a">
        <feFuncA type="linear" slope="0.15"/>
      </feComponentTransfer>
      <feComposite in="a" in2="SourceGraphic" operator="in"/>
    </filter>
  </defs>

  <path d="{squircle(S)}" fill="url(#plate)"/>
  <path d="{squircle(S)}" fill="#ffffff" filter="url(#grain)" opacity="0.55"/>

  <g fill="none" stroke-linecap="round" stroke-width="{STROKE}">
    <path d="M{TRUNK_X} {TOP} L{TRUNK_X} {BOT}" stroke="url(#lane)"/>
    <path d="M{fork[0]} {fork[1]} C{left[0][0]} {left[0][1]} {left[1][0]} {left[1][1]} {merge[0]} {merge[1]}"
          stroke="url(#blue)"/>
    <path d="M{fork[0]} {fork[1]} C{right[0][0]} {right[0][1]} {right[1][0]} {right[1][1]} {merge[0]} {merge[1]}"
          stroke="url(#green)"/>
  </g>

  <g stroke="{PLATE}" stroke-width="{RING}">
    <circle cx="{blue_node[0]:.1f}" cy="{blue_node[1]:.1f}" r="{NODE}" fill="url(#blue)"/>
    <circle cx="{green_node[0]:.1f}" cy="{green_node[1]:.1f}" r="{NODE}" fill="url(#green)"/>
    <circle cx="{TRUNK_X}" cy="{FORK_Y}" r="{NODE}" fill="url(#lane)"/>
    <circle cx="{TRUNK_X}" cy="{MERGE_Y}" r="{NODE}" fill="url(#lane)"/>
  </g>
</svg>
"""

(HERE / "icon.svg").write_text(svg)
for px in (1024, 512, 256, 128, 64, 32):
    subprocess.run(
        ["rsvg-convert", "-w", str(px), "-h", str(px), "-o", f"{HERE}/icon-{px}.png", f"{HERE}/icon.svg"],
        check=True,
    )
print("rendu:", ", ".join(f"icon-{p}.png" for p in (1024, 512, 256, 128, 64, 32)))
