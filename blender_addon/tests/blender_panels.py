# SPDX-License-Identifier: GPL-3.0-or-later
# (Light gradient keys; Panels). Task T12: a table-driven check that the panels
# draw every setting of (each row of props.RENDER, SKY, LIGHT is drawn by name, 5.0-only rows
# only as 5.0), on real property groups with a recording layout, and that the light's key buttons add and remove.
#   blender -b --factory-startup --python blender_panels.py -- <substrate> <program> <prefix>
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402


class Recorder:
    """A layout that records what is drawn: property names and operator calls (nested boxes share one record)."""

    def __init__(self, drawn=None):
        self.drawn = drawn if drawn is not None else {"props": [], "labels": [], "ops": []}

    def box(self, **kw):
        return Recorder(self.drawn)

    row = column = box

    def grid_flow(self, **kw):
        return Recorder(self.drawn)

    def prop(self, data, key, **kw):
        self.drawn["props"].append(key)

    def label(self, text="", **kw):
        self.drawn["labels"].append(text)

    def separator(self, **kw):
        pass

    def operator(self, idname, **kw):
        self.drawn["ops"].append(idname)
        self.drawn.setdefault("opkw", []).append((idname, kw))
        return type("Op", (), {})()


def shown(drawn):
    """The names a draw shows: its properties and the headings of nested groups and lists (`name` or `name: n`)."""
    return set(drawn["props"]) | {t.split(":")[0] for t in drawn["labels"]}


def check(module, label, group, table, five_only, extra_drawn=()):
    panels = module.panels
    skipped = panels.TEXT_SETTINGS
    for five in (True, False):
        rec = Recorder()
        panels.draw_group(rec, group, five, five_only, editable_lists=panels.LIGHT_LISTS)
        names_shown = shown(rec.drawn)
        missing = [name for name, _kind, _extra in table
                   if name not in names_shown and name not in skipped and (five or name not in five_only)]
        hidden = [name for name in five_only if not five and name in names_shown]
        assert not missing, "%s (five=%s): rows not drawn: %r" % (label, five, missing)
        assert not hidden, "%s: 5.0-only rows drawn as 3.x: %r" % (label, hidden)
    print("PANEL-ROWS", label, len(table), "rows drawn")


def check_light_panel(module, bpy, scene, names):
    """The Bryce light panel (Data tab) draws without errors for a point, a spot and an area lamp, showing the light's
    class name and, for a spot or an area, the beam settings of the Blender lamp (as the earlier add-on's panel does)."""
    import types
    scene.render.engine = names.ENGINE_ID
    cases = (("POINT", {}, "Radial light", []),
             ("SPOT", {"use_square": False}, "Spotlight", ["spot_size", "use_square"]),
             ("SPOT", {"use_square": True}, "Square spotlight", ["spot_size", "use_square"]),
             ("AREA", {"shape": "DISK"}, "Parallel light", ["shape", "size"]),
             ("AREA", {"shape": "RECTANGLE"}, "Square parallel light", ["shape", "size", "size_y"]),
             ("AREA", {"shape": "ELLIPSE"}, "Parallel light", ["shape", "size", "size_y"]))
    for kind, extra, label, beam in cases:
        lamp = bpy.data.lights.new("panel-" + kind, kind)
        for key, value in extra.items():
            setattr(lamp, key, value)
        rec = Recorder()
        context = types.SimpleNamespace(light=lamp, scene=scene, engine=names.ENGINE_ID)
        module.panels.STECYK3D_PT_light.draw(types.SimpleNamespace(layout=rec), context)
        assert label in rec.drawn["labels"], (kind, extra, rec.drawn["labels"])
        for key in ("spot_size", "use_square", "shape", "size", "size_y"):
            assert (key in rec.drawn["props"]) == (key in beam), (kind, extra, key, rec.drawn["props"])
        assert "color" in rec.drawn["props"], rec.drawn["props"]
        radial = kind == "POINT"
        assert ("reach" in rec.drawn["props"]) == (not radial), (kind, rec.drawn["props"])
        assert rec.drawn["props"].count("edge") == (0 if radial else 2 if label.startswith("Square") else 1), \
            (kind, extra, rec.drawn["props"])
        assert rec.drawn["ops"].count("stecyk3d.light_flag") == 3, rec.drawn["ops"]
    print("PANEL-LIGHT-CLASSES", len(cases), "lamps drawn")
    check_light_flags_and_keys(module, bpy, scene, names)


