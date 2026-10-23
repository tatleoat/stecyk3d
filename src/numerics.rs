// SPDX-License-Identifier: MIT
//! Numerics: square root and reciprocal square root, power, the C run time's random
//! number generator, and the table-driven 2^x and log2.
//!
//!
//! The NUM entries are not released. Behaviour here follows the published definitions named
//! in sources.md and the released vectors; provisional choices are marked and questioned
//!.
//!
//! Every operation is one IEEE binary64 operation in the order written, with no fused
//! multiply-add.

use crate::version::RenderAs;

mod convert;
pub use convert::to_i32;
mod bryce5_rand;
pub use bryce5_rand::Bryce5Rand;
mod inverse_trig;
pub use inverse_trig::{acos, asin};
pub(crate) mod extended;
pub use extended::ExtValue;
mod natural_log;
pub use natural_log::ln_ext;
mod register53;
pub use register53::{Reg53, RegValue};
mod arctangent;
pub use arctangent::{atan, atan_add, atan_div, atan_ext, atan2, atan2_add, atan2_div, atan2_ext};
pub mod x87;
mod nan_ops;
pub use nan_ops::{Op as NanOp, binary as binary_x87};
mod pow_data;
mod published_pow;
mod runtime_functions;
pub use runtime_functions::{RuntimeValue, acos_runtime, asin_runtime, pow_runtime, runtime_uses_processor};

// ---------------------------------------------------------------------------------------
// : the C run time's linear congruential generator
// ---------------------------------------------------------------------------------------

/// Multiplier of the generator.
const RAND_MUL: u32 = 214013;
/// Increment of the generator.
const RAND_ADD: u32 = 2531011;

/// The C run time's `rand`, with its state held in the value.
///
/// .: no global state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CRand {
    state: u32,
}

impl CRand {
    /// A generator seeded as `srand(seed)` would seed it.
    pub fn new(seed: u32) -> Self {
        CRand { state: seed }
    }

    /// Seed again, as `srand(seed)`.
    pub fn reseed(&mut self, seed: u32) {
        self.state = seed;
    }

    /// The current state (for saving and restoring a sequence).
    pub fn state(&self) -> u32 {
        self.state
    }

    /// The generator moved on by `n` draws without making them (the step state * a + c, composed
    /// with itself n times by squaring). Gives the place `n` numbers further in the sequence.
    pub fn skip(&self, mut n: u64) -> CRand {
        let (mut a, mut c) = (RAND_MUL, RAND_ADD);
        let (mut acc_a, mut acc_c) = (1u32, 0u32);
        while n > 0 {
            if n & 1 == 1 {
                acc_a = acc_a.wrapping_mul(a);
                acc_c = acc_c.wrapping_mul(a).wrapping_add(c);
            }
            c = c.wrapping_mul(a).wrapping_add(c);
            a = a.wrapping_mul(a);
            n >>= 1;
        }
        CRand { state: self.state.wrapping_mul(acc_a).wrapping_add(acc_c) }
    }

    /// The next output, from 0 to 0x7fff, as `rand()`.
    pub fn next_u15(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(RAND_MUL).wrapping_add(RAND_ADD);
        DRAWS_MADE.with(|n| n.set(n.get().wrapping_add(1)));
        ((self.state >> 16) & 0x7fff) as u16
    }
}

