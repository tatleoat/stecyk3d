// SPDX-License-Identifier: MIT
//! Real roots of the quadratic, cubic and quartic equations the torus needs.
//!
//! (the torus's crossings: Ferrari's method in Schwarze's arrangement, sources.md
//! B12 and B13, the quadratics within the quartic Schwarze's by B13 and the revised B11's
//! note; the resolvent cubic by Cardano's formula with the trigonometric form for three
//! real roots; the arccosine's; the cosine's FCOS carried in the extended
//! register; cube roots as 2^(log2(|x|) / 3) with's functions, with the sign of x, and 0
//! where |x| < 1e-150; values between -1e-10 and 1e-10 treated as zero where the solvers
//! test for zero; square roots's for the version)
//! ("Its roots", items 1 to 5: the zero band, the cube root's product by
//! 1/3,'s arccosine, FCOS's extended value times t rounded once, pi/3 as published, and
//! the three groupings that differ from the published code)
//! (torus)
//!
//! Every operation is one binary64 operation in the order written, except the products with
//! the extended cosine, which are rounded once .

use crate::numerics::x87::cos_ext;
use crate::numerics::{X87_INDEFINITE, acos, exp2_as, log2_as, quieted, sqrt_as};
use crate::version::RenderAs;

/// The band treated as zero where the solvers test for zero.
const ZERO_BAND: f64 = 1e-10;

/// Below this magnitude a cube root is 0.
const CBRT_TINY: f64 = 1e-150;

/// The double nearest 1/3 (the published code's 1.0/3)., Conventions
const THIRD: f64 = 1.0 / 3.0;

/// The double nearest 2/27 (the published code's 2.0/27)., Conventions
const TWO_27THS: f64 = 2.0 / 27.0;

/// pi/3 as the published code forms it: the double nearest pi, divided by 3 and rounded
/// (1.0471975511965976)., item 4
const PI_THIRD: f64 = std::f64::consts::PI / 3.0; // , item 4

fn is_zero(x: f64) -> bool {
    x > -ZERO_BAND && x < ZERO_BAND
}

/// 's arccosine with's NaNs (an argument beyond ±1 gives the indefinite value).
fn acos_x87(c: f64) -> f64 {
    if c.is_nan() {
        quieted(c)
    } else if c.abs() > 1.0 {
        X87_INDEFINITE
    } else {
        acos(c)
    }
}

/// The cube root of x: 2^(log2(|x|) x (1/3)) with's functions, the product by the double
/// nearest 1/3 rounded once (not a division by 3), with the sign of x; 0 where |x| < 1e-150.
/// , (item 2)
fn cube_root(v: RenderAs, x: f64) -> f64 {
    if x.abs() < CBRT_TINY {
        return 0.0;
    }
    let m = exp2_as(v, log2_as(v, x.abs()) * THIRD); // , item 2
    if x < 0.0 { -m } else { m }
}

/// Roots of c2 x² + c1 x + c0, pushed onto `out`: none, one (a zero discriminant within the
/// band) or two. Schwarze's quadratic in normal form, the larger root first: the quadratic
/// within the quartic, which sources.md B13 (and the note of the revised B11) make Schwarze's;
/// B11's own quadratic, in general coefficients, is's (`wall_roots` in geometry.rs). The
/// tests are in the published order: zero, then negative, and every other discriminant, a NaN
/// included, gives two (NaN) values; a NaN is never in a range, so this decides no hit.
fn quadratic(v: RenderAs, c: [f64; 3], out: &mut Vec<f64>) {
    let p = c[1] / (2.0 * c[2]);
    let q = c[0] / c[2];
    let d = p * p - q;
    if is_zero(d) {
        out.push(-p);
    } else if !(d < 0.0) {
        let s = sqrt_as(v, d);
        out.push(s - p);
        out.push(-s - p);
    }
}

