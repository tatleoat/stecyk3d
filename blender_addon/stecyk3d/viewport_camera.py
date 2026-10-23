# SPDX-License-Identifier: GPL-3.0-or-later
# , viewport_pixel_size and viewport_quality,
# BS-1 4.5 and 4.6 (the projection's entries, as camera_frame uses them),
# (The Rendered viewport: the camera, size and quality, the change key)
"""The 3D viewport's view as the description's camera, the picture's size, and the key that tells a change.
Pure Python; no bpy. Matrices are 4 by 4 rows of numbers."""

import math
from collections import namedtuple

from .axes import to_bryce_direction, to_bryce_point
from .camera_frame import PARALLEL_PROGRAM

PASS_OVER_EDGES_BIT = 0x2000  # / (bit 0x2000 of the switch word) [format]

# What the engine reads from Blender at a view_update or a view_draw (plain values only).
ViewState = namedtuple("ViewState", "width height pixel_size quality perspective view_matrix window_matrix exact",
                       defaults=(None,))
# `exact`: the description's camera of the scene's own camera (gather.describe_camera) where the view is that
# camera's own view to the last bit; None otherwise (the camera is then derived from the matrices).


def matrix_rows(m):
    """A Blender matrix (or any 4 by 4 of rows) as a tuple of tuples of floats."""
    return tuple(tuple(float(v) for v in row) for row in m)


def view_key(state):
    """The change-detection key: equal for an unchanged view, different for any change of size, pixel size,
    quality, kind of view or either matrix (blender-integration.md 3.2)."""
    return (int(state.width), int(state.height), int(state.pixel_size), str(state.quality), str(state.perspective),
            matrix_rows(state.view_matrix), matrix_rows(state.window_matrix), repr(state.exact))


def pixel_size(setting, blender_value=1):
    """viewport_pixel_size: "1", "2", "4" or "8" as a number; AUTO takes Blender's own value."""
    if setting in ("1", "2", "4", "8"):
        return int(setting)
    if setting == "AUTO":
        return max(1, int(blender_value))
    raise ValueError("viewport_pixel_size %r is not AUTO, 1, 2, 4 or 8" % (setting,))


def picture_size(width, height, size):
    """The picture's size for a region: its size over the pixel size, rounded up, from 1 by 1."""
    size = max(1, int(size))
    return max(1, -(-int(width) // size)), max(1, -(-int(height) // size))


def camera_from_view(view_matrix, window_matrix, perspective, units=1.0, program="5.0"):
    """The description's camera for the viewport's view (BS-1 4.5; design note). `perspective` is Blender's
    view_perspective: PERSP and CAMERA are perspective, ORTHO is parallel (refused unless the program is 3.0,
    ). Raises ValueError, saying why, for what cannot be drawn."""
    v = matrix_rows(view_matrix)
    p = matrix_rows(window_matrix)
    # world = V^-1: the eye is -R^T t, the camera looks down its own -z and has its own y up; R's rows are the
    # camera's axes in the world's.
    eye = [-sum(v[i][j] * v[i][3] for i in range(3)) for j in range(3)]
    forward = to_bryce_direction([-v[2][0], -v[2][1], -v[2][2]], "the view's direction")
    up = to_bryce_direction([v[1][0], v[1][1], v[1][2]], "the view's up")
    camera = {"position": to_bryce_point(eye, units), "forward": forward, "up": up, "type": "PERSP",
              "pixel_aspect": 1.0}
    if perspective in ("PERSP", "CAMERA"):
        if not p[0][0] > 0.0:
            raise ValueError("the view's projection has no width")
        camera["projection"] = "perspective"
        camera["frame_angle"] = 2.0 * math.atan(1.0 / p[0][0])
        camera["pan"] = [-p[0][2] / 2.0, p[1][2] / 2.0]
    elif perspective == "ORTHO":
        if program != PARALLEL_PROGRAM:
            raise ValueError("orthographic view needs program %s: Bryce draws a parallel view only as that "
                             "program (the Rendered viewport shows the last picture)" % PARALLEL_PROGRAM)
        if not (p[0][0] > 0.0 and p[1][1] > 0.0):
            raise ValueError("the view's projection has no size")
        camera["type"] = "ORTHO"
        camera["projection"] = "parallel"
        camera["plane_height"] = 2.0 / p[1][1] * float(units)
        camera["pan"] = [p[0][3] / 2.0, -p[1][3] / 2.0]
    else:
        raise ValueError("the view is %r, not a perspective, orthographic or camera view" % (perspective,))
    return camera


EXACT_KEYS = ("position", "forward", "up", "frame_angle", "pan", "view_rows", "view_centre", "plane_height")


def exact_view(camera, scene_camera, tolerance=1e-4):
    """The viewport's camera `camera` (from Blender's single-precision view matrices), or the scene camera's own
    description `scene_camera` (the document's doubles, gather.exact_camera) where the view is that camera's: the same
    position, directions, angle and pan to `tolerance`. A view the user has orbited or zoomed stays as Blender gives it."""
    if not scene_camera or camera.get("projection") != "perspective":
        return camera
    # a panoramic scene camera is shown by Blender's viewport as a perspective view of the same eye and axes: the
    # place and axes alone tell it is that camera's view; a perspective one also its angle and pan
    panorama = scene_camera.get("projection") == "panorama"
    if not panorama and "frame_angle" not in scene_camera:
        return camera

    def near(a, b):
        return all(abs(x - y) <= tolerance * max(1.0, abs(y)) for x, y in zip(a, b))

    for key in ("position", "forward", "up") if panorama else ("position", "forward", "up", "pan"):
        if not near(camera[key], scene_camera.get(key, camera[key])):
            return camera
    if not panorama and abs(camera["frame_angle"] - scene_camera["frame_angle"]) > tolerance:
        return camera
    out = dict(camera)
    out.update({k: scene_camera[k] for k in EXACT_KEYS if k in scene_camera})
    if panorama:
        out.pop("frame_angle", None)
        out.update({"projection": "panorama", "type": scene_camera.get("type", "PANO"), "pan": scene_camera.get("pan", [0.0, 0.0])})
    return out


def for_quality(render, quality):
    """A copy of the render request  with the viewport's quality applied: RENDER as it is; NORMAL one ray
    through each pixel's centre with the pass over edges kept; DRAFT the same with the pass off."""
    out = dict(render)
    if quality == "RENDER":
        return out
    if quality not in ("NORMAL", "DRAFT"):
        raise ValueError("viewport_quality %r is not RENDER, NORMAL or DRAFT" % (quality,))
    out["antialias"] = 0
    if quality == "DRAFT" and out.get("switches") is not None:
        out["switches"] = int(out["switches"]) & ~PASS_OVER_EDGES_BIT
    return out


def viewport_members(base, state, units, program, size):
    """The members of a description that a view changes: the viewport's camera, the render request with its quality,
    and the picture size. The long-lived renderer takes these with each request and keeps the rest of the scene."""
    return {
        "camera": state.exact if state.exact is not None else
        exact_view(camera_from_view(state.view_matrix, state.window_matrix, state.perspective, units, program),
                   base.get("viewport_scene_camera")),
        "render": for_quality(base["render"], state.quality),
        "size": [size[0], size[1]],
    }


def viewport_description(base, state, units, program, size):
    """The description for a view: `base` (the scene's, from gather.describe) with the viewport's camera, quality and
    size. `base` is not changed."""
    description = dict(base)
    description.update(viewport_members(base, state, units, program, size))
    return description
