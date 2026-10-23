# SPDX-License-Identifier: GPL-3.0-or-later
# (the libraries and presets offered in the material panel; `presets_folder`, else the extractor's
# libraries; the messages when none), (the texture
#       components panel: copy from a material, add a picture, show, remove, add a mapping; the node editor's
#       new procedural texture)
# (needs Blender, not testable here)
"""The material panel's presets and texture components, and their commands."""

import base64
import os

import bpy

from . import components, material_nodes, names, presets, reader
from .engine import find_substrate, preferences

_cache = {}  # (path, mtime) -> library JSON, or the reader's refusal
_items = {}  # enum items kept alive for Blender (a dynamic enum's strings must outlive the call)


def make_images(pictures):
    """'s pictures as Blender images (the first row at the top: Blender's rows run from the bottom)."""
    images = {}
    for p in pictures:
        width, height = int(p["width"]), int(p["height"])
        data = base64.b64decode(p["argb_base64"])
        image = bpy.data.images.new(p["name"] or "picture", width=width, height=height, alpha=True)
        pixels = [0.0] * (width * height * 4)
        for row in range(height):
            target = height - 1 - row
            for col in range(width):
                at = 4 * (row * width + col)
                a, r, g, b = data[at:at + 4]
                k = 4 * (target * width + col)
                pixels[k:k + 4] = (r / 255.0, g / 255.0, b / 255.0, a / 255.0)
        image.pixels.foreach_set(pixels)
        image.pack()
        images[p["name"]] = image
    return images


def program_of(context):
    from .engine import find_program
    return find_program(preferences(context), "renderer")[0]


def offered(context, kind):
    """: [(label, key)] of the libraries of `kind`; a key is a file path, or "kept:PATH" for the extractor's."""
    render = getattr(context.scene, names.SCENE_RENDER_GROUP)
    folder = bpy.path.abspath(render.presets_folder) if render.presets_folder else ""
    if folder:
        return presets.offered_libraries(folder)[kind], True
    prefs = preferences(context)
    substrate, _ = find_substrate(prefs)
    if substrate is None:
        return [], False
    try:
        listing = reader.kept_libraries(program_of(context), substrate, render.program)
    except reader.ReadRefused:
        return [], False
    return [(label, "kept:" + path) for label, path in presets.kept_offered(listing)[kind]], False


def load(context, key):
    """: the library behind a key, read once per file version; (library, None) or (None, reason)."""
    if not key:
        return None, None
    program = program_of(context)
    try:
        if key.startswith("kept:"):
            substrate, message = find_substrate(preferences(context))
            if substrate is None:
                return None, message
            stamp = (key, os.path.getmtime(substrate))
            if stamp not in _cache:
                _cache[stamp] = reader.read_kept_library(program, substrate, key[5:])
        else:
            stamp = (key, os.path.getmtime(key))
            if stamp not in _cache:
                _cache[stamp] = reader.read_library(program, key)
        return _cache[stamp], None
    except (reader.ReadRefused, OSError) as error:
        return None, str(error)


def _library_items(kind):
    def items(self, context):
        libs, _ = offered(context, kind)
        out = [(key, label, key) for label, key in libs] or [("", "(none)", "")]
        _items[("lib", kind)] = out
        return out
    return items


def _preset_items(kind):
    def items(self, context):
        key = getattr(self, kind[:-1] + "_library")
        library, reason = load(context, key)
        if library is None:
            out = [("", "(%s)" % (reason or "no library"), "")]
        else:
            out = [(str(i), name, description) for i, name, description in presets.offered_presets(library)]
            out = out or [("", "(no preset in this library)", "")]
        _items[("preset", kind)] = out
        return out
    return items


class Stecyk3DPresetChoice(bpy.types.PropertyGroup):
    """Our own scene settings: the library and preset chosen, and the components commands' arguments."""
    material_library: bpy.props.EnumProperty(name="Material library", items=_library_items("materials"))
    material_preset: bpy.props.EnumProperty(name="Material preset", items=_preset_items("materials"))
    texture_library: bpy.props.EnumProperty(name="Texture library", items=_library_items("textures"))
    texture_preset: bpy.props.EnumProperty(name="Texture preset", items=_preset_items("textures"))
    copy_from: bpy.props.PointerProperty(name="From material", type=bpy.types.Material)
    copy_letter: bpy.props.EnumProperty(name="Component", items=[(c, c, "") for c in "ABCD"])
    picture: bpy.props.PointerProperty(name="Picture", type=bpy.types.Image)


