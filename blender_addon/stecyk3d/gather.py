# SPDX-License-Identifier: GPL-3.0-or-later
# as revised (textured channels; spot and area lamps, Bryce 5's additions;
#       metaballs, groups and their members, terrains from their pictures)
# BS-1 (the evaluated graph, items' matrices, loop triangles, slots,
#       the evaluated camera and its frame)
# (Reading the scene; needs Blender, not testable here)
"""Blender data to the plain dicts convert.py takes, then the description. Nothing here computes a number."""

from array import array

import math
import bpy

from .numbers import single
from . import (camera_frame, channels, convert, create, defaults, group_links, lamps, names, pictures, props, scene_read, solids, texture_nodes,
               texture_settings)
from .axes import mat4_mul, to_bryce_direction, to_bryce_point

LIGHT_ON = 0x100  # [format]
NO_FALL_OFF = 0x400  # [format]
SUN_DEFAULT = LIGHT_ON | NO_FALL_OFF  # a held document's sun word is this ; 0x100 alone falls off to nothing


def kept_stored_tree(obj, settings):
    """The stored tree import_op.keep_stored_tree packed on the object, while its settings are still those imported."""
    raw = obj.get("bryce_stored_tree")
    if not raw:
        return None
    import base64, json, zlib
    try:
        kept = json.loads(zlib.decompress(base64.b64decode(raw)))
    except (ValueError, zlib.error):
        return None
    if kept.get("held") != settings:
        return None
    return kept["stored"], kept.get("stored_id")


def group_to_dict(group):
    """A PropertyGroup as a plain dict by its settings' names (enum identifiers as strings, arrays as lists)."""
    out = {}
    for prop in group.bl_rna.properties:
        key = prop.identifier
        if key in ("rna_type", "name") or key.endswith("_menu") or key in names.SWITCH_VIEWS:
            continue  # a `<setting>_menu` or a named-bit switch only shows and sets a setting's number (node_menus.py)
        value = getattr(group, key)
        if prop.type == "POINTER":
            out[key] = group_to_dict(value) if isinstance(value, bpy.types.PropertyGroup) else value
        elif prop.type == "COLLECTION":
            out[key] = [group_to_dict(item) for item in value]
        elif getattr(prop, "is_array", False):
            out[key] = list(value)
        else:
            out[key] = value
    return out


def find_node(tree, idname):
    if tree is None:
        return None
    for node in tree.nodes:
        if node.bl_idname == idname:
            return node
    return None


def linked_from(socket):
    """The node feeding `socket`, through node groups (group_links); a broken group is a GroupLinkError."""
    return group_links.linked_from(socket)


def sky_group(world):
    """: the Bryce Sky node's `settings`, else the World's `bryce` group."""
    node = find_node(world.node_tree if world and world.use_nodes else None, "BryceSkyNode")
    if node is not None:
        return node.settings
    return getattr(world, names.DATA_GROUP) if world is not None else None


def sun_axis(depsgraph, sky, first_sun_axis):
    """The sun lamp's own z axis (`sun_object`, else the graph's first sun lamp), Blender's axes.

    `sun_object` is read as evaluated (BS-1 4.1, 1.8); `first_sun_axis` is the first SUN item's, copied during
    the pass over the graph (BS-1 1.2)."""
    lamp = sky.sun_object
    if lamp is not None:
        return list(lamp.evaluated_get(depsgraph).matrix_world.col[2][:3])
    return first_sun_axis


# Task T1: Blender's factory scene has no sun lamp, and its `sun_direction` default is the zero vector, which
# convert.sky_and_sun must refuse (a direction of no length). A sensible default instead, so a scene with no
# sun renders: Blender's axes, z up — 45 degrees of elevation, tipped toward -Y. Our choice under
# (render rather than fail); the sky's own sun_direction wins when it is not zero, and a real sun lamp wins
# over both .
DEFAULT_SUN_AXIS = (0.0, -0.7071067811865476, 0.7071067811865476)


def material_diffuse(material):
    """A Bryce material's diffuse colour A (tint), from its Bryce Material node or its `bryce` group."""
    if material is None:
        return None
    node = find_node(material.node_tree if material.use_nodes else None, "BryceMaterialNode")
    if node is not None:
        return list(node.inputs["diffuse_color"].value)
    return list(getattr(material, names.DATA_GROUP).diffuse_color.a)


def material_is_gel(material):
    """A material with textures (a gel): a linked Bryce Material input, or texture components."""
    if material is None:
        return False
    node = find_node(material.node_tree if material.use_nodes else None, "BryceMaterialNode")
    if node is not None:
        return any(socket.is_linked for socket in node.inputs)
    return len(getattr(material, names.DATA_GROUP).components) > 0


