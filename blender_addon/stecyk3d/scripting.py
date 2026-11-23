# SPDX-License-Identifier: GPL-3.0-or-later
# (The scripting API, api.py: its plain parts)
"""The scripting API's plain parts: a terrain's heights from a height function and the exact height of its surface
under a point, channel values put over a preset's, the choice of library and preset by name, a script's arguments,
variants and their parallel runs, the contact sheet's layout and its letters. Pure Python; api.py is the Blender
side."""

import math
import os
import re
import subprocess
import time

from . import create, names
from .numbers import single, singles
from .solids import AUTO_SIZES, HEIGHT_TOP


# ---------------------------------------------------------------- terrains

class TerrainGrid:
    """A terrain's heights made from a function of world x and y (Blender units), and the surface the renderer
    draws over them, so that things can be set on it by numbers.

    The grid has n by n points spread over `size` (x, y) about `centre` (x, y); `z_range` (low, high) is the
    height that 0 and 65,535 stand for. A height outside the range is held to it (`clamped` counts them).
    `location` and `scale` are the terrain object's, as Blender keeps them (binary32)."""

    def __init__(self, n, centre, size, z_range, height):
        if n not in AUTO_SIZES:
            raise ValueError("a terrain of %r points a side is not one of %s"
                             % (n, ", ".join(str(s) for s in AUTO_SIZES)))
        lo, hi = float(z_range[0]), float(z_range[1])
        if not hi > lo:
            raise ValueError("a terrain's z_range %r is not (low, high) with high above low" % (tuple(z_range),))
        if not (size[0] > 0 and size[1] > 0):
            raise ValueError("a terrain's size %r is not two lengths above 0" % (tuple(size),))
        self.n = n
        self.sx, self.sz = create.terrain_scales(n, False)
        self.scale = tuple(singles((size[0] / (self.sx * n), size[1] / (self.sx * n),
                                    (hi - lo) / (HEIGHT_TOP * self.sz))))
        self.location = tuple(singles((centre[0], centre[1], lo + self.scale[2] * create.HALF_HEIGHT * self.sz)))
        self.heights = []
        self.clamped = 0
        for j in range(n):
            y = self.location[1] + (j - n / 2.0) * self.cell[1]
            for i in range(n):
                x = self.location[0] + (i - n / 2.0) * self.cell[0]
                z = height(x, y)
                if z is None or z != z:
                    raise ValueError("the height function gives %r at (%g, %g)" % (z, x, y))
                level = int(round((z - lo) / (hi - lo) * HEIGHT_TOP))
                if not 0 <= level <= HEIGHT_TOP:
                    self.clamped += 1
                    level = min(max(level, 0), HEIGHT_TOP)
                self.heights.append(level)
        self.object = None  # api.terrain puts the Blender object here

    @property
    def cell(self):
        """The world size (x, y) of one cell of the grid."""
        return (self.scale[0] * self.sx, self.scale[1] * self.sx)

    def _grid(self, x, y):
        """World (x, y) as grid coordinates: grid point (i, j) is at (i, j)."""
        cx, cy = self.cell
        return (x - self.location[0]) / cx + self.n / 2.0, (y - self.location[1]) / cy + self.n / 2.0

    def _world_z(self, level):
        return self.location[2] + self.scale[2] * (level - create.HALF_HEIGHT) * self.sz

    def _cell_of(self, f):
        return min(int(math.floor(f)), self.n - 2)

    def z_at(self, x, y):
        """The world height of the surface at (x, y), or None off the grid.

        The renderer draws each cell (c, r) as two triangles split along the diagonal from grid point (c + 1, r)
        to (c, r + 1) (src/geometry/relief.rs), so this is a plane in each triangle, not a blend of the four
        corners."""
        u, v = self._grid(x, y)
        last = self.n - 1
        if not (0.0 <= u <= last and 0.0 <= v <= last):
            return None
        c, r = self._cell_of(u), self._cell_of(v)
        n, h = self.n, self.heights
        a, b, e, g = h[r * n + c], h[r * n + c + 1], h[(r + 1) * n + c + 1], h[(r + 1) * n + c]
        fu, fv = u - c, v - r
        if c + r + 1 <= u + v:
            level = (g - e) * (1.0 - fu) + (e - b) * fv + b
        else:
            level = (g - a) * fv + a + (b - a) * fu
        return self._world_z(level)

    def lowest_under(self, x, y, radius):
        """The lowest height of the surface over the disc of `radius` about (x, y), the parts off the grid left
        out, so that nothing set there has an edge in the air. Exact: the surface is a plane in each triangle, so
        its lowest point on the disc is a grid point inside it, a crossing of the circle with a cell's edge or
        diagonal, or the circle's point downhill of a triangle's slope."""
        if radius < 0:
            raise ValueError("a radius of %r is below 0" % (radius,))
        cx, cy = self.cell
        n, last = self.n, self.n - 1
        points = [(x, y)]
        u0, v0 = self._grid(x, y)
        ru, rv = radius / cx, radius / cy
        i_lo, i_hi = max(0, int(math.floor(u0 - ru))), min(last, int(math.ceil(u0 + ru)))
        j_lo, j_hi = max(0, int(math.floor(v0 - rv))), min(last, int(math.ceil(v0 + rv)))
        if radius > 0 and i_lo <= i_hi and j_lo <= j_hi:
            gx = lambda i: self.location[0] + (i - n / 2.0) * cx
            gy = lambda j: self.location[1] + (j - n / 2.0) * cy
            r2 = radius * radius
            for j in range(j_lo, j_hi + 1):  # grid points inside the disc
                for i in range(i_lo, i_hi + 1):
                    if (gx(i) - x) ** 2 + (gy(j) - y) ** 2 <= r2:
                        points.append((gx(i), gy(j)))
            for i in range(i_lo, i_hi + 1):  # the circle across the lines x = const
                d = r2 - (gx(i) - x) ** 2
                if d >= 0:
                    points += [(gx(i), y - math.sqrt(d)), (gx(i), y + math.sqrt(d))]
            for j in range(j_lo, j_hi + 1):  # and y = const
                d = r2 - (gy(j) - y) ** 2
                if d >= 0:
                    points += [(x - math.sqrt(d), gy(j)), (x + math.sqrt(d), gy(j))]
            # the diagonals u + v = k: with P = centre + radius (cos t, sin t), a cos t + b sin t = k - u0 - v0
            a, b = radius / cx, radius / cy
            reach, phase = math.hypot(a, b), math.atan2(b, a)
            for k in range(i_lo + j_lo, i_hi + j_hi + 1):
                d = (k - u0 - v0) / reach
                if -1.0 <= d <= 1.0:
                    for t in (phase + math.acos(d), phase - math.acos(d)):
                        points.append((x + radius * math.cos(t), y + radius * math.sin(t)))
            h = self.heights
            for r in range(max(0, j_lo - 1), min(last - 1, j_hi) + 1):  # each triangle's downhill point
                for c in range(max(0, i_lo - 1), min(last - 1, i_hi) + 1):
                    a_, b_, e_, g_ = h[r * n + c], h[r * n + c + 1], h[(r + 1) * n + c + 1], h[(r + 1) * n + c]
                    for second, du, dv in ((False, b_ - a_, g_ - a_), (True, e_ - g_, e_ - b_)):
                        slope_x, slope_y = du / cx, dv / cy  # in world units (the z scale is above 0)
                        length = math.hypot(slope_x, slope_y)
                        if length == 0.0:
                            continue
                        px, py = x - radius * slope_x / length, y - radius * slope_y / length
                        pu, pv = self._grid(px, py)
                        fu, fv = pu - c, pv - r
                        inside = (fu <= 1 and fv <= 1 and fu + fv >= 1) if second else (
                            fu >= 0 and fv >= 0 and fu + fv <= 1)
                        if inside:
                            points.append((px, py))
        zs = [z for z in (self.z_at(px, py) for px, py in points) if z is not None]
        if not zs:
            raise ValueError("no part of the disc of radius %g about (%g, %g) is over the terrain"
                             % (radius, x, y))
        return min(zs)


