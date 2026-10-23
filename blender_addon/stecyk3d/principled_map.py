# SPDX-License-Identifier: GPL-3.0-or-later
# (Shader operators: the Principled BSDF to Bryce channels table)
"""A Blender Principled BSDF's plain values as a Bryce material value (components.py's form). Pure Python.

The table (written by us; no original-program knowledge is needed, Bryce's channels are only what the Materials Lab
calls them):

    Principled BSDF             Bryce Material channel      how
    Base Color                  diffuse_color               linear -> display (sRGB) values, as the node stores colours "as shown"
    Roughness                   specularity                 1 - roughness (a rough surface has no highlight)
    Metallic                    metallicity                 as is
    Alpha                       transparency                1 - alpha
    Emission Color + Strength   ambient_color + ambience    colour as above; ambience = min(strength, 1) when strength > 0
    (everything else)           the new material's numbers  specular colour, halo, reflection, bump, refraction, flags

A linked input (a texture or any node) is not carried: its constant is used and the caller is told which were linked.
"""

from . import defaults


def display(c):
    """One linear colour component as the display value (the sRGB transfer function, clamped to 0..1)."""
    c = min(max(float(c), 0.0), 1.0)
    return round(12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055, 7)


def display_colour(rgb):
    return [display(c) for c in list(rgb)[:3]]


def new_material_value():
    """'s new material as a material value (what the New Bryce material button writes)."""
    from . import components
    new = defaults.NEW_MATERIAL
    value = components.empty_material()
    value["flags"] = new["flags"]
    value["volume_color"] = list(new["volume_color"])
    for key in ("refraction", "volume_edge", "volume_sharpness", "volume_quality"):
        value[key] = new[key]
    for name, colour in new["colours"].items():
        value["channels"][name]["value"] = list(colour)
    for name, number in new["values"].items():
        value["channels"][name]["value"] = number
    return value


def material_value(base_color, roughness, metallic, alpha, emission_color, emission_strength):
    """The Bryce material value for a Principled BSDF's constants."""
    value = new_material_value()
    ch = value["channels"]
    ch["diffuse_color"]["value"] = display_colour(base_color)
    ch["specularity"]["value"] = min(max(1.0 - float(roughness), 0.0), 1.0)
    ch["metallicity"]["value"] = min(max(float(metallic), 0.0), 1.0)
    ch["transparency"]["value"] = min(max(1.0 - float(alpha), 0.0), 1.0)
    if float(emission_strength) > 0.0:
        ch["ambient_color"]["value"] = display_colour(emission_color)
        ch["ambience"]["value"] = min(float(emission_strength), 1.0)
    return value