/// Real roots of c3 x³ + c2 x² + c1 x + c0 (the first is the largest of three).
pub(crate) fn cubic(v: RenderAs, c: [f64; 4]) -> Vec<f64> {
    let a = c[2] / c[3];
    let b = c[1] / c[3];
    let cc = c[0] / c[3];
    // x = y - a/3: y³ + p y + q = 0
    let sq_a = a * a;
    let p = THIRD * (-THIRD * sq_a + b);
    // , item 5: the constant term grouped by version.
    let q = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (cc - (b * THIRD - sq_a * TWO_27THS) * a) * 0.5,
        RenderAs::Bryce50 => (((sq_a * a) * TWO_27THS - (b * a) * THIRD) + cc) * 0.5,
    };
    let cb_p = p * p * p;
    let d = q * q + cb_p;
    let mut s = Vec::with_capacity(3);
    if is_zero(d) {
        if is_zero(q) {
            s.push(0.0);
        } else {
            let u = cube_root(v, -q);
            s.push(2.0 * u);
            s.push(-u);
        }
    } else if d < 0.0 {
        // Three real roots: the trigonometric form.
        let phi = THIRD * acos_x87(-q / sqrt_as(v, -cb_p));
        let t = 2.0 * sqrt_as(v, -p);
        s.push(cos_ext(phi).mul_to_f64(t));
        s.push(cos_ext(phi + PI_THIRD).mul_to_f64(-t));
        s.push(cos_ext(phi - PI_THIRD).mul_to_f64(-t));
    } else {
        let sqrt_d = sqrt_as(v, d);
        let u = cube_root(v, sqrt_d - q);
        let w = -cube_root(v, sqrt_d + q);
        s.push(u + w);
    }
    let sub = THIRD * a;
    s.iter().map(|x| x - sub).collect()
}

/// Real roots of c4 x⁴ + c3 x³ + c2 x² + c1 x + c0, in no particular order.
pub(super) fn quartic(v: RenderAs, c: [f64; 5]) -> Vec<f64> {
    let a = c[3] / c[4];
    let b = c[2] / c[4];
    let cc = c[1] / c[4];
    let d = c[0] / c[4];
    // x = y - a/4: y⁴ + p y² + q y + r = 0
    let sq_a = a * a;
    let p = -3.0 / 8.0 * sq_a + b;
    // , item 5: the linear term grouped by version (5.0 as published)
    // the constant term grouped alike in every version.
    let q = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => cc - (b * 0.5 - sq_a * 0.125) * a,
        RenderAs::Bryce50 => 1.0 / 8.0 * sq_a * a - 1.0 / 2.0 * a * b + cc,
    };
    let r = d - ((cc * a) * 0.25 - (b * 0.0625 + sq_a * (-3.0 / 256.0)) * sq_a);
    let mut s;
    if is_zero(r) {
        // y (y³ + p y + q) = 0
        s = cubic(v, [q, p, 0.0, 1.0]);
        s.push(0.0);
    } else {
        // One root z of the resolvent cubic gives two quadratics.
        let z = cubic(v, [1.0 / 2.0 * r * p - 1.0 / 8.0 * q * q, -r, -1.0 / 2.0 * p, 1.0])[0];
        let mut u = z * z - r;
        let mut w = 2.0 * z - p;
        if is_zero(u) {
            u = 0.0;
        } else if u > 0.0 {
            u = sqrt_as(v, u);
        } else {
            return Vec::new();
        }
        if is_zero(w) {
            w = 0.0;
        } else if w > 0.0 {
            w = sqrt_as(v, w);
        } else {
            return Vec::new();
        }
        s = Vec::with_capacity(4);
        quadratic(v, [z - u, if q < 0.0 { -w } else { w }, 1.0], &mut s);
        quadratic(v, [z + u, if q < 0.0 { w } else { -w }, 1.0], &mut s);
    }
    let sub = 1.0 / 4.0 * a;
    s.iter().map(|x| x - sub).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quartic_with_known_roots() {
        // (x - 1)(x - 2)(x + 1)(x + 3) = x⁴ + x³ - 7x² - x + 6
        let mut r = quartic(RenderAs::Bryce31, [6.0, -1.0, -7.0, 1.0, 1.0]);
        r.sort_by(|a, b| a.total_cmp(b));
        let want = [-3.0, -1.0, 1.0, 2.0];
        assert_eq!(r.len(), 4);
        for (a, b) in r.iter().zip(want) {
            assert!((a - b).abs() < 1e-9, "{r:?}");
        }
    }

    #[test]
    fn cube_root_rules() {
        assert_eq!(cube_root(RenderAs::Bryce31, 1e-151), 0.0);
        assert!((cube_root(RenderAs::Bryce31, -8.0) + 2.0).abs() < 1e-12);
    }
}