thread_local! {
    /// Draws made on this thread by every generator, for `draws_made` (a measure of work that is the same on
    /// every machine, unlike the clock).
    static DRAWS_MADE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many draws generators have made on this thread so far. A serial render takes the difference between two
/// readings as its measure of how much a frame that draws costs: the same number on any machine and under any
/// load. (Independent draws)
pub fn draws_made() -> u64 {
    DRAWS_MADE.with(|n| n.get())
}

impl Default for CRand {
    /// Seeded with 1, as the C standard says `rand` behaves before any `srand`.
    fn default() -> Self {
        CRand::new(1)
    }
}

// ---------------------------------------------------------------------------------------
// : square root and reciprocal square root
// ---------------------------------------------------------------------------------------

/// Inputs not greater than this give a square root of 0 and a reciprocal root of
/// `RSQRT_OF_TINY`.
pub const SQRT_TINY: f64 = 1e-150;

/// Reciprocal square root returned for zero and tiny inputs. (the double
/// nearest 1.7 x 10^308; square-root.tsv, bits 7fee42d130773b76)
pub const RSQRT_OF_TINY: f64 = 1.7e308;

/// The first guess's high word is this less half the input's high word.
const RECIPROOT_GUESS_HI: u32 = 1_608_987_383;

/// 3/2, the Newton step's constant.
const THREE_HALVES: f64 = 1.5;

/// Scale that keeps y·y and (y·y)·h off the subnormal grid: 2^100 per factor, 2^-200 after.
/// ("rounded to 53 significant bits as if the
/// exponent range were unbounded")
const SCALE_UP: f64 = 1_267_650_600_228_229_401_496_703_205_376.0; // 2^100
const SCALE_DOWN_SQ: f64 = 6.223_015_277_861_142e-61; // 2^-200

/// (y·y)·h, each product rounded to 53 significant bits with an unbounded exponent range.
/// Scaling by powers of two is exact, so for results in the normal range this is the plain
/// product; only where y·y falls below the normal range does it differ.
fn yy_h(y: f64, h: f64) -> f64 {
    let ys = y * SCALE_UP;
    ((ys * ys) * h) * SCALE_DOWN_SQ
}

/// The square root and the reciprocal square root of `x` as a render as 3.0 computes them.
///
pub fn sqrt_pair(x: f64) -> (f64, f64) {
    if x == f64::INFINITY {
        return (f64::INFINITY, f64::INFINITY);
    }
    if !(x > SQRT_TINY) {
        return (0.0, RSQRT_OF_TINY);
    }
    let h = x / 2.0;
    // First guess: the high 32 bits H replaced by 1,608,987,383 - floor(H / 2), the low 32
    // bits kept.
    let bits = x.to_bits();
    let hi = (bits >> 32) as u32;
    let guess_hi = RECIPROOT_GUESS_HI.wrapping_sub(hi >> 1);
    let mut y = f64::from_bits(((guess_hi as u64) << 32) | (bits & 0xffff_ffff));
    // y(k+1) = (3/2 - (y(k) y(k)) h) y(k); y3 is kept for the square root.
    let step = |y: f64| (THREE_HALVES - yy_h(y, h)) * y;
    for _ in 0..3 {
        y = step(y);
    }
    let y3 = y;
    let q = step(y3); // y4
    let r = (y3 * x) * (THREE_HALVES - yy_h(y3, h));
    (r, q)
}

/// The x87 indefinite value: the quiet NaN with the sign set.
pub const X87_INDEFINITE: f64 = f64::from_bits(0xfff8_0000_0000_0000);

/// A NaN made quiet, its other bits kept.
pub fn quieted(x: f64) -> f64 {
    f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000)
}

/// The square root and reciprocal square root a render as `version` uses.
///
pub fn sqrt_pair_as(version: RenderAs, x: f64) -> (f64, f64) {
    match version {
        RenderAs::Bryce30 => sqrt_pair(x),
        RenderAs::Bryce31 | RenderAs::Bryce50 => {
            if x.is_nan() {
                let q = quieted(x);
                return (q, q);
            }
            if x < 0.0 {
                return (X87_INDEFINITE, X87_INDEFINITE);
            }
            let r = x.sqrt();
            (r, 1.0 / r)
        }
    }
}

/// The square root of `x` as a render as `version` computes it.
pub fn sqrt_as(version: RenderAs, x: f64) -> f64 {
    sqrt_pair_as(version, x).0
}

/// The reciprocal square root of `x` as a render as `version` computes it.
pub fn rsqrt_as(version: RenderAs, x: f64) -> f64 {
    sqrt_pair_as(version, x).1
}

/// The square root of `x` (renders as 3.0).
pub fn sqrt(x: f64) -> f64 {
    sqrt_pair(x).0
}

/// The reciprocal square root of `x`.
pub fn rsqrt(x: f64) -> f64 {
    sqrt_pair(x).1
}

// ---------------------------------------------------------------------------------------
// : power
// ---------------------------------------------------------------------------------------
/// `x` to the power `y`, as the published pow names computes it (no fused
/// multiply-add, every operation rounded on its own).
///
/// (the published pow).
pub fn pow(x: f64, y: f64) -> f64 {
    published_pow::pow(x, y)
}

/// 2^e for a normal exponent e, exactly.
fn pow2(e: i32) -> f64 {
    debug_assert!((-1022..=1023).contains(&e));
    f64::from_bits(((e + 1023) as u64) << 52)
}
// ---------------------------------------------------------------------------------------
// : 2^x and log2(y)
// ---------------------------------------------------------------------------------------
//
// , (answer 0103, second: the functions as
// mathematics: tables, coefficients, grouping and every rounding)

