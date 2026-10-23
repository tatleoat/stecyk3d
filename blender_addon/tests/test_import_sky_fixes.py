# SPDX-License-Identifier: GPL-3.0-or-later
# (Import: sky and render settings; found by tests/blender_vs_bryce)
"""Pure-Python checks of what an import takes from a document's sky and render settings: the sun in the camera's
frame, the render group's own values, the numbers Blender's floats cannot hold, and the smallest picture."""

import math
import unittest

from stecyk3d import importing


class SunInWorld(unittest.TestCase):
    def test_identity_camera_keeps_the_direction(self):
        rows = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
        self.assertEqual(importing.sun_in_world(rows, [0.1, 0.2, 0.3]), [0.1, 0.2, 0.3])

    def test_quarter_turn_about_y(self):
        # right, up, forward of a camera turned 90 degrees about up: the view's forward is the world's +x
        rows = [[0, 0, -1], [0, 1, 0], [1, 0, 0]]
        got = importing.sun_in_world(rows, [0.0, 0.0, 1.0])  # straight ahead of the camera
        self.assertEqual([round(x, 12) for x in got], [1.0, 0.0, 0.0])

    def test_singular_rows_leave_it(self):
        self.assertEqual(importing.sun_in_world([[0, 0, 0]] * 3, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0])


def a_sky(**more):
    sky = {"flags": 0x100, "sun_direction_bryce": [0.0, 0.5, -0.8], "plain": [0.3, 0.5, 0.9]}
    sky.update(more)
    return sky


class ExactNumbers(unittest.TestCase):
    def test_infinity_and_nan_are_kept_beside_the_settings(self):
        plan = importing.sky_plan(a_sky(plain=[math.nan, 0.5, 0.9], discs=[math.inf] + [0.0] * 14))
        self.assertEqual(sorted(plan["exact_numbers"]), ["discs", "plain"])
        self.assertTrue(math.isinf(plan["exact_numbers"]["discs"][0]))

    def test_finite_skies_have_none(self):
        self.assertNotIn("exact_numbers", importing.sky_plan(a_sky()))


class DocumentsOwnSunAndDoubles(unittest.TestCase):
    def test_the_sun_direction_is_kept_as_the_document_holds_it(self):
        plan = importing.sky_plan(a_sky(sun_direction_bryce=[0.6188527477552761, 1.2377054955105522, -1.4439897447623107]))
        self.assertEqual(plan["sun_direction_exact"], [0.6188527477552761, 1.2377054955105522, -1.4439897447623107])

    def test_exact_sky_puts_the_documents_doubles_in_the_description(self):
        from stecyk3d import convert
        result = {"sky": {"fog": 0.004999999888, "cloud_detail": 0.5, "cloud_detail_z": 0.5, "discs": [1.0] * 15},
                  "sun": {"intensity": 1.0}}
        convert.exact_sky(result, {"fog": 0.005, "sky5_cloud_detail": [0.1, 0.2], "sun_intensity": 0.7,
                                   "sky5_star_orientation": [1.0, 2.0, 3.0]})
        self.assertEqual(result["sky"]["fog"], 0.005)
        self.assertEqual((result["sky"]["cloud_detail"], result["sky"]["cloud_detail_z"]), (0.1, 0.2))
        self.assertEqual(result["sun"]["intensity"], 0.7)
        self.assertNotIn("star_orientation", result["sky"])  # a Bryce 5 field the description has only with the part on

    def test_gather_uses_the_exact_number_only_while_the_property_is_untouched(self):
        import json
        try:
            from stecyk3d import gather
        except ImportError:  # gather needs bpy
            self.skipTest("no bpy")
        world = {"bryce_exact_doubles": json.dumps({"fog": {"exact": 0.005, "held": 0.004999999888}})}
        self.assertEqual(gather.unchanged_exact_sky(world, {"fog": 0.004999999888}), {"fog": 0.005})
        self.assertEqual(gather.unchanged_exact_sky(world, {"fog": 0.25}), {})
        sun = {"bryce_exact_sun": json.dumps({"direction": [0.0, 2.0, 0.0], "held": [0.0, 0.0, 1.0]})}
        self.assertEqual(gather.unchanged_exact_sun(sun, [0.0, 0.0, 1.0]), [0.0, 2.0, 0.0])
        self.assertIsNone(gather.unchanged_exact_sun(sun, [0.0, 1.0, 0.0]))


class SmallestPicture(unittest.TestCase):
    def test_one_by_one_is_refused_with_the_reason(self):
        document = {"picture_size": [1, 1], "sky": a_sky(), "objects": [], "materials": [], "textures": [],
                    "pictures": [], "camera": None}
        plan = importing.plan_import("x.br3", document, None, {"objects": True},
                                     {"program": "3.0", "has_world": False})
        self.assertEqual(plan["result"], "refused")
        self.assertIn("1 by 1", plan["reason"])
        self.assertIn("at least 4 by 4", plan["reason"])




class SunLampColour(unittest.TestCase):
    def test_light_part_colour_goes_to_the_lamp(self):
        self.assertEqual(importing.sun_lamp_data({"sun_light_color": [1.0, 0.5, 0.25]}), {"color": [1.0, 0.5, 0.25]})

    def test_absent_or_not_finite_leaves_the_lamp_alone(self):
        self.assertEqual(importing.sun_lamp_data({}), {})
        self.assertEqual(importing.sun_lamp_data({"sun_light_color": [1.0, float("nan"), 0.0]}), {})
        self.assertEqual(importing.sun_lamp_data({"sun_light_color": [float("inf"), 0.0, 0.0]}), {})


if __name__ == "__main__":
    unittest.main()