def light_settings(light):
    """: the Bryce Light node's `settings`, its own material's diffuse A and whether that is a gel;
    else the lamp's `bryce` group."""
    node = find_node(light.node_tree if light.use_nodes else None, "BryceLightNode")
    if node is not None:
        settings = node.settings
        try:
            own = linked_from(node.inputs["material"])
        except group_links.GroupLinkError:
            own = None  # a broken group: the light's own settings stand
        if own is not None and own.bl_idname == "BryceMaterialNode":
            gel = any(socket.is_linked for socket in own.inputs)
            return group_to_dict(settings), list(own.inputs["diffuse_color"].value), gel
        return group_to_dict(settings), material_diffuse(settings.material), material_is_gel(settings.material)
    settings = getattr(light, names.DATA_GROUP)
    return group_to_dict(settings), material_diffuse(settings.material), material_is_gel(settings.material)


def lamp_record(data):
    """A Blender lamp as the plain dict lamps.describe_lamp takes (Blender's own names)."""
    record = {"type": data.type}
    if data.type == "SPOT":
        record.update(spot_size=data.spot_size, use_square=data.use_square)
    elif data.type == "AREA":
        record.update(shape=data.shape, size=data.size, size_y=data.size_y)
    return record


TEXTURE_NODES = ("BryceTextureNode", "BrycePictureNode")
MIX_NODES = ("BryceMixColorNode", "BryceMixValueNode")


def socket_value(socket):
    value = socket.value
    return list(value) if socket.bl_idname == "BryceColorSocket" else value


def texture_column(node, columns, material, problems):
    """A texture node's `column` , noted once; two nodes of one column are refused."""
    column = int(node.column)
    clash = channels.column_clash(material.name, columns, column, node.key)
    if clash:
        problems.append(clash)
    columns.setdefault(("node", column), node)  # the node itself, read off once (it may be inside a group)
    return column


def channel_link(socket, columns, material, problems, notices):
    """The plain link channels.textured_material takes, for a linked Bryce Material input ; the link is
    read through node groups (group_links), a broken group noted as a problem."""
    try:
        return _channel_link(socket, columns, material, problems, notices)
    except group_links.GroupLinkError as error:
        problems.append("material %r: channel %s: %s" % (material.name, socket.identifier, error))
        return None


def _channel_link(socket, columns, material, problems, notices):
    link = socket.links[0]
    source = link.from_node
    if source.bl_idname in TEXTURE_NODES:
        notice = channels.output_notice(material.name, socket.identifier, None, link.from_socket.identifier)
        if notice:
            notices.append(notice)  # read as the named output
        return {"texture": texture_column(source, columns, material, problems),
                "output": channels.named_output(socket.identifier)}
    if source.bl_idname in MIX_NODES:
        out = {"mix": source.by}
        for key in ("a", "b", "alpha"):
            side = {"value": socket_value(source.inputs[key])}
            feeding = linked_from(source.inputs[key])
            if feeding is not None:
                if feeding.bl_idname in TEXTURE_NODES:
                    notice = channels.output_notice(material.name, socket.identifier, key,
                                                    source.inputs[key].links[0].from_socket.identifier)
                    if notice:
                        notices.append(notice)
                    side["texture"] = texture_column(feeding, columns, material, problems)
                else:
                    problems.append("material %r: %s's input %s is fed by %s, not a Bryce texture"
                                    % (material.name, source.name, key, feeding.bl_idname))
            out[key] = side
        return out
    problems.append("material %r: channel %s is fed by %s, not a Bryce texture or mix"
                    % (material.name, socket.identifier, source.bl_idname))
    return None


def picture_entry(node, name):
    """: a Bryce Picture node as the description's texture: its image's pixels (Blender's floats, the bottom
    row first) and its Bryce Mapping, or the default mapping where none is linked."""
    problem = texture_nodes.picture_problem(node)
    if problem:
        raise ValueError(problem)
    image = node.image
    width, height = image.size
    rgba = list(image.pixels)[:4 * width * height]  # Blender's pixels are always r, g, b, a
    mapping = texture_nodes.mapping_of(node.inputs["vector"])
    return pictures.picture_texture(name, width, height, rgba, mapping)


def texture_entry(tex_node, name, what, problems):
    """: a Bryce Texture or Bryce Picture node as the description's texture called `name`, or None with a
    problem noted (`what` says whose it is)."""
    if tex_node.bl_idname == "BrycePictureNode":
        try:
            return picture_entry(tex_node, name)
        except (texture_nodes.TextureGraphError, ValueError) as error:
            problems.append(texture_nodes.fault_message(what, tex_node, error))
            return None
    try:
        entry = texture_settings.texture_settings(texture_nodes.texture_input(tex_node))
    except (texture_nodes.TextureGraphError, KeyError, ValueError) as error:
        problems.append(texture_nodes.fault_message(what, tex_node, error))
        return None
    return dict(entry, name=name, kind="procedural")


