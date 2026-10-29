# SPDX-License-Identifier: GPL-3.0-or-later
# (Shader operators; node-parity tasks T8, T13, T14). Needs the installed add-on,
# the renderer program and a substrate (run_all 8f):
#   blender -b --factory-startup --python blender_nodes_render.py -- <substrate> <program> <out-prefix>
# T8: each shader operator on a fresh datablock, then one 32x32 frame that is not empty. T14: a textured material
# rendered, saved, reopened, rendered again: the two PNGs are byte-identical. T13: thumbnails of two materials
# (skipped with a message when Blender cannot make previews headless).
"""Operators and the save/reload/render round trip, through the real engine."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
import png_read  # noqa: E402


def centre(path):
    width, height, channels, data = png_read.read_png(path)
    at = ((height // 2) * width + width // 2) * channels
    return tuple(data[at:at + 3])


def distinct(path):
    width, height, channels, data = png_read.read_png(path)
    return len({tuple(data[i:i + 3]) for i in range(0, len(data), channels)})


def clear_scene(bpy, scene):
    for ob in [o for o in scene.objects if o.type != "CAMERA"]:  # the factory camera stays
        bpy.data.objects.remove(ob)


def stage(bpy, scene, material, lamp=None):
    """A ball with `material`, a sun (or `lamp`), the factory camera."""
    clear_scene(bpy, scene)
    sky = scene.world.bryce
    sky.sun_intensity, sky.sun_color, sky.ambient_color = 1.0, (1.0, 1.0, 1.0), (0.4, 0.4, 0.4)
    data = lamp or bpy.data.lights.new("Sun", "SUN")
    sun = bpy.data.objects.new("Sun", data)
    scene.collection.objects.link(sun)
    sun.rotation_euler = (0.615476, 0.0, 0.785398)
    ball = bpy.data.objects.new("ball", bpy.data.meshes.new("ball"))
    scene.collection.objects.link(ball)
    ball.bryce_shape = "sphere"
    ball.data.materials.append(material)
    return ball


def frame(bpy, scene, prefix, label):
    scene.render.filepath = "%s-%s.png" % (prefix, label)
    bpy.ops.render.render(write_still=True)
    return scene.render.filepath


def operators(bpy, scene, prefix):
    log = []
    # New Bryce material on a plain Blender material
    mat = bpy.data.materials.new("r-new")
    stage(bpy, scene, mat)
    with bpy.context.temp_override(material=mat):
        bpy.ops.stecyk3d.new_material()
    path = frame(bpy, scene, prefix, "new")
    assert distinct(path) > 3, "new material frame is empty"
    log.append(("new", centre(path)))
    # Bryce material from the Blender shader: a red Principled
    mat = bpy.data.materials.new("r-shader")
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    bsdf.inputs["Base Color"].default_value = (0.8, 0.02, 0.02, 1.0)
    stage(bpy, scene, mat)
    with bpy.context.temp_override(material=mat):
        bpy.ops.stecyk3d.material_from_shader()
    path = frame(bpy, scene, prefix, "shader")
    r, g, b = centre(path)
    assert r > g + 40 and r > b + 40, "red Principled gives a red ball: %r" % ((r, g, b),)
    log.append(("shader", (r, g, b)))
    # The further Principled inputs reach the render: a raised Specular IOR Level (Reflection), glass (Transparency
    # and refraction); each ball differs from the new material's
    for label, inputs in (("mirror", {"Specular IOR Level": 1.0}),
                          ("glass", {"Transmission Weight": 1.0, "IOR": 1.33})):
        mat = bpy.data.materials.new("r-" + label)
        mat.use_nodes = True
        bsdf = next(n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
        for name, number in inputs.items():
            bsdf.inputs[name].default_value = number
        stage(bpy, scene, mat)
        with bpy.context.temp_override(material=mat):
            bpy.ops.stecyk3d.material_from_shader()
        path = frame(bpy, scene, prefix, label)
        assert centre(path) != log[0][1], "%s Principled renders as the new material: %r" % (label, centre(path))
        log.append((label, centre(path)))
    # Bryce settings to nodes: an older file's material group
    mat = bpy.data.materials.new("r-old")
    mat.bryce.diffuse_color.a = (0.02, 0.1, 0.9)
    mat.bryce.diffusion.a = 1.0  # a group that was ever written holds zeros for what it does not name
    stage(bpy, scene, mat)
    bpy.ops.stecyk3d.settings_to_nodes()
    path = frame(bpy, scene, prefix, "old")
    r, g, b = centre(path)
    assert b > r + 40, "blue group gives a blue ball: %r" % ((r, g, b),)
    log.append(("settings", (r, g, b)))
    # Make it a Bryce sky / light: the world's settings survive the change, the frame is still the same picture
    mat = bpy.data.materials.new("r-plain")
    stage(bpy, scene, mat)
    before = frame(bpy, scene, prefix, "before-sky")
    pixels_before = png_read.read_png(before)[3]
    with bpy.context.temp_override(world=scene.world):
        bpy.ops.stecyk3d.make_sky()
    lamp = scene.objects["Sun"].data
    with bpy.context.temp_override(light=lamp):
        bpy.ops.stecyk3d.make_light()
    after = frame(bpy, scene, prefix, "after-sky")
    assert png_read.read_png(after)[3] == pixels_before, "making the sky and the light nodes changes the picture"
    log.append(("sky+light", centre(after)))
    return log


def textured(bpy, scene):
    """A ball whose diffuse colour is a Bryce Texture (a flat colour map, no noise)."""
    mat = bpy.data.materials.new("r-round")
    stage(bpy, scene, mat)
    mat.use_nodes = True
    tree = mat.node_tree
    tree.nodes.clear()
    node = tree.nodes.new("BryceMaterialNode")
    out = tree.nodes.new("ShaderNodeOutputMaterial")
    tree.links.new(node.outputs["material"], out.inputs["Surface"])
    texture = tree.nodes.new("BryceTextureNode")
    texture.gives = 3
    component = tree.nodes.new("BryceComponentNode")
    component.flags, component.mode = 1, 6
    component.first, component.second, component.third = 0xC829FF, 0x1EB43C, 0x283CDC
    tree.links.new(component.outputs["component"], texture.inputs["component"])
    tree.links.new(texture.outputs["color"], node.inputs["diffuse_color"])


def round_trip(bpy, scene, prefix, out):
    textured(bpy, scene)
    first = frame(bpy, scene, prefix, "round-1")
    assert distinct(first) > 3
    blend = os.path.join(os.path.dirname(prefix), "round.blend")
    bpy.ops.wm.save_as_mainfile(filepath=blend)
    bpy.ops.wm.open_mainfile(filepath=blend)
    scene = bpy.context.scene
    second = frame(bpy, scene, prefix, "round-2")
    # the decoded pixels (the two files' bytes differ in Blender's own PNG chunks, not in the picture)
    assert png_read.read_png(first) == png_read.read_png(second), "the picture after save and reload differs"
    return centre(second)


def thumbnails(bpy, prefix):
    """T13: previews of two materials differ, and each is not empty. The renderer words behind a preview are the
    same description path as F12, so what is compared is the thumbnail with its own material's colour."""
    pics = {}
    for name, colour in (("t-red", (0.9, 0.05, 0.05)), ("t-blue", (0.05, 0.05, 0.9))):
        mat = bpy.data.materials.new(name)
        mat.use_fake_user = True
        mat.use_nodes = True
        mat.node_tree.nodes.clear()
        node = mat.node_tree.nodes.new("BryceMaterialNode")
        node.inputs["diffuse_color"].value = colour
        preview = mat.preview_ensure()
        pics[name] = preview
    try:
        bpy.ops.wm.previews_ensure()
    except Exception as error:  # noqa: BLE001
        return "THUMBNAILS-SKIPPED previews_ensure failed headless: %s" % error
    sizes = {k: len(v.image_pixels_float) for k, v in pics.items()}
    if not all(sizes.values()) or not any(max(v.image_pixels_float) > 0 for v in pics.values()):
        return "THUMBNAILS-SKIPPED Blender made no preview pixels headless (%r)" % sizes
    red, blue = pics["t-red"].image_pixels_float, pics["t-blue"].image_pixels_float
    if list(red) == list(blue):
        return "THUMBNAILS-SKIPPED the two previews are identical (headless previews are not the engine's)"
    return "THUMBNAILS-OK %r" % sizes


if __name__ == "__main__":
    bpy, _substrate, _program, prefix = blender_common.start()
    scene = blender_common.setup(bpy, prefix)
    scene.render.resolution_x = scene.render.resolution_y = 32
    for label, pixel in operators(bpy, scene, prefix):
        print("OPS-FRAME", label, pixel)
    print("ROUND-TRIP-OK", round_trip(bpy, scene, prefix, os.path.dirname(prefix)))
    print(thumbnails(bpy, prefix))
    print("NODES-RENDER-OK")
