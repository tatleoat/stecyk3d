// SPDX-License-Identifier: MIT
//! Numbers with a double's 53-bit significand and an exponent range far wider than a double's:
//! the intermediate numbers of's quotient and point and of's disc coordinates, which
//! are formed so and only rounded to a double at the end.
//!
//! (Wide numbers)

use std::cmp::Ordering;

/// A number m * 2^e with m a finite non-zero double of magnitude in [1, 2) (a normalised
/// significand) and e any integer; or, for a zero, an infinity or a number that is not a number,
/// that double itself (`e` is then 0 and not read).
#[derive(Clone, Copy, Debug)]
pub struct Wide {
    m: f64,
    e: i64,
}

/// 2^k for k in the range of the doubles' normal exponents.
fn pow2(k: i64) -> f64 {
    debug_assert!((-1022..=1023).contains(&k));
    f64::from_bits(((k + 1023) as u64) << 52)
}

impl Wide {
    fn special(x: f64) -> Wide {
        Wide { m: x, e: 0 }
    }

    fn is_special(&self) -> bool {
        self.m == 0.0 || !self.m.is_finite()
    }

    /// A significand of any finite non-zero magnitude scaled by 2^e, normalised.
    fn normalised(m: f64, e: i64) -> Wide {
        if m == 0.0 || !m.is_finite() {
            return Wide::special(m);
        }
        let bits = m.to_bits();
        let mut exp = ((bits >> 52) & 0x7ff) as i64;
        let mut bits = bits;
        if exp == 0 {
            // a subnormal: scale into the normal range first (exact)
            let scaled = m * pow2(54);
            bits = scaled.to_bits();
            exp = ((bits >> 52) & 0x7ff) as i64 - 54;
        }
        let unbiased = exp - 1023;
        let mantissa = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1023u64 << 52));
        Wide { m: mantissa, e: e + unbiased }
    }

    pub fn from_f64(x: f64) -> Wide {
        if x == 0.0 || !x.is_finite() {
            return Wide::special(x);
        }
        Wide::normalised(x, 0)
    }

    /// The representative used when a special value meets another number: a finite non-zero
    /// wide number stands for a number of its sign, so that 0 times it is 0 and infinity
    /// times it is infinity.
    fn rep(&self) -> f64 {
        if self.is_special() { self.m } else { self.m.signum() }
    }

    /// The product, the significand rounded to 53 bits, the exponent unbounded.
    pub fn mul(self, o: Wide) -> Wide {
        if self.is_special() || o.is_special() {
            return Wide::special(self.rep() * o.rep());
        }
        Wide::normalised(self.m * o.m, self.e + o.e)
    }

    /// The quotient, the significand rounded to 53 bits, the exponent unbounded.
    pub fn div(self, o: Wide) -> Wide {
        if self.is_special() || o.is_special() {
            return Wide::special(self.rep() / o.rep());
        }
        Wide::normalised(self.m / o.m, self.e - o.e)
    }

    /// The sum, rounded once to 53 bits, the exponent unbounded.
    pub fn add(self, o: Wide) -> Wide {
        match (self.is_special(), o.is_special()) {
            (true, true) => Wide::special(self.m + o.m),
            // a zero added to a number is that number; an infinity or a NaN swallows it
            (true, false) => {
                if self.m == 0.0 { o } else { Wide::special(self.m + o.rep()) }
            }
            (false, true) => {
                if o.m == 0.0 { self } else { Wide::special(self.rep() + o.m) }
            }
            (false, false) => {
                let (big, small) = if self.e >= o.e { (self, o) } else { (o, self) };
                let d = big.e - small.e;
                if d > 120 {
                    // the smaller is far below half a unit in the last place of the larger
                    return big;
                }
                let s = small.m * pow2(-d.min(1022));
                let sum = big.m + s;
                Wide::normalised(sum, big.e)
            }
        }
    }

    pub fn neg(self) -> Wide {
        Wide { m: -self.m, e: self.e }
    }

    /// Rounded to a double: to infinity beyond the range, to a subnormal or a zero (of the
    /// number's sign) below the normal doubles, each rounded once to nearest even.
    pub fn to_f64(self) -> f64 {
        if self.is_special() {
            return self.m;
        }
        if self.e > 1023 {
            return f64::INFINITY * self.m.signum();
        }
        if self.e >= -1022 {
            return self.m * pow2(self.e);
        }
        if self.e < -1080 {
            return 0.0 * self.m.signum();
        }
        // m * 2^-1022 is exact; the second factor is a power of two >= 2^-58
        (self.m * pow2(-1022)) * pow2(self.e + 1022)
    }

    /// Whether the number is not a number.
    pub fn is_nan(&self) -> bool {
        self.m.is_nan()
    }

    /// How the number compares with a double (`None`: unordered, a NaN on either side).
    pub fn cmp_f64(&self, x: f64) -> Option<Ordering> {
        if self.m.is_nan() || x.is_nan() {
            return None;
        }
        if self.is_special() {
            return self.m.partial_cmp(&x);
        }
        if self.e > 1023 {
            // beyond every finite double, short of an infinity
            let positive = self.m > 0.0;
            return Some(if x.is_infinite() {
                if x > 0.0 { Ordering::Less } else { Ordering::Greater }
            } else if positive {
                Ordering::Greater
            } else {
                Ordering::Less
            });
        }
        self.to_f64().partial_cmp(&x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_arithmetic_is_a_doubles() {
        let a = [0.1, 3.0, -7.25, 1e-300, 1e300, 5e-324, 1.0 / 3.0];
        for &x in &a {
            for &y in &a {
                let (wx, wy) = (Wide::from_f64(x), Wide::from_f64(y));
                let same = |w: Wide, d: f64| {
                    let g = w.to_f64();
                    g.to_bits() == d.to_bits() || (g.is_nan() && d.is_nan())
                };
                assert!(same(wx.mul(wy), x * y), "{x} * {y}");
                assert!(same(wx.div(wy), x / y), "{x} / {y}");
                assert!(same(wx.add(wy), x + y), "{x} + {y}");
            }
        }
    }

    #[test]
    fn exponents_beyond_the_doubles_survive() {
        // 1e305 / 1e-4 overflows a double but not a wide number; times 1e-300 comes back
        let t = Wide::from_f64(1e305).div(Wide::from_f64(1e-4));
        assert_eq!(t.to_f64(), f64::INFINITY);
        let p = Wide::from_f64(1e-300).mul(t);
        assert!((p.to_f64() / 1e9 - 1.0).abs() < 1e-12, "{}", p.to_f64());
    }

    #[test]
    fn zeros_follow_ieee() {
        let z = Wide::from_f64(0.0);
        let nz = Wide::from_f64(-0.0);
        assert_eq!(z.add(nz).to_f64().to_bits(), 0.0f64.to_bits());
        assert_eq!(nz.add(nz).to_f64().to_bits(), (-0.0f64).to_bits());
        let big = Wide::from_f64(1e305).div(Wide::from_f64(1e-4));
        assert_eq!(z.mul(big).to_f64().to_bits(), 0.0f64.to_bits());
        assert_eq!(nz.mul(big).to_f64().to_bits(), (-0.0f64).to_bits());
    }
}
