# SPDX-License-Identifier: GPL-3.0-or-later
# (the sky's `cumulus_shade`, the sun lamp and `sun_direction`, Bryce 5's sun settings, the scene's
#       switch word and the bits that are free as 3.0 or 3.1)
"""The parts of the environment that are single settings: the sky's ambient colour elsewhere, the sun's soft
shadow numbers, the sun's direction and the scene's switch word. Pure Python."""

from .axes import to_bryce_direction
from .numbers import single, singles, word

# Bryce 5's switches of the scene's switch word: a render reads them as 5.0 alone, so as 3.0 or 3.1 they are free.
# README (environment-other: 0x40, 0x40000, 0x80000, 0x400000, 0x800000, 0x1000000, 0x2000000, 0x4000000) [format]
BRYCE5_SWITCH_BITS = 0x40 | 0x40000 | 0x80000 | 0x400000 | 0x800000 | 0x1000000 | 0x2000000 | 0x4000000


def free_bits(program):
    """The bits of the scene's switch word that no render of `program` reads: Bryce 5's, as 3.0 or 3.1."""
    return 0 if program == "5.0" else BRYCE5_SWITCH_BITS


def cumulus_shade(sky):
    """`cumulus_shade`:'s ambient colour (offset 160) as three singles; where the sky holds none,
    `ambient_color`."""
    return singles(sky["cumulus_shade"] if "cumulus_shade" in sky else sky["ambient_color"])


def sun_soft_shadow(sky):
    """: the sun's radius for soft shadows (the ratio of, as set, not times `units`), its shadow
    ambience and its shadow colour, as singles; None for a sky that holds none of them."""
    if "sun_radius" not in sky:
        return None
    return {"radius": single(sky["sun_radius"]),
            "shadow_ambience": single(sky["sun_shadow_ambience"]),
            "shadow_colour": singles(sky["sun_shadow_color"])}


def sun_lamp_axis(sun_lamps, sun_object=None):
    """: the sun lamp whose own z axis gives the sun's direction. `sun_lamps` is a list of (name, own z axis
    in Blender's axes) in the order of the graph; the lamp named by `sun_object` if there is one, else the scene's
    first sun lamp. None where there is none."""
    if sun_object is not None:
        for name, axis in sun_lamps:
            if name == sun_object:
                return list(axis)
    return list(sun_lamps[0][1]) if sun_lamps else None


def sun_direction(axis, sun_direction_setting):
    """: the direction toward the sun in Bryce's axes, unit length: the lamp's own z axis, or without a sun
    lamp `sun_direction`."""
    return to_bryce_direction(axis if axis is not None else singles(sun_direction_setting),
                              "the direction toward the sun")


def scene_word(setting):
    """`scene_flags` as 32 bits."""
    return word(setting)
