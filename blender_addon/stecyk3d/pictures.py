# SPDX-License-Identifier: GPL-3.0-or-later
# (Bryce Picture: a picture texture component of `image`; `column`; without a mapping linked it lies by
#       the surface's own coordinates, once; the description holds each picture a channel names once)
"""A Bryce Picture node's picture and mapping as the description holds them. Pure Python."""

from .numbers import singles

# A picture with no Bryce Mapping linked: mode 2, switch word 0, scale 0 ('s picture scale 17·0 + 1 = 1, the
# picture once), offset 0 and the turn (0, 0, 0, 1). spec: README (picture: `mapping` without a mapping node)
DEFAULT_MAPPING = {"mode": 2, "flags": 0, "scale": [0.0, 0.0, 0.0], "offset": [0.0, 0.0, 0.0],
                   "rotation": [0.0, 0.0, 0.0, 1.0]}


def mapping_description(mapping):
    """The mapping of a picture: the Bryce Mapping node's settings (mode, flags, scale, offset, rotation as a
    quaternion x, y, z, w), the lengths and the turn as singles; None gives DEFAULT_MAPPING."""
    if mapping is None:
        return {k: (list(v) if isinstance(v, list) else v) for k, v in DEFAULT_MAPPING.items()}
    return {"mode": int(mapping["mode"]), "flags": int(mapping["flags"]) & 0xFFFFFFFF,
            "scale": singles(mapping["scale"]), "offset": singles(mapping["offset"]),
            "rotation": singles(mapping["rotation"])}


def _level(v):
    """An 8-bit level held by Blender as a float, level over 255, back to the level."""
    return min(255, max(0, int(round(float(v) * 255.0))))


def picture_description(width, height, rgba):
    """A Blender image's pixels as holds a picture: `rgba` is Blender's flat list (r, g, b, a for each pixel,
    the bottom row first, each float a level over 255); the result has the pixels as eight hexadecimal digits each
    (alpha, red, green, blue), the top row first."""
    if width < 1 or height < 1 or len(rgba) != 4 * width * height:
        raise ValueError("a picture of %d by %d pixels holds %d values, not %d"
                         % (width, height, 4 * width * height, len(rgba)))
    pixels = []
    for row in range(height - 1, -1, -1):  # Blender's rows run from the bottom
        for column in range(width):
            k = 4 * (row * width + column)
            r, g, b, a = (_level(v) for v in rgba[k:k + 4])
            pixels.append("%02X%02X%02X%02X" % (a, r, g, b))
    return {"width": width, "height": height, "pixels": pixels}


def picture_texture(name, width, height, rgba, mapping=None):
    """The description's texture entry for a Bryce Picture node.."""
    return {"name": name, "kind": "picture", "picture": picture_description(width, height, rgba),
            "mapping": mapping_description(mapping)}
