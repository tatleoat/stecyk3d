// SPDX-License-Identifier: MIT
//! The Microsoft C runtime's pow, arcsine and arccosine, as the Variants
//! state them (processor instructions at 53-bit precision, results left extended).
//!
//! (FPATAN, F2XM1 as emulated here),
//! ("Revision of 2026-10-22")
//!
//! No render calls these: the entries' Outputs are the published libraries'. They exist so
//! that the Variants' evidence (power-runtime.tsv, arcsine-arccosine-runtime.tsv) is run.

use super::arctangent::atan2_ext;
use super::extended::{Dd, ExtValue, dd_to_ext, decompose};
use super::published_pow;
use super::x87::f2xm1_plus_one;

/// A result of the runtime: a number in the extended format, an infinity or a NaN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RuntimeValue {
    Nan,
    Infinity { negative: bool },
    Finite(ExtValue),
}

impl RuntimeValue {
    /// The result as a double (an extended value rounded once). The NaN is the host's.
    pub fn to_f64(self) -> f64 {
        match self {
            RuntimeValue::Nan => f64::NAN,
            RuntimeValue::Infinity { negative } => {
                if negative { f64::NEG_INFINITY } else { f64::INFINITY }
            }
            RuntimeValue::Finite(v) => v.to_f64(),
        }
    }

    /// The 80 bits as 20 hexadecimal digits (sign and exponent, then the significand), or
    /// `None` for a NaN.
    pub fn extended_hex(self) -> Option<String> {
        let (negative, exponent, mantissa) = match self {
            RuntimeValue::Nan => return None,
            RuntimeValue::Infinity { negative } => (negative, 0x7fff_u16, 1u64 << 63),
            RuntimeValue::Finite(v) if v.mantissa == 0 => (v.negative, 0, 0),
            RuntimeValue::Finite(v) => (v.negative, (v.exponent + 63 + 16383) as u16, v.mantissa),
        };
        let sign_exponent = exponent | if negative { 0x8000 } else { 0 };
        Some(format!("{sign_exponent:04x}{mantissa:016x}"))
    }
}

fn zero(negative: bool) -> RuntimeValue {
    RuntimeValue::Finite(ExtValue { negative, mantissa: 0, exponent: 0 })
}

/// A finite double as an extended value (exact).
fn ext_of(d: f64) -> ExtValue {
    if d == 0.0 {
        return ExtValue { negative: d.is_sign_negative(), mantissa: 0, exponent: 0 };
    }
    let (m, e) = decompose(d);
    let lz = m.leading_zeros() as i32;
    ExtValue { negative: d.is_sign_negative(), mantissa: m << lz, exponent: e - lz }
}

fn of_double(d: f64) -> RuntimeValue {
    if d.is_nan() {
        RuntimeValue::Nan
    } else if d.is_infinite() {
        RuntimeValue::Infinity { negative: d < 0.0 }
    } else {
        RuntimeValue::Finite(ext_of(d))
    }
}

/// s = the square root of (1 + x)(1 - x), each operation rounded to 53 bits.
fn root_term(x: f64) -> f64 {
    ((1.0 + x) * (1.0 - x)).sqrt()
}

