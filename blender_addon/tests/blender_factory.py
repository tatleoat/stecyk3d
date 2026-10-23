# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Task T1 acceptance, run 1:
#   a headless Blender with the add-on installed from its install file, engine selected, rendering
#   Blender's factory default scene to a non-empty picture.
"""F12 on the factory default scene (no sun lamp — task T1 says render it anyway)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)

# The factory scene has Camera, Cube and a POINT light, and no sun: exactly the case that used to stop.
bpy.ops.render.render(write_still=True)
print("FACTORY-F12-DONE")
