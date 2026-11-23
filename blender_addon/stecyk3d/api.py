# SPDX-License-Identifier: GPL-3.0-or-later
# (The scripting API; docs/manual/scripting.md)
# (needs Blender; its plain parts are scripting.py, tested without Blender)
"""Stecyk3D's scripting API: build and render Bryce scenes from a Python script, in Blender or headless
(`blender -b --python build.py -- ...`). The names here are kept stable from release to release; everything else in
the add-on may change. An unsupported case is refused with an error that names it.

    from bl_ext.user_default.stecyk3d import api       # the package name: see api.package_name()

    api.begin(program="5.0")
    rock = api.preset_material("Rocks&Stones", "Pitted Granite")
    api.shape("sphere", "Ball", (0, 0, 1), material=rock)
    api.sky(sun_intensity=1.0)
    api.render("out/ball.png", resolution=(320, 240))
"""

import os
import sys

import bpy
from mathutils import Vector

from . import components, create, defaults, gather, material_nodes, names, preset_ops, presets, scripting, shader_ops
from .create_ops import stand_in_object
from .scripting import Arguments, TerrainGrid, variant_flags  # noqa: F401  (part of the API)
from .terrain_tool import make_terrain_object

LIGHT_KINDS = ("POINT", "SPOT", "AREA", "SUN")


def package_name():
    """The add-on's package as Blender knows it: an installed extension's is long ("bl_ext.<repository>.stecyk3d").
    A script finds it with the same test: the enabled add-on whose name ends in "stecyk3d"."""
    return __package__


def _scene(scene=None):
    return scene if scene is not None else bpy.context.scene


def _program(scene=None):
    return getattr(_scene(scene), names.SCENE_RENDER_GROUP).program


def log(*parts):
    """What the API did, on standard output (headless runs show it in the terminal)."""
    print("STECYK3D", *parts)


# ---------------------------------------------------------------- the scene

def begin(program="5.0", clear=False, scene=None):
    """The scene renders with Stecyk3D as the program given ("3.0", "3.1" or "5.0"). With `clear` (asked for, never the
    default: it empties a scene open in Blender too), its objects go, it gets a new empty world, and every datablock nothing uses any more is removed, so a script (or each
    variant of render_variants) starts from nothing. Returns the scene."""
    scene = _scene(scene)
    if program not in names.PROGRAM:
        raise ValueError("program %r is not one of %s" % (program, ", ".join(names.PROGRAM)))
    scene.render.engine = names.ENGINE_ID
    getattr(scene, names.SCENE_RENDER_GROUP).program = program
    if clear:
        for obj in list(scene.objects):
            bpy.data.objects.remove(obj)
        scene.world = bpy.data.worlds.new("World")
        bpy.data.orphans_purge(do_local_ids=True, do_linked_ids=True, do_recursive=True)
    return scene


def arguments(argv=None):
    """The script's arguments after Blender's `--` (Arguments: .positional, .has(flag), .get(flag, default))."""
    return scripting.script_arguments(sys.argv if argv is None else argv)


# ---------------------------------------------------------------- materials

def _libraries(context):
    offered, _ = preset_ops.offered(context, "materials")
    return offered


def preset_material(library, *preset_names, channels=None, context=None):
    """A new material from the first of Bryce's material presets `preset_names` (in library `library`, named as
    the Material panel offers them, "Waters&Liquids") that loads; each that does not is logged and the next tried.
    `channels` puts values over the preset's channel constants (Materials Lab numbers, colours as display values:
    {"reflection": 0.65, "diffuse_color": (0.04, 0.38, 0.68)}). A library, preset or channel name that does not
    exist is refused (a misspelling is no fallback), and so is a list none of whose presets loads."""
    context = context or bpy.context
    if not preset_names:
        raise ValueError("preset_material needs at least one preset name")
    offered = _libraries(context)
    keys = scripting.library_matches(offered, library)
    if not keys:
        raise ValueError("no material library %r; offered: %s"
                         % (library, ", ".join(label for label, _ in offered) or "none (is the substrate set?)"))
    loaded, reasons = [], []
    for key in keys:
        lib, why = preset_ops.load(context, key)
        if lib is None:
            reasons.append(why)
        else:
            loaded.append(lib)
    if not loaded:
        raise RuntimeError("material library %s cannot be read: %s" % (library, "; ".join(reasons)))
    for name in preset_names:
        found = [(lib, scripting.preset_index(presets.offered_presets(lib), name, library)) for lib in loaded]
        found = [(lib, index) for lib, index in found if index is not None]
        if not found:
            raise ValueError("library %s has no preset %r" % (library, name))
        lib, index = found[0]
        try:
            value = components.apply_material_preset(lib, index)
        except Exception as error:  # the preset does not load in the add-on: the next name is tried
            log("PRESET %s / %s won't load (%s)" % (library, name, error))
            continue
        scripting.override_channels(value, channels, "%s / %s" % (library, name))
        material, images = bpy.data.materials.new(name), {}
        try:
            images = preset_ops.make_images(preset_ops._used_pictures(lib, value))
            material_nodes.write_material(material, value, images)
        except Exception as error:
            bpy.data.materials.remove(material)
            for image in images.values():
                bpy.data.images.remove(image)
            log("PRESET %s / %s won't load (%s)" % (library, name, error))
            continue
        log("PRESET %s / %s" % (library, name))
        return material
    raise RuntimeError("no preset of %s loads: %s" % (library, ", ".join(preset_names)))


