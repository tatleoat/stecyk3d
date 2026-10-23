// SPDX-License-Identifier: MIT
//! The x87 unit's sine, cosine and tangent (FSIN, FCOS, FPTAN) as extended values, and
//! the operations that take them to a double.
//!
//! (reduction by the 66-bit approximation of pi; the result at extended
//! precision; the next operation rounds to a double once; |x| >= 2^63 returned unreduced),
//! ("Extended values")
//!
//! An emulation, which's Freedom allows. The argument is reduced exactly (integer
//! long division) by pi66 / 2. The sine and cosine of the remainder are summed as Taylor
//! series in double-double arithmetic (about 2^-100 relative), and the result is rounded
//! once to a 64-bit significand. Only error-free transformations are used, with no FMA.

use super::extended::{Dd, ExtValue, decompose, dd_to_ext, round_to_f64, sin_cos_series};

/// pi rounded to 66 significant bits, as the integer P with pi66 = P * 2^-64.
const PI66_TIMES_2_64: u128 = 0x3_243f_6a88_85a3_08d3;

/// Scale of the fixed-point remainder: r = R * 2^-118. numerics
const SCALE_BITS: i32 = 118;

/// Arguments from 2^63 on are returned unreduced.
const UNREDUCED_FROM: f64 = 9.223_372_036_854_775_808e18;

/// |x| (at least 0.5) reduced by pi66/2: the remainder R (r = R * 2^-118, exact) and the
/// quadrant k mod 4.
fn reduce(ax: f64) -> (u128, u32) {
    let (m, e) = decompose(ax);
    // |x| * 2^118 = m * 2^(e + 118); e + 118 >= 0 for |x| >= 0.5.
    let zeros = e + SCALE_BITS;
    debug_assert!(zeros >= 0);
    let divisor: u128 = PI66_TIMES_2_64 << (SCALE_BITS - 65); // (pi66/2) * 2^118
    // The same long division as taking the bits of m * 2^zeros one at a time, but in wide
    // steps: the first step takes as many bits as fit in 127, then 8 at a time (the remainder
    // is below 2^119, so rem << 8 cannot overflow). Only the quotient mod 4 is kept.
    let mbits = 64 - m.leading_zeros() as i32;
    let first = (127 - mbits).min(zeros);
    let n0 = (m as u128) << first;
    let mut rem = n0 % divisor;
    let mut k = ((n0 / divisor) & 3) as u32;
    let mut left = zeros - first;
    while left > 0 {
        let c = left.min(8);
        let wide = rem << c;
        k = ((k << c) | (wide / divisor) as u32) & 3;
        rem = wide % divisor;
        left -= c;
    }
    (rem, k)
}

#[cfg(test)]
fn reduce_bitwise(ax: f64) -> (u128, u32) {
    let (m, e) = decompose(ax);
    let zeros = e + SCALE_BITS;
    let divisor: u128 = PI66_TIMES_2_64 << (SCALE_BITS - 65);
    let mut rem: u128 = 0;
    let mut k: u32 = 0;
    let mbits = 64 - m.leading_zeros() as i32;
    for i in (0..mbits + zeros).rev() {
        let bit = if i >= zeros { ((m >> (i - zeros)) & 1) as u128 } else { 0 };
        rem = (rem << 1) | bit;
        k <<= 1;
        if rem >= divisor {
            rem -= divisor;
            k |= 1;
        }
        k &= 3;
    }
    (rem, k)
}

#[cfg(test)]
mod reduce_tests {
    use super::*;

    /// The wide-step reduction must equal the bit-serial one for every argument.
    #[test]
    fn wide_reduce_matches_bitwise() {
        let mut s: u64 = 0x1234_5678_9abc_def1;
        for i in 0..2_000_000u32 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let x = match i % 3 {
                0 => f64::from_bits(s >> 12 | 0x3ff0_0000_0000_0000) * (1u64 << (s % 63)) as f64 / 2.0,
                1 => (s % 100_000) as f64 / 128.0,
                _ => f64::from_bits((s & 0x000f_ffff_ffff_ffff) | ((1022 + s % 64) << 52)),
            };
            if !(0.5..9.223_372_036_854_775_808e18).contains(&x) {
                continue;
            }
            assert_eq!(reduce(x), reduce_bitwise(x), "x = {x:e}");
        }
    }
}

