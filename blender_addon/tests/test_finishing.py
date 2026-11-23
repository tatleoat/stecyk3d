# SPDX-License-Identifier: GPL-3.0-or-later
# (Finishing: glow, lens flare, JPEG round trip, palette; not Bryce's.) Pure, but needs numpy: plain Python without
# it skips these; Blender's own Python has it (blender_finishing.py runs them there).
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import finishing  # noqa: E402

try:
    import numpy as np
except ImportError:  # plain Python: the settings checks still run, the image work is skipped
    np = None

OFF = {"finish_threshold": 0.6, "glow": False, "glow_size": 0.07, "glow_strength": 1.0,
       "glow_tint": (0.62, 0.82, 1.0), "flare": False, "flare_strength": 1.0, "jpeg": False, "jpeg_quality": 75,
       "palette": False, "palette_colours": 256}


def with_(**changes):
    return dict(OFF, **changes)


class Settings(unittest.TestCase):
    def test_off_is_not_wanted(self):
        self.assertFalse(finishing.wanted(OFF))
        self.assertFalse(finishing.wanted({}))
        for switch in finishing.SWITCHES:
            self.assertTrue(finishing.wanted(with_(**{switch: True})))

    def test_the_defaults_are_off(self):
        from unittest import mock
        sys.modules.setdefault("bpy", mock.MagicMock())
        from stecyk3d import defaults
        held = {name: defaults.GROUP_DEFAULTS[("render", name)] for name in finishing.SETTINGS}
        self.assertEqual(held, OFF)
        self.assertFalse(finishing.wanted(held))

    def test_off_has_no_problems_whatever_the_camera(self):
        self.assertEqual(finishing.problems(OFF, "ORTHO"), [])
        self.assertEqual(finishing.problems(with_(palette_colours=1), None), [])

    def test_flare_names_a_camera_that_is_not_perspective(self):
        found = finishing.problems(with_(flare=True), "ORTHO")
        self.assertTrue(any("perspective camera" in p and "ORTHO" in p for p in found), found)
        self.assertFalse(any("perspective" in p for p in finishing.problems(with_(flare=True), "PERSP")))

    def test_out_of_range_numbers_are_named(self):
        self.assertTrue(any("2 to 256" in p for p in finishing.problems(with_(palette=True, palette_colours=1))))
        self.assertTrue(any("1 to 100" in p for p in finishing.problems(with_(jpeg=True, jpeg_quality=0))))
        self.assertTrue(any("size" in p for p in finishing.problems(with_(glow=True, glow_size=0.0))))


def picture(height, width, value=(20, 30, 40)):
    out = np.zeros((height, width, 3), dtype=np.uint8)
    out[...] = value
    return out


