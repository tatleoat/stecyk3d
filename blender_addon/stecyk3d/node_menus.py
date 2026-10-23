# SPDX-License-Identifier: GPL-3.0-or-later
# , (the tables of names: noise-names, noise-names-five, method-names,
# blend-names, filter-names, color-mode-names),
# (Node menus; tasks T2 and T3 of NODE-PARITY-PLAN.md)
"""Named menus for the Bryce nodes' number settings. The stored value stays the same integer word : a menu
is a second property that shows the name and sets the number. Names come from the user's substrate  and
from names.py; a number with no name shows as "Noise 27". The reading half is pure Python (no bpy)."""

import hashlib
import os
import struct

from . import names

# kind -> (table names the labels are read from, fallback prefix)
NOISE, METHOD, BLEND, FILTER, COLOUR_MODE = "noise", "method", "blend", "filter", "colour mode"
FALLBACK = {NOISE: "Noise", METHOD: "Method", BLEND: "Blend", FILTER: "Filter", COLOUR_MODE: "Colour mode"}
TABLE_NAMES = ("noise-names", "noise-names-five", "method-names", "blend-names", "filter-names", "color-mode-names")
FIVE_FIRST = 27

_cache = {}  # path -> (signature, {table: [labels]})


def read_tables(path):
    """The tables of names of a substrate file (layout): {name: [labels]}; {} when the file cannot be
    read (a menu then shows numbers). The file hash is checked: a damaged file gives no names."""
    try:
        with open(path, "rb") as f:
            data = f.read()
        if data[:8] != b"BRYSUBST" or struct.unpack_from("<I", data, 8)[0] != 4:
            return {}
        if hashlib.sha256(data[:-32]).digest() != data[-32:]:
            return {}
        count, at, out = struct.unpack_from("<I", data, 12)[0], 16, {}
        for _ in range(count):
            kind, _role, n = data[at], data[at + 1], data[at + 2]
            at += 3
            name = data[at:at + n].decode("ascii", "replace")
            at += n
            (m,) = struct.unpack_from("<H", data, at)
            at += 2 + m + 32
            (length,) = struct.unpack_from("<I", data, at)
            at += 4
            if kind == 0:
                if name in TABLE_NAMES:
                    out[name] = [x.decode("utf-8", "replace") for x in data[at:at + length].split(b"\0")[:-1]]
                at += length
        return out
    except (OSError, struct.error, IndexError):
        return {}


def tables_of(path):
    """read_tables, remembered until the file's size or time changes."""
    try:
        st = os.stat(path)
    except OSError:
        return EMPTY
    signature = (st.st_size, st.st_mtime_ns)
    hit = _cache.get(path)
    if hit is None or hit[0] != signature:
        hit = _cache[path] = (signature, read_tables(path))
    return hit[1]


def forget():
    _cache.clear()


EMPTY = {}


def current_tables():
    """The tables of the substrate the add-on's preferences name (needs Blender); {} when there is none."""
    try:
        import bpy
        from . import engine
        path, _problem = engine.find_substrate(engine.preferences(bpy.context))
    except Exception:  # no preferences yet (a file loading, a test): numbers then
        return EMPTY
    return tables_of(path) if path else EMPTY


def label_in(tables, kind, number):
    """The menu text of `number` for `kind` from `tables`, else "Noise 27" style."""
    n = int(number)
    pick = {NOISE: lambda: (tables.get("noise-names", []) if n < FIVE_FIRST else tables.get("noise-names-five", []),
                            n if n < FIVE_FIRST else n - FIVE_FIRST),
            METHOD: lambda: (tables.get("method-names", []), n),
            BLEND: lambda: (tables.get("blend-names", []), n),
            FILTER: lambda: (tables.get("filter-names", []), n),
            COLOUR_MODE: lambda: (tables.get("color-mode-names", []), n)}[kind]
    labels, index = pick()
    if 0 <= index < len(labels) and labels[index]:
        return labels[index]
    return "%s %d" % (FALLBACK[kind], n)


