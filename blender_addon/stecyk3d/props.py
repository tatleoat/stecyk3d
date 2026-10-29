# SPDX-License-Identifier: GPL-3.0-or-later
# ,, (material, texture, filter, colour map,
# noise groups),
# (Settings under's names; needs Blender, not testable here)
"""Blender PropertyGroups whose names and types are's, so a file of the earlier add-on reads into them."""

import bpy

from . import defaults, names, node_menus

# A table row: (name, type, extra). Types as: float, float[n], colour, int, int[n], word, word[n], bool,
# enum (extra: the identifier -> number table), string, path (extra: "FILE_PATH" or "DIR_PATH"), image,
# material, object, group (extra: the group's table key), list (extra: the group's table key).

FILTER = [("kind", "word", None), ("n", "int", None), ("a", "float", None), ("b", "float", None)]
COLOUR_MAP = [("mode", "word", None), ("first", "word", None), ("second", "word", None),
              ("third", "word", None)]
NOISE = [("kind", "word", None), ("method", "word", None), ("count", "int", None),
         ("dims", "int", None)]
COMPONENT = [
    ("flags", "word", None), ("space", "word", None), ("bump", "bool", None), ("matrix", "float[9]", None),
    ("phase_matrix", "float[9]", None), ("phase_amount", "float", None), ("custom", "bool", None),
    ("phase_custom", "bool", None), ("noise", "group", "noise"), ("phase_noise", "group", "noise"),
    ("filter", "group", "filter"), ("colors", "group", "colour_map"),
]
RECORD = [
    ("count", "word", None), ("outputs", "word", None), ("blend", "word[2]", None), ("color_space", "word", None),
    ("filter", "group", "filter"), ("colors", "group", "colour_map"), ("has_phase", "bool", None),
    ("phase_amount", "float", None), ("phase_matrix", "float[9]", None), ("phase_noise", "group", "noise"),
    ("phase_custom", "bool", None), ("phase_with_offsets", "bool", None),
    ("c0", "group", "component"), ("c1", "group", "component"), ("c2", "group", "component"),
]
TEXTURE = [
    ("mode", "word", None), ("flags", "word", None), ("scale", "float[3]", None), ("offset", "float[3]", None),
    ("rotation", "float[4]", None), ("label", "string", None), ("is_picture", "bool", None),
    ("image", "image", None), ("record", "group", "record"), ("exact", "string", None),
    ("show_raw", "bool", None),
]
RENDER = [
    ("program", "enum", names.PROGRAM), ("antialias", "enum", names.ANTIALIAS), ("super_rays", "int", None),
    ("premium_rays", "int", None), ("tir_depth", "int", None), ("spatial_kind", "enum", names.SPATIAL_KIND),
    ("spatial_level", "enum", names.SPATIAL_LEVEL), ("focus", "enum", names.FOCUS),
    ("focal_distance", "float", None), ("lens_radius", "float", None), ("depth", "int", None),
    ("units", "float", None), ("preview", "bool", None), ("cores", "enum", names.CORES),
    ("viewport_pixel_size", "enum", names.VIEWPORT_PIXEL_SIZE),
    ("viewport_quality", "enum", names.VIEWPORT_QUALITY), ("data_folder", "path", "DIR_PATH"),
    ("presets_folder", "path", "DIR_PATH"), ("exact_png", "path", "FILE_PATH"),
    ("keep_description", "path", "FILE_PATH"), ("renderer", "path", "FILE_PATH"),
    ("reader", "path", "FILE_PATH"), ("presets_tool", "path", "FILE_PATH"), ("terrain_tool", "path", "FILE_PATH"),
    # Not: the add-on's own switch, kept out of the description so
    # the settings vectors never see it. Default on — the live picture is the default way to render.
    ("progressive", "bool", None), ("live_order", "enum", names.LIVE_ORDER),
    ("wash_grain", "enum", names.WASH_GRAIN),
    # Finishing (finishing.py): the 90s treatment done to the picture after the render, not Bryce's; all off by
    # default, and kept out of the description like the rows above.
    ("finish_threshold", "float", None), ("glow", "bool", None), ("glow_size", "float", None),
    ("glow_strength", "float", None), ("glow_tint", "colour", None), ("flare", "bool", None),
    ("flare_strength", "float", None), ("jpeg", "bool", None), ("jpeg_quality", "int", None),
    ("palette", "bool", None), ("palette_colours", "int", None),
]
SKY = [
    ("flags", "word", None), ("cloud_height", "float", None), ("cumulus_color", "colour", None),
    ("stratus_color", "colour", None), ("cloud_offset", "float[2]", None), ("cover", "float", None),
    ("cover_amplitude", "float", None), ("cloud_frequency", "float", None), ("detail", "float", None),
    ("detail_z", "float", None), ("zenith", "colour", None), ("horizon", "colour", None),
    ("ambient", "colour", None), ("plain", "colour", None), ("cumulus_shade", "colour", None),
    ("haze_color", "colour", None), ("haze", "float", None), ("fog_color", "colour", None),
    ("fog_height", "float", None), ("fog", "float", None), ("moon", "float[4]", None),
    ("discs", "float[15]", None), ("stars", "word[3]", None), ("has_stratus", "bool", None),
    ("stratus", "group", "texture"), ("has_cumulus", "bool", None), ("cumulus", "group", "texture"),
    ("moon_picture", "image", None), ("sun_object", "object", None), ("sun_direction", "float[3]", None),
    ("sun_flags", "word", None), ("sun_intensity", "float", None), ("sun_color", "colour", None),
    ("sun_glow_color", "colour", None), ("sun_radius", "float", None), ("sun_shadow_ambience", "float", None),
    ("sun_shadow_color", "colour", None), ("scene_flags", "word", None), ("ambient_color", "colour", None),
    ("ambient_amount", "float", None), ("shadow_lift", "float", None), ("sky5", "bool", None),
    ("sky5_flags2", "word", None), ("sky5_haze_offset", "float", None), ("sky5_haze_thickness", "float", None),
    ("sky5_fog_offset", "float", None), ("sky5_haze_shift", "colour", None),
    ("sky5_cloud_detail", "float[2]", None), ("sky5_sun_glow_size", "float", None),
    ("sky5_moon_glow_size", "float", None), ("sky5_sun_disc", "float", None), ("sky5_moon_disc", "float", None),
    ("sky5_air_steps", "int", None), ("sky5_star_intensity", "float", None),
    ("sky5_comet_intensity", "float", None), ("sky5_star_orientation", "float[3]", None),
    ("sky5_lit_air", "float", None),  # (our own setting for the lit air)
]
COLOUR_KEY = [("color", "colour", None), ("position", "float", None), ("midpoint", "float", None)]
VALUE_KEY = [("value", "float", None), ("position", "float", None), ("midpoint", "float", None)]
LIGHT = [
    ("flags", "word", None), ("intensity", "float", None), ("color", "colour", None), ("tint", "bool", None),
    ("material", "material", None), ("edge", "float[2]", None), ("reach", "float", None),
    ("radius", "float", None), ("shadow_ambience", "float", None), ("shadow_color", "colour", None),
    ("range", "float", None), ("gradient_range", "float", None), ("gradient_offset", "float", None),
    ("color_keys", "list", "colour_key"), ("value_keys", "list", "value_key"),
    ("gradient_color_flags", "word", None), ("gradient_value_flags", "word", None),
]
COLOUR_CHANNEL = [
    ("a", "colour", None), ("b", "colour", None), ("alpha", "float", None), ("mode", "enum", names.CHANNEL_MODE),
    ("src_a", "enum", names.SOURCE), ("src_b", "enum", names.SOURCE), ("src_alpha", "enum", names.SOURCE),
]
VALUE_CHANNEL = [
    ("a", "float", None), ("b", "float", None), ("alpha", "float", None), ("mode", "enum", names.CHANNEL_MODE),
    ("src_a", "enum", names.SOURCE), ("src_b", "enum", names.SOURCE), ("src_alpha", "enum", names.SOURCE),
]
MATERIAL = (
    [("flags", "word", None), ("volume_color", "colour", None), ("refraction", "float", None),
     ("volume_edge", "float", None), ("volume_sharpness", "float", None), ("volume_quality", "float", None)]
    + [(c, "group", "colour_channel") for c in names.COLOUR_CHANNELS]
    + [(c, "group", "value_channel") for c in names.VALUE_CHANNELS]
    + [("mix_with", "material", None), ("mix_amount", "float", None), ("components", "list", "texture"),
       ("initialised", "bool", None), ("active_component", "int", None), ("exact", "string", None),
       ("mix", "string", None)]
)
TERRAIN = [
    ("image", "image", None), ("grid", "enum", names.TERRAIN_GRID), ("lower", "float", None),
    ("upper", "float", None), ("mirrored", "bool", None), ("smooth", "bool", None), ("faces", "word", None),
    ("solid_faces", "word", None), ("exact", "string", None),
]
TREE = [
    ("seed", "word", None), ("whorls", "int[2]", None), ("trunk_whorl", "int", None), ("whorl", "int", None),
    ("angle_thick", "float", None), ("angle_thin", "float", None), ("thickness", "float", None),
    ("least", "float", None), ("share", "float[2]", None), ("random", "float", None), ("spread", "float", None),
    ("gravity", "float", None), ("twist", "float", None), ("leaves", "int", None),
    ("leaf_kind", "enum", names.LEAF_KIND), ("leaf_size", "float", None), ("bottom", "float[2]", None),
    ("low", "float[2]", None), ("high", "float[2]", None), ("top", "float[2]", None), ("leaf", "string", None),
    ("grown", "string", None), ("exact", "string", None),
]

