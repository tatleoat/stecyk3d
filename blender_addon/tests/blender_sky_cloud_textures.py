# SPDX-License-Identifier: GPL-3.0-or-later
# (Sky cloud textures). Headless: a Bryce Texture linked to the Bryce Sky
# node's `cumulus` input reaches the render: two different flat-colour textures give two different skies.
#   blender -b --factory-startup --python blender_sky_cloud_textures.py -- <substrate> <program> <out-prefix>
"""Cloud layer texture from a node: description carries `sky.cumulus_texture`, and the pictures differ."""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
import png_read  # noqa: E402


def sky_scene(bpy, scene, colours):
    """A sky with a lower cloud layer (flag 0x10), the camera looking up; `colours` the texture's noise
    frequency, or None for no texture."""
    world = scene.world
    world.use_nodes = True
    tree = world.node_tree
    tree.nodes.clear()
    sky_node = tree.nodes.new("BryceSkyNode")
    sky = sky_node.settings
    sky.flags = 0x3E000283 | 0x10 | 0x40  # with the clouds  and the lower layer on
    sky.sun_intensity, sky.sun_color, sky.ambient_color = 1.0, (1, 1, 1), (0.4, 0.4, 0.4)
    sky.cloud_height, sky.cover, sky.cover_amplitude, sky.cloud_frequency = 36.1328125, 0.5, 1.0, 0.25
    sky.cumulus_color, sky.stratus_color = (1, 1, 1), (1, 1, 1)
    if colours is not None:
        texture = tree.nodes.new("BryceTextureNode")
        texture.gives = 3
        component = tree.nodes.new("BryceComponentNode")
        component.flags, component.mode = 1, 0
        noise = tree.nodes.new("BryceNoiseNode")  # a noise whose frequency `colours` sets: the cloud shape moves with it
        noise.kind, noise.count, noise.dims, noise.frequency = int(os.environ.get("KIND", "1")), 1, 3, (colours, colours, colours)
        tree.links.new(noise.outputs["value"], component.inputs["value"])
        tree.links.new(component.outputs["component"], texture.inputs["component"])
        tree.links.new(texture.outputs["color"], sky_node.inputs["cumulus"])
    scene.camera.rotation_euler = (math.radians(150), 0.0, 0.0)
    scene.camera.location = (0.0, 0.0, 1.0)


def pixels(path):
    return png_read.read_png(path)[3]


if __name__ == "__main__":
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.render.resolution_x = scene.render.resolution_y = 32
    for old in [o for o in scene.objects if o.type == "MESH"]:
        bpy.data.objects.remove(old)
    shots = {}
    for label, colours in (("none", None), ("red", 1.0), ("blue", 4.0)):
        sky_scene(bpy, scene, colours)
        scene.render.filepath = "%s-cloud-%s.png" % (prefix, label)
        bpy.ops.render.render(write_still=True)
        shots[label] = pixels(scene.render.filepath)
    module = sys.modules[blender_common.ADDON]
    description, problems = module.gather.describe(bpy.context.evaluated_depsgraph_get())
    print("CLOUD-DESC", description["sky"].get("cumulus_texture"), [t["name"] for t in description["textures"]], problems)
    assert description["sky"].get("cumulus_texture") == 0 and description["textures"][0]["name"] == "sky:cumulus"
    assert shots["red"] != shots["blue"], "two cloud textures, two skies"
    print("SKY-CLOUD-TEXTURES-OK none==red:", shots["none"] == shots["red"])