def cloud_textures(world, sky_out, textures, problems):
    """(Sky cloud textures): a Bryce Texture or Bryce Picture linked to the
    Bryce Sky node's input `stratus` (upper layer) or `cumulus` (lower) is the layer's cloud texture; it joins the
    description's `textures` as `sky:stratus` or `sky:cumulus`, and the sky's `stratus_texture` or
    `cumulus_texture`  is its place there."""
    node = find_node(world.node_tree if world is not None and world.use_nodes else None, "BryceSkyNode")
    if node is None or sky_out is None:
        return
    for layer in ("stratus", "cumulus"):
        try:
            source = linked_from(node.inputs[layer])
        except group_links.GroupLinkError as error:
            problems.append("sky: the %s layer: %s" % (layer, error))
            continue
        if source is None:
            continue
        if source.bl_idname not in TEXTURE_NODES:
            problems.append("sky: the %s layer is fed by %s, not a Bryce texture" % (layer, source.bl_idname))
            continue
        entry = texture_entry(source, "sky:" + layer, "sky %s layer" % layer, problems)
        if entry is not None:
            textures.append(entry)
            sky_out[layer + "_texture"] = len(textures) - 1


def material_from_node(material, node, name, problems, notices, textures):
    """: one Bryce Material node, plain or with textures and mixes linked to its channels , as the
    description's material. `name` is the texture-name stem (the material's name; a mix's second material has a
    stem of its own so that the two materials' columns do not clash, Mix Materials feed)."""
    node = group_links.view(node)  # its inputs read through node groups
    settings = {k: getattr(node, k) for k in ("flags", "refraction", "volume_edge", "volume_sharpness",
                                              "volume_quality")}
    settings["volume_color"] = list(node.volume_color)
    inputs = {socket.identifier: socket_value(socket) for socket in node.inputs}
    links, columns = {}, {}
    stem = type("Stem", (), {"name": name})()  # channel_link and texture_column read only `.name`
    for socket in node.inputs:
        if not socket.is_linked:
            continue
        try:
            has_link = bool(socket.links)
        except group_links.GroupLinkError as error:
            problems.append("material %r: channel %s: %s" % (name, socket.identifier, error))
            continue
        if has_link:
            link = channel_link(socket, columns, stem, problems, notices)
            if link is not None:
                links[socket.identifier] = link
    if not links:
        return convert.material_settings(settings, inputs)
    described = channels.textured_material(settings, inputs, links)
    # : each texture a channel names is read off its nodes once and
    # put in the description's `textures` under its material's name and column.
    for column in described["textures"]:
        entry = texture_entry(columns[("node", column)], channels.texture_name(name, column),
                              "material %r" % material.name, problems)
        if entry is not None:
            textures.append(entry)
    return channels.describe_references(described, name)


def mix_root(tree):
    """The Bryce Mix Materials node that makes the material: the one whose output is linked, else the first
    one; None where the tree has none."""
    if tree is None:
        return None
    found = [n for n in tree.nodes if n.bl_idname == "BryceMixMaterialsNode"]
    for node in found:
        if node.outputs["material"].is_linked:
            return node
    return found[0] if found else None


def mix_material(material, mix, problems, notices, textures):
    """(Mix Materials feed): a Bryce Mix Materials node as the description's
    material: `first` as the material, `second` as `mix` (13 amounts all the node's `amount`, the second's
    channels in `colors` and `values`; spec's `materials[].mix`)."""
    ends = []
    for key in ("first", "second"):
        try:
            source = linked_from(mix.inputs[key])
        except group_links.GroupLinkError as error:
            problems.append("material %r: Bryce Mix Materials input %s: %s" % (material.name, key, error))
            source = None
        if source is not None and source.bl_idname != "BryceMaterialNode":
            problems.append("material %r: Bryce Mix Materials input %s is fed by %s, not a Bryce Material "
                            "(nested mixes are not read)" % (material.name, key, source.bl_idname))
            source = None
        ends.append(source)
    first, second = ends
    if first is None and second is None:
        problems.append("material %r: Bryce Mix Materials has no Bryce Material linked" % material.name)
        return new_material_settings()
    if first is None or second is None:
        # one end only: the mix is that material (the missing side has nothing to mix in)
        return material_from_node(material, first or second, material.name, problems, notices, textures)
    one = material_from_node(material, first, material.name, problems, notices, textures)
    two = material_from_node(material, second, material.name + "#2", problems, notices, textures)
    return mix_described(one, two, float(mix.amount))


