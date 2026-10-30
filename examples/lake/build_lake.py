# SPDX-License-Identifier: GPL-3.0-or-later
"""A Bryce 5 remake of a moonlit lake photo, built with Stecyk3D's scripting API (docs/manual/scripting.md): a
wooden dock running out to a T, a forested mountain on the left, a hazy far peak, a pine ridge on the right, reeds
and a dirt path in front, a full moon in a starry sky. Rebuilds the whole scene, renders it and saves the .blend.

    blender -b --python build_lake.py -- <out folder> [--quick] [--res WxH] [--name STEM] [look flags]

  --quick              640 x 452, no .blend saved          --res WxH      the picture's size (2000x1414)
  --pitch DEG          the camera's tilt down (8.3)        --name STEM    STEM.png, STEM_raw.png, bryce_STEM.blend
  --moon --ambient --sky-gain --haze --haze-thick --lit-air --glow --stars --star-int --disc I=V,...: the sky
  --reflect --bump --water-diffuse: the water   --far-glow: the far peak   --deck --rail --dirt: preset names
  --no-halo            no moon halo (the post pass)

variants.py renders several looks of it at once, one Blender each. Only the add-on's api module is used (with
Blender's own bpy, bmesh and mathutils for the meshes, camera and the halo), so the add-on's insides may change
under it. Everything random comes from SEED, so a rerun gives the same scene. Blender units are metres, the water
is z = 0 and the camera looks down +Y at the moon. Angles and heights were measured off the 2000 x 1414 photo with
a 20 mm lens: the horizon is 38.5% down, so the camera pitches 8.3 degrees down.
"""
import importlib
import math
import os
import random
import time

import bpy
from mathutils import Vector, noise as mnoise

# An extension's package name is long (bl_ext.<repository>.stecyk3d): find it, then import its api.
api = importlib.import_module(next(k for k in bpy.context.preferences.addons.keys() if k.endswith("stecyk3d"))
                              + ".api")

ARGS = api.arguments()
OUT = os.path.abspath(ARGS.positional[0])
QUICK = ARGS.has("--quick")
RES = tuple(int(v) for v in ARGS.get("--res", "640x452" if QUICK else "2000x1414").split("x"))
SEED = 1998

rng = random.Random(SEED)
scene = api.begin("5.0", clear=True)

# ---------------------------------------------------------------- helpers

def clamp01(t):
    return 0.0 if t < 0.0 else 1.0 if t > 1.0 else t


def smoothstep(e0, e1, x):
    t = clamp01((x - e0) / (e1 - e0))
    return t * t * (3 - 2 * t)


def fbm(x, y, octaves, seed):
    """Perlin fBm, about -0.5..0.5."""
    total, amp, f, norm = 0.0, 1.0, 1.0, 0.0
    for _ in range(octaves):
        total += amp * mnoise.noise(Vector((x * f + seed, y * f - seed, seed * 0.37)))
        norm += amp
        amp *= 0.5
        f *= 2.03
    return total / norm


def ridged(x, y, octaves, seed):
    """Ridged fBm, 0..1, sharp crests."""
    total, amp, f, norm = 0.0, 1.0, 1.0, 0.0
    for _ in range(octaves):
        n = 1.0 - abs(mnoise.noise(Vector((x * f + seed, y * f - seed, seed * 0.61))))
        total += amp * n * n
        norm += amp
        amp *= 0.5
        f *= 2.03
    return total / norm

# ---------------------------------------------------------------- materials

# The lake: a calm mirror with a deep blue body, so it reads blue even where it mirrors the black forest.
WATER = api.preset_material("Waters&Liquids", "Shiny and Still", channels=dict(
    reflection=ARGS.get("--reflect", 0.65), bump=ARGS.get("--bump", 0.012),
    diffuse_color=[0.04, 0.38, 0.68], diffusion=ARGS.get("--water-diffuse", 0.8)))
