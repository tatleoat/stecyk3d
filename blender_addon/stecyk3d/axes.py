# SPDX-License-Identifier: GPL-3.0-or-later
# (The change of axes and units)
"""Blender's axes and units to the description's. Pure Python, binary64 arithmetic on singles."""

import math

from .numbers import single, singles


def to_bryce_point(p, units):
    """A point of Blender's world to the description's: (u*x, u*z, u*y)."""
    u = single(units)
    x, y, z = p
    return [u * x, u * z, u * y]


def to_bryce_axes(d):
    """A direction of Blender's axes to Bryce's, not normalised: (x, z, y)."""
    x, y, z = d
    return [x, z, y]


def to_bryce_direction(d, what="direction"):
    """A direction of Blender's axes to Bryce's, unit length."""
    x, y, z = to_bryce_axes(d)
    length = math.sqrt(x * x + y * y + z * z)
    if length == 0.0:
        raise ValueError("%s has no length" % what)
    return [x / length, y / length, z / length]


def quaternion_matrix(q):
    """The 3x3 rotation of a unit quaternion (w, x, y, z), as Blender's rotation_quaternion holds it."""
    w, x, y, z = q
    return [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)],
        [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)],
        [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)],
    ]


def mat4_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def object_matrix(location, rotation_quaternion, scale, shear=None):
    """Blender's matrix_world = T * R * S from an object's own settings (singles), times bryce_shear if held."""
    t = singles(location)
    r = quaternion_matrix(singles(rotation_quaternion))
    s = singles(scale)
    m = [[r[i][j] * s[j] for j in range(3)] + [t[i]] for i in range(3)]
    m.append([0.0, 0.0, 0.0, 1.0])
    if shear is not None:
        flat = [float(v) for v in shear]
        if len(flat) != 16:
            raise ValueError("bryce_shear holds %d numbers, not 16" % len(flat))
        m = mat4_mul(m, [flat[0:4], flat[4:8], flat[8:12], flat[12:16]])
    return m


def transform_point(m, v):
    """M * v for a point v (w = 1)."""
    return [m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3] for i in range(3)]


def triangle_order(a, b, c):
    """A triangle's corners in the opposite order ."""
    return (a, c, b)
