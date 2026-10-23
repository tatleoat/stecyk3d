// SPDX-License-Identifier: MIT
//! The processor's arctangent (FPATAN) as an extended value, and the double results the
//! next operation makes of it.
//!
//! (FPATAN; both forms: atan2(y, x), the angle of the point (x, y), and
//! atan(x) = atan2(x, 1); the next operation rounds once),;
//! (the arctangents are the processor's; atan2(1e300, -1))
//! (the arctangent)
//!
//! An emulation, as's Freedom allows. The angle is computed to about 2^-104 relative
//! in double-double arithmetic and rounded once to a 64-bit significand (round to nearest):
//! the extended value is taken to be the correctly rounded arctangent, which the vectors
//! confirm (see the design note for the count).

use super::extended::{Dd, ExtValue, dd_to_ext, decompose, nan_of, scale2, sin_cos_series};

/// pi as a double-double (the double nearest pi, and the double nearest the rest).
/// numerics (a constant of mathematics)
const PI: Dd = Dd { hi: f64::from_bits(0x4009_21FB_5444_2D18), lo: f64::from_bits(0x3CA1_A626_3314_5C07) };
/// pi / 2. numerics
const HALF_PI: Dd = Dd { hi: f64::from_bits(0x3FF9_21FB_5444_2D18), lo: f64::from_bits(0x3C91_A626_3314_5C07) };
/// pi / 4. numerics
const QUARTER_PI: Dd = Dd { hi: f64::from_bits(0x3FE9_21FB_5444_2D18), lo: f64::from_bits(0x3C81_A626_3314_5C07) };

/// Below 2^SMALL_RATIO_EXP the arctangent of a ratio r is r itself to far more bits than
/// the extended format keeps (r^2/3 < 2^-118). numerics
const SMALL_RATIO_EXP: i32 = -60;

/// atan(q) for 0 <= q <= 1, as a double-double: Newton's method on sin z - q cos z = 0
/// from the double guess, two steps (each doubles the correct bits, to the limit of the
/// double-double arithmetic).
fn atan_dd(q: Dd) -> Dd {
    if q.hi == 0.0 {
        return Dd::new(0.0);
    }
    let mut z = Dd::new(q.hi.atan());
    for _ in 0..2 {
        let (s, c) = sin_cos_series(z);
        let f = s.sub(q.mul(c));
        let df = c.add(q.mul(s));
        z = z.sub(f.div_f64(df.hi));
    }
    z
}

/// The extended value FPATAN gives for the point (x, y): the angle in (-pi, pi], with the
/// signs of both coordinates choosing the quadrant. Neither argument may be a NaN.
///
/// . Special values (zeros and infinities) as the Intel manual's table for
/// FPATAN: y = ±0 gives ±0 for x = +0 or x > 0 and ±pi for x = -0 or x < 0; x = ±0 with
/// y finite and nonzero gives ±pi/2; y = ±infinity gives ±pi/4, ±3pi/4 or ±pi/2 for
/// x = +infinity, -infinity or finite; x = ±infinity with y finite gives ±0 or ±pi.
pub fn atan2_ext(y: f64, x: f64) -> ExtValue {
    debug_assert!(!x.is_nan() && !y.is_nan());
    let y_negative = y.is_sign_negative();
    let x_negative = x.is_sign_negative();
    let signed = |v: ExtValue| if y_negative { v.negated() } else { v };
    let zero = ExtValue { negative: false, mantissa: 0, exponent: 0 };

    if y == 0.0 {
        return signed(if x_negative { dd_to_ext(PI, 0) } else { zero });
    }
    if y.is_infinite() {
        let angle = if x == f64::INFINITY {
            QUARTER_PI
        } else if x == f64::NEG_INFINITY {
            PI.sub(QUARTER_PI)
        } else {
            HALF_PI
        };
        return signed(dd_to_ext(angle, 0));
    }
    if x.is_infinite() {
        return signed(if x_negative { dd_to_ext(PI, 0) } else { zero });
    }
    if x == 0.0 {
        return signed(dd_to_ext(HALF_PI, 0));
    }

    // Both finite and nonzero. The ratio of the smaller magnitude to the larger is
    // q * 2^d with q in (1/2, 2) computed from the significands (no overflow or underflow).
    let (ym, ye) = decompose(y);
    let (xm, xe) = decompose(x);
    let y_larger = magnitude_greater(ym, ye, xm, xe);
    let (nm, ne, dm, de) = if y_larger { (xm, xe, ym, ye) } else { (ym, ye, xm, xe) };
    // Normalise both significands to [2^52, 2^53) (subnormals have fewer bits).
    let (nm, ne) = normalise(nm, ne);
    let (dm, de) = normalise(dm, de);
    let q = Dd::new(nm as f64).div_f64(dm as f64);
    let d = ne - de;

    // The angle for the first quadrant, before reflection: atan(r) or pi/2 - atan(r).
    let (first, extra) = if d >= SMALL_RATIO_EXP {
        let r = Dd { hi: scale2(q.hi, d), lo: scale2(q.lo, d) };
        let a = atan_dd(r);
        (if y_larger { HALF_PI.sub(a) } else { a }, 0)
    } else if y_larger || x_negative {
        // r is far below the resolution of the sum it joins (or underflows to zero).
        let r = if d > -1000 { Dd { hi: scale2(q.hi, d), lo: scale2(q.lo, d) } } else { Dd::new(0.0) };
        (if y_larger { HALF_PI.sub(r) } else { r }, 0)
    } else {
        // atan(r) = r to far more than 64 bits; the exponent is carried separately.
        (q, d)
    };
    let angle = if x_negative { (PI.sub(first), 0) } else { (first, extra) };
    signed(dd_to_ext(angle.0, angle.1))
}