def check_light_flags_and_keys(module, bpy, scene, names):
    """The flag switches change only their bit of the light's flags; the Light Lab starter and the key remove icons
    change only the key lists; the starter is offered only while both lists are empty."""
    import types
    lamp = bpy.data.lights.new("panel-flags", "SPOT")
    group = module.panels.light_group(lamp)
    group.flags = 0x1100
    other = (group.intensity, tuple(group.color), group.reach, tuple(group.edge), group.gradient_color_flags)
    with bpy.context.temp_override(light=lamp):
        assert bpy.ops.stecyk3d.light_flag(bit=0x800) == {"FINISHED"} and group.flags == 0x1900, group.flags
        assert bpy.ops.stecyk3d.light_flag(bit=0x100) == {"FINISHED"} and group.flags == 0x1800, group.flags
        assert bpy.ops.stecyk3d.light_flag(bit=0x800) == {"FINISHED"} and group.flags == 0x1000, group.flags
        assert bpy.ops.stecyk3d.light_flag(bit=0x4) == {"CANCELLED"} and group.flags == 0x1000
    check_gradient_switches(module, bpy, group, lamp)
    rec = Recorder()
    module.panels.draw_light(rec, group, True, lamp)
    pressed = {kw["text"]: kw["depress"] for name, kw in rec.drawn["opkw"] if name == "stecyk3d.light_flag"}
    assert pressed == {"On": False, "No shadows": False, "Visible object": False, "Gradient": False,
                       "Colours: Repeat": False, "Colours: Repeat after the offset": False,
                       "Strengths: Repeat": False, "Strengths: Repeat after the offset": False}, pressed
    assert "stecyk3d.light_lab" in rec.drawn["ops"] and any(t.startswith("No keys") for t in rec.drawn["labels"])
    with bpy.context.temp_override(light=lamp):
        assert bpy.ops.stecyk3d.light_lab() == {"FINISHED"}
    assert len(group.color_keys) == 2 and len(group.value_keys) == 2 and group.flags == 0x1000
    assert tuple(group.color_keys[0].color) == (1.0, 1.0, 1.0) and group.color_keys[1].position == 1.0
    assert (group.intensity, tuple(group.color), group.reach, tuple(group.edge), group.gradient_color_flags) == other
    rec = Recorder()
    module.panels.draw_light(rec, group, True, lamp)
    assert "stecyk3d.light_lab" not in rec.drawn["ops"], rec.drawn["ops"]
    removes = [kw for name, kw in rec.drawn["opkw"] if name == "stecyk3d.list_edit" and kw.get("icon") == "X"]
    assert len(removes) == 4 and all(kw["text"] == "" for kw in removes), removes
    with bpy.context.temp_override(light=lamp):
        assert bpy.ops.stecyk3d.list_edit(name="value_keys", action="REMOVE", index=0) == {"FINISHED"}
    assert len(group.color_keys) == 2 and len(group.value_keys) == 1
    rec = Recorder()
    module.panels.draw_light(rec, group, True, lamp)
    assert any(t.startswith("One key says nothing") for t in rec.drawn["labels"])
    print("PANEL-LIGHT-FLAGS-KEYS ok")


