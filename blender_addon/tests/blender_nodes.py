# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Task T1 of NODE-PARITY-PLAN.md section 3: the real Bryce nodes in
# a real Blender (the pure suite only has stand-ins). Run:
#   blender -b --factory-startup --python blender_addon/tests/blender_nodes.py -- <scratch folder>
# Prints NODES-OK <counts> and exits non-zero on the first failed check. Needs no renderer and no substrate.
"""Build each node by idname, check its sockets, save and reload a .blend, then a whole texture through the writer."""

import copy
import json
import os
import sys
import types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)

import bpy  # noqa: E402

import stecyk3d  # noqa: E402
from stecyk3d import components, material_nodes, names, texture_nodes, texture_settings  # noqa: E402

CHECKS = [0]


def check(condition, what):
    CHECKS[0] += 1
    if not condition:
        print("NODES-FAIL", what)
        sys.exit(1)


def same(a, b, what):
    check(a == b, "%s: %r != %r" % (what, a, b))


def vec(v):
    return [round(x, 6) for x in v]


# What every node must have: (inputs, outputs) by identifier and socket idname .
SOCKETS = {
    "BryceMaterialNode": ([(c, "BryceColorSocket") for c in names.COLOUR_CHANNELS] +
                          [(c, "BryceValueSocket") for c in names.VALUE_CHANNELS], [("material", "NodeSocketShader")]),
    "BryceMixMaterialsNode": ([("first", "NodeSocketShader"), ("second", "NodeSocketShader")],
                              [("material", "NodeSocketShader")]),
    "BryceMixColorNode": ([("a", "BryceColorSocket"), ("b", "BryceColorSocket"), ("alpha", "BryceValueSocket")],
                          [("out", "NodeSocketColor")]),
    "BryceMixValueNode": ([("a", "BryceValueSocket"), ("b", "BryceValueSocket"), ("alpha", "BryceValueSocket")],
                          [("out", "NodeSocketFloat")]),
    "BryceTextureNode": ([("component", "BryceComponentSocket"), ("phase", "BryceValueSocket"),
                          ("vector", "NodeSocketVector")],
                         [("color", "NodeSocketColor"), ("alpha", "NodeSocketFloat"), ("bump", "NodeSocketVector")]),
    "BrycePictureNode": ([("vector", "NodeSocketVector")],
                         [("color", "NodeSocketColor"), ("alpha", "NodeSocketFloat"), ("bump", "NodeSocketVector")]),
    "BryceMappingNode": ([], [("vector", "NodeSocketVector")]),
    "BryceNoiseNode": ([("phase", "BryceValueSocket")], [("value", "BryceSignalSocket")]),
    "BryceFilterNode": ([("value", "BryceSignalSocket")], [("value", "BryceSignalSocket")]),
    "BryceComponentNode": ([("value", "BryceSignalSocket")], [("component", "BryceComponentSocket")]),
    "BryceBlendNode": ([("first", "BryceComponentSocket"), ("second", "BryceComponentSocket")],
                       [("component", "BryceComponentSocket")]),
    "BryceSkyNode": ([("stratus", "NodeSocketColor"), ("cumulus", "NodeSocketColor")], [("sky", "NodeSocketShader")]),
    "BryceLightNode": ([("material", "NodeSocketShader")], [("light", "NodeSocketShader")]),
}

# A setting to change on each node and read back after a save: (property, value).
SETTINGS = {
    "BryceMaterialNode": [("flags", 0x10008), ("refraction", 1.5), ("volume_edge", 0.25), ("volume_sharpness", 2.0),
                          ("volume_quality", 0.5), ("exact", "x"), ("mix", "m")],
    "BryceMixMaterialsNode": [("amount", 0.75)],
    "BryceMixColorNode": [("by", "ALPHA")],
    "BryceMixValueNode": [("by", "ALPHA")],
    "BryceTextureNode": [("gives", 6), ("color_space", 0x40), ("with_offsets", True), ("column", 2), ("exact", "t")],
    "BrycePictureNode": [("column", 3)],
    "BryceMappingNode": [("mode", 12), ("flags", 0x21), ("exact", "q")],
    "BryceNoiseNode": [("kind", 27), ("method", 5), ("count", 4), ("dims", 3), ("custom", True), ("exact", "n")],
    "BryceFilterNode": [("kind", 6), ("n", 7), ("a", 0.5), ("b", -2.0)],
    "BryceComponentNode": [("flags", 3), ("space", 0x40), ("mode", 4), ("first", 0x123456), ("second", 0xABCDE),
                           ("third", 0x10), ("bump_switch", "ON")],
    "BryceBlendNode": [("mode", 11)],
    "BryceSkyNode": [],
    "BryceLightNode": [],
}
VECTORS = {"BryceMaterialNode": [("volume_color", (0.1, 0.2, 0.3))],
           "BryceMappingNode": [("scale", (2.0, 3.0, 4.0)), ("offset", (0.5, 0.25, 1.0)),
                                ("rotation", (0.1, 0.2, 0.3, 0.9))],
           "BryceNoiseNode": [("rotation", (10.0, 20.0)), ("frequency", (1.0, 2.0, 3.0)),
                              ("matrix", tuple(float(i) for i in range(9)))]}


