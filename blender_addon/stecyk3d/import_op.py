# SPDX-License-Identifier: GPL-3.0-or-later
# , (what each choice makes; nothing made before
#       the document is read whole; a second whole import refused before anything is made; the program set to 5.0
#       by a .br5 whole import; a world already in the scene given the sky and kept)
# (an import; needs Blender, not testable here)
"""File > Import > Bryce document: carries out importing.plan_import's plan in Blender."""

import json
import math
import base64
import zlib

import bpy
from bpy_extras.io_utils import ImportHelper
from mathutils import Matrix

from . import colour, importing, material_nodes, names, props, reader
from .create_ops import stand_in_object
from .gather import find_node, group_to_dict, sky_group
from . import convert
from .preset_ops import make_images, program_of
from .terrain_tool import make_terrain_object


def _signed(v):
    v = int(v) & 0xFFFFFFFF
    return v - (1 << 32) if v >= (1 << 31) else v


def fill_group(group, settings, words=()):
    """Copies settings by name into a registered group; words as Blender's signed 32-bit numbers ."""
    for key, value in settings.items():
        if value is None or not hasattr(group, key):
            continue
        prop = group.bl_rna.properties.get(key)
        try:
            if prop is not None and prop.type == "INT" and not getattr(prop, "is_array", False):
                setattr(group, key, _signed(value))
            elif prop is not None and prop.type == "INT":
                setattr(group, key, [_signed(v) for v in value])
            else:
                setattr(group, key, value)
        except (TypeError, ValueError):
            pass  # a number outside what the setting holds: left at its default (Errors, our choice)


def keep_exact_sky(world, sky):
    """The sky numbers a document holds as binary64 that a Blender float cannot (convert.SKY_DOUBLES): kept with the
    single the property holds, so describe sends the document's number while the property is untouched."""
    group = sky_group(world)
    kept = {}
    for setting in convert.SKY_DOUBLES:
        want = sky.get(setting)
        if want is None or not hasattr(group, setting):
            continue
        held = getattr(group, setting)
        if isinstance(want, (list, tuple)):
            if len(want) != len(held) or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in want):
                continue
            if list(want) == [float(h) for h in held]:
                continue
            kept[setting] = {"exact": [float(v) for v in want], "held": [float(h) for h in held]}
        elif isinstance(want, (int, float)) and not isinstance(want, bool) and math.isfinite(want):
            if float(want) == float(held):
                continue
            kept[setting] = {"exact": float(want), "held": float(held)}
    if kept:
        world["bryce_exact_doubles"] = json.dumps(kept)


def keep_exact_sun(world, sky, sun):
    """The sun direction the document holds (Bryce's axes, any length) beside the sun lamp's own z axis as the import
    left it; gather sends the document's direction while the lamp still stands there ."""
    exact = sky.get("sun_direction_exact")
    if not exact or not all(isinstance(c, float) and math.isfinite(c) for c in exact):
        return
    world["bryce_exact_sun"] = json.dumps({"direction": exact, "held": [float(c) for c in sun.matrix_world.col[2][:3]]})


def world_for(scene, plan, images=None):
    if plan["world"] is None:
        return None
    world = scene.world
    if world is None:
        world = bpy.data.worlds.new("Bryce sky")
        scene.world = world
    world.use_nodes = True
    if find_node(world.node_tree, "BryceSkyNode") is None:
        node = world.node_tree.nodes.new("BryceSkyNode")
        output = next((n for n in world.node_tree.nodes if n.bl_idname == "ShaderNodeOutputWorld"), None)
        if output is not None:
            world.node_tree.links.new(node.outputs["sky"], output.inputs["Surface"])
    props.apply_program_defaults(getattr(scene, names.SCENE_RENDER_GROUP).program)
    fill_group(sky_group(world), plan["sky"])
    exact = plan["sky"].get("exact_numbers")
    if exact:
        # Blender's float properties clamp an infinity to the largest single; the document's own (a rainbow disc, a
        # sky colour) is kept beside them and gather puts it back where the property still holds the clamp.
        world["bryce_exact_numbers"] = json.dumps(exact)
    keep_exact_sky(world, plan["sky"])
    node = find_node(world.node_tree, "BryceSkyNode")
    for layer, comp in (plan.get("sky_textures") or {}).items():  # the cloud layers' textures (not dropped)
        texture = material_nodes.add_component_nodes(world.node_tree, comp, images,
                                                     (-600.0, -300.0 * len(world.node_tree.nodes)))
        world.node_tree.links.new(texture.outputs["color"], node.inputs[layer])
    if plan["sky"].get("sun_flags") == 0:
        world["bryce_sun_flags_explicit"] = 1  # a document's own zero word is the sun off, not unset (gather.py)
    return world


