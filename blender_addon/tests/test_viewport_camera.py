# SPDX-License-Identifier: GPL-3.0-or-later
# , viewport_pixel_size and viewport_quality, BS-1 4.5 (the projection),
# (The Rendered viewport: the camera, size and quality, the change key)
"""The viewport's view as the description's camera, the picture's size, the change key. The expected camera of a
view that is a scene camera's own follows from the scene camera's path (`camera_frame.renderer_view`,
`axes`), which the released vectors check."""

import math
import unittest

from stecyk3d import axes, camera_frame, viewport_camera as vc


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]


def mul3(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def world_matrix(location, yaw, pitch):
    r = mul3(rot_z(yaw), rot_x(pitch))
    return [r[0] + [location[0]], r[1] + [location[1]], r[2] + [location[2]], [0.0, 0.0, 0.0, 1.0]]


def inverse_rigid(m):
    r = [[m[j][i] for j in range(3)] for i in range(3)]  # R^T
    t = [-sum(r[i][k] * m[k][3] for k in range(3)) for i in range(3)]
    return [r[0] + [t[0]], r[1] + [t[1]], r[2] + [t[2]], [0.0, 0.0, 0.0, 1.0]]


def cam(kind="PERSP", **extra):
    c = {"type": kind, "sensor_fit": "AUTO", "sensor_width": 36.0, "sensor_height": 24.0, "lens": 50.0,
         "shift_x": 0.0, "shift_y": 0.0, "ortho_scale": 6.0, "clip_start": 0.1, "clip_end": 1000.0}
    c.update(extra)
    return c


def scene_camera(m, record, picture, units=1.0, program="5.0"):
    """What gather.describe_camera makes of a camera object with world matrix m (the same lines)."""
    view = camera_frame.renderer_view(record, picture, program, units)
    out = {"position": axes.to_bryce_point([m[0][3], m[1][3], m[2][3]], units),
           "forward": axes.to_bryce_direction([-m[0][2], -m[1][2], -m[2][2]], "view"),
           "up": axes.to_bryce_direction([m[0][1], m[1][1], m[2][1]], "up")}
    out.update(view)
    return out


class Camera(unittest.TestCase):
    def check_same(self, got, want):
        for key in ("position", "forward", "up", "pan"):
            for a, b in zip(got[key], want[key]):
                self.assertAlmostEqual(a, b, places=12, msg=key)
        for key in ("projection", "pixel_aspect"):
            self.assertEqual(got[key], want[key])
        for key in ("frame_angle", "plane_height"):
            if key in want:
                self.assertAlmostEqual(got[key], want[key], places=12, msg=key)

    def test_a_scene_cameras_own_view_perspective(self):
        picture = {"resolution_x": 640, "resolution_y": 480}
        for shift in (0.0, 0.2):
            record = cam(shift_x=shift, shift_y=-0.1 * shift)
            m = world_matrix([3.0, -7.0, 2.5], 0.6, 1.2)
            p = camera_frame.projection_matrix(record, picture)
            want = scene_camera(m, record, picture)
            for kind in ("PERSP", "CAMERA"):
                got = vc.camera_from_view(inverse_rigid(m), p, kind, 1.0, "5.0")
                self.check_same(got, want)

    def test_units(self):
        picture = {"resolution_x": 100, "resolution_y": 100}
        m = world_matrix([1.0, 2.0, 3.0], 0.1, 0.2)
        p = camera_frame.projection_matrix(cam(), picture)
        got = vc.camera_from_view(inverse_rigid(m), p, "PERSP", 2.0)
        want = scene_camera(m, cam(), picture, 2.0)
        self.check_same(got, want)
        for a, b in zip(got["position"], [2.0, 6.0, 4.0]):
            self.assertAlmostEqual(a, b, places=12)

    def test_a_scene_cameras_own_view_parallel(self):
        picture = {"resolution_x": 800, "resolution_y": 400}
        record = cam("ORTHO", ortho_scale=8.0)
        m = world_matrix([0.0, -10.0, 4.0], 0.0, 1.0)
        p = camera_frame.projection_matrix(record, picture)
        want = scene_camera(m, record, picture, 1.0, "3.0")
        got = vc.camera_from_view(inverse_rigid(m), p, "ORTHO", 1.0, "3.0")
        self.check_same(got, want)
        self.assertEqual(got["projection"], "parallel")

    def test_parallel_refused_unless_3_0(self):
        p = camera_frame.projection_matrix(cam("ORTHO"), {"resolution_x": 10, "resolution_y": 10})
        m = world_matrix([0.0, 0.0, 0.0], 0.0, 0.0)
        for program in ("3.1", "5.0"):
            with self.assertRaises(ValueError) as e:
                vc.camera_from_view(inverse_rigid(m), p, "ORTHO", 1.0, program)
            self.assertIn("orthographic", str(e.exception))

    def test_other_views_refused(self):
        m = world_matrix([0.0, 0.0, 0.0], 0.0, 0.0)
        p = camera_frame.projection_matrix(cam(), {"resolution_x": 10, "resolution_y": 10})
        with self.assertRaises(ValueError):
            vc.camera_from_view(inverse_rigid(m), p, "FISHEYE")

    def test_identity_view_looks_down_minus_z_of_blender_which_is_minus_y_of_bryce(self):
        ident = [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]
        p = camera_frame.projection_matrix(cam(), {"resolution_x": 10, "resolution_y": 10})
        got = vc.camera_from_view(ident, p, "PERSP")
        self.assertEqual(got["forward"], [0.0, -1.0, 0.0])  # Blender (0, 0, -1) as (x, z, y)
        self.assertEqual(got["up"], [0.0, 0.0, 1.0])  # Blender (0, 1, 0)


class SizeAndKey(unittest.TestCase):
    def test_pixel_size(self):
        self.assertEqual([vc.pixel_size(s) for s in ("1", "2", "4", "8")], [1, 2, 4, 8])
        self.assertEqual(vc.pixel_size("AUTO", 2), 2)
        self.assertEqual(vc.pixel_size("AUTO", 0), 1)
        with self.assertRaises(ValueError):
            vc.pixel_size("3")

    def test_picture_size_rounds_up_from_1(self):
        self.assertEqual(vc.picture_size(1311, 878, 1), (1311, 878))
        self.assertEqual(vc.picture_size(1311, 878, 4), (328, 220))
        self.assertEqual(vc.picture_size(3, 3, 8), (1, 1))
        self.assertEqual(vc.picture_size(0, 0, 1), (1, 1))

    def state(self, **change):
        ident = [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]
        values = dict(width=100, height=50, pixel_size=1, quality="RENDER", perspective="PERSP", view_matrix=ident,
                      window_matrix=ident)
        values.update(change)
        return vc.ViewState(**values)

    def test_key_equal_for_equal_views_and_different_for_each_change(self):
        base = vc.view_key(self.state())
        self.assertEqual(vc.view_key(self.state()), base)
        # lists and tuples of the same numbers are the same view
        self.assertEqual(vc.view_key(self.state(view_matrix=tuple(tuple(r) for r in self.state().view_matrix))), base)
        moved = [row[:] for row in self.state().view_matrix]
        moved[0][3] = 1e-6
        for change in (dict(width=101), dict(height=51), dict(pixel_size=2), dict(quality="DRAFT"),
                       dict(perspective="ORTHO"), dict(view_matrix=moved), dict(window_matrix=moved)):
            self.assertNotEqual(vc.view_key(self.state(**change)), base, change)


class Quality(unittest.TestCase):
    render = {"antialias": 2, "switches": 0x2000 | 0x4000, "version": "5.0"}

    def test_render_normal_draft(self):
        self.assertEqual(vc.for_quality(self.render, "RENDER"), self.render)
        normal = vc.for_quality(self.render, "NORMAL")
        self.assertEqual((normal["antialias"], normal["switches"]), (0, 0x6000))
        draft = vc.for_quality(self.render, "DRAFT")
        self.assertEqual((draft["antialias"], draft["switches"]), (0, 0x4000))
        self.assertEqual(self.render["antialias"], 2)  # the input is not changed
        with self.assertRaises(ValueError):
            vc.for_quality(self.render, "FAST")

    def test_viewport_description_keeps_the_scene_and_sets_camera_size_quality(self):
        ident = [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]
        p = camera_frame.projection_matrix(cam(), {"resolution_x": 10, "resolution_y": 10})
        state = vc.ViewState(80, 40, 2, "DRAFT", "PERSP", ident, p)
        base = {"render": dict(self.render), "objects": [1], "camera": {"stale": True}}
        d = vc.viewport_description(base, state, 1.0, "5.0", (40, 20))
        self.assertEqual(d["size"], [40, 20])
        self.assertEqual(d["objects"], [1])
        self.assertEqual(d["render"]["antialias"], 0)
        self.assertEqual(d["camera"]["projection"], "perspective")
        self.assertEqual(base["camera"], {"stale": True})


if __name__ == "__main__":
    unittest.main()
