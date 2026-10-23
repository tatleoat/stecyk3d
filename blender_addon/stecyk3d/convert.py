# SPDX-License-Identifier: GPL-3.0-or-later
# BS-1 3.3 (a matrix of negative determinant reverses the winding of the triangles it carries)
# (Conversions)
"""Settings as plain values (dicts keyed by's saved names) to the description. Pure Python."""

import math

from . import environment, names
from .axes import (object_matrix, to_bryce_point, transform_point, triangle_order)
from .scene_read import is_mirrored
from .numbers import shortest_single, single, single_product, singles, word

GAMMA_BIT = 0x80000  # [format]
DITHERING_BIT = 0x40000  # [format]
DEPTH_OF_FIELD_BIT = 0x2000000  # focus (scene_flags bit) [format]
SKY_SUN_FROM_VIEW_BIT = 0x200  # flags (cleared: the add-on gives the sun's direction) [format]
RAYS_A_SIDE_THREE = 4  # super_rays (3.0 and 3.1 take 4)
PREMIUM = 4  # antialias (4 is Premium)
PREMIUM_AS_THREE = 2  # antialias (with a program other than 5.0, setting 4 is taken as 2)
FIVE = "5.0"  # program
MATERIAL_UNUSED_FLAG = 0x40  # [format]



def render_request(bryce_render, scene_flags, camera_focus=None):
    """The render request  from `bryce_render` and the scene's switch word (`scene_flags`).

    camera_focus: (focus distance, lens radius) in Blender units from the Blender camera, for focus CAMERA.
    """
    r = bryce_render
    program = r["program"]
    if program not in names.PROGRAM:
        raise ValueError("program %r is not one of 3.0, 3.1, 5.0" % (program,))
    five = program == FIVE
    flags = word(scene_flags)
    antialias = int(r["antialias"])
    if str(antialias) not in names.ANTIALIAS:
        raise ValueError("antialias %r is not one of 0, 1, 2, 4" % (r["antialias"],))
    if antialias == PREMIUM and not five:
        antialias = PREMIUM_AS_THREE
    out = {
        "version": program,
        "antialias": antialias,
        "rays_a_side": int(r["super_rays"]) if five else RAYS_A_SIDE_THREE,
        "premium_rays": int(r["premium_rays"]) if five else None,
        "tir_depth": int(r["tir_depth"]) if five and "tir_depth" in r else None,
        "structure": names.SPATIAL_KIND_CODE[r["spatial_kind"]] if five else None,
        "spatial": names.SPATIAL_LEVEL[str(r["spatial_level"])],
        "ray_depth": int(r["depth"]),
        "gamma": five and bool(flags & GAMMA_BIT),
        "dithering": five and bool(flags & DITHERING_BIT),
        "units": single(r.get("units", 1.0)),
        "preview": bool(r.get("preview", False)),
        # (the render program reads the switch word: the pass over
        # edges 0x2000, corner blocks 0x4000, depth of field 0x2000000). The settings vectors
        # walk the expected row's fields, so an added field keeps every vector.
        "switches": flags,
    }
    if five and antialias == PREMIUM and flags & DEPTH_OF_FIELD_BIT:
        u = single(r.get("units", 1.0))
        focus = r.get("focus", "CAMERA")
        if focus == "NUMBERS":
            out["focal_distance"] = single(r["focal_distance"]) * u
            out["lens_radius"] = single(r["lens_radius"]) * u
        elif camera_focus is not None:
            out["focal_distance"] = float(camera_focus[0]) * u
            out["lens_radius"] = float(camera_focus[1]) * u
    return out



SKY_FLOATS = (
    ("cloud_height", "cloud_height"), ("cover", "cover"), ("cover_amplitude", "amplitude"),
    ("cloud_frequency", "frequency"), ("shadow_lift", "shadows"), ("ambient_amount", "ambience"),
    ("haze", "haze"), ("fog_height", "fog_height"), ("fog", "fog"),
)
SKY_COLOURS = (
    ("cumulus_color", "cloud_colour"), ("stratus_color", "upper_cloud_colour"), ("zenith", "sky_colour"),
    ("horizon", "horizon_colour"), ("ambient", "dome_colour"), ("plain", "plain_colour"),
    ("ambient_color", "ambient_colour"), ("haze_color", "haze_colour"), ("fog_color", "fog_colour"),
)
SKY_FIVE_PREFIX = "sky5_"
BRYCE5_BITS = 0xC0000000  # flags (0x40000000 and 0x80000000 kept with sky5 on) [format]
SKY5_SINGLES = (
    ("sky5_sun_glow_size", "sun_glow_weight"), ("sky5_moon_glow_size", "moon_glow_weight"),
    ("sky5_sun_disc", "sun_disc_weight"), ("sky5_moon_disc", "moon_disc_weight"),
    ("sky5_star_intensity", "star_intensity"), ("sky5_comet_intensity", "comet_intensity"),
)
SKY5_WHEN_SET = (
    ("sky5_haze_offset", "haze_offset"), ("sky5_haze_thickness", "haze_thickness"),
    ("sky5_fog_offset", "fog_offset"),
)
SUN_FIVE = ("sun_radius", "sun_shadow_ambience", "sun_shadow_color")


