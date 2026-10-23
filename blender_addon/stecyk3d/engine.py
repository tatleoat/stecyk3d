# SPDX-License-Identifier: GPL-3.0-or-later
# (the render engine STECYK3D; refusing without a substrate; size from resolution and percentage;
# progress; thumbnails; temporary files removed),,
# exact_png, viewport_pixel_size
#       and viewport_quality (in viewport.py)
# (The render engine and preferences; the live picture;
#       needs Blender, not testable here)
"""The render engine and the add-on's preferences."""

import json
import os
import subprocess
import sys
import tempfile
import time

import bpy

from . import gather, names, refusal_report, stream, viewport

SUBSTRATE_VARIABLE = "BRYCE_SUBSTRATE"

RECORD_MAGIC = stream.RECORD_MAGIC  # kept under the old names (the live picture)
RECORD_HEAD = stream.RECORD_HEAD
LivePicture = stream.LivePicture  # moved to stream.py (pure, tested without bpy)


LAST_MAKE = {}  # the last Make substrate: {"seconds", "lines" (what it found, per folder and file), "error"}


class Stecyk3DPreferences(bpy.types.AddonPreferences):
    bl_idname = __package__
    # The one thing the user gives: where each Bryce version is installed (each optional, any subset). The
    # extractor finds the program files and data folders inside, whatever the letter case, and a wrapper folder.
    bryce_30: bpy.props.StringProperty(name="Bryce 3.0 folder", subtype="DIR_PATH",
                                       description="Your Bryce 3.0 install folder (optional)")
    bryce_31: bpy.props.StringProperty(name="Bryce 3.1 folder", subtype="DIR_PATH",
                                       description="Your Bryce 3.1 install folder (optional)")
    bryce_50: bpy.props.StringProperty(name="Bryce 5.0 folder", subtype="DIR_PATH",
                                       description="Your Bryce 5.0 install folder (optional)")
    # Advanced overrides: empty means "find it yourself".
    substrate: bpy.props.StringProperty(name="Substrate file", subtype="FILE_PATH")
    renderer_program: bpy.props.StringProperty(name="Renderer program", subtype="FILE_PATH")
    extractor_program: bpy.props.StringProperty(name="Extractor program", subtype="FILE_PATH")
    # Read only to migrate a value saved by an older version (not shown): see MakeSubstrate.
    install_folders: bpy.props.StringProperty(options={"HIDDEN"})
    show_advanced: bpy.props.BoolProperty(name="Advanced", default=False)
    # Set once the earlier release's saved values were read (migrate.migrate_preferences), so a cleared field stays cleared.
    earlier_values_read: bpy.props.BoolProperty(options={"HIDDEN"})

    def fields(self):
        return {"3.0": self.bryce_30, "3.1": self.bryce_31, "5.0": self.bryce_50}

    def draw(self, context):
        from . import extract
        layout = self.layout
        # Laid out like the earlier add-on's page: a box for "Your Bryce" (the folders), then the substrate (the
        # button, then what is found, read only), then the closed Advanced subpanel.
        box = layout.box().column()
        box.label(text="Your Bryce", icon="FILE_FOLDER")
        for line in ("The folder of your own installed Bryce 3D 3.0 or 3.1, and of your Bryce 5 if you have it (each optional).",
                     "Make substrate reads a few of Bryce's own tables (its noise, its leaf, its dither) out of them once, and keeps",
                     "a copy of the files it reads after (the moon, Bryce 5's stars and leaves, the preset libraries) beside them.",
                     "Bryce 3D gives what Stecyk3D's renders as 3.0 and 3.1 need; Bryce 5 what renders as 5.0 need besides.",
                     "After Make substrate, Bryce may be uninstalled."):
            box.label(text=line)
        for field in ("bryce_30", "bryce_31", "bryce_50"):
            version = {"bryce_30": "3.0", "bryce_31": "3.1", "bryce_50": "5.0"}[field]
            box.prop(self, field)
            ok, text = extract.kind_mark(version, bpy.path.abspath(getattr(self, field)))
            row = box.row()
            row.label(text="    " + text, icon="BLANK1" if ok is None else "CHECKMARK" if ok else "ERROR")
        if self.install_folders.strip() and not any(self.fields().values()):
            box.label(text="An older list of install folders is saved; Make substrate moves it into the fields.",
                      icon="INFO")
        layout.label(text="Bryce's own tables (the substrate)", icon="FILE")
        button = layout.row()
        button.scale_y = 1.4
        button.operator(MakeSubstrate.bl_idname, icon="IMPORT")
        path, message = find_substrate(self)
        layout.label(text=("Substrate: " + path) if path else "Substrate: not made yet", icon="CHECKMARK" if path else "ERROR")
        if LAST_MAKE:
            col = layout.box().column()
            if LAST_MAKE.get("error"):
                col.label(text="Last Make substrate: failed (%.2f s)" % LAST_MAKE["seconds"], icon="ERROR")
                for line in LAST_MAKE["error"]:
                    col.label(text="    " + line)
            else:
                col.label(text="Last Make substrate: done in %.2f s" % LAST_MAKE["seconds"], icon="INFO")
                for line in LAST_MAKE["lines"]:
                    col.label(text="    " + line, icon="CHECKMARK")
        for label, name, override in (("Renderer", extract.RENDERER_NAME, self.renderer_program),
                                      ("Extractor", extract.EXTRACTOR_NAME, self.extractor_program)):
            found, _ = extract.search_program(name, bpy.path.abspath(override) if override else "")
            row = layout.row()
            row.label(text="%s: %s" % (label, found or "not found (see Advanced)"), icon="CHECKMARK" if found else "ERROR")
        if hasattr(layout, "panel"):  # Blender 4.1 and later: a real subpanel, closed at first
            header, body = layout.panel("stecyk3d_advanced", default_closed=True)
            header.label(text="Advanced")
        else:
            layout.prop(self, "show_advanced", toggle=True)
            body = layout.column() if self.show_advanced else None
        if body is not None:
            body.label(text="Leave these empty unless the add-on cannot find the programs or the substrate itself.")
            body.prop(self, "substrate")
            body.prop(self, "renderer_program")
            body.prop(self, "extractor_program")


