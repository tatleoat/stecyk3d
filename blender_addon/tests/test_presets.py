# SPDX-License-Identifier: GPL-3.0-or-later
# A material preset whose mixed channel holds a constant that is not a number (the reader's null): "Distant
# Mountain" and "Whole Mountain" (Planes&Terrains), "Riverbed" and "Wet Rock" (Rocks&Stones) of Bryce 5.0.
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import components, names  # noqa: E402

TEXTURE = {"kind": "procedural", "name": "t", "mapping": {"mode": 0}, "record": {}}
ALPHA_MIX = 0x4
ALTITUDE_MIX = 0x2


def channel(name, refs=(None, None, None), word=0, b=None, alpha=1.0):
    colour = name in names.COLOUR_CHANNELS
    if b is None:
        b = [0.0] * 3 if colour else 1.0
    return {"a": [0.5] * 3 if colour else 0.5, "b": b, "alpha": alpha, "word": word, "refs": list(refs)}


def library(changed):
    """A library of one material preset whose channels are plain but for `changed` {name: channel}."""
    channels = {n: changed.get(n) or channel(n) for n in names.CHANNELS}
    material = {"name": "m", "flags": 0x10000, "volume_color": [1.0] * 3, "refraction": 1.0,
                "volume_numbers": [0.0, 1.0, 0.75], "channels": channels}
    return {"kind": "materials", "presets": [{"index": 0, "name": "p", "description": "", "material": material}],
            "textures": [dict(TEXTURE, name=str(k)) for k in range(3)], "pictures": []}


class NotANumber(unittest.TestCase):
    def test_a_mixed_channels_unread_constants_become_0(self):
        # Riverbed's Ambient: mixed by alpha, a texture in each slot, alpha stored as no number, B's numbers
        # as stored (read as they are).
        b = [9.183549615799121e-41, 2.8432106545937733e-29, 0.0]
        lib = library({"ambient_color": channel("ambient_color", (0, 1, 2), ALPHA_MIX, b=b, alpha=None),
                       "transparent_color": channel("transparent_color", b=[0.0, 0.0, None])})
        value = components.apply_material_preset(lib, 0)
        link = value["channels"]["ambient_color"]["link"]
        self.assertEqual(link["mix"], "ALPHA")
        self.assertEqual(link["alpha"], {"value": 0.0, "texture": 2})
        self.assertEqual(link["b"]["value"], [components.single(x) for x in b])
        # an unmixed channel's B is not taken at all
        self.assertIsNone(value["channels"]["transparent_color"]["link"])

    def test_part_of_a_colour_that_is_no_number_is_0(self):
        lib = library({"diffuse_color": channel("diffuse_color", (0, 1, 2), ALPHA_MIX, b=[0.25, None, 0.5])})
        link = components.apply_material_preset(lib, 0)["channels"]["diffuse_color"]["link"]
        self.assertEqual(link["b"]["value"], [0.25, 0.0, 0.5])

    def test_the_mix_by_altitude_never_reads_alpha(self):
        lib = library({"bump": channel("bump", (0, 1, None), ALTITUDE_MIX, alpha=None)})
        link = components.apply_material_preset(lib, 0)["channels"]["bump"]["link"]
        self.assertEqual(link["mix"], "ALTITUDE")
        self.assertEqual(link["alpha"], {"value": 0.0, "texture": None})

    def test_the_volume_density_of_a_material_that_is_no_volume_is_not_read(self):
        # "Tutorial Light Gel" (Tutorial.mat): flags 0x10048, its Volume density's A and B no numbers
        lib = library({"volume_density": channel("volume_density", b=None)})
        lib["presets"][0]["material"]["channels"]["volume_density"].update(a=None, b=None)
        self.assertEqual(components.apply_material_preset(lib, 0)["channels"]["volume_density"]["value"], 0.0)
        lib["presets"][0]["material"]["flags"] |= 0x100
        with self.assertRaisesRegex(components.Refused, "volume_density A is not a number"):
            components.apply_material_preset(lib, 0)

    def test_a_constant_the_renderer_would_read_is_refused_naming_it(self):
        lib = library({"diffusion": channel("diffusion", (0, None, 2), ALPHA_MIX, b=None)})
        lib["presets"][0]["material"]["channels"]["diffusion"]["b"] = None
        with self.assertRaisesRegex(components.Refused, "diffusion B is not a number"):
            components.apply_material_preset(lib, 0)
        lib = library({"ambience": channel("ambience", (0, 1, None), ALPHA_MIX, alpha=None)})
        with self.assertRaisesRegex(components.Refused, "ambience alpha is not a number"):
            components.apply_material_preset(lib, 0)
        lib = library({})
        lib["presets"][0]["material"]["channels"]["specularity"]["a"] = None
        with self.assertRaisesRegex(components.Refused, "specularity A is not a number"):
            components.apply_material_preset(lib, 0)


if __name__ == "__main__":
    unittest.main()