DIRT = api.preset_material("Rocks&Stones", ARGS.get("--dirt", "Freshly Dug Grave"), "Riverbed", "Dried Mud")
FOREST = api.preset_material("Planes&Terrains", "Conifer Forest", "Forested Granite", "Highlands", "Verdant Hills")
# The far peak in plain blue-grey ("Distant Mountain" did not load in the add-on); the haze does the rest.
FAR_GLOW = ARGS.get("--far-glow", 1.0)  # Bryce ambience on the far peak: the air between lit by the moon
FAR = api.colour_material("Far peak", (0.22, 0.40, 0.52), glow=((0.20, 0.48, 0.65), FAR_GLOW) if FAR_GLOW else None)
DECK = api.preset_material("Simple&Fast", ARGS.get("--deck", "Warped Wood"), "Light Wood")
RAIL = api.preset_material("Simple&Fast", ARGS.get("--rail", "Walnut Wood"), "Walnut Wood")
PEBBLE = api.preset_material("Rocks&Stones", "Wet Rock", "Pitted Granite")
STUMP = api.preset_material("Miscellaneous", "Mud/Bark Fusion", "Giger's Moss")
NEEDLES = [api.colour_material("Needles %d" % k, c, roughness=1.0) for k, c in  # matte: no streaks of moon glint
           enumerate([(0.025, 0.11, 0.09), (0.035, 0.13, 0.10), (0.02, 0.09, 0.08)])]
REEDS = [api.colour_material("Reeds %d" % k, c, roughness=0.35) for k, c in  # glossy, so tips catch the moon
         enumerate([(0.20, 0.50, 0.08), (0.30, 0.58, 0.10), (0.08, 0.26, 0.05), (0.42, 0.55, 0.16)])]

# ---------------------------------------------------------------- terrains

CAM = Vector((0.0, -3.0, 2.0))


# The near shore: a spit of dirt the camera stands on, its tip where the dock begins (y = 0), water either side.
def shore_height(x, y):
    half = 1.7 + 0.45 * max(0.0, -y)                  # the spit widens toward the camera
    d = min(half - abs(x), (0.8 - y) * 1.6)          # roughly metres in from the water's edge
    z = -1.0 + 1.42 * smoothstep(-4.0, 1.4, d)
    return z + 0.05 * fbm(x / 1.5, y / 1.5, 3, 5.0) + 0.03 * fbm(x * 2, y * 2, 2, 9.0)


shore = api.terrain("Shore", 512, (0.0, -4.0), (48.0, 28.0), (-1.2, 1.0), shore_height, DIRT)

# The basin round the lake: a big forested mountain rising off the left, a low far shore in the middle and a pine
# ridge on the right. The lake is an ellipse 370 m across, its near end under the camera.
LAKE_C, LAKE_R = (0.0, 150.0), (185.0, 175.0)


def basin_height(x, y):
    e = math.hypot(x / LAKE_R[0], (y - LAKE_C[1]) / LAKE_R[1]) + 0.06 * fbm(x / 90, y / 90, 3, 11.0)
    if e < 1.0:
        return -4.0 * min(1.0, (1.0 - e) * 6)
    rise = smoothstep(1.0, 1.9, e)
    left = smoothstep(-80.0, -420.0, x)
    right = smoothstep(60.0, 360.0, x)
    hills = (14.0 + 110.0 * left ** 1.2 + 55.0 * right + 18.0 * fbm(x / 160, y / 160, 5, 3.0)
             + 35.0 * left * ridged(x / 120, y / 120, 4, 7.0))
    return 1.2 + rise * hills


basin = api.terrain("Basin", 512, (0.0, 425.0), (1400.0, 1150.0), (-5.0, 200.0), basin_height, FOREST)

# The far peak, 1.5 km off: a broad ridge whose summit is 15 degrees left of the moon, sliding away to the right.
PEAK = (-378.0, 1500.0)


def far_height(x, y):
    ridge = 238.0 - (0.18 * (PEAK[0] - x) if x < PEAK[0] else 0.30 * (x - PEAK[0]))
    ridge += 16.0 * fbm(x / 140, 3.0, 4, 21.0) + 10.0 * ridged(x / 60, y / 60, 3, 4.0)
    return max(-5.0, ridge) * math.exp(-((y - PEAK[1]) / 230.0) ** 2) - 2.0


far = api.terrain("Far peak", 256, (-200.0, 1500.0), (1800.0, 700.0), (-5.0, 300.0), far_height, FAR)

