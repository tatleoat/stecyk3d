# SPDX-License-Identifier: GPL-3.0-or-later
# (what an import makes for each choice: the materials, the world and its sky, the objects in the
#       document's order with groups as empties and their members as children, the sun as one lamp, the camera in
#       use as the scene's camera; a .br5 whole import sets the program to 5.0; the document read whole before
#       anything is made; a second whole import refused before anything is made)
# (an import)
"""What importing a document makes, as a plan of plain values. Pure Python."""

import math

from . import components
from .create import terrain_scales
from .numbers import single
from .reader import heights_of
from .solids import BOX_FACTOR, BOX_SHAPES

SUN_IN_VIEW_BIT = 0x200  # [format]
PROGRAMS = ("3.0", "3.1", "5.0")  # program (the order of the reader's `worlds`)
MIN_PICTURE_SIDE = 4  # Blender's smallest render width and height
WHOLE_IMPORT_MARK = "stecyk3d_whole_import"  # our own custom property on the scene
FOV_SHARE = 0.8
PLANE_HEIGHT = 1e-5
SENSOR_WIDTH = 36.0  # Blender's default sensor width; the angle is what matters
LAMP_TYPES = {"radial": "POINT", "cone": "SPOT", "pyramid": "SPOT", "box": "AREA", "cylinder": "AREA"}
SHAPE_KINDS = ("sphere", "cube", "cylinder", "cone", "plane", "torus", "slab", "square", "pyramid")


def is_five_document(path):
    """A `.br5` document (the older programs cannot open it)."""
    return str(path).lower().endswith(".br5")


def swap_yz(v):
    return [v[0], v[2], v[1]]


def mesh_shading_plan(mesh):
    """A mesh's mixed normals and texture coordinates ('s kinds, as the reader gives them) for Blender: per
    triangle, in the plan's (opposite) corner order, `smooth` (flag word bit 0), `corner_normals` (three vectors in
    Blender's axes, or None) and `corner_uvs` (three pairs with v counted from the bottom, or None). Empty for a
    mesh that carries none."""
    flags = mesh.get("flags")
    if not flags or not any(flags):
        return {}
    normals, uvs = mesh.get("normals") or [], mesh.get("uvs") or []
    order = (0, 2, 1)
    smooth, corner_normals, corner_uvs = [], [], []
    for k, flag in enumerate(flags):
        ids = (mesh.get("corner_normals") or [None] * len(flags))[k] if flag & 1 else None
        uv_ids = (mesh.get("corner_uvs") or [None] * len(flags))[k] if flag & 2 else None
        smooth.append(bool(flag & 1 and ids is not None))
        corner_normals.append(None if ids is None else [swap_yz(normals[ids[j]]) for j in order])
        corner_uvs.append(None if uv_ids is None else
                          [[single(uvs[uv_ids[j]][0]), 1.0 - single(uvs[uv_ids[j]][1])] for j in order])
    return {"smooth": smooth, "corner_normals": corner_normals, "corner_uvs": corner_uvs}


def blender_matrix(world, units):
    """taken back:'s twelve numbers (rows x, y, z, translation) in the description's world to the
    Blender matrix whose columns X, Y, Z, T give them by."""
    u = single(units)
    rx, ry, rz, t = world[0:3], world[3:6], world[6:9], world[9:12]
    X = [v / u for v in swap_yz(rx)]
    Z = [v / u for v in swap_yz(ry)]
    Y = [v / u for v in swap_yz(rz)]
    T = [v / u for v in swap_yz(t)]
    return [[X[i], Y[i], Z[i], T[i]] for i in range(3)] + [[0.0, 0.0, 0.0, 1.0]]


def unscale_terrain(matrix, n, mirrored):
    """A terrain's Blender matrix: the world matrix of its own space with the terrain scaling taken off
    (the terrain's own box is 8 by 8 by 2 Blender units at scale 1)."""
    sx, sz = terrain_scales(n, mirrored)
    out = [list(r) for r in matrix]
    for r in range(3):
        out[r][0] /= sx
        out[r][1] /= sx
        out[r][2] /= sz
    return out


