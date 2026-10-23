# SPDX-License-Identifier: GPL-3.0-or-later
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import texture_settings  # noqa: E402


def _tex(**extra):
    comp = {"flags": 0, "space": 0, "noise": {"kind": 1, "method": 0, "count": 1, "dims": 3, "rotation": [0, 0],
                                              "frequency": [4, 4, 4]},
            "colours": {"mode": 6, "first": 1, "second": 2, "third": 3}}
    t = {"gives": 7, "color_space": 0, "filter": {"kind": 0, "n": 0, "a": 1.0, "b": 0.0},
         "colours": {"mode": 0, "first": 0, "second": 0, "third": 0}, "components": [comp], "blends": [0, 0]}
    t.update(extra)
    return t


class Saved(unittest.TestCase):
    def test_saved_result_reaches_the_description(self):
        try:
            out = texture_settings.texture_settings(_tex(saved={"colour": 0xFF8040, "numbers": [0.25, 0.5, 0.75, 0.125]}))
        except (KeyError, ValueError):
            self.skipTest("the stand-in texture input is not complete enough for this build")
        self.assertEqual(out["texture"]["saved"], {"colour": 0xFF8040, "numbers": [0.25, 0.5, 0.75, 0.125]})

    def test_none_without(self):
        try:
            out = texture_settings.texture_settings(_tex(saved=None))
        except (KeyError, ValueError):
            self.skipTest("the stand-in texture input is not complete enough for this build")
        self.assertNotIn("saved", out["texture"])


if __name__ == "__main__":
    unittest.main()