def colour_material(name, rgb, roughness=0.8, metallic=0.0, glow=None):
    """A new Bryce material made from a Principled BSDF's colour (linear, as Blender's colour picker gives it),
    roughness and metallic by the add-on's own conversion (Bryce material from the Blender shader). `glow`
    (rgb, strength) is the Principled emission: Bryce's ambient colour and ambience, so the object lights itself."""
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    principled = next(n for n in material.node_tree.nodes if n.bl_idname == shader_ops.PRINCIPLED)
    principled.inputs["Base Color"].default_value = (*rgb, 1.0)
    principled.inputs["Roughness"].default_value = roughness
    principled.inputs["Metallic"].default_value = metallic
    if glow:
        principled.inputs["Emission Color"].default_value = (*glow[0], 1.0)
        principled.inputs["Emission Strength"].default_value = glow[1]
    shader_ops.from_principled(material)
    return material


# ---------------------------------------------------------------- objects

def _place(obj, location, scale, rotation, material):
    obj.location = location
    obj.rotation_mode = "XYZ"
    obj.rotation_euler = rotation
    obj.scale = scale
    if material is not None:
        obj.data.materials.append(material)


def shape(kind, name, location, scale=(1.0, 1.0, 1.0), rotation=(0.0, 0.0, 0.0), material=None, collection=None):
    """A Bryce shape (create.SHAPES_ADDED: plane, slab, cube, sphere, cylinder, cone, pyramid, torus, square, disc;
    metaball and tree with the program 5.0), as the Add menu makes it, at `location` with `scale` and `rotation`
    (radians, XYZ), linked into `collection` (the scene's by default). Made by the data API: no 3D cursor, no
    selection, no operator. Returns the object."""
    if kind in create.FIVE_ONLY_SHAPES and _program() != "5.0":
        raise ValueError("a %s is a shape of the program 5.0; the scene's program is %s" % (kind, _program()))
    made = create.new_shape(kind, location)
    obj = stand_in_object(name, kind, made["torus"], collection or _scene().collection)
    obj.bryce_shape = kind
    if made["torus"] is not None:
        obj["bryce_torus"] = made["torus"]
    _place(obj, location, scale, rotation, material)
    return obj


def terrain(name, n, centre, size, z_range, height, material=None, collection=None):
    """A Bryce terrain of n by n points (16, 32 ... 1024) over `size` (x, y) about `centre` (x, y), its heights
    `height(x, y)` in world units held to `z_range` (low, high). Returns a TerrainGrid: `.object`, and the exact
    `.z_at(x, y)` and `.lowest_under(x, y, radius)` of the surface the renderer draws."""
    grid = TerrainGrid(n, centre, size, z_range, height)
    if grid.clamped:
        log("TERRAIN %s: %d of %d heights held to z_range %r" % (name, grid.clamped, n * n, tuple(z_range)))
    obj = make_terrain_object(name, n, grid.heights, dict(scripting.TERRAIN_SETTINGS),
                              collection or _scene().collection)
    obj.location, obj.scale = grid.location, grid.scale
    if material is not None:
        obj.data.materials.append(material)
    grid.object = obj
    return grid