# Group tables in registration order (a group's own groups first). Class names are ours.
GROUPS = [
    ("filter", FILTER), ("colour_map", COLOUR_MAP), ("noise", NOISE), ("component", COMPONENT),
    ("record", RECORD), ("texture", TEXTURE), ("render", RENDER), ("sky", SKY), ("colour_key", COLOUR_KEY),
    ("value_key", VALUE_KEY), ("light", LIGHT), ("colour_channel", COLOUR_CHANNEL),
    ("value_channel", VALUE_CHANNEL), ("material", MATERIAL), ("terrain", TERRAIN), ("tree", TREE),
]

# : a setting a file does not hold takes the value the
# original's own new document or new object has; settings the original has no counterpart of keep our choice.
# defaults.py (the table)
DEFAULTS = dict(defaults.GROUP_DEFAULTS)

CLASSES = {}


def _enum_label(ident):
    """The dropdown text for an identifier: ROWS -> Rows, SPATIAL_KIND -> Spatial kind; letters (A, AB) and numbers stay."""
    if len(ident) <= 2:
        return ident
    return ident.replace("_", " ").capitalize()


def _enum_items(table):
    """Identifiers and numbers are what files store; only the label is made readable."""
    return [(ident, _enum_label(ident), "", number) for ident, number in table.items()]


