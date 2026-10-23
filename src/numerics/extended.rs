// SPDX-License-Identifier: MIT
//! The x87 extended format, the roundings of the operations that take an extended value
//! and a double, and the double-double arithmetic used to compute extended values.
//!
//! (extended format; "carried in the extended register": the next operation
//! takes the full 64-bit significand and rounds only its own result to a double, once)
//! (the vectors' rows "then divide" and "then add", made with precision
//! control at 53 bits)
//! ("Extended values"),
//!
//! Every rounding here is done on integers (exact significands), so no result depends on
//! the host's floating-point unit. The double-double arithmetic uses error-free
//! transformations only (Dekker 1971; Knuth's two-sum), with no fused multiply-add.

/// A value in the x87 extended format: (-1)^negative * mantissa * 2^exponent, with the
/// mantissa's top bit set unless the value is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtValue {
    pub negative: bool,
    pub mantissa: u64,
    pub exponent: i32,
}

impl ExtValue {
    pub(crate) const ZERO: ExtValue = ExtValue { negative: false, mantissa: 0, exponent: 0 };

    /// The value with its sign changed.
    pub fn negated(self) -> ExtValue {
        ExtValue { negative: !self.negative, ..self }
    }

    /// The value as a double: the 64-bit significand rounded once to binary64 (round to
    /// nearest, ties to even), as a store of the register to memory does.
    pub fn to_f64(self) -> f64 {
        round_to_f64(self.negative, self.mantissa as u128, false, self.exponent)
    }