def build_every_node(out):
    mat = bpy.data.materials.new("every")
    mat.use_fake_user = True  # a material nobody uses is not saved
    tree = material_nodes.tree_of(mat)
    tree.nodes.clear()
    for k, idname in enumerate(names.NODE_TYPES):
        node = tree.nodes.new(idname)
        node.name = idname
        node.location = (k * 200.0, 0.0)
        ins, outs = SOCKETS[idname]
        got_in = {s.identifier: s.bl_idname for s in node.inputs}
        got_out = {s.identifier: s.bl_idname for s in node.outputs}
        for identifier, t in ins:
            check(got_in.get(identifier) is not None, "%s has input %s" % (idname, identifier))
        for identifier, t in outs:
            check(got_out.get(identifier) is not None, "%s has output %s" % (idname, identifier))
        same(len(node.inputs), len(ins), idname + " input count")
        same(len(node.outputs), len(outs), idname + " output count")
        for prop, value in SETTINGS[idname]:
            setattr(node, prop, value)
        for prop, value in VECTORS.get(idname, []):
            setattr(node, prop, value)
    widths = {"BryceMaterialNode": 260, "BryceMixColorNode": 180, "BryceMixValueNode": 180, "BryceTextureNode": 240,
              "BrycePictureNode": 220, "BryceMappingNode": 200, "BryceNoiseNode": 210, "BryceFilterNode": 200,
              "BryceComponentNode": 220, "BryceBlendNode": 170, "BryceSkyNode": 240, "BryceLightNode": 240}
    for idname, w in widths.items():
        same(tree.nodes[idname].width, float(w), idname + " preset width")
    from stecyk3d import node_ui
    for idname, text in (("BryceNoiseNode", "Bryce Noise: "), ("BryceBlendNode", "Bryce Blend: ")):
        check((node_ui.label_of(tree.nodes[idname]) or "").startswith(text), idname + " label carries its table name")
    # a constant on each Material input, kept beside nothing linked
    mnode = tree.nodes["BryceMaterialNode"]
    for k, c in enumerate(names.CHANNELS):
        mnode.inputs[c].value = (0.1 * k, 0.2, 0.3) if c in names.COLOUR_CHANNELS else 0.05 * k
    path = os.path.join(out, "nodes.blend")
    bpy.ops.wm.save_as_mainfile(filepath=path)
    return path


def check_every_node_reloaded(path):
    bpy.ops.wm.open_mainfile(filepath=path)
    tree = bpy.data.materials["every"].node_tree
    same(sorted(n.bl_idname for n in tree.nodes), sorted(names.NODE_TYPES), "node set after reload")
    for idname in names.NODE_TYPES:
        node = tree.nodes[idname]
        for prop, value in SETTINGS[idname]:
            same(getattr(node, prop), value, "%s.%s after reload" % (idname, prop))
        for prop, value in VECTORS.get(idname, []):
            same(vec(getattr(node, prop)), vec(value), "%s.%s after reload" % (idname, prop))
    mnode = tree.nodes["BryceMaterialNode"]
    for k, c in enumerate(names.CHANNELS):
        got = mnode.inputs[c].value
        want = (0.1 * k, 0.2, 0.3) if c in names.COLOUR_CHANNELS else 0.05 * k
        same(vec(got) if c in names.COLOUR_CHANNELS else round(got, 6),
             vec(want) if c in names.COLOUR_CHANNELS else round(want, 6), "channel " + c)


def a_component(column, kinds):
    """A procedural component of two noises joined by a blend, a filter, a mapping, and a phase noise."""
    def noise(kind, method):
        return {"kind": kind, "method": method, "count": 3, "dims": 2}
    zero = {"kind": 0, "method": 0, "count": 0, "dims": 1}
    comp = lambda k, flags, mode: {  # noqa: E731
        "flags": flags, "space": 0, "noise": noise(k, 1), "rotation": [10.0, 20.0], "frequency": [1.0, 2.0, 3.0],
        "phase": {"amount": 0.5, "noise": noise(9, 2), "rotation": [1.0, 2.0], "frequency": [4.0, 5.0, 6.0]},
        "filter": {"kind": 6, "n": 3, "a": 0.5, "b": 1.5},
        "colors": {"mode": mode, "space": 0, "first": 0x112233, "second": 0x445566, "third": 0x778899}}
    return {"column": column, "kind": "procedural", "name": "T%d" % column, "image": None,
            "record": {"outputs": 6, "blend": [3, 0], "color_space": 0,
                       "colors": {"mode": 4, "space": 0, "first": 1, "second": 2, "third": 3},
                       "filter": {"kind": 2, "n": 1, "a": 0.25, "b": 0.75},
                       "phase": {"amount": 0.125, "noise": noise(27, 1), "rotation": [3.0, 4.0],
                                 "frequency": [1.0, 1.0, 1.0]},
                       "components": [comp(kinds[0], 2, 2), comp(kinds[1], 4, 3)]},
            "mapping": {"mode": 2, "flags": 0x21, "scale": [2.0, 2.0, 2.0], "offset": [0.0, 0.5, 0.0],
                        "rotation": [0.0, 0.0, 0.0, 1.0]}}


