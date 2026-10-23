# SPDX-License-Identifier: GPL-3.0-or-later
# , (Making the substrate)
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from stecyk3d import extract  # noqa: E402


class ExtractCommand(unittest.TestCase):
    def test_folders_split_on_lines_and_semicolons(self):
        self.assertEqual(extract.install_folders(" /a/Bryce 3D 3.0 ;\n/b/Bryce 5;;  "), ["/a/Bryce 3D 3.0", "/b/Bryce 5"])
        self.assertEqual(extract.install_folders(""), [])

    def test_command_with_and_without_out(self):
        self.assertEqual(extract.extract_command("x", "/s.bin", ["/a", "/b"]), ["x", "--out", "/s.bin", "/a", "/b"])
        self.assertEqual(extract.extract_command("x", "", ["/a"]), ["x", "/a"])

    def test_problems(self):
        with tempfile.TemporaryDirectory() as folder:
            program = os.path.join(folder, "stecyk3d-extract")
            open(program, "w").close()
            self.assertEqual(extract.problems(program, [folder]), [])
            self.assertEqual(len(extract.problems("", [])), 2)
            self.assertIn("is not a folder", extract.problems(program, [os.path.join(folder, "none")])[0])

    def test_command_with_version_fields(self):
        fields = {"3.0": "/a", "3.1": "", "5.0": "/c"}
        self.assertEqual(extract.extract_command("x", "/s.bin", fields),
                         ["x", "--out", "/s.bin", "--bryce", "3.0=/a", "--bryce", "5.0=/c"])

    def test_problems_with_fields(self):
        with tempfile.TemporaryDirectory() as folder:
            program = os.path.join(folder, "stecyk3d-extract")
            open(program, "w").close()
            self.assertEqual(extract.problems(program, {"3.1": folder}), [])
            self.assertIn("no Bryce folder is set", extract.problems(program, {"3.0": ""})[0])
            self.assertIn("Bryce 5.0: ", extract.problems(program, {"5.0": folder + "/none"})[0])

    def test_old_list_migrates(self):
        moved = extract.migrate_install_folders("/x/Bryce31;/x/Bryce 5;/y/Bryce3D", {"3.0": "", "3.1": "", "5.0": ""})
        self.assertEqual(moved, {"3.0": "/y/Bryce3D", "3.1": "/x/Bryce31", "5.0": "/x/Bryce 5"})
        kept = extract.migrate_install_folders("/x/Bryce31", {"3.1": "/mine"})
        self.assertEqual(kept["3.1"], "/mine")  # a set field is never overwritten
        self.assertEqual(kept["3.0"], "/x/Bryce31")  # the unclear leftover goes to the first empty field

    def test_search_order(self):
        with tempfile.TemporaryDirectory() as root:
            addon = os.path.join(root, "proj", "blender_addon", "stecyk3d")
            os.makedirs(addon)
            exe = extract.EXTRACTOR_NAME
            env = {"PATH": os.path.join(root, "onpath"), "CARGO_TARGET_DIR": ""}

            def touch(*parts):
                path = os.path.join(root, *parts)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                open(path, "w").close()
                os.chmod(path, 0o755)
                return path

            self.assertEqual(extract.search_program(exe, "", addon, env, root)[0], "")
            cargo = touch("proj", "target", "debug", exe)
            self.assertEqual(extract.search_program(exe, "", addon, env, root)[0], cargo)
            on_path = touch("onpath", exe)
            self.assertEqual(extract.search_program(exe, "", addon, env, root)[0], on_path)
            bundled = touch("proj", "blender_addon", "stecyk3d", "bin", exe)
            self.assertEqual(extract.search_program(exe, "", addon, env, root)[0], bundled)
            own = touch("mine", exe)
            self.assertEqual(extract.search_program(exe, own, addon, env, root)[0], own)
            self.assertEqual(extract.search_program(exe, os.path.join(root, "gone"), addon, env, root)[0], bundled)

    def test_report_reading(self):
        out = ("/a/B3: recognised as Bryce 3D 3.0\nBryce 3D.exe: tables taken: perm\n/a/B3: 21 kept files taken under X\n"
               "/a/B5: recognised as Corel Bryce 5.0\n/a/B5: 86 kept files taken under Y\n")
        self.assertEqual(extract.success_lines([("3.0", "/a/B3"), ("5.0", "/a/B5")], out),
                         ["Bryce 3.0: Bryce 3D 3.0 recognised, 21 kept files",
                          "Bryce 5.0: Corel Bryce 5.0 recognised, 86 kept files"])
        self.assertEqual(extract.failure_lines("refused: /a: no Bryce program file found\n"),
                         ["/a: no Bryce program file found"])

    def test_kind_mark_and_file_lines(self):
        with tempfile.TemporaryDirectory() as root:
            for sub, exe in (("a", "bryce 3d.EXE"), ("b/inner", "Bryce5.exe")):
                os.makedirs(os.path.join(root, sub))
                open(os.path.join(root, sub, exe), "w").close()
            self.assertEqual(extract.program_kind(os.path.join(root, "a")), "3.x")
            self.assertEqual(extract.program_kind(os.path.join(root, "b")), "5.0")
            self.assertEqual(extract.program_kind(os.path.join(root, "gone")), "")
            self.assertTrue(extract.kind_mark("3.1", os.path.join(root, "a"))[0])
            self.assertFalse(extract.kind_mark("5.0", os.path.join(root, "a"))[0])
            self.assertIsNone(extract.kind_mark("3.0", "")[0])
        self.assertEqual(extract.file_lines("x\nBryce 3D.exe: tables taken: perm, leaf\n"),
                         ["Bryce 3D.exe: found perm, leaf"])


if __name__ == "__main__":
    unittest.main()