    /// 1 / self, the exact quotient rounded once to binary64 (round to nearest, ties to
    /// even).,
    pub fn recip_to_f64(self) -> f64 {
        if self.mantissa == 0 {
            return if self.negative { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        let m = self.mantissa as u128; // top bit set: 2^63 <= m < 2^64
        let numerator: u128 = 1u128 << 127;
        let q = numerator / m;
        let sticky = numerator % m != 0;
        rounded_at_53(self.negative, q, sticky, -127 - self.exponent)
    }

    /// self * d, the exact product rounded to a 53-bit significand once (then stored as a
    /// double).
    pub fn mul_to_f64(self, d: f64) -> f64 {
        if d.is_nan() {
            return quiet(d);
        }
        let negative = self.negative != d.is_sign_negative();
        if d.is_infinite() {
            return if self.mantissa == 0 {
                X87_INDEFINITE
            } else if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        let (dm, de) = decompose(d);
        if self.mantissa == 0 || dm == 0 {
            return if negative { -0.0 } else { 0.0 };
        }
        let product = (self.mantissa as u128) * (dm as u128);
        rounded_at_53(negative, product, false, self.exponent + de)
    }

    /// The exact order of self and a finite or infinite double; `None` against a not-a-number.
    pub fn cmp_f64(self, d: f64) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::*;
        if d.is_nan() {
            return None;
        }
        let d_neg = d.is_sign_negative() && d != 0.0;
        let self_zero = self.mantissa == 0;
        let s_neg = self.negative && !self_zero;
        if d.is_infinite() {
            return Some(if d_neg { Greater } else { Less });
        }
        let (dm, de) = decompose(d);
        if self_zero && dm == 0 {
            return Some(Equal);
        }
        if s_neg != d_neg {
            return Some(if s_neg { Less } else { Greater });
        }
        // Magnitudes: |self| against |d|.
        let mag = if self_zero {
            Less
        } else if dm == 0 {
            Greater
        } else {
            let top = |m: u64, e: i32| e + (64 - m.leading_zeros() as i32);
            let (t1, t2) = (top(self.mantissa, self.exponent), top(dm, de));
            if t1 != t2 {
                t1.cmp(&t2)
            } else {
                let low = self.exponent.min(de);
                let a = (self.mantissa as u128) << (self.exponent - low);
                let b = (dm as u128) << (de - low);
                a.cmp(&b)
            }
        };
        Some(if s_neg { mag.reverse() } else { mag })
    }

    /// The magnitude.
    pub fn magnitude(self) -> ExtValue {
        ExtValue { negative: false, ..self }
    }

    /// self * other, both extended, the exact product rounded to a 53-bit significand once
    /// (then stored as a double).
    pub fn mul_ext_to_f64(self, other: ExtValue) -> f64 {
        let negative = self.negative != other.negative;
        if self.mantissa == 0 || other.mantissa == 0 {
            return if negative { -0.0 } else { 0.0 };
        }
        let product = (self.mantissa as u128) * (other.mantissa as u128);
        rounded_at_53(negative, product, false, self.exponent + other.exponent)
    }

    /// self / d, the exact quotient rounded to a 53-bit significand once (precision
    /// control at 53 bits), then stored as a double.
    pub fn div_to_f64(self, d: f64) -> f64 {
        if d.is_nan() {
            return quiet(d);
        }
        let negative = self.negative != d.is_sign_negative();
        let signed_zero = if negative { -0.0 } else { 0.0 };
        let signed_inf = if negative { f64::NEG_INFINITY } else { f64::INFINITY };
        if d.is_infinite() {
            return signed_zero;
        }
        if d == 0.0 {
            return if self.mantissa == 0 { X87_INDEFINITE } else { signed_inf };
        }
        if self.mantissa == 0 {
            return signed_zero;
        }
        let (dm, de) = decompose(d);
        let numerator = (self.mantissa as u128) << 64;
        let q = numerator / dm as u128;
        let sticky = numerator % dm as u128 != 0;
        rounded_at_53(negative, q, sticky, self.exponent - 64 - de)
    }

    /// self + d, the exact sum rounded to a 53-bit significand once (precision control at
    /// 53 bits), then stored as a double.
    pub fn add_to_f64(self, d: f64) -> f64 {
        if d.is_nan() || d.is_infinite() {
            return quiet(d);
        }
        if self.mantissa == 0 {
            if d == 0.0 {
                // Round to nearest: the sum of two zeros is -0 only when both are -0.
                return if self.negative && d.is_sign_negative() { -0.0 } else { 0.0 };
            }
            return d;
        }
        let (dm, de) = decompose(d);
        if dm == 0 {
            return rounded_at_53(self.negative, self.mantissa as u128, false, self.exponent);
        }
        // Place both at the scale 2^(top - 126), where top is above both leading bits, so
        // each is below 2^126 and their sum below 2^127. The smaller one, if shifted right,
        // keeps a jammed sticky bit far below the rounding position (correct for both
        // addition and subtraction with this many guard bits).
        let a_top = self.exponent + 64;
        let b_top = de + 53;
        let base = a_top.max(b_top) - 126;
        let a = align(self.mantissa as u128, self.exponent - base);
        let b = align(dm as u128, de - base);
        let d_negative = d.is_sign_negative();
        let (negative, n) = if self.negative == d_negative {
            (self.negative, a + b)
        } else if a > b {
            (self.negative, a - b)
        } else if b > a {
            (d_negative, b - a)
        } else {
            return 0.0; // exact cancellation: +0 under round to nearest
        };
        rounded_at_53(negative, n, false, base)
    }
}

/// n * 2^shift as an integer, with the bits shifted out (if any) jammed into bit 0.
fn align(n: u128, shift: i32) -> u128 {
    if shift >= 0 {
        n << shift
    } else if shift > -128 {
        let s = (-shift) as u32;
        let lost = n & ((1u128 << s) - 1) != 0;
        (n >> s) | u128::from(lost)
    } else {
        u128::from(n != 0)
    }
}

/// The x87 indefinite value , the result of an invalid operation.
pub(crate) const X87_INDEFINITE: f64 = f64::from_bits(0xfff8_0000_0000_0000);

/// A NaN made quiet (its top fraction bit set, the rest kept).
pub(crate) fn quiet(x: f64) -> f64 {
    f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000)
}

/// The NaN an operation on two operands hands on, when at least one is a NaN.
/// (two NaNs: the larger significand; equal significands: the positive one;
/// one NaN: that NaN; made quiet)
pub(crate) fn nan_of(a: f64, b: f64) -> f64 {
    match (a.is_nan(), b.is_nan()) {
        (true, false) => quiet(a),
        (false, true) => quiet(b),
        _ => {
            let fa = quiet(a).to_bits() & 0x000f_ffff_ffff_ffff;
            let fb = quiet(b).to_bits() & 0x000f_ffff_ffff_ffff;
            if fa > fb || (fa == fb && !a.is_sign_negative()) { quiet(a) } else { quiet(b) }
        }
    }
}

/// The integer significand and exponent of a finite double: |d| = m * 2^e.
pub(crate) fn decompose(d: f64) -> (u64, i32) {
    let bits = d.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp_field == 0 { (frac, -1074) } else { (frac | (1u64 << 52), exp_field - 1075) }
}

/// x * 2^e, by exact steps (exact while the result is a normal double; a result in the
/// subnormal range is rounded once, by the last step).
pub(crate) fn scale2(x: f64, e: i32) -> f64 {
    let mut v = x;
    let mut e = e;
    while e > 1000 {
        v *= f64::from_bits(((1023 + 1000) as u64) << 52);
        e -= 1000;
    }
    while e < -1000 {
        v *= f64::from_bits(((1023 - 1000) as u64) << 52);
        e += 1000;
    }
    v * f64::from_bits(((1023 + e) as u64) << 52)
}

/// Rounds the integer n (with `sticky` meaning "something nonzero lies below n's last
/// bit") to `bits` significant bits, to nearest, ties to even. Returns (m, e) with the
/// rounded value m * 2^e.
pub(crate) fn round_u128(n: u128, sticky: bool, bits: u32) -> (u128, i32) {
    let len = 128 - n.leading_zeros();
    if len <= bits {
        return (n, 0); // exact, so any sticky part is below a half: round down
    }
    let drop = len - bits;
    let kept = n >> drop;
    let rest = n & ((1u128 << drop) - 1);
    let half = 1u128 << (drop - 1);
    let round_up = rest > half || (rest == half && (sticky || kept & 1 == 1));
    let m = if round_up { kept + 1 } else { kept };
    // A carry into a new bit (m = 2^bits) is still exact as a value.
    (m, drop as i32)
}

/// (-1)^negative * (n + sticky part) * 2^e rounded to a 53-bit significand with an
/// unbounded exponent (the x87 unit under precision control 53), then stored as a double
/// (a second rounding only if the result lies in the subnormal range).
/// f·2^i for a finite double f, rounded once to binary64 (exact unless it falls below the
/// normal doubles or overflows).
pub(crate) fn scaled_to_f64(f: f64, i: i32) -> f64 {
    let (m, e) = decompose(f);
    round_to_f64(f.is_sign_negative(), m as u128, false, e + i)
}

fn rounded_at_53(negative: bool, n: u128, sticky: bool, e: i32) -> f64 {
    let (m, de) = round_u128(n, sticky, 53);
    round_to_f64(negative, m, false, e + de)
}

/// (-1)^negative * (n + sticky part) * 2^e rounded once to binary64 (round to nearest,
/// ties to even), subnormal results included; overflow gives infinity.
pub(crate) fn round_to_f64(negative: bool, n: u128, sticky: bool, e: i32) -> f64 {
    let sign = if negative { -1.0 } else { 1.0 };
    if n == 0 {
        return sign * 0.0;
    }
    let len = (128 - n.leading_zeros()) as i32;
    let lead = e + len - 1; // exponent of the leading bit
    // Significant bits available at this magnitude: 53 for normal results, fewer below.
    let precision = if lead >= -1022 { 53 } else { 53 - (-1022 - lead) };
    let drop = len - precision;
    let (m, scale) = if drop <= 0 {
        (n, e) // exact
    } else if drop > len {
        return sign * 0.0; // below half the smallest subnormal
    } else {
        let d = drop as u32;
        let kept = if d >= 128 { 0 } else { n >> d };
        let rest = if d >= 128 { n } else { n & ((1u128 << d) - 1) };
        let half = 1u128 << (d - 1);
        let up = rest > half || (rest == half && (sticky || kept & 1 == 1));
        (if up { kept + 1 } else { kept }, e + drop)
    };
    if m == 0 {
        return sign * 0.0;
    }
    if scale + (128 - m.leading_zeros() as i32) - 1 > 1023 {
        return sign * f64::INFINITY;
    }
    // m has at most 54 bits and m * 2^scale is representable: exact conversion and scaling.
    sign * scale2(m as f64, scale)
}

// ---------------------------------------------------------------------------------------
// double-double arithmetic (Dekker 1971; Knuth's two-sum), no fused operations
// ---------------------------------------------------------------------------------------

/// hi + lo, with |lo| at most half an ulp of hi.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dd {
    pub hi: f64,
    pub lo: f64,
}

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    let err = (a - (s - bb)) + (b - bb);
    (s, err)
}

