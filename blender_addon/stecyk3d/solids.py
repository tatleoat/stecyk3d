# SPDX-License-Identifier: GPL-3.0-or-later
# (a metaball's and a group's own transform; bryce_negative's sign; groups and their
#       members' roles; bryce_terrain's heights from the picture's red channel, grid, faces (grid AUTO: a picture
#       of another side resampled to the next grid size, with a notice); the terrain tool
#       "from a mesh"; the thinned viewport stand-in)
# (The revision of: Solids)
"""Metaballs, groups and terrains: plain values to the description's fields. Pure Python."""

import math

from . import names
from .numbers import single

HEIGHT_TOP = 65535
FACE_BITS = 0x3F  # [format]
GRID_MIN, GRID_MAX = 2, 1024  # Limits, Limits (2 to 1024 points a side)
AUTO_SIZES = tuple(int(k) for k in names.TERRAIN_GRID if k != "AUTO")
OWN_BOX = (8.0, 8.0, 2.0)
STAND_IN_MAX = 256


# ---------------------------------------------------------------- the own transform ('s form)

def swap_yz(v):
    """C(x, y, z) = (x, z, y) ."""
    return [v[0], v[2], v[1]]


def columns_times_units(matrix, units):
    """The world matrix's columns X, Y, Z, T (Blender's axes), each times single(units)."""
    u = single(units)
    return [[matrix[i][j] * u for i in range(3)] for j in range(4)]


def own_transform(matrix, units):
    """: rows C(X), C(Z), C(Y), then the translation C(T), each column times `units`; twelve numbers."""
    X, Y, Z, T = columns_times_units(matrix, units)
    return swap_yz(X) + swap_yz(Z) + swap_yz(Y) + swap_yz(T)


BOX_SHAPES = ("cube", "pyramid")
BOX_FACTOR = 2.0


def object_transform(shape, matrix, units):
    """The description's transform of an ordinary shape ('s form): as own_transform, and for a cube and
    a pyramid the three axes times 2 (the shape's own box is Blender's -1 to 1, Bryce's own cube -0.5 to 0.5).
    ; README torus-shear."""
    flat = own_transform(matrix, units)
    if shape in BOX_SHAPES:
        flat = [c * BOX_FACTOR for c in flat[:9]] + flat[9:]
    return flat


def torus_radii(bryce_torus):
    """`bryce_torus`: the tube's and the ring's radius, in the object's own space (not times units), as the
    setting's singles."""
    if len(bryce_torus) != 2:
        raise ValueError("bryce_torus holds %d numbers, not 2" % len(bryce_torus))
    return [single(v) for v in bryce_torus]


# ---------------------------------------------------------------- metaballs and groups

def metaball(matrix, units, negative):
    """A metaball : its own transform and its sign (−1 with bryce_negative)."""
    return {"world": own_transform(matrix, units), "sign": -1 if negative else 1}


def member_entry(obj, units):
    """A group member : its world, its role, and the cut switch (true only for a negative member)."""
    role = obj.get("bryce_role", "NEUTRAL")
    if role not in names.ROLE:
        raise ValueError("object %r: bryce_role %r is not one of %s" % (obj["name"], role, ", ".join(names.ROLE)))
    return {
        "world": own_transform(obj["matrix"], units),
        "role": role,
        "cut_shows_positive": role == "NEGATIVE" and bool(obj.get("bryce_cut_shows_positive")),
    }


def describe_groups(objects, units):
    """Every group among `objects` by name: its own transform and its members (the objects it parents)."""
    groups = {}
    for obj in objects:
        if obj.get("bryce_shape") == "group":
            groups[obj["name"]] = {"world": own_transform(obj["matrix"], units), "members": {}}
    for obj in objects:
        parent = obj.get("parent")
        if parent in groups:
            groups[parent]["members"][obj["name"]] = member_entry(obj, units)
    return groups


# ---------------------------------------------------------------- terrains from a picture

def height_of(level):
    """: a red value (a binary32 from 0 to 1) times 65,535, rounded half to even, held to 0..65,535."""
    h = round(single(level) * HEIGHT_TOP)  # exact in binary64; Python's round is half to even
    return min(max(h, 0), HEIGHT_TOP)


def resize_grid(heights, old, new):
    """: bilinear, truncated; the binary64 place, its fraction the place less its whole part."""
    for side in (old, new):
        if not GRID_MIN <= side <= GRID_MAX:
            raise ValueError("a terrain grid of %d points a side is outside 2 to 1024" % side)
    return _bilinear(heights, old, new)


def auto_size(side):
    """The grid AUTO takes for a picture of `side` points a side: the side itself where it is one of AUTO_SIZES,
    else the next one up, at most the largest (our choice: the program has no grid between them). A side below
    GRID_MIN is refused."""
    if side < GRID_MIN:
        raise ValueError("the terrain's picture is %d point%s a side, fewer than %d"
                         % (side, "" if side == 1 else "s", GRID_MIN))
    return next((s for s in AUTO_SIZES if s >= side), AUTO_SIZES[-1])


def resample(heights, old, new):
    """A picture's heights, `old` points a side (any side from GRID_MIN up, the largest too), brought to the grid
    size `new` as resize_grid does: bilinear over the same extent, the first and last points kept where they are,
    so the terrain's shape and its box stay the same."""
    if old < GRID_MIN or new not in AUTO_SIZES:
        raise ValueError("a picture of %d points a side cannot be resampled to %d" % (old, new))
    return _bilinear(heights, old, new)


