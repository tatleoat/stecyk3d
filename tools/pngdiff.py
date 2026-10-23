# SPDX-License-Identifier: MIT
"""Counts the pixels that differ between an 8-bit RGB PNG and a raw level dump (debugging aid).
usage: python3 -I tools/pngdiff.py picture.png dump.rgb"""
import sys
import struct
import zlib


def decode(path):
    data = open(path, "rb").read()
    pos = 8
    idat = b""
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h = struct.unpack(">II", body[:8])
        if kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * 3
    rows = []
    prev = bytearray(stride)
    i = 0
    for _ in range(h):
        f = raw[i]
        line = bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - 3] if x >= 3 else 0
            b = prev[x]
            c = prev[x - 3] if x >= 3 else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 255
        rows.append(line)
        prev = line
    return w, h, b"".join(rows)


w, h, a = decode(sys.argv[1])
b = open(sys.argv[2], "rb").read()
d = [i // 3 for i in range(0, len(a), 3) if a[i:i + 3] != b[i:i + 3]]
print(len(d), "of", w * h, "differ")
for p in d[:20]:
    print((p % w, p // w), "expected", list(a[p * 3:p * 3 + 3]), "got", list(b[p * 3:p * 3 + 3]))