TERRAIN_SETTINGS = create.new_terrain("terrain", (0.0, 0.0, 0.0))["terrain"]  # the Add menu's new terrain's


# ---------------------------------------------------------------- materials

def override_channels(value, channels, what):
    """A material value (components.py's form) with `channels` put over its channels' constants: Materials Lab
    numbers, colours as display values (as the Bryce Material node holds them). `what` names the material in
    refusals. A channel fed by a mix of two values is refused: its constant is not what the mix shows."""
    for name, v in (channels or {}).items():
        if name not in names.CHANNELS:
            raise ValueError("%s: %r is not a Bryce material channel (%s)" % (what, name, ", ".join(names.CHANNELS)))
        ch = value["channels"][name]
        if ch["link"] is not None and "mix" in ch["link"]:
            raise ValueError("%s: channel %s is a mix of two values; setting its constant would not show"
                             % (what, name))
        if name in names.COLOUR_CHANNELS:
            if isinstance(v, (int, float)) or len(v) != 3:
                raise ValueError("%s: channel %s is a colour and takes three numbers, not %r" % (what, name, v))
            ch["value"] = singles(v)
        else:
            if not isinstance(v, (int, float)):
                raise ValueError("%s: channel %s takes one number, not %r" % (what, name, v))
            ch["value"] = single(v)
    return value


