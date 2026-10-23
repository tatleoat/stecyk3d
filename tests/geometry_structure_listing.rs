// SPDX-License-Identifier: MIT
// : the boxes the structure lists a disc and a tree by, from the entry's
// own statements (no released vector holds them: the entry says they matter only in ties).

use stecyk3d::geometry::structure::{Layout, Listed, Structure};
use stecyk3d::geometry::{Object, Placement, Scene, Shape};
use stecyk3d::version::RenderAs;

fn listed(shape: Shape, version: RenderAs) -> Structure {
    let objects = vec![Object { shape, placement: Placement::IDENTITY }];
    let scene = Scene { objects, ..Scene::default() };
    Structure::build(
        &scene,
        &[Listed::Object(0)],
        ([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]),
        version,
        Layout::Tree,
        1,
    )
}

#[test]
fn a_disc_is_listed_by_its_24_points_widened() {
    for version in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
        let s = listed(Shape::Disc, version);
        let e = &s.entries()[0];
        assert_eq!(e.object, Some(0));
        // x and z reach -1 and 1 (the points at 90 and 270 degrees), widened by 1 %.
        assert!((e.bounds.max[0] - 1.02).abs() < 1e-12 && (e.bounds.min[2] + 1.02).abs() < 1e-12);
        // y is flat: 1e-7 at each end as 5.0, nothing as 3.0 and 3.1.
        let flat = if version == RenderAs::Bryce50 { 1e-7 } else { 0.0 };
        assert_eq!((e.bounds.min[1], e.bounds.max[1]), (-flat, flat), "{version:?}");
    }
}