def has_textures(described):
    """Whether any channel of a described material is more than a plain number (a texture or a mix on it)."""
    return any(isinstance(entry, dict) for entry in described["channels"].values())


def lerp(a, b, m):
    """One channel's A from `a` to `b` by `m` (a list of three for a colour, else a number)."""
    if isinstance(a, (list, tuple)):
        return [single(x + (y - x) * m) for x, y in zip(a, b)]
    return single(a + (b - a) * m)


def mix_described(one, two, amount):
    """(Mix Materials feed). The renderer evaluates a second material (spec
    ) only on a material that has a texture of its own, so: both plain, the numbers are mixed here (the
    result has no `mix`); the first with textures, `two` is its `mix`; only the second with textures, the two swap
    places and the amount becomes 1 - amount. Flags and volume numbers stay the first material's, but for
    the refraction and the volume's colour and numbers, which are mixed by the same amount."""
    textured_one, textured_two = has_textures(one), has_textures(two)
    out = dict(one)
    for key in ("refraction",):
        out[key] = lerp(one[key], two[key], amount)
    out["volume_colour"] = lerp(one["volume_colour"], two["volume_colour"], amount)
    out["volume_numbers"] = lerp(one["volume_numbers"], two["volume_numbers"], amount)
    if not textured_one and not textured_two:
        out["channels"] = {c: lerp(one["channels"][c], two["channels"][c], amount) for c in names.CHANNELS}
        return out
    base, other, share = (one, two, amount) if textured_one else (two, one, 1.0 - amount)
    out["channels"] = base["channels"]
    out["mix"] = {
        "amounts": [share] * len(names.CHANNELS),
        "colors": [other["channels"][c] for c in names.COLOUR_CHANNELS],
        "values": [other["channels"][c] for c in names.VALUE_CHANNELS],
    }
    return out


def material_plain(material, problems, notices, textures):
    """: a Bryce Material node, plain or with textures and mixes linked to its channels , or a
    Bryce Mix Materials node of two of them."""
    tree = material.node_tree if material.use_nodes else None
    mix = mix_root(tree)
    if mix is not None:
        return mix_material(material, mix, problems, notices, textures)
    node = find_node(tree, "BryceMaterialNode")
    if node is not None:
        return material_from_node(material, node, material.name, problems, notices, textures)
    if not material.is_property_set(names.DATA_GROUP):
        # : a material with neither a Bryce Material node nor a `bryce` group ever written
        # (a plain Blender material, the factory scene's "Material") holds none of the settings, so each takes
        # the value of the original's new material; the group's own zeros are not settings the file holds.
        return new_material_settings()
    group = group_to_dict(getattr(material, names.DATA_GROUP))
    inputs = {c: group[c]["a"] for c in names.CHANNELS}
    if group.get("components"):
        problems.append("material %r has texture components (not supported yet)" % material.name)
    return convert.material_settings(group, inputs)


def new_material_settings():
    """'s new material (defaults.NEW_MATERIAL) as convert.material_settings takes a material's settings."""
    new = defaults.NEW_MATERIAL
    settings = {k: new[k] for k in ("flags", "refraction", "volume_edge", "volume_sharpness", "volume_quality")}
    settings["volume_color"] = list(new["volume_color"])
    inputs = {c: list(v) for c, v in new["colours"].items()}
    inputs.update(new["values"])
    return convert.material_settings(settings, inputs)


def placement_flags(obj):
    """: the three flag words of the placement an import kept on the object (`bryce_flags`), or None."""
    kept = obj.get("bryce_flags")
    if kept is None or len(kept) != 3:
        return None
    return [int(w) & 0xFFFFFFFF for w in kept]


def light_material(light):
    """The Blender material of a lamp's own material , or None."""
    node = find_node(light.node_tree if light.use_nodes else None, "BryceLightNode")
    if node is not None:
        try:
            own = linked_from(node.inputs["material"])
        except group_links.GroupLinkError:
            own = None
        if own is not None:
            return None  # a material node wired in carries no Blender material to name
        return node.settings.material
    return getattr(light, names.DATA_GROUP).material


def slot_material_name(obj, index, materials, problems, notices, textures):
    """The name of the material in the object's slot `index` (BS-1 2.6), read into `materials` once; "" for none."""
    slots = getattr(obj, "material_slots", ())
    if index >= len(slots) or slots[index].material is None:
        return ""
    material = slots[index].material
    if material.name not in materials:
        materials[material.name] = material_plain(material, problems, notices, textures)
    return material.name