def choice(context):
    return context.scene.stecyk3d_presets


def _chosen(context, kind):
    c = choice(context)
    key = getattr(c, kind[:-1] + "_library")
    index = getattr(c, kind[:-1] + "_preset")
    library, reason = load(context, key)
    if key and library is None:
        raise components.Refused("the library cannot be read: %s" % reason)
    return library, (int(index) if index not in ("", None) else None)


class STECYK3D_OT_apply_material_preset(bpy.types.Operator):
    """Make the material shown a Bryce material from the preset chosen """
    bl_idname = "stecyk3d.apply_material_preset"
    bl_label = "Bryce material preset"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        try:
            library, index = _chosen(context, "materials")
            value = components.apply_material_preset(library, index)
        except components.Refused as refused:
            self.report({"ERROR"}, str(refused))
            return {"CANCELLED"}
        offered_name = presets.offered_name(library["presets"][index]["name"], index)
        material = context.material
        name, made = components.preset_target(material.name if material else None, offered_name)
        if made:
            material = bpy.data.materials.new(name)
            obj = context.active_object
            if obj is not None and obj.type == "MESH":
                obj.data.materials.append(material)
                obj.active_material_index = len(obj.data.materials) - 1
        material_nodes.write_material(material, value, make_images(_used_pictures(library, value)))
        return {"FINISHED"}


def _used_pictures(library, value):
    """: the library's pictures that the material's picture components name."""
    named = {c["image"] for c in value["components"] if c["kind"] == "picture"}
    return [p for p in library.get("pictures", []) if p["name"] in named]


def _component_command(operator, context, change, pictures=()):
    material = context.material
    if material is None:
        operator.report({"ERROR"}, "no material is shown")
        return {"CANCELLED"}
    before = material_nodes.components_value(material)
    try:
        after = change(before)
    except components.Refused as refused:
        operator.report({"ERROR"}, str(refused))
        return {"CANCELLED"}
    tree = material_nodes.tree_of(material)
    images = make_images(pictures) if pictures else None
    old = set(components.columns_of(before))
    new = set(components.columns_of(after))
    for column in old - new:
        material_nodes.remove_component_nodes(tree, column)
    for column in sorted(new - old):
        comp = components.find(after, column)
        material_nodes.add_component_nodes(tree, comp, images, (-600.0, -300.0 * column))
    material["stecyk3d_shown"] = -1 if after["shown"] is None else int(after["shown"])
    return {"FINISHED"}


class STECYK3D_OT_add_texture_preset(bpy.types.Operator):
    """Add the texture preset chosen as a new texture component """
    bl_idname = "stecyk3d.add_texture_preset"
    bl_label = "Texture preset"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        def change(value):
            library, index = _chosen(context, "textures")
            return components.add_component(value, components.texture_preset_component(library, index))
        return _component_command(self, context, change)


class STECYK3D_OT_copy_component(bpy.types.Operator):
    """Add a copy of another material's texture component """
    bl_idname = "stecyk3d.copy_component"
    bl_label = "Add texture from a material"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        c = choice(context)
        source = material_nodes.components_value(c.copy_from) if c.copy_from is not None else None
        return _component_command(self, context, lambda v: components.copy_component(v, source, c.copy_letter))


class STECYK3D_OT_add_picture(bpy.types.Operator):
    """Add a picture texture component of the image chosen """
    bl_idname = "stecyk3d.add_picture"
    bl_label = "Add picture texture"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        image = choice(context).picture
        return _component_command(self, context,
                                  lambda v: components.add_picture(v, image.name if image else None))


class STECYK3D_OT_show_component(bpy.types.Operator):
    """Show a texture component: its node becomes the active one, selected """
    bl_idname = "stecyk3d.show_component"
    bl_label = "Show"
    letter: bpy.props.StringProperty()

    def execute(self, context):
        material = context.material
        result = _component_command(self, context, lambda v: components.show(v, self.letter))
        if result == {"FINISHED"}:
            node = material_nodes.texture_nodes(material.node_tree).get(components.column_of(self.letter))
            for n in material.node_tree.nodes:
                n.select = False
            node.select = True
            material.node_tree.nodes.active = node
        return result


class STECYK3D_OT_remove_component(bpy.types.Operator):
    """Remove the texture component shown, the others keeping their letters """
    bl_idname = "stecyk3d.remove_component"
    bl_label = "Remove texture"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        return _component_command(self, context, components.remove_shown)


