# SPDX-License-Identifier: GPL-3.0-or-later
# (Defaults, the owner's decision of 2026-10-22: a setting the file does not hold takes the value the
# original's own new document or new object has),, (a new material,
# a new texture's mapping),
# (Defaults: one table, pure Python, read by props and nodes)
"""What a setting holds when no file gives it: the numbers a new Bryce document and its new objects have.

Where the programs differ the table gives the numbers of the program the add-on starts with, 3.0 (the first item of
`program`); the `FIVE` table overrides them for the program 5.0. Every constant names its row of's Defaults
table. Pure Python: props.py and nodes.py read it."""

WORD_SKY_FLAGS_30 = 0x3E000283  # [format]
WORD_SKY_FLAGS_50 = 0x7E0002B3  # [format]
WORD_SCENE_FLAGS = 0x23BF  # [format]
WORD_SUN_FLAGS = 0x400501  # [format]
WORD_LIGHT_FLAGS_30 = 0x1100  # [format]
WORD_LIGHT_FLAGS_50 = 0x100  # [format]
WORD_MATERIAL_FLAGS = 0x10000  # [format]
WORD_LIGHT_MATERIAL_FLAGS = 0x10004  # [format]

# Blender's axes of the sun's direction toward the sun: the description's (x, y, z) is Blender's (x, z, y)
# ('s change of axes).
SUN_DIRECTION_DESCRIPTION = (0.7427813527082074, 0.5570860145311556, -0.3713906763541037)
SUN_DIRECTION_BLENDER = (0.7427813527082074, -0.3713906763541037, 0.5570860145311556)

# Settings of a group by (group key, setting name), the program 3.0's numbers.
GROUP_DEFAULTS = {
    ("render", "antialias"): "1",
    ("render", "super_rays"): 4, ("render", "premium_rays"): 8,
    ("render", "tir_depth"): 2, ("render", "spatial_kind"): "TREE", ("render", "spatial_level"): "1",
    ("render", "depth"): 6,
    ("render", "focal_distance"): 15.0, ("render", "lens_radius"): 0.10000000149011612,
    ("render", "units"): 1.0,  # add-on's own (`units` has no original counterpart)
    ("render", "live_order"): "ROWS",  # (Live order)
    ("render", "wash_grain"): "AUTO",  # (Wash grain)
    ("render", "progressive"): True,  # (task T2)
    ("sky", "flags"): WORD_SKY_FLAGS_30, ("sky", "cloud_height"): 36.1328125,
    ("sky", "cumulus_color"): (0.8, 0.8, 0.8), ("sky", "stratus_color"): (1.0, 1.0, 1.0),
    ("sky", "cloud_offset"): (0.0, 0.0),
    ("sky", "cover"): 0.5, ("sky", "cover_amplitude"): 1.0, ("sky", "cloud_frequency"): 0.25,
    ("sky", "zenith"): (0.2, 0.2, 1.0), ("sky", "horizon"): (1.0, 1.0, 1.0),
    ("sky", "ambient"): (0.0, 0.0, 0.0), ("sky", "plain"): (0.2, 0.2, 1.0),
    ("sky", "shadow_lift"): 0.1, ("sky", "ambient_color"): (1.0, 1.0, 1.0),
    ("sky", "ambient_amount"): 1.0, ("sky", "cumulus_shade"): (1.0, 1.0, 1.0),
    ("sky", "haze_color"): (1.0, 1.0, 1.0), ("sky", "haze"): 0.4,
    ("sky", "fog_color"): (1.0, 1.0, 1.0), ("sky", "fog_height"): 0.0, ("sky", "fog"): 0.0,
    ("sky", "moon"): (220.0, 0.0, -30.0, 1.0),
    ("sky", "discs"): (1.0, 4.0, 1.0, 1.0, 1.0, 5.0, 2.5, 100.0, 0.3, 0.1405408347024, 0.1051042352657, 2.0, 0.0,
                       2.0, 0.0),
    ("sky", "has_stratus"): True, ("sky", "has_cumulus"): True,
    ("sky", "sun_direction"): SUN_DIRECTION_BLENDER,
    ("sky", "sun_flags"): WORD_SUN_FLAGS, ("sky", "sun_intensity"): 1.0,
    ("sky", "sun_color"): (1.0, 1.0, 1.0), ("sky", "sun_glow_color"): (1.0, 1.0, 1.0),
    ("sky", "sun_radius"): 0.0, ("sky", "sun_shadow_ambience"): 0.0,
    ("sky", "sun_shadow_color"): (1.0, 1.0, 1.0),
    ("sky", "scene_flags"): WORD_SCENE_FLAGS,
    ("sky", "sky5_flags2"): 0, ("sky", "sky5_haze_offset"): 0.5, ("sky", "sky5_haze_thickness"): 0.5,
    ("sky", "sky5_fog_offset"): 0.11, ("sky", "sky5_haze_shift"): (0.0, 0.05, 0.25),
    ("sky", "sky5_cloud_detail"): (0.5, 0.5), ("sky", "sky5_sun_glow_size"): 1.0,
    ("sky", "sky5_moon_glow_size"): 1.0, ("sky", "sky5_sun_disc"): 1.0, ("sky", "sky5_moon_disc"): 1.0,
    ("sky", "sky5_air_steps"): 500, ("sky", "sky5_star_intensity"): 1.0,
    ("sky", "sky5_comet_intensity"): 1.0, ("sky", "sky5_star_orientation"): (0.0, 1.0, 0.0),
    ("sky", "sky5_lit_air"): 0.5,
    ("light", "flags"): WORD_LIGHT_FLAGS_30, ("light", "intensity"): 5.0, ("light", "color"): (1.0, 1.0, 1.0),
    ("light", "edge"): (0.0, 0.0), ("light", "radius"): 0.0, ("light", "shadow_ambience"): 0.0,
    ("light", "shadow_color"): (1.0, 1.0, 1.0), ("light", "range"): 1.0, ("light", "gradient_range"): 5.0,
    ("light", "gradient_offset"): 0.0,
    # A new texture on a channel: mapping mode 1, scale 0.51 on every axis, offset 0, no turn.
    ("texture", "mode"): 1, ("texture", "scale"): (0.51, 0.51, 0.51), ("texture", "rotation"): (0.0, 0.0, 0.0, 1.0),
}

