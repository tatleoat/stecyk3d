# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport; Tests)
"""The viewport session and the engine-side functions, with a fake program (fake_renderer.py) and a fake engine.
No Blender: the timer is called by the test."""

import os
import sys
import time
import types
import unittest

from stecyk3d import camera_frame, viewport, viewport_camera as vc, viewport_session as vs

HERE = os.path.dirname(os.path.abspath(__file__))
FAKE = os.path.join(HERE, "fake_renderer.py")


def command():
    return [sys.executable, "-I", FAKE, "--substrate", "none", "--serve"]


def projection():
    return camera_frame.projection_matrix(
        {"type": "PERSP", "sensor_fit": "AUTO", "sensor_width": 36.0, "sensor_height": 24.0, "lens": 50.0,
         "shift_x": 0.0, "shift_y": 0.0, "ortho_scale": 6.0, "clip_start": 0.1, "clip_end": 100.0},
        {"resolution_x": 8, "resolution_y": 6})


def view(x=0.0, width=8, height=6, quality="RENDER", pixel=1):
    # the eye at Blender x = -x (V's translation column is -x), looking down -z: description position[0] = -t0... see
    m = [[1.0, 0.0, 0.0, -x], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
    return vc.ViewState(width, height, pixel, quality, "PERSP", m, projection())


def expected(width, height, shift):
    out = bytearray()
    for y in range(height):
        for x in range(width):
            v = (7 * x + 13 * y + shift) % 256
            out += bytes([v, (v + 1) % 256, (v + 2) % 256])
    return bytes(out)


BASE = {"render": {"antialias": 1, "switches": 0, "units": 1.0, "version": "5.0"}, "objects": []}


def wait(session, until, seconds=20.0):
    end = time.time() + seconds
    while time.time() < end:
        session.tick()
        if until():
            return True
        time.sleep(0.01)
    return False


class Session(unittest.TestCase):
    def make(self):
        s = vs.ViewportSession(command, time.monotonic)
        self.addCleanup(s.close)
        s.set_scene(BASE, 1.0, "5.0")
        return s

    def test_a_run_ends_in_the_exact_picture_and_close_removes_the_folder(self):
        s = self.make()
        s.set_view(view(x=3.0))
        self.assertTrue(s.busy())
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(bytes(s.levels), expected(8, 6, 3))
        self.assertEqual(s.size, (8, 6))
        self.assertFalse(s.busy())
        self.assertEqual(s.progress, 1.0)
        self.assertEqual(os.listdir(s._folder).count("levels-1.rgb"), 0)  # the levels file is taken and removed
        folder = s._folder
        s.close()
        self.assertFalse(os.path.exists(folder))

    def test_the_pixels_show_as_they_arrive_then_the_final_picture_replaces_them(self):
        os.environ["FAKE_DELAY"] = "0.05"
        self.addCleanup(os.environ.pop, "FAKE_DELAY", None)
        s = self.make()
        s.set_view(view(x=1.0, width=8, height=20))
        seen = []
        end = time.time() + 20
        while time.time() < end and not s.final:
            if s.tick():
                seen.append(s.progress)
            time.sleep(0.01)
        self.assertTrue(s.final)
        partial = [p for p in seen if 0.0 < p < 1.0]
        self.assertGreaterEqual(len(partial), 2, seen)  # more than one picture on the way
        self.assertEqual(sorted(partial), partial)
        self.assertEqual(bytes(s.levels), expected(8, 20, 1))

    def test_a_change_cancels_the_running_child_and_starts_another(self):
        os.environ["FAKE_DELAY"] = "0.2"
        self.addCleanup(os.environ.pop, "FAKE_DELAY", None)
        s = self.make()
        s.set_view(view(x=1.0))
        self.assertTrue(wait(s, lambda: s._active is not None))
        server = s._server
        self.assertTrue(s.set_view(view(x=5.0)))
        self.assertIsNone(s._active)  # the old request's pixels are not taken from now on
        self.assertTrue(wait(s, lambda: s._active is not None))  # the newer request is out: the server ends the old
        os.environ["FAKE_DELAY"] = "0"
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(bytes(s.levels), expected(8, 6, 5))
        self.assertIs(s._server, server)  # one renderer served both requests
        self.assertEqual(s.server_starts, 1)

    def test_the_same_view_changes_nothing(self):
        s = self.make()
        self.assertTrue(s.set_view(view()))
        self.assertTrue(wait(s, lambda: s.final))
        v = s.version
        self.assertFalse(s.set_view(view()))
        s.tick()
        self.assertTrue(s.final)
        self.assertEqual(s.version, v)
        self.assertFalse(s.busy())

    def test_the_debounce_holds_a_start_back_while_the_view_keeps_changing(self):
        now = [100.0]
        s = vs.ViewportSession(command, lambda: now[0])
        self.addCleanup(s.close)
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view(x=1.0))
        s.tick()
        self.assertIsNone(s._active)  # nothing yet: the view has not been still
        now[0] += vs.DEBOUNCE / 2
        s.set_view(view(x=2.0))  # moved again: the wait starts over
        now[0] += vs.DEBOUNCE * 0.75
        s.tick()
        self.assertIsNone(s._active)
        now[0] += vs.DEBOUNCE
        s.tick()
        self.assertIsNotNone(s._active)

    def test_a_size_change_starts_from_white_and_the_same_size_from_the_old_picture(self):
        s = self.make()
        s.set_view(view(x=0.0))
        self.assertTrue(wait(s, lambda: s.final))
        s.set_view(view(x=9.0))
        self.assertTrue(wait(s, lambda: s._active is not None))
        self.assertEqual(s.size, (8, 6))
        self.assertTrue(wait(s, lambda: s.final))
        s.set_view(view(x=9.0, width=5, height=4))
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(bytes(s.levels), expected(5, 4, 9))
        s.set_view(view(x=9.0, width=16, height=12, pixel=2))  # pixel size 2: the picture is 8 by 6 again
        self.assertTrue(wait(s, lambda: s.final))
        self.assertEqual(s.size, (8, 6))

    def test_a_refusing_child_leaves_a_message_not_a_picture(self):
        os.environ["FAKE_FAIL"] = "1"
        self.addCleanup(os.environ.pop, "FAKE_FAIL", None)
        s = self.make()
        s.set_view(view())
        self.assertTrue(wait(s, lambda: s.message != ""))
        self.assertIn("the scene is refused", s.message)
        self.assertFalse(s.final)

    def test_an_orthographic_view_of_program_5_is_a_message_and_starts_nothing(self):
        s = self.make()
        v = view()
        s.set_view(v._replace(perspective="ORTHO"))
        s.tick()
        time.sleep(vs.DEBOUNCE + 0.05)
        s.tick()
        self.assertIsNone(s._active)
        self.assertIn("orthographic", s.message)

    def test_a_missing_program_is_a_message(self):
        s = vs.ViewportSession(lambda: ["/nonexistent/program"], time.monotonic)
        self.addCleanup(s.close)
        s.set_scene(BASE, 1.0, "5.0")
        s.set_view(view())
        time.sleep(vs.DEBOUNCE + 0.05)
        s.tick()
        self.assertIn("cannot be started", s.message)
        self.assertFalse(s.busy())

    def test_close_stops_the_child(self):
        os.environ["FAKE_DELAY"] = "0.5"
        self.addCleanup(os.environ.pop, "FAKE_DELAY", None)
        s = self.make()
        s.set_view(view())
        self.assertTrue(wait(s, lambda: s._active is not None))
        server = s._server
        s.close()
        self.assertIsNotNone(server.poll())
        self.assertFalse(s.busy())