_first_shown = {}  # (material name, letter) -> the component as the panel first showed it in this Blender session


def remember_first_shown(material, comp):
    """: the settings a component had when the panel first showed it in this session."""
    import copy
    _first_shown.setdefault((material.name, comp["column"]), copy.deepcopy(comp))


def _replace_command(operator, context, make):
    """Replaces the shown component's nodes by those of `make(component)` (Reset, Randomize)."""
    material = context.material
    if material is None:
        operator.report({"ERROR"}, "no material is shown")
        return {"CANCELLED"}
    before = material_nodes.components_value(material)
    column = components.shown_column(before)
    comp = components.find(before, column) if column is not None else None
    if comp is None:
        operator.report({"ERROR"}, "the material has no texture component to change")
        return {"CANCELLED"}
    remember_first_shown(material, comp)
    new = make(material, comp)
    if new is None:
        operator.report({"ERROR"}, "no first state of the component was kept in this session")
        return {"CANCELLED"}
    tree = material_nodes.tree_of(material)
    material_nodes.remove_component_nodes(tree, column)
    material_nodes.add_component_nodes(tree, new, None, (-600.0, -300.0 * column))
    return {"FINISHED"}


class STECYK3D_OT_reset_component(bpy.types.Operator):
    """Give the component shown the settings it had when the panel first showed it in this session """
    bl_idname = "stecyk3d.reset_component"
    bl_label = "Reset component"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        return _replace_command(self, context, lambda m, comp: _first_shown.get((m.name, comp["column"])))


class STECYK3D_OT_randomize_component(bpy.types.Operator):
    """Give the component shown settings drawn at random (which settings is the add-on's design)"""
    bl_idname = "stecyk3d.randomize_component"
    bl_label = "Randomize component"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        import random
        return _replace_command(self, context, lambda m, comp: components.randomize_component(comp, random))


class STECYK3D_OT_add_mapping(bpy.types.Operator):
    """Give the texture component shown a mapping node """
    bl_idname = "stecyk3d.add_mapping"
    bl_label = "Add a Bryce Mapping"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        material = context.material
        if material is None:
            self.report({"ERROR"}, "no material is shown")
            return {"CANCELLED"}
        value = material_nodes.components_value(material)
        try:
            after = components.add_mapping(value)
        except components.Refused as refused:
            self.report({"ERROR"}, str(refused))
            return {"CANCELLED"}
        column = components.shown_column(value)
        node = material_nodes.texture_nodes(material.node_tree)[column]
        if not node.inputs["vector"].is_linked:
            mapping = material_nodes._mapping_node(material.node_tree, components.find(after, column)["mapping"],
                                                   (node.location[0] - material_nodes.COLUMN_GAP,
                                                    node.location[1] + 120.0))
            material.node_tree.links.new(mapping.outputs["vector"], node.inputs["vector"])
        return {"FINISHED"}


LEAST_TEXTURE = {
    "outputs": 1, "blend": [0, 0], "color_space": 0,
    "colors": {"mode": 0, "space": 0, "first": 0, "second": 0, "third": 0},
    "filter": {"kind": 0, "n": 0, "a": 0.0, "b": 0.0},
    "phase": {"amount": 0.0, "noise": {"kind": 0, "method": 0, "count": 0, "dims": 1}, "rotation": [0.0, 0.0],
              "frequency": [0.0, 0.0, 0.0]},
    "components": [{"flags": 1, "space": 0, "noise": {"kind": 0, "method": 0, "count": 1, "dims": 3},
                    "rotation": [0.0, 0.0], "frequency": [1.0, 1.0, 1.0],
                    "phase": {"amount": 0.0, "noise": {"kind": 0, "method": 0, "count": 0, "dims": 1},
                              "rotation": [0.0, 0.0], "frequency": [0.0, 0.0, 0.0]},
                    "filter": {"kind": 0, "n": 0, "a": 0.0, "b": 0.0},
                    "colors": {"mode": 0, "space": 0, "first": 0, "second": 0, "third": 0}}],
}


