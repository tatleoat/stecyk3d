# SPDX-License-Identifier: GPL-3.0-or-later
# (a new shape: one mesh object marked as the shape, at the cursor, no turn, scale 1, selected alone
#       and active; a torus's radii 0.25 and 0.75; metaball and tree for the program 5.0)
# (a new terrain or symmetrical lattice:'s relief for the seed, grid the picture's own size,
#       bounds, smooth, faces, the scales (1, 1, 1) and (0.25, 0.25, 1), the box's middle at the cursor moved up by
#       1; a terrain from a mesh: the grid's first and last points on the mesh's box; a group from the selection)
# (the Create menu)
"""What the Add menu's Bryce entries make, as plain values. Pure Python."""

import random

from .numbers import single
from .solids import HEIGHT_TOP

TORUS_RADII = (0.25, 0.75)
SHAPES_ADDED = ("plane", "slab", "cube", "sphere", "cylinder", "cone", "pyramid", "torus", "square", "disc",
                "metaball", "tree")
FIVE_ONLY_SHAPES = ("metaball", "tree")  # Variants (offered for the program 5.0)

TERRAIN_SCALE = (1.0, 1.0, 1.0)
LATTICE_SCALE = (0.25, 0.25, 1.0)
TERRAIN_RAISE = 1.0
TERRAIN_LOWER = -0.00001
LATTICE_LOWER = 0.03
TERRAIN_UPPER = 1.00001
ALL_SOLID_FACES = 63
SEED_MAX = 2147483647

OWN_BOX_BLENDER = (8.0, 8.0, 2.0)
BOX_MARGIN = 0.01
HALF_HEIGHT = 32767.5
BOX_HALF_Y = 32767.51
BOX_HALF_Y_MIRRORED = 65535.01


class Refused(Exception):
    """The command refused; nothing was added (Errors)."""



def new_shape(shape, cursor):
    """: the new object's settings: `bryce_shape`, place, turn, scale, and the torus's radii."""
    if shape not in SHAPES_ADDED:
        raise ValueError("%r is not a shape the Add menu makes" % (shape,))
    return {"shape": shape, "location": [float(v) for v in cursor], "rotation": [0.0, 0.0, 0.0],
            "scale": [1.0, 1.0, 1.0], "torus": list(TORUS_RADII) if shape == "torus" else None}


def _ring(radius, z, count):
    import math
    return [(radius * math.cos(2 * math.pi * k / count), radius * math.sin(2 * math.pi * k / count), z)
            for k in range(count)]


