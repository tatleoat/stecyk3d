# SPDX-License-Identifier: GPL-3.0-or-later
# (Making the substrate from the add-on). No bpy here: testable without Blender.
"""The command that makes the substrate from the user's Bryce installs, and the reading of its outcome."""

import os
import re
import shutil
import sys


def install_folders(text):
    """The installs the user named, one per line or separated by ';', blanks dropped."""
    parts = []
    for line in text.replace(";", "\n").splitlines():
        line = line.strip()
        if line:
            parts.append(line)
    return parts


VERSIONS = ("3.0", "3.1", "5.0")  # the three folder fields, in this order
EXTRACTOR_NAME = "stecyk3d-extract"
RENDERER_NAME = "stecyk3d-render"


def guess_version(path):
    """Which field an old free-text install path most likely belongs in, from its folder names ("" if unclear).
    Only for moving an old saved value across; the extractor still refuses a folder in the wrong field."""
    low = path.lower().replace("\\", "/")
    if re.search(r"(?<![0-9])(3\.1|31)(?![0-9])", low):
        return "3.1"
    if re.search(r"(?<![0-9])(3\.0|30)(?![0-9])", low) or "bryce3d" in low or "bryce 3d" in low:
        return "3.0"
    if re.search(r"(?<![0-9])5(\.0)?(?![0-9])", low) or "corel" in low or "daz" in low:
        return "5.0"
    return ""


def migrate_install_folders(text, fields):
    """Move an old ';'-separated install list into empty version fields: `fields` maps version -> path and the
    result is the new mapping (fields already set are kept; a path whose version is unclear goes to the first
    empty field left)."""
    out = dict(fields)
    rest = []
    for path in install_folders(text):
        v = guess_version(os.path.basename(path.rstrip("/\\")) or path)
        if v and not out.get(v):
            out[v] = path
        else:
            rest.append(path)
    for path in rest:
        for v in VERSIONS:
            if not out.get(v):
                out[v] = path
                break
    return out


def fields_set(fields):
    """The (version, folder) pairs the user filled in, in version order."""
    return [(v, fields[v]) for v in VERSIONS if fields.get(v)]


def extract_command(extractor, out, fields):
    """stecyk3d-extract --out OUT --bryce V=FOLDER ... ; `out` "" lets the extractor take's default
    place. `fields` is a version -> folder mapping (or, for old callers, a plain list of folders)."""
    command = [extractor]
    if out:
        command += ["--out", out]
    if isinstance(fields, dict):
        for v, folder in fields_set(fields):
            command += ["--bryce", "%s=%s" % (v, folder)]
        return command
    return command + list(fields)


def search_program(name, override="", addon_dir=None, env=None, home=None):
    """(path, tried): where the program `name` is. Order: the override from the Advanced preferences; a copy
    bundled beside the add-on (<addon>/bin, <addon>/../bin); a folder on PATH; the usual cargo folders
    (<CARGO_TARGET_DIR>/release|debug, a `target/release|debug` in any folder above the add-on, ~/.cargo/bin).
    `tried` lists what was looked at, for the message when nothing is found (path is "")."""
    env = os.environ if env is None else env
    exe = name + (".exe" if sys.platform.startswith("win") else "")
    tried = []

    def hit(path):
        tried.append(path)
        return os.path.isfile(path)

    if override:
        if hit(override):
            return override, tried
    here = addon_dir or os.path.dirname(os.path.abspath(__file__))
    for folder in (os.path.join(here, "bin"), os.path.join(os.path.dirname(here), "bin")):
        if hit(os.path.join(folder, exe)):
            return os.path.join(folder, exe), tried
    on_path = shutil.which(name, path=env.get("PATH"))
    if on_path:
        return on_path, tried
    tried.append("PATH")
    targets = []
    if env.get("CARGO_TARGET_DIR"):
        targets.append(env["CARGO_TARGET_DIR"])
    up = here
    for _ in range(6):
        up = os.path.dirname(up)
        targets.append(os.path.join(up, "target"))
    for target in targets:
        for kind in ("release", "debug"):
            if hit(os.path.join(target, kind, exe)):
                return os.path.join(target, kind, exe), tried
    home = home if home is not None else env.get("HOME", "")
    if home and hit(os.path.join(home, ".cargo", "bin", exe)):
        return os.path.join(home, ".cargo", "bin", exe), tried
    return "", tried