class MakeSubstrate(bpy.types.Operator):
    """Make the substrate from the Bryce folders set in the preferences, and use it from then on """
    bl_idname = "stecyk3d.make_substrate"
    bl_label = "Make substrate"

    def execute(self, context):
        from . import extract
        prefs = preferences(context)
        if prefs.install_folders.strip():  # an older saved list moves into the empty fields, once
            moved = extract.migrate_install_folders(prefs.install_folders, prefs.fields())
            prefs.bryce_30, prefs.bryce_31, prefs.bryce_50 = moved["3.0"], moved["3.1"], moved["5.0"]
            prefs.install_folders = ""
        fields = {v: bpy.path.abspath(f) for v, f in prefs.fields().items() if f}
        program, tried = extract.search_program(
            extract.EXTRACTOR_NAME, bpy.path.abspath(prefs.extractor_program) if prefs.extractor_program else "")
        stop = extract.problems(program, fields)
        if stop:
            self.report({"ERROR"}, "Substrate not made: " + "; ".join(stop))
            return {"CANCELLED"}
        out = substrate_target(prefs)
        if not out:
            self.report({"ERROR"}, "Substrate not made: the default place cannot be told here; name a substrate "
                                   "file under Advanced")
            return {"CANCELLED"}
        os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
        started = time.monotonic()
        run = subprocess.run(extract.extract_command(program, out, fields), capture_output=True, text=True)
        LAST_MAKE.clear()
        LAST_MAKE["seconds"] = time.monotonic() - started
        LAST_MAKE["error"] = extract.failure_lines(run.stderr) or ["the extractor failed"] if run.returncode != 0 else []
        LAST_MAKE["lines"] = extract.success_lines(extract.fields_set(fields), run.stdout) + extract.file_lines(run.stdout)
        if run.returncode != 0:
            self.report({"ERROR"}, "Substrate not made: " + "; ".join(extract.failure_lines(run.stderr) or
                                                                        ["the extractor failed"]))
            return {"CANCELLED"}
        pairs = extract.fields_set(fields)
        self.report({"INFO"}, "Substrate made at %s. %s" % (out, "; ".join(extract.success_lines(pairs, run.stdout))))
        return {"FINISHED"}


def preferences(context):
    return context.preferences.addons[__package__].preferences


DEFAULT_FILE = "substrate.bin"
PROGRAM_FOLDER = "stecyk3d"  # : the folder named after the program, the name the renderer calls itself