def sky_flags(setting):
    """flags: the word with the sun-from-the-view bit cleared."""
    return word(setting) & ~SKY_SUN_FROM_VIEW_BIT & 0xFFFFFFFF


def sky_and_sun(sky, sun_axis=None):
    """The description's sky (fields) and sun  from a Bryce Sky node's settings .

    sun_axis: the sun lamp's own z axis in Blender's axes, if the scene has a sun lamp .
    """
    out = {"flags": sky_flags(sky["flags"])}
    for setting, field in SKY_FLOATS:
        out[field] = single(sky[setting])
    for setting, field in SKY_COLOURS:
        out[field] = singles(sky[setting])
    out["cloud_offset"] = singles(sky["cloud_offset"])
    out["ambient_colour_elsewhere"] = environment.cumulus_shade(sky)  # cumulus_shade
    for setting in ("detail", "detail_z"):
        if setting in sky:
            out[setting] = single(sky[setting])
    for setting in ("moon", "discs"):
        if setting in sky:
            out[setting] = singles(sky[setting])
    if "stars" in sky:
        out["stars"] = [word(w) for w in sky["stars"]]

    bryce5_part(sky, out)

    sun = {
        "colour": singles(sky["sun_color"]),
        "glow_colour": singles(sky["sun_glow_color"]),
        "direction": environment.sun_direction(sun_axis, sky["sun_direction"]),
        "intensity": single(sky["sun_intensity"]),
        "switches": word(sky["sun_flags"]),
    }

    five = {}
    for key, value in sky.items():
        if key.startswith(SKY_FIVE_PREFIX) or key in SUN_FIVE or key == "sky5":
            five[key] = _carry(value)
    result = {"sky": out, "sun": sun}
    if five:
        result["five"] = five
    if "scene_flags" in sky:
        result["scene_flags"] = word(sky["scene_flags"])
    return result


# The sky numbers a document holds as binary64 (scene_sky.rs `double`, `doubles`); a Blender float property keeps a
# single, which cut the fog 0.005 to 0.004999999888 and with it's whole number F (bit 0x80 of the opened word)
# from 1 to 0. The import keeps the document's number beside the world (import_op.keep_exact_sky) and describe sends it
# again while the property still holds the single the import gave. setting -> (part of the result, field).
SKY_DOUBLES = {setting: ("sky", field) for setting, field in SKY_FLOATS}
SKY_DOUBLES.update({
    "cloud_offset": ("sky", "cloud_offset"), "moon": ("sky", "moon"), "discs": ("sky", "discs"),
    "detail": ("sky", "detail"), "detail_z": ("sky", "detail_z"), "sun_intensity": ("sun", "intensity"),
    "sky5_lit_air": ("sky", "lit_air"), "sky5_star_orientation": ("sky", "star_orientation"),
    "sky5_cloud_detail": ("sky", ("cloud_detail", "cloud_detail_z")),
})
SKY_DOUBLES.update({setting: ("sky", field) for setting, field in SKY5_SINGLES + SKY5_WHEN_SET})


def exact_sky(result, exact):
    """Puts the document's binary64 numbers (setting -> number or list) in the described sky and sun, where the
    description has the field (a Bryce 5 field only with the sky part on)."""
    for setting, value in exact.items():
        part, field = SKY_DOUBLES[setting]
        target = result[part]
        if isinstance(field, tuple):
            for name, v in zip(field, value):
                if name in target:
                    target[name] = v
        elif field in target:
            target[field] = value


