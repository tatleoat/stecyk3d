// SPDX-License-Identifier: MIT
//! Converting a double to a 32-bit integer.
//!

/// The 64-bit value a conversion gives when the truncation is not representable.
const INTEGER_INDEFINITE: i64 = i64::MIN;

/// 2^63, the first double above the range of a 64-bit signed integer.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;

/// `x` truncated toward zero to a 64-bit two's-complement integer, of which the low 32 bits
/// are returned. A NaN, an infinity or a truncation outside the 64-bit range gives the
/// indefinite value, whose low 32 bits are 0.
///
pub fn to_i32(x: f64) -> i32 {
    let t = x.trunc();
    // The range of i64 is [-2^63, 2^63); -2^63 itself is representable. NaN fails both tests.
    let wide = if t >= -TWO_POW_63 && t < TWO_POW_63 { t as i64 } else { INTEGER_INDEFINITE };
    wide as i32 // low 32 bits.
}

#[cfg(test)]
mod tests {
    use super::to_i32;

    #[test]
    fn entry_examples() {
        assert_eq!(to_i32(-1.5) as u32, 0xffff_ffff);
        assert_eq!(to_i32(2147483648.0) as u32, 0x8000_0000);
        assert_eq!(to_i32(-2147483649.0) as u32, 0x7fff_ffff);
        assert_eq!(to_i32(1e20), 0);
        assert_eq!(to_i32(f64::NAN), 0);
        assert_eq!(to_i32(f64::INFINITY), 0);
    }
}
