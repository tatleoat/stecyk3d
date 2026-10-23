# SPDX-License-Identifier: GPL-3.0-or-later
# (The revision of: Lamps)
"""A Blender lamp and its Bryce Light settings to the description's light. Pure Python."""

import math

from .convert import FIVE, radial_light
from .numbers import single, singles, word
from .solids import columns_times_units, own_transform, swap_yz

GRADIENT_BIT = 0x200000  # [format]
RANGE_BIT = 0x20000000  # [format]

ROUND_AREA = ("DISK", "ELLIPSE")
SQUARE_AREA = ("SQUARE", "RECTANGLE")
SECOND_SIZE = ("RECTANGLE", "ELLIPSE")


def _scaled(v, k):
    return [c * k for c in v]


def _minus(a, b):
    return [x - y for x, y in zip(a, b)]


def beam_kind(lamp):
    """: cone, pyramid, cylinder or box for a SPOT or AREA lamp."""
    kind = lamp["type"]
    if kind == "SPOT":
        return "pyramid" if lamp.get("use_square") else "cone"
    if kind == "AREA":
        shape = lamp.get("shape", "SQUARE")
        if shape in ROUND_AREA:
            return "cylinder"
        if shape in SQUARE_AREA:
            return "box"
        raise ValueError("an area lamp of shape %r is not one of DISK, ELLIPSE, SQUARE, RECTANGLE" % (shape,))
    raise ValueError("a %s lamp has no beam" % (kind,))


def beam_spread(lamp):
    """'s s1 and s2."""
    if lamp["type"] == "SPOT":
        theta = single(lamp["spot_size"])
        s = 2.0 * math.tan(theta / 2.0)
        return s, s
    s1 = single(lamp["size"])
    s2 = single(lamp["size_y"]) if lamp.get("shape") in SECOND_SIZE else s1
    return s1, s2


def beam_light(lamp, light, matrix, units, own_diffuse=None):
    """One of's lights with a beam ('s table), from a SPOT or AREA lamp's world matrix."""
    kind = beam_kind(lamp)
    X, Y, Z, T = columns_times_units(matrix, units)
    s1, s2 = beam_spread(lamp)
    r = single(light["reach"])
    edge = singles(light["edge"])
    if kind == "cone":  # table
        rows = (_scaled(X, s1 * r), _scaled(Z, r), _scaled(Y, s2 * r))
        exponents = [edge[0]]
    elif kind == "pyramid":
        rows = (_scaled(X, 2.0 * s1 * r), _scaled(Z, 2.0 * r), _scaled(Y, 2.0 * s2 * r))
        exponents = [edge[0], edge[1]]
    elif kind == "cylinder":
        rows = (_scaled(X, s1 / 2.0), _scaled(Z, r), _scaled(Y, s2 / 2.0))
        exponents = [edge[0]]
    else:  # box
        rows = (_scaled(X, s1), _scaled(Z, 2.0 * r), _scaled(Y, s2))
        exponents = [edge[0], edge[1]]
    position = swap_yz(_minus(T, _scaled(Z, r)))
    world = [c for row in rows for c in swap_yz(row)] + list(position)
    # Colour, strength and switches as a radial light's .
    plain = radial_light(light, [0.0, 0.0, 0.0], units, own_diffuse)
    return {
        "kind": kind,
        "world": world,
        "exponents": exponents,
        "colour": plain["colour"],
        "intensity": plain["intensity"],
        "position": position,
        "switches": plain["switches"],
    }


def _sorted_keys(keys, value_name, out_name, is_colour):
    def place(key):
        return single(key["position"])
    out = []
    for key in sorted(keys, key=place):  # stable: two keys of one place keep their order (our choice)
        value = singles(key[value_name]) if is_colour else single(key[value_name])
        out.append({out_name: value, "place": place(key), "midpoint": single(key["midpoint"])})
    return out


def five_additions(light, program, units, own_diffuse=None, own_is_gel=False):
    """'s last paragraph: the fields a light gains as 5.0, and its switch word for its program."""
    flags = word(light["flags"])
    five = program == FIVE
    out = {}
    if not five:
        out["switches"] = flags & ~GRADIENT_BIT
        return out
    out["switches"] = flags
    u = single(units)
    radius = single(light.get("radius", 0.0))
    ambience = single(light.get("shadow_ambience", 0.0))
    if radius > 0.0 or ambience != 0.0 or flags & RANGE_BIT or flags & GRADIENT_BIT:
        out["radius"] = radius * u
        out["shadow_ambience"] = ambience
        out["shadow_colour"] = singles(light.get("shadow_color", (0.0, 0.0, 0.0)))
        out["range"] = single(light.get("range", 0.0)) * u
    if flags & GRADIENT_BIT:
        out["gradient"] = [single(light.get("gradient_range", 0.0)) * u, single(light.get("gradient_offset", 0.0))]
        out["gradient_keys"] = {
            "colour_keys": _sorted_keys(light.get("color_keys") or [], "color", "colour", True),
            "value_keys": _sorted_keys(light.get("value_keys") or [], "value", "value", False),
            "colour_switches": word(light.get("gradient_color_flags", 0)),
            "value_switches": word(light.get("gradient_value_flags", 0)),
        }
        out["own_colour"] = singles(light["color"])
        if light.get("tint") and own_diffuse is not None and not own_is_gel:
            out["tint_colour"] = singles(own_diffuse)
    return out


def describe_lamp(lamp, light, matrix, units, program, own_diffuse=None, own_is_gel=False):
    """The description's light for a POINT, SPOT or AREA lamp , its Bryce 5 additions included."""
    if lamp["type"] == "POINT":
        position = [matrix[0][3], matrix[1][3], matrix[2][3]]
        out = radial_light(light, position, units, own_diffuse)
        # A radial light's own object is a sphere the size of its matrix' scale (a document's light object is
        # placed like any object); the description carries it only where the matrix is not a plain place.
        # (Lights as objects)
        columns = columns_times_units(matrix, 1.0)[:3]
        if any(abs(sum(c * c for c in col) - 1.0) > 1e-9 for col in columns):
            out["world"] = own_transform(matrix, units)
    else:
        out = beam_light(lamp, light, matrix, units, own_diffuse)
    out.update(five_additions(light, program, units, own_diffuse, own_is_gel))
    # README (gel-gradient: `gel`, true for a lamp whose own material has a texture on its Diffuse colour:
    # no Diffuse colour D is given for it)
    out["gel"] = bool(own_is_gel)
    if own_is_gel:  # : the light's own colour; the gel's material is read at each lit point
        out["colour"] = singles(light["color"])
    return out