fn quick_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let err = b - (s - a);
    (s, err)
}

fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1 (Dekker's split)
    let hi = c - (c - a);
    (hi, a - hi)
}

pub(crate) fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let err = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, err)
}

impl Dd {
    pub(crate) fn new(x: f64) -> Dd {
        Dd { hi: x, lo: 0.0 }
    }

    pub(crate) fn add(self, y: Dd) -> Dd {
        let (s, e) = two_sum(self.hi, y.hi);
        let (t, f) = two_sum(self.lo, y.lo);
        let e = e + t;
        let (s, e) = quick_two_sum(s, e);
        let e = e + f;
        let (hi, lo) = quick_two_sum(s, e);
        Dd { hi, lo }
    }

    pub(crate) fn neg(self) -> Dd {
        Dd { hi: -self.hi, lo: -self.lo }
    }

    pub(crate) fn sub(self, y: Dd) -> Dd {
        self.add(y.neg())
    }

    pub(crate) fn mul(self, y: Dd) -> Dd {
        let (p, e) = two_prod(self.hi, y.hi);
        let e = e + (self.hi * y.lo + self.lo * y.hi);
        let (hi, lo) = quick_two_sum(p, e);
        Dd { hi, lo }
    }

    /// self / d for a double d (one correction step: about 2^-104 relative).
    pub(crate) fn div_f64(self, d: f64) -> Dd {
        let q1 = self.hi / d;
        let (p, e) = two_prod(q1, d);
        let r = ((self.hi - p) - e) + self.lo;
        let q2 = r / d;
        let (hi, lo) = quick_two_sum(q1, q2);
        Dd { hi, lo }
    }