/// For a finite |x| below 2^63: sin r and cos r of the remainder of |x| by pi66/2, and the
/// quadrant.
fn reduced(ax: f64) -> (Dd, Dd, u32) {
    if ax < 0.5 {
        let (s, c) = sin_cos_series(Dd::new(ax));
        return (s, c, 0);
    }
    let (rem, quadrant) = reduce(ax);
    // r = rem * 2^-118 in [0, pi66/2). Near the top of the quadrant the remainder is taken
    // from the next multiple of pi66/2, so that pi66 stands for pi throughout: the sine of
    // the binary64 pi is then exactly pi66 - pi_d = 2259 * 2^-64, as states
    // (the complement to the true pi/2 would add (pi - pi66)/2 and miss it).
    let divisor = PI66_TIMES_2_64 << (SCALE_BITS - 65);
    let near_end = rem > divisor / 2;
    let r_dd = Dd::from_scaled_int(rem, -SCALE_BITS);
    let comp_dd = Dd::from_scaled_int(divisor - rem, -SCALE_BITS);
    // Each from the series on the smaller of r and pi66/2 - r.
    let (sin_r, cos_r) = if near_end {
        let (s, c) = sin_cos_series(comp_dd);
        (c, s)
    } else {
        sin_cos_series(r_dd)
    };
    (sin_r, cos_r, quadrant)
}

/// The function a caller asks of the unit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trig {
    Sin,
    Cos,
    Tan,
}

/// Slots of the per-thread memo of `trig_ext`. A pure function of (f, x), so the memo cannot
/// change a result; the long-division reduction costs far more than a lookup, and texture
/// materials ask for the same few angles for every sample of a picture.
const MEMO_SLOTS: usize = 256;

thread_local! {
    static TRIG_MEMO: std::cell::RefCell<Vec<Option<(u8, u64, ExtValue)>>> =
        std::cell::RefCell::new(vec![None; MEMO_SLOTS]);
}

fn trig_ext(f: Trig, x: f64) -> ExtValue {
    let tag = f as u8;
    let bits = x.to_bits();
    let slot = ((bits ^ (bits >> 29) ^ (bits >> 47)).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as usize;
    let slot = (slot ^ tag as usize) % MEMO_SLOTS;
    if let Some(hit) = TRIG_MEMO.with(|m| match m.borrow()[slot] {
        Some((t, b, v)) if t == tag && b == bits => Some(v),
        _ => None,
    }) {
        return hit;
    }
    let v = trig_ext_compute(f, x);
    TRIG_MEMO.with(|m| m.borrow_mut()[slot] = Some((tag, bits, v)));
    v
}

fn trig_ext_compute(f: Trig, x: f64) -> ExtValue {
    if x.is_nan() || x.is_infinite() {
        return dd_to_ext(Dd::new(f64::NAN), 0); // NaN is not represented; callers avoid it
    }
    if x == 0.0 && f != Trig::Cos {
        return ExtValue { negative: x.is_sign_negative(), mantissa: 0, exponent: 0 };
    }
    let ax = x.abs();
    if ax >= UNREDUCED_FROM {
        return dd_to_ext(Dd::new(x), 0);
    }
    let (s, c, k) = reduced(ax);
    let value = match f {
        Trig::Sin => [s, c, s.neg(), c.neg()][k as usize],
        Trig::Cos => [c, s.neg(), c.neg(), s][k as usize],
        Trig::Tan => {
            if k % 2 == 0 { s.div(c) } else { c.div(s).neg() }
        }
    };
    let ext = dd_to_ext(value, 0);
    // sin and tan are odd, cos is even.
    if x < 0.0 && f != Trig::Cos { ext.negated() } else { ext }
}

/// FSIN of a binary64 argument: the extended result.
pub fn sin_ext(x: f64) -> ExtValue {
    trig_ext(Trig::Sin, x)
}

/// FCOS of a binary64 argument: the extended result.
pub fn cos_ext(x: f64) -> ExtValue {
    trig_ext(Trig::Cos, x)
}

/// FPTAN of a binary64 argument: the extended tangent.
pub fn tan_ext(x: f64) -> ExtValue {
    trig_ext(Trig::Tan, x)
}

/// `a / x` for an extended `x`, rounded once to binary64 (a double divided by a register
/// value, stored as a double).
pub fn div_f64_by(a: f64, x: ExtValue) -> f64 {
    if a.is_nan() {
        return a;
    }
    let negative = a.is_sign_negative() != x.negative;
    let sign = if negative { -1.0 } else { 1.0 };
    if x.mantissa == 0 {
        return if a == 0.0 { super::extended::X87_INDEFINITE } else { sign * f64::INFINITY };
    }
    if a.is_infinite() {
        return sign * f64::INFINITY;
    }
    let (am, ae) = decompose(a);
    if am == 0 {
        return sign * 0.0;
    }
    // (am * 2^74) / mantissa has at least 63 significant bits; the remainder is the sticky part.
    let numerator = (am as u128) << 74;
    let q = numerator / x.mantissa as u128;
    let sticky = numerator % x.mantissa as u128 != 0;
    round_to_f64(negative, q, sticky, ae - 74 - x.exponent)
}

/// ln 2 as a double-double (hi, lo): the double nearest it and the double nearest the rest.
/// (F2XM1's emulation; the constant is ln 2, published to many digits)
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fefa_39ef); // ln 2, high part
const LN2_LO: f64 = f64::from_bits(0x3c7a_bc9e_3b39_803f); // ln 2, low part