# The lake: Bryce's infinite plane.
api.shape("plane", "Lake", (0, 0, 0.0), (100, 100, 1), material=WATER)

peak_z = far.z_at(*PEAK)

# ---------------------------------------------------------------- the dock

DECK_Z = 0.47           # top of the boards
HALF_W = 0.625          # half the deck's width
PITCH, BOARD = 0.165, 0.14
GANGWAY_END, WALK_END = 6.5, 30.0
dock = []


def box(name, centre, half, material, rot=(0.0, 0.0, 0.0)):
    dock.append(api.shape("cube", name, centre, half, rot, material))


def jitter():
    return (rng.uniform(-0.012, 0.012), rng.uniform(-0.008, 0.008), rng.uniform(-0.02, 0.02))


# boards across the gangway and the walkway, each a little out of true
y, k = 0.0, 0
while y + BOARD <= WALK_END:
    rx, ry, rz = jitter()
    box("Board %03d" % k, (rng.uniform(-0.02, 0.02), y + BOARD / 2, DECK_Z - 0.025 + rng.uniform(-0.006, 0.0)),
        (HALF_W + rng.uniform(-0.03, 0.03), BOARD / 2, 0.025), DECK, (rx, ry, rz))
    y += PITCH
    k += 1
# stringers under the boards
for x in (-0.48, 0.48):
    box("Stringer %+.1f" % x, (x, WALK_END / 2, DECK_Z - 0.05 - 0.09), (0.05, WALK_END / 2, 0.09), RAIL)
# the T: long boards running across, then a cross-beam under them
T_Y0, T_Y1, T_HALF = WALK_END, WALK_END + 2.0, 7.0
y = T_Y0
while y + BOARD <= T_Y1:
    rx, ry, rz = jitter()
    box("T board %03d" % k, (0.0, y + BOARD / 2, DECK_Z - 0.025), (T_HALF, BOARD / 2, 0.025), DECK, (rx * 0.3, 0, 0))
    y += PITCH
    k += 1
box("T beam", (0.0, (T_Y0 + T_Y1) / 2, DECK_Z - 0.16), (T_HALF, 0.9, 0.11), RAIL)
# a swimming raft beyond the T
box("Raft", (0.0, 37.5, 0.30), (2.5, 1.5, 0.14), DECK)
# pilings down into the lake bed
for y in [0.2 + 3.0 * i for i in range(10)] + [T_Y1 - 0.2]:
    for x in (-0.58, 0.58):
        dock.append(api.shape("cylinder", "Piling %.1f %+.1f" % (y, x), (x, y, -1.0), (0.09, 0.09, DECK_Z + 0.9),
                              material=RAIL))
for x in (-6.6, -3.3, 3.3, 6.6):
    dock.append(api.shape("cylinder", "T piling %+.1f" % x, (x, T_Y0 + 1.0, -1.0), (0.1, 0.1, DECK_Z + 0.9),
                          material=RAIL))
# ladder rails at the T
for x in (-1.9, -1.5, 1.5, 1.9):
    box("Ladder %+.1f" % x, (x, T_Y1 - 0.08, DECK_Z + 0.35), (0.025, 0.025, 0.35), RAIL)
# the gangway's railings: square posts, a wide flat handrail on top, a thin rail halfway
RAIL_X, RAIL_TOP = 0.70, DECK_Z + 1.0
for x in (-RAIL_X, RAIL_X):
    for y in (0.12, 2.2, 4.3, GANGWAY_END - 0.12):
        box("Post %+.1f %.1f" % (x, y), (x, y, (RAIL_TOP - 0.6) / 2), (0.05, 0.05, (RAIL_TOP + 0.6) / 2), RAIL,
            (0, 0, rng.uniform(-0.03, 0.03)))
    box("Handrail %+.1f" % x, (x * 1.03, GANGWAY_END / 2, RAIL_TOP + 0.025), (0.10, GANGWAY_END / 2 + 0.1, 0.025),
        RAIL, (0, math.copysign(0.08, x), 0))
    box("Mid rail %+.1f" % x, (x * 0.95, GANGWAY_END / 2, DECK_Z + 0.42), (0.03, GANGWAY_END / 2, 0.04), RAIL)

# ---------------------------------------------------------------- the near shore: stump, pebbles


