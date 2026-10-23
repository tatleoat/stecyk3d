# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests; the live picture). Task T2 acceptance:
#   a headless Blender renders with the live picture on — the renderer's stream records reach
#   the add-on while it draws (progressive updates happen) — and the finished picture is
#   exact: the same bytes with the live picture on and off, and Blender's own saved picture
#   matching the renderer's levels (check_pixels.py compares them).
"""The live picture: progressive updates, and the exact final picture, with AA off and on."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402
import blender_sun_objects  # noqa: E402


def instrument(module, stats):
    """Count the stream: records arriving and results refreshed. The add-on's own functions,
    wrapped on the classes — no Blender calls, so the counters are safe from the headless run."""
    seen = stats

    orig_feed = module.engine.LivePicture.feed

    def feed(self, chunk):
        before = self.got
        orig_feed(self, chunk)
        if self.got != before:
            seen["pixels"] += self.got - before
            seen["feeds"] += 1

    module.engine.LivePicture.feed = feed

    orig_refresh = module.engine.BryceRenderEngine.refresh

    def refresh(self, result, scene):
        seen["refreshes"] += 1
        return orig_refresh(self, result, scene)

    module.engine.BryceRenderEngine.refresh = refresh


def main():
    import bpy  # noqa: E402  (Blender's own module, already in sys.modules)
    bpy, _substrate, program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.bryce_render.exact_png = prefix + "-exact.png"  # : the renderer's levels
    # A picture big enough that the render lasts longer than the engine's 0.25 s paint step:
    # the 64 by 48 factory picture draws in milliseconds, the whole stream arrives in one
    # read and no intermediate refresh can happen — there would be nothing progressive to see.
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    blender_sun_objects.build(bpy, scene)

    module = sys.modules["bl_ext.user_default.stecyk3d"]
    stats = {"pixels": 0, "feeds": 0, "refreshes": 0}
    instrument(module, stats)

    # The live picture is the default (task T2): do not touch `progressive`; it is True.
    bpy.ops.render.render(write_still=True)
    live_pixels, live_refreshes, live_feeds = stats["pixels"], stats["refreshes"], stats["feeds"]
    with open(prefix + "-exact.png", "rb") as f:
        exact_live = f.read()
    with open(prefix + "-exact-live.png", "wb") as f:  # keep this run's pair for check_pixels.py
        f.write(exact_live)
    with open(prefix + "-render.png", "rb") as f:
        with open(prefix + "-render-live.png", "wb") as g:
            g.write(f.read())

    # The same scene with anti-aliasing on (the scattered preview runs, then the exact frame),
    # then with the live picture off: the finished picture must be the same bytes each way.
    scene.bryce_render.antialias = "2"
    # Scattered streams the preview and then the frame; Rows streams only the row-order preview and the exact frame
    # arrives whole (Live order), so the two-passes check belongs to Scattered.
    scene.bryce_render.live_order = "SCATTERED"
    stats.update({"pixels": 0, "feeds": 0, "refreshes": 0})
    bpy.ops.render.render(write_still=True)
    aa_pixels, aa_refreshes = stats["pixels"], stats["refreshes"]
    with open(prefix + "-exact.png", "rb") as f:
        exact_aa = f.read()

    scene.bryce_render.progressive = False
    bpy.ops.render.render(write_still=True)
    with open(prefix + "-exact.png", "rb") as f:
        exact_plain = f.read()

    print("PROGRESSIVE-STATS live_pixels=%d feeds=%d refreshes=%d aa_pixels=%d aa_refreshes=%d"
          % (live_pixels, live_feeds, live_refreshes, aa_pixels, aa_refreshes))
    n = int(scene.render.resolution_x) * int(scene.render.resolution_y)
    ok = True
    if live_pixels < n or live_refreshes < 1:  # the whole picture arrived in pieces, and the view was refreshed
        print("PROGRESSIVE-UPDATES-BAD")
        ok = False
    if aa_pixels < 2 * n:  # the quick scattered preview and the frame each send the picture once
        print("PROGRESSIVE-AA-BAD (preview or frame missing)")
        ok = False
    if exact_aa != exact_plain:
        print("PROGRESSIVE-FINAL-BAD (the live render's finished picture is not the plain one's)")
        ok = False
    if exact_live == exact_aa:
        print("PROGRESSIVE-AA-SAME-BAD (the anti-aliasing setting changed nothing)")
        ok = False
    print("PROGRESSIVE-DONE" if ok else "PROGRESSIVE-FAILED")


main()
