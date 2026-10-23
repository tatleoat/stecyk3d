# SPDX-License-Identifier: GPL-3.0-or-later
# (the node walk of a Bryce Texture, checked on: the vector's
#       settings are laid out as nodes, read back by the walk, and converted; the result is the expected texture)
# Errors (a graph that makes no texture is refused with a message naming it)
# (Textures in the export)
"""The node walk of texture_nodes.py on stand-in nodes (attribute access only); python3 standard library only."""

import os
import types
import unittest

from stecyk3d import channels, texture_nodes, texture_settings
from test_texture_sky5_vectors import DIR, compare, join_pieces, load


class Socket:
    def __init__(self, value=0.0, source=None):
        self.value = value
        self.links = [types.SimpleNamespace(from_node=source)] if source is not None else []

    @property
    def is_linked(self):
        return bool(self.links)


def node(idname, name, inputs=None, **attrs):
    return types.SimpleNamespace(bl_idname=idname, name=name, inputs=inputs or {}, **attrs)


def noise_node(n, name):
    phase = Socket(n.get("phase_amount", 0.0), n.get("phase_node"))
    attrs = dict(kind=n["kind"], method=n["method"], count=n["count"], dims=n["dims"], custom=n.get("custom", False),
                 rotation=n.get("rotation", [0.0, 0.0]), frequency=n.get("frequency", [1.0] * 3),
                 matrix=n.get("matrix", [0.0] * 9))
    return node("BryceNoiseNode", name, {"phase": phase}, **attrs)


def component_node(c, name):
    phase_node = noise_node(c["phase_noise"], name + "-phase") if c.get("phase_noise") else None
    source = noise_node(dict(c["noise"], phase_amount=c.get("phase_amount", 0.0), phase_node=phase_node), name + "-n")
    if c.get("filter"):
        f = c["filter"]
        source = node("BryceFilterNode", name + "-f", {"value": Socket(source=source)},
                      kind=f["kind"], n=f["n"], a=f["a"], b=f["b"])
    colours = c["colours"]
    return node("BryceComponentNode", name, {"value": Socket(source=source)}, flags=c["flags"], space=c["space"],
                mode=colours["mode"], first=colours["first"], second=colours["second"], third=colours["third"])


def texture_node(t):
    comps = [component_node(c, "c%d" % i) for i, c in enumerate(t["components"])]
    blends = t["blends"]
    top = comps[0]
    if len(comps) >= 2:
        top = node("BryceBlendNode", "b0", {"first": Socket(source=comps[0]), "second": Socket(source=comps[1])},
                   mode=blends[0])
    if len(comps) == 3:
        top = node("BryceBlendNode", "b1", {"first": Socket(source=top), "second": Socket(source=comps[2])},
                   mode=blends[1])
    phase = t.get("phase")
    phase_noise = noise_node(phase["noise"], "tp") if phase and phase.get("noise") else None
    m = t.get("mapping")
    mapping = None
    if m:
        mapping = node("BryceMappingNode", "map", mode=m["mode"], flags=m["flags"], scale=m["scale"],
                       offset=m["offset"], rotation=m["rotation"])
    f, c = t["filter"], t["colours"]
    return node("BryceTextureNode", "tex",
                {"component": Socket(source=top), "phase": Socket(phase["amount"] if phase else 0.0, phase_noise),
                 "vector": Socket(source=mapping)},
                gives=t["gives"], color_space=t["color_space"],
                filter=types.SimpleNamespace(kind=f["kind"], n=f["n"], a=f["a"], b=f["b"]),
                colors=types.SimpleNamespace(mode=c["mode"], first=c["first"], second=c["second"], third=c["third"]))


@unittest.skipUnless(os.path.exists(os.path.join(DIR, "texture.jsonl")), "texture.jsonl not present")
class WalkVectors(unittest.TestCase):
    def test_every_case_through_nodes(self):
        cases = load("texture.jsonl")
        failures = []
        for case in cases:
            case = join_pieces(case)
            got = texture_settings.texture_settings(texture_nodes.texture_input(texture_node(case["texture"])))
            compare(case["expected"], got, case["case"], failures)
        self.assertEqual(failures, [], "\n".join(failures[:30]))