/// F2XM1 of a binary64 argument in its documented range (|x| <= 1): 2^x - 1 as the extended
/// result. Emulated as e^(x ln 2) - 1 by its Taylor series in double-double (about 2^-100
/// relative), rounded once to a 64-bit significand.;
pub fn f2xm1_ext(x: f64) -> ExtValue {
    if x == 0.0 {
        return ExtValue { negative: x.is_sign_negative(), mantissa: 0, exponent: 0 };
    }
    if x.abs() <= 0.5 {
        if let Some(v) = f2xm1_fast(x) {
            return v;
        }
    }
    f2xm1_slow(x)
}

/// 2^(k/64) for k from -32 to 32 as double-doubles, made once by the Taylor series of the slow
/// path's kind. (F2XM1's emulation: the table-driven fast path)
fn power_table() -> &'static [Dd; 65] {
    static TABLE: std::sync::OnceLock<[Dd; 65]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let k = i as f64 - 32.0;
            if k == 0.0 {
                return Dd::new(1.0);
            }
            let z = Dd::new(k / 64.0).mul(Dd { hi: LN2_HI, lo: LN2_LO });
            let mut term = Dd::new(1.0);
            let mut sum = Dd::new(1.0);
            let mut n = 1.0;
            loop {
                term = term.mul(z).div_f64(n);
                sum = sum.add(term);
                if term.hi.abs() <= sum.hi.abs() * 1e-34 {
                    return sum;
                }
                n += 1.0;
            }
        })
    })
}

/// The relative error bound of the fast path's double-double value (the true bound is about
/// 2^-79); a result whose rounding to the 64-bit significand the bound could change is left to
/// the slow path.
const FAST_ERROR: f64 = 2.117_582_368_135_751e-22; // 2^-72

/// 2^x - 1 for |x| <= 1/2 as 2^(k/64) * 2^r - 1, r = x - k/64 exact and |r| <= 1/128, the
/// second factor by its Taylor series in double-double up to z^3 and a double for the rest.
/// `None` where the value's rounding to a 64-bit significand is not certain at the bound.
/// (the fast path agrees with the slow path wherever it answers: tested)
fn f2xm1_fast(x: f64) -> Option<ExtValue> {
    let k = (x * 64.0).round();
    let r = x - k / 64.0;
    let z = Dd::new(r).mul(Dd { hi: LN2_HI, lo: LN2_LO });
    let z2 = z.mul(z);
    let z3 = z2.mul(z);
    let h = z.hi;
    let rest = h * h * h * h * (1.0 / 24.0 + h * (1.0 / 120.0 + h * (1.0 / 720.0 + h * (1.0 / 5040.0 + h / 40320.0))));
    let s = z.add(z2.div_f64(2.0)).add(z3.div_f64(6.0)).add(Dd::new(rest));
    let t = power_table()[(k as i32 + 32) as usize];
    let v = t.sub(Dd::new(1.0)).add(t.mul(s));
    if v.hi == 0.0 || !v.hi.is_finite() {
        return None;
    }
    let eps = Dd::new(v.hi.abs() * FAST_ERROR);
    let low = dd_to_ext(v.sub(eps), 0);
    let high = dd_to_ext(v.add(eps), 0);
    (low == high).then_some(low)
}

