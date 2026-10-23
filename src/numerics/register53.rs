// SPDX-License-Identifier: MIT
//! Arithmetic in the extended register at 53-bit precision: every operation rounded to the
//! significand of a binary64 (to nearest, ties to even) with the exponent range of the extended
//! format, so that nothing falls to a subnormal or to 0, or rises to an infinity, at the
//! magnitudes a double can reach.
//!
//! (The arithmetic: "Every +, -, x and / and sqrt below is rounded to 53
//! significant bits as's extended register forms it at 53-bit precision ... the
//! exponent range of the extended format"),
//! (Extended values, the register at 53 bits)

use super::extended::{ExtValue, decompose, round_u128};

/// A finite value m * 2^e with a 53-bit significand (bit 52 set unless the value is zero) and
/// an unbounded exponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reg53 {
    negative: bool,
    m: u64,
    e: i32,
}

impl Reg53 {
    pub const ZERO: Reg53 = Reg53 { negative: false, m: 0, e: 0 };

    /// A finite double, exactly (subnormals normalised); `None` for an infinity or a not-a-number.
    pub fn from_f64(d: f64) -> Option<Reg53> {
        if !d.is_finite() {
            return None;
        }
        let (m, e) = decompose(d);
        Some(Reg53::normalised(d.is_sign_negative(), m as u128, false, e))
    }

    /// (-1)^negative * (n + sticky) * 2^e rounded to 53 bits.
    fn normalised(negative: bool, n: u128, sticky: bool, e: i32) -> Reg53 {
        if n == 0 {
            return Reg53 { negative, m: 0, e: 0 };
        }
        let len = 128 - n.leading_zeros();
        if len <= 53 {
            let shift = 53 - len;
            return Reg53 { negative, m: (n << shift) as u64, e: e - shift as i32 };
        }
        let (mut m, de) = round_u128(n, sticky, 53);
        let mut e = e + de;
        if m >> 53 != 0 {
            m >>= 1; // a carry into a new bit: the value is 2^53, exactly representable halved
            e += 1;
        }
        Reg53 { negative, m: m as u64, e }
    }

    pub fn mul(self, other: Reg53) -> Reg53 {
        let negative = self.negative != other.negative;
        if self.m == 0 || other.m == 0 {
            return Reg53 { negative, m: 0, e: 0 };
        }
        Reg53::normalised(negative, self.m as u128 * other.m as u128, false, self.e + other.e)
    }

    pub fn add(self, other: Reg53) -> Reg53 {
        if self.m == 0 && other.m == 0 {
            // two zeros: -0 only when both are -0 (round to nearest)
            return Reg53 { negative: self.negative && other.negative, m: 0, e: 0 };
        }
        if self.m == 0 {
            return other;
        }
        if other.m == 0 {
            return self;
        }
        // The operand of the larger exponent is shifted up; one more than 64 places apart, the
        // other lies below a quarter of a unit in the last place of the result (a power of two
        // has a finer place below it: still far under), so the larger stands.
        let (big, small) = if self.e >= other.e { (self, other) } else { (other, self) };
        let diff = big.e - small.e;
        if diff > 64 {
            return big;
        }
        let a = (big.m as u128) << diff;
        let b = small.m as u128;
        if big.negative == small.negative {
            return Reg53::normalised(big.negative, a + b, false, small.e);
        }
        match a.cmp(&b) {
            std::cmp::Ordering::Equal => Reg53::ZERO,
            std::cmp::Ordering::Greater => Reg53::normalised(big.negative, a - b, false, small.e),
            std::cmp::Ordering::Less => Reg53::normalised(small.negative, b - a, false, small.e),
        }
    }

    pub fn neg(self) -> Reg53 {
        Reg53 { negative: !self.negative, ..self }
    }

    pub fn sub(self, other: Reg53) -> Reg53 {
        self.add(other.neg())
    }

    /// The value as the extended format holds it (a 64-bit significand, the 53 bits first).
    pub fn to_ext(self) -> ExtValue {
        if self.m == 0 {
            return ExtValue { negative: self.negative, ..ExtValue::ZERO };
        }
        ExtValue { negative: self.negative, mantissa: self.m << 11, exponent: self.e - 11 }
    }

    /// Stored as a binary64: rounded once, subnormals and overflow included.
    pub fn to_f64(self) -> f64 {
        self.to_ext().to_f64()
    }

    fn is_zero(self) -> bool {
        self.m == 0
    }
}

/// A value of the register at 53 bits as an operation leaves it: a finite `Reg53`, or an
/// infinity or a not-a-number (held as a double). An operation with an infinity or a
/// not-a-number gives what binary64 gives with the finite operand taken by its sign and
/// whether it is zero (a finite non-zero value beyond binary64's range is still non-zero
/// there). (The register)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegValue {
    Finite(Reg53),
    Special(f64),
}

impl RegValue {
    pub fn from_f64(d: f64) -> RegValue {
        match Reg53::from_f64(d) {
            Some(r) => RegValue::Finite(r),
            None => RegValue::Special(d),
        }
    }

