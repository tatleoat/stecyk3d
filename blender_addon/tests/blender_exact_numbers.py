# SPDX-License-Identifier: GPL-3.0-or-later
# (Import: the numbers Blender's floats cannot hold). Headless:
#   blender -b --factory-startup --python blender_exact_numbers.py -- <substrate> <program> <out-prefix>
"""An import keeps a document's infinity and NaN in the World's `bryce_exact_numbers`; gather puts them back where the
property still holds Blender's clamp, and leaves a number the user has changed since."""

import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402

FLT_MAX = 3.4028234663852886e38

if __name__ == "__main__":
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    module = sys.modules[blender_common.ADDON]
    sky = module.gather.sky_group(scene.world)
    sky.plain = (FLT_MAX, 0.5, 0.9)  # what Blender holds after being given an infinity
    sky.discs = [1.0] * 15
    sky.discs[9] = FLT_MAX
    scene.world["bryce_sun_flags_explicit"] = 1
    scene.world["bryce_exact_numbers"] = json.dumps({"plain": [math.inf, 0.5, 0.9], "discs": [1.0] * 9 + [math.inf] + [1.0] * 5})
    description, problems = module.gather.describe(bpy.context.evaluated_depsgraph_get())
    assert math.isinf(description["sky"]["plain_colour"][0]), description["sky"]["plain_colour"]
    assert math.isinf(description["sky"]["discs"][9]), description["sky"]["discs"]
    sky.plain = (0.25, 0.5, 0.9)  # the user changed it: their number wins
    description, problems = module.gather.describe(bpy.context.evaluated_depsgraph_get())
    assert description["sky"]["plain_colour"][0] == 0.25, description["sky"]["plain_colour"]
    # the camera's own numbers (import_op.keep_exact_camera / gather.exact_camera): a pitch whose double cosine and sine
    # Blender would hold as singles comes back to the double while the camera is untouched, and not once it is moved
    c, sn = math.cos(math.radians(8.0)), math.sin(math.radians(8.0))
    plan_cam = {"name": "exact", "sensor_fit": "HORIZONTAL", "sensor_width": 36.0, "lens": 47.8211042277,
                "shift_x": 0.0, "shift_y": 0.0, "resolution": [64, 48], "pixel_aspect": [1.0, 1.0],
                "matrix": [[1.0, 0.0, 0.0, 0.0], [0.0, c, -sn, 0.5], [0.0, sn, c, -30.0], [0.0, 0.0, 0.0, 1.0]]}
    cam = module.import_op.make_camera(plan_cam, scene, scene.collection)
    bpy.context.view_layer.update()
    exact = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["camera"]
    assert abs(exact["forward"][1] + c) < 1e-15 and abs(exact["forward"][2] - sn) < 1e-15, exact  # Bryce y and z swapped
    cam.location.x += 1.0  # moved by the user: Blender's singles are read, the double no longer applies
    bpy.context.view_layer.update()
    moved = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["camera"]
    assert abs(moved["forward"][2] - sn) > 1e-12, moved  # Blender's singles
    assert abs(moved["position"][0] - 1.0) < 1e-6, moved["position"]
    # the sky's binary64 numbers (import_op.keep_exact_sky / gather.unchanged_exact_sky): the fog 0.005 is a double in the
    # document and's whole number F is (fog * 100 + 0.5) cut; the single 0.004999999888 makes it 0
    sky.fog, sky.cover = 0.005, 0.25
    assert sky.fog != 0.005  # Blender holds the single
    module.import_op.keep_exact_sky(scene.world, {"fog": 0.005, "cover": 0.25})
    assert set(json.loads(scene.world["bryce_exact_doubles"])) == {"fog"}, scene.world["bryce_exact_doubles"]  # 0.25 is exact
    assert module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["sky"]["fog"] == 0.005
    sky.fog = 0.01  # changed by the user: their number wins
    assert abs(module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["sky"]["fog"] - 0.01) < 1e-8
    # the sun direction of any length (sw-05, sw-06): the lamp's own axis is unit, the document's direction is sent
    # while the lamp stands where the import put it
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.rotation_euler = (0.3, 0.2, 0.1)
    bpy.context.view_layer.update()
    sky.sun_object = sun
    wanted = [0.6188527477552761, 1.2377054955105522, -1.4439897447623107]
    module.import_op.keep_exact_sun(scene.world, {"sun_direction_exact": wanted}, sun)
    got = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["sun"]["direction"]
    assert got == wanted, got
    sun.rotation_euler = (0.5, 0.2, 0.1)  # moved: the lamp's unit axis
    bpy.context.view_layer.update()
    got = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["sun"]["direction"]
    assert abs(sum(c * c for c in got) - 1.0) < 1e-9, got
    # a document's tree (import_op.keep_stored_tree / gather.kept_stored_tree): sent as stored, with its shared number,
    # while the growth settings are those of the import; grown again from them once the user changes one
    identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
    stored = {"pieces": [{"parent": -1, "matrix": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]}], "leaves": [], "shapes": []}
    plan_tree = {"kind": "tree", "name": "doc tree", "object_type": "MESH", "bryce_shape": "tree", "matrix": identity,
                 "leaves_material": None, "stored": stored, "stored_id": 7}
    tree = module.import_op.make_object(plan_tree, scene.collection, {}, 1.0)
    bpy.context.view_layer.update()
    entry = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["trees"][0]
    assert entry.get("stored") == stored and entry.get("stored_id") == 7 and "tree" not in entry, entry
    tree.bryce_tree.gravity += 0.25  # a changed setting: grown from the settings again
    tree.update_tag()
    bpy.context.view_layer.update()
    entry = module.gather.describe(bpy.context.evaluated_depsgraph_get())[0]["trees"][0]
    assert "stored" not in entry and "tree" in entry, entry
    print("EXACT-NUMBERS-OK")
