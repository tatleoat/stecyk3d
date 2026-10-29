# SPDX-License-Identifier: GPL-3.0-or-later
# (Output paths.) An output path with no drive and no `//` ("out\lake.png"): Blender itself resolves it (on Windows
# onto the root of the Windows drive, elsewhere against the working folder) and Stecyk3D's picture lands exactly where
# Workbench's does (the same rule as Cycles and Eevee), with exact_png beside it; a `//` path goes beside the saved
# blend file. The files and folders this makes are removed again.
#   blender -b --factory-startup --python blender_output_paths.py -- <substrate> <program> <out-prefix>
"""Relative output paths: Stecyk3D writes where Blender's own engines write."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
from png_read import read_png  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
folder = "stecyk3d-relative-test-%d" % os.getpid()  # no drive, no //
made = []


def render(engine, name, exact=""):
    scene.render.engine = engine
    scene.render.filepath = os.path.join(folder, name)
    scene.bryce_render.exact_png = exact
    bpy.ops.render.render(write_still=True)
    # Blender's own answer for where its output path goes (frame_path resolves it as the render does; the still
    # takes the path without the frame number)
    place = os.path.join(os.path.dirname(scene.render.frame_path(frame=scene.frame_current)), name)
    made.append(place)
    return place


try:
    ours = render("STECYK3D", "lake.png", os.path.join(folder, "lake-exact.png"))
    theirs = render("BLENDER_WORKBENCH", "lake-workbench.png")
    assert os.path.isfile(ours), "OUTPUT-PATHS-BAD: Stecyk3D's picture is not at %s" % ours
    assert os.path.isfile(theirs), "OUTPUT-PATHS-BAD: Workbench's picture is not at %s" % theirs
    assert os.path.dirname(ours) == os.path.dirname(theirs)
    exact = os.path.join(os.path.dirname(ours), "lake-exact.png")
    made.append(exact)
    assert os.path.isfile(exact), "OUTPUT-PATHS-BAD: exact_png is not beside the picture, at %s" % exact
    assert read_png(exact)[3] == bytes(b for i, b in enumerate(read_png(ours)[3]) if i % 4 != 3)
    if sys.platform.startswith("win"):
        assert os.path.splitdrive(ours)[1].startswith("\\" + folder), ours  # the drive's root, as Blender does
    print("OUTPUT-PATHS relative:", os.path.dirname(ours))

    # `//`: beside the saved blend file, for the picture and for exact_png
    bpy.ops.wm.save_as_mainfile(filepath=prefix + "-paths.blend")
    scene.render.engine = "STECYK3D"
    scene.render.filepath = "//%s-paths-out/lake.png" % os.path.basename(prefix)
    scene.bryce_render.exact_png = "//%s-paths-out/lake-exact.png" % os.path.basename(prefix)
    bpy.ops.render.render(write_still=True)
    beside = os.path.join(os.path.dirname(prefix), os.path.basename(prefix) + "-paths-out")
    for name in ("lake.png", "lake-exact.png"):
        assert os.path.isfile(os.path.join(beside, name)), "OUTPUT-PATHS-BAD: %s not beside the blend file" % name
    print("OUTPUT-PATHS-OK")
finally:
    for path in made:
        if os.path.isfile(path):
            os.remove(path)
    for path in {os.path.dirname(p) for p in made}:
        if os.path.isdir(path) and not os.listdir(path):
            os.rmdir(path)
