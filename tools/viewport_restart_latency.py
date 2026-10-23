#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The add-on's ViewportSession against the real renderer: change the view while a render runs and time the new view.

usage: viewport_restart_latency.py RENDERER SUBSTRATE DESCRIPTION ORDER(ROWS|SCATTERED) [--after 0.3] [--limit 1.5]

The scene is the description file's (the glass vine, 110k triangles, in the owner's report). The view is a 1574x952
region; its camera is the description's own, moved along x by the view matrix's translation so that each change is a
real view change. Prints the seconds from the change to (a) the old request being stopped (the renderer's VEND) and
(b) the first pixels of the new request reaching the session's picture. Exit 1 when (b) is over --limit seconds."""
import argparse, json, os, sys, time
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, "..", "blender_addon"))
sys.path.insert(0, os.path.join(here, "..", "blender_addon", "tests"))
from stecyk3d import viewport_session as vs, viewport_camera as vc
from test_viewport_session import view

p = argparse.ArgumentParser()
p.add_argument("renderer"); p.add_argument("substrate"); p.add_argument("description"); p.add_argument("order")
p.add_argument("--after", type=float, default=0.3); p.add_argument("--limit", type=float, default=1.5)
a = p.parse_args()
base = json.load(open(a.description))

def members(base_, state, units, program, size):
    cam = dict(base_["camera"])
    pos = list(cam["position"]); pos[0] += -state.view_matrix[0][3]
    cam["position"] = pos
    return {"camera": cam, "render": vc.for_quality(base_["render"], state.quality), "size": [size[0], size[1]]}
vs.viewport_members = members

s = vs.ViewportSession(lambda: [a.renderer, "--substrate", a.substrate, "--serve"], time.monotonic)
s.set_live_order(a.order)
s.set_scene(base, 1.0, base["render"]["version"])
s.set_view(view(0.0, 1574, 952))
def pump(until, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        s.tick()
        if until(): return True
        time.sleep(0.005)
    return False
assert pump(lambda: s._active is not None, 5), "no request"
first_id = s._active
time.sleep(a.after)
t = time.monotonic()
s.set_view(view(0.4, 1574, 952))
stopped = None
def got():
    global stopped
    return s._active is not None and s._active != first_id and s.progress > 0
ok = pump(got, 120)
print("order=%s: request %d -> %s; first pixels of the new view %.3fs after the change" % (
    a.order, first_id, s._active, time.monotonic() - t if ok else -1))
s.close()
sys.exit(0 if ok and time.monotonic() - t <= a.limit else 1)
