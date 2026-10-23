// SPDX-License-Identifier: MIT
// A beam exponent that is not a number, a light whose I is not a number, and the sun with
// switch bit 0x20000000, from the statements of.

use stecyk3d::shading::lighting::{falloff, light_switch};
use stecyk3d::shading::lights::{Beam, ConeOrder, beam_factor};
use stecyk3d::version::RenderAs;

const ALL: [RenderAs; 3] = [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50];

fn factor(v: RenderAs, beam: Beam, u: [f64; 3]) -> Option<f64> {
    beam_factor(v, beam, u, ConeOrder::of(v)).expect("no open item")
}

#[test]
fn single_exponent_nan_is_zero() {
    for v in ALL {
        for u in [[0.0, 0.2, 0.0], [0.1, 0.5, 0.05], [0.3, -0.4, 0.2]] {
            for (nan, zero) in [
                (Beam::Cone { exponent: f64::NAN }, Beam::Cone { exponent: 0.0 }),
                (Beam::Cylinder { exponent: f64::NAN }, Beam::Cylinder { exponent: 0.0 }),
            ] {
                assert_eq!(factor(v, nan, u), factor(v, zero, u), "{v:?} {u:?}");
            }
        }
    }
}

#[test]
fn pair_exponents_with_nan() {
    let n = f64::NAN;
    let u = [0.1, -0.2, 0.15];
    for v in ALL {
        for make in [|ex, ez| Beam::Pyramid { ex, ez }, |ex, ez| Beam::Boxed { ex, ez }] {
            // (NaN, 0) as (0, 0); (NaN, 1) as (1, 1); (NaN, NaN) as (0, 0).
            assert_eq!(factor(v, make(n, 0.0), u), factor(v, make(0.0, 0.0), u));
            assert_eq!(factor(v, make(n, 1.0), u), factor(v, make(1.0, 1.0), u));
            assert_eq!(factor(v, make(n, n), u), factor(v, make(0.0, 0.0), u));
            assert_eq!(factor(v, make(1.0, n), u), factor(v, make(1.0, 1.0), u));
            // (NaN, 2): the power with the NaN exponent is a NaN, so is the factor.
            assert!(factor(v, make(n, 2.0), u).expect("inside").is_nan());
            assert!(factor(v, make(2.0, n), u).expect("inside").is_nan());
        }
    }
}

#[test]
fn sun_with_range_bit() {
    let sw = light_switch::RANGE_FALL_OFF;
    // The sun's distance term is +infinity: a range of 0 or above puts nothing out, below 0 or
    // not a number lights fully; with bit 0x400 the range bit is not read.
    assert_eq!(falloff(RenderAs::Bryce50, sw, 5.0, f64::INFINITY), 0.0);
    assert_eq!(falloff(RenderAs::Bryce50, sw, 0.0, f64::INFINITY), 0.0);
    assert_eq!(falloff(RenderAs::Bryce50, sw, -1.0, f64::INFINITY), 1.0);
    assert_eq!(falloff(RenderAs::Bryce50, sw, f64::NAN, f64::INFINITY), 1.0);
    assert_eq!(falloff(RenderAs::Bryce50, sw | light_switch::NO_FALL_OFF, 5.0, f64::INFINITY), 1.0);
}
