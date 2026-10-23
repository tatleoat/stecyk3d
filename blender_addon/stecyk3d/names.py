# SPDX-License-Identifier: GPL-3.0-or-later
# (Settings under's names)
"""The interface names a .blend file of the earlier add-on holds, and the enum tables. Pure Python."""

# A unique engine id (task T1): two add-ons cannot both register the same bl_idname, so a shared "BRYCE_3D"
# would stop this add-on being enabled at the same time as another Bryce-style one.'s files keep
# mattering because every SETTING name stays the earlier add-on's (below); a file that had selected the
# earlier engine now comes in with Blender's default engine and the user picks "Stecyk3D" once. The id is
# also what the panels' COMPAT_ENGINES and polls use, and what a scene's `render.engine` holds.
# (the render engine and preferences)
ENGINE_ID = "STECYK3D"

# Groups of settings .
SCENE_RENDER_GROUP = "bryce_render"
DATA_GROUP = "bryce"
OBJECT_TERRAIN_GROUP = "bryce_terrain"
OBJECT_TREE_GROUP = "bryce_tree"
NODE_SETTINGS_GROUP = "settings"

# Node and socket types .
NODE_TYPES = (
    "BryceMaterialNode", "BryceMixMaterialsNode", "BryceTextureNode", "BrycePictureNode",
    "BryceMappingNode", "BryceNoiseNode", "BryceFilterNode", "BryceComponentNode", "BryceBlendNode",
    "BryceMixColorNode", "BryceMixValueNode", "BryceSkyNode", "BryceLightNode",
)
SOCKET_TYPES = ("BryceColorSocket", "BryceValueSocket", "BryceSignalSocket", "BryceComponentSocket")

# Enum tables: identifier -> the number Blender keeps ("the number given after each identifier").
PROGRAM = {"3.0": 0, "3.1": 1, "5.0": 2}  # program
ANTIALIAS = {"0": 0, "1": 1, "2": 2, "4": 3}  # antialias (identifier is the setting's number)
SPATIAL_KIND = {"TREE": 0, "GRID": 1}  # spatial_kind
SPATIAL_KIND_CODE = {"TREE": "bspT", "GRID": "grid"}  # spatial_kind (offset 176) [format]
SPATIAL_LEVEL = {"0": 0, "1": 1, "2": 2}  # spatial_level
FOCUS = {"CAMERA": 0, "NUMBERS": 1}  # focus
CORES = {"ALL": 0, "FAST": 1, "ONE": 2}  # cores
VIEWPORT_PIXEL_SIZE = {"AUTO": 0, "1": 1, "2": 2, "4": 3, "8": 4}  # viewport_pixel_size
WASH_GRAIN = {"AUTO": 0, "EXACT": 1, "FAST": 2}  # (Wash grain); not
LIVE_ORDER = {"ROWS": 0, "SCATTERED": 1}  # (the live picture, Live order); not
VIEWPORT_QUALITY = {"RENDER": 0, "NORMAL": 1, "DRAFT": 2}  # viewport_quality
MIX_BY = {"ALTITUDE": 0, "ALPHA": 1}  # Bryce Mix Colour / Mix Value `by`
BUMP_SWITCH = {"AUTO": 0, "ON": 1, "OFF": 2}  # Bryce Component bump_switch
CHANNEL_MODE = {"A": 0, "ALTITUDE": 1, "ALPHA": 2}  # the group on materials, channel `mode`
ROLE = {"NEUTRAL": 0, "POSITIVE": 1, "NEGATIVE": 2, "INTERSECT": 3}  # bryce_role
TERRAIN_GRID = {"AUTO": 0, "16": 1, "32": 2, "64": 3, "128": 4, "256": 5, "512": 6, "1024": 7}
LEAF_KIND = {"3": 0, "4": 1, "5": 2, "6": 3, "7": 4}  # leaf_kind


def source_identifiers():
    """`src_a`, `src_b`, `src_alpha` : NONE = 0, then the texture components A = 1 to the thirty-ninth."""
    out = {"NONE": 0}
    for n in range(1, 40):
        out[component_letter(n - 1)] = n
    return out


