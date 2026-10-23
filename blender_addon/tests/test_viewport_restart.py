# SPDX-License-Identifier: GPL-3.0-or-later
# (The long-lived renderer: cancel; The Rendered viewport: timer)
"""A view change ends the old view's render at once, not when the new request is sent and not when the old pass is
over. Part one uses the fake program; part two the real `stecyk3d-render` (STECYK3D_RENDER, STECYK3D_SUBSTRATE; skipped without
them) and times, as the viewport's timer does (a tick every 50 ms), how soon the new view's first pixels arrive."""

import json
import os
import time
import unittest

from stecyk3d import viewport_session as vs
from test_viewport_session import BASE, Session as _Base, view, wait

HERE = os.path.dirname(os.path.abspath(__file__))
SCENE = os.path.join(HERE, "..", "..", "tests", "support", "serve-scene.json")
BOUND = float(os.environ.get("RESTART_BOUND", "1.0"))  # seconds from the view change to the new view's first pixels


class CancelAtOnce(_Base):
    def test_the_renderer_is_told_to_stop_when_the_view_changes_not_when_the_next_request_is_sent(self):
        os.environ["FAKE_DELAY"] = "0.2"
        self.addCleanup(os.environ.pop, "FAKE_DELAY", None)
        self.addCleanup(setattr, vs, "DEBOUNCE", vs.DEBOUNCE)
        s = self.make()
        ended = []
        real = s._ended
        s._ended = lambda *a: ended.append(a[:2]) or real(*a)
        s.set_view(view(x=1.0))
        self.assertTrue(wait(s, lambda: s._active is not None and s._decoder.got > 0))
        vs.DEBOUNCE = 60.0  # no new request for a minute
        s.set_view(view(x=5.0))
        self.assertTrue(wait(s, lambda: ended, seconds=5.0), "the old render was left running")
        self.assertEqual(ended, [(1, 2)])  # cancelled
        self.assertEqual(s.starts, 1)  # and no other request was sent meanwhile


def real_program():
    program, substrate = os.environ.get("STECYK3D_RENDER"), os.environ.get("STECYK3D_SUBSTRATE")
    ok = program and substrate and os.path.exists(program) and os.path.exists(substrate)
    return (program, substrate) if ok else None


@unittest.skipUnless(real_program(), "set STECYK3D_RENDER and STECYK3D_SUBSTRATE to time the real renderer")
class RealRenderer(unittest.TestCase):
    def timed(self, order, grain):
        program, substrate = real_program()
        with open(SCENE, encoding="utf-8") as f:
            base = json.load(f)
        base["render"]["antialias"] = 4  # a render that lasts: far more than the 0.3 s before the change
        session = vs.ViewportSession(lambda: [program, "--substrate", substrate, "--serve"], time.monotonic)
        self.addCleanup(session.close)
        session.set_scene(base, 1.0, "5.0")
        session.set_live_order(order)
        session.set_wash_grain(grain)
        session.set_view(view(x=1.0, width=1200, height=800))
        began = time.monotonic()
        while time.monotonic() - began < 0.3:  # the timer's ticks while the first view renders
            session.tick()
            time.sleep(0.05)
        self.assertIsNotNone(session._active, "the first render ended before the change: no test")
        first = session._active
        changed = time.monotonic()
        session.set_view(view(x=3.0, width=1200, height=800))
        shown = None
        while time.monotonic() - changed < 10.0:
            session.tick()
            if session._active not in (None, first) and session._decoder is not None and session._decoder.got > 0:
                shown = time.monotonic() - changed
                break
            time.sleep(0.05)
        self.assertIsNotNone(shown, "the new view never showed a pixel")
        print("  %-9s %-6s new view's first pixels %.3f s after the change" % (order, grain, shown))
        self.assertLess(shown, BOUND)

    def test_rows_exact(self):
        self.timed("ROWS", "EXACT")

    def test_rows_independent(self):
        self.timed("ROWS", "FAST")

    def test_scattered_exact(self):
        self.timed("SCATTERED", "EXACT")

    def test_scattered_independent(self):
        self.timed("SCATTERED", "FAST")


if __name__ == "__main__":
    unittest.main()
