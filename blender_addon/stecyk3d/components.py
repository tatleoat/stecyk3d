# SPDX-License-Identifier: GPL-3.0-or-later
# (a material preset replaces the material's Bryce settings: flags, the thirteen channels' A and
#       their other settings, the textures and the volume settings; a new material named as the preset is offered
#       where there is none; a texture preset adds a component in the first free letter, named as offered, with
# its mapping, linked to no channel; nothing chosen: refused; a fifth component: refused, Limits)
# (the texture components: copy a component of another material into the first free letter, a
#       picture component without a mapping, show, remove the shown one keeping the other letters, add a mapping;
#       the refusals)
# (what a channel's word and references mean: one texture, or a mix by altitude or by alpha; which
#       output a channel takes)
# (a material's components)
"""A material's Bryce settings and texture components as plain values, and the commands on them. Pure Python."""

import copy

from . import names
from .numbers import single, singles, word

MATERIAL_LETTERS = 4  # Limits (the manual's four textures a material: A to D)
VOLUME_BITS = 0x100 | 0x200 | 0x400  # Accuracy, [format]
MIX_ALTITUDE_BIT = 0x2 # [format]
MIX_ALPHA_BIT = 0x4  # [format]
OUTPUT_BITS = (("v", 0x1), ("c", 0x2), ("b", 0x4))

NO_MAPPING = {"mode": 0, "flags": 0, "scale": [1.0, 1.0, 1.0], "offset": [0.0, 0.0, 0.0],
              "rotation": [0.0, 0.0, 0.0, 1.0]}


class Refused(Exception):
    """A command refused; nothing changed (Errors). The message is for the user."""


def letter(column):
    return names.component_letter(column)


def column_of(letter_text):
    for column in range(64):
        if names.component_letter(column) == letter_text:
            return column
    raise Refused("%r is not a component letter" % (letter_text,))


def channel_output(channel):
    """: colour channels take a texture's `color`, `bump` its `bump`, every other value channel its `alpha`."""
    if channel in names.COLOUR_CHANNELS:
        return "color"
    return "bump" if channel == "bump" else "alpha"


def empty_material():
    """A material with no Bryce settings of its own yet (the constants 0, no component)."""
    channels = {}
    for c in names.COLOUR_CHANNELS:
        channels[c] = {"value": [0.0, 0.0, 0.0], "link": None}
    for c in names.VALUE_CHANNELS:
        channels[c] = {"value": 0.0, "link": None}
    return {"flags": 0, "volume_color": [0.0, 0.0, 0.0], "refraction": 0.0, "volume_edge": 0.0,
            "volume_sharpness": 0.0, "volume_quality": 0.0, "channels": channels, "components": [],
            "shown": None}


def outputs_text(word_value):
    """The vectors' spelling of an outputs word: "v", "vc", "vcb" ... (`gives`)."""
    return "".join(ch for ch, bit in OUTPUT_BITS if word_value & bit)


# ---------------------------------------------------------------- from the reader  to components

def component_from_texture(texture, name, pictures=()):
    """: a texture of the reader  as a component: a procedural one with its record and mapping, or a
    picture one with its mapping and its picture's name. The column is given when it is added."""
    if texture["kind"] == "picture":
        index = texture.get("picture")
        image = pictures[index]["name"] if index is not None and index < len(pictures) else None
        return {"column": None, "kind": "picture", "name": name, "record": None,
                "mapping": copy.deepcopy(texture["mapping"]), "image": image}
    return {"column": None, "kind": "procedural", "name": name, "record": copy.deepcopy(texture["record"]),
            "mapping": copy.deepcopy(texture["mapping"]), "image": None}