def matrix_of(obj, matrix_world):
    m = [list(row) for row in matrix_world]
    shear = obj.get("bryce_shear")
    if shear is not None and obj.type != "LIGHT":
        flat = [float(v) for v in shear]
        m = mat4_mul(m, [flat[0:4], flat[4:8], flat[8:12], flat[12:16]])
    return m


def placed_record(obj, shape, matrix):
    """An object of the description as solids.describe_groups takes it (parenting, role, cut switch)."""
    return {"name": obj.name, "bryce_shape": shape, "matrix": matrix,
            "parent": obj.parent.name if obj.parent is not None else None,
            "bryce_role": props.legacy_get(obj, "bryce_role"),
            "bryce_cut_shows_positive": props.legacy_get(obj, "bryce_cut_shows_positive")}


def terrain_of(obj):
    """: the terrain's heights from its picture's red channel (Blender's floats, bottom row first)."""
    settings = getattr(obj, names.OBJECT_TERRAIN_GROUP)
    image = settings.image
    if image is None:
        raise ValueError("the terrain has no picture")
    width, height = image.size
    per_pixel = image.channels
    if width * height == 0:
        raise ValueError("the terrain's picture %r is missing or empty" % image.name)
    buffer = array("f", bytes(4 * width * height * per_pixel))
    image.pixels.foreach_get(buffer)
    red = buffer[0::per_pixel]
    plain = {k: v for k, v in group_to_dict(settings).items() if k != "image"}
    return solids.terrain(plain, width, height, red)


def evaluated_camera(depsgraph):
    """The scene's camera as evaluated (BS-1 4.1), or None."""
    camera = depsgraph.scene_eval.camera
    if camera is None and depsgraph.scene.camera is not None:
        camera = depsgraph.id_eval_get(depsgraph.scene.camera)
    return camera


def camera_record(data):
    """A camera data-block as the plain dict camera_frame takes (Blender's property names, BS-1 4.3 to 4.9)."""
    record = {k: getattr(data, k) for k in ("type", "lens", "sensor_width", "sensor_height", "sensor_fit",
                                            "shift_x", "shift_y", "ortho_scale", "clip_start", "clip_end")}
    # Outputs : the panoramic type and a central cylindrical camera's longitude range
    for key in ("panorama_type", "central_cylindrical_range_u_min", "central_cylindrical_range_u_max"):
        if hasattr(data, key):
            record[key] = getattr(data, key)
    return record


def picture_record(render):
    """The picture's size and pixel aspects (BS-1 4.4)."""
    return {k: getattr(render, k) for k in ("resolution_x", "resolution_y", "pixel_aspect_x", "pixel_aspect_y",
                                            "resolution_percentage")}


def exact_camera(obj, m, record):
    """Puts the document's own numbers back into the camera's matrix `m` and `record` (in place) where the camera
    still holds the singles the import gave it (import_op.keep_exact_camera); a camera the user moved or changed
    is read as Blender holds it."""
    import json
    raw = obj.get("bryce_exact_camera")
    if not raw:
        return None
    try:
        kept = json.loads(raw)
        exact, held = kept["exact"], kept["held"]
        now = [v for row in obj.matrix_world for v in row] + [obj.data.lens, obj.data.sensor_width, obj.data.shift_x,
                                                              obj.data.shift_y]
        if len(exact) != 20 or len(held) != 20 or any(a != b for a, b in zip(now, held)):
            return None
    except (ValueError, KeyError, TypeError):
        return None
    for i in range(4):
        m[i] = list(exact[4 * i:4 * i + 4])
    record["lens"], record["sensor_width"], record["shift_x"], record["shift_y"] = exact[16:20]
    return kept


def describe_camera(depsgraph, units, problems, program="5.0"):
    """The description's camera (BS-1 section 4), or None with a problem noted."""
    obj = evaluated_camera(depsgraph)
    if obj is None:
        return None
    m = [list(row) for row in obj.matrix_world]
    record = camera_record(obj.data)
    kept = exact_camera(obj, m, record)
    try:
        view = camera_frame.renderer_view(record, picture_record(depsgraph.scene.render), program, units)
    except ValueError as error:
        problems.append("camera %r: %s" % (obj.name, error))
        return None
    camera = {"position": to_bryce_point([m[0][3], m[1][3], m[2][3]], units),
              "forward": to_bryce_direction([-m[0][2], -m[1][2], -m[2][2]], "the camera's view"),  # BS-1 4.2
              "up": to_bryce_direction([m[0][1], m[1][1], m[2][1]], "the camera's up"),  # BS-1 4.2
              "type": record["type"], "lens": record["lens"], "sensor_width": record["sensor_width"]}
    camera.update(view)
    if kept and kept.get("view_rows"):
        # the document's view rows (maybe scaled) and centre, while the camera is as the import left it
        camera["view_rows"], camera["view_centre"] = kept["view_rows"], kept["view_centre"]
        if kept.get("plane_height") and record["type"] == "PERSP":
            camera["plane_height"] = kept["plane_height"]
    return camera