/// 2^(k/1024) for k from -512 to 512 as double-doubles. (the fast sum)
fn fine_table() -> &'static [Dd; 1025] {
    static TABLE: std::sync::OnceLock<Box<[Dd; 1025]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        Box::new(std::array::from_fn(|i| {
            let k = i as f64 - 512.0;
            if k == 0.0 {
                return Dd::new(1.0);
            }
            let z = Dd::new(k / 1024.0).mul(Dd { hi: LN2_HI, lo: LN2_LO });
            let mut term = Dd::new(1.0);
            let mut sum = Dd::new(1.0);
            let mut n = 1.0;
            loop {
                term = term.mul(z).div_f64(n);
                sum = sum.add(term);
                if term.hi.abs() <= sum.hi.abs() * 1e-34 {
                    return sum;
                }
                n += 1.0;
            }
        }))
    })
}

/// The absolute bound within which `fast_plus_one` knows 2^x: the series and the sums in doubles
/// give about 2^-62; the 64-bit rounding of F2XM1's result before the one is added is at most
/// 2^-65. A sum that lies closer than this to a rounding boundary is left to the slow path.
const PLUS_ONE_BOUND: f64 = 8.673_617_379_884_035e-19; // 2^-60

/// round53(round64(2^x - 1) + 1) for |x| <= 1/2 where it is certain from 2^x to about 2^-62 in
/// absolute value, else `None`. 2^x = 2^(k/1024) * 2^r with r = x - k/1024 exact, |r| <= 1/2048.
/// (the fast sum: the answer is the slow path's wherever it answers; tested)
fn fast_plus_one(x: f64) -> Option<f64> {
    let k = (x * 1024.0).round();
    let r = x - k / 1024.0;
    // z = r ln 2 to about 2^-70: the exact product of r and the high part, and the low part's
    let (zp, ze) = super::extended::two_prod(r, LN2_HI);
    let z = zp + (ze + r * LN2_LO);
    let s = z * (1.0 + z * (0.5 + z * (1.0 / 6.0 + z * (1.0 / 24.0 + z * (1.0 / 120.0)))));
    let t = fine_table()[(k as i32 + 512) as usize];
    let a = t.hi * s;
    let u = a + (t.lo + t.lo * s);
    let w_hi = t.hi + u;
    let w_lo = u - (w_hi - t.hi); // quick two-sum: exact since |t.hi| >= |u|
    if !w_hi.is_finite() || w_hi == 0.0 {
        return None;
    }
    // w_hi is the double nearest to t.hi + u; the true 2^x differs from t.hi + u by less than
    // the bound, so w_hi is the nearest double to 2^x unless the remainder is near a half ulp.
    let ulp = f64::from_bits((w_hi.abs().to_bits() >> 52) << 52) * f64::EPSILON;
    let half = ulp * 0.5;
    if (w_lo.abs() - half).abs() <= PLUS_ONE_BOUND {
        return None;
    }
    // A power of two has a finer ulp below it: leave it to the slow path.
    if w_hi.abs().to_bits() & 0x000f_ffff_ffff_ffff == 0 {
        return None;
    }
    Some(w_hi)
}

/// round53(round64(2^x - 1) + 1), the value `f2xm1_ext(x).add_to_f64(1.0)` has: F2XM1's result
/// rounded to the extended format, then one added in a double. (F2XM1; the rows
/// "then add"),
pub fn f2xm1_plus_one(x: f64) -> f64 {
    if x.abs() <= 0.5 && x != 0.0 {
        if let Some(v) = fast_plus_one(x) {
            return v;
        }
    }
    f2xm1_ext(x).add_to_f64(1.0)
}

/// The slow path: e^(x ln 2) - 1 by its Taylor series in double-double, rounded once.
fn f2xm1_slow(x: f64) -> ExtValue {
    let z = Dd::new(x).mul(Dd { hi: LN2_HI, lo: LN2_LO });
    let mut term = z;
    let mut sum = z;
    let mut n = 2.0;
    loop {
        term = term.mul(z).div_f64(n);
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-34 || term.hi == 0.0 {
            return dd_to_ext(sum, 0);
        }
        n += 1.0;
    }
}

/// FSIN rounded once to binary64.
pub fn sin_f64(x: f64) -> f64 {
    sin_ext(x).to_f64()
}

