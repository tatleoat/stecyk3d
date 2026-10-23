# SPDX-License-Identifier: GPL-3.0-or-later
# debug (task T1): writes the description JSON the add-on would hand the renderer for the
# sun-and-objects test scene, without rendering. Usage inside Blender:
#   blender -b --factory-startup --python blender_addon/tests/dump_desc.py -- <substrate> <program> <prefix>
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
import blender_sun_objects  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
blender_sun_objects.build(bpy, scene)

depsgraph = bpy.context.evaluated_depsgraph_get()
module = sys.modules["bl_ext.user_default.stecyk3d"]
description, problems = module.gather.describe(depsgraph)
description["size"] = [64, 48]
with open(prefix + "-desc.json", "w", encoding="utf-8") as f:
    json.dump(description, f, indent=1)
print("DUMP-DESC problems=%r" % (problems,))
