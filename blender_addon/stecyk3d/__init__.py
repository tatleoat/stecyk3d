# SPDX-License-Identifier: GPL-3.0-or-later
# (Package layout: bpy is imported only inside register)
"""Stecyk3D: renders Blender scenes the way Bryce would."""

bl_info = {
    "name": "Stecyk3D",
    "description": "Render Blender scenes as Bryce 3D 3.0, 3.1 or Bryce 5.0 would",
    "version": (0, 1, 0),
    "blender": (4, 2, 0),
    "category": "Render",
}

_MODULES = ("props", "nodes", "engine", "panels", "shader_ops", "terrain_tool", "preset_ops", "node_menu", "create_ops", "import_op", "migrate")


def _modules():
    import importlib
    return [importlib.import_module("." + name, __name__) for name in _MODULES]


def register():
    for module in _modules():
        module.register()


def unregister():
    for module in reversed(_modules()):
        module.unregister()
