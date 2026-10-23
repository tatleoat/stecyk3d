# SPDX-License-Identifier: GPL-3.0-or-later
# (Numbers)
"""How the numbers a .blend file holds become the description's numbers. Pure Python."""

import struct

WORD_MODULUS = 1 << 32


def single(x):
    """The nearest binary32 to x, as the Python float (binary64) that holds it exactly."""
    return struct.unpack("<f", struct.pack("<f", float(x)))[0]


def singles(values):
    """single() of each value, as a list."""
    return [single(v) for v in values]


def word(i):
    """A word setting (Blender keeps a signed 32-bit number) as 32 bits without a sign."""
    return int(i) % WORD_MODULUS


def shortest_single(x):
    """The shortest decimal that reads back as single(x), taken as the nearest binary64 (trees)."""
    s = single(x)
    if s != s or s in (float("inf"), float("-inf")):
        return s
    for digits in range(1, 10):  # 9 significant digits always round-trip a binary32
        text = "%.*g" % (digits, s)
        value = float(text)
        if single(value) == s:
            return value
    return s


def single_product(a, b):
    """The binary32 product of two singles, rounded to nearest (exact in binary64 first, then rounded once)."""
    return single(single(a) * single(b))