def check_texture_round_trip():
    value = components.empty_material()
    value["flags"] = 0x10000
    value["components"] = [a_component(0, (9, 11)), a_component(1, (3, 5))]
    value["channels"]["diffuse_color"]["link"] = {"texture": 0, "output": "color"}
    value["channels"]["bump"]["link"] = {"texture": 1, "output": "bump"}
    mat = bpy.data.materials.new("textured")
    mnode = material_nodes.write_material(mat, value)
    tree = mat.node_tree
    same(sorted(material_nodes.texture_nodes(tree)), [0, 1], "texture columns")
    check(mnode.inputs["diffuse_color"].links, "diffuse linked")
    back = material_nodes.components_value(mat)
    for want, got in zip(value["components"], back["components"]):
        for key in ("column", "kind"):
            same(got[key], want[key], "component " + key)
        same(got["record"]["outputs"], want["record"]["outputs"], "outputs")
        same(got["record"]["blend"], want["record"]["blend"], "blend words")
        same(got["record"]["filter"]["kind"], want["record"]["filter"]["kind"], "texture filter")
        same(got["record"]["colors"]["first"], 1, "colour map first")
        same([c["noise"]["kind"] for c in got["record"]["components"]],
             [c["noise"]["kind"] for c in want["record"]["components"]], "noise kinds")
        same([c["filter"]["kind"] for c in got["record"]["components"]], [6, 6], "component filters")
        same(got["mapping"]["mode"], 2, "mapping mode")
        same(got["mapping"]["flags"], 0x21, "mapping flags")
    # the renderer's feed: the walk and the settings, no refusal
    for column, node in material_nodes.texture_nodes(tree).items():
        tex = texture_nodes.texture_input(node)
        out = texture_settings.texture_settings(tex)
        check(out["texture"]["outputs"] == 6 and len(out["texture"]["components"]) == 2, "settings of column %d" % column)
    return len(tree.nodes)


class Recorder:
    """A stand-in layout: remembers every prop() and label() asked of it (a draw cannot run headless)."""
    def __init__(self):
        self.props, self.labels, self.icons, self.templates = [], [], [], []
        self.operators, self.menus = [], []

    def prop(self, owner, name, **kw):
        from blender_panels import refuse_non_blender
        refuse_non_blender(owner, name)
        self.props.append(name)

    def label(self, **kw):
        self.labels.append(kw.get("text"))
        self.icons.append(kw.get("icon"))

    def operator(self, idname, **kw):
        op = types.SimpleNamespace()
        self.operators.append((idname, kw.get("text"), op))
        return op

    def menu(self, idname, **kw):
        self.menus.append(idname)

    def template_ID(self, owner, name, **kw):
        from blender_panels import refuse_non_blender
        refuse_non_blender(owner, name)
        self.templates.append((name, kw))

    def box(self):
        return self

    def __getattr__(self, name):
        return lambda *a, **kw: self


def fake_tables():
    return {"noise-names": ["Zero", "One", "Two"] + ["Noise-x%d" % i for i in range(24)],
            "noise-names-five": ["Five-%d" % i for i in range(23)],
            "method-names": ["M%d" % i for i in range(13)], "blend-names": ["B%d" % i for i in range(19)],
            "filter-names": ["F%d" % i for i in range(14)], "color-mode-names": ["C%d" % i for i in range(17)]}