class FakeEngine:
    def __init__(self):
        self.redraws = 0
        self.stats = []
        self.timers = []
        self.reports = []

    def tag_redraw(self):
        self.redraws += 1

    def update_stats(self, a, b):
        self.stats.append(b)

    def get_preview_pixel_size(self, scene):
        return 1

    def report(self, kinds, text):
        self.reports.append(text)


def fake_context(x=0.0, width=8, height=6, quality="RENDER", pixel="1", updates=()):
    v = view(x, width, height)
    group = types.SimpleNamespace(viewport_pixel_size=pixel, viewport_quality=quality)
    scene = types.SimpleNamespace(bryce_render=group)
    return types.SimpleNamespace(
        region=types.SimpleNamespace(width=width, height=height),
        region_data=types.SimpleNamespace(view_perspective="PERSP", view_matrix=v.view_matrix,
                                          window_matrix=v.window_matrix),
        scene=scene)


class Functions(unittest.TestCase):
    def setUp(self):
        self.engine = FakeEngine()
        self.described = 0

        def describe(depsgraph, notices, view_camera=False):
            self.described += 1
            assert view_camera
            return dict(BASE), self.problems

        self.problems = []
        self.describe = describe
        self.resolve = lambda: ("prog", "sub", "")
        old = viewport.timer_registrar
        viewport.timer_registrar = lambda f, i: self.engine.timers.append(f)
        self.addCleanup(setattr, viewport, "timer_registrar", old)

    def start(self, depsgraph):
        viewport.view_update(self.engine, fake_context(), depsgraph, self.resolve, self.describe)
        vp = self.engine._vp
        vp.session.command = command
        return vp

    def test_update_gathers_only_when_something_changed(self):
        dg = types.SimpleNamespace(updates=[])
        viewport.view_update(self.engine, fake_context(), dg, self.resolve, self.describe)
        self.addCleanup(viewport.close, self.engine)
        self.assertEqual(self.described, 1)  # the first call
        viewport.view_update(self.engine, fake_context(x=1.0), dg, self.resolve, self.describe)
        self.assertEqual(self.described, 1)  # a view alone: no gather
        viewport.view_update(self.engine, fake_context(x=1.0), types.SimpleNamespace(updates=[1]), self.resolve,
                             self.describe)
        self.assertEqual(self.described, 2)

    def test_timer_runs_to_the_final_picture_and_asks_for_redraws(self):
        dg = types.SimpleNamespace(updates=[])
        vp = self.start(dg)
        self.addCleanup(viewport.close, self.engine)
        self.assertTrue(vp.session.busy())
        viewport.keep_ticking(self.engine)
        self.assertEqual(len(self.engine.timers), 1)
        viewport.keep_ticking(self.engine)
        self.assertEqual(len(self.engine.timers), 1)  # one timer at a time
        step = self.engine.timers[0]
        end = time.time() + 20
        result = 0
        while result is not None and time.time() < end:
            result = step()
            time.sleep(0.01)
        self.assertIsNone(result)  # idle: the timer stops
        self.assertTrue(vp.session.final)
        self.assertGreaterEqual(self.engine.redraws, 1)
        self.assertFalse(vp.timer_on)

    def test_the_timer_stops_when_the_engine_is_gone(self):
        dg = types.SimpleNamespace(updates=[])
        self.start(dg)
        viewport.keep_ticking(self.engine)
        step = self.engine.timers[0]
        viewport.close(self.engine)
        self.engine = None
        import gc
        gc.collect()
        self.assertIsNone(step())

    def test_a_problem_of_the_gathering_is_the_status_and_no_child_starts(self):
        self.problems = ["the scene has no World with a Bryce sky"]
        vp = self.start(types.SimpleNamespace(updates=[]))
        self.assertIn("no World", vp.problem)
        self.assertIn("no World", self.engine.stats[-1])
        self.assertIsNone(vp.session._active)
        self.assertIsNone(vp.session.base)

    def test_no_program_is_the_status(self):
        self.resolve = lambda: (None, None, "No substrate file is set")
        viewport.view_update(self.engine, fake_context(), types.SimpleNamespace(updates=[]), self.resolve,
                             self.describe)
        self.assertEqual(self.engine.stats[-1], "No substrate file is set")

    def test_a_draw_that_sees_a_new_view_restarts(self):
        dg = types.SimpleNamespace(updates=[])
        vp = self.start(dg)
        self.addCleanup(viewport.close, self.engine)
        key = vp.session.key
        # view_draw reads the view first and draws only when there is a picture: with none yet it returns
        # before importing gpu, which a plain Python has not.
        viewport.view_draw(self.engine, fake_context(x=4.0), dg)
        self.assertNotEqual(vp.session.key, key)
        self.assertIsNone(vp.session.levels)


if __name__ == "__main__":
    unittest.main()