def set_marks(obj, plan_obj):
    """: a group member's role and cut switch, a metaball's sign, as the object's marks."""
    if plan_obj.get("role") is not None:
        obj.bryce_role = plan_obj["role"]
        obj.bryce_cut_shows_positive = bool(plan_obj.get("cut_shows_positive"))
    if plan_obj.get("negative"):
        obj.bryce_negative = True
    if plan_obj.get("placement_flags"):
        obj["bryce_flags"] = [int(w) - (1 << 32 if int(w) >= (1 << 31) else 0) for w in plan_obj["placement_flags"]]  # signed: an ID int is 32-bit; gather masks back  #: hidden bits and the like, carried to the render


def make_object(plan_obj, collection, materials, units):
    kind = plan_obj["object_type"]
    name = plan_obj["name"] or plan_obj["kind"]
    if kind == "LIGHT":
        light = bpy.data.lights.new(name, plan_obj["lamp"])
        for key, value in (plan_obj.get("lamp_data") or {}).items():
            setattr(light, key, value)
        obj = bpy.data.objects.new(name, light)
        if plan_obj.get("light"):
            settings = dict(plan_obj["light"])
            for key in ("radius", "range", "gradient_range"):
                if settings.get(key) is not None:  # a NaN radius reads as null: left at its default, not a crash
                    settings[key] = settings[key] / units  # : these are times `units` in the description
            fill_group(getattr(light, names.DATA_GROUP), settings)
            group = getattr(light, names.DATA_GROUP)
            if plan_obj.get("material") is not None and plan_obj["material"] in materials:
                group.material = materials[plan_obj["material"]]
    elif kind == "EMPTY":
        obj = bpy.data.objects.new(name, None)
    elif plan_obj.get("bryce_shape") == "terrain":
        obj = make_terrain_object(name, plan_obj["size"], plan_obj["heights"], plan_obj["terrain"], None)
    elif plan_obj["kind"] == "mesh":
        mesh = bpy.data.meshes.new(name)
        mesh.from_pydata(plan_obj["points"], [], plan_obj["triangles"])
        mesh.update()
        set_mesh_shading(mesh, plan_obj)
        obj = bpy.data.objects.new(name, mesh)
    else:
        obj = stand_in_object(name, plan_obj["bryce_shape"], plan_obj.get("torus"), None)
    collection.objects.link(obj)
    if plan_obj.get("bryce_shape"):
        obj.bryce_shape = plan_obj["bryce_shape"]
    if plan_obj.get("torus"):
        obj["bryce_torus"] = plan_obj["torus"]
    obj.matrix_world = Matrix(plan_obj["matrix"])
    keep_shear(obj, plan_obj["matrix"])
    keep_stored_tree(obj, plan_obj)
    set_marks(obj, plan_obj)
    if obj.type == "MESH":
        for index in (plan_obj.get("material"), plan_obj.get("leaves_material")):
            if index is not None and index in materials:
                obj.data.materials.append(materials[index])
    return obj


def keep_shear(obj, wanted):
    """A document's matrix may be more than a place, a turn and a scale (a shear, e.g. R after rows (1,0,0),(0.5,1,0),
    (0,0,1)); Blender's matrix_world is made of those three, so what it cannot hold goes in `bryce_shear` ,
    which gather multiplies on the right: wanted = held x shear. Left off where the matrix is held to 1e-9."""
    if obj.type == "LIGHT":
        return
    held = obj.matrix_basis.copy()  # from location, rotation and scale as just set (no depsgraph update per object)
    try:
        shear = held.inverted() @ Matrix(wanted)
    except ValueError:  # a flat (singular) matrix: nothing to split
        return
    identity = Matrix.Identity(4)
    if all(abs(shear[i][j] - identity[i][j]) <= 1e-9 for i in range(4) for j in range(4)):
        return
    obj["bryce_shear"] = [float(shear[i][j]) for i in range(4) for j in range(4)]