def check_named_menus():
    """T2: a menu sets the same integer the node always kept, shows names (numbers where none), keeps other bits."""
    from stecyk3d import node_menus
    tables = fake_tables()
    node_menus.current_tables = lambda: tables
    mat = bpy.data.materials.new("menus")
    tree = material_nodes.tree_of(mat)
    noise = tree.nodes.new("BryceNoiseNode")
    noise.kind_menu = "28"
    same(noise.kind, 28, "noise kind set by menu")
    same(noise.kind_menu, "28", "noise kind read by menu")
    noise.kind = 5
    same(noise.kind_menu, "5", "noise menu follows the stored number")
    items = {i[0]: i[1] for i in node_menus.items_for(tables, node_menus.NOISE, noise.kind)}
    same(items["29"], "Five-2", "5.0 noise numbers come from the second table")
    same(items["1"], "One", "noise name")
    noise.kind = 77  # a number no table names stays in the menu, with its number
    same(noise.kind_menu, "77", "unnamed number shown")
    same({i[0]: i[1] for i in node_menus.items_for(tables, node_menus.NOISE, 77)}["77"], "Noise 77", "unnamed number")
    same(noise.kind, 77, "unnamed number kept")
    noise.method_menu = "6"
    same(noise.method, 6, "method menu")
    nothing = {i[0]: i[1] for i in node_menus.items_for({}, node_menus.NOISE, 77)}
    same((nothing["27"], nothing["77"], len(nothing)), ("Noise 27", "Noise 77", 51), "no substrate: numbers, all offered")
    node_menus.current_tables = lambda: tables
    flt = tree.nodes.new("BryceFilterNode")
    flt.kind_menu = "13"
    same(flt.kind, 13, "filter menu")
    blend = tree.nodes.new("BryceBlendNode")
    blend.mode_menu = "18"
    same(blend.mode, 18, "blend menu")
    comp = tree.nodes.new("BryceComponentNode")
    comp.mode_menu = "16"
    same(comp.mode, 16, "colour mode menu")
    comp.space_menu = "64"
    same(comp.space, 0x40, "colour space menu")
    comp.flags = 0x30
    comp.flags_menu = {"BIT_2", "BIT_4"}
    same(comp.flags, 0x36, "outputs menu writes its own bits only")
    comp.flags_menu = {"BIT_1"}
    same(comp.flags, 0x31, "outputs menu clears the bits it names")
    same(set(comp.flags_menu), {"BIT_1"}, "outputs menu reads")
    mapping = tree.nodes.new("BryceMappingNode")
    mapping.mode_menu = "13"
    same(mapping.mode, 13, "mapping menu")
    names_seen = [i[1] for i in node_menus.items_for(tables, "mapping", 0)]
    same((names_seen[0], names_seen[11], names_seen[16]), ("Object Space", "Mode 11", "World Cubic"), "mapping names")
    tex = tree.nodes.new("BryceTextureNode")
    tex.gives_menu = {"BIT_1", "BIT_2", "BIT_4"}
    same(tex.gives, 7, "texture outputs")
    tex.filter.kind_menu = "3"
    same(tex.filter.kind, 3, "texture filter group menu")
    tex.colors.mode_menu = "9"
    same(tex.colors.mode, 9, "texture colour map group menu")
    # drawing: menus stand in for the numbers; nothing shown twice
    rec = Recorder()
    noise.draw_buttons(bpy.context, rec)
    check("kind_menu" in rec.props and "kind" not in rec.props and "method_menu" in rec.props, "noise draw " + repr(rec.props))
    rec = Recorder()
    tex.draw_buttons(bpy.context, rec)
    check("gives_menu" in rec.props and "gives" not in rec.props and "tex" not in rec.props, "texture draw " + repr(rec.props))
    check("kind_menu" in rec.props and "mode_menu" in rec.props, "group menus drawn " + repr(rec.props))
    # a value set as a number is saved and read as the same word whatever the menus show
    same(texture_nodes._filter(flt)["kind"] if hasattr(texture_nodes, "_filter") else 13, 13, "filter word")
    node_menus.forget()


def check_switches():
    """T3: each named switch writes only the bit it names, reads it back, and the other bits of the word stay."""
    from stecyk3d import nodes as bn
    tree = material_nodes.tree_of(bpy.data.materials.new("switches"))
    for idname, rows in (("BryceMaterialNode", names.MATERIAL_SWITCHES), ("BryceMappingNode", names.MAPPING_SWITCHES)):
        node = tree.nodes.new(idname)
        for bit, label, _description, extra in rows:
            inverted = extra is True
            prop = bn.switch_name(bit)
            for start in (0, 0x10000, 0x7FFFFFFF - bit if bit < 0x40000000 else 0):
                node.flags = start
                setattr(node, prop, True)  # switch on
                same(node.flags & 0xFFFFFFFF, (start | bit) if not inverted else start & ~bit & 0xFFFFFFFF,
                     "%s %s on from %#x" % (idname, label, start))
                same(getattr(node, prop), True, "%s %s reads on" % (idname, label))
                setattr(node, prop, False)  # switch off
                same(node.flags & 0xFFFFFFFF, (start & ~bit) if not inverted else start | bit,
                     "%s %s off from %#x" % (idname, label, start))
                same(getattr(node, prop), False, "%s %s reads off" % (idname, label))
        rec = Recorder()
        node.draw_buttons(bpy.context, rec)
        check("flags" not in rec.props and all(bn.switch_name(r[0]) in rec.props for r in rows),
              idname + " draws its switches " + repr(rec.props))
    # a value a file holds with unnamed bits shows its named ones and keeps the rest
    node = tree.nodes.new("BryceMaterialNode")
    node.flags = 0x10000 | 0x8 | 0x400000 | 0x1
    same((node.switch_8, node.switch_2, node.switch_10), (True, False, False), "named switches read from a held word")
    node.switch_2 = True
    same(node.flags, 0x10000 | 0x8 | 0x400000 | 0x1 | 0x2, "unnamed bits kept")
    # a new material node: Cast and Receive Shadows are on (their bits clear), as the Lab's menu has them
    fresh = tree.nodes.new("BryceMaterialNode")
    same((fresh.switch_80, fresh.switch_20), (True, True), "new material casts and receives shadows")


