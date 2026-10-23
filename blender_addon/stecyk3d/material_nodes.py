# SPDX-License-Identifier: GPL-3.0-or-later
# (the Bryce nodes: a material's channels as the Bryce Material node's inputs, a texture component as a
#       Bryce Texture fed by Bryce Components (Bryce Blends between them) fed by Bryce Noises, a Bryce Filter, a Bryce
# Mapping on `vector`, a Bryce Picture), (a material preset written into the nodes; a texture preset's
# component),
# (the Blender layer; needs Blender, not
#         testable here)
"""A Bryce material value (components.py) written into a material's nodes, and its components read back."""

import bpy

from . import names

OUTPUT_NODE = "ShaderNodeOutputMaterial"
COLUMN_GAP = 260.0  # where new nodes stand (Freedom)


def tree_of(material):
    material.use_nodes = True
    return material.node_tree


def bryce_material_node(tree):
    for node in tree.nodes:
        if node.bl_idname == "BryceMaterialNode":
            return node
    return None


def texture_nodes(tree):
    """The texture components of a tree by column: {column: Bryce Texture or Bryce Picture node}."""
    out = {}
    for node in tree.nodes:
        if node.bl_idname in ("BryceTextureNode", "BrycePictureNode"):
            out.setdefault(int(node.column), node)
    return out


def _texture_parts(tree):
    """Every Bryce Texture or Bryce Picture node of a material's tree, those inside its node groups too, in the order
    the tree lists them."""
    found = []
    for node in tree.nodes:
        if node.bl_idname in ("BryceTextureNode", "BrycePictureNode"):
            found.append(node)
        elif node.bl_idname == "ShaderNodeGroup" and node.node_tree is not None and node.node_tree is not tree:
            found.extend(n for n in node.node_tree.nodes if n.bl_idname in ("BryceTextureNode", "BrycePictureNode"))
    return found


def adopt_legacy_columns(materials):
    """A material of a file the earlier add-on saved holds its textures in node groups without a column of their own
    (that add-on counted the columns off the order of the textures, so every node reads 0). Two textures of one column
    are refused; here the second and later of a column take the lowest column still free, in the order the
    material lists them, so that each channel keeps its own texture. Run on a file just loaded, never on edits."""
    for material in materials:
        tree = material.node_tree if material.use_nodes else None
        if tree is None:
            continue
        taken, later = set(), []
        for node in _texture_parts(tree):
            column = int(node.column)
            if column in taken:
                later.append(node)
            else:
                taken.add(column)
        for node in later:
            free = next((c for c in range(4) if c not in taken), None)
            if free is None:
                break
            node.column = free
            taken.add(free)


def _set_group(group, values):
    for key, value in values.items():
        if hasattr(group, key):
            setattr(group, key, value)


def _mapping_node(tree, mapping, at):
    node = tree.nodes.new("BryceMappingNode")
    node.location = at
    node.mode = _signed(mapping["mode"])
    node.flags = _signed(mapping["flags"])
    node.scale = mapping["scale"]
    node.offset = mapping["offset"]
    node.rotation = mapping["rotation"]
    return node


def _signed(word):
    """A word as Blender's signed 32-bit IntProperty holds it (read back through numbers.word)."""
    word = int(word) & 0xFFFFFFFF
    return word - (1 << 32) if word >= (1 << 31) else word


def _noise_node(tree, noise, rotation, frequency, phase, at):
    node = tree.nodes.new("BryceNoiseNode")
    node.location = at
    node.kind = _signed(noise["kind"])
    node.method = _signed(noise["method"])
    node.count = int(noise["count"])
    node.dims = int(noise["dims"])
    node.rotation = rotation
    node.frequency = frequency
    node.custom = False
    if phase is not None:
        node.inputs["phase"].value = float(phase["amount"])
        if int(phase["noise"]["kind"]) != 0:
            p = _noise_node(tree, phase["noise"], phase["rotation"], phase["frequency"], None,
                            (at[0] - COLUMN_GAP, at[1] - 120.0))
            tree.links.new(p.outputs["value"], node.inputs["phase"])
    return node


