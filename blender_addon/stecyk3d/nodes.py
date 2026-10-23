# SPDX-License-Identifier: GPL-3.0-or-later
# (Node and socket types; needs Blender, not testable here)
"""The Bryce node and socket types, by's identifiers and's settings and sockets."""

import bpy

from . import defaults, names, node_menus, node_ui, props
from .node_menus import BLEND, COLOUR_MODE, FILTER, METHOD, NOISE

SHADER = "NodeSocketShader"
COLOUR_OUT = "NodeSocketColor"
VALUE_OUT = "NodeSocketFloat"
VECTOR = "NodeSocketVector"


# ---------------------------------------------------------------- sockets ("Sockets")

class BryceColorSocket(bpy.types.NodeSocket):
    bl_idname = "BryceColorSocket"
    bl_label = "Bryce colour"
    value: bpy.props.FloatVectorProperty(name="value", size=3, subtype="COLOR_GAMMA")

    def draw(self, context, layout, node, text):
        if self.is_output or self.is_linked:
            layout.label(text=text)
        else:
            layout.prop(self, "value", text=text)

    def draw_color(self, context, node):
        return (0.78, 0.78, 0.16, 1.0)


class BryceValueSocket(bpy.types.NodeSocket):
    bl_idname = "BryceValueSocket"
    bl_label = "Bryce value"
    value: bpy.props.FloatProperty(name="value")

    def draw(self, context, layout, node, text):
        if self.is_output or self.is_linked:
            layout.label(text=text)
        else:
            layout.prop(self, "value", text=text)

    def draw_color(self, context, node):
        return (0.63, 0.63, 0.63, 1.0)


class BryceSignalSocket(bpy.types.NodeSocket):
    bl_idname = "BryceSignalSocket"
    bl_label = "Bryce signal"

    def draw(self, context, layout, node, text):
        layout.label(text=text)

    def draw_color(self, context, node):
        return (0.35, 0.55, 0.9, 1.0)


class BryceComponentSocket(bpy.types.NodeSocket):
    bl_idname = "BryceComponentSocket"
    bl_label = "Bryce component"

    def draw(self, context, layout, node, text):
        layout.label(text=text)

    def draw_color(self, context, node):
        return (0.85, 0.45, 0.2, 1.0)


# ---------------------------------------------------------------- nodes (table)

class _BryceNode:
    """Shared behaviour: inputs and outputs from the class's tables, made by identifier."""
    INPUTS = ()
    OUTPUTS = ()

    @classmethod
    def poll(cls, ntree):
        return ntree.bl_idname == "ShaderNodeTree"

    def init(self, context):
        for identifier, socket in self.INPUTS:
            self.inputs.new(socket, identifier, identifier=identifier)
        for identifier, socket in self.OUTPUTS:
            self.outputs.new(socket, identifier, identifier=identifier)

    def draw_buttons(self, context, layout):
        node_ui.draw(self, context, layout)

    def draw_buttons_ext(self, context, layout):
        node_ui.draw(self, context, layout, extended=True)

    def draw_label(self):
        return node_ui.label_of(self) or self.bl_label

    def update(self):
        node_ui.refresh_downstream(self)  # T7: a node the render would refuse turns red


def switch_name(bit):
    return "switch_%X" % bit


def add_switches(cls, store, rows):
    """T3: one BoolProperty per named switch of the word `store` (`switch_2`, `switch_8`, ...). Each writes only its
    own bit (inverted ones: on while the bit is clear); `store` itself is unchanged, so are the files that hold it."""
    cls.SWITCH_STORE = store
    for bit, label, description, flag in rows:
        if isinstance(flag, bool):  # a material switch row: (bit, label, description, inverted)
            inverted, note = flag, ""
        else:  # a mapping switch row: (bit, label, description, the program that first reads it)
            inverted, note = False, " (as %s and later)" % flag if flag != "3.0" else ""
        cls.__annotations__[switch_name(bit)] = node_menus.bit_property(store, bit, label, description + note, inverted)


TEXTURE_OUTPUTS = (("color", COLOUR_OUT), ("alpha", VALUE_OUT), ("bump", VECTOR))


def _word():
    return bpy.props.IntProperty()