/// FPATAN of the point (x, y) as the processor leaves it, where this machine's processor is
/// an Intel one (the processor's values are made on) with its control word at 64-bit
/// precision and rounding to nearest; else `None`. The result is finite and not zero, or
/// `None`.
///
/// The original's runtime (the C runtime that 3.0 and 3.1
/// carry) executes FPATAN itself on the point, so the extended value's last bit is the
/// processor's: no rule states which of the two neighbouring 64-bit values it gives (the
/// correctly rounded one in 97.3% of the file's 5,254 values, the next one up or down in the
/// rest). Intel's instruction on the machine this was read on (a 12th generation Core)
/// reproduces all 5,254 values of arcsine-arccosine-runtime.tsv bit for bit.
#[cfg(target_arch = "x86_64")]
fn processor_fpatan(y: f64, x: f64) -> Option<ExtValue> {
    use std::arch::asm;
    use std::sync::OnceLock;
    static INTEL: OnceLock<bool> = OnceLock::new();
    let intel = *INTEL.get_or_init(|| {
        // SAFETY: cpuid is available on every x86_64 processor.
        let id = unsafe { std::arch::x86_64::__cpuid(0) };
        (id.ebx, id.edx, id.ecx) == (u32::from_le_bytes(*b"Genu"), u32::from_le_bytes(*b"ineI"), u32::from_le_bytes(*b"ntel"))
    });
    if !intel {
        return None;
    }
    let mut control = 0u16;
    let mut out = [0u8; 10];
    // SAFETY: x87 instructions on memory this function owns; the stack of the unit is left as
    // found (two loads, FPATAN pops one, the store pops the other).
    unsafe {
        asm!("fnstcw word ptr [{c}]", c = in(reg) &mut control, options(nostack));
        // precision control 64 bits (bits 8 and 9 set), rounding to nearest (bits 10 and 11 clear)
        if control & 0x0f00 != 0x0300 {
            return None;
        }
        asm!("fld qword ptr [{y}]", "fld qword ptr [{x}]", "fpatan", "fstp tbyte ptr [{o}]",
            y = in(reg) &y, x = in(reg) &x, o = in(reg) out.as_mut_ptr(), options(nostack));
    }
    let mantissa = u64::from_le_bytes(out[..8].try_into().unwrap());
    let sign_exponent = u16::from_le_bytes([out[8], out[9]]);
    let exponent = i32::from(sign_exponent & 0x7fff);
    if mantissa >> 63 == 0 || exponent == 0 || exponent == 0x7fff {
        return None;
    }
    Some(ExtValue { negative: sign_exponent & 0x8000 != 0, mantissa, exponent: exponent - 16383 - 63 })
}

#[cfg(not(target_arch = "x86_64"))]
fn processor_fpatan(_y: f64, _x: f64) -> Option<ExtValue> {
    None
}

/// FPATAN for the runtime's two functions: the processor's own value where this machine's is
/// an Intel one, else the emulation (correctly rounded to 64 bits: the last bit differs from
/// the processor's on about 2.7% of the file's values, by one unit).,
fn runtime_fpatan(y: f64, x: f64) -> ExtValue {
    processor_fpatan(y, x).unwrap_or_else(|| atan2_ext(y, x))
}

/// Whether `asin_runtime` and `acos_runtime` give the processor's own extended values on this
/// machine (and not the emulation's).
pub fn runtime_uses_processor() -> bool {
    processor_fpatan(0.5, 1.0).is_some()
}

/// The runtime's arcsine, left extended.
pub fn asin_runtime(x: f64) -> RuntimeValue {
    if !(x.abs() <= 1.0) {
        return RuntimeValue::Nan;
    }
    RuntimeValue::Finite(runtime_fpatan(x, root_term(x)))
}

/// The runtime's arccosine, left extended.
pub fn acos_runtime(x: f64) -> RuntimeValue {
    if !(x.abs() <= 1.0) {
        return RuntimeValue::Nan;
    }
    RuntimeValue::Finite(runtime_fpatan(root_term(x), x))
}

/// log2 of a finite positive x as a double-double (about 2^-100 relative).
fn log2_dd(x: f64) -> Dd {
    let (m, e) = decompose(x);
    let bits = 64 - m.leading_zeros() as i32;
    let mut k = e + bits; // x = (m / 2^bits) 2^k with the factor in [0.5, 1)
    let mut v = Dd::from_scaled_int(m as u128, -bits);
    if v.hi < 0.75 {
        v = v.add(v);
        k -= 1;
    }
    let one = Dd::new(1.0);
    let s = v.sub(one).div(v.add(one));
    let s2 = s.mul(s);
    let (mut term, mut sum, mut n) = (s, s, 1.0f64);
    loop {
        term = term.mul(s2);
        n += 2.0;
        let add = term.div_f64(n);
        sum = sum.add(add);
        if add.hi == 0.0 || add.hi.abs() <= sum.hi.abs() * 1e-34 {
            break;
        }
    }
    let ln_v = sum.add(sum);
    let ln2 = Dd { hi: f64::from_bits(0x3FE6_2E42_FEFA_39EF), lo: f64::from_bits(0x3C7A_BC9E_3B39_803F) };
    Dd::new(k as f64).add(ln_v.div(ln2))
}