def add_component_nodes(tree, comp, images=None, at=(0.0, 0.0)):
    """: the nodes of one component (procedural or picture), its mapping linked to `vector`; the texture node
    is returned, standing at `at`."""
    x, y = at
    if comp["kind"] == "picture":
        node = tree.nodes.new("BrycePictureNode")
        node.location = (x, y)
        node.column = int(comp["column"])
        image = (images or {}).get(comp.get("image")) or bpy.data.images.get(comp.get("image") or "")
        node.image = image
    else:
        record = comp["record"]
        node = tree.nodes.new("BryceTextureNode")
        node.location = (x, y)
        node.column = int(comp["column"])
        node.gives = _signed(record["outputs"])
        saved = record.get("saved")
        if saved:  # : the result as saved in the document (colour word, four doubles), kept with the node
            node["bryce_saved"] = [int(saved["colour"]) & 0xFFFFFFFF] + [float(v) for v in saved["numbers"]]
        node.color_space = _signed(record["color_space"])
        _set_group(node.colors, {k: _signed(v) for k, v in record["colors"].items() if k != "space"})
        f = record["filter"]
        _set_group(node.filter, {"kind": _signed(f["kind"]), "n": int(f["n"]), "a": f["a"], "b": f["b"]})
        phase = record["phase"]
        node.inputs["phase"].value = float(phase["amount"])
        if int(phase["noise"]["kind"]) != 0:
            p = _noise_node(tree, phase["noise"], phase["rotation"], phase["frequency"], None, (x - COLUMN_GAP, y + 200))
            tree.links.new(p.outputs["value"], node.inputs["phase"])
        made = []
        for k, c in enumerate(record["components"]):
            cy = y - 220.0 * k
            noise = _noise_node(tree, c["noise"], c["rotation"], c["frequency"], c["phase"], (x - 3 * COLUMN_GAP, cy))
            source = noise.outputs["value"]
            if int(c["filter"]["kind"]) != 0:
                filt = tree.nodes.new("BryceFilterNode")
                filt.location = (x - 2.5 * COLUMN_GAP, cy)
                filt.kind = _signed(c["filter"]["kind"])
                filt.n = int(c["filter"]["n"])
                filt.a, filt.b = c["filter"]["a"], c["filter"]["b"]
                tree.links.new(source, filt.inputs["value"])
                source = filt.outputs["value"]
            cn = tree.nodes.new("BryceComponentNode")
            cn.location = (x - 2 * COLUMN_GAP, cy)
            cn.flags = _signed(c["flags"])
            cn.space = _signed(c["space"])
            cn.mode = _signed(c["colors"]["mode"])
            cn.first, cn.second, cn.third = (_signed(c["colors"][key]) for key in ("first", "second", "third"))
            tree.links.new(source, cn.inputs["value"])
            made.append(cn.outputs["component"])
        result = made[0]
        for k in range(1, len(made)):
            blend = tree.nodes.new("BryceBlendNode")
            blend.location = (x - COLUMN_GAP, y - 220.0 * (k - 0.5))
            blend.mode = _signed(record["blend"][k - 1])
            tree.links.new(result, blend.inputs["first"])
            tree.links.new(made[k], blend.inputs["second"])
            result = blend.outputs["component"]
        tree.links.new(result, node.inputs["component"])
    node.label = comp.get("name") or ""
    if comp.get("mapping") is not None:
        m = _mapping_node(tree, comp["mapping"], (x - COLUMN_GAP, y + 120.0))
        tree.links.new(m.outputs["vector"], node.inputs["vector"])
    return node


def _from(socket):
    return socket.links[0].from_node if socket.is_linked and socket.links else None


def _noise_of(node):
    noise = {"kind": node.kind & 0xFFFFFFFF, "method": node.method & 0xFFFFFFFF, "count": node.count,
             "dims": node.dims}
    return noise, list(node.rotation), list(node.frequency)


