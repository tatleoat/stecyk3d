# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport: the ladder; Live order)
"""The coarse-first ladder, the live order flags and the resampled stand-in, with the fake program."""

import os
import sys
import time
import unittest

from stecyk3d import stream, viewport_session as vs
from test_viewport_session import BASE, command, expected, view, wait


class Ladder(unittest.TestCase):
    def test_first_divisor(self):
        self.assertEqual(vs.first_divisor(160, 120, 1), 1)  # 19200 pixels: no ladder
        self.assertEqual(vs.first_divisor(400, 300, 1), 2)  # 200 by 150 = 30000
        self.assertEqual(vs.first_divisor(1574, 952, 1), 8)  # 197 by 119 = 23443
        self.assertEqual(vs.first_divisor(1574, 952, 4), 2)  # asked pixel size 4: 394 by 238 is already 3 rungs down
        self.assertEqual(vs.first_divisor(100000, 100000, 1), vs.MAX_DIVISOR)

    def test_live_order_name(self):
        self.assertEqual(stream.live_order_name("ROWS"), "rows")
        self.assertEqual(stream.live_order_name("SCATTERED"), "scattered")

    def test_live_flags(self):
        self.assertEqual(stream.live_flags("ROWS"), ["--stream", "--rows"])
        self.assertEqual(stream.live_flags("SCATTERED"), ["--stream", "--scattered"])

    @unittest.skipUnless(__import__("importlib.util").util.find_spec("numpy"), "numpy")
    def test_resample_nearest(self):
        grey = bytearray([10, 10, 10, 20, 20, 20, 30, 30, 30, 40, 40, 40])  # 2 by 2
        big = stream.resample(grey, (2, 2), (4, 4))
        self.assertEqual(bytes(big[:12]), bytes([10] * 6 + [20] * 6))
        self.assertEqual(bytes(stream.resample(grey, (2, 2), (2, 2))), bytes(grey))

    def test_a_big_view_goes_coarse_to_fine_and_ends_in_the_exact_picture(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_live_order("SCATTERED")  # the ladder belongs to Scattered; Rows renders the asked size at once
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=400, height=300))
        sizes = []

        def done():
            if not sizes or sizes[-1] != s.size:
                sizes.append(s.size)
            return s.final
        self.assertTrue(wait(s, done))
        self.assertEqual([z for z in sizes if z != (0, 0)], [(200, 150), (400, 300)])
        self.assertEqual(bytes(s.levels), expected(400, 300, 0))
        self.assertEqual(s.starts, 2)

    def test_rows_has_no_ladder_the_asked_size_at_once(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        self.assertEqual(s.live_order, "ROWS")
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=400, height=300))
        sizes = []

        def done():
            if not sizes or sizes[-1] != s.size:
                sizes.append(s.size)
            return s.final
        self.assertTrue(wait(s, done))
        self.assertEqual([z for z in sizes if z != (0, 0)], [(400, 300)])  # never a coarse picture
        self.assertEqual(s.starts, 1)
        self.assertEqual(bytes(s.levels), expected(400, 300, 0))

    def test_scattered_to_rows_mid_ladder_restarts_at_the_asked_size(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_live_order("SCATTERED")
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=400, height=300))
        self.assertTrue(wait(s, lambda: s.divisor == 2 and s._active is not None))  # the coarse rung is in flight
        s.set_live_order("ROWS")
        self.assertIsNone(s.divisor)
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual((s.size, s.divisor), ((400, 300), 1))
        self.assertEqual(s.starts, 2)  # the coarse request, then one Rows request: no second rung
        self.assertEqual(bytes(s.levels), expected(400, 300, 0))

    def test_a_change_during_a_coarse_rung_starts_the_ladder_again(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_live_order("SCATTERED")
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=400, height=300))
        self.assertTrue(wait(s, lambda: s.divisor == 2 and s._active is not None))
        s.set_view(view(x=3.0, width=400, height=300))
        self.assertIsNone(s.divisor)
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(bytes(s.levels), expected(400, 300, 3))

    def test_a_change_stops_the_render_at_once_not_when_the_next_request_comes(self):
        log = os.path.join(__import__("tempfile").mkdtemp(), "log")
        for key, value in (("FAKE_DELAY", "0.05"), ("FAKE_LOG", log)):
            os.environ[key] = value
            self.addCleanup(os.environ.pop, key, None)
        # a window wide enough for a loaded machine, whatever the constant is (0.04 s now)
        self.addCleanup(setattr, vs, "DEBOUNCE", vs.DEBOUNCE)
        vs.DEBOUNCE = 1.0
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=160, height=120))
        self.assertTrue(wait(s, lambda: s._active is not None and s.progress > 0))
        changed = time.monotonic()
        s.set_view(view(x=3.0, width=160, height=120))
        # the new request is not sent for DEBOUNCE seconds; the old render must stop long before
        deadline = changed + vs.DEBOUNCE * 0.7
        while time.monotonic() < deadline and not os.path.exists(log):
            time.sleep(0.005)
        self.assertEqual(s.starts, 1, "the debounce has not run out")
        self.assertTrue(os.path.exists(log), "the old render was stopped without waiting for the next request")
        self.assertLess(float(open(log).read().split()[2]) - changed, vs.DEBOUNCE)
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(bytes(s.levels), expected(160, 120, 3))

    def test_the_live_order_chooses_the_flags_and_a_change_restarts(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        flags = []
        original = stream.live_order_name
        vs.live_order_name = lambda order: flags.append(order) or original(order)
        try:
            s.set_scene(BASE, 1.0, "5.0")
            s.set_view(view())
            self.assertTrue(wait(s, lambda: s.final))
            first = bytes(s.levels)
            s.set_live_order("SCATTERED")
            self.assertFalse(s.final)
            self.assertTrue(wait(s, lambda: s.final))
            self.assertEqual(bytes(s.levels), first)  # the same finished picture
        finally:
            vs.live_order_name = original
        self.assertEqual(flags, ["ROWS", "SCATTERED"])


@unittest.skipUnless(__import__("importlib.util").util.find_spec("numpy"), "numpy")
class Texture(unittest.TestCase):
    def test_words_hold_the_pixels_and_a_change_rewrites_only_its_rows(self):
        from stecyk3d import viewport_texture as vt
        levels = bytearray(range(24))  # 4 by 2 pixels
        words = vt.Words()
        self.assertTrue(words.sync(levels, (4, 2), None))  # a first size is the whole picture
        self.assertEqual(words.word(0, 0), (0x40 << 24) | (0 << 16) | (1 << 8) | 2)
        self.assertEqual(words.word(3, 1), (0x40 << 24) | (21 << 16) | (22 << 8) | 23)
        levels[12:15] = bytes([200, 100, 50])  # pixel (0, 1)
        self.assertFalse(words.sync(levels, (4, 2), None))  # nothing said to be changed: nothing taken
        self.assertEqual(words.word(0, 1), (0x40 << 24) | (12 << 16) | (13 << 8) | 14)
        self.assertTrue(words.sync(levels, (4, 2), (1, 1)))
        self.assertEqual(words.word(0, 1), (0x40 << 24) | (200 << 16) | (100 << 8) | 50)
        self.assertEqual(words.floats.shape, (8,))
        # as a float the word is an ordinary number between 2 and 8
        self.assertTrue(all(2.0 <= f < 8.0 for f in words.floats.tolist()))

    def test_the_session_reports_the_rows_that_changed(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(width=8, height=6))
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(s.take_dirty(), (0, 5))
        self.assertIsNone(s.take_dirty())


if __name__ == "__main__":
    unittest.main()
