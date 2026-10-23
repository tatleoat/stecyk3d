// SPDX-License-Identifier: MIT
//! The octave count and method limits stated 2026-10-22 (Limits): a method that is not
//! 1 to 12 is method 0, and a count below 0 gives, by method, what count 0 gives (0, 1, 2, 3, 5,
//! 6, 7, 11, 12), N(0.5·p) (4), s0 alone with g = n + 8 (8) or what a count of 5 gives (9, 10).
//! Also the orders and arithmetic of the same release  that no vector
//! reaches, as relations the text states.
//!

use stecyk3d::textures::octaves::{NoiseSample, octave_value};
use stecyk3d::textures::orders::{Order3, TextureOrders};
use stecyk3d::version::RenderAs;

fn noise(p: [f64; 3]) -> NoiseSample {
    // A smooth stand-in with values in 0 to 1 (the function is ours: only relations are tested).
    NoiseSample::Double(0.5 + 0.45 * (1.3 * p[0] + 0.7 * p[1] + 2.1 * p[2] + 0.2).sin())
}

fn run(method: i64, n: i64, p: [f64; 3]) -> Option<stecyk3d::textures::octaves::Octaves> {
    let orders = TextureOrders::of(RenderAs::Bryce30);
    let start = Some([0.31, 0.62, 0.17]);
    octave_value(method, n, false, p, &orders, &noise, None, start)
}

const P: [f64; 3] = [0.37, -1.21, 2.05];

#[test]
fn a_method_that_is_not_one_to_twelve_is_method_zero() {
    for n in [-4, -1, 0, 1, 3, 9] {
        let zero = run(0, n, P).unwrap();
        for method in [13, 14, 100, 12345, -1, -10, i64::from(i32::MAX / 10)] {
            assert_eq!(run(method, n, P).unwrap(), zero, "method {method}, count {n}");
        }
    }
}

#[test]
fn a_count_below_zero_is_count_zero_for_most_methods() {
    for method in [0, 1, 2, 3, 5, 6, 7, 11, 12] {
        let zero = run(method, 0, P).unwrap();
        for n in [-1, -5, -9] {
            assert_eq!(run(method, n, P).unwrap(), zero, "method {method}, count {n}");
        }
    }
}

#[test]
fn method_four_below_zero_is_the_noise_at_half_the_point() {
    let half = P.map(|c| c * 0.5);
    for n in [-1, -4, -9] {
        let got = run(4, n, P).unwrap();
        assert_eq!(got.value, noise(half).stored(), "count {n}");
        assert_eq!(got.left, P, "count {n}");
    }
}

#[test]
fn method_eight_below_zero_is_s0_alone_with_g_of_n_plus_eight() {
    for n in [-1, -4, -8, -9] {
        let g = (n + 8) as f64;
        let at = |p: [f64; 3]| noise(p).stored();
        let want = ((at(P.map(|c| c + 0.15)) - at(P)) * g).abs();
        let want = if want >= 1.0 { 1.0 } else { want };
        assert_eq!(run(8, n, P).unwrap().value, want, "count {n}");
    }
}

#[test]
fn methods_nine_and_ten_below_zero_give_all_the_stages() {
    for method in [9, 10] {
        let five = run(method, 5, P).unwrap();
        for n in [-1, -3, -9] {
            assert_eq!(run(method, n, P).unwrap(), five, "method {method}, count {n}");
        }
    }
}

#[test]
fn a_count_above_nine_is_refused() {
    assert!(run(0, 10, P).is_none());
}

#[test]
fn the_bump_gradient_squares_are_summed_by_version() {
    // : (y² + z²) + x² as 3.0 and 3.1, (x² + y²) + z² as 5.0.
    assert_eq!(TextureOrders::of(RenderAs::Bryce30).bump_squares, Order3::BcA);
    assert_eq!(TextureOrders::of(RenderAs::Bryce31).bump_squares, Order3::BcA);
    assert_eq!(TextureOrders::of(RenderAs::Bryce50).bump_squares, Order3::AbC);
}
