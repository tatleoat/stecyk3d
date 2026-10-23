//! x^y as the published pow computes it: a translation into Rust of the double-precision
//! power function of the Arm Optimized Routines (Szabolcs Nagy, Arm Limited, 2018), as musl
//! v1.2.6 carries it. Its branches without a fused multiply-add are the ones translated, every
//! operation a binary64 operation rounded on its own, so the result does not depend on the
//! compiler or the machine.
//!
//! Copyright (c) 2018, Arm Limited. SPDX-License-Identifier: MIT
//! (the notice: LICENSES/musl-COPYRIGHT.md)

use super::pow_data as data;

fn d(b: u64) -> f64 {
    f64::from_bits(b)
}

/// The top 12 bits of a double (sign and exponent).
fn top12(x: f64) -> u32 {
    (x.to_bits() >> 52) as u32
}

const OFF: u64 = 0x3fe6_9555_0000_0000;
const LOG_TABLE_BITS: u32 = 7;
const EXP_TABLE_BITS: u32 = 7;
const SIGN_BIAS: u64 = 0x800 << EXP_TABLE_BITS;

/// log(x) as hi + tail, for the bits `ix` of x (a subnormal x normalised with the exponent
/// below 0).
fn log_inline(ix: u64) -> (f64, f64) {
    let n = 1u64 << LOG_TABLE_BITS;
    let a = data::LOG_POLY.map(d);
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - LOG_TABLE_BITS)) % n) as usize;
    let k = (tmp as i64) >> 52;
    let iz = ix.wrapping_sub(tmp & (0xfff_u64 << 52));
    let z = d(iz);
    let kd = k as f64;
    let [invc, logc, logctail] = data::LOG_TAB[i].map(d);
    // z split so that rhi, rlo and rhi·rhi are exact.
    let zhi = d(iz.wrapping_add(1 << 31) & (u64::MAX << 32));
    let zlo = z - zhi;
    let rhi = zhi * invc - 1.0;
    let rlo = zlo * invc;
    let r = rhi + rlo;
    let t1 = kd * d(data::LN2HI) + logc;
    let t2 = t1 + r;
    let lo1 = kd * d(data::LN2LO) + logctail;
    let lo2 = t1 - t2 + r;
    let ar = a[0] * r;
    let ar2 = r * ar;
    let ar3 = r * ar2;
    let arhi = a[0] * rhi;
    let arhi2 = rhi * arhi;
    let hi = t2 + arhi2;
    let lo3 = rlo * (ar + arhi);
    let lo4 = t2 - hi + arhi2;
    let p = ar3 * (a[1] + r * a[2] + ar2 * (a[3] + r * a[4] + ar2 * (a[5] + r * a[6])));
    let lo = lo1 + lo2 + lo3 + lo4 + p;
    let y = hi + lo;
    (y, hi - y + lo)
}

/// The result past the largest double (an infinity of the sign's).
fn overflow(sign: u64) -> f64 {
    xflow(sign, f64::from_bits(0x7000_0000_0000_0000))
}

/// The result below the least double (a zero of the sign's).
fn underflow(sign: u64) -> f64 {
    xflow(sign, f64::from_bits(0x1000_0000_0000_0000))
}

fn xflow(sign: u64, y: f64) -> f64 {
    (if sign != 0 { -y } else { y }) * y
}

/// The cases where scale · (1 + tmp) may leave the range of normal doubles.
fn special_case(tmp: f64, sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        // k > 0: the exponent of scale may have overflowed by up to 460.
        let scale = d(sbits.wrapping_sub(1009 << 52));
        return f64::from_bits(0x7F00_0000_0000_0000) * (scale + scale * tmp); // 0x1p1009
    }
    // k < 0: care in the subnormal range; sbits is the signed scale.
    let sbits = sbits.wrapping_add(1022 << 52);
    let scale = d(sbits);
    let mut y = scale + scale * tmp;
    if y.abs() < 1.0 {
        let one = if y < 0.0 { -1.0 } else { 1.0 };
        let lo = scale - y + scale * tmp;
        let hi = one + y;
        let lo = one - hi + y + lo;
        y = (hi + lo) - one;
        if y == 0.0 {
            y = d(sbits & 0x8000_0000_0000_0000);
        }
    }
    f64::from_bits(0x0010_0000_0000_0000) * y // 0x1p-1022
}