class Refusals(unittest.TestCase):
    def test_no_component(self):
        t = node("BryceTextureNode", "t", {"component": Socket(), "phase": Socket(), "vector": Socket()})
        with self.assertRaises(texture_nodes.TextureGraphError):
            texture_nodes.texture_input(t)

    def test_picture_node(self):
        with self.assertRaises(texture_nodes.TextureGraphError):
            texture_nodes.texture_input(node("BrycePictureNode", "p"))

    def test_two_filters_in_a_row(self):
        inner = node("BryceFilterNode", "f1", {"value": Socket()}, kind=1, n=0, a=0.0, b=0.0)
        outer = node("BryceFilterNode", "f2", {"value": Socket(source=inner)}, kind=1, n=0, a=0.0, b=0.0)
        comp = node("BryceComponentNode", "c", {"value": Socket(source=outer)}, flags=1, space=0, mode=0, first=0,
                    second=0, third=0)
        t = node("BryceTextureNode", "t", {"component": Socket(source=comp), "phase": Socket(), "vector": Socket()})
        with self.assertRaises(texture_nodes.TextureGraphError):
            texture_nodes.texture_input(t)


class NoNoise(unittest.TestCase):
    # , : a component with no Bryce Noise (alone or behind a Bryce Filter)
    # makes a texture; its noise is type 0; the space is add-on design (angles 0, frequencies 1)
    def _read(self, with_filter):
        source = None
        if with_filter:
            source = node("BryceFilterNode", "f", {"value": Socket()}, kind=7, n=0, a=1.0, b=0.25)
        comp = node("BryceComponentNode", "c", {"value": Socket(source=source)}, flags=1, space=0, mode=6,
                    first=0xC829FF, second=0x1EB43C, third=0x283CDC)
        t = node("BryceTextureNode", "t", {"component": Socket(source=comp), "phase": Socket(5.0), "vector": Socket()},
                 gives=1, color_space=0, filter=types.SimpleNamespace(kind=0, n=0, a=0.0, b=0.0),
                 colors=types.SimpleNamespace(mode=0, first=0, second=0, third=0))
        return texture_nodes.texture_input(t)["components"][0]

    def test_type_zero_and_identity_space(self):
        for with_filter in (False, True):
            c = self._read(with_filter)
            self.assertEqual(c["noise"]["kind"], 0)
            self.assertEqual(c["noise"]["rotation"], [0.0, 0.0])
            self.assertEqual(c["noise"]["frequency"], [1.0, 1.0, 1.0])
            self.assertEqual(c["phase_noise"], None)
            self.assertEqual(c["filter"] is not None, with_filter)


class WithOffsets(unittest.TestCase):
    # , item 8: with_offsets is not carried; it no longer refuses a graph
    def test_with_offsets_is_not_carried(self):
        case = load("texture.jsonl")[0]
        t = texture_node(join_pieces(case)["texture"])
        t.with_offsets = True
        self.assertNotIn("with_offsets", texture_nodes.texture_input(t))


class References(unittest.TestCase):
    def test_names(self):
        settings = {"flags": 0, "refraction": 1.0, "volume_edge": 0.0, "volume_sharpness": 0.0, "volume_quality": 0.0,
                    "volume_color": [0.0, 0.0, 0.0]}
        inputs = {c: ([0.5] * 3 if c in ("diffuse_color", "ambient_color", "specular_color", "specular_halo",
                                         "transparent_color") else 0.5) for c in channels.names.CHANNELS}
        described = channels.textured_material(settings, inputs, {"diffusion": {"texture": 1, "output": "alpha"}})
        out = channels.describe_references(described, "M")
        self.assertEqual(out["channels"]["diffusion"]["a_tex"], "M:1")
        self.assertNotIn("b_tex", out["channels"]["diffusion"])
        self.assertNotIn("textures", out)
        self.assertNotIn("b", out["channels"]["ambience"])


if __name__ == "__main__":
    unittest.main()
