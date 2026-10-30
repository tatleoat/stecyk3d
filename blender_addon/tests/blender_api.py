# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). The scripting API (api.py) in headless Blender: the scene, shapes, terrains, materials and presets,
# sky and lights, a render to a relative path, variants in this Blender and in parallel ones, the contact sheet.
#   blender -b --factory-startup --python blender_api.py -- <substrate> <renderer-program> <out-prefix>
# The preset checks need a substrate that keeps Bryce's material libraries (the extractor's Keep libraries); with
# none offered they are skipped and say so. The last line is API-OK.
"""The scripting API, run in Blender."""

import importlib
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402

CHILD = '''
import addon_utils, bpy, importlib, math
ADDON = %r
addon_utils.enable(ADDON, default_set=True)
api = importlib.import_module(ADDON + ".api")
args = api.arguments()
prefs = bpy.context.preferences.addons[ADDON].preferences
prefs.substrate, prefs.renderer_program = args.get("--substrate", ""), args.get("--program", "")
if args.has("--fail"):
    raise RuntimeError("asked to fail")
api.begin("5.0", clear=True)
api.shape("sphere", "Ball", (0, 0, 0), material=api.colour_material("Grey", (0.5, 0.5, 0.5)))
api.sky(sun_intensity=args.get("--sun", 1.0), ambient_color=(0.2, 0.2, 0.2))
cam = bpy.data.objects.new("Camera", bpy.data.cameras.new("Camera"))
bpy.context.scene.collection.objects.link(cam)
bpy.context.scene.camera = cam
cam.location, cam.rotation_euler = (0, -5, 0), (math.pi / 2, 0, 0)
api.render(args.positional[0] + "/" + args.get("--name", "x") + ".png", (32, 24))
'''


def expect_error(kind, words, call, *args, **kw):
    try:
        call(*args, **kw)
    except kind as error:
        assert words in str(error), (words, str(error))
        return
    raise AssertionError("no %s naming %r" % (kind.__name__, words))