def _phase_of(socket):
    p = _from(socket)
    if p is not None and p.bl_idname == "BryceNoiseNode":
        noise, rotation, frequency = _noise_of(p)
    else:
        noise, rotation, frequency = {"kind": 0, "method": 0, "count": 0, "dims": 1}, [0.0, 0.0], [0.0, 0.0, 0.0]
    return {"amount": socket.value, "noise": noise, "rotation": rotation, "frequency": frequency}


def component_from_nodes(node):
    """: the component a texture node and the nodes feeding it hold, as components.py's value."""
    mapping_node = _from(node.inputs["vector"])
    mapping = None
    if mapping_node is not None and mapping_node.bl_idname == "BryceMappingNode":
        mapping = {"mode": mapping_node.mode & 0xFFFFFFFF, "flags": mapping_node.flags & 0xFFFFFFFF,
                   "scale": list(mapping_node.scale), "offset": list(mapping_node.offset),
                   "rotation": list(mapping_node.rotation)}
    if node.bl_idname == "BrycePictureNode":
        image = node.image.name if node.image else None
        return {"column": int(node.column), "kind": "picture", "name": node.label or image or "", "record": None,
                "mapping": mapping, "image": image}
    comps, blends = [], []

    def walk(socket):
        n = _from(socket)
        if n is None:
            return
        if n.bl_idname == "BryceBlendNode":
            walk(n.inputs["first"])
            blends.append(n.mode & 0xFFFFFFFF)
            walk(n.inputs["second"])
        elif n.bl_idname == "BryceComponentNode":
            src = _from(n.inputs["value"])
            filt = {"kind": 0, "n": 0, "a": 0.0, "b": 0.0}
            if src is not None and src.bl_idname == "BryceFilterNode":
                filt = {"kind": src.kind & 0xFFFFFFFF, "n": src.n, "a": src.a, "b": src.b}
                src = _from(src.inputs["value"])
            if src is not None and src.bl_idname == "BryceNoiseNode":
                noise, rotation, frequency = _noise_of(src)
                phase = _phase_of(src.inputs["phase"])
            else:
                noise, rotation, frequency = {"kind": 0, "method": 0, "count": 0, "dims": 1}, [0.0, 0.0], [0.0] * 3
                phase = {"amount": 0.0, "noise": dict(noise), "rotation": [0.0, 0.0], "frequency": [0.0] * 3}
            comps.append({"flags": n.flags & 0xFFFFFFFF, "space": n.space & 0xFFFFFFFF, "noise": noise,
                          "rotation": rotation, "frequency": frequency, "phase": phase, "filter": filt,
                          "colors": {"mode": n.mode & 0xFFFFFFFF, "space": n.space & 0xFFFFFFFF,
                                     "first": n.first & 0xFFFFFFFF, "second": n.second & 0xFFFFFFFF,
                                     "third": n.third & 0xFFFFFFFF}})

    walk(node.inputs["component"])
    record = {"outputs": node.gives & 0xFFFFFFFF, "blend": (blends + [0, 0])[:2],
              "color_space": node.color_space & 0xFFFFFFFF,
              "colors": {"mode": node.colors.mode & 0xFFFFFFFF, "space": node.color_space & 0xFFFFFFFF,
                         "first": node.colors.first & 0xFFFFFFFF, "second": node.colors.second & 0xFFFFFFFF,
                         "third": node.colors.third & 0xFFFFFFFF},
              "filter": {"kind": node.filter.kind & 0xFFFFFFFF, "n": node.filter.n, "a": node.filter.a,
                         "b": node.filter.b},
              "phase": _phase_of(node.inputs["phase"]), "components": comps}
    saved = node.get("bryce_saved")
    if saved is not None:
        record["saved"] = {"colour": int(saved[0]), "numbers": [float(v) for v in saved[1:5]]}
    return {"column": int(node.column), "kind": "procedural", "name": node.label, "record": record,
            "mapping": mapping, "image": None}


