# SPDX-License-Identifier: GPL-3.0-or-later
"""Files and preferences made under the add-on's first name keep working.

The first release shipped as the extension `big_bruce` with the render engine `BIG_BRUCE_3D`. This is the only place
that still spells those names. Blender resets a scene whose engine is not registered while it reads the file (before
any load_post handler runs), so the old engine id is registered again for the moment a file is read (load_pre) and
dropped once the scenes have been moved to the new id (load_post).
"""

OLD_ENGINE_ID = "BIG_BRUCE_3D"
OLD_ADDON_ID = "bl_ext.user_default.big_bruce"
# our own plain custom properties: old key -> new key
OLD_PRESETS = "big_bruce_presets"  # the scene property that held the preset picks (now scene.stecyk3d_presets)
RENAMED_KEYS = {"big_bruce_shown": "stecyk3d_shown", "big_bruce_whole_import": "stecyk3d_whole_import"}
OLD_PROGRAM_FOLDER = "vista"  # the old default folder of the substrate (now named after the program, stecyk3d)
OLD_PROGRAM_NAMES = {"vista-render": "stecyk3d-render", "vista-extract": "stecyk3d-extract",
                     "vista-cases": "stecyk3d-cases"}


def move_keys(holder):
    """Move the renamed custom properties of one datablock (anything with keys/[]/del); returns how many moved.
    A value already under the new key wins, and the old key goes either way."""
    moved = 0
    for old, new in RENAMED_KEYS.items():
        if old in holder.keys():
            if new not in holder.keys():
                holder[new] = holder[old]
                moved += 1
            del holder[old]
    return moved


def renamed_program(path):
    """A saved path to one of the old program names, spelled with the new name; other paths unchanged."""
    import os
    folder, base = os.path.split(path)
    stem, dot, ext = base.partition(".")
    if stem in OLD_PROGRAM_NAMES:
        return os.path.join(folder, OLD_PROGRAM_NAMES[stem] + dot + ext)
    return path


def migrate_data(scenes, materials):
    """Scenes on the old engine go to the new one; the renamed custom properties move. Returns (scenes, keys)."""
    from . import names
    switched = 0
    for scene in scenes:
        if scene.render.engine == OLD_ENGINE_ID:
            scene.render.engine = names.ENGINE_ID
            switched += 1
    keys = sum(move_keys(holder) for holder in list(scenes) + list(materials))
    return switched, keys


def migrate_presets(scenes):
    """Carry the scene's remembered preset picks to scene.stecyk3d_presets: the component letter, the material to copy
    from and the picture. (The library and preset menus are lists built on the fly, so those picks start afresh.)
    Blender hands out a registered property's saved values only while it is registered, so a stand-in is registered
    for the read. Returns how many scenes had values."""
    import bpy
    group = type("EarlierPresetChoice", (bpy.types.PropertyGroup,), {
        "__annotations__": {
            "copy_from": bpy.props.PointerProperty(type=bpy.types.Material),
            "copy_letter": bpy.props.EnumProperty(items=[(c, c, "") for c in "ABCD"]),
            "picture": bpy.props.PointerProperty(type=bpy.types.Image)},
        "__module__": __name__})
    bpy.utils.register_class(group)
    setattr(bpy.types.Scene, OLD_PRESETS, bpy.props.PointerProperty(type=group))
    moved = 0
    try:
        for scene in scenes:
            old, new = getattr(scene, OLD_PRESETS), getattr(scene, "stecyk3d_presets", None)
            if new is None:
                continue
            # an untouched old group reads as its defaults (letter A, no material, no picture): nothing to carry
            if old.copy_letter != "A" or old.copy_from is not None or old.picture is not None:
                if new.copy_letter == "A" and new.copy_from is None and new.picture is None:
                    new.copy_letter, new.copy_from, new.picture = old.copy_letter, old.copy_from, old.picture
                    moved += 1
    finally:
        delattr(bpy.types.Scene, OLD_PRESETS)
        bpy.utils.unregister_class(group)
    return moved


