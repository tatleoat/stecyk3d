# SPDX-License-Identifier: GPL-3.0-or-later
"""The parts of the move from the first release's names that need no Blender."""
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from stecyk3d import migrate  # noqa: E402


class MoveKeys(unittest.TestCase):
    def test_old_keys_move_and_go(self):
        holder = {"big_bruce_shown": 2, "big_bruce_whole_import": "/doc.br5", "other": 1}
        self.assertEqual(migrate.move_keys(holder), 2)
        self.assertEqual(holder, {"stecyk3d_shown": 2, "stecyk3d_whole_import": "/doc.br5", "other": 1})

    def test_a_value_under_the_new_key_wins(self):
        holder = {"big_bruce_shown": 2, "stecyk3d_shown": 0}
        self.assertEqual(migrate.move_keys(holder), 0)
        self.assertEqual(holder, {"stecyk3d_shown": 0})

    def test_nothing_to_move(self):
        holder = {"stecyk3d_shown": 1}
        self.assertEqual(migrate.move_keys(holder), 0)
        self.assertEqual(holder, {"stecyk3d_shown": 1})


class ProgramNames(unittest.TestCase):
    def test_old_program_names_get_the_new_ones(self):
        self.assertEqual(migrate.renamed_program("/a/bin/vista-render"), "/a/bin/stecyk3d-render")
        self.assertEqual(migrate.renamed_program("C:/a/vista-extract.exe"), "C:/a/stecyk3d-extract.exe")

    def test_other_paths_stay(self):
        self.assertEqual(migrate.renamed_program("/a/bin/stecyk3d-render"), "/a/bin/stecyk3d-render")
        self.assertEqual(migrate.renamed_program("/a/mine"), "/a/mine")


if __name__ == "__main__":
    unittest.main()