def make_rock(name, size, squash, material):
    """A lumpy, flat-shaded stone: an icosphere pushed in and out by noise, then squashed (a plain Blender mesh)."""
    import bmesh
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=1, radius=1.0)
    offset = Vector((rng.uniform(0, 500), rng.uniform(0, 500), rng.uniform(0, 500)))
    for v in bm.verts:
        v.co *= 1.0 + 0.30 * mnoise.noise(v.co * 1.4 + offset)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    scene.collection.objects.link(obj)
    obj.scale = (size * rng.uniform(0.9, 1.3), size * rng.uniform(0.8, 1.1), size * squash)
    obj.rotation_euler = (0.0, 0.0, rng.uniform(0, 2 * math.pi))
    obj.data.materials.append(material)
    return obj


STUMP_AT = (1.45, -0.8)
stump_z = shore.lowest_under(*STUMP_AT, 0.35)
api.shape("cylinder", "Stump", (STUMP_AT[0], STUMP_AT[1], stump_z + 0.12), (0.35, 0.35, 0.2), material=STUMP)
pebbles = 0
for k in range(70):
    x, y = rng.uniform(-1.3, 1.3), rng.uniform(-2.6, 0.2)
    size = rng.uniform(0.018, 0.05)
    p = make_rock("Pebble %02d" % k, size, 0.55, PEBBLE)
    p.location = (x, y, shore.lowest_under(x, y, size) + size * 0.15)
    pebbles += 1

# ---------------------------------------------------------------- reeds and grass


def on_path(x, y):
    if y < 0.3:
        return abs(x) < 0.80 + 0.22 * max(0.0, -y)
    return abs(x) < HALF_W + 0.05


def in_view(x, y, half_angle):
    dx, dy = x - CAM.x, y - CAM.y
    return dy > 0.3 and abs(math.degrees(math.atan2(dx, dy))) < half_angle


blades = []  # (x, y, z, height, width, facing, lean, material index)
tries = 0
while len(blades) < 7000 and tries < 60000:
    tries += 1
    x, y = rng.uniform(-8.0, 8.0), rng.uniform(-2.9, 5.0)
    if on_path(x, y) or not in_view(x, y, 48):
        continue
    z = shore.z_at(x, y)
    if z is None or z < -0.55:
        continue
    away = smoothstep(0.7, 3.0, abs(x))                # taller further from the path
    kind = rng.random()
    if kind < 0.55:   # grass
        h, w, lean, m = (rng.uniform(0.25, 0.75) * (0.7 + 0.6 * away), rng.uniform(0.02, 0.035),
                         rng.uniform(0.1, 0.45), rng.choice((0, 1, 2)))
    elif kind < 0.85:  # reeds
        h, w, lean, m = (rng.uniform(0.9, 2.0) * (0.6 + 0.5 * away), rng.uniform(0.012, 0.022),
                         rng.uniform(0.05, 0.25), rng.choice((0, 2, 3)))
    else:              # leafy weeds
        h, w, lean, m = rng.uniform(0.25, 0.65), rng.uniform(0.05, 0.08), rng.uniform(0.4, 0.8), 1
    blades.append((x, y, z, h, w, rng.uniform(0, math.pi), lean, m))
# tall reeds standing out in the shallows, right of the dock
for k in range(90):
    x, y = rng.uniform(2.5, 9.0), rng.uniform(1.0, 9.0)
    z = shore.z_at(x, y)
    if z is None or not in_view(x, y, 48):
        continue
    blades.append((x, y, z, rng.uniform(1.2, 2.4), rng.uniform(0.01, 0.016), rng.uniform(0, math.pi),
                   rng.uniform(0.02, 0.12), 3))


def mesh_object(name, verts, faces, mats, materials):
    """A plain Blender mesh with one material index a face."""
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.polygons.foreach_set("material_index", mats)
    obj = bpy.data.objects.new(name, me)
    scene.collection.objects.link(obj)
    for mat in materials:
        obj.data.materials.append(mat)
    return obj