def _program_defaults(key, group, program):
    """A new sky or light's settings that hold the other program's default take this program's, as they would
    if the program were changed now (props.apply_program_defaults): a new world or lamp is made with 3.0's."""
    for name, _old, _new in defaults.five_rows(key):
        value = defaults.switched_value(key, name, getattr(group, name), program)
        if value is not None:
            setattr(group, name, value)


def _known(group, what, settings, program, five_only):
    """Refuses any setting the group does not have, and the program 5.0's settings under another program."""
    known = {p.identifier for p in group.bl_rna.properties if p.identifier not in ("rna_type", "name")}
    for key in settings:
        if key not in known:
            raise ValueError("%s has no setting %r" % (what, key))
        if program != "5.0" and five_only(key):
            raise ValueError("%s setting %r is the program 5.0's; the scene's program is %s" % (what, key, program))


def _set(group, what, settings):
    for key, value in settings.items():
        try:
            setattr(group, key, value)
        except (TypeError, ValueError, AttributeError) as error:
            raise ValueError("%s setting %r: %s" % (what, key, error)) from None


def sky(world=None, **settings):
    """The scene's world (or `world`) gets a Bryce Sky node if it has none, and `settings` by name (the sky
    settings of props.SKY and the World tab's switches such as flag_8, Custom colours). An unknown name is
    refused, and so is a setting of the program 5.0 (sky5_*, sun_radius ...) under another program. Returns the
    node's settings, for reading or setting more."""
    if world is None:
        scene = _scene()
        if scene.world is None:
            scene.world = bpy.data.worlds.new("World")
        world = scene.world
    made = shader_ops.make_sky(world)
    group = gather.find_node(world.node_tree, "BryceSkyNode").settings
    if made:
        _program_defaults("sky", group, _program())
    _known(group, "the sky", settings, _program(),
           lambda key: key in names.FIVE_ONLY["sky"] or key.startswith("sky5"))
    _set(group, "sky", settings)
    return group


def light(name, kind="POINT", location=(0.0, 0.0, 0.0), direction=None, collection=None, **settings):
    """A lamp: POINT, SPOT or AREA with a Bryce Light node and `settings` by name (props.LIGHT: intensity, color,
    edge, reach ...; an unknown name, or a setting of the program 5.0 under another program, is refused); or a
    SUN, whose light is the sky's (set it with sky(sun_intensity=..., sun_color=...)), so it takes no settings.
    `direction` is the way from the scene to the light (the lamp's own +Z): for a sun, the way to the sun.
    Returns the object."""
    if kind not in LIGHT_KINDS:
        raise ValueError("a light of kind %r is not one of %s" % (kind, ", ".join(LIGHT_KINDS)))
    if kind == "SUN" and settings:
        raise ValueError("a sun's light is the sky's: set %s with sky(), not light()" % ", ".join(settings))
    data = bpy.data.lights.new(name, kind)
    obj = bpy.data.objects.new(name, data)
    (collection or _scene().collection).objects.link(obj)
    obj.location = location
    if direction is not None:
        obj.rotation_mode = "XYZ"
        obj.rotation_euler = Vector(direction).to_track_quat("Z", "Y").to_euler()
    if kind != "SUN":
        shader_ops.make_light(data)
        group = gather.find_node(data.node_tree, "BryceLightNode").settings
        _program_defaults("light", group, _program())
        try:
            _known(group, "light %r" % name, settings, _program(), lambda key: key in names.FIVE_ONLY["light"])
            _set(group, "light %r" % name, settings)
        except ValueError:
            bpy.data.objects.remove(obj)
            bpy.data.lights.remove(data)
            raise
    return obj


# ---------------------------------------------------------------- rendering