def components_value(material):
    """The material's components as components.py's value (channels are not read: the commands on components
    need none; links to a removed node go with it)."""
    from . import components
    value = components.empty_material()
    if material is None or material.node_tree is None:
        return value
    for column, node in sorted(texture_nodes(material.node_tree).items()):
        value["components"].append(component_from_nodes(node))
    shown = material.get("stecyk3d_shown")
    value["shown"] = shown if shown is not None and shown >= 0 else None
    return value


def remove_component_nodes(tree, column):
    """: removes the component's texture node and the nodes that feed only it (its mapping where no other
    component uses that mapping)."""
    node = texture_nodes(tree).get(column)
    if node is None:
        return
    feeding, stack = [], [node]
    while stack:
        n = stack.pop()
        for socket in n.inputs:
            for link in list(socket.links):
                src = link.from_node
                used_elsewhere = any(l.to_node != n for out in src.outputs for l in out.links)
                if not used_elsewhere and src not in feeding:
                    feeding.append(src)
                    stack.append(src)
    for n in feeding + [node]:
        tree.nodes.remove(n)


def write_material(material, value, images=None):
    """: the material's nodes become the value's: a Bryce Material node feeding the output, its
    settings and channels' constants, every component's nodes, the channels' links. Nothing before is kept.
    A value with a `mix` (a second material and an amount, components.material_from_record) gets a second Bryce
    Material node and a Bryce Mix Materials node between the two and the output."""
    tree = tree_of(material)
    tree.nodes.clear()
    out = tree.nodes.new(OUTPUT_NODE)
    out.location = (500.0, 0.0)
    node = _write_material_node(tree, value, images, 0.0)
    mix = value.get("mix")
    if mix:
        other = _write_material_node(tree, mix["second"], images, -1200.0)
        joined = tree.nodes.new("BryceMixMaterialsNode")
        joined.location = (300.0, 0.0)
        joined.amount = mix["amount"]
        tree.links.new(node.outputs["material"], joined.inputs["first"])
        tree.links.new(other.outputs["material"], joined.inputs["second"])
        tree.links.new(joined.outputs["material"], out.inputs["Surface"])
    else:
        tree.links.new(node.outputs["material"], out.inputs["Surface"])
    material["stecyk3d_shown"] = -1 if value.get("shown") is None else int(value["shown"])
    return node


def _write_material_node(tree, value, images, y):
    """One Bryce Material node with its components' nodes and links, `y` below the first row of nodes."""
    node = tree.nodes.new("BryceMaterialNode")
    node.location = (0.0, y)
    node.flags = _signed(value["flags"])
    node.volume_color = value["volume_color"]
    for key in ("refraction", "volume_edge", "volume_sharpness", "volume_quality"):
        setattr(node, key, value[key])
    texture_at = {}
    for k, comp in enumerate(value["components"]):
        texture_at[comp["column"]] = add_component_nodes(tree, comp, images, (-600.0, y - 300.0 * k))
    for name in names.CHANNELS:
        ch = value["channels"][name]
        node.inputs[name].value = ch["value"]
        link = ch["link"]
        if link is None:
            continue
        if "texture" in link and link["texture"] in texture_at:
            tree.links.new(texture_at[link["texture"]].outputs[link["output"]], node.inputs[name])
        elif "mix" in link:
            colour = name in names.COLOUR_CHANNELS
            mix = tree.nodes.new("BryceMixColorNode" if colour else "BryceMixValueNode")
            mix.location = (-250.0, y - 40.0 * names.CHANNELS.index(name))
            mix.by = link["mix"]
            for side in ("a", "b", "alpha"):
                mix.inputs[side].value = link[side]["value"]
                column = link[side]["texture"]
                if column in texture_at:
                    output = "alpha" if side == "alpha" else ("color" if colour else (
                        "bump" if name == "bump" else "alpha"))
                    tree.links.new(texture_at[column].outputs[output], mix.inputs[side])
            tree.links.new(mix.outputs["out"], node.inputs[name])
    return node