@unittest.skipIf(np is None, "numpy is not available")
class Finish(unittest.TestCase):
    def test_off_gives_the_same_bytes(self):
        levels = bytes(range(256)) * 9  # 48 x 16 pixels
        self.assertEqual(finishing.finish(levels, 48, 16, OFF), levels)

    def test_glow_brightens_round_a_bright_spot_and_never_darkens(self):
        p = picture(64, 96)
        p[30:34, 46:50] = 255
        out = np.frombuffer(finishing.finish(p.tobytes(), 96, 64, with_(glow=True)), np.uint8).reshape(64, 96, 3)
        self.assertTrue((out >= p).all())
        self.assertGreater(int(out[32, 56, 2]), int(p[32, 56, 2]))  # beside the spot: brighter, and bluer by the tint
        self.assertGreater(int(out[32, 56, 2]) - int(p[32, 56, 2]), int(out[32, 56, 0]) - int(p[32, 56, 0]))
        near, far = int(out[32, 56].sum()), int(out[2, 2].sum())
        self.assertGreater(near, far)

    def test_glow_with_nothing_bright_changes_nothing(self):
        p = picture(32, 32, (100, 100, 100))
        self.assertEqual(finishing.finish(p.tobytes(), 32, 32, with_(glow=True)), p.tobytes())

    def test_glow_does_not_wrap_round_the_edges(self):
        p = picture(64, 64, (0, 0, 0))
        p[0:3, 30:34] = 255  # a light at the top edge
        out = np.frombuffer(finishing.finish(p.tobytes(), 64, 64, with_(glow=True, glow_size=0.05)),
                            np.uint8).reshape(64, 64, 3)
        self.assertEqual(int(out[63, 32].sum()), 0)  # nothing shows at the bottom edge

    def test_flare_only_where_a_light_shows(self):
        p = picture(60, 80, (10, 10, 10))
        p[10:14, 10:14] = 255
        on = with_(flare=True)
        lit = finishing.finish(p.tobytes(), 80, 60, on, lights=[(12.0, 12.0)])
        self.assertNotEqual(lit, p.tobytes())
        out = np.frombuffer(lit, np.uint8).reshape(60, 80, 3)
        self.assertTrue((out >= p).all())
        # sprites on the far side of the centre, along the line from the light
        self.assertGreater(int(out[30:, 40:].astype(int).sum()), int(p[30:, 40:].astype(int).sum()))
        for lights in ([(40.0, 30.0)], [(-5.0, 12.0)], [(12.0, 500.0)], []):  # dark there, or outside the picture
            self.assertEqual(finishing.finish(p.tobytes(), 80, 60, on, lights=lights), p.tobytes())

    def test_light_is_found_at_the_bright_pixels_centre(self):
        shade = picture(400, 400, (0, 0, 0)).astype(float)
        shade[200:204, 100:104] = 1.0  # pixel centres 100.5 to 103.5: the centre at 102
        x, y, amount = finishing.light_in_picture(shade, (105.0, 199.0), 0.6)  # projected a few pixels off
        self.assertAlmostEqual(x, 102.0)
        self.assertAlmostEqual(y, 202.0)
        self.assertEqual(amount, 1.0)

    def test_jpeg_tables_follow_libjpeg_scaling(self):
        luma, chroma = finishing.jpeg_quantisers(50)
        self.assertEqual(luma[0, 0], 16)
        self.assertEqual(chroma[7, 7], 99)
        luma, _ = finishing.jpeg_quantisers(100)
        self.assertTrue((luma == 1).all())
        luma, _ = finishing.jpeg_quantisers(10)
        self.assertEqual(luma[0, 0], 80)  # 16 * 500 % = 80

    def test_jpeg_flat_picture_survives_and_detail_loses_more_at_low_quality(self):
        flat = picture(24, 40, (120, 60, 200))
        out = finishing.jpeg_round_trip(flat, 75)
        self.assertLessEqual(int(np.abs(out.astype(int) - flat).max()), 2)
        rng = np.random.default_rng(1)
        busy = rng.integers(0, 256, size=(32, 32, 3)).astype(np.uint8)
        error = [float(np.abs(finishing.jpeg_round_trip(busy, q).astype(int) - busy).mean()) for q in (95, 50, 10)]
        self.assertLess(error[0], error[1])
        self.assertLess(error[1], error[2])
        self.assertEqual(finishing.jpeg_round_trip(busy, 50).shape, busy.shape)

    def test_jpeg_odd_sizes(self):
        p = picture(13, 21, (50, 100, 150))
        self.assertEqual(finishing.jpeg_round_trip(p, 60).shape, (13, 21, 3))

    def test_palette_has_at_most_n_colours_and_is_repeatable(self):
        rng = np.random.default_rng(7)
        p = rng.integers(0, 256, size=(40, 50, 3)).astype(np.uint8)
        for n in (2, 16, 256):
            out = finishing.reduce_palette(p, n)
            self.assertLessEqual(len(np.unique(out.reshape(-1, 3), axis=0)), n)
            self.assertTrue((finishing.reduce_palette(p, n) == out).all())

    def test_palette_leaves_a_picture_of_few_colours(self):
        p = picture(16, 16, (10, 20, 30))
        p[4:8] = (200, 100, 0)
        self.assertTrue((finishing.reduce_palette(p, 2) == p).all())

    def test_palette_dithers_a_gradient(self):
        ramp = np.repeat(np.linspace(0, 255, 64).astype(np.uint8)[None, :, None], 16, axis=0).repeat(3, axis=2)
        out = finishing.reduce_palette(ramp, 4)
        self.assertLessEqual(len(np.unique(out.reshape(-1, 3), axis=0)), 4)
        # dithered: some column between two palette colours holds both of them
        self.assertTrue(any(len(np.unique(out[:, x, 0])) > 1 for x in range(64)))

    def test_bayer_matrix(self):
        b = finishing.bayer_matrix(8)
        self.assertEqual(sorted(b.reshape(-1).tolist()), list(range(64)))
        self.assertEqual(finishing.bayer_matrix(2).tolist(), [[0, 2], [3, 1]])


if __name__ == "__main__":
    unittest.main()
