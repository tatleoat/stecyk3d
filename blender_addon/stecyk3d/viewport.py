# SPDX-License-Identifier: GPL-3.0-or-later
# viewport_pixel_size and viewport_quality; 3.1 to 3.6, 4.1, 4.2, 5.1, 5.2
# (The Rendered viewport: timer and draw)
"""What the engine's `view_update` and `view_draw` do, as functions of an engine-like object, so that a fake engine
can call them. bpy and gpu are imported only inside the functions that need them (the timer and the drawing), so
this module imports, and its update path runs, without Blender. No thread; the work is a child program."""

import time
import weakref

from . import viewport_camera
from .stream import LIVE_ORDERS, WASH_GRAINS
from .viewport_session import ViewportSession, debug

TIMER_STEP = 0.05  # seconds between timer calls while the session is busy
REDRAW_STEP = 0.1  # seconds between redraws asked for while a picture arrives


class ViewportState:
    """Everything one viewport engine instance keeps (`engine._vp`)."""

    def __init__(self, session):
        self.session = session
        self.timer_on = False
        self.last_redraw = 0.0
        self.problem = ""  # why the scene cannot be drawn (the gathering's refusal), or ""
        self.shown_status = None
        self.texture = None
        self.words = None  # viewport_texture.Words, made at the first draw
        self.texture_version = -1
        self.told_colour = False
        self.colour_seen = set()  # names of the scenes already checked this session (set once, then left alone)
        self.register_once = colour_registrar  # tests give their own
        self.draw_cost = 0.0  # seconds the last texture upload took: redraws are asked for less often when it is long
        self.register_timer = timer_registrar  # tests give their own


def _register_timer(function, interval):
    import bpy
    bpy.app.timers.register(function, first_interval=interval)


timer_registrar = _register_timer
colour_registrar = _register_timer


def plain_colour_later(engine, vp, scene):
    """The first time the viewport starts on a scene that is not on the plain colour settings (colour.py), set them
    from a one-shot timer: Blender does not allow changing scene data inside the draw or the update. Once per scene
    per session; a later change by the user is left alone (the draw only says so, once)."""
    from .colour import is_plain
    name = getattr(scene, "name", None)
    if name is None or name in vp.colour_seen:  # (no name: not a Blender scene)
        return
    vp.colour_seen.add(name)
    if is_plain(scene):
        return
    reference = weakref.ref(engine)

    def once():
        target = reference()
        if target is None:
            return None
        try:
            import bpy
            from .colour import make_plain
            current = bpy.data.scenes.get(name)
            changed = make_plain(current) if current is not None else []
            if changed:
                target.report({"INFO"}, "Stecyk3D set the scene's %s so the viewport shows the renderer's levels "
                                        "unchanged" % ", ".join(changed))
                getattr(target, "_vp").told_colour = True
                target.tag_redraw()
        except Exception as error:  # a courtesy; the draw still tells the user once
            debug("plain colour: %s" % error)
        return None

    vp.register_once(once, 0.0)


def exact_camera_of_view(scene, depsgraph, width, height, perspective, view, window):
    """The description's camera of the scene's own camera when the view is exactly that camera's view: a camera
    view whose matrices are the camera's own as Blender computes them (singles) for this region, and a camera that
    still holds the singles an import gave it (`bryce_exact_camera`). The viewport's camera is otherwise made from
    the view matrices, which Blender holds as singles, so a picture of fine grain (the wash of a Bryce 5 sky, 3.0
    clouds) would be made of other pixels than F12's. None where any of that is not so.
    (The Rendered viewport: the camera)"""
    camera = getattr(scene, "camera", None)
    if depsgraph is None or perspective != "CAMERA" or camera is None or not camera.get("bryce_exact_camera"):
        return None
    try:
        own_view = viewport_camera.matrix_rows(camera.matrix_world.inverted())
        own_window = viewport_camera.matrix_rows(camera.calc_matrix_camera(depsgraph, x=width, y=height))
        if own_view != view or own_window != window:
            return None
        from . import gather  # (bpy: only a Blender session asks)
        problems = []
        from . import names
        group = gather.group_to_dict(getattr(scene, names.SCENE_RENDER_GROUP))
        found = gather.describe_camera(depsgraph, group["units"], problems, group["program"])
        return None if problems else found
    except Exception:
        return None


def view_state(engine, context, depsgraph=None):
    """The view as plain values, or None when there is no 3D region to read (blender-integration.md 3.2)."""
    region = getattr(context, "region", None)
    data = getattr(context, "region_data", None)
    if region is None or data is None:
        return None
    scene = context.scene
    group = getattr(scene, "bryce_render")
    size = viewport_camera.pixel_size(group.viewport_pixel_size, engine.get_preview_pixel_size(scene))
    view, window = viewport_camera.matrix_rows(data.view_matrix), viewport_camera.matrix_rows(data.window_matrix)
    exact = exact_camera_of_view(scene, depsgraph, int(region.width), int(region.height), data.view_perspective,
                                 view, window)
    return viewport_camera.ViewState(
        int(region.width), int(region.height), size, group.viewport_quality, data.view_perspective, view, window, exact)


