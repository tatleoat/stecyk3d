# SPDX-License-Identifier: GPL-3.0-or-later
# (the Add menu's Bryce shapes: one mesh object marked as the shape, at the cursor, no turn, scale 1,
#       selected alone and active; the torus's radii; metaball and tree offered for the program 5.0)
# (needs Blender, not testable here)
"""The Add menu's Bryce shapes and terrains, and the Object menu's group from selection."""

import bpy
from mathutils import Matrix

from . import create, names, reader
from .preset_ops import program_of
from .terrain_tool import make_terrain_object, select_alone

def stand_in_object(name, shape, torus, collection):
    """A mesh object showing the shape (create.stand_in, Freedom), linked into `collection`."""
    verts, faces = create.stand_in(shape, torus)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    if collection is not None:
        collection.objects.link(obj)
    return obj


def is_five(context):
    return getattr(context.scene, names.SCENE_RENDER_GROUP).program == "5.0"


class STECYK3D_OT_add_shape(bpy.types.Operator):
    """Add one of Bryce's shapes at the 3D cursor """
    bl_idname = "stecyk3d.add_shape"
    bl_label = "Add Bryce shape"
    bl_options = {"REGISTER", "UNDO"}
    shape: bpy.props.EnumProperty(items=[(s, s.capitalize(), "") for s in create.SHAPES_ADDED])

    def execute(self, context):
        made = create.new_shape(self.shape, context.scene.cursor.location)
        obj = stand_in_object("Bryce " + self.shape, self.shape, made["torus"], context.collection)
        obj.location = made["location"]
        obj.rotation_mode = "XYZ"
        obj.rotation_euler = made["rotation"]
        obj.scale = made["scale"]
        obj.bryce_shape = self.shape
        if made["torus"] is not None:
            obj["bryce_torus"] = made["torus"]
        select_alone(context, obj)
        return {"FINISHED"}


class STECYK3D_OT_add_terrain(bpy.types.Operator):
    """Add a new Bryce terrain or symmetrical lattice, its relief made from the seed """
    bl_idname = "stecyk3d.add_terrain"
    bl_label = "Add Bryce terrain"
    bl_options = {"REGISTER", "UNDO"}
    kind: bpy.props.EnumProperty(items=[("terrain", "Terrain", ""), ("lattice", "Symmetrical lattice", "")])
    seed: bpy.props.IntProperty(name="Seed", min=0, max=create.SEED_MAX, default=0,
                                description="0: a seed drawn at random each time")

    def execute(self, context):
        seed = self.seed or create.draw_seed()
        try:
            size, heights = reader.new_relief(program_of(context), seed)
        except reader.ReadRefused as refused:
            self.report({"ERROR"}, "no terrain made: %s" % refused)
            return {"CANCELLED"}
        made = create.new_terrain(self.kind, context.scene.cursor.location)
        name = "Bryce lattice" if self.kind == "lattice" else "Bryce terrain"
        obj = make_terrain_object(name, size, heights, made["terrain"], context.collection)
        obj.matrix_world = Matrix(create.matrix_of(made["location"], made["scale"]))
        select_alone(context, obj)
        return {"FINISHED"}


class STECYK3D_OT_group_from_selection(bpy.types.Operator):
    """Make a Bryce group of the selected objects """
    bl_idname = "stecyk3d.group_from_selection"
    bl_label = "Bryce group from selection"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        selected = list(context.selected_objects)
        index = {obj: k for k, obj in enumerate(selected)}
        places = [list(obj.matrix_world.translation) for obj in selected]
        parents = [index.get(obj.parent, -1) for obj in selected]
        try:
            plan = create.group_plan(places, parents, list(range(len(selected))))
        except create.Refused as refused:
            self.report({"ERROR"}, str(refused))
            return {"CANCELLED"}
        empty = bpy.data.objects.new("Bryce group", None)
        context.collection.objects.link(empty)
        empty.location = plan["place"]
        empty.bryce_shape = "group"
        context.view_layer.update()
        for k in plan["members"]:
            obj = selected[k]
            world = obj.matrix_world.copy()
            obj.parent = empty
            obj.matrix_parent_inverse = empty.matrix_world.inverted()
            obj.matrix_world = world
        select_alone(context, empty)
        return {"FINISHED"}


class STECYK3D_MT_add(bpy.types.Menu):
    bl_idname = "STECYK3D_MT_add"
    bl_label = "Bryce shape"

    def draw(self, context):
        five = is_five(context)
        for shape in create.SHAPES_ADDED:
            if shape in create.FIVE_ONLY_SHAPES and not five:
                continue  # Variants
            self.layout.operator(STECYK3D_OT_add_shape.bl_idname, text=shape.capitalize()).shape = shape


class STECYK3D_MT_add_terrain(bpy.types.Menu):
    bl_idname = "STECYK3D_MT_add_terrain"
    bl_label = "Bryce terrain"

    def draw(self, context):
        self.layout.operator(STECYK3D_OT_add_terrain.bl_idname, text="Terrain").kind = "terrain"
        self.layout.operator(STECYK3D_OT_add_terrain.bl_idname, text="Symmetrical lattice").kind = "lattice"


def add_menu(self, context):
    if context.engine == names.ENGINE_ID:
        # the earlier add-on's two entries, straight in the Add menu
        self.layout.menu(STECYK3D_MT_add.bl_idname, icon="MESH_UVSPHERE")
        self.layout.menu(STECYK3D_MT_add_terrain.bl_idname, icon="RNDCURVE")


def object_menu(self, context):
    if context.engine == names.ENGINE_ID:
        self.layout.operator(STECYK3D_OT_group_from_selection.bl_idname, icon="GROUP")


CLASSES = [STECYK3D_OT_add_shape, STECYK3D_OT_add_terrain, STECYK3D_OT_group_from_selection, STECYK3D_MT_add,
           STECYK3D_MT_add_terrain]


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)
    bpy.types.VIEW3D_MT_add.append(add_menu)
    bpy.types.VIEW3D_MT_object.append(object_menu)


def unregister():
    bpy.types.VIEW3D_MT_object.remove(object_menu)
    bpy.types.VIEW3D_MT_add.remove(add_menu)
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