/// sign · exp(x + xtail), the sign −1 where `sign_bias` is SIGN_BIAS.
fn exp_inline(x: f64, xtail: f64, sign_bias: u64) -> f64 {
    let n = 1u64 << EXP_TABLE_BITS;
    let mut abstop = top12(x) & 0x7ff;
    let tiny = top12(f64::from_bits(0x3C90_0000_0000_0000)); // 0x1p-54
    if abstop.wrapping_sub(tiny) >= top12(512.0).wrapping_sub(tiny) {
        if abstop.wrapping_sub(tiny) >= 0x8000_0000 {
            // Tiny x (0 among them): no spurious underflow; WANT_ROUNDING is 1.
            let one = 1.0 + x;
            return if sign_bias != 0 { -one } else { one };
        }
        if abstop >= top12(1024.0) {
            return if x.to_bits() >> 63 != 0 { underflow(sign_bias) } else { overflow(sign_bias) };
        }
        // Large x: the special case below.
        abstop = 0;
    }
    // exp(x) = 2^(k/N) · exp(r); EXP_USE_TOINT_NARROW and TOINT_INTRINSICS are 0.
    let z = d(data::INV_LN2_N) * x;
    let shift = d(data::SHIFT);
    let mut kd = z + shift;
    let ki = kd.to_bits();
    kd -= shift;
    let mut r = x + kd * d(data::NEG_LN2_HI_N) + kd * d(data::NEG_LN2_LO_N);
    r += xtail;
    let idx = (2 * (ki % n)) as usize;
    let top = ki.wrapping_add(sign_bias) << (52 - EXP_TABLE_BITS);
    let tail = d(data::EXP_TAB[idx]);
    let sbits = data::EXP_TAB[idx + 1].wrapping_add(top);
    let [c2, c3, c4, c5] = data::EXP_POLY.map(d);
    let r2 = r * r;
    let tmp = tail + r + r2 * (c2 + r * c3) + r2 * r2 * (c4 + r * c5);
    if abstop == 0 {
        return special_case(tmp, sbits, ki);
    }
    let scale = d(sbits);
    scale + scale * tmp
}

/// 0 if not a whole number, 1 if odd, 2 if even, for the bits of a non-zero finite double.
fn check_int(iy: u64) -> u32 {
    let e = (iy >> 52 & 0x7ff) as i64;
    if e < 0x3ff {
        return 0;
    }
    if e > 0x3ff + 52 {
        return 2;
    }
    let s = (0x3ff + 52 - e) as u32;
    if iy & ((1u64 << s) - 1) != 0 {
        return 0;
    }
    if iy & (1u64 << s) != 0 {
        return 1;
    }
    2
}

/// Whether the bits are those of a zero, an infinity or a NaN.
fn zero_inf_nan(i: u64) -> bool {
    i.wrapping_mul(2).wrapping_sub(1) >= f64::INFINITY.to_bits().wrapping_mul(2).wrapping_sub(1)
}

/// x^y.
pub fn pow(x: f64, y: f64) -> f64 {
    let mut sign_bias = 0;
    let mut ix = x.to_bits();
    let iy = y.to_bits();
    let mut topx = top12(x);
    let topy = top12(y);
    let inf = f64::INFINITY.to_bits();
    let one = 1.0f64.to_bits();
    if topx.wrapping_sub(0x001) >= 0x7ff - 0x001 || (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
        // x below 2^-1022 (or 0), an infinity or a NaN; or |y| below 2^-65, from 2^63, or NaN.
        if zero_inf_nan(iy) {
            if iy.wrapping_mul(2) == 0 {
                return 1.0; // WANT_SNAN is 0
            }
            if ix == one {
                return 1.0;
            }
            if ix.wrapping_mul(2) > inf.wrapping_mul(2) || iy.wrapping_mul(2) > inf.wrapping_mul(2) {
                return x + y;
            }
            if ix.wrapping_mul(2) == one.wrapping_mul(2) {
                return 1.0;
            }
            if (ix.wrapping_mul(2) < one.wrapping_mul(2)) == (iy >> 63 == 0) {
                return 0.0; // |x| < 1 and y = inf, or |x| > 1 and y = -inf
            }
            return y * y;
        }
        if zero_inf_nan(ix) {
            let mut x2 = x * x;
            if ix >> 63 != 0 && check_int(iy) == 1 {
                x2 = -x2;
            }
            return if iy >> 63 != 0 { 1.0 / x2 } else { x2 };
        }
        // x and y are non-zero and finite.
        if ix >> 63 != 0 {
            let yint = check_int(iy);
            if yint == 0 {
                return (x - x) / (x - x); // __math_invalid
            }
            if yint == 1 {
                sign_bias = SIGN_BIAS;
            }
            ix &= 0x7fff_ffff_ffff_ffff;
            topx &= 0x7ff;
        }
        if (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
            // sign_bias is 0 here: y is not odd.
            if ix == one {
                return 1.0;
            }
            if (topy & 0x7ff) < 0x3be {
                // |y| < 2^-65: x^y is about 1 + y·log(x); WANT_ROUNDING is 1.
                return if ix > one { 1.0 + y } else { 1.0 - y };
            }
            return if (ix > one) == (topy < 0x800) { overflow(0) } else { underflow(0) };
        }
        if topx == 0 {
            // A subnormal x normalised so that its exponent becomes negative.
            ix = (x * f64::from_bits(0x4330_0000_0000_0000)).to_bits(); // 0x1p52
            ix &= 0x7fff_ffff_ffff_ffff;
            ix = ix.wrapping_sub(52 << 52);
        }
    }
    let (hi, lo) = log_inline(ix);
    let yhi = d(iy & (u64::MAX << 27));
    let ylo = y - yhi;
    let lhi = d(hi.to_bits() & (u64::MAX << 27));
    let llo = hi - lhi + lo;
    let ehi = yhi * lhi;
    let elo = ylo * lhi + y * llo;
    exp_inline(ehi, elo, sign_bias)
}