def _bilinear(heights, old, new):
    if old == new:
        return list(heights)
    places = []
    for a in range(new):
        place = float(a * (old - 1)) / float(new - 1)
        whole = int(place)
        nxt = whole + 1 if whole + 1 < old else whole
        places.append((whole, nxt, place - whole))
    out = []
    for r0, r1, fr in places:
        for c0, c1, fc in places:
            h00 = float(heights[r0 * old + c0])
            h01 = float(heights[r0 * old + c1])
            h10 = float(heights[r1 * old + c0])
            h11 = float(heights[r1 * old + c1])
            top = h00 + (h01 - h00) * fc
            bottom = h10 + (h11 - h10) * fc
            out.append(int(top + (bottom - top) * fr))
    return out


def terrain(bryce_terrain, width, height, red, notices=None):
    """: the description's terrain from `bryce_terrain` and the picture's red channel (bottom row first).
    With grid AUTO a picture whose side is not a grid size is resampled to auto_size's, and a line saying so is
    added to `notices`."""
    if width != height:
        raise ValueError("the terrain's picture is %d by %d, not square" % (width, height))
    if len(red) != width * height:
        raise ValueError("the terrain's picture holds %d values, not %d" % (len(red), width * height))
    grid = str(bryce_terrain.get("grid", "AUTO"))
    if grid not in names.TERRAIN_GRID:
        raise ValueError("grid %r is not one of %s" % (grid, ", ".join(names.TERRAIN_GRID)))
    levels = [height_of(v) for v in red]
    if grid == "AUTO":
        size = auto_size(width)
        heights = resample(levels, width, size)
        if size != width and notices is not None:
            notices.append("the terrain's picture is %d points a side, not a grid size: resampled to %d "
                           "(bilinear, same extent)" % (width, size))
    else:
        size = int(grid)
        heights = resize_grid(levels, width, size)
    return {
        "size": size,
        "heights": heights,
        "lower": single(bryce_terrain.get("lower", 0.0)),
        "upper": single(bryce_terrain.get("upper", 1.0)),
        "mirrored": bool(bryce_terrain.get("mirrored", False)),
        "smooth": bool(bryce_terrain.get("smooth", False)),
        "faces": int(bryce_terrain.get("faces", 0)) & FACE_BITS,
        "solid_faces": int(bryce_terrain.get("solid_faces", 0)) & FACE_BITS,
    }


# ---------------------------------------------------------------- the terrain tool "from a mesh"

def mesh_box(points):
    """The smallest and largest x, y, z of the points; a zero extent or no points is refused."""
    if not points:
        raise ValueError("the mesh is empty")
    lo = [min(p[k] for p in points) for k in range(3)]
    hi = [max(p[k] for p in points) for k in range(3)]
    if any(h - l == 0.0 for l, h in zip(lo, hi)):
        raise ValueError("the mesh is flat")
    return lo, hi


def heights_from_mesh(points, triangles, n):
    """'s terrain tool: n by n heights (row j along y), by rays straight down, 0 where none meets.

    points: the mesh's points (object space, Blender's axes); triangles: three point numbers each."""
    if n not in AUTO_SIZES:
        raise ValueError("a grid of %d is not one of %s" % (n, ", ".join(str(s) for s in AUTO_SIZES)))
    (xlo, ylo, zlo), (xhi, yhi, zhi) = mesh_box(points)
    xs = [xlo + (xhi - xlo) * i / (n - 1) for i in range(n)]
    ys = [ylo + (yhi - ylo) * j / (n - 1) for j in range(n)]
    best = [None] * (n * n)
    for tri in triangles:
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = (points[k] for k in tri)
        det = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if det == 0.0:
            continue  # seen edge on from above: a ray meets no area of it
        i0 = _first_at_least(xs, min(ax, bx, cx))
        i1 = _last_at_most(xs, max(ax, bx, cx))
        j0 = _first_at_least(ys, min(ay, by, cy))
        j1 = _last_at_most(ys, max(ay, by, cy))
        for j in range(j0, j1 + 1):
            y = ys[j]
            for i in range(i0, i1 + 1):
                x = xs[i]
                l1 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / det
                l2 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / det
                l3 = 1.0 - l1 - l2
                if l1 < 0.0 or l2 < 0.0 or l3 < 0.0:
                    continue
                z = l1 * az + l2 * bz + l3 * cz
                k = j * n + i
                if best[k] is None or z > best[k]:
                    best[k] = z
    span = zhi - zlo
    out = []
    for z in best:
        if z is None:
            out.append(0)
        else:
            out.append(min(max(round((z - zlo) / span * HEIGHT_TOP), 0), HEIGHT_TOP))
    return out


def _first_at_least(values, v):
    lo, hi = 0, len(values)
    while lo < hi:
        mid = (lo + hi) // 2
        if values[mid] < v:
            lo = mid + 1
        else:
            hi = mid
    return lo


def _last_at_most(values, v):
    lo, hi = 0, len(values)
    while lo < hi:
        mid = (lo + hi) // 2
        if values[mid] <= v:
            lo = mid + 1
        else:
            hi = mid
    return lo - 1


def terrain_placement(lo, hi, n):
    """The 4x4 matrix that puts the grid's point (0, 0) at height 0 on `lo` and (n - 1, n - 1) at 65,535 on `hi`
    . The terrain's own space is create.py's ('s own box 8 by 8 by 2 at scale 1); this
    replaces's placement, which laid the box's edges, not the grid's points, on the mesh's box.
    """
    from .create import from_mesh_matrix  # create imports this module
    return from_mesh_matrix(lo, hi, n)


def stand_in_rows(n):
    """: the rows (and columns) the viewport stand-in keeps: every k-th, and always the last."""
    k = max(1, math.ceil((n - 1) / STAND_IN_MAX))
    rows = list(range(0, n, k))
    if rows[-1] != n - 1:
        rows.append(n - 1)
    return rows