class STECYK3D_OT_new_procedural_texture(bpy.types.Operator):
    """Add a noise, a component and a texture node, linked, at the cursor """
    bl_idname = "stecyk3d.new_procedural_texture"
    bl_label = "New texture (Noise, Component, Texture)"
    bl_options = {"REGISTER", "UNDO"}

    @classmethod
    def poll(cls, context):
        space = context.space_data
        return space is not None and getattr(space, "edit_tree", None) is not None

    def execute(self, context):
        tree = context.space_data.edit_tree
        value = components.empty_material()
        for column, node in material_nodes.texture_nodes(tree).items():
            value["components"].append({"column": column})
        column = components.first_free_column(value)
        column = column if column is not None else max(components.columns_of(value)) + 1
        comp = {"column": column, "kind": "procedural", "name": "", "record": LEAST_TEXTURE, "mapping": None}
        cursor = context.space_data.cursor_location
        material_nodes.add_component_nodes(tree, comp, None, (cursor[0], cursor[1]))
        return {"FINISHED"}


def draw_preset_box(layout, context, kind):
    """The library and preset choice of one kind ("materials" or "textures") and its apply button; or why none is offered."""
    c = choice(context)
    op = STECYK3D_OT_apply_material_preset if kind == "materials" else STECYK3D_OT_add_texture_preset
    box = layout.box()
    libs, folder_set = offered(context, kind)
    if not libs:
        box.label(text=presets.none_offered_message(kind, folder_set), icon="PRESET")  # Errors
        return
    box.use_property_split = True
    box.prop(c, kind[:-1] + "_library")
    box.prop(c, kind[:-1] + "_preset")
    box.operator(op.bl_idname, icon="PRESET")


class STECYK3D_PT_presets(bpy.types.Panel):
    """The Material tab's "Texture components": the texture preset, the component letters and their buttons (the
    material preset is at the top of the Bryce material panel above)."""
    bl_label = "Texture components"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_context = "material"
    bl_parent_id = "STECYK3D_PT_material"
    COMPAT_ENGINES = {names.ENGINE_ID}

    @classmethod
    def poll(cls, context):
        return context.engine == names.ENGINE_ID and context.material is not None

    def draw(self, context):
        layout = self.layout
        c = choice(context)
        material = context.material
        value = material_nodes.components_value(material)
        shown = components.shown_column(value)
        row = layout.row(align=True)
        for comp in value["components"]:
            letter = components.letter(comp["column"])
            op = row.operator(STECYK3D_OT_show_component.bl_idname, text=letter, depress=comp["column"] == shown)
            op.letter = letter
        row = layout.row(align=True)
        row.operator(STECYK3D_OT_copy_component.bl_idname, icon="ADD")
        row.operator(STECYK3D_OT_add_picture.bl_idname, icon="IMAGE_DATA")
        row.operator(STECYK3D_OT_remove_component.bl_idname, icon="REMOVE")
        shown_comp = components.find(value, shown)
        if shown_comp is not None:
            remember_first_shown(material, shown_comp)  # : Reset's state, kept when first shown
            layout.label(text="Texture %s: %s" % (components.letter(shown_comp["column"]),
                                                  shown_comp["name"] or shown_comp["kind"]))
            from . import panels
            tree = material.node_tree
            shown_node = material_nodes.texture_nodes(tree).get(shown_comp["column"]) if tree else None
            if shown_node is not None:
                panels.draw_texture_parts(layout, context, shown_node)  # the texture editor, as the old UI shows it
        col = layout.column()
        col.use_property_split = True
        col.prop(c, "copy_from")
        col.prop(c, "copy_letter")
        col.prop(c, "picture")
        row = layout.row(align=True)
        row.operator(STECYK3D_OT_add_mapping.bl_idname, icon="ORIENTATION_GLOBAL")
        row.operator(STECYK3D_OT_reset_component.bl_idname, icon="LOOP_BACK")
        row.operator(STECYK3D_OT_randomize_component.bl_idname, icon="FILE_REFRESH")
        draw_preset_box(layout, context, "textures")


CLASSES = [Stecyk3DPresetChoice, STECYK3D_OT_apply_material_preset, STECYK3D_OT_add_texture_preset,
           STECYK3D_OT_copy_component, STECYK3D_OT_add_picture, STECYK3D_OT_show_component,
           STECYK3D_OT_remove_component, STECYK3D_OT_reset_component, STECYK3D_OT_randomize_component,
           STECYK3D_OT_add_mapping, STECYK3D_OT_new_procedural_texture,
           STECYK3D_PT_presets]


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)
    bpy.types.Scene.stecyk3d_presets = bpy.props.PointerProperty(type=Stecyk3DPresetChoice)


def unregister():
    del bpy.types.Scene.stecyk3d_presets
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
