# SPDX-License-Identifier: GPL-3.0-or-later
# (Mix Materials feed). Headless: a sphere whose material is a Bryce Mix
# Materials node of a red and a blue Bryce Material, rendered with F12 at amounts 0, 0.5 and 1.
#   blender -b --factory-startup --python blender_mix_materials.py -- <substrate> <program> <out-prefix>
"""The mix is at 0 the first material, at 1 the second, at 0.5 between them (red channel falls, blue rises)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
import png_read  # noqa: E402


def build(bpy, scene, amount, textured=None):
    sky = scene.world.bryce
    sky.sun_intensity = 1.0
    sky.sun_color = (1.0, 1.0, 1.0)
    sky.ambient_color = (0.4, 0.4, 0.4)
    for old in [o for o in scene.objects if o.type == "MESH"]:
        bpy.data.objects.remove(old)
    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.rotation_euler = (0.615476, 0.0, 0.785398)
    ball = bpy.data.objects.new("ball", bpy.data.meshes.new("ball"))
    scene.collection.objects.link(ball)
    ball.bryce_shape = "sphere"
    ball.location = (0, 0, 0)
    material = bpy.data.materials.new("mixed%s" % amount)
    material.use_nodes = True
    tree = material.node_tree
    tree.nodes.clear()
    first = tree.nodes.new("BryceMaterialNode")
    second = tree.nodes.new("BryceMaterialNode")
    first.inputs["diffuse_color"].value = (0.9, 0.05, 0.05)
    second.inputs["diffuse_color"].value = (0.05, 0.05, 0.9)
    mix = tree.nodes.new("BryceMixMaterialsNode")
    mix.amount = amount
    tree.links.new(first.outputs["material"], mix.inputs["first"])
    tree.links.new(second.outputs["material"], mix.inputs["second"])
    if textured:  # a Bryce Texture (one noise component, default settings) on the textured end's diffuse colour
        end = first if textured == "first" else second
        texture = tree.nodes.new("BryceTextureNode")
        texture.gives = 3
        component = tree.nodes.new("BryceComponentNode")  # no noise: a flat colour from the colour map (mode 6)
        component.flags, component.mode = 1, 6
        component.first, component.second, component.third = 0xC829FF, 0x1EB43C, 0x283CDC
        tree.links.new(component.outputs["component"], texture.inputs["component"])
        tree.links.new(texture.outputs["color"], end.inputs["diffuse_color"])
    ball.data.materials.append(material)


def centre(path):
    width, height, channels, data = png_read.read_png(path)
    at = ((height // 2) * width + width // 2) * channels
    return tuple(data[at:at + 3])


if __name__ == "__main__":
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.render.resolution_x = scene.render.resolution_y = 32
    shots = {}
    for amount in (0.0, 0.5, 1.0):
        build(bpy, scene, amount)
        scene.render.filepath = "%s-mix-%s.png" % (prefix, amount)
        bpy.ops.render.render(write_still=True)
        shots[amount] = centre(scene.render.filepath)
        print("MIX-AMOUNT", amount, shots[amount])
        for ob in [o for o in scene.objects if o.name.startswith(("ball", "Sun"))]:
            bpy.data.objects.remove(ob)
    # one end textured: the mix is native (first) or the two swap places (second); 0 and 1 still pick one end
    for textured in ("first", "second"):
        for amount in (0.0, 1.0):
            build(bpy, scene, amount, textured)
            scene.render.filepath = "%s-mix-%s-%s.png" % (prefix, textured, amount)
            bpy.ops.render.render(write_still=True)
            shots[(textured, amount)] = centre(scene.render.filepath)
            print("MIX-TEXTURED", textured, amount, shots[(textured, amount)])
            for ob in [o for o in scene.objects if o.name.startswith(("ball", "Sun"))]:
                bpy.data.objects.remove(ob)
    # the plain end shows where its amount is full, whichever end holds the texture; the textured end differs from it
    assert shots[("first", 1.0)][2] > shots[("first", 1.0)][0], "second (blue) at 1: %r" % (shots,)
    assert shots[("second", 0.0)][0] > shots[("second", 0.0)][2], "first (red) at 0: %r" % (shots,)
    assert shots[("first", 0.0)] != shots[("first", 1.0)] and shots[("second", 0.0)] != shots[("second", 1.0)], shots
    (r0, _g0, b0), (rh, _gh, bh), (r1, _g1, b1) = shots[0.0], shots[0.5], shots[1.0]
    assert r0 > b0 and b1 > r0 - 1 and b1 > r1, "first is red, second blue: %r" % (shots,)
    assert r0 > rh > r1 and b0 < bh < b1, "0.5 is between the two: %r" % (shots,)
    print("MIX-MATERIALS-OK")