def blade_mesh(name, items, materials):
    """Every blade as a bent strip: a quad up to 55% of its height, then a triangle to the tip."""
    verts, faces, mats = [], [], []
    for x, y, z, h, w, facing, lean, m in items:
        wx, wy = math.cos(facing) * w / 2, math.sin(facing) * w / 2
        bx, by = -math.sin(facing) * (1 if rng.random() < 0.5 else -1), math.cos(facing)
        base = len(verts)
        p0 = (x, y, z - 0.05)
        p1 = (x + bx * lean * 0.25 * h, y + by * lean * 0.25 * h, z + 0.55 * h)
        p2 = (x + bx * lean * h, y + by * lean * h, z + h * (1 - 0.25 * lean * lean))
        verts += [(p0[0] - wx, p0[1] - wy, p0[2]), (p0[0] + wx, p0[1] + wy, p0[2]),
                  (p1[0] - wx * 0.7, p1[1] - wy * 0.7, p1[2]), (p1[0] + wx * 0.7, p1[1] + wy * 0.7, p1[2]), p2]
        faces += [(base, base + 1, base + 3, base + 2), (base + 2, base + 3, base + 4)]
        mats += [m, m]
    return mesh_object(name, verts, faces, mats, materials)


blade_mesh("Reeds", blades, REEDS)

# ---------------------------------------------------------------- pines


def pine_mesh(name, items, materials):
    """Every pine as three stacked six-sided cones (no trunk: the nearest is 250 m off)."""
    TIERS = ((0.02, 0.50, 0.20), (0.28, 0.76, 0.15), (0.52, 1.00, 0.09))  # bottom, top, radius, as shares of height
    verts, faces, mats = [], [], []
    for x, y, z, h, turn, m in items:
        for lo, hi, r in TIERS:
            r *= h * rng.uniform(0.85, 1.15)
            base = len(verts)
            for s in range(6):
                a = turn + s * math.pi / 3
                verts.append((x + r * math.cos(a), y + r * math.sin(a), z + lo * h))
            verts.append((x, y, z + hi * h))
            faces += [(base + s, base + (s + 1) % 6, base + 6) for s in range(6)]
            mats += [m] * 6
    return mesh_object(name, verts, faces, mats, materials)


STEP = 8.0
pines = []
for gy in range(int(150 / STEP), int(1000 / STEP)):
    for gx in range(int(-700 / STEP), int(700 / STEP)):
        x, y = (gx + rng.random()) * STEP, (gy + rng.random()) * STEP
        if not in_view(x, y, 48):
            continue
        d = math.hypot(x - CAM.x, y - CAM.y)
        if d > 950 or (d > 450 and rng.random() > 0.5):
            continue
        z = basin.z_at(x, y)
        if z is None or z < 0.6:
            continue
        pines.append((x, y, basin.lowest_under(x, y, 3.0) - 1.0, rng.uniform(17.0, 28.0), rng.uniform(0, math.pi),
                      rng.randrange(3)))
pine_mesh("Pines", pines, NEEDLES)

# ---------------------------------------------------------------- lights, camera, sky

# The moon over the lake, 13.5 degrees up and 0.6 right of straight ahead, and the only light in the scene. Bryce
# keeps the moon opposite the sun, so the sun goes under the horizon behind the camera.
MOON_AZIMUTH, MOON_ELEVATION = math.radians(0.6), math.radians(13.5)
moon_dir = Vector((math.sin(MOON_AZIMUTH) * math.cos(MOON_ELEVATION),
                   math.cos(MOON_AZIMUTH) * math.cos(MOON_ELEVATION), math.sin(MOON_ELEVATION)))
api.light("Sun", "SUN", direction=-moon_dir)

cam_data = bpy.data.cameras.new("Camera")
cam_data.lens, cam_data.sensor_width = 20.0, 36.0
cam = bpy.data.objects.new("Camera", cam_data)
scene.collection.objects.link(cam)
scene.camera = cam
cam.location = CAM
cam.rotation_euler = (math.radians(90 - ARGS.get("--pitch", 8.3)), 0, 0)  # 16:9 frames want about 6