def keep_stored_tree(obj, plan_obj):
    """A document's tree is drawn as stored : its pieces, leaves and leaf shapes cannot be grown again from the
    settings the add-on shows (growing from defaults gave another tree, and one that could even open). The
    stored form is kept packed on the object with the settings as imported; gather.stored_tree_of uses it while the
    settings are still those (a user who changed them wants the new growth)."""
    stored = plan_obj.get("stored")
    if not stored or plan_obj.get("bryce_shape") != "tree":
        return
    held = convert.tree_settings(group_to_dict(getattr(obj, names.OBJECT_TREE_GROUP)))
    body = json.dumps({"stored": stored, "stored_id": plan_obj.get("stored_id"), "held": held}, separators=(",", ":"))
    obj["bryce_stored_tree"] = base64.b64encode(zlib.compress(body.encode(), 6)).decode()


def set_mesh_shading(mesh, plan_obj):
    """The mesh's smooth faces with their stored corner normals (custom normals) and its texture coordinates as the
    active UV map. A triangle from_pydata left out (one with a repeated corner) is skipped with its numbers.
    """
    smooth = plan_obj.get("smooth")
    if not smooth or len(mesh.polygons) != len(smooth):
        return
    mesh.polygons.foreach_set("use_smooth", smooth)
    normals, uvs = plan_obj["corner_normals"], plan_obj["corner_uvs"]
    if any(n is not None for n in normals):
        loops = []
        for n in normals:
            loops.extend(n if n is not None else ([0.0, 0.0, 0.0],) * 3)
        mesh.normals_split_custom_set(loops)  # a zero vector keeps the face's own
    if any(uv is not None for uv in uvs):
        layer = mesh.uv_layers.new(name="UVMap")
        flat = []
        for uv in uvs:
            for c in (uv if uv is not None else ([0.0, 0.0],) * 3):
                flat.extend(c)
        layer.data.foreach_set("uv", flat)
    mesh.update()


def keep_exact_camera(obj, plan_cam):
    """Blender holds a camera's matrix, lens and shift as singles, so the numbers of the document come back a few
    parts in 1e8 off; a picture of fine grain (the wash of a Bryce 5 sky, high-frequency clouds) is made of other
    pixels for that. The document's own numbers are kept beside the camera with the singles Blender gave back;
    gather.exact_camera uses them while the camera still holds exactly those singles (not moved or changed since).
    (Import: the sky and the render settings)"""
    import json
    flat = [v for row in plan_cam["matrix"] for v in row]
    numbers = flat + [plan_cam["lens"], plan_cam["sensor_width"], plan_cam["shift_x"], plan_cam["shift_y"]]
    if any(v != v or abs(v) == float("inf") for v in numbers):
        return
    bpy.context.view_layer.update()  # the matrix is recomposed from location, rotation, scale: read it as it settles
    data = obj.data
    held = [v for row in obj.matrix_world for v in row] + [data.lens, data.sensor_width, data.shift_x, data.shift_y]
    kept = {"exact": numbers, "held": held}
    if plan_cam.get("view_rows") and all(c == c and abs(c) != float("inf") for r in plan_cam["view_rows"] for c in r):
        kept["view_rows"], kept["view_centre"] = plan_cam["view_rows"], plan_cam["view_centre"]
        kept["plane_height"] = plan_cam.get("plane_height")
    obj["bryce_exact_camera"] = json.dumps(kept)


def make_camera(plan_cam, scene, collection):
    data = bpy.data.cameras.new(plan_cam["name"] or "Bryce camera")
    data.type = "PANO" if plan_cam.get("panorama") else "PERSP"
    if data.type == "PANO":  # the one panorama Bryce draws (camera_frame.renderer_view): a whole turn, central cylindrical
        data.panorama_type = "CENTRAL_CYLINDRICAL"
        data.central_cylindrical_range_u_min, data.central_cylindrical_range_u_max = -math.pi, math.pi
        data.central_cylindrical_range_v_min, data.central_cylindrical_range_v_max = -math.pi * 0.5, math.pi * 0.5
    data.sensor_fit = plan_cam["sensor_fit"]
    data.sensor_width = plan_cam["sensor_width"]
    data.lens = plan_cam["lens"]
    data.shift_x, data.shift_y = plan_cam["shift_x"], plan_cam["shift_y"]
    obj = bpy.data.objects.new(data.name, data)
    collection.objects.link(obj)
    obj.matrix_world = Matrix(plan_cam["matrix"])
    keep_exact_camera(obj, plan_cam)
    scene.camera = obj
    scene.render.resolution_x, scene.render.resolution_y = plan_cam["resolution"]
    scene.render.pixel_aspect_x, scene.render.pixel_aspect_y = plan_cam["pixel_aspect"]
    return obj


