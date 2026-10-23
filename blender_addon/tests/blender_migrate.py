# SPDX-License-Identifier: GPL-3.0-or-later
# Needs the installed add-on (run_all 8g2):  blender -b --factory-startup --python blender_migrate.py -- x x <out-prefix>
# A file saved under the first release's names (engine BIG_BRUCE_3D, plain custom properties big_bruce_*, the preset
# picks scene.big_bruce_presets) is moved to the current names when it is opened; the first release's saved
# preferences are copied once into this one's empty fields.
"""Opening a file of the first release moves its engine id, custom properties and preset picks; preferences copy once."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402


def main():
    import addon_utils
    import bpy
    prefix = sys.argv[-1]
    addon_utils.enable(blender_common.ADDON, default_set=True)
    migrate = sys.modules[blender_common.ADDON + ".migrate"]

    # make a file the way the first release left it
    migrate.hold_old_engine_id(True)
    scene = bpy.context.scene
    scene.render.engine = migrate.OLD_ENGINE_ID
    other = bpy.data.scenes.new("other")
    other.render.engine = "BLENDER_EEVEE"
    material = bpy.data.materials.new("m")
    material.use_fake_user = True  # kept in the file with no user
    material["big_bruce_shown"] = 3
    scene["big_bruce_whole_import"] = "/some/doc.br5"
    # the earlier name of the scene's preset choices: a property group registered under it for the save
    old_group = type("OldChoice", (bpy.types.PropertyGroup,), {
        "__annotations__": {"copy_letter": bpy.props.EnumProperty(items=[(c, c, "") for c in "ABCD"])},
        "__module__": __name__})
    bpy.utils.register_class(old_group)
    bpy.types.Scene.big_bruce_presets = bpy.props.PointerProperty(type=old_group)
    scene.big_bruce_presets.copy_letter = "C"
    path = prefix + "-old.blend"
    bpy.ops.wm.save_as_mainfile(filepath=path)
    migrate.hold_old_engine_id(False)
    del bpy.types.Scene.big_bruce_presets
    bpy.utils.unregister_class(old_group)
    assert bpy.types.RenderEngine.bl_rna_get_subclass_py(migrate.OLD_ENGINE_ID) is None

    bpy.ops.wm.open_mainfile(filepath=path)
    engines = {s.name: s.render.engine for s in bpy.data.scenes}
    assert engines["Scene"] == "STECYK3D", engines
    assert engines["other"] == "BLENDER_EEVEE", engines
    assert bpy.data.materials["m"].get("stecyk3d_shown") == 3
    assert "big_bruce_shown" not in bpy.data.materials["m"].keys()
    assert bpy.context.scene.get("stecyk3d_whole_import") == "/some/doc.br5"
    assert "big_bruce_whole_import" not in bpy.context.scene.keys()
    assert bpy.context.scene.stecyk3d_presets.copy_letter == "C", bpy.context.scene.stecyk3d_presets.copy_letter
    assert not hasattr(bpy.context.scene, "big_bruce_presets")
    # the stand-in engine id is gone again after the load
    assert bpy.types.RenderEngine.bl_rna_get_subclass_py(migrate.OLD_ENGINE_ID) is None

    # a second open of a migrated file changes nothing
    bpy.ops.wm.save_as_mainfile(filepath=prefix + "-new.blend")
    bpy.ops.wm.open_mainfile(filepath=prefix + "-new.blend")
    assert bpy.context.scene.render.engine == "STECYK3D"
    # the earlier extension's saved preferences are copied once into this one's empty fields
    addons = bpy.context.preferences.addons
    if migrate.OLD_ADDON_ID not in addons.keys():
        addons.new().module = migrate.OLD_ADDON_ID
    writer = type("EarlierPrefsWriter", (bpy.types.AddonPreferences,), {
        "bl_idname": migrate.OLD_ADDON_ID,
        "__annotations__": {n: bpy.props.StringProperty() for n in ("bryce_30", "bryce_31", "bryce_50", "substrate")}})
    bpy.utils.register_class(writer)
    old = addons[migrate.OLD_ADDON_ID].preferences
    old.bryce_30, old.bryce_50, old.substrate = "/old/Bryce3D", "/old/Bryce50", "/old/substrate.bin"
    bpy.utils.unregister_class(writer)
    new = addons[blender_common.ADDON].preferences
    new.earlier_values_read, new.bryce_31 = False, "/mine/Bryce31"
    new.bryce_30 = new.bryce_50 = new.substrate = ""
    copied = migrate.migrate_preferences()
    assert (new.bryce_30, new.bryce_50, new.substrate) == ("/old/Bryce3D", "/old/Bryce50", "/old/substrate.bin"), copied
    assert new.bryce_31 == "/mine/Bryce31"  # a value already set is kept
    new.bryce_30 = ""
    assert migrate.migrate_preferences() == {} and new.bryce_30 == ""  # read once only: a cleared field stays cleared
    print("MIGRATE-OK")


main()