/// The large value: 3.4e38 as a binary32 number, widened.
const NUM6_HUGE: f64 = f64::from_bits(0x47ef_f933_c000_0000);

/// T[i] = 2^(i/64), i = 0..63, each the correctly rounded double.
/// : generated and checked against a double-double
/// evaluation by examples/num6_search.rs (`gen`).
const EXP2_TABLE: [u64; 64] = [
    0x3ff0000000000000, 0x3ff02c9a3e778061, 0x3ff059b0d3158574, 0x3ff0874518759bc8,
    0x3ff0b5586cf9890f, 0x3ff0e3ec32d3d1a2, 0x3ff11301d0125b51, 0x3ff1429aaea92de0,
    0x3ff172b83c7d517b, 0x3ff1a35beb6fcb75, 0x3ff1d4873168b9aa, 0x3ff2063b88628cd6,
    0x3ff2387a6e756238, 0x3ff26b4565e27cdd, 0x3ff29e9df51fdee1, 0x3ff2d285a6e4030b,
    0x3ff306fe0a31b715, 0x3ff33c08b26416ff, 0x3ff371a7373aa9cb, 0x3ff3a7db34e59ff7,
    0x3ff3dea64c123422, 0x3ff4160a21f72e2a, 0x3ff44e086061892d, 0x3ff486a2b5c13cd0,
    0x3ff4bfdad5362a27, 0x3ff4f9b2769d2ca7, 0x3ff5342b569d4f82, 0x3ff56f4736b527da,
    0x3ff5ab07dd485429, 0x3ff5e76f15ad2148, 0x3ff6247eb03a5585, 0x3ff6623882552225,
    0x3ff6a09e667f3bcd, 0x3ff6dfb23c651a2f, 0x3ff71f75e8ec5f74, 0x3ff75feb564267c9,
    0x3ff7a11473eb0187, 0x3ff7e2f336cf4e62, 0x3ff82589994cce13, 0x3ff868d99b4492ed,
    0x3ff8ace5422aa0db, 0x3ff8f1ae99157736, 0x3ff93737b0cdc5e5, 0x3ff97d829fde4e50,
    0x3ff9c49182a3f090, 0x3ffa0c667b5de565, 0x3ffa5503b23e255d, 0x3ffa9e6b5579fdbf,
    0x3ffae89f995ad3ad, 0x3ffb33a2b84f15fb, 0x3ffb7f76f2fb5e47, 0x3ffbcc1e904bc1d2,
    0x3ffc199bdd85529c, 0x3ffc67f12e57d14b, 0x3ffcb720dcef9069, 0x3ffd072d4a07897c,
    0x3ffd5818dcfba487, 0x3ffda9e603db3285, 0x3ffdfc97337b9b5f, 0x3ffe502ee78b3ff6,
    0x3ffea4afa2a490da, 0x3ffefa1bee615a27, 0x3fff50765b6e4540, 0x3fffa7c1819e90d8,
];