def render(path, resolution=None, scene=None):
    """Renders the scene (F12, written as a PNG) to `path`, made absolute against the current folder (Blender would
    read a relative path against the .blend), its folder made. `resolution` (width, height) at 100%. Returns the
    absolute path. A refused render raises Blender's error, which names the problems."""
    scene = _scene(scene)
    if bpy.context.scene != scene:
        raise ValueError("render() renders the context's scene; %r is not it" % scene.name)
    path = os.path.abspath(bpy.path.abspath(path) if path.startswith("//") else path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    scene.render.engine = names.ENGINE_ID
    if resolution is not None:
        scene.render.resolution_x, scene.render.resolution_y = (int(v) for v in resolution)
        scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    if not os.path.isfile(path):
        raise RuntimeError("the render wrote no picture at %s" % path)
    log("RENDER", path)
    return path


def render_variants(build, variants, out_dir, resolution=None, sheet=None, columns=3):
    """Renders each variant in this Blender, one after the other: `build(**overrides)` builds the scene (it would
    call begin() to start from nothing, or change only what the variant changes), then the picture goes to
    OUT_DIR/NAME.png. `variants` is {name: overrides} or a list of overrides (named v1, v2 ...). With `sheet`, a
    contact sheet of them all is written there too. Returns the pictures' paths in order."""
    out_dir = os.path.abspath(out_dir)
    paths, labels = [], []
    for name, overrides in scripting.variant_items(variants):
        if not isinstance(overrides, dict):
            raise ValueError("variant %s: render_variants takes a dict of keyword values, not %r" % (name, overrides))
        log("VARIANT", name, overrides)
        build(**overrides)
        paths.append(render(os.path.join(out_dir, name + ".png"), resolution))
        labels.append(name)
    if sheet:
        contact_sheet(paths, sheet, labels, columns)
    return paths


def render_parallel(script, variants, out_dir, blender=None, jobs=None, args=()):
    """Renders each variant in a headless Blender of its own, at most `jobs` at once (all by default; one Blender
    builds and renders serially, so this is quicker when building takes long), and waits for them. Each runs
        blender -b --python SCRIPT -- OUT_DIR --name NAME [args] [the variant's flags]
    and must write OUT_DIR/NAME.png; a variant's overrides are flags ({"moon": 12, "no_halo": True} gives
    `--moon 12 --no-halo`) or a list of text. Output goes to OUT_DIR/NAME.log. `blender` is this Blender by
    default. A run that fails is refused, naming its log. Returns the pictures' paths in order."""
    return scripting.run_parallel(blender or bpy.app.binary_path, script, variants, out_dir, jobs, args, log=log)


def contact_sheet(paths, out_path, labels=None, columns=3, background=(0.08, 0.08, 0.08), ink=(1.0, 1.0, 1.0)):
    """One PNG of the pictures `paths` in a grid of `columns` columns, each with its label below it (`labels`, else
    the file names; drawn in capitals with the add-on's own small font). Returns the sheet's absolute path."""
    import numpy as np
    if labels is not None and len(labels) != len(paths):
        raise ValueError("%d labels for %d pictures" % (len(labels), len(paths)))
    labels = labels if labels is not None else [os.path.splitext(os.path.basename(p))[0] for p in paths]
    pictures = []
    for path in paths:
        image = bpy.data.images.load(os.path.abspath(path), check_existing=False)
        try:
            w, h = image.size
            if w == 0 or h == 0:
                raise ValueError("%s is not a picture Blender can read" % path)
            px = np.empty(w * h * 4, dtype=np.float32)
            image.pixels.foreach_get(px)
            pictures.append(px.reshape(h, w, 4)[::-1])  # Blender's rows run from the bottom
        finally:
            bpy.data.images.remove(image)
    cell_w = max(p.shape[1] for p in pictures)
    scale = scripting.label_scale(cell_w)
    pad = 3 * scale
    layout = scripting.sheet_layout([(p.shape[1], p.shape[0]) for p in pictures], columns,
                                    scripting.GLYPH_HEIGHT * scale + 2 * pad, 2 * pad)
    width, height = layout["size"]
    sheet = np.empty((height, width, 4), dtype=np.float32)
    sheet[...] = (*background, 1.0)
    for px, (x, y) in zip(pictures, layout["pictures"]):
        sheet[y:y + px.shape[0], x:x + px.shape[1]] = px
        sheet[y:y + px.shape[0], x:x + px.shape[1], 3] = 1.0
    for text, (x, y, w) in zip(labels, layout["labels"]):
        mask = np.array(scripting.text_mask(str(text), scale, w - 2 * pad), dtype=bool)
        if mask.size:
            region = sheet[y + pad:y + pad + mask.shape[0], x + pad:x + pad + mask.shape[1]]
            region[mask] = (*ink, 1.0)
    out_path = os.path.abspath(out_path)
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    image = bpy.data.images.new("contact sheet", width, height, alpha=False)
    try:
        image.pixels.foreach_set(sheet[::-1].ravel())
        image.filepath_raw = out_path
        image.file_format = "PNG"
        image.save()
    finally:
        bpy.data.images.remove(image)
    log("SHEET", out_path)
    return out_path