def carry_out(context, path, plan):
    scene = context.scene
    render = getattr(scene, names.SCENE_RENDER_GROUP)
    images = make_images(plan["pictures"])
    materials = {}
    document_name = bpy.path.display_name_from_filepath(path)
    for m in plan["materials"]:
        material = bpy.data.materials.new("%s: %s" % (document_name, m["name"]))
        material_nodes.write_material(material, m["bryce"], images)
        materials[m["index"]] = material
    # : the program first, so the settings of what exists take its defaults
    # before the document's own numbers are written over them.
    render.program = plan["program"]
    fill_group(render, plan.get("render") or {})  # : the document's own render settings
    colour.make_plain(scene)  # the scene then shows the renderer's levels unchanged, as F12 and the viewport do
    world = world_for(scene, plan, images)
    if not plan["mark_whole"]:
        return
    collection = bpy.data.collections.new(document_name)
    scene.collection.children.link(collection)
    units = render.units or 1.0
    made = {}
    sun = None
    for plan_obj in plan["objects"]:
        obj = make_object(plan_obj, collection, materials, units)
        if plan_obj.get("index") is not None:
            made[plan_obj["index"]] = obj
        if plan_obj.get("lamp") == "SUN":
            sun = obj
    context.view_layer.update()
    for plan_obj in plan["objects"]:  # : a group's members are its children, their places kept
        group = plan_obj.get("group")
        if group is not None and group in made and plan_obj.get("index") in made:
            child = made[plan_obj["index"]]
            world_matrix = child.matrix_world.copy()
            child.parent = made[group]
            child.matrix_parent_inverse = made[group].matrix_world.inverted()
            child.matrix_world = world_matrix
    if sun is not None and world is not None:
        sky_group(world).sun_object = sun
        keep_exact_sun(world, plan["sky"], sun)
    if plan["camera"] is not None:
        make_camera(plan["camera"], scene, collection)
    scene[importing.WHOLE_IMPORT_MARK] = path


class STECYK3D_OT_import_document(bpy.types.Operator, ImportHelper):
    """Import a Bryce document (.br3, .br5): its sky, its materials, or the whole scene"""
    bl_idname = "stecyk3d.import_document"
    bl_label = "Import Bryce document"
    bl_options = {"REGISTER", "UNDO"}
    filter_glob: bpy.props.StringProperty(default="*.br3;*.br5", options={"HIDDEN"})
    sky: bpy.props.BoolProperty(name="Sky, sun and light", default=True)
    materials: bpy.props.BoolProperty(name="Materials and textures", default=True)
    objects: bpy.props.BoolProperty(name="Objects, lights and camera", default=True)

    def execute(self, context):
        scene = context.scene
        render = getattr(scene, names.SCENE_RENDER_GROUP)
        options = {"sky": self.sky, "materials": self.materials, "objects": self.objects}
        state = {"program": render.program, "has_world": scene.world is not None,
                 "whole_import_done": bool(scene.get(importing.WHOLE_IMPORT_MARK)), "units": render.units or 1.0}
        document, refusal = None, None
        if not (self.objects and state["whole_import_done"]):  # refused before reading: nothing is made
            try:
                document = reader.read_document(program_of(context), self.filepath, render.program)
            except reader.ReadRefused as refused:
                refusal = str(refused)
        plan = importing.plan_import(self.filepath, document, refusal, options, state)
        if plan["result"] == "refused":
            self.report({"ERROR"}, "Not imported: " + plan["reason"])
            return {"CANCELLED"}
        carry_out(context, self.filepath, plan)
        for o in plan["objects"]:
            if o.get("note"):
                self.report({"WARNING"}, "%s: %s" % (o["name"], o["note"]))
        return {"FINISHED"}


def menu_entry(self, context):
    self.layout.operator(STECYK3D_OT_import_document.bl_idname, text="Bryce document (.br3, .br5)")


def register():
    bpy.utils.register_class(STECYK3D_OT_import_document)
    bpy.types.TOPBAR_MT_file_import.append(menu_entry)


def unregister():
    bpy.types.TOPBAR_MT_file_import.remove(menu_entry)
    bpy.utils.unregister_class(STECYK3D_OT_import_document)