/// S[i] = log2(1 + i/64), i = 0..63, each the correctly rounded double.
const LOG2_TABLE: [u64; 64] = [
    0x0000000000000000, 0x3f96e79685c2d22a, 0x3fa6bad3758efd87, 0x3fb0eb389fa29f9b,
    0x3fb663f6fac91316, 0x3fbbc84240adabba, 0x3fc08c588cda79e4, 0x3fc32ae9e278ae1a,
    0x3fc5c01a39fbd688, 0x3fc84c2bd02f03b3, 0x3fcacf5e2db4ec94, 0x3fcd49ee4c325970,
    0x3fcfbc16b902680a, 0x3fd11307dad30b76, 0x3fd24407ab0e073a, 0x3fd37124cea4cded,
    0x3fd49a784bcd1b8b, 0x3fd5c01a39fbd688, 0x3fd6e221cd9d0cde, 0x3fd800a563161c54,
    0x3fd91bba891f1709, 0x3fda33760a7f6051, 0x3fdb47ebf73882a1, 0x3fdc592fad295b56,
    0x3fdd6753e032ea0f, 0x3fde726aa1e754d2, 0x3fdf7a8568cb06cf, 0x3fe03fda8b97997f,
    0x3fe0c10500d63aa6, 0x3fe140c9faa1e544, 0x3fe1bf311e95d00e, 0x3fe23c41d42727c8,
    0x3fe2b803473f7ad1, 0x3fe3327c6ab49ca7, 0x3fe3abb3faa02167, 0x3fe423b07e986aa9,
    0x3fe49a784bcd1b8b, 0x3fe510118708a8f9, 0x3fe5848226989d34, 0x3fe5f7cff41e09af,
    0x3fe66a008e4788cc, 0x3fe6db196a76194a, 0x3fe74b1fd64e0754, 0x3fe7ba18f93502e4,
    0x3fe82809d5be7073, 0x3fe894f74b06ef8b, 0x3fe900e6160002cd, 0x3fe96bdad2acb5f6,
    0x3fe9d5d9fd5010b3, 0x3fea3ee7f38e181f, 0x3feaa708f58014d3, 0x3feb0e4126bcc86c,
    0x3feb74948f5532da, 0x3febda071cc67e6e, 0x3fec3e9ca2e1a055, 0x3feca258dca93316,
    0x3fed053f6d260896, 0x3fed6753e032ea0f, 0x3fedc899ab3ff56c, 0x3fee29142e0e0140,
    0x3fee88c6b3626a73, 0x3feee7b471b3a950, 0x3fef45e08bcf0655, 0x3fefa34e1177c233,
];

/// The coefficients of 2^x's polynomial in r.
const A1: f64 = f64::from_bits(0x3f77_e13b_6e28_18d0);
const A0: f64 = f64::from_bits(0x3f65_f6df_6afd_fca4);
const B1: f64 = f64::from_bits(0x3fce_364f_a5bc_e718);
const B0: f64 = f64::from_bits(0x3faf_2b28_6532_4148);
const C1: f64 = f64::from_bits(0x3fef_fd95_da61_f655);
const C0: f64 = f64::from_bits(0x3fe6_3c6a_5f94_5da2);

/// The coefficients of log2's polynomial in u.
const G5: f64 = f64::from_bits(0x3fd2_739f_4ec9_7d9c);
const G4: f64 = f64::from_bits(0x3fcd_6039_263b_4779);
const H3: f64 = f64::from_bits(0x3fde_c709_ce5c_8365);
const H2: f64 = f64::from_bits(0x3fd7_153d_3375_8194);
const J1: f64 = f64::from_bits(0x3ff7_1547_652b_8287);
const J0: f64 = f64::from_bits(0x3fe7_1547_6527_27db);

/// log2 of a y not greater than this is the negative large value. (D,
/// 0031fa182c40c60d, the double nearest 10^-307)
const LOG2_TINY_Y: f64 = f64::from_bits(0x0031_fa18_2c40_c60d);

/// The large value a version gives: L as 3.0 and 3.1, +infinity as 5.0.
fn num6_large(version: RenderAs) -> f64 {
    match version {
        RenderAs::Bryce50 => f64::INFINITY,
        RenderAs::Bryce30 | RenderAs::Bryce31 => NUM6_HUGE,
    }
}

/// At or below this binary32 (the one nearest 10^-18) 3.0's root of a single is +0.
const SINGLE_ROOT_FLOOR: f32 = 1e-18;
/// The constant of the first guess's encoding.
const SINGLE_ROOT_GUESS: u32 = 1_597_609_915;

/// How the last product c·y2·x of is grouped; the vectors decide it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootProduct {
    /// (c·y2)·x
    Left,
    /// c·(y2·x)
    Right,
    /// (c·x)·y2
    Outer,
}

/// s(x), 3.0's square root of a single (the cylinder light's ρ as 3.0).
pub fn sqrt_single_30(x: f32) -> f64 {
    sqrt_single_30_with(RootProduct::Right, x)
}

/// `sqrt_single_30` with a given grouping of the last product.
pub fn sqrt_single_30_with(g: RootProduct, x: f32) -> f64 {
    // : x not greater than the threshold (zeros, subnormals, negatives, NaN): +0.
    if !(x > SINGLE_ROOT_FLOOR) {
        return 0.0;
    }
    let xd = x as f64;
    let h = xd / 2.0;
    let y0 = f32::from_bits(SINGLE_ROOT_GUESS.wrapping_sub(x.to_bits() / 2)) as f64;
    // Two Newton-Raphson steps y' = y·(3/2 − h·y·y). (the form;
    // s(+infinity) = −infinity holds with it)
    let step = |y: f64| y * (1.5 - h * y * y);
    let y2 = step(step(y0)) as f32 as f64;
    let c = 1.5 - h * y2 * y2;
    match g {
        RootProduct::Left => c * y2 * xd,
        RootProduct::Right => c * (y2 * xd),
        RootProduct::Outer => c * xd * y2,
    }
}