def _unit(v):
    length = math.sqrt(sum(c * c for c in v))
    if not length > 0.0:
        raise ValueError("a direction of length 0")
    return [c / length for c in v]


def _cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def axis_matrix(z_axis, location):
    """A turn whose own z axis is `z_axis` (Blender's axes), at `location`."""
    z = _unit(z_axis)
    helper = [0.0, 0.0, 1.0] if abs(z[2]) < 0.9 else [0.0, 1.0, 0.0]
    x = _unit(_cross(helper, z))
    y = _cross(z, x)
    return [[x[i], y[i], z[i], location[i]] for i in range(3)] + [[0.0, 0.0, 0.0, 1.0]]


def sun_lamp_data(sky):
    """The sun lamp's colour is the document's sun light part's (which the wash of reads): the sky's
    own sun colour is another number and stays in the World group. None given by an older reader: the lamp's own."""
    colour = sky.get("sun_light_color")
    if not colour or any(c is None or c != c or abs(c) == float("inf") for c in colour):
        return {}
    return {"color": [min(max(float(c), 0.0), 1.0) for c in colour]}


def camera_plan(camera, program, picture_size, units):
    """taken into Blender: the eye, a camera looking along the view's forward with its up, the angle the
    plane's width spans (0.8 of the field of view, widened by the zoom), the pan as Blender's shift."""
    view = camera["views"][PROGRAMS.index(program)]
    if view is None:
        view = next((v for v in camera["views"] if v is not None), None)
    if view is None:
        return None
    u = single(units)
    forward = _unit(swap_yz(view["forward"]))
    up = swap_yz(view["up"])
    dot = sum(a * b for a, b in zip(up, forward))
    up = _unit([a - dot * b for a, b in zip(up, forward)])
    z_col = [-c for c in forward]  # a Blender camera looks along its own -z, its up is +y
    x_col = _cross(up, z_col)
    width, height = picture_size
    fov = float(camera["field_of_view"])
    zoom = [float(z) if float(z) > 0.0 else 1.0 for z in camera["zoom"]]
    aspect = float(camera["aspect"]) if float(camera["aspect"]) > 0.0 else 1.0
    half_tan = math.tan(FOV_SHARE * fov / 2.0)
    h = PLANE_HEIGHT * (float(camera["scale"]) if program == "3.0" else 1.0)
    w = (width / height) * aspect * h
    distance = (w / 2.0) / half_tan if half_tan else 0.0
    # : a camera stored as a panorama (word bit 0x10000) is drawn as one, except as 3.1 and 5.0 where one named as
    # a view ("top", "right", ...) is read as parallel and so drawn in perspective . A panorama's plane centre
    # is the eye : the camera stands at the view's centre.
    panorama = bool(int(camera.get("projection_word") or 0) & 0x10000) and not (
        program != "3.0" and camera.get("name") in ("top", "right", "front", "left", "back", "bottom"))
    if panorama:
        distance = 0.0
    eye_bryce = [c - f * distance for c, f in zip(view["centre"], _unit(view["forward"]))]
    location = [v / u for v in swap_yz(eye_bryce)]
    angle = 2.0 * math.atan(half_tan * zoom[0])
    ratio = aspect * zoom[0] / zoom[1]  # the plane's width to height over the picture's
    pixel_aspect = (ratio, 1.0) if ratio >= 1.0 else (1.0, 1.0 / ratio)
    a = (width * pixel_aspect[0]) / (height * pixel_aspect[1])
    pan = camera["pan"]
    return {
        "matrix": [[x_col[i], up[i], z_col[i], location[i]] for i in range(3)] + [[0.0, 0.0, 0.0, 1.0]],
        "sensor_fit": "HORIZONTAL", "sensor_width": SENSOR_WIDTH,
        "lens": SENSOR_WIDTH / (2.0 * math.tan(angle / 2.0)),
        "shift_x": -float(pan[0]), "shift_y": float(pan[1]) / a,  # the inverse of camera_frame.renderer_view
        "resolution": [int(width), int(height)], "pixel_aspect": list(pixel_aspect),
        "name": camera.get("name", "camera"),
        "panorama": panorama,
        # the view's rows and centre exactly as the document holds them (a row may be scaled, e.g. right = 1.5, which a
        # Blender camera cannot carry): kept beside the camera, sent again while it is untouched (import_op, gather)
        "view_rows": [[float(c) for c in view[k]] for k in ("right", "up", "forward")],
        "view_centre": [float(c) for c in view["centre"]],
        "plane_height": h if program == "3.0" and h != PLANE_HEIGHT else None,  # 3.0's size , for the eye's distance
    }


