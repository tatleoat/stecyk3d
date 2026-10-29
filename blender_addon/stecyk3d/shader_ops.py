# SPDX-License-Identifier: GPL-3.0-or-later
# (Shader operators: node-parity task T8)
"""Buttons that make a datablock a Bryce one: New Bryce material, Make it a Bryce sky / light, Bryce settings to
nodes, and Bryce material from the Blender shader (the Principled BSDF's table is in principled_map.py)."""

import bpy

from . import gather, material_nodes, names, principled_map

WORLD_OUTPUT = "ShaderNodeOutputWorld"
LIGHT_OUTPUT = "ShaderNodeOutputLight"
PRINCIPLED = "ShaderNodeBsdfPrincipled"


def copy_group(src, dst):
    """Every setting of one Bryce property group into another of the same kind (lists item by item)."""
    for prop in src.bl_rna.properties:
        key = prop.identifier
        if key in ("rna_type", "name") or key.endswith("_menu") or key in names.SWITCH_VIEWS:
            continue
        value = getattr(src, key)
        if prop.type == "POINTER" and isinstance(value, bpy.types.PropertyGroup):
            copy_group(value, getattr(dst, key))
        elif prop.type == "COLLECTION":
            target = getattr(dst, key)
            target.clear()
            for item in value:
                copy_group(item, target.add())
        elif not prop.is_readonly:
            setattr(dst, key, value if not getattr(prop, "is_array", False) else list(value))


def _output(tree, idname):
    out = next((n for n in tree.nodes if n.bl_idname == idname), None)
    if out is None:
        out = tree.nodes.new(idname)
        out.location = (300.0, 0.0)
    return out


def make_sky(world):
    """A Bryce Sky node on the World Output (the Background goes); the world's `bryce` settings move onto it."""
    world.use_nodes = True
    tree = world.node_tree
    if gather.find_node(tree, "BryceSkyNode") is not None:
        return False
    group = getattr(world, names.DATA_GROUP)
    out = _output(tree, WORLD_OUTPUT)
    for node in [n for n in tree.nodes if n.bl_idname == "ShaderNodeBackground"]:
        tree.nodes.remove(node)
    node = tree.nodes.new("BryceSkyNode")
    node.location = (0.0, 0.0)
    copy_group(group, node.settings)
    tree.links.new(node.outputs["sky"], out.inputs["Surface"])
    return True


def make_light(light):
    """A Bryce Light node on the light's Output; the lamp's `bryce` settings move onto it."""
    light.use_nodes = True
    tree = light.node_tree
    if gather.find_node(tree, "BryceLightNode") is not None:
        return False
    group = getattr(light, names.DATA_GROUP)
    out = _output(tree, LIGHT_OUTPUT)
    node = tree.nodes.new("BryceLightNode")
    node.location = (0.0, 0.0)
    copy_group(group, node.settings)
    tree.links.new(node.outputs["light"], out.inputs["Surface"])
    return True


def new_bryce_material(material):
    material_nodes.write_material(material, principled_map.new_material_value())


def value_from_group(group):
    """A material's `bryce` group (older files) as a material value: flags, volume settings and each channel's A.
    Returns (value, notes): what the nodes cannot carry from the group (textures, mixes) is named in notes."""
    value = principled_map.new_material_value()
    value["flags"] = group.flags
    value["volume_color"] = list(group.volume_color)
    for key in ("refraction", "volume_edge", "volume_sharpness", "volume_quality"):
        value[key] = getattr(group, key)
    notes = []
    for name in names.CHANNELS:
        ch = getattr(group, name)
        value["channels"][name]["value"] = list(ch.a) if name in names.COLOUR_CHANNELS else ch.a
        if ch.mode != "A" or ch.src_a != "NONE" or ch.src_b != "NONE" or ch.src_alpha != "NONE":
            notes.append(name)
    return value, notes


def settings_to_nodes():
    """Materials, worlds and lights that hold settings in their `bryce` group get nodes instead; the group is then
    cleared. Returns (materials, worlds, lights, notes)."""
    done, notes = [0, 0, 0], []
    for material in list(bpy.data.materials):
        if not material.is_property_set(names.DATA_GROUP):
            continue
        if gather.find_node(material.node_tree if material.use_nodes else None, "BryceMaterialNode") is not None:
            continue
        value, left = value_from_group(getattr(material, names.DATA_GROUP))
        if getattr(material, names.DATA_GROUP).components:
            left.append("%d texture components" % len(getattr(material, names.DATA_GROUP).components))
        if left:
            notes.append("%s: not carried: %s" % (material.name, ", ".join(left)))
        material_nodes.write_material(material, value)
        material.property_unset(names.DATA_GROUP)
        done[0] += 1
    for world in list(bpy.data.worlds):
        if world.is_property_set(names.DATA_GROUP) and make_sky(world):
            world.property_unset(names.DATA_GROUP)
            done[1] += 1
    for light in list(bpy.data.lights):
        if light.is_property_set(names.DATA_GROUP) and make_light(light):
            light.property_unset(names.DATA_GROUP)
            done[2] += 1
    return done[0], done[1], done[2], notes