def default_substrate_path():
    """: substrate.bin in a folder named after the program, in the
    platform's folder for each user's application data: on Windows the folder APPDATA names, on macOS
    ~/Library/Application Support, elsewhere the folder XDG_DATA_HOME names, or ~/.local/share where that
    variable is not set or is empty. "" where the folder cannot be told."""
    env = os.environ
    home = env.get("HOME", "")
    if sys.platform.startswith("win"):
        base = env.get("APPDATA", "")
    elif sys.platform == "darwin":
        base = os.path.join(home, "Library", "Application Support") if home else ""
    else:
        base = env.get("XDG_DATA_HOME", "") or (os.path.join(home, ".local", "share") if home else "")
    return os.path.join(base, PROGRAM_FOLDER, DEFAULT_FILE) if base else ""


def earlier_substrate_path():
    """The substrate an earlier release made in its own default folder, when this release's place has none yet."""
    from . import migrate
    path = default_substrate_path()
    if not path:
        return ""
    earlier = os.path.join(os.path.dirname(os.path.dirname(path)), migrate.OLD_PROGRAM_FOLDER, DEFAULT_FILE)
    return earlier if os.path.isfile(earlier) and not os.path.isfile(path) else ""


def substrate_target(prefs):
    """Where Make substrate writes, the same place find_substrate reads: the override, else BRYCE_SUBSTRATE,
    else's default place."""
    if prefs.substrate:
        return bpy.path.abspath(prefs.substrate)
    return os.environ.get(SUBSTRATE_VARIABLE, "") or default_substrate_path()


def find_program(prefs, which):
    """(path, message): the renderer or extractor (which = 'renderer'|'extractor'): the Advanced override, else
    the search of extract.search_program. message is "" when found."""
    from . import extract
    name = extract.RENDERER_NAME if which == "renderer" else extract.EXTRACTOR_NAME
    override = prefs.renderer_program if which == "renderer" else prefs.extractor_program
    path, tried = extract.search_program(name, bpy.path.abspath(override) if override else "")
    return path, ("" if path else extract.program_missing_message(which, name, tried))


def find_substrate(prefs):
    """: the preference, else BRYCE_SUBSTRATE, else substrate.bin at the default place."""
    path = bpy.path.abspath(prefs.substrate) if prefs.substrate else os.environ.get(SUBSTRATE_VARIABLE, "")
    if not path:
        path = earlier_substrate_path() or default_substrate_path()
    if not path:
        return None, ("No substrate file yet. Set your Bryce folders in the Stecyk3D add-on preferences and "
                      "press Make substrate.")
    if not os.path.isfile(path):
        return None, ("No substrate file at %s. Set your Bryce folders in the Stecyk3D add-on preferences "
                      "and press Make substrate." % path)
    return path, None


def wait_for_output(run, streaming, timeout=0.01):
    """Sleep until the renderer's pipe has data (or at most `timeout`), so a quick picture is not held up by a
    fixed pause; without a pipe, wait for the program to end for the same short time."""
    if streaming:
        try:
            import select
            select.select([run.stdout], [], [], timeout)
            return
        except (OSError, ValueError, TypeError):  # Windows pipes, or a closed pipe: fall back to a short sleep
            pass
    try:
        run.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        pass


