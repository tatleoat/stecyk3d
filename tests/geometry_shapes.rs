// SPDX-License-Identifier: MIT
// Tests of the primitive forms  and the grazing limits , on rays whose answers
// follow from the forms alone (bit-exact results: tests/geometry_vectors.rs).

use stecyk3d::geometry::{Object, Part, Placement, Range, Ray, Scene, Shape, ShapeContext, intersect_shape, texture};
use stecyk3d::version::RenderAs;

/// A pyramid table of our own (the unit normal of the side 2x + y = 0.5).
fn ctx(version: RenderAs) -> ShapeContext {
    let s5 = 5f64.sqrt();
    ShapeContext { version, pyramid_normal: [1.0 / s5, 2.0 / s5], shadow: false }
}

const ALL: Range = Range { near: 1e-9, far: f64::INFINITY };

fn down_from(x: f64, z: f64) -> Ray {
    Ray { origin: [x, 10.0, z], dir: [0.0, -1.0, 0.0] }
}

fn hit_t(shape: Shape, ray: Ray) -> Option<f64> {
    intersect_shape(shape, &ray, ALL, &ctx(RenderAs::Bryce31)).map(|h| h.t)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + b.abs())
}

#[test]
fn tops_of_the_forms() {
    // Heights of each form's top surface where a downward ray at (x, z) meets it.
    let cases: &[(Shape, f64, f64, Option<f64>)] = &[
        (Shape::Plane, 3.0, 4.0, Some(0.0)),
        (Shape::Slab, 100.0, -7.0, Some(1.0)),
        (Shape::Cube, 0.25, -0.25, Some(0.5)),
        (Shape::Cube, 0.6, 0.0, None),
        (Shape::Sphere, 0.0, 0.0, Some(1.0)),
        (Shape::Sphere, 0.6, 0.0, Some(0.8)),
        (Shape::Cylinder, 0.9, 0.0, Some(1.0)),
        (Shape::Cylinder, 1.1, 0.0, None),
        // Down the axis: the apex is a double root, which renders as 3.1 do not count
        // (cone-002), so the ray meets the base.
        (Shape::Cone, 0.0, 0.0, Some(-1.0)),
        (Shape::Cone, 0.5, 0.0, Some(0.0)), // radius (1 - y)/2 = 0.5 at y = 0
        (Shape::Pyramid, 0.0, 0.0, Some(0.5)),
        (Shape::Pyramid, 0.25, 0.1, Some(0.0)), // 2x + y = 0.5
        (Shape::Pyramid, -0.1, -0.25, Some(0.0)),
        (Shape::Square, 0.99, -0.99, Some(0.0)),
        (Shape::Square, 1.01, 0.0, None),
        (Shape::Disc, 0.7, 0.7, Some(0.0)),
        (Shape::Disc, 0.71, 0.71, None),
        (Shape::Torus { tube: 0.25, ring: 1.0 }, 1.0, 0.0, Some(0.25)),
        (Shape::Torus { tube: 0.25, ring: 1.0 }, 0.0, 0.0, None),
        (Shape::Torus { tube: 0.25, ring: 1.0 }, 0.0, -1.2, Some(0.15)),
    ];
    for &(shape, x, z, top) in cases {
        let got = hit_t(shape, down_from(x, z)).map(|t| 10.0 - t);
        match (got, top) {
            (Some(g), Some(w)) => assert!(close(g, w), "{shape:?} at ({x}, {z}): top {g}, want {w}"),
            (None, None) => {}
            _ => panic!("{shape:?} at ({x}, {z}): got {got:?}, want {top:?}"),
        }
    }
}