def migrate_preferences():
    """Best effort: carry the old extension's preference values (the three Bryce folders, the substrate file and
    the two program overrides) into this one's, where this one's are still empty. The old values stay in the
    user's preferences file, but Blender hands out an add-on's preferences only while a class for them is
    registered, so a stand-in class is registered for the read."""
    import os
    import bpy
    addons = bpy.context.preferences.addons
    if OLD_ADDON_ID not in addons.keys() or __package__ not in addons.keys():
        return {}
    fields = {"bryce_30": "DIR_PATH", "bryce_31": "DIR_PATH", "bryce_50": "DIR_PATH", "substrate": "FILE_PATH",
              "renderer_program": "FILE_PATH", "extractor_program": "FILE_PATH"}
    annotations = {name: bpy.props.StringProperty(subtype=subtype) for name, subtype in fields.items()}
    stand_in = type("Stecyk3DEarlierPreferences", (bpy.types.AddonPreferences,),
                    {"bl_idname": OLD_ADDON_ID, "__annotations__": annotations})
    bpy.utils.register_class(stand_in)
    try:
        old = addons[OLD_ADDON_ID].preferences
        new = addons[__package__].preferences
        if old is None or new is None or getattr(new, "earlier_values_read", True):
            return {}
        new.earlier_values_read = True
        copied = {}
        for name in fields:
            value = getattr(old, name, "")
            if not value or getattr(new, name):
                continue
            if name.endswith("_program"):
                value = renamed_program(value)
                if not os.path.isfile(bpy.path.abspath(value)):
                    continue
            setattr(new, name, value)
            copied[name] = value
        return copied
    finally:
        bpy.utils.unregister_class(stand_in)


_legacy_engine = None
_legacy_registered = False


def _engine_class():
    """A stand-in with the old id and nothing else (not a subclass of the real engine: Blender then drops the real
    engine's own class when this one is unregistered)."""
    global _legacy_engine
    if _legacy_engine is None:
        import bpy

        def render(self, depsgraph):
            self.report({"ERROR"}, "This scene still names the earlier render engine id; choose Stecyk3D")

        _legacy_engine = type("EarlierEngineId", (bpy.types.RenderEngine,),
                              {"bl_idname": OLD_ENGINE_ID, "bl_label": "Stecyk3D (earlier id)", "render": render,
                               "__module__": __name__})
    return _legacy_engine


def hold_old_engine_id(on):
    import bpy
    global _legacy_registered
    if on and not _legacy_registered:
        bpy.utils.register_class(_engine_class())
        _legacy_registered = True
    elif not on and _legacy_registered:
        bpy.utils.unregister_class(_engine_class())
        _legacy_registered = False


def _persistent(function):
    import bpy
    return bpy.app.handlers.persistent(function)


def _on_load_pre(*_):
    hold_old_engine_id(True)


def _on_load_post(*_):
    import bpy
    try:
        migrate_data(bpy.data.scenes, bpy.data.materials)
        migrate_presets(bpy.data.scenes)
        migrate_preferences()
    finally:
        hold_old_engine_id(False)


_handlers = []


def register():
    import bpy
    pre, post = _persistent(_on_load_pre), _persistent(_on_load_post)
    _handlers[:] = [pre, post]
    bpy.app.handlers.load_pre.append(pre)
    bpy.app.handlers.load_post.append(post)
    try:
        migrate_preferences()
    except Exception:  # best effort: the install script sets the folders again anyway
        pass


def unregister():
    import bpy
    for handlers, handler in ((bpy.app.handlers.load_pre, _handlers[0:1]), (bpy.app.handlers.load_post, _handlers[1:2])):
        for h in handler:
            if h in handlers:
                handlers.remove(h)
    _handlers.clear()
    hold_old_engine_id(False)