/// 3.0's reciprocal square root of a single: c·y2 as forms them (its s(x) without
/// the product with x, a binary64), and for x not above's threshold the binary32
/// nearest 3.4 × 10^38.,
pub fn rsqrt_single_30(x: f32) -> f64 {
    if !(x > SINGLE_ROOT_FLOOR) {
        return 3.4e38_f32 as f64;
    }
    let h = x as f64 / 2.0;
    let y0 = f32::from_bits(SINGLE_ROOT_GUESS.wrapping_sub(x.to_bits() / 2)) as f64;
    let step = |y: f64| y * (1.5 - h * y * y);
    let y2 = step(step(y0)) as f32 as f64;
    (1.5 - h * y2 * y2) * y2
}

/// The polynomial of step 5 for the remainder r.
fn exp2_poly(r: f64) -> f64 {
    let q = r * r;
    let a = (A1 + r * A0) * q;
    let b = B1 + r * B0;
    let c = C1 + r * C0;
    (a + b) * q + c
}

/// 2^x as a render as `version` computes it.
pub fn exp2_as(version: RenderAs, x: f64) -> f64 {
    let t = x + 1023.0;
    if !x.is_finite() {
        return 0.0;
    }
    // : for t not greater than -2^31, W is's 32-bit result for t
    // read from 0 to 2^32 - 1: +0 where W is 0 or above 2^31, the large value from 2049 to
    // 2^31.: the same by W for every finite t with
    // |t| not less than 2^31 (t of 2^31 or more included), and +0 where W is 1 to 2048.
    if t.abs() >= 2_147_483_648.0 {
        let w = to_i32(t) as u32;
        if (1..=2048).contains(&w) {
            // : the band of W from 1 to 2048 runs
            // steps 4 to 6 of with n = W - 1 and f = t - n, whose fraction's six top
            // bits give i and are then cleared in r (f is not in [1, 2) here)
            let n = (w - 1) as i32;
            let f = t - n as f64;
            let bits = f.to_bits();
            let i = ((bits >> 46) & 63) as usize;
            let r = f64::from_bits(bits & !(63u64 << 46));
            let p = exp2_poly(r);
            let big_p = match n {
                0 => 0.0,
                2047 => f64::INFINITY,
                _ => pow2(n - 1023),
            };
            return p * (f64::from_bits(EXP2_TABLE[i]) * big_p);
        }
        return if (2049..=0x8000_0000).contains(&w) { num6_large(version) } else { 0.0 };
    }
    // : N is t cut toward zero; n = N - 1. Limits: |t| < 2^31.
    // (beyond the Limits, Open, the steps as written
    // are kept: the subtraction saturates rather than wrapping at |t| >= 2^63)
    let n = (t.trunc() as i64).saturating_sub(1);
    if n < 0 {
        return 0.0;
    }
    if n > 2047 {
        return num6_large(version);
    }
    // : f = t - n in [1, 2); i = whole part of 64 (f - 1); r = f - i/64
    let f = t - n as f64;
    let i = (64.0 * (f - 1.0)).trunc();
    let r = f - i / 64.0;
    // : q = r r; a = (A1 + r A0) q; b = B1 + r B0; c = C1 + r C0;
    // p = (a + b) q + c
    let p = exp2_poly(r);
    // : scale = T[i] P, P = 2^(n - 1023) (+0 for n = 0, +infinity for 2047)
    let big_p = match n {
        0 => 0.0,
        2047 => f64::INFINITY,
        _ => pow2(n as i32 - 1023),
    };
    let scale = f64::from_bits(EXP2_TABLE[i as usize]) * big_p;
    p * scale
}

