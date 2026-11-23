# SPDX-License-Identifier: GPL-3.0-or-later
# (Where exact_png is written: Blender's own rule for its output path; one file per frame.) Pure.
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from stecyk3d import outputs  # noqa: E402


class BlenderPlace(unittest.TestCase):
    def test_windows_path_with_no_drive_goes_on_the_drive_root(self):
        # what Cycles, Eevee and Stecyk3D all printed as `Saved: 'C:\out\lake.png'` for "out\lake.png"
        self.assertEqual(outputs.blender_place("out\\lake.png", "", True, "C:"), "C:\\out\\lake.png")
        self.assertEqual(outputs.blender_place("out/lake.png", "D:\\scenes", True, "C:"), "C:\\out/lake.png")
        self.assertEqual(outputs.blender_place("\\out\\lake.png", "", True, "E:"), "E:\\out\\lake.png")

    def test_windows_absolute_and_network_paths_stay(self):
        self.assertEqual(outputs.blender_place("D:\\pics\\a.png", "", True), "D:\\pics\\a.png")
        self.assertEqual(outputs.blender_place("\\\\server\\share\\a.png", "", True), "\\\\server\\share\\a.png")

    def test_blend_relative(self):
        self.assertEqual(outputs.blender_place("//out/a.png", os.path.join("x", "y"), True),
                         os.path.join("x", "y", "out/a.png"))
        # no blend file saved yet: Blender leaves it relative (the working folder), with no drive added
        self.assertEqual(outputs.blender_place("//out/a.png", "", True), "out/a.png")

    def test_elsewhere_relative_stays_relative(self):
        self.assertEqual(outputs.blender_place("out/lake.png", "", False), "out/lake.png")
        self.assertEqual(outputs.blender_place("/tmp/lake.png", "", False), "/tmp/lake.png")


class FrameFile(unittest.TestCase):
    def test_hashes_become_the_frame(self):
        self.assertEqual(outputs.frame_file("//exact_####.png", 7), "//exact_0007.png")
        self.assertEqual(outputs.frame_file("a##/b_###.png", 12), "a##/b_012.png")
        self.assertEqual(outputs.frame_file("b_#.png", 123), "b_123.png")

    def test_no_hashes_frame_before_the_extension(self):
        self.assertEqual(outputs.frame_file("lake.png", 3), "lake0003.png")
        self.assertEqual(outputs.frame_file("lake", 3), "lake0003")


if __name__ == "__main__":
    unittest.main()