/// t = FYL2X(x, y) for x finite and positive, y finite and not zero: the extended value of
/// y log2 x, or `None` when its size is far beyond what any double result can hold.
fn fyl2x(x: f64, y: f64) -> Option<ExtValue> {
    let l = log2_dd(x);
    if (y * l.hi).abs() > 8192.0 {
        return None;
    }
    Some(dd_to_ext(Dd::new(y).mul(l), 0))
}

/// n (t rounded to the nearest whole number, ties to even) and f = t - n rounded to 53
/// bits, for an extended t of size below 2^40.
fn split_whole(t: ExtValue) -> (i64, f64) {
    if t.mantissa == 0 {
        return (0, 0.0);
    }
    let sign = if t.negative { -1.0 } else { 1.0 };
    if t.exponent >= 0 {
        return (if t.negative { -1 } else { 1 } * ((t.mantissa as i128) << t.exponent) as i64, 0.0);
    }
    let s = (-t.exponent) as u32;
    if s > 126 {
        return (0, t.to_f64());
    }
    let m = t.mantissa as u128;
    let whole = if s >= 128 { 0 } else { m >> s };
    let rest = m - (whole << s);
    let half = 1u128 << (s - 1);
    let n = whole + u128::from(rest > half || (rest == half && whole & 1 == 1));
    let diff = m as i128 - ((n << s) as i128);
    let f = sign * diff.signum() as f64 * super::extended::scale2(diff.unsigned_abs() as f64, -(s as i32));
    let n = if t.negative { -(n as i64) } else { n as i64 };
    (n, f)
}

/// 2^n r as an extended value, for a double r in [0.7, 1.5]; an infinity beyond 2^1024 and
/// a multiple of 2^-1074 cut toward zero below 2^-1022.
fn scaled(negative: bool, r: f64, n: i64) -> RuntimeValue {
    let (rm, re) = decompose(r);
    let e = re as i64 + n; // value = rm * 2^e, rm below 2^53
    let top = e + 64 - rm.leading_zeros() as i64; // the value is below 2^top
    if top > 1024 {
        return RuntimeValue::Infinity { negative };
    }
    if e >= -1074 {
        let mut v = ext_of(r);
        v.exponent += n as i32;
        v.negative = negative;
        return RuntimeValue::Finite(v);
    }
    let drop = -1074 - e;
    let units = if drop >= 64 { 0 } else { rm >> drop };
    if units == 0 {
        return zero(negative);
    }
    let lz = units.leading_zeros() as i32;
    RuntimeValue::Finite(ExtValue { negative, mantissa: units << lz, exponent: -1074 - lz })
}

/// x^y as the runtime computes it, left extended.
pub fn pow_runtime(x: f64, y: f64) -> RuntimeValue {
    if x.is_nan() || y.is_nan() {
        return RuntimeValue::Nan;
    }
    if y == 0.0 {
        return of_double(1.0);
    }
    if y.is_infinite() && x.abs() == 1.0 {
        return RuntimeValue::Nan;
    }
    if y.is_infinite() || x == 0.0 || x.is_infinite() {
        return of_double(published_pow::pow(x, y));
    }
    let mut negative = false;
    if x < 0.0 {
        if y.fract() != 0.0 {
            return RuntimeValue::Nan;
        }
        negative = y.abs() < 9_007_199_254_740_992.0 && (y.abs() as u64) & 1 == 1;
    }
    let Some(t) = fyl2x(x.abs(), y) else {
        // |t| beyond 8192: far above or below the range of a double.
        let up = (y > 0.0) == (x.abs() > 1.0);
        return if up { RuntimeValue::Infinity { negative } } else { zero(negative) };
    };
    let (n, f) = split_whole(t);
    scaled(negative, f2xm1_plus_one(f), n)
}