def _node_classes():
    P = bpy.props
    C = props.CLASSES

    class BryceMaterialNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceMaterialNode"
        bl_label = "Bryce Material"
        bl_width_default = 260  # the earlier add-on's preset width
        INPUTS = tuple((c, "BryceColorSocket") for c in names.COLOUR_CHANNELS) + \
            tuple((c, "BryceValueSocket") for c in names.VALUE_CHANNELS)
        OUTPUTS = (("material", SHADER),)
        flags: _word()
        volume_color: P.FloatVectorProperty(size=3, subtype="COLOR_GAMMA")
        refraction: P.FloatProperty()
        volume_edge: P.FloatProperty()
        volume_sharpness: P.FloatProperty()
        volume_quality: P.FloatProperty()
        exact: P.StringProperty()
        mix: P.StringProperty()
        converted: P.StringProperty()

        def init(self, context):
            """; defaults.py"""
            _BryceNode.init(self, context)
            new = defaults.NEW_MATERIAL
            self.flags = new["flags"]
            self.volume_color = new["volume_color"]
            self.refraction = new["refraction"]
            self.volume_edge = new["volume_edge"]
            self.volume_sharpness = new["volume_sharpness"]
            self.volume_quality = new["volume_quality"]
            for name, value in {**new["colours"], **new["values"]}.items():
                self.inputs[name].value = value

    class BryceMixMaterialsNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceMixMaterialsNode"
        bl_label = "Bryce Mix Materials"
        INPUTS = (("first", SHADER), ("second", SHADER))
        OUTPUTS = (("material", SHADER),)
        amount: P.FloatProperty(soft_min=0.0, soft_max=1.0)

    class BryceMixColorNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceMixColorNode"
        bl_label = "Bryce Mix Colour"
        bl_width_default = 180  # the earlier add-on's preset width
        INPUTS = (("a", "BryceColorSocket"), ("b", "BryceColorSocket"), ("alpha", "BryceValueSocket"))
        OUTPUTS = (("out", COLOUR_OUT),)
        by: P.EnumProperty(items=[(k, k, "", v) for k, v in names.MIX_BY.items()])

    class BryceMixValueNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceMixValueNode"
        bl_label = "Bryce Mix Value"
        bl_width_default = 180  # the earlier add-on's preset width
        INPUTS = (("a", "BryceValueSocket"), ("b", "BryceValueSocket"), ("alpha", "BryceValueSocket"))
        OUTPUTS = (("out", VALUE_OUT),)
        by: P.EnumProperty(items=[(k, k, "", v) for k, v in names.MIX_BY.items()])

    class _ColumnNode(_BryceNode):
        """T6: Texture and Picture nodes: a new or pasted node takes a free column (A to D) instead of repeating
        another's; with all four taken it keeps its own and shows the fault."""

        def init(self, context):
            _BryceNode.init(self, context)
            node_ui.take_free_column(self)

        def copy(self, node):
            node_ui.take_free_column(self)

    class BryceTextureNode(_ColumnNode, bpy.types.Node):
        bl_idname = "BryceTextureNode"
        bl_label = "Bryce Texture"
        bl_width_default = 240  # the earlier add-on's preset width
        INPUTS = (("component", "BryceComponentSocket"), ("phase", "BryceValueSocket"), ("vector", VECTOR))
        OUTPUTS = TEXTURE_OUTPUTS
        gives: _word()
        gives_menu: node_menus.flag_menu("gives", names.OUTPUT_BITS, "Gives")
        color_space: _word()
        color_space_menu: node_menus.int_menu("color_space", "space", "Colour space")
        filter: P.PointerProperty(type=C["filter"])
        colors: P.PointerProperty(type=C["colour_map"])
        with_offsets: P.BoolProperty()
        column: P.IntProperty()
        exact: P.StringProperty()
        tex: P.PointerProperty(type=C["texture"])

    class BrycePictureNode(_ColumnNode, bpy.types.Node):
        bl_idname = "BrycePictureNode"
        bl_label = "Bryce Picture"
        bl_width_default = 220  # the earlier add-on's preset width
        INPUTS = (("vector", VECTOR),)
        OUTPUTS = TEXTURE_OUTPUTS
        image: P.PointerProperty(type=bpy.types.Image)
        column: P.IntProperty()
        tex: P.PointerProperty(type=C["texture"])

    class BryceMappingNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceMappingNode"
        bl_label = "Bryce Mapping"
        bl_width_default = 200  # the earlier add-on's preset width
        OUTPUTS = (("vector", VECTOR),)
        mode: P.IntProperty(default=defaults.NEW_MAPPING["mode"])
        mode_menu: node_menus.int_menu("mode", "mapping", "Mode")
        flags: _word()
        scale: P.FloatVectorProperty(size=3, default=defaults.NEW_MAPPING["scale"])
        offset: P.FloatVectorProperty(size=3)
        rotation: P.FloatVectorProperty(size=4, default=defaults.NEW_MAPPING["rotation"])
        exact: P.StringProperty()

    class BryceNoiseNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceNoiseNode"
        bl_label = "Bryce Noise"
        bl_width_default = 210  # the earlier add-on's preset width
        INPUTS = (("phase", "BryceValueSocket"),)
        OUTPUTS = (("value", "BryceSignalSocket"),)
        kind: _word()
        kind_menu: node_menus.int_menu("kind", NOISE, "Type")
        method: _word()
        method_menu: node_menus.int_menu("method", METHOD, "Method")
        count: P.IntProperty()
        dims: P.IntProperty()
        rotation: P.FloatVectorProperty(size=2)
        frequency: P.FloatVectorProperty(size=3)
        custom: P.BoolProperty()
        matrix: P.FloatVectorProperty(size=9)
        exact: P.StringProperty()

    class BryceFilterNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceFilterNode"
        bl_label = "Bryce Filter"
        bl_width_default = 200  # the earlier add-on's preset width
        INPUTS = (("value", "BryceSignalSocket"),)
        OUTPUTS = (("value", "BryceSignalSocket"),)
        kind: _word()
        kind_menu: node_menus.int_menu("kind", FILTER, "Filter")
        n: P.IntProperty()
        a: P.FloatProperty()
        b: P.FloatProperty()

    class BryceComponentNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceComponentNode"
        bl_label = "Bryce Component"
        bl_width_default = 220  # the earlier add-on's preset width
        INPUTS = (("value", "BryceSignalSocket"),)
        OUTPUTS = (("component", "BryceComponentSocket"),)
        flags: _word()
        flags_menu: node_menus.flag_menu("flags", names.OUTPUT_BITS, "Gives")
        space: _word()
        space_menu: node_menus.int_menu("space", "space", "Colour space")
        mode: _word()
        mode_menu: node_menus.int_menu("mode", COLOUR_MODE, "Colour mode")
        first: _word()
        second: _word()
        third: _word()
        bump_switch: P.EnumProperty(items=[(k, k, "", v) for k, v in names.BUMP_SWITCH.items()])

    class BryceBlendNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceBlendNode"
        bl_label = "Bryce Blend"
        bl_width_default = 170  # the earlier add-on's preset width
        INPUTS = (("first", "BryceComponentSocket"), ("second", "BryceComponentSocket"))
        OUTPUTS = (("component", "BryceComponentSocket"),)
        mode: _word()
        mode_menu: node_menus.int_menu("mode", BLEND, "Blend")

    class BryceSkyNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceSkyNode"
        bl_label = "Bryce Sky"
        bl_width_default = 240  # the earlier add-on's preset width
        INPUTS = (("stratus", COLOUR_OUT), ("cumulus", COLOUR_OUT))
        OUTPUTS = (("sky", SHADER),)
        settings: P.PointerProperty(type=C["sky"])

        def draw_buttons(self, context, layout):
            layout.label(text="Settings in World properties")

        draw_buttons_ext = draw_buttons

    class BryceLightNode(_BryceNode, bpy.types.Node):
        bl_idname = "BryceLightNode"
        bl_label = "Bryce Light"
        bl_width_default = 240  # the earlier add-on's preset width
        INPUTS = (("material", SHADER),)
        OUTPUTS = (("light", SHADER),)
        settings: P.PointerProperty(type=C["light"])

        def draw_buttons(self, context, layout):
            layout.label(text="Settings in Light properties")

        draw_buttons_ext = draw_buttons

    add_switches(BryceMaterialNode, "flags", names.MATERIAL_SWITCHES)
    add_switches(BryceMappingNode, "flags", names.MAPPING_SWITCHES)
    return [BryceMaterialNode, BryceMixMaterialsNode, BryceMixColorNode, BryceMixValueNode, BryceTextureNode,
            BrycePictureNode, BryceMappingNode, BryceNoiseNode, BryceFilterNode, BryceComponentNode,
            BryceBlendNode, BryceSkyNode, BryceLightNode]


SOCKETS = [BryceColorSocket, BryceValueSocket, BryceSignalSocket, BryceComponentSocket]
_registered = []


def register():
    for cls in SOCKETS:
        bpy.utils.register_class(cls)
    for cls in _node_classes():
        bpy.utils.register_class(cls)
        _registered.append(cls)


def unregister():
    while _registered:
        bpy.utils.unregister_class(_registered.pop())
    for cls in reversed(SOCKETS):
        bpy.utils.unregister_class(cls)


draw_settings = node_ui.draw_settings  # kept: the name earlier tests and tools use