def check_gradient_switches(module, bpy, group, lamp):
    """The Gradient switch (bit 0x200000 of flags) and the Repeat switches (bits 2 and 1 of each key list's flag word)
    change only their own bit and word; they are drawn pressed to match, and the plain Bryce 3D panel has no Gradient."""
    words = ("flags", "gradient_color_flags", "gradient_value_flags")
    group.flags, group.gradient_color_flags, group.gradient_value_flags = 0x1000, 0, 0
    with bpy.context.temp_override(light=lamp):
        for word, bit in (("flags", 0x200000), ("gradient_color_flags", 2), ("gradient_color_flags", 1),
                          ("gradient_value_flags", 2), ("gradient_value_flags", 1)):
            before = {w: getattr(group, w) for w in words}
            assert bpy.ops.stecyk3d.light_flag(bit=bit, word=word) == {"FINISHED"}
            for w in words:
                assert getattr(group, w) == (before[w] ^ bit if w == word else before[w]), (word, bit, w)
            rec = Recorder()
            module.panels.draw_light(rec, group, True, lamp)
            on = [kw["text"] for name, kw in rec.drawn["opkw"] if name == "stecyk3d.light_flag" and kw["depress"]]
            assert len(on) == 1, (word, bit, on)
            assert bpy.ops.stecyk3d.light_flag(bit=bit, word=word) == {"FINISHED"}
            assert {w: getattr(group, w) for w in words} == before
        assert bpy.ops.stecyk3d.light_flag(bit=4, word="gradient_value_flags") == {"CANCELLED"}
        assert bpy.ops.stecyk3d.light_flag(bit=2, word="intensity") == {"CANCELLED"}
        group.gradient_color_flags = 3
        group.gradient_value_flags = 1
    rec = Recorder()
    module.panels.draw_light(rec, group, True, lamp)
    pressed = {kw["text"]: kw["depress"] for name, kw in rec.drawn["opkw"] if name == "stecyk3d.light_flag"}
    assert pressed["Colours: Repeat"] and pressed["Colours: Repeat after the offset"], pressed
    assert not pressed["Strengths: Repeat"] and pressed["Strengths: Repeat after the offset"], pressed
    rec = Recorder()
    module.panels.draw_light(rec, group, False, lamp)
    assert "Gradient" not in [kw["text"] for name, kw in rec.drawn["opkw"] if name == "stecyk3d.light_flag"]
    group.gradient_color_flags = group.gradient_value_flags = 0
    group.flags = 0x1000
    print("PANEL-GRADIENT-SWITCHES ok")


def check_preferences_page(module, bpy):
    """The preferences page: a kind mark under each folder field, and the last Make substrate's timing and lines."""
    import tempfile
    from unittest import mock
    engine = sys.modules[module.__name__ + ".engine"]
    prefs = bpy.context.preferences.addons[module.__name__].preferences
    work = tempfile.mkdtemp(prefix="bb_prefpage_")
    for sub, exe in (("b3", "Bryce 3D.exe"), ("b5", "BRYCE5.EXE"), ("none", "x.txt")):
        os.makedirs(os.path.join(work, sub))
        open(os.path.join(work, sub, exe), "w").close()

    def drawn():
        layout = mock.MagicMock()
        layout.panel.return_value = (mock.MagicMock(), mock.MagicMock())
        box = layout.box.return_value.column.return_value
        row = box.row.return_value
        with mock.patch.object(type(prefs), "layout", layout, create=True):
            type(prefs).draw(prefs, bpy.context)
        marks = [(c.kwargs["text"].strip(), c.kwargs["icon"]) for c in row.label.call_args_list]
        texts = [c.kwargs.get("text", "") for c in box.label.call_args_list]
        return marks, texts, layout.box.return_value.column.return_value

    prefs.bryce_30, prefs.bryce_31, prefs.bryce_50 = (os.path.join(work, "b3"), os.path.join(work, "b5"), "")
    marks, texts, _ = drawn()
    assert marks[0] == ("looks like a Bryce 3D 3.0 install", "CHECKMARK"), marks
    assert marks[1] == ("looks like Bryce 5.0, not 3.1", "ERROR"), marks
    assert marks[2][1] == "BLANK1", marks
    assert any("Bryce may be uninstalled" in t or "After Make substrate" in t for t in texts), texts
    prefs.bryce_30, prefs.bryce_50 = os.path.join(work, "none"), os.path.join(work, "b5")
    marks, _, _ = drawn()
    assert marks[0] == ("no Bryce program file here", "ERROR") and marks[2][1] == "CHECKMARK", marks
    prefs.bryce_30 = prefs.bryce_31 = prefs.bryce_50 = ""

    engine.LAST_MAKE.clear()
    engine.LAST_MAKE.update(seconds=1.25, error=[], lines=["Bryce 3.0: Bryce 3D 3.0 recognised, 21 kept files",
                                                          "Bryce 3D.exe: found perm"])
    _, _, col = drawn()
    said = [c.kwargs["text"] for c in col.label.call_args_list]
    assert "Last Make substrate: done in 1.25 s" in said and any("Bryce 3D.exe: found perm" in t for t in said), said
    engine.LAST_MAKE.clear()


