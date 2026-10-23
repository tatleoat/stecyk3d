# SPDX-License-Identifier: GPL-3.0-or-later
# , Errors (the refusals shown by T7)
# (Node drawing; tasks T4 and T7 of NODE-PARITY-PLAN.md)
"""How the Bryce nodes draw themselves: grouped layouts, the image template on a Picture node, notes for a program
that does not read a setting, the N-panel's extended view, and the node's own faults. Draws only through the
`layout` it is given and attribute reads, so a test may give it a recording stand-in."""

import textwrap

import bpy

from . import components, names, node_menus, texture_nodes

HIDDEN = ("exact", "mix", "converted", "tex")  # : text in the earlier work's form / leftovers, kept, never shown
FAULT_COLOUR = (0.55, 0.12, 0.12)
WRAP = 38


def draw_settings(layout, owner, annotations, skip=()):
    """Every setting of `owner` in table order: a number that has a menu (`<name>_menu`) is drawn as that menu, a
    group of settings (filter, colour map) in a box of its own, the named switches together in a box."""
    properties = owner.bl_rna.properties
    store = getattr(owner, "SWITCH_STORE", None)
    switches = [name for name in annotations if name.startswith("switch_")]
    for name in annotations:
        if name in HIDDEN or name in skip or name + "_menu" in annotations or name == store or name in switches:
            continue  # the menu of the same number stands in its place; named switches are drawn together below
        prop = properties[name]
        if prop.type == "POINTER" and isinstance(getattr(owner, name), bpy.types.PropertyGroup):
            box = layout.box()
            box.label(text=prop.name if prop.name != name else name.capitalize())
            group = getattr(owner, name)
            draw_settings(box, group, type(group).__annotations__)
        else:
            layout.prop(owner, name, expand=prop.type == "ENUM" and prop.is_enum_flag)
    draw_switches(layout, owner, switches)


def draw_switches(layout, owner, switches, program=None):
    if not switches:
        return
    box = layout.box()
    box.label(text="Switches")
    for name in switches:
        box.prop(owner, name)
    for note in program_notes(owner, program):
        box.label(text=note, icon="INFO")


def _version(text):
    return tuple(int(x) for x in str(text).split(".") if x.isdigit())


def program_of(context):
    """The program the scene renders as ("3.0", "3.1", "5.0"), or None when it cannot be read (no scene)."""
    try:
        return getattr(context.scene, names.SCENE_RENDER_GROUP).program
    except AttributeError:
        return None


def program_notes(owner, program):
    """Notes for settings that are on but that the chosen 3.x program does not read (T4)."""
    if program is None:
        return []
    out = []
    kind = getattr(owner, "kind", None)
    if owner.bl_idname == "BryceNoiseNode" and kind is not None and int(kind) >= node_menus.FIVE_FIRST \
            and _version(program) < (5, 0):
        out.append("Noise types from 27 are Bryce 5 only")
    if owner.bl_idname == "BryceMappingNode":
        for bit, label, _description, first in names.MAPPING_SWITCHES:
            if isinstance(first, str) and _version(program) < _version(first) and int(owner.flags) & bit:
                out.append("%s: read from Bryce %s" % (label, first))
    return out


# ---------------------------------------------------------------- faults (T7)

def owner_of(node):
    """The material or world whose node tree holds `node` (a tree's id_data is the tree), else the tree."""
    tree = node.id_data
    for collection in (bpy.data.materials, bpy.data.worlds):
        for owner in collection:
            if owner.node_tree is not None and owner.node_tree.as_pointer() == tree.as_pointer():
                return owner
    return tree


def what_of(node):
    owner = owner_of(node)
    if isinstance(owner, bpy.types.Material):
        return "material %r" % owner.name
    return "%s %r" % ("world" if isinstance(owner, bpy.types.World) else "node tree", owner.name)


def faults(node):
    """[message] shown on a Texture or Picture node: what the render would refuse it for (texture_nodes.faults_of,
    the same text) and a column shared with another texture of the tree (channels.column_clash) or past
    the fourth letter ."""
    if node.bl_idname not in ("BryceTextureNode", "BrycePictureNode"):
        return []
    from . import channels
    what = what_of(node)
    out = texture_nodes.faults_of(node, what)
    columns = {}
    for other in node.id_data.nodes:  # in node order: the first of a column is the one the render keeps
        if other.bl_idname in ("BryceTextureNode", "BrycePictureNode"):
            clash = channels.column_clash(owner_of(node).name, columns, int(other.column), other.name)
            if clash and other.name == node.name:
                out.append(clash)
    if int(node.column) >= components.MATERIAL_LETTERS:
        out.append("%s: %r is column %d, a material has the letters A to D only" % (what, node.name, node.column))
    return out


def free_column(node):
    """T6: the first letter (0 to 3, A to D) no other Texture or Picture node of `node`'s tree has, or None when
    the four are taken (the node then keeps its column and the fault is shown on it, T7)."""
    taken = {int(o.column) for o in node.id_data.nodes
             if o.bl_idname in ("BryceTextureNode", "BrycePictureNode") and o.as_pointer() != node.as_pointer()}
    for column in range(components.MATERIAL_LETTERS):
        if column not in taken:
            return column
    return None


def take_free_column(node):
    """A new or copied texture node takes a free column when the one it has is already another node's."""
    column = free_column(node)
    others = {int(o.column) for o in node.id_data.nodes
              if o.bl_idname in ("BryceTextureNode", "BrycePictureNode") and o.as_pointer() != node.as_pointer()}
    if column is not None and int(node.column) in others:
        node.column = column


