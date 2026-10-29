# SPDX-License-Identifier: GPL-3.0-or-later
# (Animation: Render Animation, headless.) Three frames of a sphere that moves across the picture: the sky above it
# stays the same and the columns it leaves and reaches change; exact_png writes one file per frame, each the frame's
# levels. Then a keyframed sun lamp, camera and sky setting (the haze): each frame of the animation is the picture a
# still render at that frame gives, so the sky follows the frame (it used to keep the frame the render started at).
#   blender -b --factory-startup --python blender_animation.py -- <substrate> <program> <out-prefix>
"""bpy.ops.render.render(animation=True) over three frames, against still renders of the same frames."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blender_common  # noqa: E402
from png_read import read_png  # noqa: E402

bpy, _substrate, _program, prefix = blender_common.start()
scene = blender_common.setup(bpy, prefix)
sky = scene.world.bryce
sky.sun_intensity = 1.0
sky.sun_color = (1.0, 1.0, 1.0)
sky.ambient_color = (0.2, 0.2, 0.2)
for obj in [o for o in scene.objects if o.type == "MESH"]:
    bpy.data.objects.remove(obj)
camera = scene.camera
camera.location = (0.0, -10.0, 2.0)
camera.rotation_euler = (1.45, 0.0, 0.0)
ball = bpy.data.objects.new("ball", bpy.data.meshes.new("ball"))
scene.collection.objects.link(ball)
ball.bryce_shape = "sphere"
for frame, x in ((1, -3.0), (2, 0.0), (3, 3.0)):
    ball.location = (x, 0.0, 1.0)
    ball.keyframe_insert("location", frame=frame)
scene.frame_start, scene.frame_end = 1, 3
scene.frame_set(1)


def rgb(path):
    width, height, channels, data = read_png(path)
    return width, height, bytes(b for i, b in enumerate(data) if channels == 3 or i % 4 != 3)


def column(data, width, height, x):
    return bytes(data[(y * width + x) * 3 + c] for y in range(height) for c in range(3))


# 1. the sphere moves; nothing else does
scene.render.filepath = prefix + "-move-####.png"
scene.bryce_render.exact_png = prefix + "-move-exact-####.png"
bpy.ops.render.render(animation=True)
frames = [rgb(prefix + "-move-%04d.png" % f) for f in (1, 2, 3)]
width, height = frames[0][0], frames[0][1]
pictures = [f[2] for f in frames]
assert len(set(pictures)) == 3, "ANIMATION-BAD: the three frames are not three pictures"
top = [p[:width * 3] for p in pictures]
assert top[0] == top[1] == top[2], "ANIMATION-BAD: the sky above the sphere changed with nothing in it animated"
left, right = width // 6, width - 1 - width // 6  # where the sphere is at frame 1, and at frame 3
assert column(pictures[0], width, height, left) != column(pictures[2], width, height, left), "ANIMATION-BAD: left"
assert column(pictures[0], width, height, right) != column(pictures[2], width, height, right), "ANIMATION-BAD: right"
for frame, picture in zip((1, 2, 3), pictures):
    exact = read_png(prefix + "-move-exact-%04d.png" % frame)[3]
    assert exact == picture, "ANIMATION-BAD: exact_png of frame %d is not that frame's levels" % frame
print("ANIMATION moving sphere: 3 frames, the sky unchanged, the sphere's columns changed")

# 2. a keyframed sun lamp, camera and sky setting: each frame of the animation is that frame's still render
scene.bryce_render.exact_png = ""
sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
scene.collection.objects.link(sun)
for frame, tilt, haze, lift in ((1, 0.3, 0.4, 2.0), (2, 0.9, 1.5, 2.5), (3, 1.4, 3.0, 3.0)):
    sun.rotation_euler = (tilt, 0.0, 0.4)
    sun.keyframe_insert("rotation_euler", frame=frame)
    sky.haze = haze
    sky.keyframe_insert("haze", frame=frame)
    camera.location = (0.0, -10.0, lift)
    camera.keyframe_insert("location", frame=frame)
scene.frame_set(1)
scene.render.filepath = prefix + "-sky-####.png"
bpy.ops.render.render(animation=True)
animated = [rgb(prefix + "-sky-%04d.png" % f)[2] for f in (1, 2, 3)]
assert len({p[:width * 3] for p in animated}) == 3, "ANIMATION-BAD: the sky did not follow the frames"
for frame in (1, 2, 3):
    scene.frame_set(frame)
    scene.render.filepath = prefix + "-still-%d.png" % frame
    bpy.ops.render.render(write_still=True)
    assert rgb(scene.render.filepath)[2] == animated[frame - 1], \
        "ANIMATION-BAD: frame %d of the animation is not the still render of that frame" % frame
print("ANIMATION-OK")
