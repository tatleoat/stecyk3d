# SPDX-License-Identifier: GPL-3.0-or-later
# (a Bryce Material's channels fed by one texture output or by a Bryce Mix Colour or
#       Mix Value; their switch words, B, alpha and references; the flag word without bit 0x40; each texture
#       once in the description)
# Errors as revised at (a texture output other than the one named for a
# channel; : two textures of one material with the same `column`)
# (The revision of: Channels fed by textures; Texture links of
"""A Bryce Material node with links to its inputs, as plain values, to's channels. Pure Python."""

from . import names
from .convert import MATERIAL_UNUSED_FLAG
from .numbers import single, singles, word

TEXTURE_SWITCH = (0x10000000, 0x20000000, 0x40000000, 0x80000000)  # [format]
MIX_SWITCH = {"ALTITUDE": 2, "ALPHA": 4}
ZERO_SIDE_VALUES = ("volume_density",)


def named_output(channel, side=None):
    """: the texture output that feeds a channel (side None), or a mix's `a`, `b` or `alpha` input (the
    mix being for `channel`): colour channels `color`, `bump` `bump`, every other value channel `alpha`; a mix's
    `alpha` input always `alpha`."""
    if side == "alpha":
        return "alpha"
    if channel in names.COLOUR_CHANNELS:
        return "color"
    return "bump" if channel == "bump" else "alpha"


def output_notice(material, channel, side, output):
    """a link from another output than the one names is read as the named one (the link's output
    is ignored, the channel takes the kind of result it is); the user is told. None when the output is the
    named one. `side` is None for a link to the channel itself, else the mix's input name."""
    named = named_output(channel, side)
    if output == named:
        return None
    where = "channel %s" % channel if side is None else "%s of the mix on channel %s" % (side, channel)
    return ("material %r: the texture's %s output is linked to %s, which takes the texture's %s; "
            "the link's output is read as %s (a channel takes the kind of result it is)"
            % (material, output, where, named, named))


def column_clash(material, columns, column, node_name):
    """two textures of one material with the same `column` are refused, naming both nodes and the
    column. `columns` maps column -> the first node's name, kept by the caller; returns the message or None."""
    first = columns.setdefault(column, node_name)
    if first == node_name:
        return None
    return "material %r: textures %r and %r share column %d" % (material, first, node_name, column)


def _constant(channel, value):
    return singles(value) if channel in names.COLOUR_CHANNELS else single(value)


def _one_texture(channel, constant, column):
    """: a channel fed by one texture output linked to its input."""
    colour = channel in names.COLOUR_CHANNELS
    zero = colour or channel in ZERO_SIDE_VALUES
    if colour:
        b = [0.0, 0.0, 0.0]
    else:
        b = 0.0 if zero else 1.0
    return {
        "switches": TEXTURE_SWITCH[column] if 0 <= column < len(TEXTURE_SWITCH) else 0,
        "a": _constant(channel, constant),
        "b": b,
        "alpha": 0.0 if zero else 1.0,
        "references": [column, None, None],
    }


def _mixed(channel, link):
    """: a channel fed by a Bryce Mix Colour or Mix Value."""
    by = link["mix"]
    if by not in MIX_SWITCH:
        raise ValueError("channel %s: a mix by %r is not ALTITUDE or ALPHA" % (channel, by))
    sides = [link.get(k) or {} for k in ("a", "b", "alpha")]
    # A texture linked to `alpha` stays named with ALTITUDE too: the mix by height does not read it, but a material
    # is evaluated only when it names a texture (the scene-top pictures' documents name one and nothing else),
    # so dropping it left the height mix unevaluated: plain A. (once said "not used"; it is not read, but named.)
    refs = [side.get("texture") for side in sides]
    return {
        "switches": MIX_SWITCH[by],
        "a": _constant(channel, sides[0]["value"]),
        "b": _constant(channel, sides[1]["value"]),
        "alpha": single(sides[2]["value"]),
        "references": refs,
    }


def textured_material(node, inputs, links):
    """'s flag word and thirteen channels (switches, A, B, alpha, references by texture column)."""
    channels = {}
    textures = []
    for channel in names.CHANNELS:
        link = links.get(channel)
        if link is None:
            entry = {"switches": 0, "a": _constant(channel, inputs[channel]), "b": None, "alpha": None,
                     "references": [None, None, None]}
        elif "mix" in link:
            entry = _mixed(channel, link)
        else:
            entry = _one_texture(channel, inputs[channel], int(link["texture"]))
        channels[channel] = entry
        for ref in entry["references"]:
            if ref is not None and ref not in textures:
                textures.append(ref)
    return {"flags": word(node["flags"]) & ~MATERIAL_UNUSED_FLAG, "channels": channels, "textures": textures,
            "volume_colour": singles(node["volume_color"]), "refraction": single(node["refraction"]),
            "volume_numbers": [single(node["volume_edge"]), single(node["volume_sharpness"]),
                               single(node["volume_quality"])]}


def reference_indices(described):
    """The references of every channel as places in the description's list of textures (the textures a channel
    names, in the order they are first named; each once however many channels name it), in place of columns.
    ; README (picture:
    `references` [0, null, null] for the picture as reference A)."""
    order = described["textures"]
    return {channel: [None if ref is None else order.index(ref) for ref in entry["references"]]
            for channel, entry in described["channels"].items()}


def texture_name(material, column):
    """(Textures in the export): the description's name of a material's
    texture of one column; unique in a description because a material's columns are."""
    return "%s:%d" % (material, column)


def describe_references(described, material):
    """(Textures in the export): `textured_material`'s channels in the form
    of the description: the references (columns) become `a_tex`, `b_tex` and `alpha_tex` (texture names), and a
    side the channel does not use (None) is left out. The list of `textures` (columns) is not kept."""
    channels = {}
    for channel, entry in described["channels"].items():
        out = {"switches": entry["switches"], "a": entry["a"]}
        for key in ("b", "alpha"):
            if entry[key] is not None:
                out[key] = entry[key]
        for key, ref in zip(("a_tex", "b_tex", "alpha_tex"), entry["references"]):
            if ref is not None:
                out[key] = texture_name(material, ref)
        channels[channel] = out
    kept = {k: v for k, v in described.items() if k not in ("channels", "textures")}
    kept["channels"] = channels
    return kept