def apply_program_defaults(program):
    """'s rows with a 5.0 value: each world's sky and each light's settings that still hold the other kind's
    default take the program's own."""
    from .gather import sky_group  # imported late: gather reads props' names
    targets = [("sky", sky_group(w)) for w in bpy.data.worlds] + \
              [("light", getattr(l, names.DATA_GROUP, None)) for l in bpy.data.lights]
    for key, group in targets:
        if group is None:
            continue
        for name, _old, _new in defaults.five_rows(key):
            if not hasattr(group, name):
                continue
            value = defaults.switched_value(key, name, getattr(group, name), program)
            if value is not None:
                setattr(group, name, value)


def _program_changed(self, context):
    apply_program_defaults(self.program)


# Limits of the add-on's own settings that have no Bryce counterpart (the finishing; finishing.problems checks the
# same ranges).
LIMITS = {("render", "finish_threshold"): {"min": 0.0, "max": 1.0},
          ("render", "glow_size"): {"min": 0.001, "soft_max": 0.5}, ("render", "glow_strength"): {"min": 0.0},
          ("render", "flare_strength"): {"min": 0.0}, ("render", "jpeg_quality"): {"min": 1, "max": 100},
          ("render", "palette_colours"): {"min": 2, "max": 256}}


def make_property(group_key, name, kind, extra):
    default = DEFAULTS.get((group_key, name))
    kw = {"name": name}
    kw.update(LIMITS.get((group_key, name), {}))
    if (group_key, name) == ("render", "program"):
        kw["update"] = _program_changed
    if default is not None:
        kw["default"] = default
    if kind == "float":
        return bpy.props.FloatProperty(**kw)
    if kind.startswith("float["):
        return bpy.props.FloatVectorProperty(size=int(kind[6:-1]), **kw)
    if kind == "colour":
        # : kept as the colour is shown, no colour conversion.
        return bpy.props.FloatVectorProperty(size=3, subtype="COLOR_GAMMA", soft_min=0.0, soft_max=1.0, **kw)
    if kind in ("int", "word"):
        return bpy.props.IntProperty(**kw)
    if kind.startswith("int[") or kind.startswith("word["):
        return bpy.props.IntVectorProperty(size=int(kind[kind.index("[") + 1:-1]), **kw)
    if kind == "bool":
        return bpy.props.BoolProperty(**kw)
    if kind == "enum":
        return bpy.props.EnumProperty(items=_enum_items(extra), **kw)
    if kind == "string":
        return bpy.props.StringProperty(**kw)
    if kind == "path":
        return bpy.props.StringProperty(subtype=extra, **kw)
    if kind == "image":
        return bpy.props.PointerProperty(type=bpy.types.Image, **kw)
    if kind == "material":
        return bpy.props.PointerProperty(type=bpy.types.Material, **kw)
    if kind == "object":
        return bpy.props.PointerProperty(type=bpy.types.Object, **kw)
    if kind == "group":
        return bpy.props.PointerProperty(type=CLASSES[extra], **kw)
    if kind == "list":
        return bpy.props.CollectionProperty(type=CLASSES[extra], **kw)
    raise ValueError("unknown setting type %r" % kind)