def material_from_record(material, textures, pictures=(), first_column=0):
    """: a material of the reader  as a Bryce material value: its flags, volume settings,
    each channel's A with its link , and every texture its channels name as a component, lettered in the
    order first named from `first_column`."""
    out = empty_material()
    out["flags"] = word(material["flags"])
    out["volume_color"] = singles(material["volume_color"])
    out["refraction"] = single(material["refraction"])
    edge, sharpness, quality = material["volume_numbers"]
    out["volume_edge"], out["volume_sharpness"], out["volume_quality"] = single(edge), single(sharpness), single(
        quality)
    columns = {}  # texture index -> column

    def column_for(index):
        if index is None:
            return None
        if index not in columns:
            columns[index] = first_column + len(columns)
            t = textures[index]
            comp = component_from_texture(t, t.get("name", ""), pictures)
            comp["column"] = columns[index]
            out["components"].append(comp)
        return columns[index]

    for name in names.CHANNELS:
        ch = material["channels"][name]
        colour = name in names.COLOUR_CHANNELS
        value = singles(ch["a"]) if colour else single(ch["a"])
        if name == "volume_density" and not out["flags"] & VOLUME_BITS:
            value = 0.0  # Accuracy (-10-22): no volume bit, density 0 whatever is stored
        refs = [column_for(r) for r in ch["refs"]]
        w = word(ch["word"])
        link = None
        if w & (MIX_ALTITUDE_BIT | MIX_ALPHA_BIT):
            b = singles(ch["b"]) if colour else single(ch["b"])
            link = {"mix": "ALTITUDE" if w & MIX_ALTITUDE_BIT else "ALPHA",
                    "a": {"value": value, "texture": refs[0]},
                    "b": {"value": b, "texture": refs[1]},
                    "alpha": {"value": single(ch["alpha"]), "texture": refs[2]}}
        elif w and refs[0] is not None:
            link = {"texture": refs[0], "output": channel_output(name)}
        out["channels"][name] = {"value": value, "link": link}
    if out["components"]:
        out["shown"] = out["components"][-1]["column"]
    mix = material.get("mix")
    if mix:
        out["mix"] = _second_material(material, mix, textures, pictures, first_column + len(columns))
    return out


def _second_material(material, mix, textures, pictures, first_column):
    """(Animation: the document at its opening time). The second channels the
    reader gives for a document opened between two keys  as a second Bryce material and one Mix Materials
    amount: the second material is the first with each second channel put in, and the amount is that of the first
    channel that has one (a Mix Materials node has one amount for all channels; the amounts of a track's channels
    come from one time, so they agree where the keys do). Returns {"second": material value, "amount": m}."""
    entries = list(mix["colors"]) + list(mix["values"])
    second = dict(material)
    second.pop("mix", None)
    second["channels"] = dict(material["channels"])
    amount = 0.0
    for name, entry, share in zip(names.CHANNELS, entries, mix["amounts"]):
        if entry is None:
            continue
        second["channels"][name] = entry
        if amount == 0.0:
            amount = float(share)
    return {"second": material_from_record(second, textures, pictures, first_column), "amount": amount}


# ----------------------------------------------------------------: applying a preset

def preset_target(active_material_name, offered_name):
    """: the Blender material a material preset fills: (name, made). The material shown keeps its name;
    where there is none a new one is made, named as the preset is offered."""
    if active_material_name is not None:
        return active_material_name, False
    return offered_name, True


def apply_material_preset(library, index):
    """: the Bryce material a material preset makes; nothing of the material before is kept.
    Refused when no library or no preset is chosen, or the library holds no such preset."""
    preset = _chosen(library, index, "materials")
    return material_from_record(preset["material"], library.get("textures", []), library.get("pictures", []))


def _chosen(library, index, kind):
    if library is None or index is None:
        raise Refused("choose a library and a preset first")
    if library.get("kind") != kind:
        raise Refused("the library chosen is not a %s library" % kind[:-1])
    for p in library["presets"]:
        if p["index"] == index:
            if "unreadable" in p:
                # ,: a preset whose record cannot be read changes nothing
                # and gives the reader's reason; the library's other presets stay usable
                raise Refused("the preset cannot be read: %s" % p["unreadable"])
            return p
    raise Refused("the library holds no preset %d" % index)


def texture_preset_component(library, index):
    """: the component a texture preset adds: its texture with the mapping it holds, named as offered."""
    from .presets import offered_name
    preset = _chosen(library, index, "textures")
    return component_from_texture(preset["texture"], offered_name(preset["name"], preset["index"]),
                                  library.get("pictures", []))


# ----------------------------------------------------------------: the commands

def columns_of(material):
    return [c["column"] for c in material["components"]]


def first_free_column(material):
    """: the first component letter not taken, A then B, C, D; None when all four are."""
    taken = set(columns_of(material))
    for column in range(MATERIAL_LETTERS):
        if column not in taken:
            return column
    return None


def find(material, column):
    for comp in material["components"]:
        if comp["column"] == column:
            return comp
    return None


