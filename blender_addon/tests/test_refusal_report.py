# SPDX-License-Identifier: GPL-3.0-or-later
# (The refusal report). Pure: the wording of the report.
import unittest

from stecyk3d import refusal_report as r


class RefusalReport(unittest.TestCase):
    def test_lines_are_split_trimmed_and_not_repeated(self):
        self.assertEqual(r.lines_of(["a\nb", " b ", ""], "c\n\nd"), ["a", "b", "c", "d"])

    def test_summary_is_short_and_names_the_text(self):
        lines = ["x" * 300, "second", "third"]
        text = r.summary("Not rendered", lines)
        self.assertLess(len(text), 160)
        self.assertTrue(text.endswith("see Text Editor > Stecyk3D report"))
        self.assertIn("3 problems", text)
        self.assertIn("1 problem)", r.summary("Not rendered", ["only"]))

    def test_text_has_one_problem_per_line(self):
        body = r.text_of("Not rendered", ["one", "two"]).splitlines()
        self.assertEqual(body, ["Not rendered:", "  one", "  two"])


if __name__ == "__main__":
    unittest.main()