def draw_faults(layout, node):
    for message in faults(node):
        box = layout.box()
        for i, line in enumerate(textwrap.wrap(message, WRAP) or [message]):
            box.label(text=line, icon="ERROR" if i == 0 else "BLANK1")


def refresh_colour(node):
    """Mark a node red while it has a fault (a node's colour cannot be set while drawing; called from update())."""
    try:
        bad = bool(faults(node))
    except (AttributeError, RuntimeError):
        return
    if bad != node.use_custom_color or (bad and [round(x, 4) for x in node.color] != list(FAULT_COLOUR)):
        node.use_custom_color = bad
        if bad:
            node.color = FAULT_COLOUR


def refresh_downstream(node):
    """refresh_colour for `node` and for every Texture or Picture node its outputs feed (a link edited in the
    chain changes whether the texture it ends in is refused; Blender calls update() on the edited nodes only)."""
    seen, todo = set(), [node]
    while todo:
        current = todo.pop()
        if current.name in seen:
            continue
        seen.add(current.name)
        refresh_colour(current)
        for socket in current.outputs:
            todo.extend(link.to_node for link in socket.links if link.to_node.bl_idname in names.NODE_TYPES)


# ---------------------------------------------------------------- layouts

def _props(layout, owner, *props):
    for name in props:
        layout.prop(owner, name)


def _group(layout, owner, title, name):
    box = layout.box()
    box.label(text=title)
    group = getattr(owner, name)
    draw_settings(box, group, type(group).__annotations__)


def label_of(node):
    if node.bl_idname in ("BryceTextureNode", "BrycePictureNode"):
        kind = "Picture" if node.bl_idname == "BrycePictureNode" else "Texture"
        return "%s %s" % (kind, names.component_letter(int(node.column)))
    kind, number = {"BryceNoiseNode": (node_menus.NOISE, "kind"), "BryceFilterNode": (node_menus.FILTER, "kind"),
                    "BryceBlendNode": (node_menus.BLEND, "mode")}.get(node.bl_idname, (None, None))
    if kind is not None and (kind != node_menus.FILTER or node.kind):  # a Filter of kind 0 keeps its plain title
        return "%s: %s" % (node.bl_label, node_menus.label_in(node_menus.current_tables(), kind, getattr(node, number)))
    return None


def _material(layout, node, program, extended):
    switches = [n for n in node.__annotations__ if n.startswith("switch_")]
    draw_switches(layout, node, switches, program)
    box = layout.box()
    box.label(text="Volume")
    _props(box, node, "volume_color", "refraction", "volume_edge", "volume_sharpness", "volume_quality")
    if extended:
        layout.prop(node, "flags")


def _texture(layout, node, program, extended):
    _props(layout, node, "column", "gives_menu", "color_space_menu")
    _group(layout, node, "Filter", "filter")
    _group(layout, node, "Colour map", "colors")
    if extended:
        layout.prop(node, "with_offsets")
        layout.prop(node, "gives")
        layout.prop(node, "color_space")
    draw_faults(layout, node)


def _picture(layout, node, program, extended):
    layout.template_ID(node, "image", open="image.open")
    layout.prop(node, "column")
    if extended and node.image is not None:
        layout.label(text="%d x %d pixels" % tuple(node.image.size))
    draw_faults(layout, node)


def _mapping(layout, node, program, extended):
    _props(layout, node, "mode_menu", "scale", "offset", "rotation")
    draw_switches(layout, node, [n for n in node.__annotations__ if n.startswith("switch_")], program)
    if extended:
        layout.prop(node, "mode")
        layout.prop(node, "flags")


def _noise(layout, node, program, extended):
    _props(layout, node, "kind_menu", "method_menu", "count", "dims", "custom")
    for note in program_notes(node, program):
        layout.label(text=note, icon="INFO")
    if node.custom or extended:
        layout.prop(node, "matrix")
    if not node.custom or extended:
        _props(layout, node, "rotation", "frequency")
    if extended:
        _props(layout, node, "kind", "method")


def _filter(layout, node, program, extended):
    _props(layout, node, "kind_menu", "n", "a", "b")


def _component(layout, node, program, extended):
    _props(layout, node, "flags_menu", "space_menu", "mode_menu")
    box = layout.box()
    box.label(text="Colour map")
    _props(box, node, "first", "second", "third")
    layout.prop(node, "bump_switch")


def _blend(layout, node, program, extended):
    layout.prop(node, "mode_menu")


def _mix_materials(layout, node, program, extended):
    layout.prop(node, "amount")


def _mix(layout, node, program, extended):
    layout.prop(node, "by")


LAYOUTS = {"BryceMaterialNode": _material, "BryceTextureNode": _texture, "BrycePictureNode": _picture,
           "BryceMappingNode": _mapping, "BryceNoiseNode": _noise, "BryceFilterNode": _filter,
           "BryceComponentNode": _component, "BryceBlendNode": _blend, "BryceMixMaterialsNode": _mix_materials,
           "BryceMixColorNode": _mix, "BryceMixValueNode": _mix}


def draw(node, context, layout, extended=False):
    """The node body (extended: the N-panel's "Node" tab, which also shows the stored numbers under the menus)."""
    layout_of = LAYOUTS.get(node.bl_idname)
    if layout_of is None:
        draw_settings(layout, node, node.__annotations__)
        return
    layout_of(layout, node, program_of(context), extended)