SPEC_COUNTS = {NOISE: 27 + 23, METHOD: 13, BLEND: 19, FILTER: 14, COLOUR_MODE: 17}
_TABLE_OF = {METHOD: "method-names", BLEND: "blend-names", FILTER: "filter-names", COLOUR_MODE: "color-mode-names"}


def count_in(tables, kind):
    """How many numbers a kind offers: the label counts , or the table's length if it is longer."""
    if kind == NOISE:
        have = len(tables.get("noise-names", [])) + len(tables.get("noise-names-five", []))
    else:
        have = len(tables.get(_TABLE_OF[kind], []))
    return max(have, SPEC_COUNTS[kind])


def mapping_label(number):
    return names.MAPPING_MODES.get(int(number)) or "Mode %d" % int(number)


def items_for(tables, kind, current):
    """[(identifier, label, description, number)] of every named number of `kind`, and `current` where it has no
    name (so a value from a file is never lost from the menu). Identifiers are the numbers as text."""
    if kind == "mapping":
        numbers = list(range(17))
        text = mapping_label
    elif kind == "space":
        numbers = sorted(names.COLOUR_SPACES)
        text = lambda n: names.COLOUR_SPACES.get(n) or "Space %d" % n  # noqa: E731
    else:
        numbers = list(range(count_in(tables, kind) or 0))
        text = lambda n: label_in(tables, kind, n)  # noqa: E731
    if int(current) not in numbers:
        numbers.append(int(current))
    return [(str(n), text(n), "", n) for n in numbers]


# ---------------------------------------------------------------- the Blender side (properties over a stored word)

_items_cache = {}  # (kind, number, tables identity) -> items; Blender needs the strings kept alive


def _signed(word):
    word = int(word) & 0xFFFFFFFF
    return word - (1 << 32) if word >= (1 << 31) else word


def int_menu(store, kind, name, description=""):
    """An EnumProperty over the IntProperty `store`: it lists the names of `kind` and writes the same integer."""
    import bpy

    def items(self, _context):
        number = int(getattr(self, store))
        tables = current_tables()
        key = (kind, number, id(tables))
        if key not in _items_cache:
            if len(_items_cache) > 512:
                _items_cache.clear()
            _items_cache[key] = items_for(tables, kind, number)
        return _items_cache[key]

    return bpy.props.EnumProperty(name=name, description=description, items=items,
                                  get=lambda self: int(getattr(self, store)),
                                  set=lambda self, value: setattr(self, store, int(value)))


def flag_menu(store, bits, name, description=""):
    """An ENUM_FLAG property over the word `store`: each of `bits` ((bit, label), ...) is one toggle; it changes
    only those bits, every other bit of the word is kept."""
    import bpy
    mask = 0
    for bit, _label in bits:
        mask |= bit
    items = [("BIT_%X" % bit, label, "", bit) for bit, label in bits]
    return bpy.props.EnumProperty(
        name=name, description=description, items=items, options={"ENUM_FLAG"},
        get=lambda self: int(getattr(self, store)) & mask,
        set=lambda self, value: setattr(self, store, _signed((int(getattr(self, store)) & ~mask & 0xFFFFFFFF)
                                                              | (int(value) & mask))))


def bit_property(store, bit, name, description="", inverted=False):
    """A BoolProperty over one bit of the word `store`: writes only that bit (`inverted`: on while the bit is clear)."""
    import bpy

    def get(self):
        return bool(int(getattr(self, store)) & bit) != inverted

    def set_(self, value):
        word = int(getattr(self, store)) & 0xFFFFFFFF
        on = bool(value) != inverted
        setattr(self, store, _signed((word | bit) if on else (word & ~bit)))

    return bpy.props.BoolProperty(name=name, description=description, get=get, set=set_)