def camera(bpy, scene):
    cam = bpy.data.objects.new("Camera", bpy.data.cameras.new("Camera"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    cam.location, cam.rotation_euler = (0.0, -8.0, 1.5), (math.radians(85), 0.0, 0.0)


def check_scene(bpy, api, scene):
    assert api.package_name() == blender_common.ADDON, api.package_name()
    api.begin("3.1", clear=True)
    assert scene.render.engine == "STECYK3D" and scene.bryce_render.program == "3.1"
    assert len(scene.objects) == 0 and len(bpy.data.materials) == 0, list(scene.objects)
    expect_error(ValueError, "program '4.0'", api.begin, "4.0")
    expect_error(ValueError, "a metaball is a shape of the program 5.0", api.shape, "metaball", "M", (0, 0, 0))
    api.begin("5.0", clear=True)
    red = api.colour_material("Red", (0.8, 0.1, 0.1), roughness=0.25, glow=((0.5, 0.2, 0.1), 0.5))
    node = next(n for n in red.node_tree.nodes if n.bl_idname == "BryceMaterialNode")
    assert abs(node.inputs["specularity"].value - 0.75) < 1e-6, node.inputs["specularity"].value
    assert abs(node.inputs["ambience"].value - 0.5) < 1e-6
    for k, kind in enumerate(("plane", "slab", "cube", "sphere", "cylinder", "cone", "pyramid", "torus", "square",
                              "disc", "metaball", "tree")):
        obj = api.shape(kind, "S " + kind, (k, 2.0, 0.5), (0.3, 0.4, 0.5), (0.1, 0.2, 0.3), material=red)
        assert obj.bryce_shape == kind and obj.name == "S " + kind
        assert tuple(round(v, 5) for v in obj.location) == (k, 2.0, 0.5)
        assert tuple(round(v, 5) for v in obj.scale) == (0.3, 0.4, 0.5)
        assert tuple(round(v, 5) for v in obj.rotation_euler) == (0.1, 0.2, 0.3)
        assert obj.data.materials[0] == red and obj.name in scene.collection.objects
        assert (kind == "torus") == ("bryce_torus" in obj.keys())
        if kind == "tree":  # a new tree's wood has no thickness, which the renderer refuses; the later render needs one
            obj.bryce_tree.thickness, obj.bryce_tree.least = 0.026025, 0.005987
    expect_error(ValueError, "not a shape", api.shape, "teapot", "T", (0, 0, 0))
    print("API scene, shapes, colour material: ok")


def check_terrain(bpy, api):
    hill = lambda x, y: 3.0 * math.exp(-((x - 1.0) ** 2 + (y - 20.0) ** 2) / 30.0) - 0.5
    g = api.terrain("Hill", 64, (0.0, 20.0), (24.0, 20.0), (-1.0, 4.0), hill)
    obj = g.object
    bpy.context.view_layer.update()
    assert obj.bryce_shape == "terrain" and obj.bryce_terrain.image is not None
    assert tuple(obj.location) == g.location and tuple(obj.scale) == g.scale  # binary32 both
    # the stand-in's points are the grid's (create.grid_point): z_at gives their height
    worst = 0.0
    for v in obj.data.vertices:
        p = obj.matrix_world @ v.co
        x, y = p.x - 1e-5 * (p.x - g.location[0]), p.y - 1e-5 * (p.y - g.location[1])  # the edges: binary32
        worst = max(worst, abs(g.z_at(x, y) - p.z))
    assert worst < 1e-4, worst
    low = g.lowest_under(1.0, 20.0, 2.0)
    assert low < g.z_at(1.0, 20.0) and abs(low - hill(1.0 + 2.0, 20.0)) < 0.05, low
    print("API terrain: ok (stand-in off z_at by at most %.2g)" % worst)


def check_presets(bpy, api, scene):
    offered = api._libraries(bpy.context)
    if not offered:
        print("API presets: SKIPPED (no material library offered)")
        return
    library = offered[0][0].rsplit(" (", 1)[0]
    lib, _ = api.preset_ops.load(bpy.context, offered[0][1])
    first, second = [n for _, n, _ in api.presets.offered_presets(lib)][:2]
    m = api.preset_material(library, first, channels={"reflection": 0.65, "diffuse_color": (0.04, 0.38, 0.68)})
    node = next(n for n in m.node_tree.nodes if n.bl_idname == "BryceMaterialNode")
    assert m.name == first, m.name
    assert node.inputs["reflection"].value == api.scripting.single(0.65)
    assert [round(c, 6) for c in node.inputs["diffuse_color"].value] == [0.04, 0.38, 0.68]
    # a preset that does not load: the next is taken, and the one that failed leaves nothing behind
    real = api.components.apply_material_preset
    first_index = next(i for i, n, _ in api.presets.offered_presets(lib) if n == first)
    def failing(library_, index):
        if index == first_index:
            raise KeyError("made to fail")
        return real(library_, index)
    api.components.apply_material_preset = failing
    try:
        before = len(bpy.data.materials)
        m2 = api.preset_material(library, first, second)
        assert m2.name.startswith(second) and len(bpy.data.materials) == before + 1
        expect_error(RuntimeError, "no preset of %s loads" % library, api.preset_material, library, first)
    finally:
        api.components.apply_material_preset = real
    expect_error(ValueError, "no material library 'Nowhere'", api.preset_material, "Nowhere", "x")
    expect_error(ValueError, "has no preset 'No such preset'", api.preset_material, library, "No such preset")
    expect_error(ValueError, "'shine' is not a Bryce material channel", api.preset_material, library, first,
                 channels={"shine": 1})
    print("API presets: ok (%s / %s, then %s)" % (library, first, second))


def check_sky_and_lights(bpy, api, scene):
    settings = api.sky(sun_intensity=1.5, flag_8=True, zenith=(0.1, 0.2, 0.3), sky5_star_intensity=10.0)
    node = next(n for n in scene.world.node_tree.nodes if n.bl_idname == "BryceSkyNode")
    assert settings == node.settings and settings.sun_intensity == 1.5 and settings.flag_8
    five = api.defaults
    assert settings.flags % (1 << 32) == five.WORD_SKY_FLAGS_50 | 0x8, hex(settings.flags)  # a new 5.0 sky's
    assert api.sky(haze=0.02).sun_intensity == 1.5  # the node is made once, the settings stay
    expect_error(ValueError, "the sky has no setting 'nonsense'", api.sky, nonsense=1)
    expect_error(ValueError, "sky setting 'cover'", api.sky, cover="thick")
    lamp = api.light("Lamp", "POINT", (1.0, -2.0, 3.0), intensity=2.0, color=(1.0, 0.9, 0.8))
    lnode = next(n for n in lamp.data.node_tree.nodes if n.bl_idname == "BryceLightNode")
    assert lnode.settings.intensity == 2.0 and tuple(lamp.location) == (1.0, -2.0, 3.0)
    assert lnode.settings.flags == five.WORD_LIGHT_FLAGS_50, hex(lnode.settings.flags)
    sun = api.light("Sun", "SUN", direction=(0.0, -1.0, 1.0))
    bpy.context.view_layer.update()
    z = sun.matrix_world.col[2]
    assert abs(z[1] + math.sqrt(0.5)) < 1e-6 and abs(z[2] - math.sqrt(0.5)) < 1e-6, tuple(z)
    expect_error(ValueError, "a sun's light is the sky's", api.light, "Sun 2", "SUN", intensity=1.0)
    expect_error(ValueError, "light 'Bad' has no setting 'shine'", api.light, "Bad", "SPOT", shine=1)
    assert "Bad" not in bpy.data.objects
    api.begin("3.1", clear=False)
    expect_error(ValueError, "'sky5_lit_air' is the program 5.0's", api.sky, sky5_lit_air=0.5)
    expect_error(ValueError, "'radius' is the program 5.0's", api.light, "Wide", "POINT", radius=1.0)
    old = api.light("Old", "SPOT", (0.0, 0.0, 4.0), direction=(0.0, 0.0, 1.0), intensity=1.0)
    onode = next(n for n in old.data.node_tree.nodes if n.bl_idname == "BryceLightNode")
    assert onode.settings.flags == five.WORD_LIGHT_FLAGS_30, hex(onode.settings.flags)
    api.begin("5.0", clear=False)
    print("API sky and lights: ok")


def check_render(bpy, api, scene, prefix):
    camera(bpy, scene)
    here = os.getcwd()
    os.chdir(os.path.dirname(prefix))
    try:
        path = api.render(os.path.basename(prefix) + "-api/single.png", (40, 30))
    finally:
        os.chdir(here)
    assert path == os.path.abspath(prefix + "-api/single.png") and os.path.isfile(path), path
    assert (scene.render.resolution_x, scene.render.resolution_y) == (40, 30)

    built = []
    def build(sun=1.0, **rest):
        assert not rest, rest
        api.begin("5.0", clear=True)
        api.shape("sphere", "Ball", (0, 0, 1), material=api.colour_material("Grey", (0.5, 0.5, 0.5)))
        api.sky(sun_intensity=sun, ambient_color=(0.1, 0.1, 0.1))
        camera(bpy, bpy.context.scene)
        built.append(sun)
    sheet = prefix + "-api/sheet.png"
    paths = api.render_variants(build, {"dim": {"sun": 0.3}, "bright": {"sun": 2.0}}, prefix + "-api", (40, 30),
                                sheet=sheet, columns=2)
    assert built == [0.3, 2.0] and [os.path.basename(p) for p in paths] == ["dim.png", "bright.png"]
    assert all(os.path.isfile(p) for p in paths)
    check_sheet(bpy, api, paths, sheet)
    print("API render, variants, contact sheet: ok")


def pixels(bpy, path):
    import numpy as np
    image = bpy.data.images.load(path, check_existing=False)
    w, h = image.size
    px = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(px)
    bpy.data.images.remove(image)
    return px.reshape(h, w, 4)[::-1]


def check_sheet(bpy, api, paths, sheet):
    s = api.scripting
    scale = s.label_scale(40)
    pad = 3 * scale
    layout = s.sheet_layout([(40, 30)] * 2, 2, s.GLYPH_HEIGHT * scale + 2 * pad, 2 * pad)
    got = pixels(bpy, sheet)
    assert (got.shape[1], got.shape[0]) == layout["size"], (got.shape, layout["size"])
    for path, (x, y) in zip(paths, layout["pictures"]):
        assert (got[y:y + 30, x:x + 40, :3] == pixels(bpy, path)[..., :3]).all(), path  # copied exactly
    x, y, w = layout["labels"][0]
    mask = s.text_mask("dim", scale, w - 2 * pad)
    for r, row in enumerate(mask):
        for c, on in enumerate(row):
            want = 1.0 if on else 0.08
            assert abs(got[y + pad + r, x + pad + c, 0] - want) < 0.003, (r, c, got[y + pad + r, x + pad + c])
    expect_error(ValueError, "1 labels for 2 pictures", api.contact_sheet, paths, sheet, ["x"])


def check_parallel(api, substrate, program, prefix):
    out = prefix + "-parallel"
    os.makedirs(out, exist_ok=True)
    script = os.path.join(out, "child.py")
    with open(script, "w") as f:
        f.write(CHILD % blender_common.ADDON)
    paths = api.render_parallel(script, {"a": {"sun": 0.5}, "b": ["--sun", "2.0"]}, out,
                                args=["--substrate", substrate, "--program", program])
    assert [os.path.basename(p) for p in paths] == ["a.png", "b.png"] and all(os.path.isfile(p) for p in paths)
    expect_error(RuntimeError, "variants failed: c (exit 1, no picture", api.render_parallel, script,
                 {"c": {"fail": True}}, out)
    print("API parallel runs: ok")


if __name__ == "__main__":
    import bpy  # noqa: E402  (Blender's own module, already in sys.modules)
    bpy, substrate, program, prefix = blender_common.start()
    prefix = os.path.abspath(prefix)
    os.makedirs(os.path.dirname(prefix), exist_ok=True)
    api = importlib.import_module(blender_common.ADDON + ".api")
    scene = bpy.context.scene
    check_scene(bpy, api, scene)
    check_terrain(bpy, api)
    check_presets(bpy, api, scene)
    check_sky_and_lights(bpy, api, scene)
    check_render(bpy, api, scene, prefix)
    check_parallel(api, substrate, program, prefix)
    print("API-OK")
