# SPDX-License-Identifier: GPL-3.0-or-later
# (Shader operators). Pure: the Principled BSDF to Bryce channels table.
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import defaults, names, principled_map  # noqa: E402


class Table(unittest.TestCase):
    def test_new_material_is_ba10s(self):
        v = principled_map.new_material_value()
        self.assertEqual(v["flags"], defaults.NEW_MATERIAL["flags"])
        self.assertEqual(v["channels"]["diffusion"]["value"], 1.0)
        self.assertEqual(v["channels"]["ambient_color"]["value"], [1.0, 1.0, 1.0])
        self.assertEqual(set(v["channels"]), set(names.CHANNELS))

    def test_each_channel(self):
        v = principled_map.material_value((1.0, 0.0, 0.21404114), 0.25, 0.5, 0.75, (0, 0, 0, 1), 0.0)
        c = v["channels"]
        self.assertEqual([round(x, 3) for x in c["diffuse_color"]["value"]], [1.0, 0.0, 0.5])
        self.assertAlmostEqual(c["specularity"]["value"], 0.75)
        self.assertAlmostEqual(c["metallicity"]["value"], 0.5)
        self.assertAlmostEqual(c["transparency"]["value"], 0.25)
        self.assertEqual(c["ambience"]["value"], defaults.NEW_MATERIAL["values"]["ambience"])  # no emission: kept

    def test_emission_and_clamps(self):
        c = principled_map.material_value((0, 0, 0), 2.0, 3.0, -1.0, (1.0, 1.0, 1.0), 5.0)["channels"]
        self.assertEqual((c["specularity"]["value"], c["metallicity"]["value"], c["transparency"]["value"]),
                         (0.0, 1.0, 1.0))
        self.assertEqual((c["ambience"]["value"], c["ambient_color"]["value"]), (1.0, [1.0, 1.0, 1.0]))

    def test_neutral_inputs_change_nothing(self):
        """The added inputs at Blender's neutral values give exactly the first table's material."""
        args = ((0.5, 0.2, 0.1), 0.3, 0.4, 0.6, (0.1, 0.2, 0.3, 1.0), 2.0)
        plain = principled_map.material_value(*args)
        neutral = principled_map.material_value(*args, transmission=0.0, ior=1.5, specular_level=0.5,
                                                specular_tint=(1.0, 1.0, 1.0, 1.0), thin_wall=False)
        self.assertEqual(plain, neutral)
        new = principled_map.new_material_value()
        for key in ("reflection", "specular_color", "transparent_color"):
            self.assertEqual(plain["channels"][key], new["channels"][key])
        self.assertEqual(plain["refraction"], defaults.NEW_MATERIAL["refraction"])

    def test_glass(self):
        """Transmission: the transparency, the base colour as the transparent colour, the IOR as refraction."""
        v = principled_map.material_value((1.0, 0.0, 0.21404114), 0.0, 0.0, 1.0, (0, 0, 0), 0.0,
                                          transmission=1.0, ior=1.33)
        c = v["channels"]
        self.assertEqual(c["transparency"]["value"], 1.0)
        self.assertEqual([round(x, 3) for x in c["transparent_color"]["value"]], [1.0, 0.0, 0.5])
        self.assertEqual(v["refraction"], 1.33)

    def test_thin_wall_does_not_bend(self):
        v = principled_map.material_value((1, 1, 1), 0.0, 0.0, 1.0, (0, 0, 0), 0.0, transmission=1.0, ior=1.5,
                                          thin_wall=True)
        self.assertEqual((v["channels"]["transparency"]["value"], v["refraction"]), (1.0, 1.0))

    def test_ior_needs_transmission(self):
        """Alpha alone passes light straight through: no refraction, an untinted transparent colour."""
        v = principled_map.material_value((1.0, 0.0, 0.0), 0.5, 0.0, 0.5, (0, 0, 0), 0.0, ior=2.0)
        self.assertEqual(v["refraction"], 1.0)
        self.assertEqual(v["channels"]["transparent_color"]["value"], [1.0, 1.0, 1.0])
        v = principled_map.material_value((1, 1, 1), 0.0, 0.0, 1.0, (0, 0, 0), 0.0, transmission=1.0, ior=0.0)
        self.assertEqual(v["refraction"], 1.0)  # an IOR of 0 or less has no Bryce meaning: left

    def test_alpha_and_transmission_add(self):
        """Half the surface by alpha, half of the rest's non-metallic share by transmission."""
        v = principled_map.material_value((0.0, 0.0, 0.0), 0.5, 0.5, 0.5, (0, 0, 0), 0.0, transmission=0.5)
        c = v["channels"]
        glass = 0.5 * 0.5 * 0.5
        self.assertAlmostEqual(c["transparency"]["value"], 0.5 + glass)
        share = glass / (0.5 + glass)  # black base colour: white mixed toward black by the transmitted share
        self.assertEqual([round(x, 9) for x in c["transparent_color"]["value"]], [round(1.0 - share, 9)] * 3)
        metal = principled_map.material_value((1, 1, 1), 0.0, 1.0, 1.0, (0, 0, 0), 0.0, transmission=1.0, ior=1.5)
        self.assertEqual((metal["channels"]["transparency"]["value"], metal["refraction"]), (0.0, 1.0))

    def test_specular_level(self):
        """Above 0.5: reflection, full at 1; below 0.5 the highlight fades to none at 0."""
        def at(level):
            return principled_map.material_value((1, 1, 1), 0.2, 0.0, 1.0, (0, 0, 0), 0.0,
                                                 specular_level=level)["channels"]
        self.assertEqual([at(x)["reflection"]["value"] for x in (0.0, 0.5, 0.75, 1.0, 3.0)], [0.0, 0.0, 0.5, 1.0, 1.0])
        self.assertAlmostEqual(at(0.25)["specularity"]["value"], 0.4)
        self.assertEqual(at(0.0)["specularity"]["value"], 0.0)
        self.assertAlmostEqual(at(1.0)["specularity"]["value"], 0.8)

    def test_diffusion_shares(self):
        """Diffusion keeps what Blender's diffuse layer gets under the transmission and the added reflection."""
        def diffusion(**kw):
            return principled_map.material_value((1, 1, 1), 0.2, kw.pop("metallic", 0.0), 1.0, (0, 0, 0), 0.0,
                                                 **kw)["channels"]["diffusion"]["value"]
        self.assertEqual(diffusion(transmission=1.0), 0.0)
        self.assertEqual(diffusion(specular_level=1.0), 0.0)
        self.assertAlmostEqual(diffusion(transmission=0.5, metallic=0.5), 0.75)
        self.assertAlmostEqual(diffusion(transmission=0.5, specular_level=0.75), 0.25)
        self.assertEqual(diffusion(metallic=1.0), 1.0)  # the first table's result

    def test_specular_tint(self):
        v = principled_map.material_value((1, 1, 1), 0.2, 0.0, 1.0, (0, 0, 0), 0.0,
                                          specular_tint=(0.21404114, 1.0, 0.0, 1.0))
        self.assertEqual([round(x, 3) for x in v["channels"]["specular_color"]["value"]], [0.5, 1.0, 0.0])

    def test_not_carried(self):
        self.assertEqual(principled_map.not_carried({"Coat Weight": 0.0, "Sheen Weight": 0.0}), [])
        self.assertEqual(principled_map.not_carried({"Sheen Weight": 0.2, "Coat Weight": 1.0, "IOR": 1.2}),
                         ["Coat Weight", "Sheen Weight"])


if __name__ == "__main__":
    unittest.main()