/// FCOS rounded once to binary64.
pub fn cos_f64(x: f64) -> f64 {
    cos_ext(x).to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fast sum gives the slow path's double wherever it answers, and answers almost always.
    #[test]
    fn f2xm1_plus_one_fast_path_agrees_with_the_slow_one() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut answered, total) = (0u32, 400_000u32);
        for i in 0..total {
            let x = match i % 3 {
                0 => (next() >> 11) as f64 / (1u64 << 53) as f64 - 0.5,
                1 => ((next() >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 2f64.powi(-((next() % 60) as i32)),
                _ => (next() % 1025) as f64 / 1024.0 - 0.5 + ((next() % 9) as f64 - 4.0) * 1e-18,
            };
            if x == 0.0 {
                continue;
            }
            if let Some(fast) = fast_plus_one(x) {
                assert_eq!(fast.to_bits(), f2xm1_ext(x).add_to_f64(1.0).to_bits(), "x = {x:e}");
                answered += 1;
            }
        }
        assert!(answered > total * 9 / 10, "the fast sum answers: {answered} of {total}");
    }

    /// The table-driven fast path of F2XM1 gives the slow path's value wherever it answers, and
    /// it answers almost always. (F2XM1's emulation)
    #[test]
    fn f2xm1_fast_path_agrees_with_the_slow_one() {
        let mut state = 0x1234_5678_9abc_def1u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut answered, total) = (0u32, 60_000u32);
        for i in 0..total {
            let x = match i % 4 {
                0 => (next() >> 11) as f64 / (1u64 << 53) as f64 - 0.5,
                1 => ((next() >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 1e-3,
                2 => (next() % 129) as f64 / 128.0 - 0.5 + ((next() % 7) as f64 - 3.0) * 1e-17,
                _ => ((next() >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 2f64.powi(-((next() % 60) as i32)),
            };
            if x == 0.0 {
                continue;
            }
            if let Some(fast) = f2xm1_fast(x) {
                assert_eq!(fast, f2xm1_slow(x), "x = {x:e}");
                answered += 1;
            }
        }
        assert!(answered > total * 9 / 10, "the fast path answers: {answered} of {total}");
    }

    #[test]
    fn small_and_simple_values() {
        assert_eq!(sin_f64(0.0), 0.0);
        assert_eq!(sin_f64(0.5), 0.479_425_538_604_203); // sin(0.5) correctly rounded
        assert_eq!(sin_f64(1.0), 0.841_470_984_807_896_5);
        assert_eq!(sin_f64(-2.0), -0.909_297_426_825_681_7);
        assert_eq!(cos_f64(0.0), 1.0);
        assert_eq!(cos_f64(-0.0), 1.0);
        assert_eq!(cos_f64(1.0), 0.540_302_305_868_139_8);
        assert_eq!(tan_ext(0.5).to_f64(), 0.546_302_489_843_790_5);
        assert_eq!(tan_ext(-0.5).to_f64(), -0.546_302_489_843_790_5);
    }

    #[test]
    fn sine_of_binary64_pi_uses_pi66() {
        // : FSIN(pi_d) is exactly 2259 * 2^-64 (pi66 - pi_d), not the
        // true sin(pi_d) = 1.2246467991473532e-16.
        let e = sin_ext(std::f64::consts::PI);
        assert_eq!(e.to_f64(), 2259.0 * 2f64.powi(-64));
        assert_eq!(sin_f64(-std::f64::consts::PI), -2259.0 * 2f64.powi(-64));
    }

    #[test]
    fn reduction_agrees_with_a_60_digit_reference() {
        // References: sin with reduction by pi66 / 2, computed to 60 digits separately.
        for (x, want) in [
            (0x405e62a3b0ab15f2u64, 0x3fea9769f0c48fc5u64),
            (0xc06a6cb6df53a036, 0x3fe946b5400862ee),
            (0x40712de0cf6cc24b, 0xbfeffe05ce72d1c5),
        ] {
            assert_eq!(sin_f64(f64::from_bits(x)).to_bits(), want, "x {x:016x}");
        }
    }

    #[test]
    fn divided_by_the_result() {
        let one = sin_ext(std::f64::consts::FRAC_PI_2); // nearly 1
        assert_eq!(div_f64_by(3.0, one), 3.0);
        assert_eq!(div_f64_by(-0.0, one).to_bits(), (-0.0f64).to_bits());
    }
}
