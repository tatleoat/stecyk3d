# SPDX-License-Identifier: GPL-3.0-or-later
# (Meshes: smooth normals and texture coordinates; found by tests/blender_vs_bryce)
"""Pure-Python checks of a mesh's mixed normals and texture coordinates on the way in (reader mesh -> Blender plan)
and out (Blender corner data -> description), and that the two agree."""

import math
import unittest

from stecyk3d import convert, importing

IDENTITY = [[1.0, 0, 0, 0], [0, 1.0, 0, 0], [0, 0, 1.0, 0], [0, 0, 0, 1.0]]


def a_mesh():
    return {"points": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "triangles": [[0, 1, 2], [0, 1, 2]],
            "normals": [[0.0, 1.0, 0.0], [0.0, 0.6, 0.8]], "uvs": [[0.25, 0.25], [0.5, 1.0]],
            "corner_normals": [[0, 1, 1], None], "corner_uvs": [[0, 1, 1], [1, 0, 0]], "flags": [3, 2]}


class Origin(unittest.TestCase):
    def test_origin_is_the_objects_place_in_bryce_axes(self):
        m = [[1.0, 0, 0, 2.0], [0, 1.0, 0, 3.0], [0, 0, 1.0, 5.0], [0, 0, 0, 1.0]]
        # Blender (x, y, z) is Bryce (x, z, y): the place carried to the world the way a corner is
        self.assertEqual([round(v, 6) for v in convert.mesh_origin(1.0, m)], [2.0, 5.0, 3.0])


class Plan(unittest.TestCase):
    def test_a_mesh_with_neither_has_no_shading(self):
        self.assertEqual(importing.mesh_shading_plan({"points": [], "triangles": []}), {})

    def test_corners_turn_opposite_and_axes_swap(self):
        plan = importing.mesh_shading_plan(a_mesh())
        self.assertEqual(plan["smooth"], [True, False])
        self.assertEqual(plan["corner_normals"][0], [[0.0, 0.0, 1.0], [0.0, 0.8, 0.6], [0.0, 0.8, 0.6]])
        self.assertIsNone(plan["corner_normals"][1])
        # v is counted from the bottom in Blender: 1 - v
        self.assertEqual(plan["corner_uvs"][1], [[0.5, 0.0], [0.25, 0.75], [0.25, 0.75]])


class Out(unittest.TestCase):
    def test_nothing_smooth_and_no_uv_is_nothing(self):
        flat = [(False, ((0, 0, 1),) * 3, None)]
        self.assertEqual(convert.mesh_shading(IDENTITY, [(0, 1, 2)], flat), {})

    def test_round_trip_of_the_plan(self):
        plan = importing.mesh_shading_plan(a_mesh())
        shading = [(plan["smooth"][k], tuple(map(tuple, plan["corner_normals"][k])) if plan["corner_normals"][k] else
                    ((0, 0, 0),) * 3, tuple(map(tuple, plan["corner_uvs"][k]))) for k in range(2)]
        # the plan's triangles are (0, 2, 1) of the reader's: the way out takes them back
        got = convert.mesh_shading(IDENTITY, [(0, 2, 1), (0, 2, 1)], shading)
        self.assertEqual(got["flags"], [3, 2])
        self.assertEqual(got["corner_uvs"], [[0, 1, 1], [1, 0, 0]])
        n = [got["normals"][i] for i in got["corner_normals"][0]]
        for have, want in zip(n, [[0.0, 1.0, 0.0], [0.0, 0.6, 0.8], [0.0, 0.6, 0.8]]):
            self.assertTrue(all(abs(a - b) < 1e-6 for a, b in zip(have, want)), (have, want))

    def test_a_mirror_matrix_carries_normals_and_keeps_facing(self):
        m = [[-1.0, 0, 0, 0], [0, 1.0, 0, 0], [0, 0, 1.0, 0], [0, 0, 0, 1.0]]
        carry = convert.normal_matrix(m)
        self.assertEqual(carry, [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
        out = convert.mesh_shading(m, [(0, 1, 2)], [(True, ((1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 1.0, 0.0)), None)])
        self.assertEqual(out["normals"], [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
        # mirrored: the corners were turned (0, 2, 1) first, then again opposite: 0, 1, 2 of the original
        self.assertEqual(out["corner_normals"], [[0, 1, 2]])

    def test_no_inverse_gives_nothing(self):
        self.assertIsNone(convert.normal_matrix([[0.0] * 4] * 4))


if __name__ == "__main__":
    unittest.main()
