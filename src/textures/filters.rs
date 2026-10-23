// SPDX-License-Identifier: MIT
//! The filters: a value shaped by a filter number with n, a, b and the surface context.
//!
//! (revised d004a59; : every filter 0 to 13 as a function
//! (Filters)

use crate::numerics::to_i32;
use crate::numerics::extended::ExtValue;
use crate::numerics::{ln_ext, sqrt_as, x87};
use crate::version::RenderAs;

use super::orders::Smoothstep;
use super::settings::{Filter, SurfaceContext};

/// The last filter number.
pub const LAST_FILTER: i64 = 13;

/// log2 e as the x87 unit holds it: the 64-bit significand nearest it, times 2^-63.
const LOG2_E_EXT: ExtValue = ExtValue { negative: false, mantissa: 0xb8aa_3b29_5c17_f0bc, exponent: -63 };
/// Filter 8's constants.
const F8_ALTITUDE: f64 = 1.2;
const F8_SLOPE: f64 = 0.5;
const F8_BASE: f64 = 0.33;
const F8_SCALE: f64 = 4.0;
/// Filter 13's base.
const F13_BASE: f64 = 0.9;

/// C(t): 0 below 0 or not a number, 1 above 1.
fn clamp(t: f64) -> f64 {
    if t > 1.0 {
        1.0
    } else if t >= 0.0 {
        t
    } else {
        0.0
    }
}

/// E(t), e^t as the x87 unit forms it.
pub fn exp_x87(t: f64) -> f64 {
    if !t.is_finite() {
        return t.exp(); // e^t itself: +inf, 0 for −inf, NaN
    }
    let y = LOG2_E_EXT.mul_to_f64(t);
    let i = y.round_ties_even();
    let f = x87::f2xm1_plus_one(y - i);
    scale2(f, i)
}

/// f·2^i rounded once to binary64.
fn scale2(f: f64, i: f64) -> f64 {
    if f == 0.0 || !f.is_finite() || i.is_nan() {
        return f * 2f64.powf(i);
    }
    let i = i.clamp(-4000.0, 4000.0) as i32;
    crate::numerics::extended::scaled_to_f64(f, i)
}

/// The filter's value. `lambda` forms filter 5's natural logarithm (binary64, the entry's
/// Open: vector-provisional).
pub fn filtered(
    filter: &Filter,
    v: f64,
    s: SurfaceContext,
    version: RenderAs,
    smooth: Smoothstep,
) -> Option<f64> {
    let (n, a, b) = (filter.n, f64::from(filter.a), f64::from(filter.b));
    let sm = |t: f64| smooth.eval(t);
    let linear = || a * v + b;
    Some(match filter.number {
        0 => v,
        1 => {
            let t = linear();
            if n <= 0 {
                return Some(1.0);
            }
            let nf = n as f64;
            (0..n).map(|i| i as f64 / nf).find(|&l| !(l < t)).unwrap_or(1.0)
        }
        2 => {
            let t = linear();
            let phi = if t > 0.0 {
                t - f64::from(to_i32(t))
            } else {
                let m = -t;
                1.0 - (m - f64::from(to_i32(m)))
            };
            sm(phi)
        }
        3 => {
            let c = x87::sin_ext(a * v).add_to_f64(1.0);
            clamp(b + c * 0.5)
        }
        4 => {
            let t = clamp(linear().abs());
            if t < 0.5 { (t + 0.5) * t } else { t }
        }
        5 => {
            if a == 2.0 {
                clamp(v * v + b)
            } else if a == 0.5 {
                clamp(sqrt_as(version, v) + b)
            } else {
                // : λ = ln 2 (64 bits) times log2(v) as an
                // extended value, ⟨λ·a⟩ the one rounding of the product; at 0 −∞, below 0 or
                // not a number a not-a-number (binary64 there, as before).
                let la = match ln_ext(v) {
                    Some(lambda) => lambda.mul_to_f64(a),
                    None => (if v == 0.0 { f64::NEG_INFINITY } else { v.ln() }) * a,
                };
                clamp(exp_x87(la) + b)
            }
        }
        6 => {
            let t = (b + v) * a;
            exp_x87(-(t * t))
        }
        7 => clamp(linear()),
        8 => {
            let (alt, slope) = (s.altitude, s.slope);
            let t = (alt * F8_ALTITUDE - (n as f64 * slope) * F8_SLOPE) * a + b * v;
            let g = (alt - F8_BASE) * F8_SCALE;
            if !(g > 0.0) {
                0.0
            } else if g < 1.0 {
                clamp(g * t)
            } else {
                clamp(t)
            }
        }
        9 => clamp((a * s.slope + b) * v),
        10 => clamp((a * s.altitude + b) * v),
        11 => clamp((a * s.orientation + b) * v),
        12 => sm(sm(clamp(linear()))),
        13 => {
            let mut k = F13_BASE - n as f64 * s.slope;
            if k < 0.0 {
                k = 0.0;
            }
            sm(sm(clamp(((a * v + b) + s.altitude) * k)))
        }
        _ => return None,
    })
}
