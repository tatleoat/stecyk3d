// SPDX-License-Identifier: MIT
//! Arcsine and arccosine as fdlibm computes them.
//!
//!
//! as revised covers asin and acos only; arctangents are's (the x87 unit's).
//!
//! defines the results as those of fdlibm's routines (the published Sun library, the
//! same algorithms as musl). The code below follows fdlibm's published algorithms step for
//! step, so that each IEEE binary64 operation is the same one in the same order; there is no
//! fused multiply-add. Constants are written as bit patterns, from fdlibm's published tables.

/// The high 32 bits of a double (sign, exponent, top 20 bits of the significand).
fn high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// The low 32 bits of a double.
fn low_word(x: f64) -> u32 {
    x.to_bits() as u32
}

/// `x` with its low 32 bits cleared: a value whose square is exact.
fn clear_low_word(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xffff_ffff_0000_0000)
}

// ---------------------------------------------------------------------------------------
// asin and acos
// ---------------------------------------------------------------------------------------

/// pi/2, high part.
const PIO2_HI: f64 = f64::from_bits(0x3FF9_21FB_5444_2D18);
/// pi/2, low part.
const PIO2_LO: f64 = f64::from_bits(0x3C91_A626_3314_5C07);
/// pi/4, high part.
const PIO4_HI: f64 = f64::from_bits(0x3FE9_21FB_5444_2D18);
/// pi rounded to a double.
const PI: f64 = f64::from_bits(0x4009_21FB_5444_2D18);

/// Numerator coefficients of the rational approximation.
const PS: [f64; 6] = [
    f64::from_bits(0x3FC5_5555_5555_5555),
    f64::from_bits(0xBFD4_D612_03EB_6F7D),
    f64::from_bits(0x3FC9_C155_0E88_4455),
    f64::from_bits(0xBFA4_8228_B568_8F3B),
    f64::from_bits(0x3F49_EFE0_7501_B288),
    f64::from_bits(0x3F02_3DE1_0DFD_F709),
];

/// Denominator coefficients (after the leading 1).
const QS: [f64; 4] = [
    f64::from_bits(0xC003_3A27_1C8A_2D4B),
    f64::from_bits(0x4000_2AE5_9C59_8AC8),
    f64::from_bits(0xBFE6_066C_1B8D_0159),
    f64::from_bits(0x3FB3_B8C5_B12E_9282),
];

/// Numerator p(t) of the rational approximation.
fn rational_p(t: f64) -> f64 {
    t * (PS[0] + t * (PS[1] + t * (PS[2] + t * (PS[3] + t * (PS[4] + t * PS[5])))))
}

/// Denominator q(t) of the rational approximation.
fn rational_q(t: f64) -> f64 {
    1.0 + t * (QS[0] + t * (QS[1] + t * (QS[2] + t * QS[3])))
}

/// The arcsine of `x`, in [-pi/2, pi/2]; NaN when |x| > 1 or x is NaN.
///
pub fn asin(x: f64) -> f64 {
    let hx = high_word(x);
    let ix = hx & 0x7fff_ffff;
    let negative = (hx >> 31) != 0;

    if ix >= 0x3ff0_0000 {
        // |x| >= 1, or NaN
        if (ix - 0x3ff0_0000) | low_word(x) == 0 {
            return x * PIO2_HI + x * PIO2_LO; // asin(±1) = ±pi/2
        }
        return (x - x) / (x - x); // NaN
    }
    if ix < 0x3fe0_0000 {
        // |x| < 0.5
        if ix < 0x3e40_0000 {
            // |x| < 2^-27: asin(x) rounds to x.
            return x;
        }
        let t = x * x;
        let w = rational_p(t) / rational_q(t);
        return x + x * w;
    }

    // 0.5 <= |x| < 1
    let w = 1.0 - x.abs();
    let t = w * 0.5;
    let p = rational_p(t);
    let q = rational_q(t);
    let s = t.sqrt();
    let result = if ix >= 0x3fef_3333 {
        // |x| > 0.975
        let w = p / q;
        PIO2_HI - (2.0 * (s + s * w) - PIO2_LO)
    } else {
        let w = clear_low_word(s);
        let c = (t - w * w) / (s + w);
        let r = p / q;
        let p = 2.0 * s * r - (PIO2_LO - 2.0 * c);
        let q = PIO4_HI - 2.0 * w;
        PIO4_HI - (p - q)
    };
    if negative { -result } else { result }
}

