# SPDX-License-Identifier: GPL-3.0-or-later
# (Texture settings)
"""A Bryce Texture's settings, as plain values read off its nodes, to the description's texture. Pure Python.

The input is the texture as the nodes hold it (the form of, key `texture`):
  gives, color_space, filter {kind, n, a, b}, colours {mode, first, second, third},
  components [one to three of {flags, space, colours, noise, phase_amount, filter or None, phase_noise or None}],
  blends [first Blend's mode, second Blend's mode], phase {amount, noise or None} or None,
  mapping {mode, flags, scale, offset, rotation} or None.
where a noise is {kind, method, count, dims} with either {custom False, rotation, frequency} or {custom True,
matrix}. Nothing here needs Blender: the node walk that makes this form is the caller's.
"""

from .numbers import single, singles, word

COLOUR_BITS = 0xFFFFFF  # [format]
MAX_COMPONENTS = 3
NO_NOISE = {"type": 0}
NO_FILTER = {"number": 0}


def _colour_word(x):
    return "%06x" % (word(x) & COLOUR_BITS)


def _colours(c, space):
    """The colour map: `mode`, `space` as set, and A, B, C as six hexadecimal digits."""
    return {"mode": int(c["mode"]), "space": word(space), "a": _colour_word(c["first"]),
            "b": _colour_word(c["second"]), "c": _colour_word(c["third"])}


def _filter(f):
    if f is None:
        return dict(NO_FILTER)
    return {"number": int(f["kind"]), "n": int(f["n"]), "a": single(f["a"]), "b": single(f["b"])}


def _noise(n):
    return {"type": int(n["kind"]), "method": int(n["method"]), "octaves": int(n["count"]),
            "dimension": int(n["dims"])}


def _space(n, out):
    """A noise's space into `out`: the angles and frequencies, or the nine numbers (`custom`) as `map`."""
    if n.get("custom"):
        out["map"] = singles(n["matrix"])
    else:
        out["angles"] = singles(n["rotation"])
        out["frequencies"] = singles(n["frequency"])


def _phase(amount, noise):
    """A phase: the constant and the Bryce Noise linked to it (none: type 0 and nothing moved)."""
    out = {"amount": single(amount)}
    if noise is None:
        out["noise"] = dict(NO_NOISE)
        return out
    out["noise"] = _noise(noise)
    _space(noise, out)
    return out


def _component(c):
    out = {"outputs": word(c["flags"]), "noise": _noise(c["noise"])}
    _space(c["noise"], out)
    out["phase"] = _phase(c.get("phase_amount", 0.0), c.get("phase_noise"))
    out["filter"] = _filter(c.get("filter"))
    out["colours"] = _colours(c["colours"], c["space"])
    return out


def _mapping(m):
    if m is None:
        return {"mode": 0, "switches": 0, "scale": [1.0, 1.0, 1.0], "offset": [0.0, 0.0, 0.0],
                "turn": [0.0, 0.0, 0.0, 1.0]}
    return {"mode": int(m["mode"]), "switches": word(m["flags"]), "scale": singles(m["scale"]),
            "offset": singles(m["offset"]), "turn": singles(m["rotation"])}


def texture_settings(tex):
    """: {"texture": ..., "mapping": ...}, the description's texture of a Bryce Texture node's settings."""
    comps = tex["components"]
    if not 1 <= len(comps) <= MAX_COMPONENTS:
        raise ValueError("a texture has one to three components, not %d" % len(comps))
    blends = list(tex.get("blends") or (0, 0))
    # A word is 0 where its Blend does not exist: one Blend joins two components, a second joins the third.
    blend = [int(blends[0]) if len(comps) >= 2 else 0, int(blends[1]) if len(comps) == 3 else 0]
    phase = tex.get("phase")
    f = tex["filter"]
    saved = tex.get("saved")
    return {
        "texture": {
            **({"saved": {"colour": word(saved["colour"]), "numbers": [float(v) for v in saved["numbers"]]}} if saved else {}),
            "outputs": word(tex["gives"]),
            "blend": blend,
            "phase": _phase(0.0, None) if phase is None else _phase(phase["amount"], phase.get("noise")),
            "filter": {"number": int(f["kind"]), "n": int(f["n"]), "a": single(f["a"]), "b": single(f["b"])},
            "colours": _colours(tex["colours"], tex["color_space"]),
            "components": [_component(c) for c in comps],
        },
        "mapping": _mapping(tex.get("mapping")),
    }


BUMP_OUTPUT_BIT = 0x4  # [format]
BUMP_CHANNEL = "bump"


def component_bump(component, channel, honour_switch=False):
    """: whether a component makes its bump in a render, for the channel its texture feeds.

    AUTO (and, unless `honour_switch`, ON and OFF too: lets an implementation take them as AUTO): the
    component makes a bump when it produces one (output bit 0x4) and the Bump Height channel names the texture.
    With `honour_switch`: ON makes it whichever channels name the texture, OFF makes none."""
    switch = component.get("bump_switch", "AUTO")
    if honour_switch and switch == "ON":
        return True
    if honour_switch and switch == "OFF":
        return False
    return channel == BUMP_CHANNEL and bool(word(component["flags"]) & BUMP_OUTPUT_BIT)