SKY_GAIN = ARGS.get("--sky-gain", 1.0)  # a night sky comes out far darker than its colours: these are solved for
#                                        the photo's measured blue (9, 103, 166): the renderer gives back about 0.72 of each,
#                                        in display values
sky = api.sky(
    has_cumulus=False, has_stratus=False, cover=0.0,
    sky5=True,                # a sky of Bryce 5: its star field
    sky5_flag2_2=True,        # Stars
    stars=(SEED, ARGS.get("--stars", 2500), 0),
    flag_8=True,              # Custom colours: a moonlit sky of our own
    zenith=tuple(c * SKY_GAIN for c in (0.05, 0.56, 0.90)),
    horizon=tuple(c * SKY_GAIN for c in (0.06, 0.58, 0.88)),
    plain=tuple(c * SKY_GAIN for c in (0.05, 0.56, 0.90)),
    haze=ARGS.get("--haze", 0.02),  # distance goes blue, not grey
    haze_color=(0.15, 0.32, 0.55),
    sky5_haze_thickness=ARGS.get("--haze-thick", 0.5),
    sky5_star_intensity=ARGS.get("--star-int", 10.0),
    ambient_color=(0.35, 0.65, 1.0),  # moonlight scattered by the sky: shade stays blue, not black
    ambient_amount=ARGS.get("--ambient", 0.6),
    sun_intensity=ARGS.get("--moon", 9.0),  # at night this is the moon's light
    sun_color=(0.85, 0.92, 1.0),
    sky5_lit_air=ARGS.get("--lit-air", 0.5),
    sky5_moon_disc=1.4,
    sky5_moon_glow_size=ARGS.get("--glow", 20.0))
discs = list(sky.discs)
for pair in ARGS.get("--disc", "").split(","):  # --disc 7=10,1=20: set Bryce's sun/moon disc numbers by index
    if pair:
        i, v = pair.split("=")
        discs[int(i)] = float(v)
api.sky(discs=discs)
print("SKY discs", [round(v, 4) for v in sky.discs], "moon", list(sky.moon))

# ---------------------------------------------------------------- report and render

print("SCENE dock pieces %d, blades %d, pines %d, pebbles %d; ground at the dock %.2f (deck %.2f), peak %.0f m"
      % (len(dock), len(blades), len(pines), pebbles, shore.z_at(0.0, 0.0), DECK_Z, peak_z))
print("CHECK shore z at camera %.2f, basin z under camera %.2f, basin left edge-of-frame (-268, 294) %.1f"
      % (shore.z_at(CAM.x, CAM.y), basin.z_at(CAM.x, CAM.y), basin.z_at(-268, 294)))

stem = ARGS.get("--name", "lake_%dx%d" % RES)
t0 = time.time()
picture = api.render(os.path.join(OUT, stem + ".png"), RES)
print("RENDER OK %.1fs -> %s" % (time.time() - t0, picture))


def moon_halo(path, out):
    """The moon's halo, put on afterwards the way a 90s Bryce render got its glow in Photoshop: the moon's disc
    blurred twice (a tight bloom and a wide halo) and screened over the picture in display values."""
    import numpy as np
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)
    lum = px[..., :3] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    mask = np.clip((lum - 0.6) / 0.15, 0.0, 1.0)
    fy, fx = np.fft.fftfreq(h)[:, None], np.fft.rfftfreq(w)[None, :]
    spectrum = np.fft.rfft2(mask)
    glow = np.zeros((h, w), dtype=np.float32)
    for sigma, strength in ((0.012, 0.7), (0.07, 0.6)):  # shares of the width
        blur = np.fft.irfft2(spectrum * np.exp(-2 * (np.pi * sigma * w) ** 2 * (fx ** 2 + fy ** 2)), s=(h, w))
        glow += strength * blur / max(blur.max(), 1e-12)
    tint = np.array([0.62, 0.82, 1.0], dtype=np.float32)
    px[..., :3] = 1 - (1 - px[..., :3]) * (1 - np.clip(glow, 0, 1)[..., None] * tint)
    img.pixels = px.ravel()
    img.filepath_raw = out
    img.file_format = "PNG"
    img.save()
    print("HALO ->", out)


if not ARGS.has("--no-halo"):
    raw = os.path.join(OUT, stem + "_raw.png")
    os.replace(picture, raw)
    moon_halo(raw, picture)
if not QUICK:
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, "bryce_" + stem + ".blend"))
    print("SAVED", os.path.join(OUT, "bryce_" + stem + ".blend"))
