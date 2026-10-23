# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport; Tests). Roadmap items 6 and 7, checked headlessly:
#   a headless Blender has no 3D viewport, so the engine's view_update and view_draw are called on a fake
#   engine object with a fake region and space, with the real depsgraph and the real matrices of the scene
#   camera. Checks: the viewport's picture of the scene camera's own view equals the F12 picture; a changed view
#   gives another picture and cancels the running child; the pixel size; the quality; an orthographic view of
#   program 5.0 is refused with a message; the picture arrives in pieces. NOT checked here (needs a window):
#   the gpu drawing (reported as not checked), the timer-driven redraw, an orbit by hand.
"""The Rendered viewport's update path in a headless Blender, as far as Blender allows."""

import os
import sys
import time
import types

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402
import blender_sun_objects  # noqa: E402
from png_read import read_png  # noqa: E402

WIDTH, HEIGHT = 160, 120


class FakeEngine:
    """What the viewport functions call on an engine."""

    def __init__(self, module):
        self._vp = None
        self.redraws = 0
        self.stats = []
        self.reports = []
        self.timers = []
        self.module = module

    def tag_redraw(self):
        self.redraws += 1

    def update_stats(self, a, b):
        self.stats.append(b)

    def get_preview_pixel_size(self, scene):
        return 1

    def report(self, kinds, text):
        self.reports.append(text)

    def resolve_program(self):
        return self.module.engine.BryceRenderEngine.resolve_program(self)

    def bind_display_space_shader(self, scene):
        pass

    def unbind_display_space_shader(self):
        pass


def context_for(bpy, scene, view_matrix, window_matrix, perspective, width=WIDTH, height=HEIGHT):
    return types.SimpleNamespace(
        region=types.SimpleNamespace(width=width, height=height),
        region_data=types.SimpleNamespace(view_perspective=perspective, view_matrix=view_matrix,
                                          window_matrix=window_matrix),
        space_data=types.SimpleNamespace(lens=50.0, clip_start=0.1, clip_end=1000.0),
        scene=scene)


def drive(viewport, engine, seconds=60.0):
    """Call the engine's timer function until it says it is idle (Blender would, every 0.05 s)."""
    steps = 0
    end = time.time() + seconds
    while engine.timers and time.time() < end:
        result = engine.timers[0]()
        steps += 1
        if result is None:
            engine.timers.clear()
            return steps
        time.sleep(0.01)
    raise SystemExit("VIEWPORT-FAILED the timer did not go idle")


def differing(levels, exact):
    return sum(1 for i in range(0, len(exact), 3) if levels[i:i + 3] != exact[i:i + 3])