# Menus beside a group's word settings (Node menus): group key -> {menu name: (word, kind, label)}.
GROUP_MENUS = {"filter": {"kind_menu": ("kind", "filter", "Filter")},
               "colour_map": {"mode_menu": ("mode", "colour mode", "Colour mode")},
               "noise": {"kind_menu": ("kind", "noise", "Type"), "method_menu": ("method", "method", "Method")}}


def _make_group(key, table):
    annotations = {name: make_property(key, name, kind, extra) for name, kind, extra in table}
    for menu, (store, menu_kind, label) in GROUP_MENUS.get(key, {}).items():
        annotations[menu] = node_menus.int_menu(store, menu_kind, label)
    for word, prefix, rows in names.GROUP_SWITCHES.get(key, ()):  # one toggle per named bit, over the word itself
        for bit, label, description in rows:
            annotations[prefix + "%x" % bit] = node_menus.bit_property(word, bit, label, description)
    cls = type("STECYK3D_PG_" + key, (bpy.types.PropertyGroup,), {"__annotations__": annotations})
    bpy.utils.register_class(cls)
    CLASSES[key] = cls


# Files made before Blender 5 (and by an older add-on) keep an object's Bryce settings as plain custom properties
# ("bryce_shape" = "metaball"). Blender 5 keeps the settings a script registers apart from those, so the registered
# property then reads as its default and the object counts as a plain mesh (a glass metaball vine came out as dark
# spheres). `legacy_get` reads the registered value first and the plain custom property as the fallback.
_LEGACY_DEFAULTS = {"bryce_shape": "", "bryce_role": "NEUTRAL", "bryce_cut_shows_positive": False,
                    "bryce_negative": False}


def legacy_get(obj, key):
    """An object's Bryce setting: the registered property when set, else the plain custom property of that name."""
    value = getattr(obj, key)
    if value != _LEGACY_DEFAULTS[key]:
        return value
    raw = obj.get(key)
    if raw is None:
        return value
    return bool(raw) if isinstance(_LEGACY_DEFAULTS[key], bool) else str(raw)


def adopt_legacy(objects):
    """Copy plain custom properties into the registered ones (original objects, not evaluated copies)."""
    for obj in objects:
        for key in _LEGACY_DEFAULTS:
            try:
                value = legacy_get(obj, key)
                if value != getattr(obj, key):
                    setattr(obj, key, value)
            except (TypeError, ValueError, AttributeError):
                pass


def kept_tree(scene, obj):
    """The grown tree (its pieces, leaves and leaf shapes) that a file of the earlier add-on keeps, packed, with the
    scene for an object it shows as a proxy mesh (`bryce_proxy` "tree"); None when the scene keeps none."""
    body = scene.get("bryce_carried_data")
    if not body:
        return None
    import base64, json, zlib
    try:
        kept = json.loads(zlib.decompress(base64.b64decode(body)))
    except (ValueError, zlib.error):
        return None
    entries = kept.get("objects") or []
    index = obj.get("bryce_index")
    entry = entries[index] if isinstance(index, int) and 0 <= index < len(entries) else None
    if entry is None or entry.get("name") != obj.name or entry.get("shape") != "tree":
        entry = next((e for e in entries if e.get("name") == obj.name and e.get("shape") == "tree"), None)
    definition = (entry or {}).get("tree") or {}
    trees = kept.get("trees") or []
    k = definition.get("definition")
    if not isinstance(k, int) or not 0 <= k < len(trees):
        return None
    tree = trees[k]
    return {"pieces": tree["pieces"], "leaves": tree["leaves"], "shapes": tree["shapes"]}


