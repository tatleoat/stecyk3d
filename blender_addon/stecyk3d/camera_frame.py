# SPDX-License-Identifier: GPL-3.0-or-later
# BS-1 (section 4: 4.3 angles, 4.4 fit, 4.5 and 4.6 projection, 4.7 types)
# Outputs as revised
# (The camera's frame)
"""A Blender camera's frame from plain values. Pure Python; no bpy.

The camera is a dict by Blender's property names (type, lens, sensor_width, sensor_height, sensor_fit,
shift_x, shift_y, ortho_scale, clip_start, clip_end); the picture a dict by resolution_x, resolution_y,
pixel_aspect_x, pixel_aspect_y (resolution_percentage, if present, is not used: 4.4).
"""

import math

PERSPECTIVE_LIKE = ("PERSP", "PANO", "CUSTOM")  # BS-1 4.7 (calc_matrix_camera of PANO equals PERSP)


def picture_aspect(picture):
    """a = (resolution_x * pixel_aspect_x) / (resolution_y * pixel_aspect_y). spec: BS-1 4.4"""
    width = float(picture["resolution_x"]) * float(picture.get("pixel_aspect_x", 1.0))
    height = float(picture["resolution_y"]) * float(picture.get("pixel_aspect_y", 1.0))
    if not (width > 0.0 and height > 0.0):
        raise ValueError("the picture has no area (%r by %r)" % (width, height))
    return width / height


def fit_is_horizontal(sensor_fit, aspect):
    """The fit axis is x for HORIZONTAL, or AUTO with a >= 1; y for VERTICAL, or AUTO with a < 1. spec: BS-1 4.4"""
    if sensor_fit == "HORIZONTAL":
        return True
    if sensor_fit == "VERTICAL":
        return False
    if sensor_fit == "AUTO":
        return aspect >= 1.0
    raise ValueError("the camera's sensor fit %r is not AUTO, HORIZONTAL or VERTICAL" % (sensor_fit,))


def fit_span(camera):
    """The size spanning the fit axis: sensor_height for VERTICAL, else sensor_width (mm); ortho_scale for
    an orthographic camera (Blender units). spec: BS-1 4.4, 4.6"""
    if camera["type"] == "ORTHO":
        return float(camera["ortho_scale"])
    if camera["sensor_fit"] == "VERTICAL":
        return float(camera["sensor_height"])
    return float(camera["sensor_width"])


def frame_angles(camera):
    """(angle_x, angle_y, angle) as Blender reports them; independent of the picture. spec: BS-1 4.3"""
    lens = float(camera["lens"])
    angle_x = 2.0 * math.atan(float(camera["sensor_width"]) / (2.0 * lens))
    angle_y = 2.0 * math.atan(float(camera["sensor_height"]) / (2.0 * lens))
    angle = angle_y if camera["sensor_fit"] == "VERTICAL" else angle_x
    return angle_x, angle_y, angle


def projection_matrix(camera, picture):
    """The 4 by 4 projection Blender's calc_matrix_camera reports, rows of numbers. spec: BS-1 4.5, 4.6"""
    a = picture_aspect(picture)
    horizontal = fit_is_horizontal(camera["sensor_fit"], a)
    near, far = float(camera["clip_start"]), float(camera["clip_end"])
    sx, sy = float(camera.get("shift_x", 0.0)), float(camera.get("shift_y", 0.0))
    span = fit_span(camera)
    if not span > 0.0:
        raise ValueError("the camera's fit span is %r, not above 0" % span)
    if not far > near:
        raise ValueError("the camera's clip_end %r is not beyond its clip_start %r" % (far, near))
    p = [[0.0] * 4 for _ in range(4)]
    kind = camera["type"]
    if kind in PERSPECTIVE_LIKE:
        k = 2.0 * float(camera["lens"]) / span  # BS-1 4.5 (K = 2 lens / span)
        if horizontal:
            p[0][0], p[1][1], p[0][2], p[1][2] = k, k * a, 2.0 * sx, 2.0 * sy * a
        else:
            p[0][0], p[1][1], p[0][2], p[1][2] = k / a, k, 2.0 * sx / a, 2.0 * sy
        p[2][2] = -(far + near) / (far - near)
        p[2][3] = -2.0 * far * near / (far - near)
        p[3][2] = -1.0
    elif kind == "ORTHO":
        k = 2.0 / span  # BS-1 4.6 (K = 2 / ortho_scale)
        if horizontal:
            p[0][0], p[1][1], p[0][3], p[1][3] = k, k * a, -2.0 * sx, -2.0 * sy * a
        else:
            p[0][0], p[1][1], p[0][3], p[1][3] = k / a, k, -2.0 * sx / a, -2.0 * sy
        p[2][2] = -2.0 / (far - near)
        p[2][3] = -(far + near) / (far - near)
        p[3][3] = 1.0
    else:
        raise ValueError("the camera's type %r is not one of PERSP, ORTHO, PANO, CUSTOM" % (kind,))
    return p