def sun_in_world(rows, s):
    """: the sun's direction S' in the world, from S given in the view's frame: the inverse of the camera's
    matrix (rows right, up, forward) applied to S as a column (the transpose where the rows are a turn)."""
    (a, b, c), (d, e, f), (g, h, i) = rows
    cof = [[e * i - f * h, -(d * i - f * g), d * h - e * g],
           [-(b * i - c * h), a * i - c * g, -(a * h - b * g)],
           [b * f - c * e, -(a * f - c * d), a * e - b * d]]  # cofactors of the rows; the inverse is its transpose over det
    det = a * cof[0][0] + b * cof[0][1] + c * cof[0][2]
    if not det or det != det:
        return list(s)
    return [sum(cof[k][r] * s[k] for k in range(3)) / det for r in range(3)]


def sky_plan(sky, edge_noise=None, program="3.0", camera=None):
    """: the World's Bryce sky settings by their names, from the document's sky and sun. `edge_noise`: the
    reader's {"3.0": [amount, second], "3.1": [...]} for the document's sky at the document's time."""
    settings = {}
    for key, value in sky.items():
        if key in ("sun_direction_bryce", "stratus_texture", "cumulus_texture"):
            continue
        settings[key] = value
    direction = list(sky["sun_direction_bryce"])
    if int(sky.get("flags") or 0) & SUN_IN_VIEW_BIT and camera is not None:
        # : bit 0x200 says the direction is in the camera's frame. The add-on's sky word has the bit cleared and
        # gives the sun as a lamp's direction in the world, so the import turns it into the world's (found by
        # tests/blender_vs_bryce: every document with the bit set was lit from the wrong side).
        view = camera["views"][PROGRAMS.index(program)]
        if view is None:
            view = next((v for v in camera["views"] if v is not None), None)
        if view is not None:
            direction = sun_in_world([view["right"], view["up"], view["forward"]], direction)
        settings["flags"] = int(sky["flags"]) & ~SUN_IN_VIEW_BIT
    exact = {k: list(v) for k, v in settings.items()
             if isinstance(v, (list, tuple)) and any(isinstance(x, float) and not math.isfinite(x) for x in v)}
    if exact:
        settings["exact_numbers"] = exact  # not a setting: world_for keeps it where Blender's floats cannot
    settings["sun_direction"] = swap_yz(direction)  # : in Blender's axes
    # the document's own direction (Bryce's axes, not made unit: sw-05 and sw-06 hold one of length 1.99), kept
    # beside the world: says a scene brought in whole sends the document's direction to the last bit
    settings["sun_direction_exact"] = [float(c) for c in direction]
    settings["has_stratus"] = sky.get("stratus_texture") is not None
    settings["has_cumulus"] = sky.get("cumulus_texture") is not None
    # (`detail` and `detail_z`: no field of a document holds them; an import sets them to what the
    # reader gives for the document's sky at the document's time,'s wind: 0 and 0 for a sky whose tracks are
    # not applied; as 5.0 not used, so 0 and 0).
    detail = (edge_noise or {}).get(program, [0.0, 0.0])
    settings["detail"], settings["detail_z"] = float(detail[0]), float(detail[1])
    return settings


