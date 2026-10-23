# SPDX-License-Identifier: GPL-3.0-or-later
# (Tests). Task T1 acceptance, run 2:
#   the same headless render for a scene with a sun lamp and at least one object of each kind the
# add-on accepts: the ten shapes of, a plain mesh, and a point lamp (kept
# from the factory scene; a sun lamp is added). Also writes <prefix>-exact.png (exact_png,
#   the renderer's levels untouched) so check_pixels.py can compare it with Blender's saved picture.
"""Sun lamp + every accepted kind, rendered with F12."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # Blender does not add the script's own folder
import blender_common  # noqa: E402


def build(bpy, scene):
    """The test scene: sky colours, a sun lamp, and one object of each accepted kind."""
    # A sky that lights the scene: the registered defaults are all zero (design note on defaults), which would
    # render black; these are the test scene's settings ('s names, held as its own settings).
    sky = scene.world.bryce
    sky.sun_intensity = 1.0
    sky.sun_color = (1.0, 1.0, 1.0)
    sky.ambient_color = (0.2, 0.2, 0.2)

    # A sun lamp (the factory scene has none).
    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun = bpy.data.objects.new("Sun", sun_data)
    scene.collection.objects.link(sun)
    sun.rotation_euler = (0.615476, 0.0, 0.785398)  # Blender's default sun rotation: 45 deg yaw, 35 deg tilt

    # One object of each shape kind the add-on accepts (names.SHAPES minus the ones the render refuses:
    # tree, terrain, metaball, group). The stand-in mesh's own shape does not matter: the renderer draws
    # the shape named by bryce_shape at the object's place .
    SHAPES = ["plane", "slab", "cube", "sphere", "cylinder", "cone", "pyramid", "torus", "square", "disc"]
    for index, shape in enumerate(SHAPES):
        mesh = bpy.data.meshes.new(shape)
        obj = bpy.data.objects.new("bryce_" + shape, mesh)
        scene.collection.objects.link(obj)
        obj.bryce_shape = shape
        obj.location = (index * 0.9 - 4.0, 0.0, 0.6)
        obj.scale = (0.35, 0.35, 0.35)
        if shape == "torus":
            obj["bryce_torus"] = (0.2, 0.8)
        if shape == "plane":  # a floor under the row: a wide plane at the ground
            obj.location = (0.0, 0.0, 0.0)
            obj.scale = (12.0, 12.0, 0.3)

    # The factory Cube (a plain mesh, no bryce_shape) and the factory POINT lamp stay as they are; a
    # material on the cube so the material path is walked too.
    material = bpy.data.materials.new("Red")
    material.diffuse_color = (0.8, 0.1, 0.1, 1.0)
    bpy.data.objects["Cube"].data.materials.append(material)  # the factory Cube has none; slots follow the mesh


if __name__ == "__main__":
    import bpy  # noqa: E402  (Blender's own module, already in sys.modules)
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.bryce_render.exact_png = prefix + "-exact.png"  # : the renderer's levels, for check_pixels.py
    build(bpy, scene)
    bpy.ops.render.render(write_still=True)
    print("SUN-OBJECTS-F12-DONE")
