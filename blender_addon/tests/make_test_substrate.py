# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests);, the same shape as
# tests/case_command.rs::substrate_file. Task T1: the test substrate for the headless renders — it holds
# the five tables every render needs (perm, waves, aa-offsets, pattern, pyramid); its numbers are ours, and the CLI identity check compares commands against
# the SAME file, so any well-formed numbers serve. Usage: python3 make_test_substrate.py OUT.bin
"""Write a well-formed version-4 substrate file (kind, role, name per table) holding the five tables every render needs."""

import hashlib
import struct
import sys


def table(name, body):
    return (bytes([0, 0, len(name)]) + name.encode() + struct.pack("<H", 4) + b"test"
            + hashlib.sha256(body).digest() + struct.pack("<I", len(body)) + body)


def main():
    offsets = [0.125, 0.375, 0.625, 0.875, 0.25, 0.5, 0.75, 0.125, 0.875, 0.375, 0.625, 0.75, 0.25, 0.5]
    tables = table("aa-offsets", b"".join(struct.pack("<d", v) for v in offsets))
    tables += table("pyramid", b"".join(struct.pack("<d", v) for v in (0.8, 0.6)))
    # : every render needs perm, waves, aa-offsets, pattern and pyramid,
    # and `stecyk3d-render --document` checks them before any pixel. Arbitrary well-formed numbers (shapes).
    tables += table("perm", b"".join(struct.pack("<i", v) for v in range(255, -1, -1)))
    tables += table("waves", b"".join(struct.pack("<d", (k * 7 % 18) / 18.0 + 0.05) for k in range(18)))
    tables += table("pattern", b"".join(struct.pack("<d", (k * 5 % 12) / 12.0 + 0.04) for k in range(12)))
    # As 5.0 also noise-bits-five, dither, leaf-points, leaf-uvs, leaf-triangles and haze-five , the
    # shapes tests/case_command.rs gives them.
    x, noise = 0x2545F491, b""
    for _ in range(64):
        x ^= (x << 13) & 0xFFFFFFFF
        x ^= x >> 17
        x ^= (x << 5) & 0xFFFFFFFF
        noise += struct.pack("<I", x)
    tables += table("noise-bits-five", noise)
    tables += table("dither", bytes(range(256)))
    tables += table("leaf-points", b"".join(struct.pack("<d", ((i * 7) % 11) / 11.0 - 0.4) for i in range(60)))
    tables += table("leaf-uvs", b"".join(struct.pack("<f", i / 40.0) for i in range(40)))
    tables += table("leaf-triangles", b"".join(struct.pack("<I", (i * 3 + i // 3) % 20) for i in range(60)))
    tables += table("haze-five", b"".join(struct.pack("<d", v) for v in (0.5, 0.25, 0.125)))
    f = b"BRYSUBST" + struct.pack("<II", 4, 11) + tables
    f += hashlib.sha256(f).digest()
    with open(sys.argv[1], "wb") as out:
        out.write(f)


main()
