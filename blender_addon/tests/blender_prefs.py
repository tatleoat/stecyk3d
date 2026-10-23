# SPDX-License-Identifier: GPL-3.0-or-later
# (Making the substrate from the add-on). Run inside Blender:
#   blender -b --factory-startup --python blender_prefs.py -- <stecyk3d-extract> [<drive_c folder holding Bryce3D, Bryce31, Bryce50>]
# The second argument is optional: without it only the checks that need no install run.
import os
import sys
import tempfile

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
rest = sys.argv[sys.argv.index("--") + 1:]
extractor = rest[0]
installs = rest[1] if len(rest) > 1 else ""

import addon_utils  # noqa: E402
import bpy  # noqa: E402

assert addon_utils.enable("stecyk3d", default_set=True), "add-on did not enable"
prefs = bpy.context.preferences.addons["stecyk3d"].preferences
from stecyk3d import engine, extract  # noqa: E402

work = tempfile.mkdtemp(prefix="bb_prefs_")
os.environ["BRYCE_SUBSTRATE"] = os.path.join(work, "out", "substrate.bin")
os.environ["PATH"] = ""  # nothing on PATH: the program has to come from the override or the search

# the three folder fields exist; the free-text list is hidden, not drawn
for name in ("bryce_30", "bryce_31", "bryce_50"):
    assert hasattr(prefs, name), name
assert hasattr(prefs, "install_folders")  # read only to migrate

# nothing set: the operator refuses with one clear message, writing nothing
prefs.extractor_program = extractor
try:
    bpy.ops.stecyk3d.make_substrate()
    raise AssertionError("ran with no folder")
except RuntimeError as e:
    assert "no Bryce folder is set" in str(e), e

# an extractor that is not there: the message names where it looks
prefs.extractor_program = os.path.join(work, "nope")
found, _ = extract.search_program("stecyk3d-extract-not-there", "", env={"PATH": ""}, home=work)
assert found == ""
assert "was not found" in extract.program_missing_message("extractor", "stecyk3d-extract", [])
prefs.extractor_program = extractor
assert engine.find_program(prefs, "extractor")[0] == extractor

# a folder with no program file
empty = os.path.join(work, "empty")
os.makedirs(os.path.join(empty, "sub"))
prefs.bryce_30 = empty
try:
    bpy.ops.stecyk3d.make_substrate()
    raise AssertionError("an empty folder was accepted")
except RuntimeError as e:
    assert "no Bryce program file" in str(e), e
assert not os.path.exists(os.environ["BRYCE_SUBSTRATE"])

# an old saved list moves into the fields on Make
prefs.bryce_30 = ""
prefs.install_folders = "/x/Bryce 5;/x/Bryce31"
try:
    bpy.ops.stecyk3d.make_substrate()
except RuntimeError:
    pass
assert (prefs.bryce_31, prefs.bryce_50, prefs.install_folders) == ("/x/Bryce31", "/x/Bryce 5", ""), \
    (prefs.bryce_31, prefs.bryce_50)
prefs.bryce_31 = prefs.bryce_50 = ""

if installs:
    # a 3.1 install in the 3.0 field is refused, naming both
    prefs.bryce_30 = os.path.join(installs, "Bryce31")
    try:
        bpy.ops.stecyk3d.make_substrate()
        raise AssertionError("wrong version accepted")
    except RuntimeError as e:
        assert "not Bryce 3.0" in str(e) and "Bryce 3.1 folder" in str(e), e
    assert not os.path.exists(os.environ["BRYCE_SUBSTRATE"])
    # the right three: a wrapper folder (the parent of all three would be ambiguous, so give each)
    prefs.bryce_30 = os.path.join(installs, "Bryce3D")
    prefs.bryce_31 = os.path.join(installs, "Bryce31")
    prefs.bryce_50 = os.path.join(installs, "Bryce50")
    assert bpy.ops.stecyk3d.make_substrate() == {"FINISHED"}
    path, message = engine.find_substrate(prefs)
    assert path == os.environ["BRYCE_SUBSTRATE"] and message is None, (path, message)
    assert prefs.substrate == "", "the override stays empty: the substrate is used without a field"

# the preferences panel draws (a stand-in layout: only the calls and the found-program labels are checked)
from unittest import mock  # noqa: E402
layout = mock.MagicMock()
layout.panel.return_value = (mock.MagicMock(), mock.MagicMock())
with mock.patch.object(type(prefs), "layout", layout, create=True):
    type(prefs).draw(prefs, bpy.context)
labels = [str(c) for c in layout.row.return_value.label.call_args_list]
assert any("Renderer:" in l for l in labels) and any("Extractor: " + extractor in l for l in labels), labels

print("PREFS-OK")
