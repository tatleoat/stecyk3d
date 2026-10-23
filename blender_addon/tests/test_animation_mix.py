# SPDX-License-Identifier: GPL-3.0-or-later
# (Animation: the document at its opening time). Pure: the reader's `mix` of a
# material opened between two keys  becomes a second material and one amount.
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import components, names  # noqa: E402


def channel(name, a, refs=(None, None, None), word=0):
    colour = name in names.COLOUR_CHANNELS
    return {"a": [a] * 3 if colour else a, "b": [0.0] * 3 if colour else 1.0, "alpha": 1.0, "word": word,
            "refs": list(refs)}


def record(refs_for_diffuse):
    return {"name": "m", "flags": 0, "volume_color": [1.0, 1.0, 1.0], "refraction": 1.0,
            "volume_numbers": [0.0, 1.0, 0.75],
            "channels": {n: channel(n, 0.5, refs_for_diffuse if n == "diffuse_color" else (None,) * 3,
                                    1 << 28 if n == "diffuse_color" and refs_for_diffuse[0] is not None else 0)
                         for n in names.CHANNELS}}


TEXTURE = {"kind": "procedural", "name": "t", "mapping": {"mode": 0}, "record": {}}


class SecondMaterial(unittest.TestCase):
    def test_no_mix_no_second(self):
        m = components.material_from_record(record((0, None, None)), [TEXTURE])
        self.assertNotIn("mix", m)

    def test_mix_is_second_material_and_amount(self):
        rec = record((0, None, None))
        second = channel("diffuse_color", 0.9, (1, None, None), 1 << 28)
        rec["mix"] = {"amounts": [0.25] + [0.0] * 12, "colors": [second, None, None, None, None],
                      "values": [None] * 8}
        m = components.material_from_record(rec, [TEXTURE, dict(TEXTURE, name="u")])
        self.assertEqual(m["mix"]["amount"], 0.25)
        other = m["mix"]["second"]
        for got in other["channels"]["diffuse_color"]["value"]:
            self.assertAlmostEqual(got, 0.9, places=6)
        self.assertEqual(other["channels"]["diffuse_color"]["link"]["texture"], other["components"][-1]["column"])
        # the second material's textures stand in columns the first does not use
        first_columns = {c["column"] for c in m["components"]}
        self.assertFalse(first_columns & {c["column"] for c in other["components"]})
        self.assertNotIn("mix", other)


if __name__ == "__main__":
    unittest.main()
