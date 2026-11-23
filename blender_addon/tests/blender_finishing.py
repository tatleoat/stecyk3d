# SPDX-License-Identifier: GPL-3.0-or-later
# (Finishing, not Bryce's.) In a real Blender: finishing.py's own tests under Blender's Python (it has numpy); with
# every finishing off, the saved picture is the renderer's levels byte for byte; each finishing changes the saved
# picture and never exact_png; the palette holds its colours; a lens flare with an orthographic camera is refused.
#   blender -b --factory-startup --python blender_finishing.py -- <substrate> <program> <out-prefix>
"""F12 with each finishing switch, against the exact picture."""

import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blender_common  # noqa: E402
import blender_sun_objects  # noqa: E402
from png_read import read_png  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()

# 1. finishing.py's own tests, here where numpy is (plain Python skips the picture ones)
sys.path.insert(0, os.path.dirname(HERE))
import test_finishing  # noqa: E402
outcome = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromModule(test_finishing))
assert outcome.wasSuccessful() and not outcome.skipped, "FINISHING-BAD: the unit tests failed or were skipped"

scene = blender_common.setup(bpy, prefix)
blender_sun_objects.build(bpy, scene)
group = scene.bryce_render
group.exact_png = prefix + "-exact.png"


def render(name):
    scene.render.filepath = "%s-%s.png" % (prefix, name)
    bpy.ops.render.render(write_still=True)
    saved = read_png(scene.render.filepath)
    exact = read_png(group.exact_png)
    rgb = bytes(b for i, b in enumerate(saved[3]) if saved[2] == 3 or i % 4 != 3)
    return rgb, exact[3]


# 2. all off (the defaults): the saved picture is the renderer's own levels
for switch in ("glow", "flare", "jpeg", "palette"):
    assert getattr(group, switch) is False, switch
plain, exact = render("off")
assert plain == exact, "FINISHING-BAD: with finishing off the saved picture is not the exact one"

# 3. each finishing changes the saved picture, never exact_png
for switch, extra in (("glow", {"finish_threshold": 0.3}), ("jpeg", {"jpeg_quality": 20}),
                      ("palette", {"palette_colours": 8})):
    setattr(group, switch, True)
    for key, value in extra.items():
        setattr(group, key, value)
    finished, still_exact = render(switch)
    setattr(group, switch, False)
    assert still_exact == exact, "FINISHING-BAD: %s changed exact_png" % switch
    assert finished != exact, "FINISHING-BAD: %s changed nothing" % switch
    if switch == "glow":
        assert all(a >= b for a, b in zip(finished, exact)), "FINISHING-BAD: the glow darkened a pixel"
    if switch == "palette":
        colours = {finished[i:i + 3] for i in range(0, len(finished), 3)}
        assert len(colours) <= 8, "FINISHING-BAD: the palette holds %d colours" % len(colours)
    print("FINISHING", switch, "changed", sum(a != b for a, b in zip(finished, exact)), "levels")
group.finish_threshold = 0.6

# 4. off again: the exact picture again
again, _ = render("off-again")
assert again == exact, "FINISHING-BAD: switching finishing off did not give the exact picture back"

# 5. a lens flare needs a perspective camera: an orthographic one is refused with the reason
group.flare = True
scene.camera.data.type = "ORTHO"
try:
    bpy.ops.render.render(write_still=True)
    raise SystemExit("FINISHING-BAD: the flare with an orthographic camera was not refused")
except RuntimeError as error:
    assert "perspective camera" in str(error), str(error)
scene.camera.data.type = "PERSP"
render("flare")  # with a perspective camera it renders (the sun may be out of the frame: no flare is then drawn)
group.flare = False

# 6. the Finishing subpanel: the threshold and the four switches, and each switch's own settings while it is on
module = sys.modules[blender_common.ADDON]
drawn = []


class Layout:
    def __getattr__(self, _name):
        return lambda *args, **kw: self

    def __setattr__(self, _name, _value):
        pass

    def prop(self, data, key, **kw):
        drawn.append(key)
        getattr(data, key)


class Panel:
    layout = Layout()


for on in (False, True):
    for switch in ("glow", "flare", "jpeg", "palette"):
        setattr(group, switch, on)
    del drawn[:]
    module.panels.STECYK3D_PT_render_finishing.draw(Panel(), bpy.context)
    rows = module.finishing.SETTINGS if on else ("finish_threshold", "glow", "flare", "jpeg", "palette")
    assert sorted(drawn) == sorted(rows), (on, drawn)
print("FINISHING-OK")
