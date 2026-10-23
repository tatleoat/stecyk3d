# SPDX-License-Identifier: GPL-3.0-or-later
# , exact_png (levels k as k/255)
# (The live picture; The Rendered viewport, the picture as it arrives)
"""The renderer's stream of finished pixels, decoded. Pure Python; no bpy.

A record is `VSTA`, x, y, n as binary32-le words (the renderer's own interface), then
n pixels of RGB: a run of one row's pixels from (x, y). Records may arrive split anywhere.
"""

import struct
from array import array

RECORD_MAGIC = b"VSTA"
RECORD_HEAD = 16


LIVE_ORDERS = ("ROWS", "SCATTERED")  # the 'Live order' setting; the first is the default


WASH_GRAINS = ("AUTO", "EXACT", "FAST")  # the 'Wash grain' setting; the first is the default


def grain_flags(grain):
    """The renderer's arguments for a Wash grain setting: EXACT keeps the program's serial order of draws
    (`--exact-random`), FAST gives every pixel a place of its own in the generator's sequence on every core
    (`--independent-random`), AUTO (nothing) lets the renderer decide from the size of the work, the same on every
    machine. (Wash grain)"""
    return {"EXACT": ["--exact-random"], "FAST": ["--independent-random"]}.get(grain, [])


def grain_name(grain):
    """The long-lived renderer's name for a Wash grain setting (`draws` of a request): exact, independent or auto."""
    return {"EXACT": "exact", "FAST": "independent"}.get(grain, "auto")


def live_order_name(order):
    """The long-lived renderer's name for a Live order setting (`order` of a request)."""
    return "scattered" if order == "SCATTERED" else "rows"


def live_flags(order):
    """The renderer's arguments for a live picture in `order`: ROWS takes the pixels row by row from the top
    (`--rows`), SCATTERED in a fixed random shuffle (`--scattered`). The wire format and the finished picture are
    the same either way. (the live picture, Live order)"""
    return ["--stream", "--scattered" if order == "SCATTERED" else "--rows"]


def resample(levels, old_size, new_size):
    """`levels` (RGB, top row first) of `old_size` (w, h) as a picture of `new_size`, each new pixel the old pixel
    under its centre (nearest), so a picture of one size stands in while another size is rendered. White where
    numpy is missing."""
    (ow, oh), (nw, nh) = old_size, new_size
    if ow == nw and oh == nh:
        return bytearray(levels)
    try:
        import numpy as np
    except ImportError:
        return bytearray(b"\xff" * (nw * nh * 3))
    grid = np.frombuffer(bytes(levels), dtype=np.uint8).reshape(oh, ow, 3)
    ys = np.minimum(((np.arange(nh) + 0.5) * oh / nh).astype(np.int64), oh - 1)
    xs = np.minimum(((np.arange(nw) + 0.5) * ow / nw).astype(np.int64), ow - 1)
    return bytearray(grid[ys][:, xs].tobytes())


class StreamDecoder:
    """Writes the pixels of every whole record into `levels` (RGB, top row first, `width * height * 3` bytes).

    `levels` may be given (a buffer of the right size, kept in place) so that a new picture starts from an old one.
    `got` counts the pixels taken; `dirty` says something was written since it was last cleared."""

    def __init__(self, width, height, levels=None):
        self.width = width
        self.height = height
        if levels is None:
            levels = bytearray(b"\xff" * (width * height * 3))  # white, as the renderer's own background
        elif len(levels) != width * height * 3:
            raise ValueError("a levels buffer of %d bytes for a %d by %d picture" % (len(levels), width, height))
        self.levels = levels
        self.pending = bytearray()
        self.got = 0
        self.dirty = False
        self.rows = None  # the first and last row written since it was last cleared (the texture's changed rows)

    def on_run(self, x, y, count, buf, source):
        """Called for each whole in-picture run; a subclass keeps its own copy of the pixels here."""

    def feed(self, chunk):
        """Add bytes read from a pipe and write every whole record in."""
        self.pending += chunk
        buf = self.pending
        pos = 0
        end = len(buf)
        width = self.width
        while end - pos >= RECORD_HEAD:
            if buf[pos:pos + 4] != RECORD_MAGIC:
                pos = buf.find(RECORD_MAGIC, pos + 1)
                if pos < 0:
                    pos = end
                    break
                continue
            x, y, count = struct.unpack_from("<III", buf, pos + 4)
            stop = pos + RECORD_HEAD + count * 3
            if stop > end:
                break  # the record is not whole yet; wait for the rest
            self.put(x, y, count, buf, pos + RECORD_HEAD)
            pos = stop
        del buf[:pos]

    def put(self, x, y, count, buf, source):
        """Write one record's pixels (`count` RGB triples at `buf[source:]`) from (x, y); a run outside the picture
        is dropped."""
        if y < self.height and x + count <= self.width:  # a whole in-picture run
            start = (y * self.width + x) * 3
            self.levels[start:start + count * 3] = buf[source:source + count * 3]
            self.on_run(x, y, count, buf, source)
            self.got += count
            self.dirty = True
            rows = self.rows
            self.rows = (y, y) if rows is None else (min(rows[0], y), max(rows[1], y))

    def fraction(self):
        # With anti-aliasing on the preview and the frame each send a picture, so the same count can mean
        # different shares; the bar stops below 1 until the frame is final.
        return min(self.got / float(self.width * self.height), 0.95)