class BryceRenderEngine(bpy.types.RenderEngine):
    bl_idname = names.ENGINE_ID
    bl_label = "Stecyk3D"  # the name in Blender's list of engines (the product name; the id stays STECYK3D)
    bl_use_preview = True  # : thumbnails by the same renderer
    bl_use_postprocess = False

    def __init__(self, *args, **kwargs):  # 1.2 (a subclass's __init__)
        super().__init__(*args, **kwargs)
        self._run = None  # the renderer program of the render now running, if any
        self._vp = None  # the Rendered viewport's state (viewport.py), made at the first view_update

    def stop_previous(self):
        """Task T2: a new render ends the program of the render it replaces — Blender starts
        one whenever the scene or the view changes while the old one is still drawing."""
        run = getattr(self, "_run", None)
        if run is not None and run.poll() is None:
            run.terminate()
            try:
                run.wait(timeout=5)
            except subprocess.TimeoutExpired:
                run.kill()
                run.wait()
        self._run = None

    def view_update(self, context, depsgraph):  # 3.1
        viewport.view_update(self, context, depsgraph, self.resolve_program, gather.describe)

    def view_draw(self, context, depsgraph):  # 3.1, 3.4
        viewport.view_draw(self, context, depsgraph)

    def resolve_program(self):
        """(renderer program, substrate, message): the preferences' paths, or why the render cannot be made."""
        prefs = preferences(bpy.context)
        substrate, message = find_substrate(prefs)
        if substrate is None:
            return None, None, message
        program, missing = find_program(prefs, "renderer")
        if not program:
            return None, None, "Not rendered: " + missing
        return program, substrate, ""

    def __del__(self):  # 1.2 and 5.2: stop the work when the engine goes
        try:
            viewport.close(self)
            self.stop_previous()
        except Exception:
            pass

    def refresh(self, result, scene):
        """Tell Blender the result's pixels have changed (2.3)."""
        try:
            self.update_result(result)
        except TypeError:  # a Blender that asks where and how (result, rect, mode, scene)
            self.update_result(result, list(result.rect), "REPLACE", scene)

    def cancel_result(self, result):
        """Close a result that will not be made final (2.3: do not merge it into the picture)."""
        try:
            self.end_result(result, cancel=True)
        except TypeError:
            self.end_result(result)

    def refuse(self, head, *texts):
        """A refused render: each problem on its own line in the Info editor and in the text "Stecyk3D report"
        (made or replaced), then one short line for the status bar that says where the list is."""
        lines = refusal_report.lines_of(*texts)
        try:
            text = bpy.data.texts.get(refusal_report.REPORT_TEXT) or bpy.data.texts.new(refusal_report.REPORT_TEXT)
            text.clear()
            text.write(refusal_report.text_of(head, lines))
        except Exception:  # a Blender that will not make a text here still gets the Info lines
            pass
        if len(lines) > 1:
            for line in lines:
                self.report({"INFO"}, line)
        self.report({"ERROR"}, refusal_report.summary(head, lines))

    def render(self, depsgraph):
        scene = depsgraph.scene
        scale = scene.render.resolution_percentage / 100.0
        width = max(1, int(scene.render.resolution_x * scale))  # Limits (from 1 by 1)
        height = max(1, int(scene.render.resolution_y * scale))

        prefs = preferences(bpy.context)
        substrate, message = find_substrate(prefs)
        if substrate is None:
            self.report({"ERROR"}, message)
            return
        try:
            notices = []
            description, problems = gather.describe(depsgraph, notices)
            for notice in notices:
                self.report({"WARNING"}, notice)
        except ValueError as error:  # convert refuses a named thing (a zero direction, Errors)
            self.report({"ERROR"}, "Not rendered: %s" % error)
            return
        if problems:
            self.refuse("Not rendered", problems)
            return
        program, missing = find_program(prefs, "renderer")
        if not program:
            self.report({"ERROR"}, "Not rendered: " + missing)
            return

        # The live picture (task T2, the default for the viewport and F12): the renderer is
        # asked for `--stream --scattered`, sends every finished pixel down the pipe as it
        # draws, in a random order so the picture develops out of noise, and the result is
        # refreshed as the pixels come. The finished picture is the exact render's either
        # way. Thumbnails (the material previews) keep the plain path: they are quick and
        # the preview windows are small. (the live picture)
        group = getattr(scene, names.SCENE_RENDER_GROUP, None)
        progressive = group is not None and bool(getattr(group, "progressive", True)) and not self.is_preview
        self.stop_previous()  # a new render ends the old render's program (task T2)

        with tempfile.TemporaryDirectory(prefix="stecyk3d_") as folder:  # Effects: removed afterwards
            desc_path = os.path.join(folder, "scene.json")
            out_path = os.path.join(folder, "levels.rgb")
            description["size"] = [width, height]
            with open(desc_path, "w", encoding="utf-8") as f:
                json.dump(description, f)
            command = [program, "--substrate", substrate, "--description", desc_path, "--levels", out_path]
            command += stream.grain_flags(getattr(group, "wash_grain", "AUTO"))
            if progressive:
                command += stream.live_flags(getattr(group, "live_order", "ROWS"))
            run = subprocess.Popen(command, stdout=(subprocess.PIPE if progressive else None),
                                   stderr=subprocess.PIPE, text=False)
            self._run = run
            live = None
            result = None
            if progressive:
                os.set_blocking(run.stdout.fileno(), False)  # 5.1
                live = LivePicture(width, height)
                self.plain_colour(scene)  # before the first pixels, so the live view is the final one's colour
                result = self.begin_result(0, 0, width, height)
                last_paint = time.time() - 1.0
            cancelled = False
            while True:
                if self.test_break():
                    cancelled = True
                    break
                got_data = False
                if live is not None:
                    # Read everything the pipe holds now (up to a bounded number of reads so progress and
                    # cancel are still looked at), without sleeping while there is more: sleeping 50 ms
                    # per 64 KB pipe-full capped F12 at ~1.3 MB/s and cost ~0.9 s at 640x480.
                    for _ in range(64):
                        try:
                            chunk = os.read(run.stdout.fileno(), 1 << 20)
                        except OSError:  # nothing this moment (the pipe is empty, or the end is reached)
                            break
                        if not chunk:
                            break
                        got_data = True
                        live.feed(chunk)
                done = run.poll() is not None
                if live is not None and live.dirty:
                    now = time.time()
                    if now - last_paint >= 0.25 or done:
                        result.layers[0].passes["Combined"].rect.foreach_set(live.pixels)
                        live.dirty = False
                        last_paint = now
                        if not done:
                            self.refresh(result, scene)
                        self.update_progress(live.fraction())
                if done:
                    break
                if got_data:
                    continue  # more may be waiting: no sleep
                wait_for_output(run, live is not None)
            self._run = None
            if cancelled:
                run.terminate()
                run.wait()
                if result is not None:
                    self.cancel_result(result)
                return
            if live is not None:  # the pipe may still hold records the program wrote as it ended
                while True:
                    try:
                        chunk = os.read(run.stdout.fileno(), 1 << 20)
                    except OSError:
                        break  # the writer has ended and the pipe is empty
                    if not chunk:
                        break
                    live.feed(chunk)
            error = (run.stderr.read() or b"").decode("utf-8", "replace").strip()
            if run.returncode != 0:
                if result is not None:
                    self.cancel_result(result)
                self.refuse("The renderer refused", error)
                return
            with open(out_path, "rb") as f:
                levels = f.read()
        if len(levels) != width * height * 3:
            self.report({"ERROR"}, "The renderer gave %d bytes, not %d" % (len(levels), width * height * 3))
            return

        if result is None:
            self.plain_colour(scene)
            result = self.begin_result(0, 0, width, height)
        # The final rectangle from the finished levels, exact (blender-integration.md 2.2 checked: rect has
        # one item per pixel and foreach_set takes w*h*4 floats, the rows from the bottom of the picture up;
        # in Blender 5.2 the flat one-argument form of foreach_set is the one offered).
        pixels = stream.rgba_floats(levels, width, height)  # k/255 as singles, alpha 1, rows from the bottom up
        result.layers[0].passes["Combined"].rect.foreach_set(pixels)
        self.end_result(result)
        self.update_progress(1.0)

        exact_png = scene.bryce_render.exact_png  # exact_png
        if exact_png:
            from .png import write_png
            write_png(bpy.path.abspath(exact_png), width, height, levels)

    def plain_colour(self, scene):
        """Task T1 (colours unchanged): the levels are written as k/255 scene-linear, so they reach an 8-bit
        picture untouched only with the view transform `Raw` and dithering 0 (
        2.6: with those, all 256 levels came back exactly; `AgX` (the factory transform) or dither 1.0 changes
        them). Blender's own note (8): the add-on may tell the user or set the scene's settings. We set them
        and say so, rather than hand back a picture whose numbers are not the renderer's. The user may change
        them again afterwards; `exact_png`  never depends on them."""
        from .colour import make_plain
        changed = make_plain(scene)
        if changed:
            self.report({"INFO"}, "Stecyk3D set the scene's %s so the rendered levels reach the picture "
                                  "unchanged" % ", ".join(changed))