def library_matches(offered, library):
    """The keys of the libraries offered ([(label, key)]) named `library`: the label itself, or the label without
    the folder the extractor's libraries carry ("Waters&Liquids (Corel Bryce 5.0)"); any letter case."""
    want = library.lower()
    return [key for label, key in offered
            if label.lower() == want or label.rsplit(" (", 1)[0].lower() == want]


def preset_index(offered, name, library):
    """The index of the preset `name` among a library's offered presets ([(index, name, description)]): the name
    itself, else the one preset whose name begins with it. None when the library has no such preset; an
    ambiguous beginning is refused, naming the presets it fits."""
    exact = [i for i, n, _ in offered if n == name]
    if exact:
        return int(exact[0])
    begins = [(i, n) for i, n, _ in offered if n.startswith(name)]
    if len(begins) > 1:
        raise ValueError("%s: %r begins %s; give the whole name" % (library, name, ", ".join(repr(n) for _, n in begins)))
    return int(begins[0][0]) if begins else None


# ---------------------------------------------------------------- a script's arguments

class Arguments:
    """The arguments after Blender's `--`: the first ones (before any flag) and flags such as `--moon 9` or
    `--quick`."""

    def __init__(self, argv):
        self.argv = list(argv)

    @property
    def positional(self):
        out = []
        for a in self.argv:
            if a.startswith("--"):
                break
            out.append(a)
        return out

    def has(self, flag):
        return flag in self.argv

    def get(self, flag, default=None):
        """The value after `flag`, of the default's type (a number, text or bool), or the default without it."""
        if flag not in self.argv:
            return default
        at = self.argv.index(flag) + 1
        if at >= len(self.argv):
            raise ValueError("%s needs a value" % flag)
        text = self.argv[at]
        if isinstance(default, bool):
            return text.lower() in ("1", "true", "yes", "on")
        if isinstance(default, int):
            return int(text)
        if isinstance(default, float):
            return float(text)
        return text


def script_arguments(argv):
    """: the arguments after the first `--` (Blender's own end there)."""
    argv = list(argv)
    return Arguments(argv[argv.index("--") + 1:] if "--" in argv else [])


# ---------------------------------------------------------------- variants

def safe_name(name):
    """A variant's name as a file name: letters, digits, `_`, `-` and `.`; anything else becomes `_`."""
    out = re.sub(r"[^A-Za-z0-9_.-]+", "_", str(name)).strip("._")
    return out or "variant"


def variant_items(variants):
    """[(file name, overrides)] of variants given as {name: overrides} or as a list of overrides (named v1, v2 ...).
    Two variants with the same file name are refused."""
    if isinstance(variants, dict):
        items = [(safe_name(k), v) for k, v in variants.items()]
    else:
        items = [("v%d" % (k + 1), v) for k, v in enumerate(variants)]
    seen = set()
    for name, _ in items:
        if name in seen:
            raise ValueError("two variants are both named %r (as a file name)" % name)
        seen.add(name)
    return items


def variant_flags(overrides):
    """A variant's overrides as command-line flags: a list of text as it stands, a dict as `--key value` (`_` in
    a key becomes `-`; True gives the flag alone, False and None leave it out)."""
    if isinstance(overrides, (list, tuple)):
        return [str(a) for a in overrides]
    flags = []
    for key, value in overrides.items():
        flag = "--" + str(key).replace("_", "-")
        if value is True:
            flags.append(flag)
        elif value is not False and value is not None:
            flags += [flag, str(value)]
    return flags