LEGACY_DIMENSIONS_DONE = "bryce_dims_from_one"  # plain scene key: the dimensions of the file are already 1 to 3


def adopt_legacy_dimensions(scenes, groups, materials, worlds):
    """The earlier add-on kept a noise's dimension as the document stores it (0 = 1-D, 1 = 2-D, 2 = 3-D) where this
    one's Bryce Noise holds 1 to 3 : read as they stand, every texture of its files (materials and the
    sky's clouds) was one dimension too few, and a cloud or leaf came out of another noise. A file of the earlier
    add-on is the one whose scene keeps `bryce_carried` or `bryce_settings` (this add-on writes neither); its noises
    are moved up by one, once, and the scene is marked so that a save and a load do not move them again."""
    old = [s for s in scenes if "bryce_carried" in s.keys() or "bryce_settings" in s.keys()]
    if not old or any(LEGACY_DIMENSIONS_DONE in s.keys() for s in old):
        return 0
    trees = list(groups) + [m.node_tree for m in materials if m.node_tree] + [w.node_tree for w in worlds if w.node_tree]
    seen, moved = set(), 0
    for tree in trees:
        if tree is None or tree.as_pointer() in seen:
            continue
        seen.add(tree.as_pointer())
        for node in tree.nodes:
            if node.bl_idname == "BryceNoiseNode" and 0 <= node.dims <= 2:
                node.dims += 1
                moved += 1
    for scene in old:
        scene[LEGACY_DIMENSIONS_DONE] = 1
    return moved


@bpy.app.handlers.persistent
def _adopt_on_load(*_):
    from . import material_nodes
    adopt_legacy(bpy.data.objects)
    adopt_legacy_dimensions(bpy.data.scenes, bpy.data.node_groups, bpy.data.materials, bpy.data.worlds)
    material_nodes.adopt_legacy_columns(bpy.data.materials)


def register():
    bpy.app.handlers.load_post.append(_adopt_on_load)
    for key, table in GROUPS:
        _make_group(key, table)
    T = bpy.types
    setattr(T.Scene, names.SCENE_RENDER_GROUP, bpy.props.PointerProperty(type=CLASSES["render"]))
    setattr(T.World, names.DATA_GROUP, bpy.props.PointerProperty(type=CLASSES["sky"]))
    setattr(T.Material, names.DATA_GROUP, bpy.props.PointerProperty(type=CLASSES["material"]))
    setattr(T.Light, names.DATA_GROUP, bpy.props.PointerProperty(type=CLASSES["light"]))
    setattr(T.Object, names.OBJECT_TERRAIN_GROUP, bpy.props.PointerProperty(type=CLASSES["terrain"]))
    setattr(T.Object, names.OBJECT_TREE_GROUP, bpy.props.PointerProperty(type=CLASSES["tree"]))
    T.Object.bryce_shape = bpy.props.StringProperty(name="bryce_shape")
    T.Object.bryce_role = bpy.props.EnumProperty(name="bryce_role", items=_enum_items(names.ROLE))
    T.Object.bryce_cut_shows_positive = bpy.props.BoolProperty(name="bryce_cut_shows_positive")
    T.Object.bryce_negative = bpy.props.BoolProperty(name="bryce_negative")
    # bryce_torus and bryce_shear (double[n]) are Blender custom properties, read with Object.get .


def unregister():
    if _adopt_on_load in bpy.app.handlers.load_post:
        bpy.app.handlers.load_post.remove(_adopt_on_load)
    T = bpy.types
    for owner, attr in ((T.Object, "bryce_negative"), (T.Object, "bryce_cut_shows_positive"),
                        (T.Object, "bryce_role"), (T.Object, "bryce_shape"),
                        (T.Object, names.OBJECT_TREE_GROUP), (T.Object, names.OBJECT_TERRAIN_GROUP),
                        (T.Light, names.DATA_GROUP), (T.Material, names.DATA_GROUP), (T.World, names.DATA_GROUP),
                        (T.Scene, names.SCENE_RENDER_GROUP)):
        delattr(owner, attr)
    for key, _ in reversed(GROUPS):
        bpy.utils.unregister_class(CLASSES.pop(key))