def sky_texture_components(document):
    """A document's cloud textures as components, by layer: the sky's `stratus_texture` and `cumulus_texture` name
    places in the reader's `textures`. The import links each to the Bryce Sky node's input of that name
    blender-addon.md, Sky cloud textures), so the cloud shape is not dropped."""
    textures, pictures = document.get("textures", []), document.get("pictures", [])
    sky, out = document["sky"], {}
    for layer in ("stratus", "cumulus"):
        index = sky.get(layer + "_texture")
        if index is not None and 0 <= index < len(textures):
            t = textures[index]
            comp = components.component_from_texture(t, t.get("name", "") or "sky " + layer, pictures)
            comp["column"] = 0
            out[layer] = comp
    return out


def beam_lamp(kind, world, units):
    """'s table taken back: a beam light's own transform (the reader's twelve numbers, rows x, y, z, place) as
    the Blender matrix, the lamp's data and the Bryce Light's reach that gather turns into the same twelve
    numbers. The lamp's spread is fixed at one (a spot's angle 2*atan(1/2), an area's size 1) and the reach is
    the length of the beam row; every other size or skew stays in the matrix' columns, which multiplies in.
    Returns (matrix, lamp data, reach)."""
    u = single(units)
    ax, ay, az = swap_yz(world[0:3]), swap_yz(world[3:6]), swap_yz(world[6:9])
    reach = single(math.sqrt(sum(c * c for c in ay))) or 1.0
    if kind in ("cone", "pyramid"):
        theta = single(2.0 * math.atan(0.5))
        s = 2.0 * math.tan(theta / 2.0)  # as lamps.beam_spread: not rounded to a single
        data = {"spot_size": theta, "use_square": kind == "pyramid"}
    else:
        s = 1.0
        data = {"shape": "DISK" if kind == "cylinder" else "SQUARE", "size": s}
    c1, c2, c3 = {"cone": (s * reach, reach, s * reach), "pyramid": (2 * s * reach, 2 * reach, 2 * s * reach),
                  "cylinder": (s / 2.0, reach, s / 2.0), "box": (s, 2.0 * reach, s)}[kind]
    X = [v / c1 for v in ax]
    Z = [v / c2 for v in ay]
    Y = [v / c3 for v in az]
    T = [p + z * reach for p, z in zip(swap_yz(world[9:12]), Z)]  # the place q = T - Z*r, taken back
    columns = [[v / u for v in col] for col in (X, Y, Z, T)]
    matrix = [[columns[j][i] for j in range(4)] for i in range(3)] + [[0.0, 0.0, 0.0, 1.0]]
    return matrix, data, reach


def light_plan(light):
    """: a lamp's kind and its Bryce Light settings by their names."""
    settings = {k: v for k, v in light.items() if k not in ("beam",)}
    settings["tint"] = True  # : the colour as it shines is the own colour times the material's Diffuse A
    return LAMP_TYPES.get(light.get("beam", "radial"), "POINT"), settings


def used_materials(document):
    """The materials the document's objects name (their placements' and the trees' leaves'), in class order:
    's materials (revised at): a material no object, light or tree names is not made."""
    used = set()
    for o in document["objects"]:
        if o.get("material") is not None:
            used.add(o["material"])
        if o.get("tree") and o["tree"].get("leaves_material") is not None:
            used.add(o["tree"]["leaves_material"])
    return sorted(used)


