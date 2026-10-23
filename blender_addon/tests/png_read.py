# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests): a minimal 8-bit PNG reader (stdlib only) for the
# end-to-end colour check: every byte of Blender's saved picture against the renderer's own levels.
"""Read an 8-bit non-interlaced PNG. Returns (width, height, channels, bytes) channels in 3 or 4."""

import struct
import zlib


def read_png(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("%s is not a PNG" % path)
    pos, idat, header = 8, b"", None
    while pos < len(data):
        (size,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + size]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
        pos += 12 + size
    width, height, depth, colour, compression, filt, interlace = header
    if depth != 8 or interlace != 0 or colour not in (0, 2, 3, 4, 6):
        raise ValueError("%s: only 8-bit non-interlaced truecolour is read (colour %d, depth %d)"
                         % (path, colour, depth))
    channels = {0: 1, 3: 1, 4: 2, 2: 3, 6: 4}[colour]
    raw = zlib.decompress(idat)
    stride = width * channels
    out, previous = bytearray(), bytearray(stride)
    row = 0
    for y in range(height):
        ft = raw[row]
        line = bytearray(raw[row + 1:row + 1 + stride])
        row += 1 + stride
        if ft == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif ft == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif ft == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif ft == 4:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = previous[i]
                c = previous[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        out += line
        previous = line
    return width, height, channels, bytes(out)