def _status(vp):
    return vp.problem or vp.session.message


def _show_status(engine, vp):
    text = _status(vp)
    if text != vp.shown_status:
        vp.shown_status = text
        try:
            engine.update_stats("Stecyk3D", text)
        except Exception:  # a status line is a courtesy
            pass


def keep_ticking(engine):
    """Register the timer if it is not running (a function of Blender's main thread: 4.2, 3.6)."""
    vp = engine._vp
    if vp.timer_on:
        return
    vp.timer_on = True
    reference = weakref.ref(engine)

    def step():
        target = reference()
        if target is None:
            return None
        return timer_step(target)

    vp.register_timer(step, TIMER_STEP)


def timer_step(engine):
    """One timer call: move the session; ask for a redraw when the picture changed. Returns the seconds until the
    next call, or None when the session is idle (the timer then stops; a change starts it again)."""
    vp = getattr(engine, "_vp", None)
    if vp is None:
        return None
    changed = vp.session.tick()
    now = time.monotonic()
    busy = vp.session.busy()
    if changed and (not busy or now - vp.last_redraw >= max(REDRAW_STEP, 3.0 * vp.draw_cost)):
        vp.last_redraw = now
        engine.tag_redraw()
    _show_status(engine, vp)
    if busy:
        return TIMER_STEP
    vp.timer_on = False
    return None


def view_update(engine, context, depsgraph, resolve, describe):
    """Blender's view_update (3.1): read the scene on the main thread, hand the work to the session.

    `resolve()` gives (program path, substrate path, message); `describe(depsgraph, notices, view_camera=True)`
    is gather.describe (a ValueError refuses, as in a final render)."""
    program, substrate, message = resolve()
    debug("view_update (%d updates)" % sum(1 for _ in depsgraph.updates))
    vp = getattr(engine, "_vp", None)
    first = vp is None
    if first:
        vp = engine._vp = ViewportState(ViewportSession(None, time.monotonic))
    if message:
        vp.problem = message
        _show_status(engine, vp)
        return
    vp.session.command = lambda: [program, "--substrate", substrate, "--serve"]
    if first or any(True for _ in depsgraph.updates):  # an orbit lists nothing: no gather for a view alone
        try:
            description, problems = describe(depsgraph, [], view_camera=True)
        except ValueError as error:
            description, problems = None, [str(error)]
        if problems:
            vp.problem = "Not rendered: " + "; ".join(problems)
        else:
            vp.problem = ""
            render = description["render"]
            vp.session.set_scene(description, render["units"], render["version"])
    plain_colour_later(engine, vp, context.scene)
    group = getattr(context.scene, "bryce_render", None)
    order = getattr(group, "live_order", "ROWS")
    vp.session.set_live_order(order if order in LIVE_ORDERS else "ROWS")
    grain = getattr(group, "wash_grain", "AUTO")
    vp.session.set_wash_grain(grain if grain in WASH_GRAINS else "AUTO")
    state = view_state(engine, context, depsgraph)
    if state is not None:
        vp.session.set_view(state)
    _show_status(engine, vp)
    keep_ticking(engine)


def view_draw(engine, context, depsgraph):
    """Blender's view_draw (3.1, 3.4): draw the newest picture; no other work beyond noticing a view change."""
    vp = getattr(engine, "_vp", None)
    if vp is None:
        return
    state = view_state(engine, context, depsgraph)
    if state is not None and vp.session.set_view(state):
        keep_ticking(engine)
    session = vp.session
    if session.levels is None:
        return
    from . import viewport_texture
    scene = context.scene
    if vp.texture is None or vp.texture_version != session.version:
        began = time.monotonic()
        if vp.words is None:
            vp.words = viewport_texture.Words()
        vp.words.sync(session.levels, session.size, session.take_dirty())
        vp.texture = viewport_texture.upload(vp.words)
        vp.texture_version = session.version
        vp.draw_cost = time.monotonic() - began
        debug("texture %dx%d in %.4fs" % (session.size[0], session.size[1], vp.draw_cost))
    if not vp.told_colour and scene.view_settings.view_transform != "Raw":
        vp.told_colour = True
        engine.report({"INFO"}, "Stecyk3D's viewport shows the renderer's levels through the scene's view "
                                "transform; set it to Raw to see them as they are")
    viewport_texture.draw(vp.texture)


def close(engine):
    """Stop the child and remove its folder (5.2); the engine's __del__ calls this."""
    vp = getattr(engine, "_vp", None)
    if vp is not None:
        vp.session.close()
