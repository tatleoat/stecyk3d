// SPDX-License-Identifier: MIT
//! Unit tests of the camera's preview ray.

use super::*;

fn turned_camera() -> CameraInput {
    let (s, c) = (0.3f64.sin(), 0.3f64.cos());
    CameraInput {
        right: [c, 0.0, -s],
        up: [0.0, 1.0, 0.0],
        forward: [s, 0.0, c],
        centre: [1.0, 3.0, -11.0],
        fov: 1.2,
        size: 1.0,
        aspect: 1.0,
        pan: [0.1, -0.05],
        zoom: [1.3, 1.3],
        projection: Projection::Perspective,
        width: 80,
        height: 60,
    }
}

/// : the preview's ray is the ray of for the middle of the
/// pixel, formed in another order from points of the world (about 11 from the origin) on a plane 1e-5 high, so they differ by the roundings of those sums (relative 1e-10 here).
#[test]
fn preview_ray_is_the_frame_ray_to_the_last_bits() {
    for version in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
        let frame = CameraModel::new(&turned_camera(), version);
        let preview = frame.clone().for_preview();
        for (x, y) in [(0u32, 0u32), (40, 30), (79, 59), (83, 63)] {
            let a = frame.ray(x as f64 + 0.5, y as f64 + 0.5);
            let b = preview.ray(x as f64 + 0.5, y as f64 + 0.5);
            assert_eq!(a.origin, b.origin);
            for k in 0..3 {
                assert!((a.dir[k] - b.dir[k]).abs() < 1e-8, "{version:?} ({x}, {y}) {k}");
            }
            assert!((a.start - b.start).abs() <= 1e-8 * a.start, "{version:?} ({x}, {y}) start");
        }
    }
}
