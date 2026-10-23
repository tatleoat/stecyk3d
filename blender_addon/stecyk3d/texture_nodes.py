# SPDX-License-Identifier: GPL-3.0-or-later
# (the node types, sockets and settings; Errors: a graph that makes no texture, a texture feeding a
#       texture, a fourth component, two filters in a row: refused with a message naming it)
# (Textures in the export)
"""Reads a Bryce Texture node and what is linked to it into the plain input form of texture_settings.py (the form
of). It touches nodes only through attribute access, so tests may give it
plain objects with the same attributes. Raises TextureGraphError, naming the node, for a graph that makes no texture."""


class TextureGraphError(ValueError):
    pass


MAX_COMPONENTS = 3
WORD = 0xFFFFFFFF  # [format]


def _from(socket):
    """The node feeding `socket`; sockets of a group_links.NodeView read through node groups."""
    if socket.is_linked and socket.links:
        return socket.links[0].from_node
    return None


def _noise(node):
    """A Bryce Noise's settings: kind, method, count, dims and its space ."""
    out = {"kind": node.kind & WORD, "method": node.method & WORD, "count": node.count, "dims": node.dims,
           "custom": bool(node.custom)}
    if node.custom:
        out["matrix"] = list(node.matrix)
    else:
        out["rotation"] = list(node.rotation)
        out["frequency"] = list(node.frequency)
    return out


def _phase_noise(socket):
    """A phase input's Bryce Noise, or None (a phase with no noise moves nothing)."""
    node = _from(socket)
    if node is None:
        return None
    if node.bl_idname != "BryceNoiseNode":
        raise TextureGraphError("%s feeds a phase input and is not a Bryce Noise" % node.name)
    return _noise(node)


def _filter(node):
    return {"kind": node.kind & WORD, "n": node.n, "a": node.a, "b": node.b}


def _component(node):
    """A Bryce Component, the filter and the noise behind it (at most one filter, then a noise)."""
    source = _from(node.inputs["value"])
    flt = None
    if source is not None and source.bl_idname == "BryceFilterNode":
        flt = _filter(source)
        source = _from(source.inputs["value"])
    if source is not None and source.bl_idname != "BryceNoiseNode":
        raise TextureGraphError("%s feeds component %s and is not a Bryce Noise (two filters in a row, or another "
                                "node)" % (source.name, node.name))
    out = {"flags": node.flags & WORD, "space": node.space & WORD,
           "colours": {"mode": node.mode & WORD, "first": node.first & WORD, "second": node.second & WORD,
                       "third": node.third & WORD},
           "filter": flt,
           "bump_switch": getattr(node, "bump_switch", "AUTO")}
    if source is None:
        # , : no Bryce Noise in the path still makes a texture whose noise
        # has type 0 (0.5 everywhere, point left as it was). The space only counts for the maps and blend that add
        # noise (modes 7, 8, 10, 13, 14). take the earlier add-on's space (angles 0, frequencies 1,
        # the identity map), so files it saved draw as they did .
        out["noise"] = {"kind": 0, "method": 0, "count": 0, "dims": 1, "custom": False,
                        "rotation": [0.0, 0.0], "frequency": [1.0, 1.0, 1.0]}
        out["phase_amount"] = 0.0
        out["phase_noise"] = None
    else:
        out["noise"] = _noise(source)
        out["phase_amount"] = source.inputs["phase"].value
        out["phase_noise"] = _phase_noise(source.inputs["phase"])
    return out