def stand_in(shape, torus=None, segments=24):
    """Freedom (the mesh shown for a shape): vertices and faces showing where the shape is in its own space
    ('s table): our stand-ins. The render takes the exact shape ."""
    import math
    if shape in ("cube", "slab"):
        v = [(x, y, z) for z in (-1.0, 1.0) for y in (-1.0, 1.0) for x in (-1.0, 1.0)]
        f = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
        if shape == "slab":
            v = [(10.0 * x, 10.0 * y, z) for x, y, z in v]  # the slab is unbounded in x and y; a wide stand-in
        return v, f
    if shape in ("square", "plane"):
        s = 1.0 if shape == "square" else 10.0
        return [(-s, -s, 0.0), (s, -s, 0.0), (s, s, 0.0), (-s, s, 0.0)], [(0, 1, 2, 3)]
    if shape == "disc":
        return [(0.0, 0.0, 0.0)] + _ring(1.0, 0.0, segments), [(0, k + 1, (k + 1) % segments + 1)
                                                               for k in range(segments)]
    if shape in ("cylinder", "cone", "pyramid", "tree"):
        count = 4 if shape == "pyramid" else segments
        radius = math.sqrt(2.0) if shape == "pyramid" else 1.0
        bottom = _ring(radius, -1.0, count)
        if shape == "pyramid":
            bottom = [(x * math.cos(math.pi / 4) - y * math.sin(math.pi / 4),
                       x * math.sin(math.pi / 4) + y * math.cos(math.pi / 4), z) for x, y, z in bottom]
        if shape == "cylinder":
            top = _ring(1.0, 1.0, count)
            v = bottom + top
            f = [(k, (k + 1) % count, count + (k + 1) % count, count + k) for k in range(count)]
            f += [tuple(range(count - 1, -1, -1)), tuple(range(count, 2 * count))]
            return v, f
        v = bottom + [(0.0, 0.0, 1.0)]
        f = [(k, (k + 1) % count, count) for k in range(count)] + [tuple(range(count - 1, -1, -1))]
        return v, f
    if shape == "torus":
        tube, ring = torus or TORUS_RADII
        rings, sides = segments, max(8, segments // 2)
        v = []
        for i in range(rings):
            a = 2 * math.pi * i / rings
            for j in range(sides):
                b = 2 * math.pi * j / sides
                r = ring + tube * math.cos(b)
                v.append((r * math.cos(a), r * math.sin(a), tube * math.sin(b)))
        f = [(i * sides + j, ((i + 1) % rings) * sides + j, ((i + 1) % rings) * sides + (j + 1) % sides,
              i * sides + (j + 1) % sides) for i in range(rings) for j in range(sides)]
        return v, f
    # sphere, metaball and anything else: a UV sphere of radius 1
    rows = max(6, segments // 2)
    v = [(0.0, 0.0, -1.0)]
    for i in range(1, rows):
        a = math.pi * i / rows - math.pi / 2
        v += _ring(math.cos(a), math.sin(a), segments)
    v.append((0.0, 0.0, 1.0))
    top = len(v) - 1
    f = [(0, 1 + (k + 1) % segments, 1 + k) for k in range(segments)]
    for i in range(rows - 2):
        base = 1 + i * segments
        f += [(base + k, base + (k + 1) % segments, base + segments + (k + 1) % segments, base + segments + k)
              for k in range(segments)]
    last = 1 + (rows - 2) * segments
    f += [(last + k, last + (k + 1) % segments, top) for k in range(segments)]
    return v, f


# ---------------------------------------------------------------- the terrain's own space

def terrain_scales(n, mirrored):
    """The Blender own-space lengths of one terrain unit: (sx, sz), x and y of Blender from Bryce's own x and z,
    Blender's z from Bryce's own y: the own box of is 8 by 8 by 2 Blender units ."""
    sx = OWN_BOX_BLENDER[0] / (2.0 * (n / 2.0 + BOX_MARGIN))
    half_y = BOX_HALF_Y_MIRRORED if mirrored else BOX_HALF_Y
    sz = OWN_BOX_BLENDER[2] / (2.0 * half_y)
    return sx, sz


def grid_point(i, j, height, n, mirrored):
    """: grid point (i, j) at `height` in the terrain object's own space (Blender's axes): x = i - n/2 and
    z = j - n/2 (Blender's x and y), y = H - 32767.5, or H for a mirrored terrain (Blender's z), scaled."""
    sx, sz = terrain_scales(n, mirrored)
    y = height if mirrored else height - HALF_HEIGHT
    return ((i - n / 2.0) * sx, (j - n / 2.0) * sx, y * sz)


def terrain_world(matrix, n, mirrored, units):
    """The description's own transform of a terrain object ('s form): its matrix times the scaling from the
    terrain's own units to Blender's own space, through's change of axes."""
    from .solids import own_transform
    sx, sz = terrain_scales(n, mirrored)
    scaled = [[matrix[r][0] * sx, matrix[r][1] * sx, matrix[r][2] * sz, matrix[r][3]] for r in range(3)]
    scaled.append(list(matrix[3]))
    return own_transform(scaled, units)


def matrix_of(location, scale):
    """A 4x4 matrix of a place and a scale, no turn."""
    return [[scale[0], 0.0, 0.0, location[0]],
            [0.0, scale[1], 0.0, location[1]],
            [0.0, 0.0, scale[2], location[2]],
            [0.0, 0.0, 0.0, 1.0]]


def world_box(location, scale, units):
    """The terrain's own box (8 by 8 by 2 about the origin, Blender's own space) carried by the object's matrix
    (place and scale, no turn) to the description's world (x, y, z = Blender x, z, y times units): (size, centre)."""
    u = single(units)
    lo, hi = [], []
    for axis in range(3):
        half = OWN_BOX_BLENDER[axis] / 2.0 * scale[axis]
        a, b = location[axis] - half, location[axis] + half
        lo.append(min(a, b) * u)
        hi.append(max(a, b) * u)
    order = (0, 2, 1)  # : (x, y, z) -> (x, z, y)
    size = [hi[k] - lo[k] for k in order]
    centre = [(hi[k] + lo[k]) / 2.0 for k in order]
    return size, centre


# ----------------------------------------------------------------: a new terrain or lattice

def draw_seed(rng=random):
    """Freedom: the seed drawn when the seed is 0."""
    return rng.randint(1, SEED_MAX)


def new_terrain(kind, cursor):
    """: the new terrain object's place, scale and `bryce_terrain` settings (its heights are's)."""
    lattice = kind == "lattice"
    if kind not in ("terrain", "lattice"):
        raise ValueError("%r is neither terrain nor lattice" % (kind,))
    return {
        "location": [float(cursor[0]), float(cursor[1]), float(cursor[2]) + TERRAIN_RAISE],
        "scale": list(LATTICE_SCALE if lattice else TERRAIN_SCALE),
        "rotation": [0.0, 0.0, 0.0],
        "terrain": {"grid": "AUTO", "lower": single(LATTICE_LOWER if lattice else TERRAIN_LOWER),
                    "upper": single(TERRAIN_UPPER), "mirrored": lattice, "smooth": True, "faces": 0,
                    "solid_faces": ALL_SOLID_FACES},
    }


# ----------------------------------------------------------------: a terrain from a mesh

def from_mesh_matrix(lo, hi, n):
    """: the scaling and moving that puts grid point (0, 0) at height 0 on `lo` and (n - 1, n - 1) at
    65535 on `hi` (Blender's axes), the terrain not mirrored. Multiplied on the left by the mesh object's matrix."""
    first = grid_point(0, 0, 0, n, False)
    last = grid_point(n - 1, n - 1, HEIGHT_TOP, n, False)
    scale = [(hi[k] - lo[k]) / (last[k] - first[k]) for k in range(3)]
    move = [lo[k] - first[k] * scale[k] for k in range(3)]
    return matrix_of(move, scale)


def transform(m, p):
    return [sum(m[r][c] * v for c, v in enumerate(p)) + m[r][3] for r in range(3)]


# ----------------------------------------------------------------: a group

def group_plan(places, parents, selected):
    """: the members (the selected objects whose parent is not selected) and the empty's place (the mean of
    their places). `parents[i]` is object i's parent index or -1. Nothing selected: refused."""
    chosen = set(selected)
    if not chosen:
        raise Refused("select the objects to group")
    members = [i for i in sorted(chosen) if parents[i] not in chosen]
    total = [0.0, 0.0, 0.0]
    for i in members:
        total = [t + float(v) for t, v in zip(total, places[i])]
    place = [t / len(members) for t in total]
    return {"members": members, "place": place}
