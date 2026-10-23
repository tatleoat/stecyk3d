# SPDX-License-Identifier: GPL-3.0-or-later
# (the terrain tool "from a mesh": grid sizes 16 to 1024, 256 first; the heights by rays
#       straight down; a flat or empty mesh refused, naming it; the terrain object's matrix, materials and
#       collections; the mesh kept and hidden from renders; the thinned viewport stand-in)
# (the grid's first and last points on the mesh's box; the new terrain smooth, not mirrored,
#       selected alone; refusals: no active mesh object, a flat or empty mesh)
# (The revision of: Solids),
"""The operator that makes a terrain object from a mesh, and the terrain object every command makes."""

import bpy
from mathutils import Matrix

from . import create, names, solids

HEIGHT_TOP = solids.HEIGHT_TOP
GRID_CHOICES = [(str(n), str(n), "") for n in solids.AUTO_SIZES]
GRID_FIRST = "256"


def heights_image(name, n, heights):
    """A float image holding the heights (row 0 at the bottom), so that terrain() reads them back exactly."""
    image = bpy.data.images.new(name, width=n, height=n, alpha=False, float_buffer=True, is_data=True)
    pixels = []
    for h in heights:
        v = h / HEIGHT_TOP  # a binary32 of h over 65,535 reads back, times 65,535 and rounded, as h
        pixels += (v, v, v, 1.0)
    image.pixels.foreach_set(pixels)
    image.pack()
    return image


def stand_in_mesh(name, n, heights, mirrored=False):
    """: the viewport stand-in, every k-th row and column and the last, where the grid's points lie in the
    terrain's own space (create.grid_point)."""
    keep = solids.stand_in_rows(n)
    verts = [create.grid_point(i, j, heights[j * n + i], n, mirrored) for j in keep for i in keep]
    side = len(keep)
    faces = [(r * side + c, r * side + c + 1, (r + 1) * side + c + 1, (r + 1) * side + c)
             for r in range(side - 1) for c in range(side - 1)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    return mesh


def make_terrain_object(name, n, heights, settings, collection):
    """A terrain object (`bryce_shape` terrain) with its heights picture and `bryce_terrain` settings, linked into
    `collection`."""
    image = heights_image(name, n, heights)
    obj = bpy.data.objects.new(name, stand_in_mesh(name, n, heights, bool(settings.get("mirrored"))))
    obj.bryce_shape = "terrain"
    group = getattr(obj, names.OBJECT_TERRAIN_GROUP)
    group.image = image
    for key, value in settings.items():
        setattr(group, key, value)
    if collection is not None:
        collection.objects.link(obj)
    return obj


def select_alone(context, obj):
    """: the new object selected alone and active."""
    for other in context.selected_objects:
        other.select_set(False)
    obj.select_set(True)
    context.view_layer.objects.active = obj


class STECYK3D_OT_terrain_from_mesh(bpy.types.Operator):
    """Make a Bryce terrain from the active mesh, seen from above"""
    bl_idname = "stecyk3d.terrain_from_mesh"
    bl_label = "Terrain from Mesh"
    bl_options = {"REGISTER", "UNDO"}
    grid: bpy.props.EnumProperty(name="Grid", items=GRID_CHOICES, default=GRID_FIRST)

    def execute(self, context):
        source = context.active_object
        if source is None or source.type != "MESH":
            self.report({"ERROR"}, "the active object is no mesh; no terrain made")  # Errors
            return {"CANCELLED"}
        n = int(self.grid)
        evaluated = source.evaluated_get(context.evaluated_depsgraph_get())
        mesh = evaluated.to_mesh()  # with the modifiers it shows
        try:
            mesh.calc_loop_triangles()
            points = [tuple(v.co) for v in mesh.vertices]  # loose points included
            triangles = [tuple(t.vertices) for t in mesh.loop_triangles]
        finally:
            evaluated.to_mesh_clear()
        try:
            lo, hi = solids.mesh_box(points)
            heights = solids.heights_from_mesh(points, triangles, n)
        except ValueError as error:
            self.report({"ERROR"}, "%s: %s; a terrain needs a surface with some height; no terrain made"
                        % (source.name, error))
            return {"CANCELLED"}
        settings = {"grid": "AUTO", "mirrored": False, "smooth": True}
        terrain = make_terrain_object(source.name + " terrain", n, heights, settings, None)
        for material in source.data.materials:
            terrain.data.materials.append(material)
        terrain.matrix_world = source.matrix_world @ Matrix(solids.terrain_placement(lo, hi, n))
        for collection in source.users_collection:
            collection.objects.link(terrain)
        source.hide_render = True  # : the mesh stays, hidden from renders
        select_alone(context, terrain)
        return {"FINISHED"}

    def invoke(self, context, event):
        return context.window_manager.invoke_props_dialog(self)


def menu_entry(self, context):
    if context.engine == names.ENGINE_ID:
        self.layout.operator(STECYK3D_OT_terrain_from_mesh.bl_idname)


def register():
    bpy.utils.register_class(STECYK3D_OT_terrain_from_mesh)
    bpy.types.VIEW3D_MT_object.append(menu_entry)


def unregister():
    bpy.types.VIEW3D_MT_object.remove(menu_entry)
    bpy.utils.unregister_class(STECYK3D_OT_terrain_from_mesh)
