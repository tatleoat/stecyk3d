# SPDX-License-Identifier: GPL-3.0-or-later
# (The live picture; the picture as it arrives)
"""The stream decoder and the texture data, from plain bytes."""

import struct
import unittest

from stecyk3d import stream


def record(x, y, pixels):
    return b"VSTA" + struct.pack("<III", x, y, len(pixels)) + b"".join(bytes(p) for p in pixels)


def single(v):
    return struct.unpack("f", struct.pack("f", v))[0]


class WashGrain(unittest.TestCase):
    def test_flags_and_request_names(self):
        self.assertEqual(stream.grain_flags("AUTO"), [])
        self.assertEqual(stream.grain_flags("EXACT"), ["--exact-random"])
        self.assertEqual(stream.grain_flags("FAST"), ["--independent-random"])
        self.assertEqual([stream.grain_name(g) for g in stream.WASH_GRAINS], ["auto", "exact", "independent"])


class Decoder(unittest.TestCase):
    def test_whole_record(self):
        d = stream.StreamDecoder(4, 2)
        d.feed(record(1, 1, [(1, 2, 3), (4, 5, 6)]))
        self.assertEqual(bytes(d.levels[(4 + 1) * 3:(4 + 3) * 3]), bytes([1, 2, 3, 4, 5, 6]))
        self.assertEqual(d.got, 2)
        self.assertTrue(d.dirty)
        self.assertEqual(bytes(d.levels[:3]), b"\xff\xff\xff")

    def test_split_anywhere(self):
        data = record(0, 0, [(9, 9, 9)]) + record(2, 0, [(7, 8, 9), (1, 1, 1)])
        for cut in range(len(data) + 1):
            d = stream.StreamDecoder(4, 1)
            d.feed(data[:cut])
            d.feed(data[cut:])
            self.assertEqual(bytes(d.levels), bytes([9, 9, 9, 255, 255, 255, 7, 8, 9, 1, 1, 1]), cut)

    def test_one_byte_at_a_time(self):
        data = record(0, 0, [(5, 6, 7)]) * 3
        d = stream.StreamDecoder(2, 1)
        for b in data:
            d.feed(bytes([b]))
        self.assertEqual(d.got, 3)

    def test_junk_between_records_is_skipped(self):
        d = stream.StreamDecoder(2, 1)
        d.feed(b"xx" + record(0, 0, [(1, 1, 1)]) + b"garbage-bytes!!" + record(1, 0, [(2, 2, 2)]))
        self.assertEqual(bytes(d.levels), bytes([1, 1, 1, 2, 2, 2]))

    def test_out_of_picture_runs_are_dropped(self):
        d = stream.StreamDecoder(2, 2)
        d.feed(record(1, 0, [(1, 1, 1), (2, 2, 2)]) + record(0, 2, [(3, 3, 3)]))
        self.assertEqual(d.got, 0)
        self.assertFalse(d.dirty)
        self.assertEqual(bytes(d.levels), b"\xff" * 12)

    def test_given_buffer_is_written_in_place(self):
        buf = bytearray(range(6))
        d = stream.StreamDecoder(2, 1, buf)
        d.feed(record(1, 0, [(0, 0, 0)]))
        self.assertIs(d.levels, buf)
        self.assertEqual(bytes(buf), bytes([0, 1, 2, 0, 0, 0]))
        with self.assertRaises(ValueError):
            stream.StreamDecoder(3, 1, buf)

    def test_fraction_stops_below_one(self):
        d = stream.StreamDecoder(2, 1)
        d.feed(record(0, 0, [(0, 0, 0), (0, 0, 0)]))
        self.assertEqual(d.fraction(), 0.95)


class Live(unittest.TestCase):
    def test_rectangle_is_bottom_row_first_k_over_255(self):
        p = stream.LivePicture(2, 2)
        p.feed(record(1, 0, [(255, 51, 0)]))  # the top row's second pixel: the rectangle's last row
        self.assertEqual(p.pixels[(2 + 1) * 4:(2 + 1) * 4 + 4], [1.0, 51 / 255.0, 0.0, 1.0])
        self.assertEqual(p.pixels[0:4], [1.0, 1.0, 1.0, 1.0])  # untouched bottom row: white

    def test_a_run_of_pixels_matches_the_pixel_by_pixel_values(self):
        w, h = 5, 3
        p = stream.LivePicture(w, h)
        run = [(k * 50 % 256, (k * 7 + 3) % 256, 255 - k) for k in range(4)]
        p.feed(record(1, 1, run))
        for k, rgb in enumerate(run):
            at = ((h - 1 - 1) * w + 1 + k) * 4
            self.assertEqual(p.pixels[at:at + 4], [rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0, 1.0])
        self.assertEqual(p.pixels[((h - 1 - 1) * w) * 4:((h - 1 - 1) * w) * 4 + 4], [1.0] * 4)  # left of the run
        self.assertEqual(len(p.pixels), w * h * 4)


class Rgba(unittest.TestCase):
    def test_every_level_exact_and_rows_from_the_bottom(self):
        w, h = 16, 16
        levels = bytearray()
        for k in range(256):
            levels += bytes([k, 255 - k, (k * 7) % 256])
        out = stream.rgba_floats(levels, w, h)
        self.assertEqual(len(out), w * h * 4)
        for y in range(h):
            for x in range(w):
                k = y * w + x
                j = ((h - 1 - y) * w + x) * 4
                self.assertEqual(out[j], single(k / 255.0))
                self.assertEqual(out[j + 1], single((255 - k) / 255.0))
                self.assertEqual(out[j + 2], single(((k * 7) % 256) / 255.0))
                self.assertEqual(out[j + 3], 1.0)

    def test_size_mismatch(self):
        with self.assertRaises(ValueError):
            stream.rgba_floats(b"\0" * 5, 1, 1)


if __name__ == "__main__":
    unittest.main()