# The Principled BSDF's inputs principled_map carries, and the keyword each is passed as (None: positional).
PRINCIPLED_CARRIED = (("Base Color", None), ("Roughness", None), ("Metallic", None), ("Alpha", None),
                      ("Emission Color", None), ("Emission Strength", None), ("Transmission Weight", "transmission"),
                      ("IOR", "ior"), ("Specular IOR Level", "specular_level"), ("Specular Tint", "specular_tint"),
                      ("Thin Wall", "thin_wall"))


def from_principled(material):
    """The Principled BSDF's constants as a Bryce material. Returns (linked, not carried): the carried inputs that
    were linked and the inputs set but with no Bryce counterpart; or None when the material has no Principled
    BSDF. An input this Blender's node does not have takes principled_map's neutral value."""
    import json
    tree = material.node_tree if material.use_nodes else None
    node = next((n for n in tree.nodes if n.bl_idname == PRINCIPLED), None) if tree else None
    if node is None:
        return None
    ins = node.inputs
    present = [(name, key) for name, key in PRINCIPLED_CARRIED if ins.get(name) is not None]
    linked = [name for name, _ in present if ins[name].is_linked]
    args = [ins[name].default_value for name, key in present if key is None]
    keywords = {key: ins[name].default_value for name, key in present if key is not None}
    value = principled_map.material_value(*args, **keywords)
    left = principled_map.not_carried({name: ins[name].default_value for name, _ in principled_map.NO_COUNTERPART
                                       if ins.get(name) is not None})
    bryce = material_nodes.write_material(material, value)
    bryce.converted = json.dumps({
        "copied": [name for name, _ in present if name not in linked and name != "Thin Wall"],
        "left": ["%s (linked: its constant was used)" % name for name in linked] + left})
    return linked, left


class _Op:
    bl_options = {"REGISTER", "UNDO"}


class STECYK3D_OT_new_material(_Op, bpy.types.Operator):
    """Give the active material (or a new one) the Bryce Material node, with a new Bryce material's settings"""
    bl_idname = "stecyk3d.new_material"
    bl_label = "New Bryce material"

    def execute(self, context):
        material = context.material
        if material is None:
            material = bpy.data.materials.new("Bryce material")
            obj = context.object
            if obj is not None and hasattr(obj.data, "materials"):
                obj.data.materials.append(material)
        new_bryce_material(material)
        return {"FINISHED"}


class STECYK3D_OT_make_sky(_Op, bpy.types.Operator):
    """Put a Bryce Sky node on the World Output (replacing the Background), keeping the world's Bryce settings"""
    bl_idname = "stecyk3d.make_sky"
    bl_label = "Make it a Bryce sky"

    @classmethod
    def poll(cls, context):
        return context.world is not None

    def execute(self, context):
        make_sky(context.world)
        return {"FINISHED"}


class STECYK3D_OT_make_light(_Op, bpy.types.Operator):
    """Put a Bryce Light node on the light's Output, keeping the light's Bryce settings"""
    bl_idname = "stecyk3d.make_light"
    bl_label = "Make it a Bryce light"

    @classmethod
    def poll(cls, context):
        return context.light is not None

    def execute(self, context):
        make_light(context.light)
        return {"FINISHED"}


class STECYK3D_OT_settings_to_nodes(_Op, bpy.types.Operator):
    """Move the Bryce settings held in older files' groups (materials, worlds, lights) onto Bryce nodes"""
    bl_idname = "stecyk3d.settings_to_nodes"
    bl_label = "Bryce settings to nodes"

    def execute(self, context):
        m, w, l, notes = settings_to_nodes()
        for note in notes:
            self.report({"WARNING"}, note)
        self.report({"INFO"}, "%d materials, %d worlds, %d lights now use nodes" % (m, w, l))
        return {"FINISHED"}


class STECYK3D_OT_from_shader(_Op, bpy.types.Operator):
    """Replace the active material's nodes with a Bryce Material made from its Principled BSDF's colour,
    roughness, metallic, alpha, transmission, IOR, specular and emission (the table is in principled_map.py)"""
    bl_idname = "stecyk3d.material_from_shader"
    bl_label = "Bryce material from the Blender shader"

    @classmethod
    def poll(cls, context):
        return context.material is not None

    def execute(self, context):
        told = from_principled(context.material)
        if told is None:
            self.report({"ERROR"}, "The material has no Principled BSDF")
            return {"CANCELLED"}
        linked, left = told
        if linked:
            self.report({"WARNING"}, "Linked inputs not carried (their constants were used): " + ", ".join(linked))
        if left:
            self.report({"WARNING"}, "Set but with no Bryce counterpart, not carried: " + ", ".join(left))
        return {"FINISHED"}


CLASSES = [STECYK3D_OT_new_material, STECYK3D_OT_make_sky, STECYK3D_OT_make_light, STECYK3D_OT_settings_to_nodes,
           STECYK3D_OT_from_shader]


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
