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


if __name__ == "__main__":
    unittest.main()
