# SPDX-License-Identifier: GPL-3.0-or-later
# (The scripting API: its plain parts, scripting.py)
"""The scripting API's plain parts: terrain heights and the surface under a point, channel overrides, choosing a
library and preset, a script's arguments, variants and their parallel runs, the contact sheet's layout and font."""

import math
import os
import sys
import tempfile
import unittest
from unittest import mock

from stecyk3d import components, create, names, scripting
from stecyk3d.numbers import single


def ramp(x, y):
    return 0.25 * x - 0.5 * y


class Terrain(unittest.TestCase):
    def test_refuses_a_side_that_is_not_offered(self):
        for n in (8, 100, 2048):
            with self.assertRaisesRegex(ValueError, "points a side"):
                scripting.TerrainGrid(n, (0, 0), (10, 10), (0, 1), ramp)

    def test_refuses_an_empty_range_and_size(self):
        with self.assertRaisesRegex(ValueError, "z_range"):
            scripting.TerrainGrid(16, (0, 0), (10, 10), (1, 1), ramp)
        with self.assertRaisesRegex(ValueError, "size"):
            scripting.TerrainGrid(16, (0, 0), (0, 10), (0, 1), ramp)

    def test_refuses_a_height_that_is_no_number(self):
        with self.assertRaisesRegex(ValueError, "height function"):
            scripting.TerrainGrid(16, (0, 0), (10, 10), (0, 1), lambda x, y: None)

    def test_place_and_scale_as_blender_holds_them(self):
        g = scripting.TerrainGrid(64, (3.0, -7.0), (48.0, 28.0), (-1.2, 1.0), ramp)
        sx, sz = create.terrain_scales(64, False)
        for value in g.location + g.scale:
            self.assertEqual(value, single(value))
        self.assertAlmostEqual(g.scale[0] * sx * 64, 48.0, places=5)
        self.assertAlmostEqual(g.scale[1] * sx * 64, 28.0, places=5)
        self.assertAlmostEqual(g.location[2] + g.scale[2] * (0 - create.HALF_HEIGHT) * sz, -1.2, places=5)
        self.assertAlmostEqual(g.location[2] + g.scale[2] * (65535 - create.HALF_HEIGHT) * sz, 1.0, places=5)
        self.assertEqual(len(g.heights), 64 * 64)

    def test_grid_points_give_their_heights(self):
        g = scripting.TerrainGrid(32, (0.0, 0.0), (31.0, 31.0), (-20.0, 20.0), ramp)
        cx, cy = g.cell
        for i, j in ((0, 0), (5, 9), (31, 31), (31, 0), (16, 16)):
            x = g.location[0] + (i - 16) * cx
            y = g.location[1] + (j - 16) * cy
            self.assertAlmostEqual(g.z_at(x, y), ramp(x, y), delta=40.0 / 65535)
        self.assertIsNone(g.z_at(g.location[0] - 17 * cx, 0.0))
        self.assertIsNone(g.z_at(0.0, g.location[1] + 15.5 * cy))

    def test_a_cell_is_two_triangles(self):
        # one raised grid point (c + 1, r): the cell's middle lies on the diagonal from (c + 1, r) to (c, r + 1),
        # half way up in the renderer's triangles (a blend of the four corners would give a quarter)
        bump = lambda x, y: 1.0 if abs(x - 1.0) < 1e-4 and abs(y) < 1e-4 else 0.0
        g = scripting.TerrainGrid(16, (0.0, 0.0), (16.0, 16.0), (0.0, 1.0), bump)  # cells of 1 by 1
        self.assertEqual(g.heights[8 * 16 + 9], 65535)
        mid = g.z_at(0.5, 0.5)
        self.assertAlmostEqual(mid, 0.5, places=4)
        self.assertAlmostEqual(g.z_at(0.25, 0.25), 0.25, places=4)  # triangle A: a plane
        self.assertAlmostEqual(g.z_at(0.75, 0.75), 0.25, places=4)  # triangle B: a plane

    def test_heights_outside_the_range_are_held_and_counted(self):
        g = scripting.TerrainGrid(16, (0.0, 0.0), (10.0, 10.0), (-1.0, 1.0), lambda x, y: 5.0 if x > 0 else -0.5)
        self.assertGreater(g.clamped, 0)
        self.assertEqual(max(g.heights), 65535)

    def test_lowest_under_is_the_lowest_of_the_disc(self):
        wave = lambda x, y: math.sin(x * 1.3) * math.cos(y * 0.7) + 0.2 * math.sin(3 * x + y)
        g = scripting.TerrainGrid(64, (0.0, 0.0), (30.0, 20.0), (-2.0, 2.0), wave)
        for x, y, r in ((0.3, 0.1, 1.7), (-5.2, 4.4, 0.6), (8.1, -3.3, 2.5), (1.0, 1.0, 0.05)):
            exact = g.lowest_under(x, y, r)
            sampled = [g.z_at(x + rr * math.cos(a), y + rr * math.sin(a))
                       for rr in [r * k / 40.0 for k in range(41)] for a in [t * math.pi / 180 for t in range(360)]]
            low = min(z for z in sampled if z is not None)
            self.assertLessEqual(exact, low + 1e-12)
            self.assertGreater(exact, low - 1e-2)  # the samples come near the true lowest point (checked finer)

    def test_lowest_under_finds_a_pit_between_samples(self):
        # cells of 1 by 1, one grid point sunk to -1 at (1, 1)
        pit = lambda x, y: -1.0 if abs(x - 1.0) < 1e-4 and abs(y - 1.0) < 1e-4 else 0.0
        g = scripting.TerrainGrid(16, (0.0, 0.0), (16.0, 16.0), (-1.0, 1.0), pit)
        self.assertAlmostEqual(g.lowest_under(0.6, 0.6, 0.6), -1.0, places=4)  # the grid point inside
        # the circle's point downhill of triangle B's plane 1 - x - y
        self.assertAlmostEqual(g.lowest_under(0.5, 0.5, 0.5), -0.5 * math.sqrt(2), places=4)
        self.assertAlmostEqual(g.lowest_under(-0.5, 1.0, 1.0), -0.5, places=4)  # the circle on a cell's edge

    def test_lowest_under_leaves_out_the_parts_off_the_grid(self):
        g = scripting.TerrainGrid(16, (0.0, 0.0), (10.0, 10.0), (0.0, 1.0), lambda x, y: 0.5)
        self.assertAlmostEqual(g.lowest_under(4.5, 0.0, 3.0), 0.5, places=4)
        with self.assertRaisesRegex(ValueError, "no part of the disc"):
            g.lowest_under(50.0, 0.0, 1.0)


