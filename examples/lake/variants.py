# SPDX-License-Identifier: GPL-3.0-or-later
"""Three looks of the lake at once, each built and rendered by a headless Blender of its own (one Blender builds and
renders serially), then one labelled contact sheet to compare them.

    blender -b --python variants.py -- <out folder> [--res WxH]

Writes <out>/<look>.png (and _raw.png), a log for each, and <out>/sheet.png.
"""
import importlib
import os

import bpy

api = importlib.import_module(next(k for k in bpy.context.preferences.addons.keys() if k.endswith("stecyk3d"))
                              + ".api")

ARGS = api.arguments()
OUT = os.path.abspath(ARGS.positional[0])
LOOKS = {  # each a set of build_lake.py's flags
    "moon_9": {"moon": 9},
    "moon_12_haze": {"moon": 12, "haze": 0.04},
    "glassy": {"reflect": 0.85, "bump": 0.004, "no_halo": True},
}
paths = api.render_parallel(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build_lake.py"), LOOKS, OUT,
                            args=["--quick", "--res", ARGS.get("--res", "640x452")])
api.contact_sheet(paths, os.path.join(OUT, "sheet.png"),
                  labels=["%s  %s" % (name, " ".join(api.variant_flags(flags)))
                          for name, flags in LOOKS.items()])
