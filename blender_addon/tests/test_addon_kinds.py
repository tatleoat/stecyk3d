# SPDX-License-Identifier: GPL-3.0-or-later
# (The add-on sends what the reader takes)
"""The committed fixture the Rust reader's test reads is what the add-on's functions make now."""

import json
import os
import unittest

import addon_kinds

FIXTURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "addon-kinds.json")


class AddOnKinds(unittest.TestCase):
    def test_fixture_is_what_the_add_on_makes(self):
        with open(FIXTURE, encoding="utf-8") as f:
            self.assertEqual(f.read(), addon_kinds.text())

    def test_each_kind_is_present(self):
        d = json.loads(addon_kinds.text())
        self.assertEqual([l["kind"] for l in d["lights"]], ["pyramid", "cylinder", "cone"])
        self.assertEqual(len(d["lights"][1]["exponents"]), 1)
        self.assertEqual(len(d["lights"][0]["exponents"]), 2)
        gradient = d["lights"][2]["gradient_keys"]
        self.assertEqual([k["place"] for k in gradient["colour_keys"]], [0.0, 0.5])  # sorted by place
        shapes = {o["name"]: o["shape"] for o in d["objects"]}
        self.assertEqual(shapes, {"G": "group", "A": "sphere", "B": "cube", "T": "terrain"})
        self.assertEqual(d["objects"][0]["members"]["B"]["cut_shows_positive"], True)
        self.assertEqual(d["textures"][0]["kind"], "picture")
        self.assertEqual(len(d["trees"]), 1)


if __name__ == "__main__":
    unittest.main()