FLOAT_MAX = 3.4028234663852886e38


def restore_exact_numbers(world, sky_dict):
    """The infinities and NaNs an import kept (import_op.world_for): a Blender float property clamps an infinity to
    the largest single and may not hold a NaN, so where the property still holds that clamp the document's own number
    goes back. A number the user has changed since stays as they set it."""
    raw = world.get("bryce_exact_numbers") if world is not None else None
    if not raw:
        return
    import json
    try:
        exact = json.loads(raw)
    except ValueError:
        return
    for key, values in exact.items():
        held = sky_dict.get(key)
        if held is None or len(held) != len(values):
            continue
        sky_dict[key] = [v if (math.isinf(v) and abs(h) == FLOAT_MAX and (h < 0) == (v < 0)) or (math.isnan(v) and math.isnan(h))
                         else h for h, v in zip(held, values)]


def unchanged_exact_sky(world, sky_dict):
    """The document's binary64 sky numbers (import_op.keep_exact_sky) whose property still holds the single the
    import gave: setting -> number or list. A number the user has changed since stays as they set it."""
    raw = world.get("bryce_exact_doubles") if world is not None else None
    if not raw:
        return {}
    import json
    try:
        kept = json.loads(raw)
    except ValueError:
        return {}
    out = {}
    for setting, entry in kept.items():
        if setting not in convert.SKY_DOUBLES:
            continue
        held = sky_dict.get(setting)
        want = entry.get("held")
        if held is None:
            continue
        same = ([float(h) for h in held] == want) if isinstance(want, list) else (float(held) == want)
        if same:
            out[setting] = entry["exact"]
    return out


def unchanged_exact_sun(world, axis):
    """The document's own sun direction (import_op.keep_exact_sun) while the sun lamp stands as the import left it."""
    raw = world.get("bryce_exact_sun") if world is not None else None
    if not raw or axis is None:
        return None
    import json
    try:
        kept = json.loads(raw)
    except ValueError:
        return None
    if [float(c) for c in axis] != kept.get("held"):
        return None
    return kept["direction"]


