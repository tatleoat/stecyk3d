// SPDX-License-Identifier: MIT
//! Tests for drawn from forward differences (Perlin 1985) and Blinn's perturbation.
//! The scale and distance fade are Open and not tested.

use stecyk3d::geom::{self, Vec3};
use stecyk3d::shading::StdArith;
use stecyk3d::shading::bump::*;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn forward_difference_of_a_linear_field_is_its_gradient() {
    let f = |p: Vec3| 2.0 * p[0] - 3.0 * p[1] + 0.5 * p[2];
    let p = [0.25, -1.0, 2.0];
    let g = forward_difference_gradient(p, f(p), 1.0 / 1024.0, f);
    assert!(close(g[0], 2.0, 1e-9));
    assert!(close(g[1], -3.0, 1e-9));
    assert!(close(g[2], 0.5, 1e-9));
}

#[test]
fn forward_difference_takes_three_extra_samples_one_per_axis() {
    let mut seen = Vec::new();
    let p = [1.0, 2.0, 3.0];
    let h = 0.5;
    forward_difference_gradient(p, 0.0, h, |q| {
        seen.push(q);
        0.0
    });
    assert_eq!(seen.len(), 3);
    for (axis, q) in seen.iter().enumerate() {
        for i in 0..3 {
            let expected = if i == axis { p[i] + h } else { p[i] };
            assert_eq!(q[i], expected);
        }
    }
}

#[test]
fn zero_gradient_or_factor_leaves_the_normal() {
    let n = [0.0, 0.0, 1.0];
    assert_eq!(perturb_normal(n, [0.0; 3], 3.0, &StdArith), n);
    assert_eq!(perturb_normal(n, [1.0, 2.0, 0.0], 0.0, &StdArith), n);
    // A gradient along the normal does not tilt it.
    assert_eq!(perturb_normal(n, [0.0, 0.0, 5.0], 1.0, &StdArith), n);
}

#[test]
fn normal_tilts_away_from_the_gradient_and_stays_unit() {
    let n = [0.0, 0.0, 1.0];
    let m = perturb_normal(n, [1.0, 0.0, 0.0], 0.5, &StdArith);
    assert!(m[0] < 0.0);
    assert!(close(m[1], 0.0, 1e-15));
    assert!(close(geom::length(m), 1.0, 1e-15));
    // Height field z = h(x) with slope s has normal proportional to (-s, 0, 1).
    assert!(close(m[0] / m[2], -0.5, 1e-15));
}

struct Recording(std::cell::Cell<Option<Option<f64>>>);
impl BumpScale for Recording {
    fn factor(&self, amount: f64, distance: Option<f64>) -> f64 {
        self.0.set(Some(distance));
        amount
    }
}

#[test]
fn distance_reaches_the_scale_only_when_fade_is_on() {
    let s = Recording(std::cell::Cell::new(None));
    let n = [0.0, 0.0, 1.0];
    let mut input = BumpInput { amount: 1.0, fade_with_distance: false, distance: 7.0 };
    bumped_normal(n, [1.0, 0.0, 0.0], &input, &s, &StdArith);
    assert_eq!(s.0.get(), Some(None));
    input.fade_with_distance = true;
    bumped_normal(n, [1.0, 0.0, 0.0], &input, &s, &StdArith);
    assert_eq!(s.0.get(), Some(Some(7.0)));
}
