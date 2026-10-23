// SPDX-License-Identifier: MIT
//! The natural logarithm as an extended value: ln 2, held to 64 bits, times log2(v), rounded
//! once to the extended format's 64-bit significand.
//!
//! (filter 5: "λ is the natural logarithm of v as an extended value: ln 2 times
//! log2(v) in the extended format, ln 2 held to 64 bits rounded to nearest"),
//! (Filter 5's logarithm)
//!
//! The product is formed in double-double (about 2^-100 relative) and then rounded once;
//! ln(m) of the significand comes from 2·atanh(s), s = (m − 1)/(m + 1), by its series.

use super::extended::{Dd, ExtValue, dd_to_ext, decompose};

/// ln 2 rounded to a 64-bit significand: 0xB17217F7D1CF79AC · 2^-64 (ln 2 =
/// 0.B17217F7D1CF79ABC9E3…₁₆, the next digit C rounding up).
const LN2_64_SIGNIFICAND: u64 = 0xB172_17F7_D1CF_79AC;

/// ln 2 as a double-double (0x3FE62E42FEFA39EF + 0x3C7ABC9E3B39803F).
fn ln2() -> Dd {
    Dd { hi: f64::from_bits(0x3FE6_2E42_FEFA_39EF), lo: f64::from_bits(0x3C7A_BC9E_3B39_803F) }
}

/// ln(v) for a finite v > 0, as ln 2 (64 bits) times the exact log2(v), rounded once to the
/// extended format. `None` for 0, a negative, an infinity or a not-a-number (the caller keeps
/// binary64's −∞ or not a number there).
pub fn ln_ext(v: f64) -> Option<ExtValue> {
    if !(v > 0.0) || !v.is_finite() {
        return None;
    }
    // v = m · 2^e exactly, m an integer below 2^53; bring it to x in [0.75, 1.5): v = x · 2^k.
    let (m, e) = decompose(v);
    let bits = 64 - m.leading_zeros() as i32; // m < 2^bits
    let mut k = e + bits; // v = (m / 2^bits) · 2^k, m / 2^bits in [0.5, 1)
    let mut x = Dd::from_scaled_int(m as u128, -bits);
    if x.hi < 0.75 {
        x = x.add(x);
        k -= 1;
    }
    let ln2_64 = Dd::from_scaled_int(LN2_64_SIGNIFICAND as u128, -64);
    // ln(x) = 2 atanh(s), s = (x − 1)/(x + 1), |s| < 0.2
    let one = Dd::new(1.0);
    let s = x.sub(one).div(x.add(one));
    let s2 = s.mul(s);
    let mut term = s;
    let mut sum = s;
    let mut n = 1.0f64;
    loop {
        term = term.mul(s2);
        n += 2.0;
        let add = term.div_f64(n);
        sum = sum.add(add);
        if add.hi == 0.0 || add.hi.abs() <= sum.hi.abs() * 1e-34 {
            break;
        }
    }
    let ln_x = sum.add(sum);
    // λ = ln2_64 · log2(v) = k · ln2_64 + ln(x) · (ln2_64 / ln 2)
    let ratio = ln2_64.div(ln2());
    let lambda = Dd::new(k as f64).mul(ln2_64).add(ln_x.mul(ratio));
    Some(dd_to_ext(lambda, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // : λ near the binary64 logarithm, and exactly ln2_64 · k at 2^k
    #[test]
    fn natural_log_as_extended() {
        assert_eq!(ln_ext(0.0), None);
        assert_eq!(ln_ext(-1.0), None);
        assert_eq!(ln_ext(f64::NAN), None);
        assert_eq!(ln_ext(f64::INFINITY), None);
        let one = ln_ext(1.0).unwrap();
        assert_eq!(one.mantissa, 0);
        // ln_ext(2) is ln 2 to 64 bits itself
        let two = ln_ext(2.0).unwrap();
        assert_eq!((two.mantissa, two.exponent), (LN2_64_SIGNIFICAND, -64));
        let half = ln_ext(0.5).unwrap();
        assert!(half.negative);
        assert_eq!((half.mantissa, half.exponent), (LN2_64_SIGNIFICAND, -64));
        for &v in &[0.1, 0.3, 0.7, 0.999, 1.001, 3.0, 1e-5, 0.5000001, 7.3e-300, 1e300] {
            let l = ln_ext(v).unwrap().to_f64();
            let want = v.ln();
            assert!((l - want).abs() <= 2.0 * f64::EPSILON * want.abs(), "{v}: {l} against {want}");
        }
    }
}