/// Whether m1 * 2^e1 > m2 * 2^e2 (positive significands).
fn magnitude_greater(m1: u64, e1: i32, m2: u64, e2: i32) -> bool {
    let (m1, e1) = normalise(m1, e1);
    let (m2, e2) = normalise(m2, e2);
    (e1, m1) > (e2, m2)
}

/// The significand moved up to [2^52, 2^53), the exponent adjusted to keep the value.
fn normalise(m: u64, e: i32) -> (u64, i32) {
    let shift = m.leading_zeros() as i32 - 11;
    (m << shift, e - shift)
}

/// The extended value FPATAN gives for the one-argument arctangent: atan2(x, 1).
/// ("the one-argument arctangent of x is the two-argument one with y = x and
/// x = 1")
pub fn atan_ext(x: f64) -> ExtValue {
    atan2_ext(x, 1.0)
}

/// atan2(y, x) as a double: the extended value rounded once.
pub fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return nan_of(x, y);
    }
    atan2_ext(y, x).to_f64()
}

/// atan(x) as a double: the extended value rounded once.
pub fn atan(x: f64) -> f64 {
    atan2(x, 1.0)
}

/// atan2(y, x) carried in the extended register, divided by d, rounded once to a double
/// (precision control at 53 bits).
pub fn atan2_div(y: f64, x: f64, d: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return nan_of(nan_of(x, y), d);
    }
    atan2_ext(y, x).div_to_f64(d)
}

/// atan2(y, x) carried in the extended register, plus d, rounded once to a double
/// (precision control at 53 bits).
pub fn atan2_add(y: f64, x: f64, d: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return nan_of(nan_of(x, y), d);
    }
    atan2_ext(y, x).add_to_f64(d)
}

/// atan(x) carried in the extended register, divided by d, rounded once.
pub fn atan_div(x: f64, d: f64) -> f64 {
    atan2_div(x, 1.0, d)
}

/// atan(x) carried in the extended register, plus d, rounded once.
pub fn atan_add(x: f64, d: f64) -> f64 {
    atan2_add(x, 1.0, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_values() {
        assert_eq!(atan(0.0).to_bits(), 0);
        assert_eq!(atan(-0.0).to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(atan(1.0).to_bits(), 0x3FE9_21FB_5444_2D18);
        assert_eq!(atan(f64::INFINITY).to_bits(), 0x3FF9_21FB_5444_2D18);
        assert_eq!(atan2(0.0, -0.0).to_bits(), 0x4009_21FB_5444_2D18);
        assert_eq!(atan2(-0.0, -1.0).to_bits(), 0xC009_21FB_5444_2D18);
        assert_eq!(atan2(f64::INFINITY, f64::NEG_INFINITY).to_bits(), 0x4002_D97C_7F33_21D2);
        // atan2(1e300, -1) is the double nearest pi/2
        assert_eq!(atan2(1e300, -1.0).to_bits(), 0x3FF9_21FB_5444_2D18);
        assert!(atan2(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn extended_pi_is_rounded_to_64_bits() {
        // pi = c90fdaa2 2168c234 c4c6... * 2^-62: rounds up in the last of 64 bits.
        let v = atan2_ext(0.0, -1.0);
        assert_eq!((v.mantissa, v.exponent), (0xC90F_DAA2_2168_C235, -62));
    }

    #[test]
    fn tiny_ratio_keeps_its_exponent() {
        let v = atan2_ext(1e-300, 1e300);
        assert!(v.mantissa >> 63 == 1 && v.exponent < -1900);
        assert_eq!(v.to_f64(), 0.0);
    }
}