def program_missing_message(label, name, tried):
    return ("The %s program (`%s`) was not found. Put it beside the add-on (in a `bin` folder), on PATH, or in "
            "the cargo `target` folder of the stecyk3d crate; or name it under Advanced in the add-on preferences."
            % (label, name))


def problems(extractor, fields):
    """What stops the command from being run, as messages for the user ([] when it can run)."""
    found = []
    if not extractor or not os.path.isfile(extractor):
        found.append(program_missing_message("extractor", EXTRACTOR_NAME, []))
    pairs = fields_set(fields) if isinstance(fields, dict) else [("", f) for f in fields]
    if not pairs:
        found.append("no Bryce folder is set: fill in at least one of the Bryce 3.0, 3.1 and 5.0 folders")
    for v, folder in pairs:
        if not os.path.isdir(folder):
            found.append("%s%s is not a folder (it does not exist)" % ("Bryce %s: " % v if v else "", folder))
    return found


PROGRAMS = {"bryce 3d.exe": "3.x", "bryce5.exe": "5.0"}  # the program file each kind of install holds (any letter case)


def program_kind(folder):
    """Which Bryce a folder looks like, by its program file (any case; in the folder or one folder inside, as the
    extractor looks): "3.x" (Bryce 3D 3.0 or 3.1), "5.0", or "" for neither or no such folder."""
    def entries(d):
        try:
            return {e.lower(): e for e in os.listdir(d)}
        except OSError:
            return {}
    names = entries(folder)
    for low, kind in PROGRAMS.items():
        if low in names:
            return kind
    for name in sorted(names.values()):
        inside = os.path.join(folder, name)
        if os.path.isdir(inside):
            sub = entries(inside)
            for low, kind in PROGRAMS.items():
                if low in sub:
                    return kind
    return ""


def kind_mark(version, folder):
    """(ok, text) for the mark beside a folder field: whether the folder looks like an install of that version.
    A 3.x program file fits the 3.0 and 3.1 fields (the extractor tells those two apart by hash, when it runs)."""
    if not folder.strip():
        return None, "(optional: leave empty if you have none)"
    kind = program_kind(folder)
    if not kind:
        return False, "no Bryce program file here"
    if (kind == "5.0") != (version == "5.0"):
        return False, "looks like Bryce %s, not %s" % (kind, version)
    return True, "looks like a Bryce %s install" % ("3D " + version if kind == "3.x" else "5.0")


def file_lines(stdout):
    """The extractor's per-program-file lines ('Bryce 3D.exe: tables taken: perm, ...'), as 'Bryce 3D.exe: found perm, ...'."""
    out = []
    for line in stdout.splitlines():
        m = re.match(r"^(.*?): tables taken: (.*)$", line)
        if m:
            out.append("%s: found %s" % (m.group(1), m.group(2).strip() or "nothing"))
    return out


def parse_report(lines):
    """Per folder of an extractor run's standard output: [{"folder", "build", "kept"}] (kept None if unseen).
    A folder's block starts at its `recognised as` line and ends at its `N kept files taken` line."""
    blocks = []
    for line in lines:
        m = re.match(r"^(.*?): recognised as (.*)$", line)
        if m:
            blocks.append({"folder": m.group(1), "build": m.group(2).strip(), "kept": None})
            continue
        m = re.search(r": (\d+) kept files taken", line)
        if m and blocks:
            blocks[-1]["kept"] = int(m.group(1))
    return blocks


def success_lines(pairs, stdout):
    """One line per version set, in order: 'Bryce 3.0: Bryce 3D 3.0 recognised, 21 kept files'."""
    blocks = parse_report(stdout.splitlines())
    out = []
    for (v, _folder), b in zip(pairs, blocks):
        kept = "" if b["kept"] is None else ", %d kept files" % b["kept"]
        out.append("Bryce %s: %s recognised%s" % (v, b["build"], kept))
    return out


def failure_lines(stderr):
    """The extractor's refusals, one per line, without the 'refused: ' lead."""
    return [l[len("refused: "):] if l.startswith("refused: ") else l for l in stderr.splitlines() if l.strip()]
