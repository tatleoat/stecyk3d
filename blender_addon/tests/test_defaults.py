# SPDX-License-Identifier: GPL-3.0-or-later
# (Defaults)
"""The defaults table names real settings, with values of their types and their own relations."""

import math
import sys
import unittest
from unittest import mock

sys.modules.setdefault("bpy", mock.MagicMock())

from stecyk3d import defaults, names  # noqa: E402
from stecyk3d import props  # noqa: E402


class DefaultsTable(unittest.TestCase):
    def test_every_key_is_a_setting_of_its_group(self):
        tables = dict(props.GROUPS)
        for (group, name), value in defaults.GROUP_DEFAULTS.items():
            row = {n: (k, e) for n, k, e in tables[group]}
            self.assertIn(name, row, (group, name))
            kind, extra = row[name]
            if kind.endswith("]"):
                self.assertEqual(len(value), int(kind[kind.index("[") + 1:-1]), (group, name))
            if kind == "enum":
                self.assertIn(value, extra, (group, name))

    def test_sun_direction_axes(self):
        x, y, z = defaults.SUN_DIRECTION_DESCRIPTION
        self.assertEqual(defaults.SUN_DIRECTION_BLENDER, (x, z, y))  # : Blender's (x, y, z) is (x, z, y)
        self.assertAlmostEqual(math.sqrt(x * x + y * y + z * z), 1.0, places=15)

    def test_new_material_names_every_channel(self):
        new = defaults.NEW_MATERIAL
        self.assertEqual(set(new["colours"]), set(names.COLOUR_CHANNELS))
        self.assertEqual(set(new["values"]), set(names.VALUE_CHANNELS))
        self.assertEqual(new["flags"], 0x10000)

    def test_light_material(self):
        m = defaults.new_light_material()
        self.assertEqual((m["flags"], m["values"]["transparency"], m["colours"]["diffuse_color"]),
                         (0x10004, 0.5, (1.0, 1.0, 1.0)))
        self.assertEqual(defaults.NEW_MATERIAL["colours"]["diffuse_color"], (0.499, 0.499, 0.499))  # not shared

    def test_five_rows_by_program(self):
        self.assertEqual(defaults.program_default("sky", "flags", "3.1"), 0x3E000283)
        self.assertEqual(defaults.program_default("sky", "flags", "5.0"), 0x7E0002B3)
        self.assertEqual(defaults.program_default("sky", "cloud_height", "5.0"), 18.06640625)
        self.assertEqual(defaults.program_default("light", "flags", "5.0"), 0x100)
        self.assertEqual(defaults.program_default("light", "intensity", "5.0"), 5.0)  # no 5.0 value: the table's

    def test_switching_the_program_changes_only_untouched_settings(self):
        sw = defaults.switched_value
        self.assertEqual(sw("sky", "cloud_height", 36.1328125, "5.0"), 18.06640625)
        self.assertEqual(sw("sky", "cloud_height", 18.06640625, "3.0"), 36.1328125)
        self.assertIsNone(sw("sky", "cloud_height", 20.0, "5.0"))  # set by the user or a file: kept
        self.assertIsNone(sw("sky", "cover", 0.5, "5.0"))  # no 5.0 row
        self.assertIsNone(sw("sky", "flags", 0x7E0002B3, "5.0"))  # already 5.0's

    def test_new_texture_mapping(self):
        self.assertEqual(defaults.NEW_MAPPING["mode"], 1)
        self.assertEqual(defaults.NEW_MAPPING["scale"], (0.51, 0.51, 0.51))


if __name__ == "__main__":
    unittest.main()
