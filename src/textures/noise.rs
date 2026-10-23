// SPDX-License-Identifier: MIT
//! The noise types as functions of the point in noise space: those that read no table (0, 1,
//! 2, 3, 19, 25). Every other type has an Open part (or needs a table whose use is Open) and is
//! refused before evaluation (`super::open_points`).
//!
//! (revised d67ee0c; : types 0, 1, 2, 3
//! 19, 25 as functions; Variants),,
//! (Noise types)

use crate::numerics::to_i32;
use crate::numerics::x87;

use super::orders::TextureOrders;
use super::settings::Dimension;

/// Type 2's scale, period, and the half below which h is 1.
const CHECK_SCALE: f64 = 512.0;
const CHECK_PERIOD: i64 = 3217;
const CHECK_HALF: i64 = 1608;
/// Type 3's period P (not 2π), K and H (not π).
#[allow(clippy::approx_constant)]
const TRI_P: f64 = 6.2831853072;
const TRI_K: f64 = 0.15915494309137826;
const TRI_H: f64 = 3.141592653589763;
/// Type 3's turn of the axes in 2-D and 3-D.
const TRI_TURN: f64 = 0.866025;
/// Type 19's Q (the nearest double to the decimal, not 1/π).
#[allow(clippy::approx_constant)]
const WAVE_Q: f64 = 0.31830988618;

/// The value of type 0 at every point.
pub const TYPE0_VALUE: f64 = 0.5;

/// Whether a type is stated as a function that reads no table (for every version that has it).
pub fn is_table_free(type_number: i64) -> bool {
    matches!(type_number, 0 | 1 | 2 | 3 | 19 | 25)
}

/// d(u) = ⟨1 − S(u)⟩, from FSIN's extended value with one rounding.
fn sine_factor(u: f64) -> f64 {
    x87::sin_ext(u).negated().add_to_f64(1.0)
}

/// h(u) of type 2.
fn check(u: f64) -> i64 {
    let i = i64::from(to_i32(CHECK_SCALE * u));
    let m = if i >= 0 { i % CHECK_PERIOD } else { (CHECK_PERIOD - 1) - ((-i) % CHECK_PERIOD) };
    i64::from(m < CHECK_HALF)
}

/// ρ(u), the place in type 3's period.
fn place(u: f64) -> f64 {
    if u > 0.0 {
        u - f64::from(to_i32(u * TRI_K)) * TRI_P
    } else {
        let n = -u;
        TRI_P - (n - f64::from(to_i32(n * TRI_K)) * TRI_P)
    }
}

/// β(u).
fn tri_bit(u: f64) -> i64 {
    i64::from(place(u) < TRI_H)
}

/// τ(u).
pub(super) fn triangle(u: f64) -> f64 {
    let r = place(u);
    let c = r * WAVE_Q;
    if !(r > TRI_H) { c } else { 2.0 - c }
}

/// N(q) for a type that reads no table, with the component's dimension setting; `None` for
/// any other type, or a dimension setting other than 1, 2 or 3.
pub fn table_free(type_number: i64, dimension: Dimension, q: [f64; 3], orders: &TextureOrders) -> Option<f64> {
    let [x, y, z] = q;
    let d = match dimension {
        Dimension::One => 1,
        Dimension::Two => 2,
        Dimension::Three => 3,
        Dimension::Other(_) => return None,
    };
    Some(match type_number {
        0 => TYPE0_VALUE,
        1 => match d {
            1 => sine_factor(x) * 0.5,
            2 => sine_factor(x) * sine_factor(y) * 0.25,
            _ => orders.type1.product([sine_factor(x), sine_factor(y), sine_factor(z)]) * 0.125,
        },
        2 => {
            let sum = match d {
                1 => return Some(check(x) as f64),
                2 => check(x) + check(y),
                _ => check(x) + check(y) + check(z),
            };
            if sum % 2 == 0 { 1.0 } else { 0.0 }
        }
        3 => match d {
            1 => tri_bit(x) as f64,
            _ => {
                let a = 0.5 * x;
                let (u1, u2) = (a - TRI_TURN * y, a + TRI_TURN * y);
                if (tri_bit(x) + tri_bit(u1) + tri_bit(u2)) % 2 == 1 { 1.0 } else { 0.0 }
            }
        },
        19 => match d {
            1 => triangle(x),
            2 => triangle(x) * triangle(y),
            _ => orders.type19.product([triangle(x), triangle(y), triangle(z)]),
        },
        25 => {
            let s = match d {
                1 => x * x,
                2 => x * x + y * y,
                _ => orders.type25.sum([x * x, y * y, z * z]),
            };
            let r = s.sqrt(); // correctly rounded (IEEE 754)
            if r.is_nan() {
                0.0
            } else if r <= 1.0 {
                r
            } else {
                1.0
            }
        }
        // : 1-D x², 2-D
        // 1 − (x² + y²), 3-D 1 − (x² + y² + z²), of the doubles themselves, not capped. Where
        // the point is not calm the format is Open: binary64 is chosen (
        // "binary64 gives every case").
        49 => {
            match d {
                1 => x * x,
                2 => orders.type49.eval(&[x * x, y * y]),
                _ => orders.type49.eval(&[x * x, y * y, z * z]),
            }
        }
        _ => return None,
    })
}
