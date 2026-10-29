# SPDX-License-Identifier: GPL-3.0-or-later
# (Where exact_png is written: the place Blender gives its own output path, and one file per frame of an animation.)
# Pure Python: the blend file's folder, the platform and the Windows drive are passed in.
"""The path of a picture the add-on writes itself, resolved the way Blender resolves its output path."""

import os
import re

HASHES = re.compile(r"#+")


def blender_place(path, blend_folder, windows, windows_drive="C:"):
    """`path` where Blender itself would write it (BLI_path_abs, as `Saved: ...` shows): `//` is the blend file's
    folder (with no file saved yet, the path after `//` stays relative: the working folder); on Windows a path with
    no drive letter and no `//` (`out\\lake.png`, `\\out\\lake.png`) goes on the root of the drive Windows is
    installed on (`C:\\out\\lake.png`), as Cycles and Eevee put their pictures; anywhere else it stays relative to the
    working folder."""
    if path.startswith("//"):
        rest = path[2:]
        return os.path.join(blend_folder, rest) if blend_folder else rest
    if windows and not re.match(r"^[A-Za-z]:", path) and not path.startswith(("\\\\", "//")):
        return windows_drive + "\\" + path.lstrip("\\/")
    return path


def frame_file(path, frame):
    """One file per frame of an animation: the last run of `#` becomes the frame number with as many digits (at
    least that many), as in Blender's output path; with no `#`, the four-digit frame number goes before the
    extension (`lake.png` -> `lake0003.png`)."""
    runs = list(HASHES.finditer(path))
    if runs:
        run = runs[-1]
        return path[:run.start()] + "%0*d" % (run.end() - run.start(), frame) + path[run.end():]
    stem, extension = os.path.splitext(path)
    return "%s%04d%s" % (stem, frame, extension)