def component_letter(index):
    """0 -> A, 25 -> Z, 26 -> AA ... (our spelling of components beyond Z; names only A, B, ...)."""
    letters = ""
    index += 1
    while index > 0:
        index, rem = divmod(index - 1, 26)
        letters = chr(ord("A") + rem) + letters
    return letters


SOURCE = source_identifiers()

# The thirteen channels of a Bryce Material, in the inputs' order .
COLOUR_CHANNELS = ("diffuse_color", "ambient_color", "specular_color", "specular_halo", "transparent_color")
VALUE_CHANNELS = (
    "diffusion", "ambience", "specularity", "transparency", "reflection", "bump", "metallicity", "volume_density",
)
CHANNELS = COLOUR_CHANNELS + VALUE_CHANNELS

# Shapes (bryce_shape).
SHAPES = (
    "plane", "slab", "cube", "sphere", "cylinder", "cone", "pyramid", "torus", "square", "disc",
    "metaball", "tree", "terrain", "group",
)

# Settings shown and used only as 5.0 ("5.0" and "rendered as 5.0").
FIVE_ONLY = {
    "render": {"super_rays", "premium_rays", "tir_depth", "spatial_kind", "spatial_level", "focus",
               "focal_distance", "lens_radius"},
    "sky": {"sun_radius", "sun_shadow_ambience", "sun_shadow_color", "sky5", "sky5_flags2",
            "sky5_haze_offset", "sky5_haze_thickness", "sky5_fog_offset", "sky5_haze_shift",
            "sky5_cloud_detail", "sky5_sun_glow_size", "sky5_moon_glow_size", "sky5_sun_disc",
            "sky5_moon_disc", "sky5_air_steps", "sky5_star_intensity", "sky5_comet_intensity", "sky5_lit_air",
            "sky5_star_orientation"},
    "light": {"radius", "shadow_ambience", "shadow_color", "range", "gradient_range", "gradient_offset",
              "color_keys", "value_keys", "gradient_color_flags", "gradient_value_flags"},
}


# ---------------------------------------------------------------- named choices for the node menus (tasks T2, T3)
# (Node menus). Only numbers and the names the released spec itself gives are
# here. The labels of the texture editor's own menus (noise types, octave methods, blends, filters, colour modes)
# are the user's install's text: they are read at run time from the substrate's tables , never kept here.
# A node's stored number is always the same integer word as before; a menu only shows and sets it.

MAPPING_MODES = {
    0: "Object Space", 1: "World Space", 2: "Parametric", 3: "Parametric Scaled", 4: "World Top", 5: "Spherical",
    6: "Cylindrical", 7: "Reflection Map", 8: "Random", 9: "Object Top", 10: "Object Front", 12: "Object Cubic",
    13: "Sinusoidal", 14: "World Front", 15: "World Side", 16: "World Cubic",
}
MAPPING_MODES_FIVE_ONLY = frozenset(range(11, 17))

OUTPUT_BITS = ((0x1, "Value"), (0x2, "Colour"), (0x4, "Bump"))
COLOUR_SPACES = {0: "RGB", 0x40: "HLS"}

# Mapping switch word ("The switch bits"). Only 0x1 has a name ("Symmetrical Tiling"); the rest
# are ours, from what each bit does there. (bit, label, description, the program that first reads it)
MAPPING_SWITCHES = (
    (0x1, "Symmetrical Tiling", "Fold the texture back and forth instead of repeating it", "3.0"),
    (0x2, "Picture: bit 0x2", "Read by a picture texture (SCN-15)", "3.0"),
    (0x4, "Parametric centred", "Modes 2 and 3: the texture point is taken about the middle of the surface", "3.1"),
    (0x8, "Take colour from texture", "A material channel takes the texture's colour (SH-2.1)", "3.1"),
    (0x10, "Take value from texture", "A material channel takes the texture's value (SH-2.1)", "3.1"),
    (0x20, "Mirrored tiling", "Mirroring step of the mapping", "5.0"),
    (0x40, "Picture: unblended pixels", "A picture texture shows one pixel's colour, not a blend", "5.0"),
)