def get_panels():
    """Blender's own panels that make sense with this engine (dimensions, output, and so on)."""
    keep = {"RENDER_PT_format", "RENDER_PT_output", "RENDER_PT_frame_range", "DATA_PT_lens", "DATA_PT_camera",
            "DATA_PT_camera_dof", "DATA_PT_context_light", "MATERIAL_PT_preview", "WORLD_PT_context_world",
            # Color Management, so the user can see and change the view transform the scene is set to
            "RENDER_PT_color_management", "RENDER_PT_color_management_advanced",
            "RENDER_PT_color_management_curves", "RENDER_PT_color_management_white_balance",
            "RENDER_PT_color_management_working_space"}
    return [getattr(bpy.types, name) for name in keep if hasattr(getattr(bpy.types, name, None), "COMPAT_ENGINES")]


def register():
    bpy.utils.register_class(MakeSubstrate)
    bpy.utils.register_class(Stecyk3DPreferences)
    bpy.utils.register_class(BryceRenderEngine)
    for panel in get_panels():
        panel.COMPAT_ENGINES.add(names.ENGINE_ID)


def unregister():
    for panel in get_panels():
        panel.COMPAT_ENGINES.discard(names.ENGINE_ID)
    bpy.utils.unregister_class(BryceRenderEngine)
    bpy.utils.unregister_class(Stecyk3DPreferences)
    bpy.utils.unregister_class(MakeSubstrate)