    /// self / y (two quotient steps: about 2^-104 relative).
    pub(crate) fn div(self, y: Dd) -> Dd {
        let q1 = self.hi / y.hi;
        let r = self.sub(y.mul(Dd::new(q1)));
        let q2 = r.hi / y.hi;
        let r = r.sub(y.mul(Dd::new(q2)));
        let q3 = r.hi / y.hi;
        let (hi, lo) = quick_two_sum(q1, q2);
        Dd { hi, lo }.add(Dd::new(q3))
    }

    /// The double-double nearest the integer n times 2^e (n < 2^127).
    pub(crate) fn from_scaled_int(n: u128, e: i32) -> Dd {
        let hi = n as f64; // rounds to nearest
        let rest = (n as i128) - (hi as u128 as i128);
        let lo = rest as f64;
        Dd { hi: scale2(hi, e), lo: scale2(lo, e) }
    }
}

/// sin t by its Taylor series, |t| <= pi/4 (about 2^-104 relative).
/// Kept as the reference `sin_cos_series` is tested against.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sin_series(t: Dd) -> Dd {
    let t2 = t.mul(t);
    let mut term = t;
    let mut sum = t;
    let mut n = 1.0;
    loop {
        term = term.mul(t2).div_f64((2.0 * n) * (2.0 * n + 1.0)).neg();
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-34 || term.hi == 0.0 {
            return sum;
        }
        n += 1.0;
    }
}

/// cos t by its Taylor series, |t| <= pi/4.
/// Kept as the reference `sin_cos_series` is tested against.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn cos_series(t: Dd) -> Dd {
    let t2 = t.mul(t);
    let mut term = Dd::new(1.0);
    let mut sum = Dd::new(1.0);
    let mut n = 1.0;
    loop {
        term = term.mul(t2).div_f64((2.0 * n - 1.0) * (2.0 * n)).neg();
        sum = sum.add(term);
        if term.hi.abs() <= sum.hi.abs() * 1e-34 || term.hi == 0.0 {
            return sum;
        }
        n += 1.0;
    }
}

