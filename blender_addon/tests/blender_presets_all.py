# SPDX-License-Identifier: GPL-3.0-or-later
# Every preset of every material and texture library the add-on offers (the extractor's libraries, through the
# substrate given; or BRYCE_PRESETS, a Presets folder, when set) is applied by the material panel's commands to the
# factory cube's material, the scene is described as F12 describes it, and the renderer draws it (8 by 6 pixels).
# A preset the reader cannot read is refused with the reader's reason, and a material the renderer does not draw
# (its named refusal: "not drawn: what the Bryce programs do here is not known") is listed; anything else is a failure.
# Needs a real substrate (the test substrate holds no library).
#   blender -b --factory-startup --python blender_presets_all.py -- <substrate> <program> <prefix>
import json
import os
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402

bpy, substrate, program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
scene.render.resolution_x, scene.render.resolution_y = 8, 6
module = sys.modules[blender_common.ADDON]
preset_ops, gather, names = module.preset_ops, module.gather, module.names
render_group = getattr(scene, names.SCENE_RENDER_GROUP)
render_group.program = "5.0"
if os.environ.get("BRYCE_PRESETS"):
    render_group.presets_folder = os.environ["BRYCE_PRESETS"]

cube = bpy.data.objects["Cube"]
bpy.context.view_layer.objects.active = cube
choice = preset_ops.choice(bpy.context)
limit = int(os.environ.get("PRESETS_LIMIT", "0"))  # for a quick run: so many presets of each library


def draw():
    """The scene described as F12 describes it and drawn by the renderer; raises with the reason."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    description, problems = gather.describe(depsgraph, [])
    if problems:
        raise RuntimeError("not rendered: %s" % "; ".join(problems))
    description["size"] = [8, 6]
    with tempfile.TemporaryDirectory(prefix="stecyk3d_presets_") as folder:
        desc_path = os.path.join(folder, "scene.json")
        with open(desc_path, "w", encoding="utf-8") as f:
            json.dump(description, f)  # as engine.py writes it (a saved result may hold NaN)
        done = subprocess.run([program, "--substrate", substrate, "--description", desc_path,
                               "--levels", os.path.join(folder, "levels.rgb")], capture_output=True)
        if done.returncode != 0:
            raise RuntimeError("the renderer refused: %s" % done.stderr.decode("utf-8", "replace").strip())


def apply(kind, library, index):
    """Apply one preset as the panel's button does; the reader's refusal comes back as the operator's error."""
    if kind == "materials":
        choice.material_preset = str(index)
        with bpy.context.temp_override(material=cube.active_material, active_object=cube, object=cube):
            return bpy.ops.stecyk3d.apply_material_preset()
    choice.texture_preset = str(index)
    material = cube.active_material
    # A texture preset adds a component: start each from a material with none, as four is the limit.
    module.material_nodes.write_material(material, module.components.empty_material())
    with bpy.context.temp_override(material=material, active_object=cube, object=cube):
        return bpy.ops.stecyk3d.add_texture_preset()


NOT_DRAWN = "not drawn: what the Bryce programs do here is not known"
counts = {"libraries": 0, "presets": 0, "applied": 0}
unread, refused, not_drawn, failed = [], [], [], []
for kind in ("materials", "textures"):
    offered, _ = preset_ops.offered(bpy.context, kind)
    assert offered, "no %s library is offered" % kind
    for label, key in offered:
        library, why = preset_ops.load(bpy.context, key)
        if library is None:
            unread.append((label, None, "the library cannot be read: %s" % why))  # a reader matter, not a preset's
            continue
        counts["libraries"] += 1
        setattr(choice, kind[:-1] + "_library", key)
        shown = module.presets.offered_presets(library)
        for index, name, _ in shown[:limit] if limit else shown:
            counts["presets"] += 1
            unreadable = library["presets"][index].get("unreadable")
            try:
                result = apply(kind, library, index)
                assert result == {"FINISHED"}, result
            except RuntimeError as error:  # an operator's ERROR report
                if unreadable is not None and unreadable in str(error):
                    refused.append((label, name, unreadable))
                else:
                    failed.append((label, name, str(error)))
                continue
            except Exception as error:  # noqa: BLE001 - every other exception is the failure being looked for
                failed.append((label, name, "%s: %s" % (type(error).__name__, error)))
                continue
            try:
                draw()
            except RuntimeError as error:  # the renderer's refusal
                if NOT_DRAWN in str(error):
                    not_drawn.append((label, name, str(error).split("cannot render: ")[-1]))
                else:
                    failed.append((label, name, str(error)))
                continue
            except Exception as error:  # noqa: BLE001 - every other exception is the failure being looked for
                failed.append((label, name, "%s: %s" % (type(error).__name__, error)))
                continue
            counts["applied"] += 1

print("PRESETS %(libraries)d libraries, %(presets)d presets, %(applied)d applied and drawn" % counts,
      "%d unread libraries, %d refused, %d applied but not drawn, %d failed"
      % (len(unread), len(refused), len(not_drawn), len(failed)))
for what, rows in (("UNREAD", unread), ("REFUSED", refused), ("NOT-DRAWN", not_drawn), ("FAILED", failed)):
    for label, name, reason in rows:
        print("%s %s | %s | %s" % (what, label, name, reason))
if failed or any(not reason.startswith("unsupported") for _, _, reason in refused):
    raise SystemExit("PRESETS-ALL-FAILED")
print("PRESETS-ALL-OK")
