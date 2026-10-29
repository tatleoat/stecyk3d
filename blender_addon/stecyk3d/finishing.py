# SPDX-License-Identifier: GPL-3.0-or-later
# (Finishing: the 90s treatment a pre-rendered Bryce picture got afterwards, in a paint program or a web
# exporter. NOT part of Bryce's renderer: the renderer's picture stays exact, and each of these changes it only
# when its setting is on. Pure: numpy arrays in and out, no bpy; numpy is imported where it is used, so plain Python
# without numpy can still import the module.)
"""Optional finishing of the rendered picture: a glow round the brightest lights, a lens flare, one JPEG round trip,
and a palette of N colours with ordered dither."""

LUMA = (0.2126, 0.7152, 0.0722)  # Rec. ITU-R BT.709 luma weights
RAMP = 0.15  # the bright mask goes from 0 at the threshold to 1 at the threshold + RAMP (display values; reference)
# The glow is two blurs of the mask, each scaled so its brightest point is 1 (reference: a tight bloom of 0.012 of
# the picture's width at 0.7 and a wide halo of 0.07 at 0.6). The size setting is the halo's width; the bloom keeps
# the reference's share of it.
BLOOM_SHARE = 0.012 / 0.07
BLOOM_WEIGHT, HALO_WEIGHT = 0.7, 0.6
PAD_SIGMAS = 3.0  # the blur pads the picture by this many widths of its gaussian, so a glow does not wrap round

# The lens flare's sprites, KPT Lens Flare style. Our own arrangement (there is no Bryce number to follow): each is
# (place on the line from the light (0) through the picture's centre (1) and beyond, radius as a share of the
# picture's diagonal, ring width as a share of the diagonal (0: a disc), colour, strength).
FLARE = (
    (0.0, 0.07, 0.0, (1.0, 0.92, 0.75), 0.35),
    (0.35, 0.012, 0.0, (1.0, 0.8, 0.5), 0.35),
    (0.55, 0.03, 0.0, (0.55, 0.75, 1.0), 0.2),
    (0.8, 0.05, 0.004, (0.5, 1.0, 0.6), 0.3),
    (1.2, 0.022, 0.0, (1.0, 0.55, 0.35), 0.25),
    (1.45, 0.01, 0.0, (0.7, 0.8, 1.0), 0.4),
    (1.75, 0.09, 0.006, (0.85, 0.6, 1.0), 0.22),
    (2.0, 0.06, 0.0, (0.45, 0.7, 1.0), 0.12),
)
FLARE_WINDOW = 0.02  # the light is looked for within this share of the diagonal round where it projects
DISC_EDGE = 0.2  # a disc's edge fades over this share of its radius

# Baseline JPEG (ITU-T T.81): the quantisation tables of Annex K (K.1 luminance, K.2 chrominance), as libjpeg scales
# them by quality (jcparam.c: 5000 / q below 50, else 200 - 2q, in percent, each entry 1 to 255).
JPEG_LUMA = (16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
             14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92,
             49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99)
JPEG_CHROMA = (17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
               47, 66, 99, 99, 99, 99, 99, 99) + (99,) * 32
BAYER_SIZE = 8  # the ordered dither's threshold matrix is 8 by 8

# The settings, as the render group holds them (props.RENDER), and the default each takes (defaults.py).
SETTINGS = ("finish_threshold", "glow", "glow_size", "glow_strength", "glow_tint", "flare", "flare_strength",
            "jpeg", "jpeg_quality", "palette", "palette_colours")
SWITCHES = ("glow", "flare", "jpeg", "palette")


def settings_of(group):
    """The finishing settings of a render group (anything with the attributes) as a plain dict."""
    out = {}
    for name in SETTINGS:
        value = getattr(group, name)
        out[name] = tuple(float(c) for c in value) if name == "glow_tint" else value
    return out


def wanted(settings):
    """True when any finishing is switched on; with none on, the picture is the renderer's, untouched."""
    return bool(settings) and any(settings.get(name) for name in SWITCHES)


def problems(settings, camera_type=None):
    """Why the finishing asked for cannot be made, as sentences (none: it can). camera_type: the scene camera's
    Blender type (PERSP, ORTHO, PANO), which the lens flare needs to be PERSP."""
    found = []
    if not wanted(settings):
        return found
    if settings.get("glow") and not settings["glow_size"] > 0.0:
        found.append("finishing: the glow's size is %r; it must be more than 0" % settings["glow_size"])
    if settings.get("jpeg") and not 1 <= settings["jpeg_quality"] <= 100:
        found.append("finishing: the JPEG quality is %r; it must be 1 to 100" % settings["jpeg_quality"])
    if settings.get("palette") and not 2 <= settings["palette_colours"] <= 256:
        found.append("finishing: the palette has %r colours; it must have 2 to 256" % settings["palette_colours"])
    if settings.get("flare") and camera_type != "PERSP":
        found.append("finishing: the lens flare needs a perspective camera; the scene's camera is %s"
                     % (camera_type or "missing"))
    try:
        import numpy  # noqa: F401
    except ImportError:
        found.append("finishing needs numpy, which this Python does not have")
    return found


