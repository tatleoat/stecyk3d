# SPDX-License-Identifier: GPL-3.0-or-later
# (The Rendered viewport: the picture on the GPU)
"""The viewport's picture as a GPU texture, cheaply. numpy is Blender's; `gpu` is imported only where it is drawn.

The picture is kept as one 32-bit word a pixel, in a buffer that lives as long as the picture's size: the word is the
pixel's red, green and blue bytes under a fixed top byte of 0x40, so that as a float it is an ordinary number between 2
and 8 that no driver changes on the way (Blender takes only float data for a texture). A change to the picture rewrites
only the rows that changed. A draw uploads the buffer as an R32F texture and looks the colours up on the GPU, instead of
building a list of four floats a pixel on the main thread, which took some 150 ms at 1.5 million pixels."""

import numpy as np

TOP_BYTE = 0x40


class Words:
    """The picture's words, kept between draws."""

    def __init__(self):
        self.size = (0, 0)
        self.bytes4 = None  # (height, width, 4) uint8: blue, green, red, TOP_BYTE: one little-endian word a pixel
        self.floats = None  # the same memory as flat float32

    def sync(self, levels, size, dirty):
        """Bring the words up to the picture `levels` (RGB bytes, top row first, `size` = (width, height)). `dirty` is
        the (first, last) row that changed since the last call, or None. Returns True when anything changed."""
        width, height = size
        if size != self.size or self.bytes4 is None:
            self.size = size
            self.bytes4 = np.empty((height, width, 4), dtype=np.uint8)
            self.bytes4[:, :, 3] = TOP_BYTE
            self.floats = self.bytes4.view(np.float32).reshape(width * height)
            dirty = (0, height - 1)
        if dirty is None:
            return False
        first, last = max(dirty[0], 0), min(dirty[1], height - 1)
        if last < first:
            return False
        rgb = np.frombuffer(levels, dtype=np.uint8).reshape(height, width, 3)[first:last + 1]
        out = self.bytes4[first:last + 1]
        out[:, :, 0] = rgb[:, :, 2]
        out[:, :, 1] = rgb[:, :, 1]
        out[:, :, 2] = rgb[:, :, 0]
        return True

    def word(self, x, y):
        """The word of one pixel (the tests)."""
        return int(self.bytes4.view(np.uint32)[y, x, 0])


_drawing = {}  # the shader, its quad and the colour table: made once, with a GPU context


def drawing():
    """The shader that draws a texture of pixel words, the quad over the region, and the colour table. Level k is the
    scene-linear value k / 255 (the renderer's levels), rounded to a half float as the viewport's framebuffer
    keeps them; Blender then puts the scene's view transform on the result."""
    if not _drawing:
        import gpu
        from gpu_extras.batch import batch_for_shader
        info = gpu.types.GPUShaderCreateInfo()
        info.sampler(0, "FLOAT_2D", "words")
        info.sampler(1, "FLOAT_2D", "table")
        info.vertex_in(0, "VEC2", "pos")
        info.vertex_in(1, "VEC2", "texCoord")
        iface = gpu.types.GPUStageInterfaceInfo("stecyk3d_words_iface")
        iface.smooth("VEC2", "uv")
        info.vertex_out(iface)
        info.fragment_out(0, "VEC4", "FragColor")
        info.vertex_source("void main() { uv = texCoord; gl_Position = vec4(pos, 0.0, 1.0); }")
        # the word under the fragment (rows from the top, the quad's v going up), then its red, green and blue bytes
        # (bits 16, 8 and 0) through the table; nearest pixels, so a pixel size above 1 shows as blocks (3.4)
        info.fragment_source(
            "void main() {"
            "  ivec2 size = textureSize(words, 0);"
            "  ivec2 p = clamp(ivec2(floor(uv * vec2(size))), ivec2(0), size - 1);"
            "  uint w = floatBitsToUint(texelFetch(words, ivec2(p.x, size.y - 1 - p.y), 0).r);"
            "  FragColor = vec4(texelFetch(table, ivec2(int((w >> 16u) & 255u), 0), 0).r,"
            "                   texelFetch(table, ivec2(int((w >> 8u) & 255u), 0), 0).r,"
            "                   texelFetch(table, ivec2(int(w & 255u), 0), 0).r, 1.0);"
            "}")
        shader = gpu.shader.create_from_info(info)
        table = (np.arange(256, dtype=np.float32) / 255.0).astype(np.float16).astype(np.float32)
        _drawing.update(
            shader=shader,
            table=gpu.types.GPUTexture((256, 1), format="R32F", data=gpu.types.Buffer("FLOAT", 256, table)),
            quad=batch_for_shader(shader, "TRI_FAN", {"pos": ((-1, -1), (1, -1), (1, 1), (-1, 1)),
                                                      "texCoord": ((0, 0), (1, 0), (1, 1), (0, 1))}))
    return _drawing


def upload(words):
    """The words as an R32F texture (the whole of it: Blender's Python cannot change part of a texture)."""
    import gpu
    width, height = words.size
    return gpu.types.GPUTexture((width, height), format="R32F",
                                data=gpu.types.Buffer("FLOAT", width * height, words.floats))


def draw(texture):
    """The texture of words over the whole region."""
    import gpu
    from mathutils import Matrix
    d = drawing()
    gpu.state.depth_test_set("NONE")
    gpu.state.blend_set("NONE")
    with gpu.matrix.push_pop():
        gpu.matrix.load_matrix(Matrix.Identity(4))
        with gpu.matrix.push_pop_projection():
            gpu.matrix.load_projection_matrix(Matrix.Identity(4))
            d["shader"].bind()
            d["shader"].uniform_sampler("words", texture)
            d["shader"].uniform_sampler("table", d["table"])
            d["quad"].draw(d["shader"])