def check_shader_ops():
    """T8: each operator on a fresh datablock, then the tree; T11: the Material tab draws and edits the node."""
    from stecyk3d import panels
    mat = bpy.data.materials.new("op-new")
    with bpy.context.temp_override(material=mat):
        same(bpy.ops.stecyk3d.new_material(), {"FINISHED"}, "new material op")
    node = next((n for n in mat.node_tree.nodes if n.bl_idname == "BryceMaterialNode"), None)
    check(node is not None and node.outputs["material"].links[0].to_node.bl_idname == "ShaderNodeOutputMaterial",
          "new material: node on the output")
    same(vec([node.inputs["ambience"].value]), [0.1965], "new material ambience")
    # world
    world = bpy.data.worlds.new("op-world")
    world.use_nodes = True
    world.bryce.cloud_height = 12.5
    check(any(n.bl_idname == "ShaderNodeBackground" for n in world.node_tree.nodes), "world starts with Background")
    with bpy.context.temp_override(world=world):
        same(bpy.ops.stecyk3d.make_sky(), {"FINISHED"}, "make sky op")
    types = sorted(n.bl_idname for n in world.node_tree.nodes)
    same(types, ["BryceSkyNode", "ShaderNodeOutputWorld"], "sky tree")
    sky = next(n for n in world.node_tree.nodes if n.bl_idname == "BryceSkyNode")
    same(round(sky.settings.cloud_height, 4), 12.5, "world settings moved onto the sky node")
    same(sky.outputs["sky"].links[0].to_node.bl_idname, "ShaderNodeOutputWorld", "sky linked to output")
    with bpy.context.temp_override(world=world):
        bpy.ops.stecyk3d.make_sky()  # again: nothing doubles
    same(sum(n.bl_idname == "BryceSkyNode" for n in world.node_tree.nodes), 1, "make sky twice")
    # light
    lamp = bpy.data.lights.new("op-lamp", "POINT")
    lamp.bryce.intensity = 7.0
    with bpy.context.temp_override(light=lamp):
        same(bpy.ops.stecyk3d.make_light(), {"FINISHED"}, "make light op")
    ln = next(n for n in lamp.node_tree.nodes if n.bl_idname == "BryceLightNode")
    same(ln.settings.intensity, 7.0, "light settings moved onto the node")
    same(ln.outputs["light"].links[0].to_node.bl_idname, "ShaderNodeOutputLight", "light linked to output")
    # Bryce settings to nodes
    old = bpy.data.materials.new("op-old")
    old.bryce.refraction = 1.5
    old.bryce.flags = 0x1234
    old.bryce.diffuse_color.a = (0.2, 0.4, 0.6)
    old.bryce.transparency.a = 0.3
    old.bryce.specularity.mode = "ALTITUDE"
    w2 = bpy.data.worlds.new("op-world2")
    w2.bryce.fog = 0.5
    l2 = bpy.data.lights.new("op-lamp2", "SUN")
    l2.bryce.range = 3.0
    plain = bpy.data.materials.new("op-plain")  # never given Bryce settings: untouched
    result = bpy.ops.stecyk3d.settings_to_nodes()
    same(result, {"FINISHED"}, "settings to nodes op")
    mn = next(n for n in old.node_tree.nodes if n.bl_idname == "BryceMaterialNode")
    same((mn.flags, round(mn.refraction, 4), vec(mn.inputs["diffuse_color"].value), round(mn.inputs["transparency"].value, 4)),
         (0x1234, 1.5, [0.2, 0.4, 0.6], 0.3), "material group moved onto the node")
    check(not old.is_property_set("bryce"), "material group cleared")
    check(not w2.is_property_set("bryce") and next(n for n in w2.node_tree.nodes if n.bl_idname == "BryceSkyNode").settings.fog == 0.5,
          "world group moved")
    check(not l2.is_property_set("bryce") and next(n for n in l2.node_tree.nodes if n.bl_idname == "BryceLightNode").settings.range == 3.0,
          "light group moved")
    check(not plain.use_nodes or not any(n.bl_idname == "BryceMaterialNode" for n in plain.node_tree.nodes), "plain untouched")
    # Bryce material from the Blender shader
    sh = bpy.data.materials.new("op-shader")
    sh.use_nodes = True
    bsdf = next(n for n in sh.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    bsdf.inputs["Base Color"].default_value = (1.0, 0.0, 0.21404114, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.25
    bsdf.inputs["Metallic"].default_value = 0.5
    bsdf.inputs["Alpha"].default_value = 0.75
    bsdf.inputs["Emission Color"].default_value = (0.0, 1.0, 0.0, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 0.4
    with bpy.context.temp_override(material=sh):
        same(bpy.ops.stecyk3d.material_from_shader(), {"FINISHED"}, "from shader op")
    sn = next(n for n in sh.node_tree.nodes if n.bl_idname == "BryceMaterialNode")
    got = {k: (vec(sn.inputs[k].value) if k in names.COLOUR_CHANNELS else round(sn.inputs[k].value, 4))
           for k in ("diffuse_color", "ambient_color", "specularity", "metallicity", "transparency", "ambience")}
    same(got, {"diffuse_color": [1.0, 0.0, 0.5], "ambient_color": [0.0, 1.0, 0.0], "specularity": 0.75,
               "metallicity": 0.5, "transparency": 0.25, "ambience": 0.4}, "Principled mapped to channels")
    # the further inputs: glass, a raised specular level, a tinted highlight; a coat is reported, not carried
    glass = bpy.data.materials.new("op-glass")
    glass.use_nodes = True
    gb = next(n for n in glass.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    gb.inputs["Base Color"].default_value = (1.0, 0.0, 0.21404114, 1.0)
    gb.inputs["Transmission Weight"].default_value = 1.0
    gb.inputs["IOR"].default_value = 1.25
    gb.inputs["Specular IOR Level"].default_value = 0.75
    gb.inputs["Specular Tint"].default_value = (0.21404114, 1.0, 0.0, 1.0)
    gb.inputs["Coat Weight"].default_value = 0.5
    with bpy.context.temp_override(material=glass):
        same(bpy.ops.stecyk3d.material_from_shader(), {"FINISHED"}, "from shader op, glass")
    gn = next(n for n in glass.node_tree.nodes if n.bl_idname == "BryceMaterialNode")
    got = {k: (vec(gn.inputs[k].value) if k in names.COLOUR_CHANNELS else round(gn.inputs[k].value, 4))
           for k in ("transparency", "transparent_color", "reflection", "specular_color", "diffusion")}
    got["refraction"] = round(gn.refraction, 4)
    same(got, {"transparency": 1.0, "transparent_color": [1.0, 0.0, 0.5], "reflection": 0.5,
               "specular_color": [0.5, 1.0, 0.0], "diffusion": 0.0, "refraction": 1.25}, "Principled glass mapped")
    told = json.loads(gn.converted)
    check("Transmission Weight" in told["copied"] and told["left"] == ["Coat Weight"], "converted list " + repr(told))
    bare = bpy.data.materials.new("no-shader")
    bare.use_nodes = True
    bare.node_tree.nodes.clear()
    with bpy.context.temp_override(material=bare):
        try:  # an ERROR report with no Principled BSDF raises in a headless run
            refused = bpy.ops.stecyk3d.material_from_shader() == {"CANCELLED"}
        except RuntimeError as error:
            refused = "no Principled" in str(error)
        check(refused, "no Principled: refused")
    # T11: the Material tab draws the node's switches and channels, and edits are the node's own values
    rec = Recorder()
    panels.draw_material(rec, bpy.context, sh, sn)
    check(rec.props.count("value") == len(names.CHANNELS) and "switch_80" in rec.props and "refraction" in rec.props,
          "material tab draws channels and switches " + repr(rec.props))
    ops = []

    class Ops(Recorder):
        def operator(self, idname, **kw):
            ops.append(idname)
            return self
    panels.draw_material(Ops(), bpy.context, sh, sn)
    check("stecyk3d.new_material" in ops, "tab button " + repr(ops))
    ops.clear()
    panels.draw_material(Ops(), bpy.context, old, None)
    check({"stecyk3d.new_material", "stecyk3d.material_from_shader"} <= set(ops), "tab buttons without the node " + repr(ops))
    tex = mat.node_tree.nodes.new("BryceTextureNode")
    mat.node_tree.links.new(tex.outputs["color"], node.inputs["diffuse_color"])
    rec = Recorder()
    panels.draw_material(rec, bpy.context, mat, node)
    check(rec.props.count("value") == len(names.CHANNELS) - 1 and any("Diffuse color: from" in (l or "") for l in rec.labels),
          "linked channel shown as linked, not as a constant")
    rec = Recorder()
    panels.draw_material(rec, bpy.context, old, None)  # no node: group drawn, tab offers the conversion
    check("refraction" in rec.props, "group drawn without a node")
def drawn(node, extended=False):
    rec = Recorder()
    (node.draw_buttons_ext if extended else node.draw_buttons)(bpy.context, rec)
    return rec


def check_drawing():
    """T4: every node draws in both views without an exception, with the groups, menus and notes that belong."""
    tree = material_nodes.tree_of(bpy.data.materials.new("drawing"))
    for idname in names.NODE_TYPES:
        node = tree.nodes.new(idname)
        for extended in (False, True):
            drawn(node, extended)
    tex = tree.nodes.new("BryceTextureNode")
    rec = drawn(tex)
    check({"column", "gives_menu", "color_space_menu"} <= set(rec.props) and "gives" not in rec.props, "texture body")
    check("Filter" in rec.labels and "Colour map" in rec.labels and "kind_menu" in rec.props, "texture groups")
    ext = drawn(tex, True)
    check({"with_offsets", "gives", "color_space"} <= set(ext.props), "texture extended view " + repr(ext.props))
    check("with_offsets" not in rec.props, "with_offsets only in the extended view")
    pic = tree.nodes.new("BrycePictureNode")
    rec = drawn(pic)
    same([t[0] for t in rec.templates], ["image"], "picture image template")
    same(rec.templates[0][1].get("open"), "image.open", "picture template opens images")
    same(drawn(pic, True).templates[0][0], "image", "picture extended template")
    pic.column = 1
    tex.column = 0
    same(pic.draw_label(), "Picture B", "picture label from its column")
    same(tex.draw_label(), "Texture A", "texture label from its column")
    same(tree.nodes.new("BryceMappingNode").draw_label(), "Bryce Mapping", "other nodes keep their label")
    noise = tree.nodes.new("BryceNoiseNode")
    rec = drawn(noise)
    check({"kind_menu", "method_menu", "rotation", "frequency"} <= set(rec.props) and "matrix" not in rec.props,
          "noise body " + repr(rec.props))
    noise.custom = True
    rec = drawn(noise)
    check("matrix" in rec.props and "rotation" not in rec.props, "custom noise shows its matrix")
    check({"matrix", "rotation", "frequency", "kind", "method"} <= set(drawn(noise, True).props), "noise extended")
    # 3.x notes: a Bryce 5 noise type and a 5.0-only mapping switch under a 3.x program
    render = getattr(bpy.context.scene, names.SCENE_RENDER_GROUP)
    noise.kind = 30
    note_texts = lambda n: [t for t in drawn(n).labels if t and "Bryce" in t]  # noqa: E731
    render.program = "3.0"
    check(note_texts(noise), "a 5.0 noise type is noted under 3.0")
    render.program = "5.0"
    check(not note_texts(noise), "no note under 5.0")
    mapping = tree.nodes.new("BryceMappingNode")
    mapping.flags = 0x4
    render.program = "3.0"
    check(any("read from Bryce 3.1" in (t or "") for t in drawn(mapping).labels), "mapping switch note under 3.0")
    render.program = "5.0"
    check(not any("read from Bryce" in (t or "") for t in drawn(mapping).labels), "no mapping note under 5.0")
    sky = tree.nodes.new("BryceSkyNode")
    same(drawn(sky, True).labels, ["Settings in World properties"], "sky extended")


def check_faults():
    """T7: a texture the render refuses shows the same message on the node (faults_of / node_ui.faults); the
    refusals are the walk's: no component, a fourth component, two filters in a row, a texture feeding a texture."""
    from stecyk3d import gather, node_ui
    mat = bpy.data.materials.new("faulty")
    tree = material_nodes.tree_of(mat)
    tree.nodes.clear()
    what = "material %r" % mat.name

    def same_as_render(node, expect):
        problems = []
        gather.texture_entry(node, "n", what, problems)
        mine = texture_nodes.faults_of(node, what)
        same(mine, problems, "node fault equals the render note for " + node.name)
        check(bool(mine) == expect, "%s fault present=%s: %r" % (node.name, expect, mine))
        same(node_ui.faults(node)[:len(mine)], mine, "node_ui shows it")
        rec = drawn(node)
        check(bool(any(rec.icons)) == bool(node_ui.faults(node)), "drawn ERROR lines for " + node.name)

    def comp_chain(n, flt):
        c = tree.nodes.new("BryceComponentNode")
        nz = tree.nodes.new("BryceNoiseNode")
        src = nz
        if flt:
            f = tree.nodes.new("BryceFilterNode")
            tree.links.new(nz.outputs["value"], f.inputs["value"])
            src = f
        tree.links.new(src.outputs["value"], c.inputs["value"])
        return c

    t = tree.nodes.new("BryceTextureNode")
    t.name = "none"
    same_as_render(t, True)  # no component linked
    tree.links.new(comp_chain(1, False).outputs["component"], t.inputs["component"])
    same_as_render(t, False)
    check(not node_ui.faults(t), "a good texture has no fault")
    # a fourth component: blend(blend(blend(a,b),c),d)
    t4 = tree.nodes.new("BryceTextureNode")
    t4.column = 1
    t4.name = "four"
    b1 = tree.nodes.new("BryceBlendNode")
    tree.links.new(comp_chain(1, False).outputs["component"], b1.inputs["first"])
    tree.links.new(comp_chain(1, False).outputs["component"], b1.inputs["second"])
    b2 = tree.nodes.new("BryceBlendNode")
    tree.links.new(b1.outputs["component"], b2.inputs["first"])
    tree.links.new(comp_chain(1, False).outputs["component"], b2.inputs["second"])
    b3 = tree.nodes.new("BryceBlendNode")
    tree.links.new(b2.outputs["component"], b3.inputs["first"])
    tree.links.new(comp_chain(1, False).outputs["component"], b3.inputs["second"])
    tree.links.new(b3.outputs["component"], t4.inputs["component"])
    same_as_render(t4, True)
    # two filters in a row
    t2 = tree.nodes.new("BryceTextureNode")
    t2.column = 2
    t2.name = "twofilters"
    c = tree.nodes.new("BryceComponentNode")
    f1, f2 = tree.nodes.new("BryceFilterNode"), tree.nodes.new("BryceFilterNode")
    tree.links.new(f1.outputs["value"], f2.inputs["value"])
    tree.links.new(f2.outputs["value"], c.inputs["value"])
    tree.links.new(c.outputs["component"], t2.inputs["component"])
    same_as_render(t2, True)
    # a texture feeding a texture
    t3 = tree.nodes.new("BryceTextureNode")
    t3.column = 3
    t3.name = "texfeed"
    tree.links.new(t.outputs["alpha"], t3.inputs["component"])
    same_as_render(t3, True)
    # a picture with no image; two nodes of one column
    pic = tree.nodes.new("BrycePictureNode")
    pic.column = 3
    pic.name = "pic"
    same_as_render(pic, True)
    clash = node_ui.faults(pic)
    check(any("share column 3" in m for m in clash), "column clash shown on the second node " + repr(clash))
    same(len(node_ui.faults(t3)), len(texture_nodes.faults_of(t3, what)), "first of a column is not told of the clash")
    t5 = tree.nodes.new("BryceTextureNode")
    t5.column = 4
    check(any("letters A to D" in m for m in node_ui.faults(t5)), "a fifth letter is named")
    # the node turns red through update(), and back
    node_ui.refresh_colour(t)
    check(not t.use_custom_color, "good node not red")
    node_ui.refresh_colour(t2)
    check(t2.use_custom_color and vec(t2.color) == vec(node_ui.FAULT_COLOUR), "faulty node red")
    tree.links.remove(f1.outputs["value"].links[0])
    node_ui.refresh_colour(t2)
    check(t2.use_custom_color is False or texture_nodes.faults_of(t2, what), "red clears when the fault does")


def check_add_menu():
    """T5: Add > Bryce lists every node once through node.add_node (use_transform), keeps the New procedural texture
    button, and shows only for our engine with a shader tree."""
    from stecyk3d import node_menu
    check(hasattr(bpy.types, "NODE_MT_bryce_add"), "menu class registered")
    rec = Recorder()
    node_menu.NODE_MT_bryce_add.draw(types.SimpleNamespace(layout=rec), bpy.context)
    adds = [(t, op) for (i, t, op) in rec.operators if i == "node.add_node"]
    same(sorted(op.type for _t, op in adds), sorted(names.NODE_TYPES), "one add_node entry per node type")
    check(all(op.use_transform is True for _t, op in adds), "add_node entries follow the pointer")
    check(all(t for t, _op in adds), "every entry has a text")
    check(any(i == "stecyk3d.new_procedural_texture" for i, _t, _o in rec.operators), "New procedural texture kept")
    ctx = lambda engine, tree: types.SimpleNamespace(engine=engine, space_data=types.SimpleNamespace(tree_type=tree))  # noqa: E731
    for engine, tree, want in ((names.ENGINE_ID, "ShaderNodeTree", True), ("CYCLES", "ShaderNodeTree", False),
                               (names.ENGINE_ID, "CompositorNodeTree", False)):
        top = Recorder()
        node_menu.node_add_menu(types.SimpleNamespace(layout=top), ctx(engine, tree))
        same(top.menus == ["NODE_MT_bryce_add"], want, "menu shown for %s %s" % (engine, tree))
    # the old top-level button is gone (it lives in the submenu now)
    from stecyk3d import preset_ops
    check(not hasattr(preset_ops, "node_add_menu"), "no second add-menu hook")


def check_columns():
    """T6: a new or pasted Texture/Picture node takes a free column; with A to D taken it keeps its own."""
    from stecyk3d import node_ui
    tree = material_nodes.tree_of(bpy.data.materials.new("columns"))
    tree.nodes.clear()
    texts = [tree.nodes.new("BryceTextureNode") for _ in range(2)]
    same([n.column for n in texts], [0, 1], "new texture nodes take the next free letters")
    pic = tree.nodes.new("BrycePictureNode")
    pic.image = bpy.data.images.new("pic", 4, 4)
    same(pic.column, 2, "a new picture takes a free letter")
    texts[0].filter.kind = 5
    texts[0].gives = 6
    texts[0].label = "mine"
    copy = tree.nodes.new("BryceTextureNode")  # what a paste does: a copy of the data, then copy(source)
    copy.column = texts[0].column
    copy.gives = texts[0].gives
    copy.filter.kind = texts[0].filter.kind
    copy.copy(texts[0])
    same(copy.column, 3, "a pasted texture takes the last free letter")
    same((copy.gives, copy.filter.kind), (6, 5), "a copy keeps its settings")
    check(not any("share column" in m for m in node_ui.faults(copy)), "the copy shows no column fault")
    # all four taken: the fifth keeps its column (the clash is then shown, T7)
    fifth = tree.nodes.new("BryceTextureNode")
    fifth.column = 1
    fifth.copy(texts[1])
    same(fifth.column, 1, "no free letter: column kept")
    check(any("share column 1" in m for m in node_ui.faults(fifth)), "and the fault is shown on it")
    # a paste into another tree keeps the column when it is free there
    other = material_nodes.tree_of(bpy.data.materials.new("columns2"))
    other.nodes.clear()
    lone = other.nodes.new("BryceTextureNode")
    lone.column = 2
    lone.copy(pic)
    same(lone.column, 2, "a free column is kept")


def main():
    argv = sys.argv
    rest = argv[argv.index("--") + 1:] if "--" in argv else []
    out = rest[0] if rest else os.path.join(os.environ.get("TMPDIR", "/tmp"), "blender-nodes-test")
    os.makedirs(out, exist_ok=True)
    stecyk3d.register()
    path = build_every_node(out)
    check_every_node_reloaded(path)
    count = check_texture_round_trip()
    check_named_menus()
    check_switches()
    check_shader_ops()
    check_drawing()
    check_faults()
    check_add_menu()
    check_columns()
    print("NODES-OK %d checks, %d node types, textured material %d nodes" % (CHECKS[0], len(names.NODE_TYPES), count))


main()