def finish(levels, width, height, settings, lights=()):
    """The finished picture: levels (width*height*3 bytes, top row first) through the finishing switched on, in this
    order: glow, lens flare, JPEG round trip, palette. `lights`: (x, y) in pixels from the top left where the sun and
    the moon project (outside the picture or hidden, they add no flare). Returns bytes of the same size."""
    import numpy as np
    picture = np.frombuffer(bytes(levels), dtype=np.uint8).reshape(height, width, 3)
    if settings.get("glow") or settings.get("flare"):
        shade = picture.astype(np.float64) / 255.0  # display values: the levels as the picture shows them
        if settings.get("flare"):  # the lights are looked for in the picture before the glow brightens round them
            found = [light_in_picture(shade, xy, settings["finish_threshold"]) for xy in lights]
        if settings.get("glow"):
            shade = glow(shade, settings["finish_threshold"], settings["glow_size"], settings["glow_strength"],
                         settings["glow_tint"])
        if settings.get("flare"):
            for light in found:
                if light is not None:
                    shade = lens_flare(shade, light[:2], settings["flare_strength"] * light[2])
        picture = to_levels(shade)
    if settings.get("jpeg"):
        picture = jpeg_round_trip(picture, int(settings["jpeg_quality"]))
    if settings.get("palette"):
        picture = reduce_palette(picture, int(settings["palette_colours"]))
    return picture.tobytes()


