# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Part of the headless Blender checks of task T1:
# shared between blender_factory.py and blender_sun_objects.py. Run only inside Blender
# (`blender -b --python ...`); the paths come from the arguments after `--`.
"""Enable the installed add-on, name the substrate and the renderer program, choose the engine."""

import sys

ADDON = "bl_ext.user_default.stecyk3d"


def arguments():
    argv = sys.argv
    rest = argv[argv.index("--") + 1:] if "--" in argv else []
    if len(rest) != 3:
        raise SystemExit("usage: -- <substrate> <renderer-program> <out-prefix>")
    return rest


def start():
    """Enable the extension and set the add-on's preferences; returns (bpy, substrate, program, prefix)."""
    import addon_utils
    substrate, program, prefix = arguments()
    module = addon_utils.enable(ADDON, default_set=True)
    if module is None:
        raise SystemExit("the add-on %s did not enable" % ADDON)
    import bpy
    prefs = bpy.context.preferences.addons[ADDON].preferences
    prefs.substrate = substrate
    prefs.renderer_program = program
    return bpy, substrate, program, prefix


def setup(bpy, prefix):
    """Engine, a small size, and PNG output at <prefix>-render.png. Returns the scene."""
    scene = bpy.context.scene
    scene.render.engine = "STECYK3D"
    scene.render.resolution_x = 64
    scene.render.resolution_y = 48
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_depth = "8"
    scene.render.filepath = prefix + "-render.png"
    return scene