def variant_command(blender, script, out_dir, name, overrides, args=()):
    """The command that renders one variant: `blender -b --python-exit-code 1 --python SCRIPT -- OUT_DIR --name NAME
    [args] [flags]` (an error in the script ends Blender with 1, not 0). The script writes OUT_DIR/NAME.png
    (api.arguments() reads these)."""
    return ([blender, "-b", "--python-exit-code", "1", "--python", script, "--", out_dir, "--name", name]
            + [str(a) for a in args] + variant_flags(overrides))


def run_parallel(blender, script, variants, out_dir, jobs=None, args=(), log=print):
    """Each variant rendered by its own headless Blender, at most `jobs` at once (all of them by default); waits
    for all. Each run's output goes to OUT_DIR/NAME.log. Returns the pictures' paths in the variants' order;
    a run that fails or writes no picture is refused, naming it and its log, after all have ended."""
    from concurrent.futures import ThreadPoolExecutor
    out_dir = os.path.abspath(out_dir)
    os.makedirs(out_dir, exist_ok=True)
    items = variant_items(variants)
    script = os.path.abspath(script)

    def one(item):
        name, overrides = item
        command = variant_command(blender, script, out_dir, name, overrides, args)
        picture = os.path.join(out_dir, name + ".png")
        if os.path.exists(picture):
            os.remove(picture)  # an earlier run's picture is not this run's
        start = time.time()
        with open(os.path.join(out_dir, name + ".log"), "w") as out:
            code = subprocess.call(command, stdout=out, stderr=subprocess.STDOUT)
        log("VARIANT %s: exit %d after %.1f s" % (name, code, time.time() - start))
        return code

    with ThreadPoolExecutor(max_workers=jobs or len(items) or 1) as pool:
        codes = list(pool.map(one, items))
    paths, failed = [], []
    for (name, _), code in zip(items, codes):
        path = os.path.join(out_dir, name + ".png")
        if code != 0 or not os.path.isfile(path):
            failed.append("%s (exit %d%s; see %s)" % (name, code, "" if os.path.isfile(path) else ", no picture",
                                                      os.path.join(out_dir, name + ".log")))
        paths.append(path)
    if failed:
        raise RuntimeError("variants failed: " + "; ".join(failed))
    return paths


# ---------------------------------------------------------------- the contact sheet