def check_camera_panel(module, bpy, scene, names):
    """The Bryce camera panel (Data tab) draws a perspective, an orthographic, a panoramic and an unsupported camera
    with the earlier add-on's notes and the camera settings ours reads."""
    import types
    scene.render.engine = names.ENGINE_ID
    cases = (("PERSP", "Perspective:", ["lens", "shift_x", "shift_y"]),
             ("ORTHO", "Bryce's parallel projection", ["ortho_scale", "shift_x", "shift_y"]),
             ("PANO", "Bryce's panorama", []),
             ("CUSTOM", "No counterpart in Bryce", []))
    drawn = 0
    for kind, note, wanted in cases:
        cam = bpy.data.cameras.new("panel-" + kind)
        try:
            cam.type = kind
        except TypeError:
            continue
        rec = Recorder()
        context = types.SimpleNamespace(camera=cam, scene=scene, engine=names.ENGINE_ID)
        module.panels.STECYK3D_PT_camera.draw(types.SimpleNamespace(layout=rec), context)
        assert any(t.startswith(note) for t in rec.drawn["labels"]), (kind, rec.drawn["labels"])
        assert rec.drawn["props"] == wanted, (kind, rec.drawn["props"])
        drawn += 1
    print("PANEL-CAMERA", drawn, "cameras drawn")


def check_render_panel(module, bpy, scene, names, good_substrate):
    """The Render tab panel: Bryce 5 flag switches (and the Premium effects) only as 5.0, drawn as buttons over the
    bits of the sky's scene flag word; the substrate box names the tables it holds, or the refusal with no substrate."""
    import types
    prefs = bpy.context.preferences.addons[blender_common.ADDON].preferences
    keep = prefs.substrate
    group = getattr(scene, names.RENDER_GROUP if hasattr(names, "RENDER_GROUP") else names.SCENE_RENDER_GROUP)
    sky = module.gather.sky_group(scene.world)
    panels = module.panels
    context = types.SimpleNamespace(scene=scene, engine=names.ENGINE_ID, preferences=bpy.context.preferences)
    counts = 0
    try:
        for substrate in (good_substrate, "/nonexistent/substrate.bin"):
            prefs.substrate = substrate
            module.node_menus.forget()
            for program, antialias in (("5.0", "4"), ("5.0", "1"), ("3.0", "1"), ("3.1", "4")):
                group.program = program
                group.antialias = antialias
                five = program == "5.0"
                for word in (0x23BF, 0x23BF | 0x400000 | 0x2000000):
                    sky.scene_flags = word
                    rec = Recorder()
                    panels.STECYK3D_PT_render.draw(types.SimpleNamespace(layout=rec), context)
                    d = rec.drawn
                    has = good_substrate not in ("x", "") and substrate == good_substrate and os.path.isfile(substrate)
                    assert ("stecyk3d.scene_flag" in d["ops"]) == five, (program, d["ops"])
                    assert ("Bryce 5" in d["labels"]) == five, d["labels"]
                    premium = [t for t in d["labels"] if t.startswith("Premium effects")]
                    assert bool(premium) == five and (not five or premium[0].endswith("effects") == (antialias == "4")), (program, premium)
                    assert d["ops"].count("stecyk3d.scene_flag") == (8 if five else 0), d["ops"]
                    assert ("tir_depth" in d["props"]) == (five and bool(word & 0x400000)), d["props"]
                    assert ("focal_distance" in d["props"]) == (five and bool(word & 0x2000000)), d["props"]
                    assert ("stecyk3d.preferences_show" in d["ops"]) == (not has), d["ops"]
                    tables = [t for t in d["labels"] if t.startswith("Substrate")]
                    assert bool(tables) == has, (has, d["labels"])
                    counts += 1
        # a substrate that holds tables of names: each is named with its count
        real = module.node_menus.current_tables
        module.node_menus.current_tables = lambda: {"noise-names": ["a", "b"], "blend-names": ["c"]}
        try:
            prefs.substrate = good_substrate
            rec = Recorder()
            panels.STECYK3D_PT_render.draw(types.SimpleNamespace(layout=rec), context)
            assert "Substrate: 2 tables of Bryce's names" in rec.drawn["labels"], rec.drawn["labels"]
            assert "noise-names (2)" in rec.drawn["labels"] and "blend-names (1)" in rec.drawn["labels"]
        finally:
            module.node_menus.current_tables = real
        # the switch flips only its bit, and again puts it back
        sky.scene_flags = 0x23BF
        with bpy.context.temp_override(scene=scene):
            assert bpy.ops.stecyk3d.scene_flag(bit=0x2000000) == {"FINISHED"}
            assert sky.scene_flags & 0xFFFFFFFF == 0x23BF | 0x2000000, hex(sky.scene_flags)
            assert bpy.ops.stecyk3d.scene_flag(bit=0x2000000) == {"FINISHED"}
            assert sky.scene_flags == 0x23BF
    finally:
        prefs.substrate = keep
        group.program = "5.0"
        sky.scene_flags = 0x23BF
    print("PANEL-RENDER", counts, "draws")