def main():
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.render.resolution_x = WIDTH
    scene.render.resolution_y = HEIGHT
    scene.bryce_render.exact_png = prefix + "-exact.png"
    blender_sun_objects.build(bpy, scene)
    module = sys.modules[blender_common.ADDON]
    viewport = module.viewport

    # The F12 picture of the scene camera: the exact levels the viewport must equal.
    bpy.ops.render.render(write_still=True)
    width, height, _channels, exact = read_png(prefix + "-exact.png")
    assert (width, height) == (WIDTH, HEIGHT), (width, height)
    exact = bytes(exact)

    depsgraph = bpy.context.evaluated_depsgraph_get()
    camera = scene.camera
    view = camera.matrix_world.inverted()
    window = camera.calc_matrix_camera(depsgraph, x=WIDTH, y=HEIGHT)
    engine = FakeEngine(module)
    viewport.timer_registrar = lambda function, interval: engine.timers.append(function)
    ok = True

    # 1. view_update, then the timer to the end: the viewport's picture of the camera's own view.
    scene.bryce_render.viewport_quality = "RENDER"
    ctx = context_for(bpy, scene, view, window, "CAMERA")
    module.engine.BryceRenderEngine.view_update(engine, ctx, depsgraph)
    vp = engine._vp
    assert vp is not None and engine.timers, "view_update made no session or no timer"
    steps = drive(viewport, engine)
    session = vp.session
    print("VIEWPORT-RUN steps=%d version=%d redraws=%d final=%s message=%r problem=%r"
          % (steps, session.version, engine.redraws, session.final, session.message, vp.problem))
    if not session.final or session.size != (WIDTH, HEIGHT):
        print("VIEWPORT-NO-FINAL-BAD")
        ok = False
    else:
        bad = differing(bytes(session.levels), exact)
        print("VIEWPORT-SAME-AS-F12 differing pixels: %d of %d" % (bad, WIDTH * HEIGHT))
        if bad:
            print("VIEWPORT-SAME-AS-F12-BAD")
            ok = False
    if engine.redraws < 1:
        print("VIEWPORT-NO-REDRAW-BAD")
        ok = False
    first = bytes(session.levels)

    # 1b. the Live order: both choices send the right order (the viewport) and flags (F12) and end on the same levels.
    session_module = sys.modules[module.__name__ + ".viewport_session"]
    orders = []
    original_name = session_module.live_order_name
    session_module.live_order_name = lambda order: orders.append(order) or original_name(order)
    flag_sets = []
    original_popen = module.engine.subprocess.Popen

    def spy_popen(command, *args, **kwargs):
        flag_sets.append([a for a in command if a in ("--rows", "--scattered", "--stream")])
        return original_popen(command, *args, **kwargs)
    module.engine.subprocess.Popen = spy_popen
    results = {}
    try:
        for order in ("SCATTERED", "ROWS"):
            scene.bryce_render.live_order = order
            module.engine.BryceRenderEngine.view_update(engine, ctx, depsgraph)
            drive(viewport, engine)
            results[order] = bytes(engine._vp.session.levels)
            bpy.ops.render.render(write_still=True)
            f12 = bytes(read_png(prefix + "-exact.png")[3])
            print("VIEWPORT-LIVE-ORDER %s viewport_same=%s f12_same=%s" % (order, results[order] == first, f12 == exact))
            if results[order] != first or f12 != exact or not engine._vp.session.final:
                print("VIEWPORT-LIVE-ORDER-BAD")
                ok = False
    finally:
        module.engine.subprocess.Popen = original_popen
        session_module.live_order_name = original_name
    wanted_flags = [["--stream", "--scattered"], ["--stream", "--rows"]]
    print("VIEWPORT-LIVE-FLAGS requests=%s f12=%s" % (orders, flag_sets))
    if not orders or orders[0] != "SCATTERED" or orders[-1] != "ROWS":
        print("VIEWPORT-LIVE-ORDER-BAD")
        ok = False
    seen = [f for f in flag_sets if f in wanted_flags]
    if seen != wanted_flags:
        print("VIEWPORT-LIVE-FLAGS-BAD")
        ok = False
    scene.bryce_render.live_order = "ROWS"
    module.engine.BryceRenderEngine.view_update(engine, ctx, depsgraph)
    drive(viewport, engine)

    # 1c. Rows is the old recomp's live render: the asked size at once (no coarse rung), whole rows filling in from
    #     the top, never back up; Scattered is not that (a coarse rung first, rows anywhere).
    big_w, big_h = 400, 300  # more than the ladder's 30,000 pixels, so Scattered has rungs
    ctx_big = context_for(bpy, scene, view, camera.calc_matrix_camera(depsgraph, x=big_w, y=big_h), "CAMERA",
                          width=big_w, height=big_h)
    stream_module = sys.modules[module.__name__ + ".stream"]
    original_put = stream_module.StreamDecoder.put
    watched = {}
    for order in ("SCATTERED", "ROWS"):
        scene.bryce_render.live_order = order
        # every record the renderer streams, as (picture size, row), whatever the timer's pace: the order they arrive
        arrivals = []

        def put(self, x, y, count, buf, source, arrivals=arrivals):
            arrivals.append(((self.width, self.height), y))
            return original_put(self, x, y, count, buf, source)
        stream_module.StreamDecoder.put = put
        try:
            module.engine.BryceRenderEngine.view_update(engine, ctx_big, depsgraph)
            drive(viewport, engine)
        finally:
            stream_module.StreamDecoder.put = original_put
        watched[order] = arrivals
        sizes = sorted({z for z, _y in arrivals})
        print("VIEWPORT-LIVE-SHAPE %s picture sizes streamed=%s records=%d" % (order, sizes, len(arrivals)))
    rows = watched["ROWS"]
    ys = [y for _z, y in rows]
    reach = ys.index(big_h - 1) + 1 if big_h - 1 in ys else len(ys)
    first_pass = ys[:reach]  # the first sweep; an edge pass may refine pixels after it
    full_size = {z for z, _y in rows} == {(big_w, big_h)}
    from_top = all(a <= b for a, b in zip(first_pass, first_pass[1:])) and set(first_pass) == set(range(big_h))
    sc = watched["SCATTERED"]
    sc_sizes = [z for z, _y in sc]
    coarse_first = sc_sizes[0] != (big_w, big_h) and (big_w, big_h) in sc_sizes
    sc_back = sum(1 for (_z1, a), (_z2, b) in zip(sc, sc[1:]) if b < a)
    print("VIEWPORT-LIVE-ROWS full_size_at_once=%s rows_0_to_%d_in_order=%s" % (full_size, big_h - 1, from_top))
    print("VIEWPORT-LIVE-SCATTERED coarse_rung_first=%s steps_back_up=%d" % (coarse_first, sc_back))
    if not (full_size and from_top and coarse_first and sc_back > 10):
        print("VIEWPORT-LIVE-SHAPE-BAD")
        ok = False
    scene.bryce_render.live_order = "ROWS"
    module.engine.BryceRenderEngine.view_update(engine, ctx, depsgraph)
    drive(viewport, engine)

    # 1d. A FRESH viewport (a new engine, nothing run before it) with Rows chosen from the start never climbs a
    #     ladder; and Scattered -> Rows while the coarse rung is still in flight restarts at the asked size.
    def sweep(engine_, switch=None):
        """Every picture size streamed, as (size, after_the_switch)."""
        seen = []
        switched = []
        viewport.timer_registrar = lambda function, interval: engine_.timers.append(function)  # this engine's own
        original = stream_module.StreamDecoder.put

        def put(self, x, y, count, buf, source):
            seen.append(((self.width, self.height), bool(switched)))
            return original(self, x, y, count, buf, source)
        stream_module.StreamDecoder.put = put
        try:
            module.engine.BryceRenderEngine.view_update(engine_, ctx_big, depsgraph)
            if switch:
                switch(engine_)
                switched.append(True)
            drive(viewport, engine_)
        finally:
            stream_module.StreamDecoder.put = original
        return seen

    def to_rows(engine_):
        for _ in range(400):  # until the coarse request is on the wire, then switch
            engine_.timers[0]()
            if engine_._vp.session.divisor == 2 and engine_._vp.session._active is not None:
                break
            time.sleep(0.005)
        scene.bryce_render.live_order = "ROWS"
        module.engine.BryceRenderEngine.view_update(engine_, ctx_big, depsgraph)

    scene.bryce_render.live_order = "ROWS"
    fresh = FakeEngine(module)
    fresh_sizes = sorted({z for z, _after in sweep(fresh)})
    print("VIEWPORT-LIVE-FRESH-ROWS sizes=%s final=%s" % (fresh_sizes, fresh._vp.session.final))
    if fresh_sizes != [(big_w, big_h)] or not fresh._vp.session.final:
        print("VIEWPORT-LIVE-FRESH-ROWS-BAD")
        ok = False
    fresh._vp.session.close()

    scene.bryce_render.live_order = "SCATTERED"
    mid = FakeEngine(module)
    seen = sweep(mid, to_rows)
    after = sorted({z for z, was_after in seen if was_after})
    before = sorted({z for z, was_after in seen if not was_after})
    session_mid = mid._vp.session
    print("VIEWPORT-LIVE-SWITCH-ROWS before=%s after=%s final=%s divisor=%s"
          % (before, after, session_mid.final, session_mid.divisor))
    if not (session_mid.final and session_mid.divisor == 1 and after == [(big_w, big_h)]):
        print("VIEWPORT-LIVE-SWITCH-ROWS-BAD")
        ok = False
    mid._vp.session.close()
    viewport.timer_registrar = lambda function, interval: engine.timers.append(function)
    scene.bryce_render.live_order = "ROWS"

    quiet = types.SimpleNamespace(updates=[])  # a view change alone lists nothing (blender-integration.md 3.3)

    # 2. view_update again with nothing changed in the scene and the same view: no new child.
    version = session.version
    module.engine.BryceRenderEngine.view_update(engine, ctx, quiet)
    if session.busy() or session.version != version:
        print("VIEWPORT-UNCHANGED-RESTART-BAD")
        ok = False

    # 3. a moved view: another picture; a view change while a child runs cancels it.
    moved = view.copy()
    moved[0][3] -= 1.5
    ctx_moved = context_for(bpy, scene, moved, window, "CAMERA")
    module.engine.BryceRenderEngine.view_update(engine, ctx_moved, quiet)
    drive(viewport, engine)
    if bytes(session.levels) == first or not session.final:
        print("VIEWPORT-MOVE-BAD")
        ok = False
    module.engine.BryceRenderEngine.view_update(engine, ctx, quiet)  # back to the first view: starts a child
    deadline = time.time() + 10
    while session._active is None and time.time() < deadline:
        time.sleep(0.02)
        engine.timers[0]()
    server = session._server
    running = session._active
    changed = session.set_view(viewport.view_state(engine, ctx_moved))
    cancelled = running is not None and session._active is None  # the request in flight is let go at once
    print("VIEWPORT-CANCEL changed=%s request_sent=%s let_go_at_once=%s" % (changed, running is not None, cancelled))
    if running is None or not cancelled or not changed:
        print("VIEWPORT-CANCEL-BAD")
        ok = False
    drive(viewport, engine)
    if not session.final or session._server is not server:  # one renderer served the cancelled and the newer request
        print("VIEWPORT-AFTER-CANCEL-BAD")
        ok = False

    # 4. the pixel size: half the size across gives a picture of half the size.
    scene.bryce_render.viewport_pixel_size = "2"
    module.engine.BryceRenderEngine.view_update(engine, ctx, quiet)
    drive(viewport, engine)
    print("VIEWPORT-PIXEL-SIZE-2 picture %dx%d" % session.size)
    if session.size != (WIDTH // 2, HEIGHT // 2) or not session.final:
        print("VIEWPORT-PIXEL-SIZE-BAD")
        ok = False
    scene.bryce_render.viewport_pixel_size = "1"

    # 5. the quality: DRAFT draws a picture of the same size (the levels may differ from RENDER's).
    scene.bryce_render.viewport_quality = "DRAFT"
    module.engine.BryceRenderEngine.view_update(engine, ctx, quiet)
    drive(viewport, engine)
    print("VIEWPORT-DRAFT final=%s size=%s message=%r" % (session.final, session.size, session.message))
    if not session.final or session.size != (WIDTH, HEIGHT):
        print("VIEWPORT-DRAFT-BAD")
        ok = False
    scene.bryce_render.viewport_quality = "RENDER"

    # 6. an orthographic view with program 5.0: a message and no new child.
    scene.bryce_render.program = "5.0"
    ortho = context_for(bpy, scene, view, window, "ORTHO")

    class SceneChanged:
        """The real depsgraph, listing the scene's change as Blender does after a setting is written."""
        updates = [scene]

        def __getattr__(self, name):
            return getattr(bpy.context.evaluated_depsgraph_get(), name)

    module.engine.BryceRenderEngine.view_update(engine, ortho, SceneChanged())
    time.sleep(0.3)
    drive(viewport, engine)
    print("VIEWPORT-ORTHO message=%r" % session.message)
    if "orthographic" not in session.message:
        print("VIEWPORT-ORTHO-BAD")
        ok = False

    # 7. view_draw: the update half runs without gpu; the drawing needs a GPU context.
    try:
        module.engine.BryceRenderEngine.view_draw(engine, ctx, depsgraph)
        print("VIEWPORT-DRAW-RAN (gpu worked headlessly; the drawing itself still needs a window to be seen)")
    except Exception as error:  # background mode has no gpu module or no context
        print("VIEWPORT-DRAW-NOT-CHECKED (%s: %s); the texture and drawing code needs a Blender with a window"
              % (type(error).__name__, error))

    # 8. the scene's colour settings: the colour-management panels are the engine's, the viewport sets Raw / None / 0
    # once from a one-shot timer (not in the update or the draw), reports it, and leaves a later change alone; an
    # import sets them too.
    names_ = module.names
    panel_names = [n for n in dir(bpy.types) if n.startswith("RENDER_PT_color_management") and hasattr(getattr(bpy.types, n), "COMPAT_ENGINES")]
    if not panel_names or not all(names_.ENGINE_ID in getattr(bpy.types, n).COMPAT_ENGINES for n in panel_names):
        print("VIEWPORT-COLOUR-PANELS-BAD", panel_names)
        ok = False
    module.engine.unregister()
    gone = [n for n in panel_names if names_.ENGINE_ID in getattr(bpy.types, n).COMPAT_ENGINES]
    module.engine.register()
    if gone:
        print("VIEWPORT-COLOUR-PANELS-UNREGISTER-BAD", gone)
        ok = False
    scene.view_settings.view_transform = "AgX"
    scene.render.dither_intensity = 1.0
    once = []
    viewport.colour_registrar = lambda function, interval: once.append(function)
    colour_engine = FakeEngine(module)
    ctx = context_for(bpy, scene, view, window, "CAMERA")
    module.engine.BryceRenderEngine.view_update(colour_engine, ctx, depsgraph)
    if scene.view_settings.view_transform != "AgX" or len(once) != 1:
        print("VIEWPORT-COLOUR-SET-IN-UPDATE-BAD", scene.view_settings.view_transform, len(once))
        ok = False
    redraws = colour_engine.redraws
    once[0]()
    plain = (scene.view_settings.view_transform, scene.view_settings.look, scene.render.dither_intensity)
    said = [r for r in colour_engine.reports if "view transform Raw" in r and "unchanged" in r]
    if plain != ("Raw", "None", 0.0) or len(said) != 1 or colour_engine.redraws != redraws + 1:
        print("VIEWPORT-COLOUR-SET-BAD", plain, colour_engine.reports)
        ok = False
    scene.view_settings.view_transform = "Standard"  # the user's own choice afterwards
    module.engine.BryceRenderEngine.view_update(colour_engine, ctx, depsgraph)
    if len(once) != 1 or scene.view_settings.view_transform != "Standard":
        print("VIEWPORT-COLOUR-FOUGHT-USER-BAD", len(once))
        ok = False
    colour_engine._vp.session.close()
    scene.view_settings.view_transform = "AgX"
    scene.render.dither_intensity = 1.0
    document = module.import_op
    plan = {"pictures": [], "materials": [], "program": "5.0", "render": {}, "mark_whole": False, "objects": [],
            "sky": None, "world": None}
    try:
        document.carry_out(types.SimpleNamespace(scene=scene), "/tmp/x.br5", plan)
    except Exception as error:
        print("VIEWPORT-COLOUR-IMPORT-ERROR %s: %s" % (type(error).__name__, error))
    if (scene.view_settings.view_transform, scene.render.dither_intensity) != ("Raw", 0.0):
        print("VIEWPORT-COLOUR-IMPORT-BAD")
        ok = False
    print("VIEWPORT-COLOUR-CHECKED")

    engine._vp.session.close()
    print("VIEWPORT-DONE" if ok else "VIEWPORT-FAILED")


main()