def sheet_layout(sizes, columns, label_height, gap):
    """Where each picture of `sizes` [(width, height)] goes on a sheet of `columns` columns, every cell as big as
    the largest picture with a label strip below it: {"size": (w, h), "pictures": [(x, y)], "labels": [(x, y,
    width)]}, pixels from the top left."""
    if not sizes:
        raise ValueError("a contact sheet needs at least one picture")
    if columns < 1:
        raise ValueError("a contact sheet needs at least one column, not %r" % (columns,))
    cw, ch = max(w for w, _ in sizes), max(h for _, h in sizes)
    cols = min(columns, len(sizes))
    rows = -(-len(sizes) // cols)
    out = {"size": (cols * cw + (cols + 1) * gap, rows * (ch + label_height) + (rows + 1) * gap),
           "pictures": [], "labels": []}
    for k, (w, h) in enumerate(sizes):
        x = gap + (k % cols) * (cw + gap)
        y = gap + (k // cols) * (ch + label_height + gap)
        out["pictures"].append((x + (cw - w) // 2, y + (ch - h) // 2))
        out["labels"].append((x, y + ch, cw))
    return out


# A 5 by 7 pixel font of our own for the labels (capitals; small letters are drawn as capitals): each glyph is
# seven rows, the row's bit 16 its left pixel.
GLYPHS = {
    "A": (14, 17, 17, 31, 17, 17, 17), "B": (30, 17, 17, 30, 17, 17, 30), "C": (14, 17, 16, 16, 16, 17, 14),
    "D": (28, 18, 17, 17, 17, 18, 28), "E": (31, 16, 16, 30, 16, 16, 31), "F": (31, 16, 16, 30, 16, 16, 16),
    "G": (14, 17, 16, 23, 17, 17, 15), "H": (17, 17, 17, 31, 17, 17, 17), "I": (14, 4, 4, 4, 4, 4, 14),
    "J": (7, 2, 2, 2, 2, 18, 12), "K": (17, 18, 20, 24, 20, 18, 17), "L": (16, 16, 16, 16, 16, 16, 31),
    "M": (17, 27, 21, 21, 17, 17, 17), "N": (17, 17, 25, 21, 19, 17, 17), "O": (14, 17, 17, 17, 17, 17, 14),
    "P": (30, 17, 17, 30, 16, 16, 16), "Q": (14, 17, 17, 17, 21, 18, 13), "R": (30, 17, 17, 30, 20, 18, 17),
    "S": (15, 16, 16, 14, 1, 1, 30), "T": (31, 4, 4, 4, 4, 4, 4), "U": (17, 17, 17, 17, 17, 17, 14),
    "V": (17, 17, 17, 17, 17, 10, 4), "W": (17, 17, 17, 21, 21, 21, 10), "X": (17, 17, 10, 4, 10, 17, 17),
    "Y": (17, 17, 10, 4, 4, 4, 4), "Z": (31, 1, 2, 4, 8, 16, 31),
    "0": (14, 17, 19, 21, 25, 17, 14), "1": (4, 12, 4, 4, 4, 4, 14), "2": (14, 17, 1, 2, 4, 8, 31),
    "3": (31, 2, 4, 2, 1, 17, 14), "4": (2, 6, 10, 18, 31, 2, 2), "5": (31, 16, 30, 1, 1, 17, 14),
    "6": (6, 8, 16, 30, 17, 17, 14), "7": (31, 1, 2, 4, 8, 8, 8), "8": (14, 17, 17, 14, 17, 17, 14),
    "9": (14, 17, 17, 15, 1, 2, 12),
    " ": (0, 0, 0, 0, 0, 0, 0), ".": (0, 0, 0, 0, 0, 12, 12), ",": (0, 0, 0, 0, 12, 4, 8),
    "-": (0, 0, 0, 31, 0, 0, 0), "_": (0, 0, 0, 0, 0, 0, 31), "=": (0, 0, 31, 0, 31, 0, 0),
    ":": (0, 12, 12, 0, 12, 12, 0), "/": (0, 1, 2, 4, 8, 16, 0), "(": (2, 4, 8, 8, 8, 4, 2),
    ")": (8, 4, 2, 2, 2, 4, 8), "[": (14, 8, 8, 8, 8, 8, 14), "]": (14, 2, 2, 2, 2, 2, 14),
    "+": (0, 4, 4, 31, 4, 4, 0), "%": (24, 25, 2, 4, 8, 19, 3), "?": (14, 17, 1, 2, 4, 0, 4),
    "!": (4, 4, 4, 4, 4, 0, 4), "'": (4, 4, 8, 0, 0, 0, 0), "#": (10, 10, 31, 10, 31, 10, 10),
    "*": (0, 4, 21, 14, 21, 4, 0), "<": (2, 4, 8, 16, 8, 4, 2), ">": (8, 4, 2, 1, 2, 4, 8),
}
GLYPH_WIDTH, GLYPH_HEIGHT, GLYPH_ADVANCE = 5, 7, 6


def text_mask(text, scale=1, width=None):
    """A label as rows of 0 and 1, top row first, each glyph `scale` pixels a font pixel; a character the font
    lacks is drawn as `?`. With `width`, the text is cut to fit that many pixels."""
    text = text.upper()
    if width is not None:
        text = text[:max(0, (width + scale) // (GLYPH_ADVANCE * scale))]
    w = max(0, len(text) * GLYPH_ADVANCE * scale - scale)
    rows = [[0] * w for _ in range(GLYPH_HEIGHT * scale)]
    for k, char in enumerate(text):
        glyph = GLYPHS.get(char, GLYPHS["?"])
        for gy, bits in enumerate(glyph):
            for gx in range(GLYPH_WIDTH):
                if bits & (16 >> gx):
                    for dy in range(scale):
                        row = rows[gy * scale + dy]
                        for dx in range(scale):
                            row[k * GLYPH_ADVANCE * scale + gx * scale + dx] = 1
    return rows


def label_scale(cell_width):
    """The labels' font pixel size for a cell this wide (ours: about 40 characters a cell)."""
    return max(1, cell_width // 240)
