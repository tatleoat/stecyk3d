# SPDX-License-Identifier: GPL-3.0-or-later
# exact_png (the 8-bit picture written with its levels as the renderer gives them), Outputs
# (The render engine)
"""A minimal 8-bit RGB PNG writer (standard library only). Pure Python."""

import struct
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"  # the PNG specification (ISO/IEC 15948)


def _chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def png_bytes(width, height, levels):
    """levels: width*height*3 bytes, top row first, red, green, blue. No gamma or colour chunks."""
    if len(levels) != width * height * 3:
        raise ValueError("levels hold %d bytes, not %d" % (len(levels), width * height * 3))
    stride = width * 3
    raw = b"".join(b"\x00" + bytes(levels[y * stride:(y + 1) * stride]) for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return PNG_SIGNATURE + _chunk(b"IHDR", header) + _chunk(b"IDAT", zlib.compress(raw, 9)) + _chunk(b"IEND", b"")


def write_png(path, width, height, levels):
    with open(path, "wb") as f:
        f.write(png_bytes(width, height, levels))
