# SPDX-License-Identifier: GPL-3.0-or-later
# (Legacy properties). Needs the installed add-on, the renderer program and a
# substrate (run_all 8g):
#   blender -b --factory-startup --python blender_legacy.py -- <substrate> <program> <out-prefix>
# A file of the earlier add-on keeps an object's Bryce settings as plain custom properties ("bryce_shape" =
# "metaball"); Blender 5 holds registered properties apart from those, so they must be read from both. Checks: the
# reading, the copy into the registered properties, a texture column that every node of the file reads as 0, the
# grown tree the earlier add-on keeps with the scene, and a glass metaball picture that is not the opaque one.
"""Plain custom properties of the earlier add-on are read as the settings they stand for."""

import base64
import json
import os
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
import png_read  # noqa: E402

CHECKS = [0]


def check(condition, what):
    CHECKS[0] += 1
    if not condition:
        print("LEGACY-FAIL", what)
        sys.exit(1)


def pixels(path):
    width, height, channels, data = png_read.read_png(path)
    return [tuple(data[i:i + 3]) for i in range(0, len(data), channels)]


def make_object(bpy, scene, name, shape, location):
    obj = bpy.data.objects.new(name, bpy.data.meshes.new(name))
    scene.collection.objects.link(obj)
    obj["bryce_shape"] = shape  # the plain custom property, as the earlier add-on's files hold it
    obj.location = location
    return obj


def glass(bpy, name, transparency):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    tree = material.node_tree
    tree.nodes.clear()
    node = tree.nodes.new("BryceMaterialNode")
    out = tree.nodes.new("ShaderNodeOutputMaterial")
    tree.links.new(node.outputs["material"], out.inputs["Surface"])
    node.inputs["diffuse_color"].value = (0.2, 0.9, 0.3)
    node.inputs["transparency"].value = transparency
    node.inputs["reflection"].value = 0.1
    node.refraction = 1.4
    return material