class Channels(unittest.TestCase):
    def value(self):
        v = components.empty_material()
        v["channels"]["bump"]["link"] = {"texture": 0, "output": "bump"}
        v["channels"]["diffusion"]["link"] = {"mix": "ALTITUDE", "a": {"value": 0.1, "texture": 0},
                                             "b": {"value": 0.2, "texture": None},
                                             "alpha": {"value": 1.0, "texture": None}}
        return v

    def test_values_go_over_the_constants_as_singles(self):
        v = scripting.override_channels(self.value(), {"reflection": 0.65, "diffuse_color": (0.04, 0.38, 0.68),
                                                      "bump": 0.012}, "test")
        self.assertEqual(v["channels"]["reflection"]["value"], single(0.65))
        self.assertEqual(v["channels"]["diffuse_color"]["value"], [single(0.04), single(0.38), single(0.68)])
        self.assertEqual(v["channels"]["bump"]["value"], single(0.012))  # one texture: the constant is its A

    def test_refusals_name_the_channel(self):
        for channels, words in (({"shine": 1.0}, "'shine' is not a Bryce material channel"),
                                ({"diffuse_color": 0.5}, "diffuse_color is a colour"),
                                ({"diffuse_color": (1, 2)}, "diffuse_color is a colour"),
                                ({"reflection": (1, 2, 3)}, "reflection takes one number"),
                                ({"diffusion": 0.5}, "diffusion is a mix")):
            with self.assertRaisesRegex(ValueError, "^test: .*" + words.replace("(", r"\(")):
                scripting.override_channels(self.value(), channels, "test")

    def test_nothing_to_put(self):
        v = self.value()
        self.assertIs(scripting.override_channels(v, None, "test"), v)


class Choosing(unittest.TestCase):
    OFFERED = [("Waters&Liquids (Corel Bryce 5.0)", "kept:a"), ("User (Corel Bryce 5.0)", "kept:b"),
               ("User (Corel Bryce 5.0)", "kept:c"), ("Rocks", "/x/Rocks.mat")]

    def test_libraries_by_label_or_name(self):
        self.assertEqual(scripting.library_matches(self.OFFERED, "Waters&Liquids"), ["kept:a"])
        self.assertEqual(scripting.library_matches(self.OFFERED, "waters&liquids (corel bryce 5.0)"), ["kept:a"])
        self.assertEqual(scripting.library_matches(self.OFFERED, "User"), ["kept:b", "kept:c"])
        self.assertEqual(scripting.library_matches(self.OFFERED, "Rocks"), ["/x/Rocks.mat"])
        self.assertEqual(scripting.library_matches(self.OFFERED, "Water"), [])

    def test_presets_by_name_or_its_beginning(self):
        offered = [(0, "Shiny and Still", ""), (1, "Shiny Waves", ""), (2, "Riverbed", ""), (3, "Riverbed 2", "")]
        self.assertEqual(scripting.preset_index(offered, "Riverbed", "L"), 2)
        self.assertEqual(scripting.preset_index(offered, "Shiny and", "L"), 0)
        self.assertIsNone(scripting.preset_index(offered, "Lava", "L"))
        with self.assertRaisesRegex(ValueError, "'Shiny' begins 'Shiny and Still', 'Shiny Waves'"):
            scripting.preset_index(offered, "Shiny", "L")


