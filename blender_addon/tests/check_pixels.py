# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Task T1: colours reach Blender's picture unchanged —
# compare the picture Blender saved with F12 against the renderer's own levels (written to
# exact_png, which bypasses Blender's colour management). Usage: python3 check_pixels.py RENDER.png EXACT.png
"""Every byte of Blender's picture against the renderer's, and that the picture is not blank."""

import sys

from png_read import read_png


def main():
    render = read_png(sys.argv[1])
    exact = read_png(sys.argv[2])
    if render[0:2] != exact[0:2]:
        raise SystemExit("sizes differ: %dx%d against %dx%d" % (render[0], render[1], exact[0], exact[1]))
    width, height, _, _ = render
    bad = 0
    first = None
    for y in range(height):
        for x in range(width):
            i = (y * width + x) * render[2]
            j = (y * width + x) * 3
            if render[3][i:i + 3] != exact[3][j:j + 3]:
                bad += 1
                if first is None:
                    first = (x, y, tuple(render[3][i:i + 3]), tuple(exact[3][j:j + 3]))
    lit = any(b != 0 for b in exact[3])
    print("picture %dx%d; mismatching pixels: %d; any lit pixel: %s" % (width, height, bad, lit))
    if first:
        print("first mismatch at (%d,%d): render=%s exact=%s" % first)
    if bad == 0 and lit:
        print("PIXELS-OK")
    else:
        raise SystemExit("PIXELS-BAD")


main()
