# SPDX-License-Identifier: GPL-3.0-or-later
# (Solids: a terrain picture whose side is not a grid size). Pure: the AUTO grid's size and the resampler.
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import solids  # noqa: E402


def picture(n, f):
    """An n by n red channel of f(u, v), u and v from 0 to 1 over the first to the last point."""
    return [f(i / (n - 1), j / (n - 1)) for j in range(n) for i in range(n)]


def bowl(u, v):
    return 0.2 + 0.6 * ((u - 0.5) ** 2 + (v - 0.5) ** 2)


class AutoSize(unittest.TestCase):
    def test_sizes(self):
        self.assertEqual([solids.auto_size(s) for s in (2, 15, 16, 17, 192, 256, 257, 1000, 1024, 1025, 4096)],
                         [16, 16, 16, 32, 256, 256, 512, 1024, 1024, 1024, 1024])

    def test_one_point_refused(self):
        with self.assertRaisesRegex(ValueError, "1 point a side, fewer than 2"):
            solids.auto_size(1)


class Resample(unittest.TestCase):
    def test_grid_sizes_unchanged(self):
        heights = list(range(16 * 16))
        self.assertEqual(solids.resample(heights, 16, 16), heights)

    def test_corners_and_a_ramp(self):
        """The first and last points stay; a ramp along the rows stays a ramp (to the truncation)."""
        old = 192
        heights = [round(i / (old - 1) * 65535) for j in range(old) for i in range(old)]
        out = solids.resample(heights, old, 256)
        self.assertEqual(len(out), 256 * 256)
        self.assertEqual((out[0], out[255], out[-256], out[-1]), (0, 65535, 0, 65535))
        for i in range(256):
            self.assertLessEqual(abs(out[i] - i / 255 * 65535), 1.0)
            self.assertEqual(out[100 * 256 + i], out[i])

    def test_same_shape_as_the_native_size(self):
        """A 192-point picture of a function resampled to 256 is the 256-point picture of it, to the
        interpolation's error."""
        lo = [solids.height_of(v) for v in picture(192, bowl)]
        native = [solids.height_of(v) for v in picture(256, bowl)]
        out = solids.resample(lo, 192, 256)
        worst = max(abs(a - b) for a, b in zip(out, native))
        self.assertLess(worst, 0.001 * 65535)

    def test_larger_than_the_largest_grid(self):
        """A picture above 1024 a side comes down to 1024 (resize_grid itself refuses it, as the program does)."""
        lo = [solids.height_of(v) for v in picture(1100, lambda u, v: u)]
        out = solids.resample(lo, 1100, 1024)
        self.assertEqual((len(out), out[0], out[1023]), (1024 * 1024, 0, 65535))
        with self.assertRaises(ValueError):
            solids.resize_grid(lo, 1100, 1024)

    def test_resize_grid_unchanged(self):
        """The grid setting's own resize is resize_grid's arithmetic, now shared with resample."""
        heights = [(i * 7919) % 65536 for i in range(37 * 37)]
        self.assertEqual(solids.resize_grid(heights, 37, 64), solids.resample(heights, 37, 64))
        with self.assertRaises(ValueError):
            solids.resample(heights, 37, 100)  # not a grid size


class TerrainPicture(unittest.TestCase):
    def test_auto_resamples_with_a_notice(self):
        notices = []
        t = solids.terrain({"grid": "AUTO"}, 192, 192, picture(192, bowl), notices)
        self.assertEqual((t["size"], len(t["heights"])), (256, 256 * 256))
        self.assertEqual(notices, ["the terrain's picture is 192 points a side, not a grid size: resampled to 256 "
                                   "(bilinear, same extent)"])

    def test_grid_size_no_notice(self):
        notices = []
        t = solids.terrain({"grid": "AUTO"}, 64, 64, picture(64, bowl), notices)
        self.assertEqual((t["size"], notices), (64, []))
        self.assertEqual(t["heights"], [solids.height_of(v) for v in picture(64, bowl)])

    def test_set_grid_is_the_programs_resize(self):
        notices = []
        t = solids.terrain({"grid": "128"}, 192, 192, picture(192, bowl), notices)
        self.assertEqual((t["size"], notices), (128, []))

    def test_still_refused(self):
        with self.assertRaisesRegex(ValueError, "not square"):
            solids.terrain({"grid": "AUTO"}, 192, 128, [0.0] * (192 * 128))
        with self.assertRaisesRegex(ValueError, "fewer than 2"):
            solids.terrain({"grid": "AUTO"}, 1, 1, [0.5])


if __name__ == "__main__":
    unittest.main()
