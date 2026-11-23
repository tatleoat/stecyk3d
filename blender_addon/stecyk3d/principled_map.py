# SPDX-License-Identifier: GPL-3.0-or-later
# (Shader operators: the Principled BSDF to Bryce channels table)
"""A Blender Principled BSDF's plain values as a Bryce material value (components.py's form). Pure Python.

The table (written by us; no original-program knowledge is needed, Bryce's channels are only what the Materials Lab
calls them):

    Principled BSDF             Bryce Material channel      how
    Base Color                  diffuse_color               linear -> display (sRGB) values, as the node stores colours "as shown"
    Roughness                   specularity                 1 - roughness (a rough surface has no highlight), times
                                                            min(2 * Specular IOR Level, 1)
    Metallic                    metallicity                 as is
    Alpha, Transmission Weight  transparency                1 - alpha * (1 - transmission * (1 - metallic))
    Transmission Weight         transparent_color           white mixed toward the base colour (display values) by the
                                                            transmitted share of the transparency
    IOR                         refraction                  as is, where light is transmitted (not Thin Wall), above 0
    Specular IOR Level          reflection                  2 * level - 1, held to 0..1 (0.5 and below: none)
    Transmission, Specular      diffusion                   the new material's 1 times (1 - transmission * (1 - metallic))
      IOR Level                                             times (1 - reflection)
    Specular Tint               specular_color              display values, as Base Color
    Emission Color + Strength   ambient_color + ambience    colour as above; ambience = min(strength, 1) when strength > 0
    (everything else)           the new material's numbers  halo, bump, volume, flags

Why each:
- Alpha and Transmission Weight are both light passing through. Blender lays the surface over "nothing" by alpha,
  and inside the opaque part sends the transmission weight of the non-metallic share through glass; the two shares
  add up. Without transmission this is 1 - alpha, as before.
- Light that passes by alpha is not tinted; transmitted light takes the base colour. Bryce's transparent colour is
  the filter of what passes, so it is the two mixed by their shares: the base colour for glass, white for alpha.
- Bryce's refraction value is the index of refraction itself: the renderer bends a ray by the ratio of the values on
  the two sides (1 outside every object; the Materials Lab shows it times 100). Blender's IOR bends only transmitted
  light, so it is carried only where Transmission Weight lets some through and Thin Wall is off (a thin wall does not
  bend); alpha's straight-through light keeps the new material's 1. Bryce's own angle law (more reflection at grazing
  angles) then follows from the transparency and refraction, as Blender's Fresnel does.
- Specular IOR Level 0.5 is Blender's "no adjustment": the reflection its dielectric already has, which Bryce's model
  gives through the highlight (and the angle law for glass). Above 0.5 the user asks for more reflection than that;
  Bryce's counterpart of an added mirror is Reflection, full at the top of Blender's slider. Below 0.5 Blender fades
  its specular layer out, highlight included, to none at 0: the highlight fades with it.
- Blender shares the light between its layers: the diffuse layer gets only what the transmission (of the
  non-metallic share) and the reflection above it leave. Bryce adds its diffuse, reflected and transmitted light
  without sharing, so a glass or a mirror with the whole Diffusion comes out white; Diffusion takes the diffuse
  layer's share. (Metallic, which Blender shares the same way, keeps the first table's result: Diffusion stays.)
- Specular Tint colours Blender's dielectric highlight and reflection; Bryce's specular colour is its highlight's.
- Not carried, reported when set (`not_carried`): Coat (an angle-dependent clear layer with its own roughness, IOR
  and normal, about 4% head-on; as Reflection it would make every lacquer a mirror), Sheen, Subsurface, Anisotropic,
  Thin Film and Diffuse Roughness, which have no Bryce counterpart.

Every input at Blender's neutral value (the keyword defaults below) leaves the material as the first table made it.
A linked input (a texture or any node) is not carried: its constant is used and the caller is told which were linked.
"""

from . import defaults

# Inputs with no Bryce counterpart, and the value at which they change nothing in Blender's shading.
NO_COUNTERPART = (("Coat Weight", 0.0), ("Sheen Weight", 0.0), ("Subsurface Weight", 0.0), ("Anisotropic", 0.0),
                  ("Thin Film Thickness", 0.0), ("Diffuse Roughness", 0.0))


def display(c):
    """One linear colour component as the display value (the sRGB transfer function, clamped to 0..1)."""
    c = min(max(float(c), 0.0), 1.0)
    return round(12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055, 7)


def display_colour(rgb):
    return [display(c) for c in list(rgb)[:3]]


def unit(v):
    return min(max(float(v), 0.0), 1.0)


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


def material_value(base_color, roughness, metallic, alpha, emission_color, emission_strength, transmission=0.0,
                   ior=1.5, specular_level=0.5, specular_tint=(1.0, 1.0, 1.0), thin_wall=False):
    """The Bryce material value for a Principled BSDF's constants (the keyword defaults are Blender's neutral
    values)."""
    value = new_material_value()
    ch = value["channels"]
    ch["diffuse_color"]["value"] = display_colour(base_color)
    ch["specularity"]["value"] = unit(1.0 - float(roughness))
    if float(specular_level) < 0.5:
        ch["specularity"]["value"] *= max(2.0 * float(specular_level), 0.0)
    ch["metallicity"]["value"] = unit(metallic)
    glass = unit(alpha) * unit(transmission) * (1.0 - unit(metallic))  # the share Blender transmits
    through = (1.0 - unit(alpha)) + glass
    ch["transparency"]["value"] = unit(through)
    if glass > 0.0:
        share = glass / through
        ch["transparent_color"]["value"] = [1.0 + (d - 1.0) * share for d in display_colour(base_color)]
        if not thin_wall and float(ior) > 0.0:
            value["refraction"] = float(ior)
    ch["reflection"]["value"] = unit(2.0 * float(specular_level) - 1.0)
    # Blender's diffuse layer gets what the transmission and the added reflection leave; Bryce adds them instead
    keep = (1.0 - unit(transmission) * (1.0 - unit(metallic))) * (1.0 - ch["reflection"]["value"])
    if keep < 1.0:
        ch["diffusion"]["value"] *= keep
    ch["specular_color"]["value"] = display_colour(specular_tint)
    if float(emission_strength) > 0.0:
        ch["ambient_color"]["value"] = display_colour(emission_color)
        ch["ambience"]["value"] = min(float(emission_strength), 1.0)
    return value


def not_carried(constants):
    """The inputs among `constants` (name -> value) that are set away from their neutral value but have no Bryce
    counterpart, in the table's order."""
    return [name for name, neutral in NO_COUNTERPART if name in constants and float(constants[name]) != neutral]