def bryce5_part(sky, out):
    """: the Bryce 5 sky part's fields into `out`. With `sky5` off the sky has none (`bryce5_part` false)
    and the Bryce 5 flag bits are not kept."""
    stars = [word(w) for w in sky["stars"]] if "stars" in sky else [0, 0, 0]
    out["star_seed"], out["star_count"], out["comet_count"] = stars  # the seed a word modulo 2^32
    five = bool(sky.get("sky5"))
    out["bryce5_part"] = five
    if "sky5_lit_air" in sky:  # (the lit air's amount, also reads it as 3.0 and 3.1)
        out["lit_air"] = single(sky["sky5_lit_air"])
    if not five:
        out["flags"] &= ~BRYCE5_BITS
        return
    out["second_flags"] = word(sky.get("sky5_flags2", 0))
    out["haze_shift_colour"] = singles(sky["sky5_haze_shift"])
    out["cloud_detail"], out["cloud_detail_z"] = singles(sky["sky5_cloud_detail"])
    for setting, field in SKY5_SINGLES:
        out[field] = single(sky[setting])
    out["air_steps"] = word(sky["sky5_air_steps"])
    out["star_orientation"] = singles(sky["sky5_star_orientation"])
    for setting, field in SKY5_WHEN_SET:
        if setting in sky:
            out[field] = single(sky[setting])


def _carry(value):
    """A held setting as plain numbers: bools and whole numbers kept, other numbers as singles."""
    if isinstance(value, bool) or isinstance(value, int):
        return value
    if isinstance(value, float):
        return single(value)
    if isinstance(value, (list, tuple)):
        return [_carry(v) for v in value]
    return value


# ----------------------------------------------------------------: points and triangles

def mesh_corners(units, location, rotation_quaternion, scale, vertices, triangle, shear=None):
    """A mesh triangle's corners in the description's world, in the description's order ."""
    m = object_matrix(location, rotation_quaternion, scale, shear)
    corners = [to_bryce_point(transform_point(m, singles(vertices[i])), units) for i in triangle]
    return list(triangle_order(*corners))


def mesh_triangles(units, matrix, vertices, triangles):
    """Every triangle of a mesh, given its world matrix (4x4, binary64), in the description's order ."""
    world = [to_bryce_point(transform_point(matrix, singles(v)), units) for v in vertices]
    if is_mirrored(matrix):  # BS-1 3.3: keep each triangle facing in the world as its polygon faces in Blender
        triangles = [(a, c, b) for a, b, c in triangles]
    return [list(triangle_order(world[a], world[b], world[c])) for a, b, c in triangles]


def mesh_origin(units, matrix):
    """The place of a mesh object's own origin in the world, in Bryce's axes and units (the description's `origin` of
    a mesh with normals or coordinates; the points are still given in the world)."""
    return list(to_bryce_point(transform_point(matrix, singles((0.0, 0.0, 0.0))), units))


