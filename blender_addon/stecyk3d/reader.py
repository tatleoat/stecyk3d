# SPDX-License-Identifier: GPL-3.0-or-later
# (a library read through the composite file of component scene-files; refused with the reader's
# reason),, (a document
# read whole before anything is made; refused with the reader's reason), as revised
# (Who reads the files: the renderer program)
"""Runs the renderer program's `read` form and hands back its JSON. Pure Python (no bpy)."""

import base64
import json
import os
import struct
import subprocess


class ReadRefused(Exception):
    """The reader refused the file; the message is its reason (Errors)."""


def _run(program, args):
    if not program:
        raise ReadRefused("no renderer program is set in the add-on's preferences; it reads Bryce's files")
    if not os.path.isfile(program):
        raise ReadRefused("the renderer program %r does not exist" % program)
    try:
        done = subprocess.run([program, "read"] + list(args), capture_output=True)
    except OSError as error:
        raise ReadRefused("the renderer program %r cannot be run: %s" % (program, error))
    if done.returncode != 0:
        reason = done.stderr.decode("utf-8", "replace").strip() or "the reader stopped with status %d" % (
            done.returncode)
        raise ReadRefused(reason)
    return json.loads(done.stdout.decode("utf-8"))


def read_library(program, path):
    """: a library file's presets (`kind`, `presets`, `textures`, `pictures`)."""
    return _run(program, ["library", path])


def read_kept_library(program, substrate, kept_path):
    """: a library the user's extractor kept, read through the substrate's check ."""
    return _run(program, ["library", "--substrate", substrate, "--kept", kept_path])


def kept_libraries(program, substrate, version):
    """: the extractor's libraries, those of the program `version` first: [{path, folder}]."""
    return _run(program, ["kept-libraries", "--substrate", substrate, "--as", version])


def read_document(program, path, version=None):
    """: the document read whole (materials, textures, pictures, sky, objects, camera, picture_size).
    With `version` ("3.0", "3.1", "5.0") the document as that program opens it: its animation tracks applied at the
    opening time , which is the state the program draws."""
    if not os.path.isfile(path):
        raise ReadRefused("%s: no such file" % path)
    return _run(program, ["document", path] + (["--as", version] if version else []))


def heights_of(grid):
    """The heights of a `{size, heights_base64}` grid: 16-bit unsigned, little-endian, row after row."""
    data = base64.b64decode(grid["heights_base64"])
    count = len(data) // 2
    if count != grid["size"] * grid["size"]:
        raise ReadRefused("the reader's grid holds %d heights, not %d" % (count, grid["size"] ** 2))
    return list(struct.unpack("<%dH" % count, data))


def new_relief(program, seed):
    """: the 128 by 128 relief of `seed` (1 to 4,294,967,295): (size, heights)."""
    grid = _run(program, ["relief", str(int(seed))])
    return grid["size"], heights_of(grid)
