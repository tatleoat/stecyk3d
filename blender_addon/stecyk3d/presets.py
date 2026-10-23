# SPDX-License-Identifier: GPL-3.0-or-later
# , (the libraries offered: a folder's top level, `.mat`
#       and `.brt` in any letter case, by the file name without the ending; a preset by its name with every tab at
#       its end removed, else `preset N`; with `presets_folder` empty, the extractor's libraries, the program's first)
# (offering)
"""Which files are preset libraries, the libraries a folder offers, and the presets a library offers. Pure Python."""

import os

MATERIAL_ENDING = ".mat"  # [interface]
TEXTURE_ENDING = ".brt"  # [interface]


def library_kind(filename):
    """: "materials" for a name ending in .mat, "textures" for .brt, whatever the letter case; else None."""
    lower = filename.lower()
    if lower.endswith(MATERIAL_ENDING):
        return "materials"
    if lower.endswith(TEXTURE_ENDING):
        return "textures"
    return None


def library_label(filename):
    """: a library is offered by its file name without the ending."""
    return os.path.basename(filename)[:-len(MATERIAL_ENDING)]


def offered_libraries(folder):
    """: {"materials": [(label, path)], "textures": [...]}, the folder's top level only.

    A folder that does not exist or cannot be listed offers nothing. The order (free) is the names'."""
    offered = {"materials": [], "textures": []}
    try:
        names = sorted(os.listdir(folder))
    except OSError:
        return offered
    for name in names:
        path = os.path.join(folder, name)
        kind = library_kind(name)
        if kind is not None and os.path.isfile(path):
            offered[kind].append((library_label(name), path))
    return offered


def none_offered_message(kind, folder_set):
    """Errors: the list says that no library is offered, naming what to change (wording ours)."""
    what = "material" if kind == "materials" else "texture"
    if folder_set:
        return ("No %s library (.%s) in the folder of the Presets folder setting; choose another folder"
                % (what, "mat" if kind == "materials" else "brt"))
    return ("No %s library: set the Presets folder setting to your Bryce install's presets, or keep the "
            "libraries with the extractor (its Keep libraries button) and set the substrate" % what)


def offered_name(name, index):
    """: the name with every tab at its end removed; where nothing is left, `preset N`."""
    shown = name.rstrip("\t")
    return shown if shown else "preset %d" % index


def offered_presets(library):
    """: [(index, name as offered, description)] of a library as the reader gives it ."""
    return [(p["index"], offered_name(p["name"], p["index"]), p["description"]) for p in library["presets"]]


def kept_offered(listing):
    """with `presets_folder` empty: the extractor's libraries as the reader lists them (the program's own
    folders first), split by kind: {"materials": [(label, kept path)], "textures": [...]}. The label carries the
    folder (Freedom)."""
    offered = {"materials": [], "textures": []}
    for entry in listing:
        path = entry["path"]
        kind = library_kind(path)
        if kind is None:
            continue
        label = library_label(path.rsplit("/", 1)[-1])
        if entry.get("folder"):
            label = "%s (%s)" % (label, entry["folder"])
        offered[kind].append((label, path))
    return offered