class ServeDecoder:
    """The framed output of `stecyk3d-render --serve` (The long-lived renderer):
    `VRDY` u32, then for each request `VBEG` u32 id, stream records, `VEND` u32 id, u32 status, u32 n, n bytes of text.
    The records of the request named by `begin` are written into its `StreamDecoder`; those of any other (one a newer
    request passed) are dropped. `feed` returns the `(id, status, text)` of every request that ended."""

    def __init__(self):
        self.pending = bytearray()
        self.ready = False
        self.current = None  # the id whose records are arriving
        self.active = None  # the id wanted, and where its pixels go
        self.target = None

    def begin(self, request_id, target):
        self.active = request_id
        self.target = target

    def drop(self):
        self.active = None
        self.target = None

    def feed(self, chunk):
        self.pending += chunk
        buf = self.pending
        pos = 0
        end = len(buf)
        ends = []
        while end - pos >= 8:
            magic = bytes(buf[pos:pos + 4])
            if magic == b"VRDY":
                self.ready = True
                pos += 8
            elif magic == b"VBEG":
                self.current = struct.unpack_from("<I", buf, pos + 4)[0]
                pos += 8
            elif magic == RECORD_MAGIC:
                if end - pos < RECORD_HEAD:
                    break
                x, y, count = struct.unpack_from("<III", buf, pos + 4)
                stop = pos + RECORD_HEAD + count * 3
                if stop > end:
                    break
                if self.target is not None and self.current == self.active:
                    self.target.put(x, y, count, buf, pos + RECORD_HEAD)
                pos = stop
            elif magic == b"VEND":
                if end - pos < 16:
                    break
                request_id, status, size = struct.unpack_from("<III", buf, pos + 4)
                stop = pos + 16 + size
                if stop > end:
                    break
                ends.append((request_id, status, bytes(buf[pos + 16:stop]).decode("utf-8", "replace")))
                pos = stop
            else:
                pos += 1  # not a frame: look for the next one
        del buf[:pos]
        return ends


class LivePicture(StreamDecoder):
    """F12's live picture: the decoder with a float rectangle for Blender's result (rows from the bottom up,
    level k as k/255, alpha 1: note). (the live picture)"""

    def __init__(self, width, height):
        super().__init__(width, height)
        self.pixels = [1.0, 1.0, 1.0, 1.0] * (width * height)

    def on_run(self, x, y, count, buf, source):
        # One extended-slice assignment a channel through a table of the 256 values (k / 255.0, the same doubles
        # as dividing), not a Python loop a pixel: at 1920 by 1440 the loop cost over a second of F12.
        base = ((self.height - 1 - y) * self.width + x) * 4
        pixels = self.pixels
        table = LEVEL_FLOATS
        span = count * 3
        for c in range(3):
            pixels[base + c:base + count * 4:4] = map(table.__getitem__, buf[source + c:source + span:3])


LEVEL_FLOATS = [k / 255.0 for k in range(256)]


def rgba_floats(levels, width, height):
    """The levels (RGB, top row first) as RGBA floats, k/255 and alpha 1, the rows from the bottom up, in an
    `array('f')` of `width * height * 4`: what a GPU texture of RGBA16F takes. Uses numpy when there is one."""
    n = width * height
    if len(levels) != n * 3:
        raise ValueError("%d bytes of levels for a %d by %d picture" % (len(levels), width, height))
    try:
        import numpy as np
    except ImportError:
        np = None
    if np is not None:
        grid = np.frombuffer(bytes(levels), dtype=np.uint8).reshape(height, width, 3)[::-1]
        out = np.ones((height, width, 4), dtype=np.float32)
        table = np.array([k / 255.0 for k in range(256)], dtype=np.float32)  # the same singles as below
        out[:, :, :3] = table[grid]
        result = array("f")
        result.frombytes(out.tobytes())
        return result
    table = array("f", [k / 255.0 for k in range(256)])
    out = array("f", [1.0]) * (n * 4)
    row_bytes = width * 3
    for row in range(height):
        source = (height - 1 - row) * row_bytes
        line = levels[source:source + row_bytes]
        base = row * width * 4
        span = width * 4
        for c in range(3):
            out[base + c:base + span:4] = array("f", map(table.__getitem__, line[c::3]))
    return out
