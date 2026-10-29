# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). A terrain whose picture is not a grid size on a side: F12 resamples it to the next grid size with a
# notice, and it renders with the outline of a terrain whose picture of the same function is that size. Run:
#   blender -b --factory-startup --python blender_addon/tests/blender_terrain_sizes.py -- <substrate> <renderer> <prefix>
# Prints TERRAIN-SIZES-OK and exits non-zero on the first failed check.
"""192 points a side against 256 (and 128, to show the comparison sees a coarser grid): the pixels the terrain covers."""

import importlib
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402
import png_read  # noqa: E402


def check(condition, what):
    if not condition:
        print("TERRAIN-SIZES-FAIL", what)
        sys.exit(1)


def relief(u, v):
    """A mountain off the centre with a ripple: an outline against the sky that a coarser grid changes."""
    return 0.05 + 0.9 * math.exp(-((u - 0.4) ** 2 + (v - 0.55) ** 2) / 0.04) * (0.8 + 0.2 * math.sin(9 * u))


def main():
    bpy, _substrate, _program, prefix = blender_common.start()
    from mathutils import Matrix
    create, gather, solids, terrain_tool = (importlib.import_module(blender_common.ADDON + "." + m)
                                            for m in ("create", "gather", "solids", "terrain_tool"))
    scene = blender_common.setup(bpy, prefix)
    scene.render.resolution_x, scene.render.resolution_y = 160, 120
    sky = scene.world.bryce
    sky.sun_intensity = 1.0
    sky.sun_color = (1.0, 1.0, 1.0)
    sky.ambient_color = (0.2, 0.2, 0.2)
    bpy.data.objects.remove(bpy.data.objects["Cube"])
    made = create.new_terrain("terrain", (0.0, 0.0, 0.0))

    def render(tag):
        scene.render.filepath = "%s-%s.png" % (prefix, tag)
        bpy.ops.render.render(write_still=True)
        return png_read.read_png(scene.render.filepath)

    width, height, channels, empty = render("empty")
    covered = {}
    for n in (192, 256, 128):
        heights = [solids.height_of(relief(i / (n - 1), j / (n - 1))) for j in range(n) for i in range(n)]
        obj = terrain_tool.make_terrain_object("Shore", n, heights, made["terrain"], scene.collection)
        obj.matrix_world = Matrix(create.matrix_of(made["location"], made["scale"]))
        notices = []
        description, problems = gather.describe(bpy.context.evaluated_depsgraph_get(), notices)
        check(problems == [], "%d: refused: %r" % (n, problems))
        entry = next(o for o in description["objects"] if o["name"] == obj.name)
        size = solids.auto_size(n)
        check(entry["terrain"]["size"] == size, "%d: grid %d, not %d" % (n, entry["terrain"]["size"], size))
        told = [line for line in notices if "terrain" in line]
        if n == size:
            check(told == [], "%d: notices %r" % (n, told))
        else:
            check(told == ["object 'Shore': the terrain's picture is %d points a side, not a grid size: resampled to "
                           "%d (bilinear, same extent)" % (n, size)], "%d: notices %r" % (n, told))
        pixels = render("t%d" % n)[3]
        covered[n] = [any(pixels[k * channels + c] != empty[k * channels + c] for c in range(3))
                      for k in range(width * height)]
        bpy.data.objects.remove(obj)
    check(sum(covered[256]) > width * height // 10, "the terrain covers %d pixels" % sum(covered[256]))
    same = sum(a != b for a, b in zip(covered[192], covered[256]))
    coarse = sum(a != b for a, b in zip(covered[128], covered[256]))
    print("covered %d; 192 vs 256: %d pixels differ, 128 vs 256: %d" % (sum(covered[256]), same, coarse))
    check(same == 0, "192 resampled: %d pixels of the outline differ from 256's" % same)
    check(coarse > 0, "the comparison sees no difference at 128")
    print("TERRAIN-SIZES-OK")


if __name__ == "__main__":
    main()