def object_plan(o, program, units):
    """One document object as a Blender object: kind, name, matrix, Bryce marks and settings."""
    world = o["worlds"][PROGRAMS.index(program)]
    if world is None:
        world = next((w for w in o["worlds"] if w is not None), None)
    matrix = blender_matrix(world, units) if world is not None else None
    kind = o["kind"]
    if matrix is not None and kind in BOX_SHAPES:
        # the stand-in's own box is Blender's -1 to 1, Bryce's cube and pyramid are 1 across: half the axes here,
        # as the description's gather doubles them (found by tests/blender_vs_bryce: every imported cube was 2x)
        matrix = [[c / BOX_FACTOR for c in row[:3]] + row[3:] for row in matrix[:3]] + [matrix[3]]
    plan = {"kind": kind, "name": o["name"], "matrix": matrix, "material": o.get("material"),
            "index": o["index"], "group": o.get("group")}
    if o.get("group") is not None:  # : a member's role and cut switch (the reader gives them for members)
        plan["role"] = o.get("role", "NEUTRAL")
        plan["cut_shows_positive"] = bool(o.get("cut_shows_positive"))
    flags = o.get("placement_flags")
    if flags and any(flags):
        # : the placement's flag words (hidden bits and the like) ride along on the object (`bryce_flags`);
        # a member's role bits are the object's role, a light's second word its switches
        first, second, third = (int(w) for w in flags)
        if kind == "light":
            second = 0
        elif o.get("group") is not None:
            second &= ~(0x38 | (0x80 if o.get("role") == "NEGATIVE" else 0))
        if first or second or third:
            plan["placement_flags"] = [first, second, third]
    if kind == "metaball":
        plan["negative"] = bool(o.get("negative"))  # bryce_negative: the metaball's sign
    if kind in SHAPE_KINDS:
        plan.update(object_type="MESH", bryce_shape=kind)
        if kind == "torus" and o.get("torus"):
            plan["torus"] = list(o["torus"])
    elif kind == "terrain":
        t = o["terrain"]
        n = t["size"]
        if matrix is not None:
            plan["matrix"] = unscale_terrain(matrix, n, t["mirrored"])
        plan.update(object_type="MESH", bryce_shape="terrain", size=n, heights=heights_of(t),
                    terrain={"grid": "AUTO", "lower": single(t["lower"]), "upper": single(t["upper"]),
                             "mirrored": bool(t["mirrored"]), "smooth": bool(t["smooth"]),
                             "faces": t["faces"], "solid_faces": t["solid_faces"]})
    elif kind == "tree":
        plan.update(object_type="MESH", bryce_shape="tree", leaves_material=o["tree"].get("leaves_material"),
                    stored=o["tree"].get("stored"),
                    stored_id=o["tree"].get("stored_id"))
    elif kind == "group":
        plan.update(object_type="EMPTY", bryce_shape="group")
    elif kind == "light":
        lamp, settings = light_plan(o["light"])
        plan.update(object_type="LIGHT", lamp=lamp, light=settings)
        if o["light"].get("beam", "radial") != "radial" and world is not None:
            plan["matrix"], plan["lamp_data"], settings["reach"] = beam_lamp(o["light"]["beam"], world, units)
    elif kind == "mesh":
        mesh = o.get("mesh") or {"points": [], "triangles": []}
        plan.update(object_type="MESH", points=[swap_yz(p) for p in mesh["points"]],
                    triangles=[[t[0], t[2], t[1]] for t in mesh["triangles"]])  # : the opposite order
        plan.update(mesh_shading_plan(mesh))
    elif kind == "metaball":
        plan.update(object_type="MESH", bryce_shape="metaball")
    else:  # a path: its own part is Open ; an empty stands for it
        plan.update(object_type="EMPTY")
    if plan["matrix"] is None:
        plan["matrix"] = [[1.0, 0, 0, 0], [0, 1.0, 0, 0], [0, 0, 1.0, 0], [0, 0, 0, 1.0]]
        plan["note"] = "its place is not carried (Open); put at the origin"
    return plan