def describe(depsgraph, notices=None, view_camera=False):
    """The description of the evaluated scene, and the list of problems that refuse the render (Errors).
    `view_camera` True: the Rendered viewport asks (The Rendered viewport); the scene's
    camera is neither read nor refused, the description's camera is left None for the viewport's own."""
    scene = depsgraph.scene
    problems = []
    notices = [] if notices is None else notices  # messages that do not refuse the render
    textures = []  # the description's textures , filled as the materials are read
    render_group = group_to_dict(getattr(scene, names.SCENE_RENDER_GROUP))
    units = render_group["units"]
    sky = sky_group(scene.world)
    if sky is None:
        problems.append("the scene has no World with a Bryce sky")
        return None, problems
    sky_dict = group_to_dict(sky)
    restore_exact_numbers(scene.world, sky_dict)
    for unset in ("sky5_haze_offset", "sky5_haze_thickness", "sky5_fog_offset"):
        if not sky.is_property_set(unset):
            sky_dict.pop(unset, None)
    # Errors (a cloud layer showing with no texture) may be refused; which flag bits make a layer show is
    # not stated in, so the check is not made here (design note).
    camera_focus = None
    camera_obj = None if view_camera else evaluated_camera(depsgraph)
    if camera_obj is not None and camera_obj.data.dof.use_dof:
        cam = camera_obj.data
        # focus CAMERA: our choice (design note): lens radius = focal length / (2 f-stop), metres.
        camera_focus = (cam.dof.focus_distance, (cam.lens / 1000.0) / (2.0 * cam.dof.aperture_fstop))
    render = convert.render_request(render_group, sky_dict["scene_flags"], camera_focus)
    program = render_group["program"]

    lights, materials, objects, trees = [], {}, [], []
    placed = []  # every object of the description, as solids.describe_groups takes them (groups)
    first_sun_axis = None
    first_sun_colour = None
    # One pass; every number is copied out while its item is current (BS-1 1.2). Placements are the item's
    # own matrix_world, never the object's (BS-1 1.2, 1.6, 1.7).
    for instance in depsgraph.object_instances:
        obj = instance.object
        matrix = matrix_of(obj, instance.matrix_world)
        if obj.type == "LIGHT":
            kind = obj.data.type
            if kind == "SUN":
                if first_sun_axis is None:
                    first_sun_axis = [matrix[i][2] for i in range(3)]  # its own z axis (BS-1 3.1)
                    first_sun_colour = [float(c) for c in obj.data.color]
                continue
            settings, own, gel = light_settings(obj.data)
            if int(settings.get("flags") or 0) == 0:
                # the same rule as the sun's: a lamp placed is described to be lit
                settings["flags"] = LIGHT_ON
            try:
                # : POINT radial, SPOT cone or pyramid, AREA cylinder or box. A point lamp's
                # place is the item matrix' translation (BS-1 5.9), which describe_lamp takes.
                light = lamps.describe_lamp(lamp_record(obj.data), settings, matrix, units, program, own, gel)
                light["name"] = obj.name
                # The light's own object is a shape when it shows or is held by a group (wants its material);
                # the group links it by name like any member . (Lights as objects)
                own_material = light_material(obj.data)
                if own_material is not None:
                    if own_material.name not in materials:
                        materials[own_material.name] = material_plain(own_material, problems, notices, textures)
                    light["material"] = own_material.name
                if placement_flags(obj):
                    light["placement_flags"] = placement_flags(obj)
                placed.append(placed_record(obj, "light", matrix))
                lights.append(light)
            except ValueError as error:
                problems.append("lamp %r: %s" % (obj.name, error))
            continue
        shape = props.legacy_get(obj, "bryce_shape")
        if shape:
            entry = {"name": obj.name, "shape": shape, "matrix": matrix, "role": props.legacy_get(obj, "bryce_role"),
                     "cut_shows_positive": props.legacy_get(obj, "bryce_cut_shows_positive"),
                     "negative": props.legacy_get(obj, "bryce_negative")}
            if placement_flags(obj):
                entry["placement_flags"] = placement_flags(obj)
            if shape == "torus":
                entry["torus"] = solids.torus_radii(obj.get("bryce_torus", (0.0, 0.0)))
            if shape not in ("metaball", "group", "terrain", "tree"):
                entry["world"] = solids.object_transform(shape, matrix, units)
            placed.append(placed_record(obj, shape, matrix))
            if shape != "group":
                # (what an import makes: an object's material is its first slot, a tree's leaves'
                # material its second); (The add-on sends what the reader takes)
                slot = slot_material_name(obj, 0, materials, problems, notices, textures)
                if slot:
                    entry["material"] = slot
            if shape == "tree":
                settings = convert.tree_settings(group_to_dict(getattr(obj, names.OBJECT_TREE_GROUP)))
                tree_entry = {"name": obj.name, "matrix": matrix, "tree": settings}
                kept = kept_stored_tree(obj, settings)
                if kept is not None:  # an imported document's tree, settings unchanged: drawn as stored
                    del tree_entry["tree"]
                    tree_entry["stored"] = kept[0]
                    if kept[1] is not None:  # trees of one stored tree  name the same number
                        tree_entry["stored_id"] = kept[1]
                    tree_entry["world"] = solids.own_transform(matrix, units)
                if placement_flags(obj):
                    tree_entry["placement_flags"] = placement_flags(obj)
                if entry.get("material"):
                    tree_entry["material"] = entry["material"]
                leaves = slot_material_name(obj, 1, materials, problems, notices, textures)
                if leaves:
                    tree_entry["leaves_material"] = leaves
                trees.append(tree_entry)
                continue
            if shape not in names.SHAPES:
                problems.append("object %r: shape %r is not one of Bryce's" % (obj.name, shape))
            elif shape == "metaball":  # : its own transform and sign
                entry.update(solids.metaball(matrix, units, props.legacy_get(obj, "bryce_negative")))
            elif shape == "group":  # : its own transform; its members are filled in below
                entry["world"] = solids.own_transform(matrix, units)
            elif shape == "terrain":  # : the heights from the picture
                try:
                    entry["terrain"] = terrain_of(obj)
                    # : the own box of is 8 by 8 by 2 Blender units at scale 1
                    entry["world"] = create.terrain_world(matrix, entry["terrain"]["size"],
                                                          entry["terrain"]["mirrored"], units)
                except ValueError as error:
                    problems.append("object %r: %s" % (obj.name, error))
            objects.append(entry)
            continue
        if obj.get("bryce_proxy") == "tree":
            # A document's tree the earlier add-on holds as a proxy mesh: the grown tree itself is in the scene's
            # kept description (Legacy properties).
            stored = props.kept_tree(scene, obj)
            if stored is not None:
                tree_entry = {"name": obj.name, "matrix": matrix, "stored": stored, "world": solids.own_transform(matrix, units)}
                wood = slot_material_name(obj, 0, materials, problems, notices, textures)
                if wood:
                    tree_entry["material"] = wood
                leaves = slot_material_name(obj, 1, materials, problems, notices, textures)
                if leaves:
                    tree_entry["leaves_material"] = leaves
                trees.append(tree_entry)
                continue
        if not scene_read.gives_geometry(obj.type, instance.show_self):  # BS-1 1.7, 2.1
            continue
        mesh = obj.to_mesh()  # BS-1 2.1: the evaluated mesh, modifiers applied
        try:
            mesh.calc_loop_triangles()
            vertices = [tuple(v.co) for v in mesh.vertices]  # BS-1 2.2: object space
            slots = obj.material_slots
            parts = {}
            corner_normals = [n.vector[:] for n in mesh.corner_normals]
            uv_layer = mesh.uv_layers.active
            corner_uvs = [d.uv[:] for d in uv_layer.data] if uv_layer is not None else None
            shading = {}
            for tri in mesh.loop_triangles:  # BS-1 2.3: Blender's tessellation, not rederived
                slot = scene_read.slot_index(tri.material_index, len(slots))
                parts.setdefault(slot, []).append(tuple(tri.vertices))
                shading.setdefault(slot, []).append(
                    (bool(tri.use_smooth), tuple(corner_normals[i] for i in tri.loops),
                     None if corner_uvs is None else tuple(corner_uvs[i] for i in tri.loops)))
            for index, triangles in sorted(parts.items(), key=lambda kv: -1 if kv[0] is None else kv[0]):
                material = slots[index].material if index is not None else None  # BS-1 2.6
                name = material.name if material else ""
                if material is not None and name not in materials:
                    materials[name] = material_plain(material, problems, notices, textures)
                part = {"name": obj.name, "shape": "mesh", "material": name,
                        "triangles": convert.mesh_triangles(units, matrix, vertices, triangles)}
                part.update(convert.mesh_shading(matrix, triangles, shading[index]))
                if "normals" in part or "uvs" in part:  # the mesh's place, so object-space textures read own points
                    part["origin"] = convert.mesh_origin(units, matrix)
                if placement_flags(obj):
                    part["placement_flags"] = placement_flags(obj)
                objects.append(part)
            placed.append(placed_record(obj, "mesh", matrix))
        finally:
            obj.to_mesh_clear()

    # : each group's members are the objects of the description it parents.
    groups = solids.describe_groups(placed, units)
    for entry in objects:
        if entry.get("shape") == "group" and entry["name"] in groups:
            entry["members"] = groups[entry["name"]]["members"]

    axis = sun_axis(depsgraph, sky, first_sun_axis)
    if axis is None and not any(float(v) for v in (sky_dict.get("sun_direction") or ())):
        # No sun lamp and no sun direction (the factory scene): render anyway, with the default sun.
        # If its intensity is also the zero default the sky would come out black, so the default sun
        # brings its light with it. Any of these the user has set wins over the default.
        axis = list(DEFAULT_SUN_AXIS)
        if not float(sky_dict.get("sun_intensity", 0.0)):
            sky_dict["sun_intensity"] = 1.0
            sky_dict["sun_color"] = [1.0, 1.0, 1.0]
    explicit_zero = scene.world is not None and bool(scene.world.get("bryce_sun_flags_explicit"))
    if int(sky_dict.get("sun_flags") or 0) == 0 and not explicit_zero:
        # sun_flags unset (a real document's sun word is never 0): the add-on describes a sun to be
        # lit, so it asks with the light's on bit, and with the fall-off bit cleared as every saved
        # document's sun has it (on alone the light falls off to nothing). A word the user
        # holds wins as it is.
        sky_dict["sun_flags"] = SUN_DEFAULT
    sky_sun = convert.sky_and_sun(sky_dict, axis)
    exact_sun = unchanged_exact_sun(scene.world, axis)
    if exact_sun is not None:
        sky_sun["sun"]["direction"] = exact_sun
    convert.exact_sky(sky_sun, unchanged_exact_sky(scene.world, sky_dict))
    camera = None if view_camera else describe_camera(depsgraph, units, problems, program)
    # The viewport's camera comes from Blender's view matrices (singles); where the view is the scene camera's, the
    # document's own doubles are the better picture (viewport_camera.exact_view). Kept apart, never sent on.
    viewport_scene_camera = None
    if view_camera and evaluated_camera(depsgraph) is not None:
        spare = []
        viewport_scene_camera = describe_camera(depsgraph, units, spare, program)
    description = convert.scene_description(render, sky_sun, lights, materials, objects, trees)
    if first_sun_colour is not None:
        description["sun"]["light_colour"] = first_sun_colour  # the sun lamp's colour: the light part's (wash)
    cloud_textures(scene.world, description["sky"], textures, problems)
    description["textures"] = textures
    description["camera"] = camera
    if viewport_scene_camera is not None:
        description["viewport_scene_camera"] = viewport_scene_camera
    return description, problems