def to_levels(shade):
    """Display values (0 to 1) to 8-bit levels, rounded to the nearest."""
    import numpy as np
    return np.floor(np.clip(shade, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


def bright_mask(shade, threshold):
    """0 to 1 per pixel: how far its luma is above the threshold, reaching 1 at RAMP above it."""
    import numpy as np
    return np.clip((shade @ np.array(LUMA) - threshold) / RAMP, 0.0, 1.0)


def gaussian_blur(plane, sigma):
    """A 2-D plane blurred by a gaussian of `sigma` pixels, by FFT over the plane padded with zeros (no wrap)."""
    import numpy as np
    height, width = plane.shape
    pad = int(np.ceil(PAD_SIGMAS * sigma))
    padded = np.zeros((height + 2 * pad, width + 2 * pad))
    padded[pad:pad + height, pad:pad + width] = plane
    fy = np.fft.fftfreq(padded.shape[0])[:, None]
    fx = np.fft.rfftfreq(padded.shape[1])[None, :]
    spectrum = np.fft.rfft2(padded) * np.exp(-2.0 * (np.pi * sigma) ** 2 * (fx ** 2 + fy ** 2))
    return np.fft.irfft2(spectrum, s=padded.shape)[pad:pad + height, pad:pad + width]


def glow(shade, threshold, size, strength, tint):
    """The glow round the bright places: the bright mask blurred twice, a tight
    bloom and a wide halo `size` of the picture's width, each scaled to 1 at its brightest, weighted, times
    `strength`, tinted and screened over the picture: 1 - (1 - picture)(1 - glow * tint). It never darkens."""
    import numpy as np
    mask = bright_mask(shade, threshold)
    if not mask.any():
        return shade
    width = shade.shape[1]
    total = np.zeros(mask.shape)
    for share, weight in ((size * BLOOM_SHARE, BLOOM_WEIGHT), (size, HALO_WEIGHT)):
        blur = np.clip(gaussian_blur(mask, share * width), 0.0, None)
        top = blur.max()
        if top > 0.0:
            total += weight * blur / top
    amount = np.clip(strength * total, 0.0, 1.0)[..., None] * np.array(tint, dtype=np.float64)
    return 1.0 - (1.0 - shade) * (1.0 - amount)


def light_in_picture(shade, xy, threshold):
    """Where a light shows: (x, y, amount) near the place `xy` it projects to, or None when that place is outside the
    picture or nothing there is brighter than the threshold (behind a mountain, under the horizon). x, y: the bright
    pixels' centre, weighted by the bright mask; amount: the mask's largest value there (0 to 1)."""
    import numpy as np
    height, width = shade.shape[:2]
    x, y = float(xy[0]), float(xy[1])
    if not (0.0 <= x < width and 0.0 <= y < height):
        return None
    reach = max(2, int(round(FLARE_WINDOW * np.hypot(width, height))))
    x0, x1 = max(0, int(x) - reach), min(width, int(x) + reach + 1)
    y0, y1 = max(0, int(y) - reach), min(height, int(y) + reach + 1)
    mask = bright_mask(shade[y0:y1, x0:x1], threshold)
    total = mask.sum()
    if total <= 0.0:
        return None
    ys, xs = np.mgrid[y0:y1, x0:x1]
    return (float((mask * (xs + 0.5)).sum() / total), float((mask * (ys + 0.5)).sum() / total), float(mask.max()))


def lens_flare(shade, xy, strength):
    """The sprites of FLARE added to the picture along the line from the light at `xy` (pixels from the top left)
    through the picture's centre, each strength times `strength`."""
    import numpy as np
    height, width = shade.shape[:2]
    diagonal = float(np.hypot(width, height))
    lx, ly = float(xy[0]), float(xy[1])
    cx, cy = width / 2.0, height / 2.0
    ys, xs = np.mgrid[0:height, 0:width]
    xs, ys = xs + 0.5, ys + 0.5
    light = np.zeros(shade.shape)
    for place, radius, ring, colour, weight in FLARE:
        px, py = lx + place * (cx - lx), ly + place * (cy - ly)
        r = radius * diagonal
        d = np.hypot(xs - px, ys - py)
        if place == 0.0:
            alpha = np.clip(1.0 - d / r, 0.0, 1.0) ** 2  # the light's own soft glow
        elif ring > 0.0:
            alpha = np.clip(1.0 - np.abs(d - r) / (ring * diagonal), 0.0, 1.0)
        else:
            alpha = np.clip((r - d) / (DISC_EDGE * r), 0.0, 1.0)
        light += (weight * alpha)[..., None] * np.array(colour)
    return np.clip(shade + strength * light, 0.0, 1.0)


def jpeg_quantisers(quality):
    """The two 8 by 8 quantisation tables libjpeg uses at this quality (1 to 100), luminance then chrominance."""
    import numpy as np
    scale = 5000 // quality if quality < 50 else 200 - 2 * quality
    tables = []
    for base in (JPEG_LUMA, JPEG_CHROMA):
        table = (np.array(base, dtype=np.int64) * scale + 50) // 100
        tables.append(np.clip(table, 1, 255).reshape(8, 8).astype(np.float64))
    return tables


def dct_matrix():
    """The 8-point DCT-II of T.81 (A.3.3) as an orthonormal matrix: row u, column x."""
    import numpy as np
    u = np.arange(8)[:, None]
    x = np.arange(8)[None, :]
    matrix = np.cos((2 * x + 1) * u * np.pi / 16.0) * 0.5
    matrix[0, :] = np.sqrt(0.125)
    return matrix


def jpeg_plane(plane, table):
    """One component through the lossy steps of baseline JPEG: level shift, 8 by 8 DCT, quantise (rounded to the
    nearest, T.81 A.3.4), dequantise, inverse DCT, back to 0..255 rounded. The plane's sides are multiples of 8."""
    import numpy as np
    height, width = plane.shape
    blocks = plane.reshape(height // 8, 8, width // 8, 8).transpose(0, 2, 1, 3) - 128.0
    c = dct_matrix()
    coefficients = np.einsum("ux,abxy,vy->abuv", c, blocks, c)
    q = coefficients / table
    q = np.sign(q) * np.floor(np.abs(q) + 0.5)
    back = np.einsum("ux,abuv,vy->abxy", c, q * table, c) + 128.0
    back = np.clip(np.floor(back + 0.5), 0.0, 255.0)
    return back.transpose(0, 2, 1, 3).reshape(height, width)


def jpeg_round_trip(picture, quality):
    """The picture (height x width x 3 levels) saved once as a baseline JPEG at `quality` and read back: JFIF's YCbCr
    (ITU-T T.871), the chrominance halved both ways (4:2:0, a 2 by 2 average), each component through jpeg_plane, the
    chrominance doubled back by repeating, RGB again. The entropy coding a file adds loses nothing, so it is left out."""
    import numpy as np
    height, width = picture.shape[:2]
    tall, wide = -(-height // 16) * 16, -(-width // 16) * 16  # whole 16 by 16 blocks, the edges repeated
    rgb = np.pad(picture.astype(np.float64), ((0, tall - height), (0, wide - width), (0, 0)), mode="edge")
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    y = np.floor(0.299 * r + 0.587 * g + 0.114 * b + 0.5)
    cb = np.floor(-0.168736 * r - 0.331264 * g + 0.5 * b + 128.0 + 0.5)
    cr = np.floor(0.5 * r - 0.418688 * g - 0.081312 * b + 128.0 + 0.5)
    luma_table, chroma_table = jpeg_quantisers(quality)
    y = jpeg_plane(np.clip(y, 0, 255), luma_table)
    halves = []
    for plane in (np.clip(cb, 0, 255), np.clip(cr, 0, 255)):
        half = np.floor(plane.reshape(tall // 2, 2, wide // 2, 2).mean(axis=(1, 3)) + 0.5)
        half = jpeg_plane(half, chroma_table)
        halves.append(half.repeat(2, axis=0).repeat(2, axis=1) - 128.0)
    cb, cr = halves
    out = np.stack((y + 1.402 * cr, y - 0.344136 * cb - 0.714136 * cr, y + 1.772 * cb), axis=-1)
    return np.clip(np.floor(out + 0.5), 0, 255).astype(np.uint8)[:height, :width]


def bayer_matrix(size=BAYER_SIZE):
    """The ordered dither's thresholds (Bayer): size by size (a power of 2), values 0 .. size*size - 1."""
    import numpy as np
    matrix = np.zeros((1, 1), dtype=np.int64)
    while matrix.shape[0] < size:
        matrix = np.block([[4 * matrix, 4 * matrix + 2], [4 * matrix + 3, 4 * matrix + 1]])
    return matrix


def median_cut(colours, counts, n):
    """At most n palette colours (rounded levels) for the colours that occur, each with its count: Heckbert's median
    cut, splitting each time the box of the largest squared error at the counted median of its widest-spread axis."""
    import numpy as np
    colours = colours.astype(np.float64)
    counts = counts.astype(np.float64)

    def error(box):
        c, w = colours[box], counts[box]
        mean = (c * w[:, None]).sum(axis=0) / w.sum()
        return (((c - mean) ** 2) * w[:, None]).sum(axis=0)

    boxes = [np.arange(len(colours))]
    errors = [error(boxes[0])]
    while len(boxes) < n:
        order = sorted(range(len(boxes)), key=lambda k: -errors[k].sum())
        k = next((k for k in order if len(boxes[k]) > 1 and errors[k].sum() > 0.0), None)
        if k is None:
            break
        box = boxes[k]
        axis = int(np.argmax(errors[k]))
        box = box[np.lexsort((colours[box, (axis + 2) % 3], colours[box, (axis + 1) % 3], colours[box, axis]))]
        running = np.cumsum(counts[box])
        cut = int(np.searchsorted(running, running[-1] / 2.0)) + 1
        cut = min(max(cut, 1), len(box) - 1)
        boxes[k:k + 1] = [box[:cut], box[cut:]]
        errors[k:k + 1] = [error(box[:cut]), error(box[cut:])]
    palette = [np.floor((colours[b] * counts[b][:, None]).sum(axis=0) / counts[b].sum() + 0.5) for b in boxes]
    return np.unique(np.array(palette), axis=0)


def nearest(points, palette, chunk=1 << 15):
    """For each point (rows of 3), the index of the nearest palette colour (the first of equals)."""
    import numpy as np
    squares = (palette ** 2).sum(axis=1)
    out = np.empty(len(points), dtype=np.int64)
    for start in range(0, len(points), chunk):
        part = points[start:start + chunk]
        out[start:start + chunk] = np.argmin(squares[None, :] - 2.0 * part @ palette.T, axis=1)
    return out


def reduce_palette(picture, n):
    """The GIF finish: the picture in at most n colours (a median-cut palette of its own colours), chosen per pixel
    after an ordered (Bayer 8 by 8) dither offset of up to half the usual distance between neighbouring palette
    colours either way. A picture of n colours or fewer comes back unchanged."""
    import numpy as np
    height, width = picture.shape[:2]
    flat = picture.reshape(-1, 3)
    colours, counts = np.unique(flat, axis=0, return_counts=True)
    if len(colours) <= n:
        return picture
    palette = median_cut(colours, counts, n)
    if len(palette) > 1:
        apart = np.sqrt(((palette[:, None, :] - palette[None, :, :]) ** 2).sum(axis=2))
        np.fill_diagonal(apart, np.inf)
        spread = float(np.median(apart.min(axis=1)))
    else:
        spread = 0.0
    bayer = (bayer_matrix() + 0.5) / float(BAYER_SIZE * BAYER_SIZE) - 0.5
    offsets = np.tile(bayer, (-(-height // BAYER_SIZE), -(-width // BAYER_SIZE)))[:height, :width]
    shifted = flat.astype(np.float64) + (offsets.reshape(-1) * spread)[:, None]
    return palette[nearest(shifted, palette)].astype(np.uint8).reshape(height, width, 3)