/// log2(y) as a render as `version` computes it.
pub fn log2_as(version: RenderAs, y: f64) -> f64 {
    // : +0, -0, negatives, -infinity and NaN included
    if !(y > LOG2_TINY_Y) {
        return -num6_large(version);
    }
    // : E, F of y's encoding; k = E - 1023; i = the top six bits of F;
    // m has F and the exponent field 1023
    let bits = y.to_bits();
    let k = ((bits >> 52) as i64 - 1023) as f64;
    let i = ((bits >> 46) & 63) as usize;
    let m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    // : g = R[i] m; u = g - 1; q = u u; e = k + S[i]
    let r_i = 64.0 / (64.0 + i as f64); // R[i], the correctly rounded quotient
    let g = r_i * m;
    let u = g - 1.0;
    let q = u * u;
    let e = k + f64::from_bits(LOG2_TABLE[i]);
    // : a = (G5 - u G4) q; b = H3 - u H2; c = J1 - u J0; p = (a + b) q + c;
    // the result is u p + e
    let a = (G5 - u * G4) * q;
    let b = H3 - u * H2;
    let c = J1 - u * J0;
    let p = (a + b) * q + c;
    u * p + e
}

/// 2^x as renders as 3.0 and 3.1 compute it (the vectors' version).
pub fn exp2(x: f64) -> f64 {
    exp2_as(RenderAs::Bryce30, x)
}