/// (sin t, cos t) by their Taylor series in one loop, |t| <= pi/4: bit for bit
/// `(sin_series(t), cos_series(t))`. Each series makes the same operations on the same values
/// in the same order and stops on its own test; the loop only interleaves the two chains (and
/// makes t^2 once), so neither waits on the other's divisions. numerics (speed; the
/// old recomp's sin_cos_q makes both series in one loop)
pub(crate) fn sin_cos_series(t: Dd) -> (Dd, Dd) {
    let t2 = t.mul(t);
    let (mut s_term, mut s_sum) = (t, t);
    let (mut c_term, mut c_sum) = (Dd::new(1.0), Dd::new(1.0));
    let (mut s_done, mut c_done) = (false, false);
    let mut n = 1.0;
    while !(s_done && c_done) {
        if !s_done {
            s_term = s_term.mul(t2).div_f64((2.0 * n) * (2.0 * n + 1.0)).neg();
            s_sum = s_sum.add(s_term);
            s_done = s_term.hi.abs() <= s_sum.hi.abs() * 1e-34 || s_term.hi == 0.0;
        }
        if !c_done {
            c_term = c_term.mul(t2).div_f64((2.0 * n - 1.0) * (2.0 * n)).neg();
            c_sum = c_sum.add(c_term);
            c_done = c_term.hi.abs() <= c_sum.hi.abs() * 1e-34 || c_term.hi == 0.0;
        }
        n += 1.0;
    }
    (s_sum, c_sum)
}

