# SPDX-License-Identifier: GPL-3.0-or-later
# (The add-on sends what the reader takes)
"""A description made of the add-on's own pure functions, one of each kind the renderer draws from documents:
pyramid, cylinder and gradient lights, a terrain, a group with members, a picture texture and a tree.
`python3 -I addon_kinds.py <out.json>` writes it; tests/description_addon_kinds.rs reads the committed copy
(blender_addon/tests/fixtures/addon-kinds.json) with the Rust reader, and test_addon_kinds.py checks that this
module still makes the same text, so the two sides cannot drift apart. Python standard library only."""

import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from stecyk3d import convert, lamps, pictures, solids  # noqa: E402

IDENTITY = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]


def moved(x, y, z):
    m = [list(r) for r in IDENTITY]
    m[0][3], m[1][3], m[2][3] = x, y, z
    return m


LIGHT = {"flags": 0x80000100 - (1 << 32), "intensity": 1.0, "color": [1.0, 0.5, 0.25], "tint": False, "edge": [2.5, 0.5],
         "reach": 4.0, "radius": 0.0, "shadow_ambience": 0.5, "shadow_color": [0.25, 0.5, 1.0], "range": 8.0,
         "gradient_range": 2.0, "gradient_offset": 0.25,
         "color_keys": [{"color": [0.5, 1.0, 0.25], "position": 0.5, "midpoint": 0.5},
                        {"color": [1.0, 0.0, 0.0], "position": 0.0, "midpoint": 0.25}],
         "value_keys": [{"value": 0.5, "position": 0.5, "midpoint": 0.5}],
         "gradient_color_flags": 3, "gradient_value_flags": 1}

TREE = {"seed": 1234, "whorls": [6, 8], "trunk_whorl": 2, "whorl": 3, "angle_thick": 0.5, "angle_thin": 1.0,
        "thickness": 0.125, "least": 0.015625, "share": [0.5, 0.75], "random": 0.25, "spread": 0.5,
        "gravity": 0.125, "twist": 1.0, "leaves": 4, "leaf_kind": "5", "leaf_size": 0.25,
        "bottom": [0.25, 0.25], "low": [0.5, 0.5], "high": [0.75, 0.25], "top": [0.25, 0.75]}


def build():
    units = 1.0
    lights = []
    for name, lamp in (("spot", {"type": "SPOT", "spot_size": 0.5, "use_square": True}),
                       ("area", {"type": "AREA", "shape": "DISK", "size": 2.0}),
                       ("lamp", {"type": "SPOT", "spot_size": 0.5, "use_square": False})):
        light = dict(LIGHT)
        if name == "lamp":
            light["flags"] = (0x80000100 | 0x200000) - (1 << 32)  # gradient on (Bryce 5)
        out = lamps.describe_lamp(lamp, light, moved(1.0, 2.0, 3.0), units, "5.0", [0.5, 0.5, 0.5], False)
        out["name"] = name
        lights.append(out)
    # a 16 by 16 picture of ramps (a size AUTO takes)
    red = [(i % 16) / 15.0 for i in range(256)]
    terrain = solids.terrain({"grid": "16", "lower": 0.25, "upper": 0.75, "mirrored": True, "smooth": True,
                              "faces": 3, "solid_faces": 1}, 16, 16, red)
    placed = [
        {"name": "G", "bryce_shape": "group", "matrix": moved(1.0, 0.0, 0.0), "parent": None},
        {"name": "A", "bryce_shape": "sphere", "matrix": moved(2.0, 0.0, 0.0), "parent": "G", "bryce_role": "POSITIVE"},
        {"name": "B", "bryce_shape": "cube", "matrix": moved(2.0, 1.0, 0.0), "parent": "G", "bryce_role": "NEGATIVE",
         "bryce_cut_shows_positive": True},
    ]
    groups = solids.describe_groups(placed, units)
    objects = [
        {"name": "G", "shape": "group", "world": solids.own_transform(moved(1.0, 0.0, 0.0), units),
         "members": groups["G"]["members"]},
        {"name": "A", "shape": "sphere", "matrix": moved(2.0, 0.0, 0.0),
         "world": solids.object_transform("sphere", moved(2.0, 0.0, 0.0), units), "material": "Wood"},
        {"name": "B", "shape": "cube", "matrix": moved(2.0, 1.0, 0.0),
         "world": solids.object_transform("cube", moved(2.0, 1.0, 0.0), units)},
        {"name": "T", "shape": "terrain", "matrix": IDENTITY, "terrain": terrain,
         "world": solids.own_transform(IDENTITY, units)},
    ]
    description = convert.scene_description(
        {"version": "5.0", "units": units, "antialias": 0},
        {"sky": {}, "sun": {"direction": [0.3, 0.6, -0.7], "colour": [1.0, 1.0, 1.0]}},
        lights,
        {"Wood": {"flags": 0, "channels": {"diffusion": {"switches": 0x10000000, "a": 0.5, "b": 1.0, "alpha": 1.0,
                                                          "a_tex": "Wood:0"}}}},
        objects,
        [{"name": "Tree", "matrix": moved(0.0, 0.0, 4.0), "tree": convert.tree_settings(TREE),
          "material": "Wood", "leaves_material": "Wood"}])
    description["textures"] = [pictures.picture_texture("Wood:0", 2, 2, [1.0, 0.0, 0.0, 1.0] * 2 + [0.0, 1.0, 0.0, 0.5] * 2)]
    description["camera"] = {"position": [0.0, 2.0, -10.0], "forward": [0.0, 0.0, 1.0], "up": [0.0, 1.0, 0.0],
                             "frame_angle": 1.0}
    description["size"] = [8, 6]
    return description


def text():
    return json.dumps(build(), indent=1, sort_keys=True) + "\n"


if __name__ == "__main__":
    with open(sys.argv[1], "w", encoding="utf-8") as f:
        f.write(text())
