# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport: timer and draw)
"""The scene settings under which the renderer's levels reach the screen or a picture unchanged. No bpy import:
it works on any scene-like object, so F12, the viewport and the import share it."""


def make_plain(scene):
    """The levels are handed to Blender as k/255 scene-linear, so they show unchanged only with the view transform
    `Raw`, look None and dithering 0 (`AgX`, the new-scene default, or any dither changes them). Sets those on
    `scene` and returns what it changed, as words (an empty list when the scene was already on them)."""
    changed = []
    view = scene.view_settings  # (in Blender 5.2 the view transform lives on the scene, not the render)
    if view.view_transform != "Raw":
        view.view_transform = "Raw"
        changed.append("view transform Raw")
    if getattr(view, "look", "None") not in ("", "None"):
        view.look = "None"
        changed.append("look None")
    if scene.render.dither_intensity != 0.0:
        scene.render.dither_intensity = 0.0
        changed.append("dithering 0")
    return changed


def is_plain(scene):
    view = scene.view_settings
    return (view.view_transform == "Raw" and getattr(view, "look", "None") in ("", "None")
            and scene.render.dither_intensity == 0.0)