    /// A stand-in double with the same sign and zero-ness (for an operation with a special).
    fn proxy(self) -> f64 {
        match self {
            RegValue::Special(d) => d,
            RegValue::Finite(r) => {
                let one = if r.is_zero() { 0.0 } else { 1.0 };
                if r.negative { -one } else { one }
            }
        }
    }

    fn op(self, other: RegValue, wide: fn(Reg53, Reg53) -> Reg53, ieee: fn(f64, f64) -> f64) -> RegValue {
        match (self, other) {
            (RegValue::Finite(a), RegValue::Finite(b)) => RegValue::Finite(wide(a, b)),
            _ => RegValue::Special(ieee(self.proxy(), other.proxy())),
        }
    }

    /// Stored as a binary64 (rounded once).
    pub fn to_f64(self) -> f64 {
        match self {
            RegValue::Finite(r) => r.to_f64(),
            RegValue::Special(d) => d,
        }
    }

    /// ⟦e⟧: rounded to binary32 (an infinity beyond its range) and taken again as a double.
    /// Rounding first to binary64 changes nothing: binary32's range and places lie well inside
    /// binary64's normal range, and a value below that range rounds to a zero either way.
    pub fn single(self) -> RegValue {
        RegValue::from_f64((self.to_f64() as f32) as f64)
    }

    /// Whether binary64 holds the value as it is (no rounding at a store).
    pub fn fits_binary64(self) -> bool {
        match self {
            RegValue::Special(_) => true,
            RegValue::Finite(r) => RegValue::from_f64(r.to_f64()) == self,
        }
    }
}

impl std::ops::Add for RegValue {
    type Output = RegValue;
    fn add(self, o: RegValue) -> RegValue {
        self.op(o, Reg53::add, |a, b| a + b)
    }
}

impl std::ops::Sub for RegValue {
    type Output = RegValue;
    fn sub(self, o: RegValue) -> RegValue {
        self.op(o, Reg53::sub, |a, b| a - b)
    }
}

impl std::ops::Mul for RegValue {
    type Output = RegValue;
    fn mul(self, o: RegValue) -> RegValue {
        self.op(o, Reg53::mul, |a, b| a * b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(d: f64) -> Reg53 {
        Reg53::from_f64(d).unwrap()
    }

    #[test]
    fn matches_binary64_in_the_normal_range() {
        let values = [1.0, 0.1, 3.0, 1e10, 1.5e-5, 123456.789, 2f64.powi(-300), 7.0e100];
        for &a in &values {
            for &b in &values {
                assert_eq!(r(a).mul(r(b)).to_f64(), a * b, "{a} * {b}");
                assert_eq!(r(a).add(r(b)).to_f64(), a + b, "{a} + {b}");
                assert_eq!(r(a).sub(r(b)).to_f64(), a - b, "{a} - {b}");
            }
        }
    }

    #[test]
    fn a_tiny_square_keeps_its_53_bits() {
        let x = 2f64.powi(-514) * 1.1;
        let wide = r(x).mul(r(x));
        assert_eq!(wide.to_f64(), x * x); // in this range the two roundings agree here
        let tiny = 2f64.powi(-600);
        assert_eq!(r(tiny).mul(r(tiny)).to_f64(), 0.0); // 2^-1200 stores as 0
        assert_eq!(r(tiny).mul(r(tiny)).mul(r(2f64.powi(200))).to_f64(), 2f64.powi(-1000)); // kept wide
    }

    // (The arithmetic: nothing overflows to an infinity in the register; an
    // infinity or a not-a-number behaves as in binary64)
    #[test]
    fn register_values_with_specials_and_zeros() {
        let v = RegValue::from_f64;
        let big = v(1e200) * v(1e200);
        assert!(matches!(big, RegValue::Finite(_)));
        assert_eq!(big.to_f64(), f64::INFINITY);
        assert_eq!((big * v(1e-200)).to_f64(), 1e200 * (1e200 * 1e-200)); // 1e200 back
        assert!(!big.fits_binary64());
        assert_eq!((v(f64::INFINITY) * (v(1e-300) * v(1e-300))).to_f64(), f64::INFINITY); // not 0
        assert!((v(f64::INFINITY) * v(0.0)).to_f64().is_nan());
        assert!((v(f64::INFINITY) - v(f64::INFINITY)).to_f64().is_nan());
        assert!((v(f64::NAN) + v(1.0)).to_f64().is_nan());
        assert!((v(-0.0) + v(0.0)).to_f64().is_sign_positive());
        assert!((v(-0.0) + v(-0.0)).to_f64().is_sign_negative());
        assert!((v(-0.0) * v(3.0)).to_f64().is_sign_negative());
        assert_eq!((v(1.0) - v(1.0)).to_f64().to_bits(), 0);
        assert_eq!(v(1e39).single().to_f64(), f64::INFINITY);
        assert_eq!(v(0.1).single().to_f64(), 0.1f32 as f64);
        assert!(v(0.1).fits_binary64());
    }
}