#[test]
fn sides_and_normals() {
    let c = ctx(RenderAs::Bryce31);
    let from_left = |y: f64| Ray { origin: [-10.0, y, 0.0], dir: [1.0, 0.0, 0.0] };
    let h = intersect_shape(Shape::Sphere, &from_left(0.0), ALL, &c).unwrap();
    assert!(close(h.t, 9.0));
    let h = intersect_shape(Shape::Cylinder, &from_left(0.5), ALL, &c).unwrap();
    assert!(close(h.t, 9.0));
    assert_eq!(h.normal, [-1.0, 0.0, 0.0]);
    let h = intersect_shape(Shape::Cone, &from_left(0.0), ALL, &c).unwrap();
    assert!(close(h.t, 9.5));
    // Pyramid side -x at y = 0: x = -0.25; its normal comes from the table.
    let h = intersect_shape(Shape::Pyramid, &from_left(0.0), ALL, &c).unwrap();
    assert!(close(h.t, 9.75));
    assert_eq!(h.normal, [-c.pyramid_normal[1], c.pyramid_normal[0], 0.0]);
    // Torus from the side through the tube: enters at x = -1.25.
    let h = intersect_shape(Shape::Torus { tube: 0.25, ring: 1.0 }, &from_left(0.0), ALL, &c).unwrap();
    assert!(close(h.t, 8.75), "{}", h.t);
    // From inside the sphere, the far side.
    let inside = Ray { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] };
    assert!(close(intersect_shape(Shape::Sphere, &inside, ALL, &c).unwrap().t, 1.0));
}

#[test]
fn entry_limits_and_orders() {
    let c = ctx(RenderAs::Bryce31);
    let from_centre = |d: [f64; 3]| Ray { origin: [0.0; 3], dir: d };
    // : |d_x| <= 1e-6 meets neither x face, even from inside.
    assert!(intersect_shape(Shape::Cube, &from_centre([1e-6, 0.0, 0.0]), ALL, &c).is_none());
    let h = intersect_shape(Shape::Cube, &from_centre([2e-6, 0.0, 0.0]), ALL, &c).unwrap();
    assert_eq!((h.t, h.normal), (250_000.0, [1.0, 0.0, 0.0]));
    let h = intersect_shape(Shape::Cube, &from_centre([1e-6, 1.0, 0.0]), ALL, &c).unwrap();
    assert_eq!((h.t, h.normal), (0.5, [0.0, 1.0, 0.0]));
    // : an edge reports the first face in the order x, y, z.
    let h = intersect_shape(Shape::Cube, &from_centre([1.0, 1.0, 0.0]), ALL, &c).unwrap();
    assert_eq!((h.t, h.normal), (0.5, [1.0, 0.0, 0.0]));
    // : the base needs |d_y| > 1e-6.
    let up = |dy: f64| Ray { origin: [0.0, -1.0, 0.0], dir: [0.0, dy, 0.0] };
    let h = intersect_shape(Shape::Pyramid, &up(2e-6), ALL, &c).unwrap();
    assert_eq!(h.normal, [0.0, 1.0, 0.0]);
    let h = intersect_shape(Shape::Pyramid, &up(1e-6), ALL, &c);
    assert!(h.map_or(true, |h| h.normal != [0.0, 1.0, 0.0]), "the base is not met at |d_y| = 1e-6");
    // : a ray parallel to the cylinder's axis meets no wall, only a cap.
    let h = intersect_shape(Shape::Cylinder, &Ray { origin: [0.5, -5.0, 0.0], dir: [0.0, 1.0, 0.0] }, ALL, &c).unwrap();
    assert_eq!((h.t, h.normal), (4.0, [0.0, -1.0, 0.0]));
}

#[test]
fn pyramid_side_texture() {
    // : on the sides toward -x and +x u = z / (0.5 - y) + 0.5, du = (0, 0, 1);
    // toward -z and +z u = x / (0.5 - y) + 0.5, du = (1, 0, 0); v = y + 0.5, dv = (0, 1, 0).
    // Through whole hits, with a table of our own (the function alone, bit for bit:
    // geometry_pyramid_sides in tests/geometry_vectors.rs).
    let c = ctx(RenderAs::Bryce31);
    let h = intersect_shape(Shape::Pyramid, &Ray { origin: [-10.0, 0.0, 0.1], dir: [1.0, 0.0, 0.0] }, ALL, &c).unwrap();
    assert_eq!(h.uv, [0.1 / 0.5 + 0.5, 0.5]);
    assert_eq!((h.du, h.dv), ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]));
    let h = intersect_shape(Shape::Pyramid, &Ray { origin: [0.1, -0.25, -10.0], dir: [0.0, 0.0, 1.0] }, ALL, &c).unwrap();
    assert!(close(h.point[2], -0.375));
    assert_eq!(h.uv, [0.1 / 0.75 + 0.5, 0.25]);
    assert_eq!((h.du, h.dv), ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
}