PARALLEL_PROGRAM = "3.0"  # , Outputs


def _type_name(kind):
    return {"ORTHO": "orthographic", "PANO": "panoramic", "CUSTOM": "custom"}.get(kind, "of type %r" % (kind,))


def _full_turn(camera):
    """True when a central cylindrical camera spans a whole turn (The camera's frame).
    The longitude range is read from `central_cylindrical_range_u_min` / `_max` when the record has them
    (Blender 5.2 defaults: -pi to pi); a record without them is taken as the default."""
    low = camera.get("central_cylindrical_range_u_min")
    high = camera.get("central_cylindrical_range_u_max")
    if low is None or high is None:
        return True
    return float(high) - float(low) >= 2.0 * math.pi - 1e-6


def renderer_view(camera, picture, program="5.0", units=1.0):
    """What the renderer program takes (design note): the projection, the angle the picture's width spans (a
    perspective camera), the plane's height (a parallel camera), the pan as fractions of the image plane's width
    and height, and the pixels' width-to-height ratio.

    Outputs and as answer A-0143 states them; which Blender camera carries which
    projection is this add-on's design (The camera's frame):
    PERSP -> perspective; ORTHO -> parallel, as program 3.0 only (refused for 3.1 and 5.0, which have no
    parallel projection); PANO of type CENTRAL_CYLINDRICAL over a whole turn -> panorama; every other PANO type
    and CUSTOM refused, naming the type. `units` is Bryce units a Blender unit."""
    kind = camera["type"]
    if kind not in ("PERSP", "ORTHO", "PANO"):
        raise ValueError("the camera is %s; only perspective, orthographic (as 3.0) and central cylindrical "
                         "panoramic cameras can be rendered" % _type_name(kind))
    pixel_aspect = float(picture.get("pixel_aspect_x", 1.0)) / float(picture.get("pixel_aspect_y", 1.0))
    if kind == "ORTHO" and program != PARALLEL_PROGRAM:
        raise ValueError("the camera is orthographic, and program %s has no parallel projection (Bryce draws "
                         "it only as 3.0); use a perspective camera or render as 3.0" % (program,))
    if kind == "PANO":
        panorama = camera.get("panorama_type")
        if panorama != "CENTRAL_CYLINDRICAL":
            raise ValueError("the camera is panoramic of type %s; only CENTRAL_CYLINDRICAL has a straight "
                             "vertical measure like Bryce's panorama" % (panorama,))
        if not _full_turn(camera):
            raise ValueError("the camera is panoramic (CENTRAL_CYLINDRICAL) but its longitude range is not a "
                             "whole turn; Bryce's panorama spans a whole turn")
    p = projection_matrix(camera, picture)
    if kind == "PERSP":
        return {
            "projection": "perspective",
            "frame_angle": 2.0 * math.atan(1.0 / p[0][0]),  # tan of half the width's angle is 1 / P[0][0]
            "pan": [-p[0][2] / 2.0, p[1][2] / 2.0],  # the frame's centre, as's pan (design note)
            "pixel_aspect": pixel_aspect,
        }
    if kind == "ORTHO":
        a = picture_aspect(picture)
        span = fit_span(camera) * float(units)  # the fit axis spans ortho_scale * units
        height = span / a if fit_is_horizontal(camera["sensor_fit"], a) else span
        return {
            "projection": "parallel",
            "plane_height": height,  # the zoomed plane's height, Bryce units; its width follows
            "pan": [p[0][3] / 2.0, -p[1][3] / 2.0],  # fractions of the plane's sides, as for perspective
            "pixel_aspect": pixel_aspect,
        }
    return {
        "projection": "panorama",
        "pan": [-p[0][2] / 2.0, p[1][2] / 2.0],
        "pixel_aspect": pixel_aspect,
    }