def main():
    bpy, substrate, program, prefix = blender_common.start()
    addon = sys.modules[blender_common.ADDON]
    gather, material_nodes, props = addon.gather, addon.material_nodes, addon.props
    scene = blender_common.setup(bpy, prefix)
    for ob in [o for o in scene.objects if o.type != "CAMERA"]:
        bpy.data.objects.remove(ob)
    scene.bryce_render.program = "5.0"  # a metaball is only rendered as Bryce 5
    sky = scene.world.bryce
    sky.sky5 = True  # the 5.0 sky word the program's defaults brought
    sky.sun_intensity, sky.sun_color, sky.ambient_color = 1.0, (1.0, 1.0, 1.0), (0.4, 0.4, 0.4)
    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.rotation_euler = (0.615476, 0.0, 0.785398)

    # 1. a plain custom property is read as the setting, by name, for shape, negative and role
    vine = make_object(bpy, scene, "Coil", "metaball", (0.0, 0.0, 0.0))
    ground = make_object(bpy, scene, "Ground", "plane", (0.0, 0.0, -1.2))
    ground.scale = (6.0, 6.0, 1.0)
    vine["bryce_negative"] = True
    check(props.legacy_get(vine, "bryce_shape") == "metaball", "the plain bryce_shape is read")
    check(props.legacy_get(vine, "bryce_negative") is True, "the plain bryce_negative is read")
    check(props.legacy_get(ground, "bryce_shape") == "plane", "the plain bryce_shape of a plane is read")
    check(props.legacy_get(ground, "bryce_negative") is False, "an absent plain bryce_negative is the default")
    vine["bryce_negative"] = False
    ground.data.materials.append(glass(bpy, "ground", 0.0))
    vine.data.materials.append(glass(bpy, "vine", 0.86))
    described, problems = gather.describe(bpy.context.evaluated_depsgraph_get())
    check(not problems, "describe has no problem: %r" % (problems,))
    shapes = {e["name"]: e["shape"] for e in described["objects"]}
    check(shapes.get("Coil") == "metaball" and shapes.get("Ground") == "plane",
          "the description holds the shapes the plain properties name: %r" % (shapes,))

    # 2. adoption copies the plain values into the registered properties (done when a file is loaded)
    props.adopt_legacy([vine, ground])
    check(vine.bryce_shape == "metaball" and ground.bryce_shape == "plane", "adopted into the registered setting")

    # 3. two texture nodes of one column (what the earlier add-on's file reads as) take separate columns
    material = bpy.data.materials.new("columns")
    material.use_nodes = True
    tree = material.node_tree
    first, second = tree.nodes.new("BryceTextureNode"), tree.nodes.new("BryceTextureNode")
    first.column = second.column = 0
    material_nodes.adopt_legacy_columns([material])
    check(sorted((int(first.column), int(second.column))) == [0, 1], "a clashing column moved to a free one")
    kept = {int(first.column), int(second.column)}
    material_nodes.adopt_legacy_columns([material])
    check({int(first.column), int(second.column)} == kept, "columns already apart stay as they are")

    # 3b. the earlier add-on's noise dimension is the document's (0 to 2): moved up to 1 to 3 once, in its files only
    dim_tree = bpy.data.node_groups.new("dims", "ShaderNodeTree")
    noises = [dim_tree.nodes.new("BryceNoiseNode") for _ in range(3)]
    for noise, dims in zip(noises, (0, 1, 2)):
        noise.dims = dims
    props.adopt_legacy_dimensions(bpy.data.scenes, bpy.data.node_groups, bpy.data.materials, bpy.data.worlds)
    check([n.dims for n in noises] == [0, 1, 2], "a file without the earlier add-on's keys keeps its dimensions")
    scene["bryce_settings"] = "{}"  # what only the earlier add-on's files hold
    moved = props.adopt_legacy_dimensions(bpy.data.scenes, bpy.data.node_groups, bpy.data.materials, bpy.data.worlds)
    check([n.dims for n in noises] == [1, 2, 3] and moved >= 3, "the earlier add-on's dimensions moved up by one")
    props.adopt_legacy_dimensions(bpy.data.scenes, bpy.data.node_groups, bpy.data.materials, bpy.data.worlds)
    check([n.dims for n in noises] == [1, 2, 3], "and not moved again after a save and a load")
    del scene["bryce_settings"], scene[props.LEGACY_DIMENSIONS_DONE]
    bpy.data.node_groups.remove(dim_tree)

    # 4. the grown tree the earlier add-on keeps with the scene, for an object it shows as a proxy
    grown = {"pieces": [{"parent": -1, "matrix": [1.0] * 12}], "leaves": [], "shapes": []}
    kept_desc = {"objects": [{"name": "Other", "shape": "sphere"}, {"name": "Tree", "shape": "tree",
                                                                    "tree": {"definition": 0}}], "trees": [grown]}
    scene["bryce_carried_data"] = base64.b64encode(zlib.compress(json.dumps(kept_desc).encode())).decode()
    tree_obj = bpy.data.objects.new("Tree", bpy.data.meshes.new("Tree"))
    tree_obj["bryce_proxy"] = "tree"
    tree_obj["bryce_index"] = 1
    check(props.kept_tree(scene, tree_obj) == grown, "the kept tree is found by its index")
    tree_obj["bryce_index"] = 7
    check(props.kept_tree(scene, tree_obj) == grown, "and by its name when the index does not fit")
    tree_obj.name = "Elsewhere"
    check(props.kept_tree(scene, tree_obj) is None, "no kept tree for a name the scene does not keep")
    del scene["bryce_carried_data"]
    check(props.kept_tree(scene, tree_obj) is None, "no kept tree without a kept description")

    # 5. a glass metaball picture is not the opaque one (the picture before the fix had every metaball opaque)
    glass_frame = blender_common_render(bpy, scene, prefix, "glass")
    vine.data.materials[0] = glass(bpy, "opaque", 0.0)
    opaque_frame = blender_common_render(bpy, scene, prefix, "opaque")
    differ = sum(1 for a, b in zip(pixels(glass_frame), pixels(opaque_frame)) if a != b)
    check(differ > 20, "the translucent metaball differs from the opaque one in %d pixels" % differ)

    # 6. a tree's textured wood beside metaballs is described as it is and drawn (it used to be sent as a
    # plain-colour copy with a notice). Only where a file is named by STECYK3D_VINE_FILE.
    vine_file = os.environ.get("STECYK3D_VINE_FILE", "")  # a saved scene with a stored tree beside metaballs
    if vine_file and os.path.exists(vine_file):
        bpy.ops.wm.open_mainfile(filepath=vine_file)
        scene = bpy.context.scene
        scene.render.resolution_x, scene.render.resolution_y = 96, 54
        described, problems = gather.describe(bpy.context.evaluated_depsgraph_get())
        stored = [t for t in described["trees"] if t.get("stored") is not None]
        check(stored and any(o.get("shape") == "metaball" for o in described["objects"]), "the file holds a stored tree and metaballs")
        for tree_entry in stored:
            wood = tree_entry["material"]
            check(not wood.endswith("(plain)"), "the wood material is not swapped for a plain copy: %r" % wood)
            check(gather.has_textures(described["materials"][wood]), "the wood keeps its textures")
        bark = blender_common_render(bpy, scene, prefix, "bark")
        check(os.path.getsize(bark) > 0, "the scene with textured wood beside metaballs is drawn")
    print("LEGACY-OK %d checks, %d pixels differ between glass and opaque" % (CHECKS[0], differ))


def blender_common_render(bpy, scene, prefix, label):
    scene.render.filepath = "%s-%s.png" % (prefix, label)
    bpy.ops.render.render(write_still=True)
    return scene.render.filepath


main()
