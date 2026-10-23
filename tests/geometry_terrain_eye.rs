// SPDX-License-Identifier: MIT
// and (the own box from the document's stored
// y numbers).

use stecyk3d::geometry::relief::{Relief, ReliefSwitches};
use stecyk3d::version::RenderAs;

fn relief(version: RenderAs, closing: u32) -> Relief {
    Relief::new(
        version,
        64,
        &vec![40000u16; 64 * 64],
        ReliefSwitches { mirrored: false, smooth: true, closing, solid: 0, holds_without_faces: false },
        [-0.00001, 1.00001],
    )
}

// Observed 2026-10-22 as 3.1 : a terrain of n = 64 closed by the closing word 0x1, the camera
// inside its footprint and under its surface at own (20, y, 20): with y + 32767.5 = 25.8 the terrain
// holds the eye, and with y + 32767.5 = 100.8 it does not (the test with the bounds -0.00001 n and
// 1.00001 n: a layer about n height units deep at the bottom of the range of heights); the test with
// the bounds and M would hold the eye there too.
#[test]
fn eye_in_a_terrain_as_three_one() {
    let r = relief(RenderAs::Bryce31, 1);
    assert!(r.holds_eye([20.0, 25.8 - 32767.5, 20.0]));
    assert!(!r.holds_eye([20.0, 100.8 - 32767.5, 20.0]));
    assert!(r.holds([20.0, 100.8 - 32767.5, 20.0]), "the point test of GEO-16.2a still holds it");
    // above the bounds' top 1.00001 n = 64.00064 and under the lower -0.00064 the eye is outside
    assert!(r.holds_eye([20.0, 64.0 - 32767.5, 20.0]));
    assert!(!r.holds_eye([20.0, 64.01 - 32767.5, 20.0]));
    assert!(!r.holds_eye([20.0, -0.01 - 32767.5, 20.0]));
    // outside the footprint
    assert!(!r.holds_eye([40.0, 25.8 - 32767.5, 20.0]));
    // a terrain with no closing face holds no eye (bit 0x40 aside)
    assert!(!relief(RenderAs::Bryce31, 0).holds_eye([20.0, 25.8 - 32767.5, 20.0]));
}

// As 3.0 no picture shows a terrain holding the eye (the scene gives nothing where the original's test
// passes: ours holds none); as 5.0 the pictures agree with the point test.
#[test]
fn eye_in_a_terrain_other_versions() {
    let p = [20.0, 25.8 - 32767.5, 20.0];
    assert!(!relief(RenderAs::Bryce30, 1).holds_eye(p));
    let five = relief(RenderAs::Bryce50, 1);
    for y in [25.8, 100.8, 20000.0, 40000.0] {
        let q = [20.0, y - 32767.5, 20.0];
        assert_eq!(five.holds_eye(q), five.holds(q));
    }
}

// : x and z of the own box run from -(h + 0.01) to h + 0.01, h the whole part of n / 2; y is the
// stored box's two numbers as the document holds them (-32767.51 to 32767.51 in every document observed).
#[test]
fn own_box_from_the_stored_numbers() {
    let r = relief(RenderAs::Bryce30, 0);
    let (lo, hi) = r.own_box();
    assert_eq!((lo, hi), ([-32.01, -32767.51, -32.01], [32.01, 32767.51, 32.01]));
    let mirrored = Relief::new(
        RenderAs::Bryce30,
        64,
        &vec![0u16; 64 * 64],
        ReliefSwitches { mirrored: true, smooth: true, closing: 0, solid: 0, holds_without_faces: false },
        [0.0, 1.0],
    );
    let (lo, hi) = mirrored.own_box();
    assert_eq!((lo[1], hi[1]), (-65535.01, 65535.01));
}