/// The arccosine of `x`, in [0, pi]; NaN when |x| > 1 or x is NaN.
///
pub fn acos(x: f64) -> f64 {
    let hx = high_word(x);
    let ix = hx & 0x7fff_ffff;
    let negative = (hx >> 31) != 0;

    if ix >= 0x3ff0_0000 {
        // |x| >= 1, or NaN
        if (ix - 0x3ff0_0000) | low_word(x) == 0 {
            return if negative { PI + 2.0 * PIO2_LO } else { 0.0 };
        }
        return (x - x) / (x - x); // NaN
    }
    if ix < 0x3fe0_0000 {
        // |x| < 0.5
        if ix <= 0x3c60_0000 {
            // |x| <= 2^-57
            return PIO2_HI + PIO2_LO;
        }
        let z = x * x;
        let r = rational_p(z) / rational_q(z);
        return PIO2_HI - (x - (PIO2_LO - x * r));
    }
    if negative {
        // -1 < x <= -0.5
        let z = (1.0 + x) * 0.5;
        let p = rational_p(z);
        let q = rational_q(z);
        let s = z.sqrt();
        let r = p / q;
        let w = r * s - PIO2_LO;
        PI - 2.0 * (s + w)
    } else {
        // 0.5 <= x < 1
        let z = (1.0 - x) * 0.5;
        let s = z.sqrt();
        let df = clear_low_word(s);
        let c = (z - df * df) / (s + df);
        let p = rational_p(z);
        let q = rational_q(z);
        let r = p / q;
        let w = r * s + c;
        2.0 * (df + w)
    }
}

#[cfg(test)]
mod tests {
    use super::{acos, asin};

    const HALF_PI_BITS: u64 = 0x3FF9_21FB_5444_2D18;
    const PI_BITS: u64 = 0x4009_21FB_5444_2D18;

    #[test]
    fn asin_special_values() {
        assert_eq!(asin(0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(asin(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(asin(1.0).to_bits(), HALF_PI_BITS);
        assert_eq!(asin(-1.0).to_bits(), HALF_PI_BITS | 1 << 63);
        assert!(asin(1.0 + f64::EPSILON).is_nan());
        assert!(asin(-2.0).is_nan());
        assert!(asin(f64::INFINITY).is_nan());
        assert!(asin(f64::NEG_INFINITY).is_nan());
        assert!(asin(f64::NAN).is_nan());
        assert_eq!(asin(1e-10), 1e-10);
    }

    #[test]
    fn acos_special_values() {
        assert_eq!(acos(1.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(acos(-1.0).to_bits(), PI_BITS);
        assert_eq!(acos(0.0).to_bits(), HALF_PI_BITS);
        assert_eq!(acos(-0.0).to_bits(), HALF_PI_BITS);
        assert_eq!(acos(1e-300).to_bits(), HALF_PI_BITS);
        assert!(acos(1.0 + f64::EPSILON).is_nan());
        assert!(acos(-1.5).is_nan());
        assert!(acos(f64::INFINITY).is_nan());
        assert!(acos(f64::NAN).is_nan());
    }

    /// Distance in ulps between two finite doubles of the same sign (or zero).
    fn ulp_distance(a: f64, b: f64) -> u64 {
        fn key(v: f64) -> i64 {
            let bits = v.to_bits() as i64;
            if bits < 0 { i64::MIN - bits } else { bits }
        }
        key(a).abs_diff(key(b))
    }

    /// xorshift64 generator for test inputs.
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// SANITY CHECK, NOT A VECTOR TEST: has no released vectors. This compares with
    /// the platform's own libm (through std), which is accurate to within an ulp; it checks
    /// that the code is free of gross mistakes, not that it gives fdlibm's exact bits.
    #[test]
    fn sanity_against_platform_libm() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let n = 1_000_000;
        let mut diff = [0usize; 2]; // asin, acos
        let mut worst = [0u64; 2];
        for i in 0..n {
            let r = rng.next();
            // Mix: uniform in [-1, 1], uniform near ±1, and random bit patterns of all scales.
            let u = (r >> 11) as f64 / (1u64 << 53) as f64; // [0, 1)
            let sign = if r & 1 == 0 { 1.0 } else { -1.0 };
            let x = match i % 4 {
                0 => sign * u,
                1 => sign * (1.0 - u * 1e-3),
                2 => sign * u * 0.5,
                _ => f64::from_bits(r & 0xBFFF_FFFF_FFFF_FFFF), // |x| < 2, all exponents below
            };
            let checks = [(asin(x), x.asin()), (acos(x), x.acos())];
            for (k, (mine, std)) in checks.into_iter().enumerate() {
                if mine.is_nan() || std.is_nan() {
                    assert!(mine.is_nan() && std.is_nan(), "case {k}: NaN mismatch at x={x:e}");
                    continue;
                }
                let d = ulp_distance(mine, std);
                if d != 0 {
                    diff[k] += 1;
                }
                worst[k] = worst[k].max(d);
                assert!(d <= 1, "case {k}: {mine:e} vs {std:e} ({d} ulp) at x={x:e}");
            }
        }
        eprintln!(
            "NUM-9 sanity check over {n} inputs each: differing results (asin, acos) = {:?}; worst ulp = {:?}",
            diff, worst
        );
    }
}