/// log2(y) as renders as 3.0 and 3.1 compute it (the vectors' version).
pub fn log2(y: f64) -> f64 {
    log2_as(RenderAs::Bryce30, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `skip(n)` is n draws made and thrown away, for the n that matter to the renderer's
    /// per-pixel places (a stride of 1,000,003) and for a few small ones.
    #[test]
    fn skip_is_n_draws() {
        for seed in [1u32, 7, 0xdead_beef] {
            for n in [0u64, 1, 2, 3, 17, 1000, 1_000_003] {
                let mut stepped = CRand::new(seed);
                for _ in 0..n {
                    stepped.next_u15();
                }
                assert_eq!(CRand::new(seed).skip(n), stepped, "seed {seed} skip {n}");
            }
        }
    }

    // : t = x + 1023 of 2^31 or more follows W as
    // the band below -2^31 does (W from 1 to 2048: the vectors, exp2-far-band.tsv).
    #[test]
    fn exp2_fallback_at_large_t() {
        let x_of = |t: f64| t - 1023.0;
        let two31 = 2_147_483_648.0;
        for v in [RenderAs::Bryce30, RenderAs::Bryce50] {
            assert_eq!(exp2_as(v, x_of(two31)), num6_large(v)); // W = 2^31
            assert_eq!(exp2_as(v, x_of(two31 + 2049.0)), 0.0); // W above 2^31
            assert_eq!(exp2_as(v, x_of(2.0 * two31 + 3000.0)), num6_large(v)); // W = 3000
            assert_eq!(exp2_as(v, 1e20), 0.0); // W = 0
            assert_eq!(exp2_as(v, x_of(two31 - 1.0)), num6_large(v)); // inside the Limits
        }
    }

    #[test]
    fn rand_seed_one_is_default() {
        let mut a = CRand::default();
        let mut b = CRand::new(1);
        assert_eq!(a.next_u15(), b.next_u15());
        assert_eq!(CRand::new(1).next_u15(), 0x29);
    }

    #[test]
    fn square_root_by_version() {
        assert_eq!(sqrt_pair_as(RenderAs::Bryce30, -4.0), (0.0, RSQRT_OF_TINY));
        assert_eq!(sqrt_pair_as(RenderAs::Bryce30, f64::NAN), (0.0, RSQRT_OF_TINY));
        for v in [RenderAs::Bryce31, RenderAs::Bryce50] {
            assert_eq!(sqrt_pair_as(v, 2.0), (2f64.sqrt(), 1.0 / 2f64.sqrt()));
            assert_eq!(sqrt_pair_as(v, 0.0), (0.0, f64::INFINITY));
            let (r, q) = sqrt_pair_as(v, -0.0);
            assert_eq!((r.to_bits(), q), ((-0.0f64).to_bits(), f64::NEG_INFINITY));
            assert_eq!(sqrt_pair_as(v, f64::INFINITY), (f64::INFINITY, 0.0));
            let (r, q) = sqrt_pair_as(v, -1.0);
            assert_eq!((r.to_bits(), q.to_bits()), (0xfff8_0000_0000_0000, 0xfff8_0000_0000_0000));
            let signalling = f64::from_bits(0x7ff0_0000_0000_0001);
            assert_eq!(sqrt_pair_as(v, signalling).0.to_bits(), 0x7ff8_0000_0000_0001);
        }
    }

    #[test]
    fn pow_special_cases() {
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(1.0, f64::NAN), 1.0);
        assert!(pow(-2.0, 0.5).is_nan());
        assert_eq!(pow(-2.0, 3.0), -8.0);
        assert_eq!(pow(-0.0, -3.0), f64::NEG_INFINITY);
        assert_eq!(pow(0.5, f64::INFINITY), 0.0);
        assert_eq!(pow(f64::NEG_INFINITY, 3.0), f64::NEG_INFINITY);
        assert_eq!(pow(2.0, -1074.0), f64::from_bits(1));
        assert_eq!(pow(2.0, -1076.0), 0.0);
        assert_eq!(pow(2.0, 1024.0), f64::INFINITY);
        assert_eq!(pow(2.0, 1023.0), pow2(1023));
        assert_eq!(pow(4.0, 0.5), 2.0);
        assert_eq!(pow(10.0, 15.0), 1e15);
    }

    #[test]
    fn sqrt_exact_squares() {
        assert_eq!(sqrt_pair(4.0), (2.0, 0.5));
        assert_eq!(sqrt_pair(0.25), (0.5, 2.0));
    }

    #[test]
    fn num6_versions_and_edges() {
        assert_eq!(exp2_as(RenderAs::Bryce31, 3000.0).to_bits(), 0x47ef_f933_c000_0000);
        assert_eq!(exp2_as(RenderAs::Bryce50, 3000.0), f64::INFINITY);
        assert_eq!(log2_as(RenderAs::Bryce50, 0.0), f64::NEG_INFINITY);
        assert_eq!(log2_as(RenderAs::Bryce50, f64::NAN), f64::NEG_INFINITY);
        assert_eq!(log2_as(RenderAs::Bryce30, -1.0).to_bits(), 0xc7ef_f933_c000_0000);
        // (edges: 2^x of NaN and both infinities is +0; log2 of +inf and of
        // the largest double is 1024)
        for v in [RenderAs::Bryce30, RenderAs::Bryce50] {
            assert_eq!(exp2_as(v, f64::INFINITY).to_bits(), 0);
            assert_eq!(exp2_as(v, f64::NAN).to_bits(), 0);
            assert_eq!(log2_as(v, f64::INFINITY), 1024.0);
            assert_eq!(log2_as(v, f64::MAX), 1024.0);
        }
    }

    #[test]
    fn num6_constants_and_exact_cases() {
        assert_eq!(NUM6_HUGE.to_bits(), 0x47ef_f933_c000_0000);
        assert_eq!(exp2(0.0), 1.0);
        assert_eq!(exp2(-1.0), 0.5);
        assert_eq!(exp2(1023.0), pow2(1023));
        assert_eq!(exp2(1024.0), f64::INFINITY);
        assert_eq!(exp2(-1022.0), 0.0);
        assert_eq!(exp2(-1021.0), pow2(-1021));
        // log2 of a power of two is exact (observed property)
        // above the tiny threshold of 1e-307 (2^-1019 is the first power of two above it)
        for e in -1019..=1023 {
            assert_eq!(log2(pow2(e)), e as f64, "2^{e}");
        }
        assert_eq!(log2(f64::from_bits(1)).to_bits(), 0xc7ef_f933_c000_0000);
        assert_eq!(log2(pow2(-1020)).to_bits(), 0xc7ef_f933_c000_0000);
        assert_eq!(log2(1e-307).to_bits(), 0xc7ef_f933_c000_0000);
        assert_eq!(exp2(1025.9999999999995), f64::INFINITY);
        assert_eq!(exp2(1025.9999999999998), NUM6_HUGE);
        assert_eq!(exp2(2000.0), NUM6_HUGE);
        assert_eq!(exp2(-1021.0000000000001), 0.0);
    }

    #[test]
    fn num6_tables_match_formulas() {
        // Each entry within half an ulp of its formula as the platform computes it
        // (a weak check; the tables were verified by double-double in the search tool).
        for i in 0..64 {
            let t = f64::from_bits(EXP2_TABLE[i]);
            assert!((t - (i as f64 / 64.0).exp2()).abs() <= f64::EPSILON, "T[{i}]");
            let l = f64::from_bits(LOG2_TABLE[i]);
            assert!((l - (1.0 + i as f64 / 64.0).log2()).abs() <= f64::EPSILON, "L[{i}]");
        }
    }
}