def add_component(material, component):
    """: a new value with `component` added in the first free letter, linked to no channel, shown."""
    column = first_free_column(material)
    if column is None:
        raise Refused("the material has four texture components (A to D) already; remove one first")
    out = copy.deepcopy(material)
    comp = copy.deepcopy(component)
    comp["column"] = column
    out["components"].append(comp)
    out["components"].sort(key=lambda c: c["column"])
    out["shown"] = column
    return out


def copy_component(material, source, letter_text):
    """: an independent copy, with its mapping, of the source material's component `letter_text`, added in
    the first free letter. Refused with no source material or no such component."""
    if source is None:
        raise Refused("no material to copy the texture from")
    comp = find(source, column_of(letter_text))
    if comp is None:
        raise Refused("the material has no texture component %s" % letter_text)
    return add_component(material, comp)


def add_picture(material, image_name):
    """: a picture component of the Blender image named, without a mapping. Refused with none named."""
    if not image_name:
        raise Refused("name the picture (a Blender image) to add")
    return add_component(material, {"column": None, "kind": "picture", "name": image_name, "record": None,
                                    "mapping": None, "image": image_name})


def show(material, letter_text):
    """: the component the panel shows becomes `letter_text`."""
    column = column_of(letter_text)
    if find(material, column) is None:
        raise Refused("the material has no texture component %s" % letter_text)
    out = copy.deepcopy(material)
    out["shown"] = column
    return out


def shown_column(material):
    """: the component shown: the one last added or last shown, and the first when none was."""
    if material["shown"] is not None and find(material, material["shown"]) is not None:
        return material["shown"]
    if material["components"]:
        return material["components"][0]["column"]
    return None


def remove_shown(material):
    """: removes the component shown, with its mapping; the other letters do not change; the channels that
    took it become constants. Refused when the material has no component."""
    column = shown_column(material)
    if column is None:
        raise Refused("the material has no texture component to remove")
    out = copy.deepcopy(material)
    out["components"] = [c for c in out["components"] if c["column"] != column]
    for ch in out["channels"].values():
        link = ch["link"]
        if link is None:
            continue
        if "texture" in link and link["texture"] == column:
            ch["link"] = None
        elif "mix" in link:
            for side in ("a", "b", "alpha"):
                if link[side]["texture"] == column:
                    link[side]["texture"] = None
    out["shown"] = None
    return out


def add_mapping(material):
    """: the component shown gets a mapping that starts as what it had without one."""
    column = shown_column(material)
    if column is None:
        raise Refused("the material has no texture component")
    out = copy.deepcopy(material)
    comp = find(out, column)
    if comp["mapping"] is None:
        comp["mapping"] = copy.deepcopy(NO_MAPPING)
    return out


def randomize_component(comp, rng):
    """: a copy of the component
    with settings drawn at random: each noise's frequency (0.25 to 8 on each axis), its two turn angles (0 to 2 pi)
    and the phase amount (0 to 1). Which settings and how drawn is ours (Bryce's own randomizing is not
    reproduced); a picture component has none to draw and is returned as it is."""
    import math
    out = copy.deepcopy(comp)
    if out["kind"] != "procedural":
        return out
    record = out["record"]
    for c in record["components"]:
        c["frequency"] = [single(rng.uniform(0.25, 8.0)) for _ in range(3)]
        c["rotation"] = [single(rng.uniform(0.0, 2.0 * math.pi)) for _ in range(2)]
    record["phase"]["amount"] = single(rng.uniform(0.0, 1.0))
    return out


def replace_component(material, comp):
    """: a new value whose component in `comp`'s letter is `comp`; the letters, the shown
    component and the channels' links stay. Refused when the material has no component in that letter."""
    out = copy.deepcopy(material)
    for k, old in enumerate(out["components"]):
        if old["column"] == comp["column"]:
            out["components"][k] = copy.deepcopy(comp)
            return out
    raise Refused("the material has no texture component %s" % letter(comp["column"]))


def describe_components(material):
    """The components as the vectors list them: column letter, kind, noise types, outputs, mapping, name."""
    out = []
    for comp in material["components"]:
        entry = {"column": letter(comp["column"]), "kind": comp["kind"], "has_mapping": comp["mapping"] is not None,
                 "name": comp["name"]}
        if comp["kind"] == "procedural":
            record = comp["record"]
            entry["noise_types"] = [c["noise"]["kind"] for c in record["components"]]
            entry["outputs"] = outputs_text(record["outputs"])
            entry["first_frequencies"] = list(record["components"][0]["frequency"])
        else:
            entry["image"] = comp["image"]
        out.append(entry)
    return out
