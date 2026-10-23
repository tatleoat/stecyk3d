# SPDX-License-Identifier: GPL-3.0-or-later
# (The refusal report). A refused render in a real Blender: every problem
# on its own line in the Info editor's reports and in the text "Stecyk3D report" (made, then replaced by the
# next refusal), the error a short line that says where the list is.
#   blender -b --factory-startup --python blender_refusal_report.py -- <substrate> <program> <out-prefix>
"""A gather problem list and a renderer refusal, each reported in full."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
module = sys.modules[blender_common.ADDON]
TEXT = "Stecyk3D report"

# 1. three problems from the reader
real = module.gather.describe
module.gather.describe = lambda depsgraph, notices=None: ({}, ["first problem", "second problem", "third problem " + "x" * 200])
try:
    bpy.ops.render.render(write_still=True)
    raise SystemExit("REFUSAL-BAD: the render did not refuse")
except RuntimeError as error:
    message = str(error)
module.gather.describe = real
assert "see Text Editor > Stecyk3D report" in message, message
assert "first problem" in message and "second problem" not in message and len(message) < 220, message
body = bpy.data.texts[TEXT].as_string().splitlines()
assert body[0] == "Not rendered:" and len(body) == 4, body
assert body[1].strip() == "first problem" and body[3].strip().startswith("third problem x"), body

# 2. the renderer refuses (the stand-in program fails on request): the text is replaced, not added to
os.environ["FAKE_FAIL"] = "1"
module_prefs = bpy.context.preferences.addons[blender_common.ADDON].preferences
wrapper = prefix + "-fake.sh"
with open(wrapper, "w") as f:
    f.write('#!/bin/sh\nexec python3 "%s" "$@"\n' % os.path.join(os.path.dirname(os.path.abspath(__file__)), "fake_renderer.py"))
os.chmod(wrapper, 0o755)
module_prefs.renderer_program = wrapper
try:
    bpy.ops.render.render(write_still=True)
    raise SystemExit("REFUSAL-BAD: the fake renderer did not refuse")
except RuntimeError as error:
    message = str(error)
assert "see Text Editor > Stecyk3D report" in message, message
assert len([t for t in bpy.data.texts if t.name.startswith(TEXT)]) == 1
assert bpy.data.texts[TEXT].as_string().startswith("The renderer refused:"), bpy.data.texts[TEXT].as_string()
print("REFUSAL-REPORT-OK")
