# SPDX-License-Identifier: GPL-3.0-or-later
# Every preset of every material and texture library of a real Bryce install applies: the library is read by the
# renderer program's reader, each material preset becomes a Bryce material (components.apply_material_preset) and
# each texture preset a component (components.texture_preset_component). A preset the reader cannot read is refused
# with the reader's reason, as the add-on's command refuses it; anything else is a failure. A library the reader
# cannot open is listed as unread, not as a failure: this checks the presets the add-on gets.
#
# Needs the renderer program (STECYK3D_RENDER, else target/release/stecyk3d-render beside the add-on's folder) and
# libraries: BRYCE_PRESETS, an install's Presets folder (walked as the extractor walks it), and/or BRYCE_SUBSTRATE,
# whose kept libraries are read. Skipped without them. Run directly to print the report:
#   BRYCE_PRESETS="C:/Program Files (x86)/Corel/Bryce 5/Presets" python tests/test_all_presets.py
import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import components, presets, reader  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PRESET_DEPTH = 4  # the extractor's: the Presets folder and at most four levels below it


def program():
    named = os.environ.get("STECYK3D_RENDER")
    if named:
        return named if os.path.isfile(named) else None
    for name in ("stecyk3d-render.exe", "stecyk3d-render"):
        path = os.path.join(REPO, "target", "release", name)
        if os.path.isfile(path):
            return path
    return None


def install_libraries(folder, depth=0):
    """[(label, path)] of the .mat and .brt files of a Presets folder, as the extractor collects them."""
    out = []
    try:
        names = sorted(os.listdir(folder))
    except OSError:
        return out
    for name in names:
        path = os.path.join(folder, name)
        if os.path.isfile(path) and presets.library_kind(name):
            out.append((path, path))
        elif os.path.isdir(path) and depth < PRESET_DEPTH:
            out.extend(install_libraries(path, depth + 1))
    return out


def sources():
    """[(label, read)] of every library named by BRYCE_PRESETS and BRYCE_SUBSTRATE; read() gives its JSON."""
    exe = program()
    if exe is None:
        return None, []
    out = []
    folder = os.environ.get("BRYCE_PRESETS")
    if folder:
        out += [(label, lambda p=path: reader.read_library(exe, p)) for label, path in install_libraries(folder)]
    substrate = os.environ.get("BRYCE_SUBSTRATE")
    if substrate:
        for entry in reader.kept_libraries(exe, substrate, "5.0"):
            path = entry["path"]
            if presets.library_kind(path):
                out.append(("kept:" + path, lambda p=path: reader.read_kept_library(exe, substrate, p)))
    return exe, out


def non_numbers(value, where=""):
    """The places in a value that hold a number that is not finite, or nothing where a number belongs."""
    if isinstance(value, dict):
        return [p for k, v in value.items() for p in non_numbers(v, "%s.%s" % (where, k))]
    if isinstance(value, list):
        return [p for k, v in enumerate(value) for p in non_numbers(v, "%s[%d]" % (where, k))]
    if isinstance(value, float) and not math.isfinite(value):
        return [where]
    return []


def unkept(components_list):
    """The components whose saved result holds nothing where a number belongs (the node keeps it as numbers)."""
    out = []
    for comp in components_list:
        saved = (comp.get("record") or {}).get("saved")
        if saved and any(v is None for v in saved["numbers"]):
            out.append(comp["name"] or "column %s" % comp["column"])
    return out


def check(libraries):
    """{"libraries", "presets", "applied", "refused": [(library, preset, reason)], "failed": [...]}."""
    report = {"libraries": 0, "presets": 0, "applied": 0, "refused": [], "failed": [], "unread": []}
    for label, read in libraries:
        try:
            library = read()
        except reader.ReadRefused as refused:
            report["unread"].append((label, None, "the library cannot be read: %s" % refused))
            continue
        report["libraries"] += 1
        kind = library["kind"]
        for index, name, _ in presets.offered_presets(library):
            report["presets"] += 1
            try:
                if kind == "materials":
                    value = components.apply_material_preset(library, index)
                    channels = {"channels": value["channels"], "mix": value.get("mix")}
                    bad = non_numbers(channels)
                    if bad:
                        raise ValueError("not a number at %s" % ", ".join(bad))
                    comps = value["components"] + (value["mix"]["second"]["components"] if value.get("mix") else [])
                else:
                    comps = components.add_component(components.empty_material(),
                                                     components.texture_preset_component(library, index))["components"]
                if unkept(comps):
                    raise ValueError("a saved result holds null: %s" % ", ".join(unkept(comps)))
            except components.Refused as refused:
                unreadable = library["presets"][index].get("unreadable")
                if unreadable is None or unreadable not in str(refused):
                    report["failed"].append((label, name, "refused: %s" % refused))
                else:
                    report["refused"].append((label, name, unreadable))
                continue
            except Exception as error:  # noqa: BLE001 - every other exception is the failure being looked for
                report["failed"].append((label, name, "%s: %s" % (type(error).__name__, error)))
                continue
            report["applied"] += 1
    return report


class EveryPreset(unittest.TestCase):
    def test_every_preset_of_every_library_applies(self):
        exe, libraries = sources()
        if exe is None or not libraries:
            self.skipTest("needs the renderer program and BRYCE_PRESETS or BRYCE_SUBSTRATE")
        report = check(libraries)
        self.assertGreater(report["presets"], 0)
        self.assertEqual(report["failed"], [])
        for _, _, reason in report["refused"]:
            self.assertTrue(reason.startswith("unsupported"), reason)


def main():
    exe, libraries = sources()
    if exe is None or not libraries:
        raise SystemExit("set BRYCE_PRESETS and/or BRYCE_SUBSTRATE, and build the renderer program")
    report = check(libraries)
    print("%d libraries read, %d unread, %d presets, %d applied, %d refused, %d failed" % (
        report["libraries"], len(report["unread"]), report["presets"], report["applied"], len(report["refused"]),
        len(report["failed"])))
    for what in ("unread", "refused", "failed"):
        for label, name, reason in report[what]:
            print("%s: %s | %s | %s" % (what.upper(), label, name, reason))


if __name__ == "__main__":
    main()
