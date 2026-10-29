# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Tree Lab trees in a real Blender: four trees added with Add > Bryce shape > Tree stand at their own
# places (each tree's world carries the object's place, and the picture shows four trees, not one at the origin), and
# a tree straight from the Add menu (every Tree Lab setting 0) is refused with its name and the setting, not drawn
# as a black picture.
#   blender -b --factory-startup --python blender_trees.py -- <substrate> <program> <out-prefix>
"""Tree Lab trees at their places; a fresh tree refused."""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
from png_read import read_png  # noqa: E402

PLACES = (-3.3, -1.1, 1.1, 3.3)
# settings that grow a tree (the add-on test scene's, no leaves)
SETTINGS = {"whorls": (6, 8), "trunk_whorl": 2, "whorl": 3, "angle_thick": 0.5, "angle_thin": 1.0, "thickness": 0.125,
            "least": 0.015625, "share": (0.5, 0.75), "random": 0.25, "spread": 0.5, "gravity": 0.125, "twist": 1.0,
            "leaves": 0, "leaf_kind": "5", "leaf_size": 0.25, "bottom": (0.25, 0.25), "low": (0.5, 0.5),
            "high": (0.75, 0.25), "top": (0.25, 0.75)}

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
scene.render.resolution_x, scene.render.resolution_y = 160, 60
scene.bryce_render.program = "5.0"
scene.bryce_render.exact_png = prefix + "-exact.png"
module = sys.modules[blender_common.ADDON]
for obj in [o for o in scene.objects if o.type != "CAMERA"]:
    bpy.data.objects.remove(obj)
scene.camera.location = (0.0, -14.0, 1.0)
scene.camera.rotation_euler = (math.pi / 2, 0.0, 0.0)
scene.camera.data.lens = 35.0
scene.camera.data.sensor_width = 36.0
sky = scene.world.bryce
sky.sun_intensity = 1.0
sky.sun_color = (1.0, 1.0, 1.0)
sky.ambient_color = (0.2, 0.2, 0.2)


def add_tree(x):
    scene.cursor.location = (x, 0.0, 1.0)  # the tree is 2 high, its foot 1 below the origin
    bpy.ops.stecyk3d.add_shape(shape="tree")
    return bpy.context.active_object


# 1. a fresh tree is refused, naming the tree and the setting
fresh = add_tree(0.0)
try:
    bpy.ops.render.render(write_still=True)
    raise SystemExit("TREES-BAD: a fresh tree was rendered")
except RuntimeError:
    pass
report = bpy.data.texts["Stecyk3D report"].as_string()
assert fresh.name in report and "trunk-thickness is 0" in report, report
bpy.data.objects.remove(fresh)

# 2. four trees at their own places
for k, x in enumerate(PLACES):
    tree = add_tree(x)
    group = getattr(tree, module.names.OBJECT_TREE_GROUP)
    for key, value in SETTINGS.items():
        setattr(group, key, value)
    group.seed = 1000 + k
entries = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["trees"]
assert sorted(round(e["world"][9], 4) for e in entries) == list(PLACES), entries

def picture(trees_shown):
    for obj in scene.objects:
        if obj.type == "MESH":
            obj.hide_render = not trees_shown
    bpy.ops.render.render(write_still=True)
    return read_png(prefix + "-exact.png")


width, height, channels, sky = picture(False)
pixels = picture(True)[3]
# the columns where a tree stands: some pixel differs from the sky alone
at = lambda p, x, y: p[channels * (y * width + x):channels * (y * width + x) + 3]  # noqa: E731
covered = [any(at(pixels, x, y) != at(sky, x, y) for y in range(height)) for x in range(width)]
runs, x = [], 0
while x < width:
    if covered[x]:
        start = x
        while x < width and covered[x]:
            x += 1
        runs.append((start + x - 1) / 2.0)
    x += 1
# where each tree's trunk should be: x / (distance · sensor half width / lens) across the half width
expected = [width / 2.0 * (1.0 + p / (14.0 * 18.0 / 35.0)) for p in PLACES]
assert all(any(abs(r - e) < 8 for r in runs) for e in expected), (runs, expected)
assert not any(abs(r - width / 2.0) < 4 for r in runs), ("a tree at the origin", runs)
print("TREES-OK")