def normal_matrix(m):
    """The inverse transpose of a 4x4 matrix' 3x3 part (the matrix that carries normals), None where it has none."""
    a = [[float(m[i][j]) for j in range(3)] for i in range(3)]
    det = (a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
           + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
    if det == 0.0 or det != det:
        return None
    # the cofactor matrix over the determinant is the inverse's transpose
    return [[(a[(i + 1) % 3][(j + 1) % 3] * a[(i + 2) % 3][(j + 2) % 3]
              - a[(i + 1) % 3][(j + 2) % 3] * a[(i + 2) % 3][(j + 1) % 3]) / det for j in range(3)] for i in range(3)]


def mesh_shading(matrix, triangles, shading):
    """The description's `normals`, `uvs`, `corner_normals`, `corner_uvs` and `flags` for a mesh part whose triangles
    are `triangles` (vertex numbers) and whose Blender shading is `shading` (per triangle: smooth, the three corner
    normals, the three texture coordinates or None): the smooth faces' normals carried to the world in Bryce's axes
    and unit length, the coordinates with v counted from the picture's first row down (a Bryce mesh), the corners in
    the opposite order , and the order kept where a mirroring matrix turned the triangles. {} where nothing is
    smooth and nothing has coordinates."""
    if not any(smooth or uv is not None for smooth, _, uv in shading):
        return {}
    carry = normal_matrix(matrix)
    if carry is None:
        return {}
    turn = (0, 2, 1) if is_mirrored(matrix) else (0, 1, 2)  # the same turn mesh_triangles made
    order = (0, 2, 1)
    normals, normal_at, uvs, uv_at = [], {}, [], {}
    corner_normals, corner_uvs, flags = [], [], []
    for smooth, ns, tex in shading:
        ids = uv_ids = None
        flag = 0
        if smooth:
            ids = []
            for n in ns:
                n = singles(n)
                w = [sum(carry[i][j] * n[j] for j in range(3)) for i in range(3)]
                length = math.sqrt(w[0] * w[0] + w[1] * w[1] + w[2] * w[2])
                if length == 0.0 or length != length:
                    ids = None
                    break
                key = (w[0] / length, w[2] / length, w[1] / length)
                if key not in normal_at:
                    normal_at[key] = len(normals)
                    normals.append(list(key))
                ids.append(normal_at[key])
        if ids is not None:
            flag |= 1
            ids = [ids[turn[k]] for k in order]
        if tex is not None:
            uv_ids = []
            for uv in tex:
                key = (single(uv[0]), single(1.0 - uv[1]))  # v counted from the first row down
                if key not in uv_at:
                    uv_at[key] = len(uvs)
                    uvs.append(list(key))
                uv_ids.append(uv_at[key])
            flag |= 2
            uv_ids = [uv_ids[turn[k]] for k in order]
        corner_normals.append(ids)
        corner_uvs.append(uv_ids)
        flags.append(flag)
    return {"normals": normals, "uvs": uvs, "corner_normals": corner_normals, "corner_uvs": corner_uvs, "flags": flags}


# ----------------------------------------------------------------: trees

TREE_SINGLE = (  # bryce_tree (setting ->'s name in the tree case)
    ("thickness", "trunk-thickness"), ("least", "least-thickness"), ("random", "randomness"),
    ("spread", "power"), ("gravity", "gravity"), ("twist", "turn"), ("leaf_size", "leaf-size"),
)
TREE_OUTLINE = (("bottom", "O0"), ("low", "O1"), ("high", "O2"), ("top", "O3"))  # outline


def tree_settings(tree):
    """'s settings by the tree case's names, from `bryce_tree` ; each double the shortest decimal."""
    out = {
        "seed": word(tree["seed"]),
        "trunk-heights": [int(v) for v in tree["whorls"]],
        "trunk-branches": int(tree["trunk_whorl"]),
        "branch-branches": int(tree["whorl"]),
        "angles": [shortest_single(tree["angle_thick"]), shortest_single(tree["angle_thin"])],
        "shares": [shortest_single(v) for v in tree["share"]],
        "leaves": int(tree["leaves"]),
        "leaf-kind": int(tree["leaf_kind"]),
        "outline": {key: [shortest_single(v) for v in tree[setting]] for setting, key in TREE_OUTLINE},
    }
    if str(tree["leaf_kind"]) not in names.LEAF_KIND:
        raise ValueError("leaf_kind %r is not one of 3 to 7" % (tree["leaf_kind"],))
    for setting, key in TREE_SINGLE:
        out[key] = shortest_single(tree[setting])
    if tree.get("leaf"):
        out["leaf"] = tree["leaf"]
    return out



def material_settings(node, inputs):
    """'s flag word, channels' A, volume colour, refraction and volume numbers , nothing linked."""
    channels = {}
    for name in names.COLOUR_CHANNELS:
        channels[name] = singles(inputs[name])
    for name in names.VALUE_CHANNELS:
        channels[name] = single(inputs[name])
    return {
        "flags": word(node["flags"]) & ~MATERIAL_UNUSED_FLAG,
        "channels": channels,
        "volume_colour": singles(node["volume_color"]),
        "refraction": single(node["refraction"]),
        "volume_numbers": [single(node["volume_edge"]), single(node["volume_sharpness"]),
                           single(node["volume_quality"])],
    }



def radial_light(light, location, units, own_diffuse=None):
    """'s radial light from a point lamp's Bryce Light settings ."""
    colour = singles(light["color"])
    if light.get("tint") and own_diffuse is not None:
        colour = [single_product(c, d) for c, d in zip(colour, own_diffuse)]
    return {
        "kind": "radial",
        "colour": colour,
        "intensity": single(light["intensity"]),
        "position": to_bryce_point(singles(location), units),
        "switches": word(light["flags"]),
    }



def srgb_encode(linear):
    """Linear light to a display value by the sRGB transfer function (IEC 61966-2-1), binary64 ."""
    if linear <= 0.0031308:  # IEC 61966-2-1
        return 12.92 * linear  # IEC 61966-2-1
    return 1.055 * linear ** (1.0 / 2.4) - 0.055  # IEC 61966-2-1


def scene_description(render, sky=None, lights=(), materials=None, objects=(), trees=()):
    """The whole description : our own arrangement of the parts the functions above make."""
    return {
        "render": render,
        "sky": None if sky is None else sky["sky"],
        "sun": None if sky is None else sky["sun"],
        "five": None if sky is None else sky.get("five"),
        "lights": list(lights),
        "materials": dict(materials or {}),
        "objects": list(objects),
        "trees": list(trees),
    }
