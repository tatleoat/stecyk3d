# SPDX-License-Identifier: GPL-3.0-or-later
# (Add > Bryce; task T5 of NODE-PARITY-PLAN.md)
"""Add > Bryce in the Shader Editor: every Bryce node, in groups, added with Blender's own add-node operator (so the
node sticks to the pointer), and the "New procedural texture" button. Shown for our engine and a shader tree only."""

import bpy

from . import names

# (group title, [(idname, menu text)]) ; the labels follow the nodes' own
GROUPS = (
    ("Material", (("BryceMaterialNode", "Material"), ("BryceMixMaterialsNode", "Mix Materials"))),
    ("Texture", (("BryceTextureNode", "Texture"), ("BrycePictureNode", "Picture"), ("BryceMappingNode", "Mapping"))),
    ("Texture parts", (("BryceNoiseNode", "Noise"), ("BryceFilterNode", "Filter"), ("BryceComponentNode", "Component"),
                       ("BryceBlendNode", "Blend"))),
    ("Mix", (("BryceMixColorNode", "Mix Colour"), ("BryceMixValueNode", "Mix Value"))),
    ("World and light", (("BryceSkyNode", "Sky"), ("BryceLightNode", "Light"))),
)


def shown(context):
    """Our engine is rendering and the editor holds a shader tree."""
    space = getattr(context, "space_data", None)
    return context.engine == names.ENGINE_ID and getattr(space, "tree_type", "ShaderNodeTree") == "ShaderNodeTree"


class NODE_MT_bryce_add(bpy.types.Menu):
    bl_idname = "NODE_MT_bryce_add"
    bl_label = "Bryce"

    def draw(self, context):
        layout = self.layout
        for k, (_title, rows) in enumerate(GROUPS):
            if k:
                layout.separator()
            for idname, text in rows:
                op = layout.operator("node.add_node", text=text)
                op.type = idname
                op.use_transform = True
        layout.separator()
        layout.operator("stecyk3d.new_procedural_texture", icon="TEXTURE")


def node_add_menu(self, context):
    if shown(context):
        self.layout.menu(NODE_MT_bryce_add.bl_idname)


def register():
    bpy.utils.register_class(NODE_MT_bryce_add)
    bpy.types.NODE_MT_add.append(node_add_menu)


def unregister():
    bpy.types.NODE_MT_add.remove(node_add_menu)
    bpy.utils.unregister_class(NODE_MT_bryce_add)
