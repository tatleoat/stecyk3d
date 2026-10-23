# SPDX-License-Identifier: GPL-3.0-or-later
# BS-1 (1.7 which items draw, 2.6 material slots, 3.3 mirrored
#       matrices, 5.4 a light's direction)
# (Reading the scene)
"""Rules for the items of Blender's evaluated graph, on plain values. Pure Python; no bpy."""

import math

GEOMETRY_TYPES = ("MESH",)  # BS-1 1.7 (curves, texts, surfaces, metaballs come again as MESH instances)


def gives_geometry(object_type, show_self=True):
    """Whether an item's triangles are taken: MESH items whose own geometry shows. spec: BS-1 1.3, 1.7, 2.1"""
    return bool(show_self) and object_type in GEOMETRY_TYPES


def slot_index(material_index, slot_count):
    """The object's slot a triangle takes, or None when the object has no slot.

    An index past the last slot takes the last (design note: what Cycles was seen to do, BS-1 2.6)."""
    if slot_count <= 0:
        return None
    return min(max(int(material_index), 0), slot_count - 1)


def determinant3(m):
    """The determinant of a 4 by 4 matrix' 3 by 3 part (rows m[i][j])."""
    return (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))


def is_mirrored(m):
    """True when the matrix reverses the winding of the triangles it carries. spec: BS-1 3.3"""
    return determinant3(m) < 0.0


def light_direction(m):
    """The way a light shines: its matrix' third column negated, unit length (Blender's axes). spec: BS-1 5.4"""
    d = [-float(m[0][2]), -float(m[1][2]), -float(m[2][2])]
    length = math.sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
    if length == 0.0:
        raise ValueError("the light's matrix has no z axis")
    return [d[0] / length, d[1] / length, d[2] / length]


def translation(m):
    """The place a matrix puts its object's origin. spec: BS-1 3.1, 5.9"""
    return [float(m[0][3]), float(m[1][3]), float(m[2][3])]