class Arguments(unittest.TestCase):
    def test_after_blenders_end(self):
        a = scripting.script_arguments(["blender", "-b", "--python", "s.py", "--", "out", "--quick", "--moon", "12",
                                        "--name", "x", "--n", "3"])
        self.assertEqual(a.positional, ["out"])
        self.assertTrue(a.has("--quick"))
        self.assertFalse(a.has("--haze"))
        self.assertEqual(a.get("--moon", 9.0), 12.0)
        self.assertEqual(a.get("--n", 1), 3)
        self.assertEqual(a.get("--name", "lake"), "x")
        self.assertEqual(a.get("--haze", 0.02), 0.02)
        self.assertEqual(scripting.script_arguments(["blender"]).argv, [])
        with self.assertRaisesRegex(ValueError, "--x needs a value"):
            scripting.Arguments(["--x"]).get("--x", 1.0)


class Variants(unittest.TestCase):
    def test_names(self):
        self.assertEqual(scripting.variant_items({"moon 12": {}, "b": {"x": 1}}), [("moon_12", {}), ("b", {"x": 1})])
        self.assertEqual([n for n, _ in scripting.variant_items([{}, {}])], ["v1", "v2"])
        self.assertEqual(scripting.safe_name("../a/b"), "a_b")
        with self.assertRaisesRegex(ValueError, "both named 'a_b'"):
            scripting.variant_items({"a b": {}, "a/b": {}})

    def test_flags(self):
        self.assertEqual(scripting.variant_flags({"moon": 12, "no_halo": True, "stars": False, "haze": None}),
                         ["--moon", "12", "--no-halo"])
        self.assertEqual(scripting.variant_flags(["--moon", 3]), ["--moon", "3"])
        self.assertEqual(scripting.variant_command("B", "s.py", "o", "n", {"moon": 1}, ["--quick"]),
                         ["B", "-b", "--python-exit-code", "1", "--python", "s.py", "--", "o", "--name", "n",
                          "--quick", "--moon", "1"])

    def test_parallel_runs_wait_and_name_failures(self):
        with tempfile.TemporaryDirectory() as out:
            calls = []

            def fake(command, stdout, stderr):
                calls.append(command)
                name = command[command.index("--name") + 1]
                if name != "bad":
                    open(os.path.join(command[command.index("--") + 1], name + ".png"), "wb").close()
                stdout.write("ran %s\n" % name)
                return 0

            with mock.patch.object(scripting.subprocess, "call", fake):
                paths = scripting.run_parallel("B", "s.py", {"a": {"moon": 1}, "b": ["--quick"]}, out, jobs=1,
                                               log=lambda *a: None)
                self.assertEqual(paths, [os.path.join(os.path.abspath(out), n + ".png") for n in "ab"])
                self.assertEqual(len(calls), 2)
                self.assertEqual(calls[0][:6], ["B", "-b", "--python-exit-code", "1", "--python", os.path.abspath("s.py")])
                with open(os.path.join(out, "a.log")) as f:
                    self.assertEqual(f.read(), "ran a\n")
                with self.assertRaisesRegex(RuntimeError, r"variants failed: bad \(exit 0, no picture; see .*bad.log\)"):
                    scripting.run_parallel("B", "s.py", {"a": {}, "bad": {}}, out, log=lambda *a: None)


class Sheet(unittest.TestCase):
    def test_layout(self):
        layout = scripting.sheet_layout([(100, 50), (100, 50), (80, 40)], 2, 10, 4)
        self.assertEqual(layout["size"], (2 * 100 + 3 * 4, 2 * (50 + 10) + 3 * 4))
        self.assertEqual(layout["pictures"], [(4, 4), (108, 4), (4 + 10, 4 + 60 + 4 + 5)])
        self.assertEqual(layout["labels"], [(4, 54, 100), (108, 54, 100), (4, 118, 100)])
        self.assertEqual(scripting.sheet_layout([(10, 10)], 3, 0, 0)["size"], (10, 10))
        with self.assertRaises(ValueError):
            scripting.sheet_layout([], 3, 0, 0)

    def test_font(self):
        for char, rows in scripting.GLYPHS.items():
            self.assertEqual(len(rows), 7, char)
            self.assertTrue(all(0 <= r < 32 for r in rows), char)
        mask = scripting.text_mask("Ab", 2)
        self.assertEqual(len(mask), 14)
        self.assertEqual(len(mask[0]), 2 * 6 * 2 - 2)
        self.assertEqual(mask, scripting.text_mask("AB", 2))
        self.assertEqual(scripting.text_mask("~", 1), scripting.text_mask("?", 1))
        self.assertEqual(mask[0][:10], [0, 0, 1, 1, 1, 1, 1, 1, 0, 0])  # the A's top: .###.
        self.assertEqual(len(scripting.text_mask("ABCDEFGH", 1, width=17)[0]), 17)  # three letters fit
        self.assertEqual(scripting.text_mask("", 1), [[] for _ in range(7)])
        self.assertEqual(scripting.label_scale(640), 2)
        self.assertEqual(scripting.label_scale(100), 1)


if __name__ == "__main__":
    unittest.main()