def plan_import(path, document, refusal, options, scene):
    """: `document` is the reader's JSON, or None with `refusal` its reason. `options`: sky, materials,
    objects. `scene`: program, has_world, whole_import_done, units. Returns {"result": "refused", "reason"} or
    the plan of what to make."""
    whole = bool(options.get("objects"))
    if whole and scene.get("whole_import_done"):
        return {"result": "refused",
                "reason": "this scene already holds a document brought in whole; bring another into a scene of "
                          "its own"}
    if document is None:
        return {"result": "refused", "reason": refusal or "the document could not be read"}
    if whole and min(int(x) for x in document.get("picture_size", (MIN_PICTURE_SIDE,) * 2)) < MIN_PICTURE_SIDE:
        # Blender's render size is at least 4 by 4 (found by tests/blender_vs_bryce: a 1 by 1 document). Scaling the
        # picture up would hand back a different picture, so the import says so and makes nothing.
        width, height = document["picture_size"]
        return {"result": "refused",
                "reason": "the document's picture is %d by %d pixels and Blender's render size is at least %d by %d; "
                          "nothing was imported" % (width, height, MIN_PICTURE_SIDE, MIN_PICTURE_SIDE)}
    program = scene.get("program", "3.0")
    if whole and is_five_document(path):
        program = "5.0"  # : the older programs cannot open a .br5
    units = scene.get("units", 1.0)
    with_materials = whole or bool(options.get("materials"))
    with_sky = whole or bool(options.get("sky"))
    textures, pictures = document.get("textures", []), document.get("pictures", [])
    materials = []
    if with_materials:
        for index in used_materials(document):
            m = document["materials"][index]
            materials.append({"index": index, "name": m["name"],
                              "bryce": components.material_from_record(m, textures, pictures)})
    plan = {"result": "imported", "program": program, "materials": materials,
            "world": ("fill" if scene.get("has_world") else "make") if with_sky else None,
            "sky": sky_plan(document["sky"], document.get("edge_noise"), program, document.get("camera")) if with_sky else None,
            "sky_textures": sky_texture_components(document) if with_sky else {},
            "objects": [], "camera": None,
            "mark_whole": whole, "pictures": pictures if with_materials else [],
            "render": render_plan(document.get("settings")) if whole else None}
    if whole:
        for o in document["objects"]:
            if o["kind"] == "camera":
                continue  # the camera in use is made below, as the scene's (the reader lists no other)
            if any(w is not None and any(v is None for v in w) for w in [o["worlds"][PROGRAMS.index(program)]]):
                continue  # a track key placed it at a number that is not one (null): nothing of it is drawn
            plan["objects"].append(object_plan(o, program, units))
        plan["objects"].append({"kind": "light", "name": "sun", "object_type": "LIGHT", "lamp": "SUN",
                                "matrix": axis_matrix(plan["sky"]["sun_direction"], [0.0, 0.0, 0.0]),
                                "lamp_data": sun_lamp_data(document["sky"]), "light": None, "material": None, "index": None, "group": None})
        plan["camera"] = camera_plan(document["camera"], program, document["picture_size"], units)
    return plan


# A Blender int property holds at most this; a document's depth word may be larger (dw-ray-depth-mirrors-4294967295
# wrapped to a negative number that the renderer refuses). Both are far past any real depth, so the clamp draws the same.
BLENDER_INT_MAX = 2**31 - 1


def render_plan(settings):
    """: the document's own render settings by their names (the options a Blender scene holds): the
    anti-aliasing and spatial options and, from a document with Bryce 5's part, its rays, depths and structure kind.
    tests/blender_vs_bryce (a document drawn as it was saved)"""
    if not settings:
        return None
    out = {"antialias": str(settings["antialias"]), "spatial_level": str(settings["spatial"])}
    if "ray_depth" in settings:
        out.update(super_rays=settings["rays_a_side"], premium_rays=settings["premium_rays"],
                   depth=min(settings["ray_depth"], BLENDER_INT_MAX),
                   tir_depth=min(settings["tir_depth"], BLENDER_INT_MAX))
        # a code that is neither bspT nor grid has no Blender choice; the renderer draws it like the grid
        # (twin-toruses with code zzzz: grid matches the document's own picture exactly, the tree does not)
        out["spatial_kind"] = "TREE" if settings.get("structure") == "bspT" else "GRID"
    return out


def made_counts(plan):
    """What the plan makes as the vectors count it: objects by kind, the camera, materials, the world."""
    counts = {}
    for o in plan.get("objects", []):
        counts[o["kind"]] = counts.get(o["kind"], 0) + 1
    if plan.get("camera") is not None:
        counts["camera"] = counts.get("camera", 0) + 1
    return counts


def group_members(plan):
    """The kinds of each made group's children, groups in the plan's order."""
    out = []
    for o in plan.get("objects", []):
        if o["kind"] == "group":
            out.append([c["kind"] for c in plan["objects"] if c.get("group") == o["index"]])
    return out