NEW_MAPPING = {"mode": 1, "scale": (0.51, 0.51, 0.51), "offset": (0.0, 0.0, 0.0),
               "rotation": (0.0, 0.0, 0.0, 1.0)}

# A new material: the numbers of's material, "A" first value.
NEW_MATERIAL = {
    "flags": WORD_MATERIAL_FLAGS,
    "colours": {"diffuse_color": (0.499, 0.499, 0.499), "ambient_color": (1.0, 1.0, 1.0),
                "specular_color": (1.0, 1.0, 1.0), "specular_halo": (0.8, 0.8, 0.8),
                "transparent_color": (1.0, 1.0, 1.0)},
    "values": {"diffusion": 1.0, "ambience": 0.1965, "specularity": 0.0, "transparency": 0.0, "reflection": 0.0,
               "bump": 0.0, "metallicity": 0.0, "volume_density": 0.0},
    "volume_color": (1.0, 1.0, 1.0), "refraction": 1.0, "volume_edge": 0.0, "volume_sharpness": 1.0,
    "volume_quality": 0.75,
}


def new_light_material():
    """A light's own material: the new material with Diffuse colour (1, 1, 1), Transparency 0.5 and flag word
    0x10004."""
    out = {k: (dict(v) if isinstance(v, dict) else v) for k, v in NEW_MATERIAL.items()}
    out["flags"] = WORD_LIGHT_MATERIAL_FLAGS
    out["colours"]["diffuse_color"] = (1.0, 1.0, 1.0)
    out["values"]["transparency"] = 0.5
    return out


# What 5.0 holds otherwise.
FIVE = {
    ("sky", "flags"): WORD_SKY_FLAGS_50, ("sky", "cloud_height"): 18.06640625,
    ("light", "flags"): WORD_LIGHT_FLAGS_50,
}


def program_default(group, name, program):
    """The default of a setting for a program: 5.0's where the table has one, else the table's. (rows
    with a 5.0 value);"""
    if program == "5.0" and (group, name) in FIVE:
        return FIVE[(group, name)]
    return GROUP_DEFAULTS.get((group, name))


def five_rows(group):
    """The settings of a group whose default differs for 5.0, as (name, 3.0 value, 5.0 value)."""
    return [(n, GROUP_DEFAULTS[(g, n)], v) for (g, n), v in FIVE.items() if g == group]


def switched_value(group, name, current, new_program):
    """The value a setting takes when the program changes to `new_program`: the new program's default where the
    setting still holds the other kind's default (nothing was set), else None (leave it). (rows with a
    5.0 value);"""
    new = program_default(group, name, new_program)
    old = program_default(group, name, "3.0" if new_program == "5.0" else "5.0")
    if old is None or new is None or old == new or current != old:
        return None
    return new