#[test]
fn grazing_limits() {
    // : plane and slab 0.0001 in 3.0, 0.00001 in 3.1 and 5.0; square and disc
    // 0.00001 in every version, and equal also misses.
    let ray = |dy: f64| Ray { origin: [0.0, 1.0, 0.0], dir: [1.0, -dy, 0.0] };
    let plane30 = |dy| intersect_shape(Shape::Plane, &ray(dy), ALL, &ctx(RenderAs::Bryce30));
    let plane31 = |dy| intersect_shape(Shape::Plane, &ray(dy), ALL, &ctx(RenderAs::Bryce31));
    assert!(plane30(0.00005).is_none());
    assert!(plane31(0.00005).is_some());
    assert!(plane31(0.000005).is_none());
    assert!(plane30(0.0002).is_some());
    let near = Ray { origin: [0.0, 0.00001, 0.0], dir: [0.0, -0.00001, 0.0] };
    for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
        assert!(intersect_shape(Shape::Square, &near, ALL, &ctx(v)).is_none());
        assert!(intersect_shape(Shape::Disc, &near, ALL, &ctx(v)).is_none());
    }
    let steeper = Ray { origin: [0.0, 0.00002, 0.0], dir: [0.0, -0.00002, 0.0] };
    assert!(intersect_shape(Shape::Disc, &steeper, ALL, &ctx(RenderAs::Bryce50)).is_some());
}

#[test]
fn placed_objects_and_nearest() {
    // A sphere scaled by 2 and moved to z = 10, and a cube at z = 5.
    let big = Placement::new([[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]], [0.0, 0.0, 10.0]).unwrap();
    let near = Placement::new([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0, 0.0, 5.0]).unwrap();
    let scene = Scene {
        objects: vec![Object { shape: Shape::Sphere, placement: big }, Object { shape: Shape::Cube, placement: near }],
        meshes: vec![],
        stored_meshes: vec![], terrains: vec![], paths: vec![], trees: vec![],
        structure: None,
        groups: Default::default(),
        metaballs: None,
    };
    let ray = Ray { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] };
    let h = scene.intersect(&ray, ALL, &ctx(RenderAs::Bryce31)).unwrap();
    assert_eq!(h.object, 1);
    assert!(close(h.t, 4.5));
    // A cube's face reports the positive axis , so the ray runs with it.
    assert_eq!(h.normal, [0.0, 0.0, 1.0]);
    assert!(!h.from_outside);
    let only_sphere = Range { near: 6.0, far: f64::INFINITY };
    let h = scene.intersect(&ray, only_sphere, &ctx(RenderAs::Bryce31)).unwrap();
    assert_eq!(h.object, 0);
    assert!(close(h.t, 8.0));
    assert!(close(h.local_point[2], -1.0));
}

#[test]
fn torus_v_is_plus_zero() {
    // : v is +0, not -0, where y = r (the tube's
    // top) and where w = rho - R is 0 with y above 0.
    let (tube, ring) = (0.25, 1.0);
    let shape = Shape::Torus { tube, ring };
    let points = [
        [1.0, tube, 0.0],    // w = 0 and y = r
        [0.0, tube, 1.0],    // w = 0 and y = r
        [1.1, tube, 0.2],    // w > 0, y = r
        [-0.3, tube, -0.8],  // w < 0, y = r
        [0.0, 0.125, -1.0],  // w = 0, 0 < y < r
        [-1.0, 0.0625, 0.0], // w = 0, 0 < y < r
    ];
    for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
        for p in points {
            let t = texture(shape, Part::Curved, p, v);
            assert_eq!(t.uv[1].to_bits(), 0, "{p:?} {v:?}: v = {:?}", t.uv[1]);
        }
    }
}