# The Materials Lab's switches on a material's flag word . Names the
# quotes from the Lab's menu: Fuzzy 0x2, Light 0x4, Blend Transparency 0x8, Distance Blur 0x10, Additive 0x2000,
# Cast Shadows (bit 0x80 clear), Receive Shadows (bit 0x20 clear). The others are named by their number.
# (bit, label, description, inverted: the switch is on while the bit is clear)
MATERIAL_SWITCHES = (
    (0x2, "Fuzzy", "Fuzzy material (shading mode)", False),
    (0x4, "Light", "Light material (shading mode)", False),
    (0x8, "Blend Transparency", "Blend Transparency (shading mode)", False),
    (0x10, "Distance Blur", "Distance blur: bump from the texture's value", False),
    (0x80, "Cast Shadows", "Ticked while bit 0x80 is clear", True),
    (0x20, "Receive Shadows", "Ticked while bit 0x20 is clear", True),
    (0x2000, "Additive", "Additive blending of the medium", False),
    (0x4000, "Volume Blend 0x4000", "A volume blend switch of the Lab's menu", False),
    (0x8000, "Volume Blend 0x8000", "A volume blend switch of the Lab's menu", False),
    (0x100, "Volume 0x100", "Volume material, kind marked by this bit (SH-2.4)", False),
    (0x200, "Volume 0x200", "Volume material, kind marked by this bit (SH-2.4)", False),
    (0x400, "Volume 0x400", "Volume material, kind marked by this bit (SH-2.4)", False),
)


# The sky's switches on its flag words (one toggle per bit; each writes only its own bit): the labels of the earlier
# add-on's World tab, from the bits the renderer's sky routines read. (bit, label, description)
SKY_SWITCHES = (
    (0x1, "Atmosphere", "Sky colours and clouds; off gives the plain colour"),
    (0x2, "Glow look 1", "Recolours the sun's glow by the sun's height (first look)"),
    (0x4, "Glow look 2", "Recolours the sun's glow by the sun's height (second look)"),
    (0x8, "Custom colours", "Sky and horizon colours as given instead of following the sun"),
    (0x10, "Clouds", "Cloud layers"), (0x20, "Stratus", "The stratus layer"), (0x40, "Cumulus", "The cumulus layer"),
    (0x80, "Haze", "Haze over distance and at the horizon"), (0x100, "Sky dome light", "Light from the whole sky"),
    (0x2000, "Haze takes the sun's colour", "Haze is tinted toward the sun's colour"),
    (0x200000, "Fog takes the sun's colour", "Fog is tinted toward the sun's colour"),
    (0x80000, "Clouds follow the view", "Cloud offsets move with the camera"),
    (0x400000, "Fixed cloud coordinates", "Third texture coordinates do not follow the cloud height"),
)
SKY5_SWITCHES = (  # bits of `flags` that a sky of Bryce 5 reads
    (0x40000000, "Clouds on a dome", "The cloud layers lie on a sphere over the scene and not on flat planes"),
    (0x80000000, "Haze shift", "Distant haze is thickened by the shift colour"),
)
SKY5_SWITCHES2 = (  # bits of `sky5_flags2`
    (0x2, "Stars", "The star field's stars are drawn (where the sky is dark enough to show them)"),
    (0x4, "Comets", "The star field's comets are drawn"),
    (0x1, "Haze as an older document's", "The haze reaches as far as in a document Bryce 5 took over from Bryce 3D"),
)
# group key -> (word, property name prefix, rows); the property is "<prefix><bit in hex>"
GROUP_SWITCHES = {"sky": (("flags", "flag_", SKY_SWITCHES), ("flags", "sky5_flag_", SKY5_SWITCHES),
                          ("sky5_flags2", "sky5_flag2_", SKY5_SWITCHES2))}
SWITCH_PREFIXES = ("flag_", "sky5_flag_", "sky5_flag2_")
# every switch view's identifier: shows and sets one bit of a word, stores nothing, so is no setting of its own
SWITCH_VIEWS = frozenset(prefix + "%x" % bit for sets in GROUP_SWITCHES.values() for _, prefix, rows in sets
                         for bit, _, _ in rows)