def _components(socket, name):
    """The components and blend words behind a texture's `component` input : one Bryce Component, or a
    Bryce Blend of two, or a second Bryce Blend joining that result with a third."""
    top = _from(socket)
    if top is None:
        raise TextureGraphError("texture %s has no component linked" % name)
    if top.bl_idname == "BryceComponentNode":
        return [_component(top)], []

    def leaf(sock, what):
        node = _from(sock)
        if node is None or node.bl_idname != "BryceComponentNode":
            raise TextureGraphError("texture %s: the %s of a Bryce Blend is not a Bryce Component" % (name, what))
        return _component(node)

    if top.bl_idname != "BryceBlendNode":
        raise TextureGraphError("texture %s: %s feeds its component input and is neither a Bryce Component nor "
                                "a Bryce Blend" % (name, top.name))
    inner = _from(top.inputs["first"])
    if inner is not None and inner.bl_idname == "BryceBlendNode":
        comps = [leaf(inner.inputs["first"], "first"), leaf(inner.inputs["second"], "second"),
                 leaf(top.inputs["second"], "second")]
        return comps, [inner.mode & WORD, top.mode & WORD]
    comps = [leaf(top.inputs["first"], "first"), leaf(top.inputs["second"], "second")]
    return comps, [top.mode & WORD]


def _mapping(socket):
    node = _from(socket)
    if node is None:
        return None
    if node.bl_idname != "BryceMappingNode":
        raise TextureGraphError("%s feeds a vector input and is not a Bryce Mapping" % node.name)
    return {"mode": node.mode & WORD, "flags": node.flags & WORD, "scale": list(node.scale),
            "offset": list(node.offset), "rotation": list(node.rotation)}


def mapping_of(socket):
    """The settings of the Bryce Mapping linked to a vector input, or None (none, object space or, for a
    picture, the surface's own coordinates once)."""
    return _mapping(socket)


def texture_input(node):
    """The input form of texture_settings.texture_settings for a Bryce Texture node ."""
    if node.bl_idname != "BryceTextureNode":
        raise TextureGraphError("%s is not a Bryce Texture (a picture node's settings are not converted)" % node.name)
    # : whether a global phase moves the points beside a bump's point is
    # stated by and is the render's rule, not a setting of the description; `with_offsets` is not
    # carried (item 8) and is read as the description reads it: not at all.
    comps, blends = _components(node.inputs["component"], node.name)
    phase_socket = node.inputs["phase"]
    colors, flt = node.colors, node.filter
    getter = getattr(node, "get", None)  # (test stand-ins for nodes have none)
    saved = getter("bryce_saved") if getter else None  # : the result as saved in the document (colour word, four doubles)
    return {
        "saved": None if saved is None else {"colour": int(saved[0]) & WORD, "numbers": [float(v) for v in saved[1:5]]},
        "gives": node.gives & WORD, "color_space": node.color_space & WORD,
        "filter": {"kind": flt.kind & WORD, "n": flt.n, "a": flt.a, "b": flt.b},
        "colours": {"mode": colors.mode & WORD, "first": colors.first & WORD, "second": colors.second & WORD,
                    "third": colors.third & WORD},
        "components": comps, "blends": blends,
        "phase": {"amount": phase_socket.value, "noise": _phase_noise(phase_socket)},
        "mapping": _mapping(node.inputs["vector"]),
    }


# ---------------------------------------------------------------- faults (T7): the walk's refusal, without raising

ERRORS = (TextureGraphError, KeyError, ValueError)  # (GroupLinkError is a ValueError)


def fault_message(what, node, error):
    """The text noted for a texture node that makes no texture; the render-time note and the node's are this one."""
    kind = "picture" if node.bl_idname == "BrycePictureNode" else "texture"
    return "%s: %s %r: %s" % (what, kind, node.name, error)


def picture_problem(node):
    """Why a Bryce Picture node has no picture , or None."""
    image = node.image
    if image is None:
        return "it has no image"
    width, height = image.size
    if width * height == 0:
        return "its image %r is missing or empty" % image.name
    return None


def faults_of(node, what):
    """[message] for a Bryce Texture or Bryce Picture node that the render would refuse (what: whose it is, e.g.
    "material 'Stone'"); [] when it makes a texture. Identical to the note the render makes (gather.texture_entry)."""
    if node.bl_idname == "BrycePictureNode":
        problem = picture_problem(node)
        return [fault_message(what, node, ValueError(problem))] if problem else []
    from . import group_links, texture_settings
    try:
        texture_settings.texture_settings(texture_input(group_links.view(node)))
    except ERRORS as error:
        return [fault_message(what, node, error)]
    return []