def check_object_panel(module, bpy, scene, names):
    """The Bryce shape panel (Object tab) draws a torus (radii), a metaball (negative), a tree (three Tree Lab boxes)
    and a terrain, and the stand-in button changes only the object's mesh."""
    import types
    scene.render.engine = names.ENGINE_ID
    scene.bryce_render.program = "5.0"
    panel = module.panels.STECYK3D_PT_object

    def draw(obj):
        rec = Recorder()
        panel.draw(types.SimpleNamespace(layout=rec), types.SimpleNamespace(object=obj, scene=scene, engine=names.ENGINE_ID))
        return rec.drawn

    def make(shape):
        obj = bpy.data.objects.new("panel-" + shape, bpy.data.meshes.new("panel-" + shape))
        scene.collection.objects.link(obj)
        obj.bryce_shape = shape
        return obj

    torus = make("torus")
    assert "bryce_torus" not in draw(torus)["props"] and any("Torus radii" in t for t in draw(torus)["labels"])
    torus["bryce_torus"] = [0.25, 0.75]
    drawn = draw(torus)
    assert '["bryce_torus"]' in drawn["props"] and "stecyk3d.shape_standin" in drawn["ops"], drawn
    meta = make("metaball")
    assert "bryce_negative" in draw(meta)["props"] and "bryce_negative" not in draw(torus)["props"]
    scene.bryce_render.program = "3.0" if "3.0" in [i.identifier for i in scene.bryce_render.bl_rna.properties["program"].enum_items] else scene.bryce_render.program
    assert any(t.startswith("Bryce 5's") for t in draw(meta)["labels"]) == (scene.bryce_render.program != "5.0")
    scene.bryce_render.program = "5.0"
    tree = make("tree")
    drawn = draw(tree)
    for title in ("Trunk and branches", "Shape (distance from the trunk, height)", "Foliage (material slot 2: the leaves')"):
        assert title in drawn["labels"], drawn["labels"]
    for key in ("seed", "thickness", "random", "bottom", "top", "leaf_kind", "leaves", "leaf_size", "leaf"):
        assert key in drawn["props"], (key, drawn["props"])
    assert "grown" not in drawn["props"] and "exact" not in drawn["props"]
    order = drawn["props"]
    assert order.index("thickness") < order.index("bottom") < order.index("leaf_kind")
    terrain_tool = module.terrain_tool
    terr = terrain_tool.make_terrain_object("panel-terrain", 16, [1000 * ((i // 16) + (i % 16)) % 65535 for i in range(256)],
                                            {"grid": "16"}, scene.collection)
    drawn = draw(terr)
    assert "lower" in drawn["props"] and drawn["ops"] == ["stecyk3d.shape_standin"], drawn
    # the buttons: only the mesh changes
    for obj in (torus, meta, tree, terr):
        before = (tuple(obj.location), tuple(obj.scale), obj.bryce_shape, obj.get("bryce_torus") and tuple(obj["bryce_torus"]),
                  len(obj.data.vertices))
        bpy.context.view_layer.objects.active = obj
        assert bpy.ops.stecyk3d.shape_standin() == {"FINISHED"}
        after = (tuple(obj.location), tuple(obj.scale), obj.bryce_shape, obj.get("bryce_torus") and tuple(obj["bryce_torus"]),
                 len(obj.data.vertices))
        assert before[:4] == after[:4] and after[4] > 0, (obj.name, before, after)
    assert len(torus.data.vertices) == 12 * 24, len(torus.data.vertices)  # the radii: ring 0.75 + tube 0.25 reaches 1
    assert abs(max(v.co.x for v in torus.data.vertices) - 1.0) < 1e-6
    assert abs(max(v.co.z for v in terr.data.vertices) - min(v.co.z for v in terr.data.vertices)) > 0
    # a plain mesh with no shape: the button refuses
    plain = bpy.data.objects.new("plain", bpy.data.meshes.new("plain"))
    scene.collection.objects.link(plain)
    bpy.context.view_layer.objects.active = plain
    assert not bpy.ops.stecyk3d.shape_standin.poll()
    print("PANEL-OBJECT 4 shapes drawn")


def check_sky_panel(module, bpy, scene, names):
    """The Bryce sky panel (World tab): the flag-bit switches, the Bryce 5 grids, the New document's sky button, the
    cloud texture boxes and the fault lines; the switches write only their own bit; the button sets only the sky."""
    import types
    panels = module.panels
    scene.render.engine = names.ENGINE_ID
    world = bpy.data.worlds.new("panel-sky")
    world.use_nodes = True
    world.node_tree.nodes.clear()
    node = world.node_tree.nodes.new("BryceSkyNode")
    sky = node.settings
    render = getattr(scene, names.SCENE_RENDER_GROUP)

    def draw():
        rec = Recorder()
        context = types.SimpleNamespace(world=world, scene=scene, engine=names.ENGINE_ID)
        panels.STECYK3D_PT_world.draw(types.SimpleNamespace(layout=rec), context)
        return rec.drawn

    old = render.program
    render.program = "3.0"
    drawn = draw()
    for bit, _l, _d in names.SKY_SWITCHES:
        assert "flag_%x" % bit in drawn["props"], bit
    assert "stecyk3d.sky_new" in drawn["ops"] and not any(k.startswith("sky5_flag") for k in drawn["props"])
    render.program = "5.0"
    sky.sky5 = True
    drawn = draw()
    for prefix, rows in (("sky5_flag_", names.SKY5_SWITCHES), ("sky5_flag2_", names.SKY5_SWITCHES2)):
        for bit, _l, _d in rows:
            assert "%s%x" % (prefix, bit) in drawn["props"], (prefix, bit)
    # the cloud layers: no texture, a switched-on layer is a fault; a texture linked is drawn in a box
    sky.flags = 0x7E0002F3
    faults = panels.sky_faults(world, "5.0")
    assert len(faults) == 2 and "no texture" in faults[0], faults
    assert any("Stratus texture: none" in t for t in drawn["labels"]), drawn["labels"]
    tex = world.node_tree.nodes.new("BryceTextureNode")
    world.node_tree.links.new(tex.outputs["color"], node.inputs["cumulus"])
    drawn = draw()
    assert any(t.startswith("Cumulus texture: ") and "none" not in t for t in drawn["labels"]), drawn["labels"]
    assert len(panels.sky_faults(world, "5.0")) == 1
    sky.flags = 0x7E0002F3 & ~0x20
    assert panels.sky_faults(world, "5.0") == []
    # a switch writes only its bit, including the top one of the word
    before = sky.flags & 0xFFFFFFFF
    sky.flag_80 = not sky.flag_80
    assert (sky.flags & 0xFFFFFFFF) == before ^ 0x80
    sky.sky5_flag_80000000 = not sky.sky5_flag_80000000
    assert (sky.flags & 0xFFFFFFFF) == before ^ 0x80 ^ 0x80000000
    sky.sky5_flag2_2 = True
    assert sky.sky5_flags2 & 2
    # the button: the new document's values for the program, nothing else
    sky.cover, sky.zenith, sky.flags, sky.sky5_flags2, sky.sun_intensity = 0.9, (0.9, 0.1, 0.1), 0, 6, 3.0
    sky.ambient_amount = 0.25
    sun_before = (sky.sun_intensity, sky.ambient_amount)
    context = {"world": world, "scene": scene}
    for program in ("3.0", "5.0"):
        render.program = program
        sky.cover, sky.zenith, sky.flags = 0.9, (0.9, 0.1, 0.1), 0
        sky.sun_intensity, sky.ambient_amount = sun_before
        with bpy.context.temp_override(**context):
            assert bpy.ops.stecyk3d.sky_new() == {"FINISHED"}
        want = module.defaults.program_default("sky", "flags", program)
        assert sky.flags & 0xFFFFFFFF == want, (program, hex(sky.flags))
        assert sky.cover == 0.5 and abs(sky.zenith[0] - 0.2) < 1e-6
        assert (sky.sun_intensity, sky.ambient_amount) == sun_before
        assert world.node_tree.nodes.get(tex.name) is not None and node.inputs["cumulus"].is_linked
    render.program = old
    print("PANEL-SKY", "switches, grids, button, cloud boxes and faults ok")


def check_material_panel(module, bpy, scene, names):
    """The Bryce material panel (Material tab): channel rows with a mix's A, B and by rows, the second material and
    its amount, the copied/left list with Dismiss, and the faults and notes lines."""
    import json
    import types
    scene.render.engine = names.ENGINE_ID
    panel = module.panels.STECYK3D_PT_material

    def draw(material):
        rec = Recorder()
        context = types.SimpleNamespace(material=material, scene=scene, engine=names.ENGINE_ID,
                                        preferences=bpy.context.preferences)
        panel.draw(types.SimpleNamespace(layout=rec), context)
        return rec.drawn

    mat = bpy.data.materials.new("panel-mat")
    mat.use_nodes = True
    tree = mat.node_tree
    tree.nodes.clear()
    one = tree.nodes.new("BryceMaterialNode")
    mix_node = tree.nodes.new("BryceMixColorNode")
    tree.links.new(mix_node.outputs["out"], one.inputs["diffuse_color"])
    # a plain material: constants only, no mix rows, no second material
    plain = bpy.data.materials.new("panel-plain")
    plain.use_nodes = True
    plain.node_tree.nodes.clear()
    plain.node_tree.nodes.new("BryceMaterialNode")
    drawn = draw(plain)
    assert "amount" not in drawn["props"] and "by" not in drawn["props"], drawn["props"]
    assert not any(t.startswith("From a Blender") for t in drawn["labels"])
    # a mix on a channel: the choice, then A and B; "by" only when mixing by a third
    drawn = draw(mat)
    assert "by" in drawn["props"] and drawn["props"].count("value") >= 13 + 1, drawn["props"]
    assert "   by" not in drawn["labels"]
    mix_node.by = "ALPHA"
    with_by = draw(mat)
    assert with_by["props"].count("value") == drawn["props"].count("value") + 1
    # two materials mixed
    two = tree.nodes.new("BryceMaterialNode")
    joined = tree.nodes.new("BryceMixMaterialsNode")
    tree.links.new(one.outputs["material"], joined.inputs["first"])
    tree.links.new(two.outputs["material"], joined.inputs["second"])
    drawn = draw(mat)
    assert "amount" in drawn["props"], drawn["props"]
    assert [t for t in drawn["labels"] if t.startswith(("First: ", "Second: "))] == ["First: " + one.name, "Second: " + two.name], drawn["labels"]
    joined.amount = 2.0
    assert joined.amount == 2.0  # soft limits only: a value written is kept
    # a graph the render refuses shows its fault (here a channel fed by a node that is no Bryce texture or mix)
    tree.links.new(tree.nodes.new("ShaderNodeRGB").outputs[0], two.inputs["diffuse_color"])
    drawn = draw(mat)
    assert any("not a Bryce texture or mix" in t for t in drawn["labels"]), drawn["labels"]
    # the copied/left list and Dismiss
    one.converted = json.dumps({"copied": ["Base colour"], "left": ["Subsurface"]})
    drawn = draw(mat)
    assert "Base colour" in drawn["labels"] and "Subsurface" in drawn["labels"], drawn["labels"]
    assert "stecyk3d.dismiss_converted" in drawn["ops"]
    two.converted = "kept"
    with bpy.context.temp_override(material=mat):
        assert bpy.ops.stecyk3d.dismiss_converted() == {"FINISHED"}
    assert one.converted == "" and two.converted == "", (one.converted, two.converted)
    assert not any(t.startswith("From a Blender") for t in draw(mat)["labels"])
    with bpy.context.temp_override(material=plain):
        assert bpy.ops.stecyk3d.dismiss_converted.poll() is False
    print("PANEL-MATERIAL 5 cases drawn")


def check_texture_editor(module, bpy, scene, names):
    """The texture editor of the Material tab's "Texture components" panel: the shown texture's mapping, its Bryce
    Texture settings and its components with their noise, filter, colours, phase and blend, from the nodes' own
    properties; and a texture with no components."""
    import copy
    import types
    scene.render.engine = names.ENGINE_ID
    preset_ops, material_nodes = module.preset_ops, module.material_nodes

    def draw(material):
        rec = Recorder()
        context = types.SimpleNamespace(material=material, scene=scene, engine=names.ENGINE_ID,
                                        preferences=bpy.context.preferences)
        preset_ops.STECYK3D_PT_presets.draw(types.SimpleNamespace(layout=rec), context)
        return rec.drawn

    def fresh(name):
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        mat.node_tree.nodes.clear()
        return mat

    # a texture with a mapping and two components (a blend, a filter node, a phase noise)
    mat = fresh("editor-mat")
    tree = mat.node_tree
    second = copy.deepcopy(preset_ops.LEAST_TEXTURE["components"][0])
    record = copy.deepcopy(preset_ops.LEAST_TEXTURE)
    record["components"].append(second)
    comp = {"column": 0, "kind": "procedural", "name": "", "record": record,
            "mapping": {"mode": 1, "flags": 0, "scale": [1.0] * 3, "offset": [0.0] * 3, "rotation": [0.0] * 4}}
    texture = material_nodes.add_component_nodes(tree, comp)
    assert texture is not None
    drawn = draw(mat)
    labels = drawn["labels"]
    assert "Bryce Mapping" in labels and "Bryce Texture" in labels, labels
    assert any(t.startswith("Component 1:") for t in labels) and any(t.startswith("Component 2:") for t in labels), labels
    assert any(", blended by " in t for t in labels), labels
    for key in ("mode_menu", "scale", "kind_menu", "method_menu", "count", "flags_menu", "first", "mode_menu"):
        assert key in drawn["props"], (key, drawn["props"])
    assert drawn["props"].count("value") >= 3, drawn["props"]  # the phase amounts
    # a texture with no component linked, and no mapping
    bare = fresh("editor-bare")
    node = bare.node_tree.nodes.new("BryceTextureNode")
    node.column = 0
    drawn = draw(bare)
    assert "Bryce Texture" in drawn["labels"] and "No Bryce Mapping: in object space" in drawn["labels"], drawn["labels"]
    assert not any(t.startswith("Component ") for t in drawn["labels"]), drawn["labels"]
    print("PANEL-TEXTURE-EDITOR 2 cases drawn")


if __name__ == "__main__":
    bpy, _substrate, _program, prefix = blender_common.start()
    module = sys.modules[blender_common.ADDON]
    props, names = module.props, module.names
    scene = bpy.context.scene
    world = scene.world
    check(module, "render", getattr(scene, names.SCENE_RENDER_GROUP), props.RENDER, names.FIVE_ONLY["render"])
    check(module, "sky (World group)", getattr(world, names.DATA_GROUP), props.SKY, names.FIVE_ONLY["sky"])
    world.use_nodes = True
    world.node_tree.nodes.clear()
    sky_node = world.node_tree.nodes.new("BryceSkyNode")
    check(module, "sky (node settings)", module.gather.sky_group(world), props.SKY, names.FIVE_ONLY["sky"])

    light = bpy.data.lights.new("lamp", "POINT")
    group = module.panels.light_group(light)
    for _ in range(2):  # the keys' own rows are drawn too, so give the lists something to draw
        group.color_keys.add()
        group.value_keys.add()
    check(module, "light (group)", group, props.LIGHT, names.FIVE_ONLY["light"])
    rec = Recorder()
    module.panels.draw_group(rec, group, True, names.FIVE_ONLY["light"], editable_lists=module.panels.LIGHT_LISTS)
    for key in ("color", "position", "midpoint", "value"):
        assert key in rec.drawn["props"], "a key's %s is drawn" % key
    assert rec.drawn["ops"].count("stecyk3d.list_edit") == 2 * (2 + 1), rec.drawn["ops"]  # a remove each, one add
    light.use_nodes = True
    light.node_tree.nodes.clear()
    light_node = light.node_tree.nodes.new("BryceLightNode")
    check(module, "light (node settings)", module.panels.light_group(light), props.LIGHT, names.FIVE_ONLY["light"])

    # the buttons: add and remove act on the active light's list
    obj = bpy.data.objects.new("lamp", light)
    scene.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    settings = light_node.settings
    for action, index, expect in (("ADD", 0, 1), ("ADD", 1, 2), ("REMOVE", 0, 1)):
        with bpy.context.temp_override(light=light):
            result = bpy.ops.stecyk3d.list_edit(name="color_keys", action=action, index=index)
        assert result == {"FINISHED"} and len(settings.color_keys) == expect, (action, len(settings.color_keys))
    with bpy.context.temp_override(light=light):
        assert bpy.ops.stecyk3d.list_edit(name="components", action="ADD") == {"CANCELLED"}
    check_light_panel(module, bpy, scene, names)
    check_camera_panel(module, bpy, scene, names)
    check_preferences_page(module, bpy)
    check_render_panel(module, bpy, scene, names, _substrate)
    check_object_panel(module, bpy, scene, names)
    check_sky_panel(module, bpy, scene, names)
    check_material_panel(module, bpy, scene, names)
    check_texture_editor(module, bpy, scene, names)
    print("PANELS-OK")