/// Rounds a double-double times 2^extra to the extended format (64-bit significand), once.
pub(crate) fn dd_to_ext(v: Dd, extra: i32) -> ExtValue {
    if v.hi == 0.0 {
        return ExtValue { negative: v.hi.is_sign_negative(), ..ExtValue::ZERO };
    }
    let negative = v.hi < 0.0;
    let (hm, he) = decompose(v.hi);
    // Work at the scale 2^(he - 70): hi's significand moved up by 70 bits.
    let base = he - 70;
    let mut n: i128 = (hm as i128) << 70;
    let mut sticky = false;
    if v.lo != 0.0 {
        let (lm, le) = decompose(v.lo);
        let shift = le - base;
        let lo_mag: i128 = if shift >= 0 {
            (lm as i128) << shift
        } else if shift > -64 {
            let s = (-shift) as u32;
            if lm & ((1u64 << s) - 1) != 0 {
                sticky = true;
            }
            (lm >> s) as i128
        } else {
            sticky = true;
            0
        };
        // lo's sign relative to hi's
        if (v.lo < 0.0) == negative {
            n += lo_mag;
        } else {
            n -= lo_mag;
            if sticky {
                // The true value lies just below n: step down one unit and keep the
                // remainder as the sticky part.
                n -= 1;
            }
        }
    }
    let (m, e) = round_u128(n as u128, sticky, 64);
    let (m, e) = if m >> 64 != 0 { (m >> 1, e + 1) } else { (m, e) };
    ExtValue { negative, mantissa: m as u64, exponent: base + e + extra }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_comparison_with_doubles() {
        use std::cmp::Ordering::*;
        let one = ExtValue { negative: false, mantissa: 1 << 63, exponent: -63 };
        // One more unit in the 64th bit: above 1, though it rounds to 1.
        let above = ExtValue { mantissa: (1 << 63) | 1, ..one };
        assert_eq!(above.to_f64(), 1.0);
        assert_eq!(one.cmp_f64(1.0), Some(Equal));
        assert_eq!(above.cmp_f64(1.0), Some(Greater));
        assert_eq!(above.negated().cmp_f64(-1.0), Some(Less));
        assert_eq!(one.cmp_f64(1.5), Some(Less));
        assert_eq!(one.cmp_f64(-2.0), Some(Greater));
        assert_eq!(ExtValue::ZERO.cmp_f64(-0.0), Some(Equal));
        assert_eq!(ExtValue::ZERO.cmp_f64(1e-300), Some(Less));
        assert_eq!(one.cmp_f64(f64::INFINITY), Some(Less));
        assert_eq!(one.cmp_f64(f64::NAN), None);
        assert_eq!(one.negated().magnitude(), one);
    }

    fn ext(negative: bool, mantissa: u64, exponent: i32) -> ExtValue {
        ExtValue { negative, mantissa, exponent }
    }

    #[test]
    fn to_f64_rounds_once() {
        // 1 + 2^-53 + 2^-63: above the halfway point, rounds up.
        let v = ext(false, (1 << 63) | (1 << 10) | 1, -63);
        assert_eq!(v.to_f64(), 1.0 + f64::EPSILON);
        // exactly halfway: ties to even (down)
        let v = ext(false, (1 << 63) | (1 << 10), -63);
        assert_eq!(v.to_f64(), 1.0);
        // subnormal result: one rounding to 2^-1074 units
        let v = ext(true, (1 << 63) | (1 << 62), -63 - 1074); // -1.5 * 2^-1074
        assert_eq!(v.to_f64().to_bits(), 0x8000_0000_0000_0002);
    }

    #[test]
    fn add_and_divide_round_once() {
        let one = ext(false, 1 << 63, -63);
        assert_eq!(one.add_to_f64(1.0), 2.0);
        assert_eq!(one.add_to_f64(-1.0).to_bits(), 0);
        assert_eq!(one.div_to_f64(3.0), 1.0 / 3.0);
        assert_eq!(one.div_to_f64(-0.0), f64::NEG_INFINITY);
        // 1 + 2^-60 plus -2^-53... : exact sum 1 - 2^-53 + 2^-60, rounds to 1 - 2^-53.
        let v = ext(false, (1 << 63) | (1 << 3), -63);
        assert_eq!(v.add_to_f64(-f64::EPSILON / 2.0), 1.0 - f64::EPSILON / 2.0);
        // a tiny addend jammed into the sticky bit
        assert_eq!(one.add_to_f64(1e-300), 1.0);
        assert_eq!(one.add_to_f64(-1e-300), 1.0);
    }

    #[test]
    fn nan_rules() {
        let a = f64::from_bits(0x7ff0_0000_0000_0001); // signalling
        assert_eq!(nan_of(a, 1.0).to_bits(), 0x7ff8_0000_0000_0001);
        let b = f64::from_bits(0xfff8_0000_0000_0002);
        assert_eq!(nan_of(a, b).to_bits(), 0xfff8_0000_0000_0002);
        let c = f64::from_bits(0xfff8_0000_0000_0001);
        assert_eq!(nan_of(c, a).to_bits(), 0x7ff8_0000_0000_0001);
    }

    /// The one-loop series is the two series bit for bit: double arguments over [-pi/4, pi/4]
    /// at every magnitude down to 2^-70, and double-double ones (a low part) as the
    /// arctangent's Newton steps and the x87 reduction hand them.
    #[test]
    fn sin_cos_series_is_the_two_series() {
        let same = |a: Dd, b: Dd| a.hi.to_bits() == b.hi.to_bits() && a.lo.to_bits() == b.lo.to_bits();
        let mut s = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let quarter_pi = std::f64::consts::FRAC_PI_4;
        let mut fixed = vec![0.0, -0.0, quarter_pi, -quarter_pi, 0.5, 1e-300, f64::MIN_POSITIVE];
        fixed.extend((0..=70).map(|k| scale2(1.0, -k)));
        for i in 0..2_000_000u64 {
            let r = next();
            let hi = if i < fixed.len() as u64 {
                fixed[i as usize]
            } else {
                // significand drawn, exponent from 2^-70 to 2^-1, either sign
                let e = -70 + (r >> 58) as i32 % 70;
                let x = scale2(f64::from_bits(0x3ff0_0000_0000_0000 | (r & ((1 << 52) - 1))), e);
                let x = if x > quarter_pi { x - 0.25 } else { x };
                if r >> 63 != 0 { -x } else { x }
            };
            let lo = if i % 2 == 0 { 0.0 } else { hi * scale2((next() >> 11) as f64, -53 - 53) };
            let t = Dd { hi, lo };
            let (sf, cf) = sin_cos_series(t);
            assert!(same(sf, sin_series(t)) && same(cf, cos_series(t)), "t = {hi:e} + {lo:e}");
        }
    }
}
