// SPDX-License-Identifier: MIT
//! A movie's range of frames: the frame numbers and times of a range given as a start and an
//! end time, and the times the movie's dialog gives for the whole animation.
//!
//! (A range: the numbers a and b, the first frame at s, 3.0's end test; the
//! dialog's split of a time into hours, minutes, seconds and a frame, and its join; the whole
//! animation's s = 0 and e from the length L; Limits)
//! (Release of)
//!
//! The steps gives in extended precision are computed here exactly on integers: `Ext`
//! holds a value as a signed integer significand times a power of two, and every operation
//! rounds its exact result once to a 64-bit significand (round to nearest, ties to even), as
//! the x87 register does; the end result is rounded once to binary64.

use crate::version::RenderAs;

/// The margin the dialog and the movie add before cutting to a whole number, as 3.1 and 5.0
/// (the movie) and as 5.0 (the dialog).
const MARGIN: f64 = 0.0005;
/// Seconds an hour and a minute.
const HOUR: f64 = 3600.0;
const MINUTE: f64 = 60.0;
/// The largest frame number the original counts (32-bit whole numbers).
const MOST_FRAMES: f64 = 2_147_483_648.0;

/// A value of the x87 extended register: (-1)^neg · m · 2^e, m below 2^64 once rounded.
/// this module's note
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ext {
    neg: bool,
    m: u128,
    e: i32,
}

fn bit_len(m: u128) -> u32 {
    128 - m.leading_zeros()
}

impl Ext {
    /// The exact value (-1)^neg · m · 2^e, with `sticky` set where nonzero bits below m were
    /// dropped, rounded to a 64-bit significand, nearest-even.
    fn rounded(neg: bool, m: u128, e: i32, sticky: bool) -> Ext {
        let n = bit_len(m);
        if n <= 64 {
            debug_assert!(!sticky || n == 0, "callers keep more than 64 bits where they drop any");
            return Ext { neg, m, e };
        }
        let shift = n - 64;
        let mut keep = m >> shift;
        let rest = m & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        let above = rest > half || (rest == half && sticky);
        let tie = rest == half && !sticky;
        if above || (tie && keep & 1 == 1) {
            keep += 1;
        }
        let mut e = e + shift as i32;
        if bit_len(keep) > 64 {
            keep >>= 1;
            e += 1;
        }
        Ext { neg, m: keep, e }
    }

    fn of_f64(d: f64) -> Ext {
        debug_assert!(d.is_finite());
        let bits = d.to_bits();
        let neg = bits >> 63 == 1;
        let field = ((bits >> 52) & 0x7ff) as i32;
        let frac = (bits & ((1u64 << 52) - 1)) as u128;
        if field == 0 {
            Ext { neg, m: frac, e: -1074 }
        } else {
            Ext { neg, m: frac | (1u128 << 52), e: field - 1075 }
        }
    }

    fn of_int(n: i64) -> Ext {
        Ext { neg: n < 0, m: n.unsigned_abs() as u128, e: 0 }
    }

    /// self + other, rounded once. The operand with the greater exponent is shifted left by at
    /// most 62 bits; bits of the other beyond that are jammed into its lowest bit (the result
    /// then holds more than 64 significant bits, so it rounds as the exact sum would).
    fn add(self, other: Ext) -> Ext {
        if self.m == 0 {
            return Ext::rounded(other.neg, other.m, other.e, false);
        }
        if other.m == 0 {
            return Ext::rounded(self.neg, self.m, self.e, false);
        }
        let (big, small) = if self.e >= other.e { (self, other) } else { (other, self) };
        let diff = (big.e - small.e) as u32;
        let (mb, ms, e, sticky) = if diff <= 62 {
            (big.m << diff, small.m, small.e, false)
        } else {
            let drop = diff - 62;
            let (kept, lost) = if drop >= 128 { (0, small.m != 0) } else { (small.m >> drop, small.m & ((1u128 << drop) - 1) != 0) };
            (big.m << 62, (kept << 1) | lost as u128, big.e - 63, false)
        };
        // With the jam the small part is one bit wider: shift the big one to match.
        let mb = if diff > 62 { mb << 1 } else { mb };
        if big.neg == small.neg {
            Ext::rounded(big.neg, mb + ms, e, sticky)
        } else if mb >= ms {
            Ext::rounded(big.neg, mb - ms, e, sticky)
        } else {
            Ext::rounded(small.neg, ms - mb, e, sticky)
        }
    }

    /// self · d (a binary64), rounded once: the exact product of a 64-bit and a 53-bit
    /// significand fits in 117 bits.
    fn mul_f64(self, d: f64) -> Ext {
        let o = Ext::of_f64(d);
        Ext::rounded(self.neg != o.neg, self.m * o.m, self.e + o.e, false)
    }

    /// The whole number n divided by the binary32 r, rounded once. r must be above 0.
    fn int_div_f32(n: u32, r: f32) -> Ext {
        let d = Ext::of_f64(f64::from(r));
        let s = 127 - bit_len(n as u128).max(1);
        let num = (n as u128) << s;
        let q = num / d.m;
        let rem = num % d.m;
        Ext::rounded(false, q, -(s as i32) - d.e, rem != 0)
    }

    /// Rounded toward zero to a whole number (a 32-bit store with that rounding).
    fn whole_toward_zero(self) -> i64 {
        let mag = if self.e >= 0 {
            if self.e >= 64 { u128::MAX } else { self.m << self.e }
        } else if -self.e >= 128 {
            0
        } else {
            self.m >> (-self.e)
        };
        let mag = mag.min(i64::MAX as u128) as i64;
        if self.neg { -mag } else { mag }
    }

    /// Rounded once to binary64 (nearest, ties to even); the values here are normal doubles.
    fn to_f64(self) -> f64 {
        if self.m == 0 {
            return if self.neg { -0.0 } else { 0.0 };
        }
        let n = bit_len(self.m);
        let (m, e) = if n > 53 {
            let r = Ext::rounded(self.neg, self.m, self.e, false); // at most 64 bits
            let shift = bit_len(r.m) - 53;
            let keep = r.m >> shift;
            let rest = r.m & ((1u128 << shift) - 1);
            let half = 1u128 << (shift - 1);
            let mut keep = keep;
            if rest > half || (rest == half && keep & 1 == 1) {
                keep += 1;
            }
            (keep, r.e + shift as i32)
        } else {
            (self.m, self.e)
        };
        let v = m as f64 * 2f64.powi(e);
        if self.neg { -v } else { v }
    }
}

/// The movie dialog's time from a time T in seconds and the document's frame rate r, as each
/// version splits T into hours, minutes, seconds and a frame and joins them again. `None`
/// where the rate's whole part is 0 (no frame within the second can be counted).
pub fn dialog_time(t: f64, rate: f32, version: RenderAs) -> Option<f64> {
    if !t.is_finite() || !(rate >= 1.0) || !rate.is_finite() {
        return None;
    }
    let margin = if version == RenderAs::Bryce50 { Some(Ext::of_f64(MARGIN)) } else { None };
    let cut = |x: Ext| margin.map_or(x, |m| x.add(m)).whole_toward_zero();
    let whole_rate = rate.trunc() as i64; // R = ⌊r⌋
    let tx = Ext::of_f64(t);
    // : N = ⌊T·R (+ 0.0005)⌋, F = N mod R
    let n = cut(tx.mul_f64(whole_rate as f64));
    let f = n.rem_euclid(whole_rate);
    // H = ⌊T·(1/3600) (+ 0.0005)⌋; T′ = T − 3600·H; M = ⌊T′·(1/60) (+ 0.0005)⌋; T″ = T′ − 60·M;
    // S = ⌊T″ (+ 0.0005)⌋, the reciprocals the binary64 ones.
    let h = cut(tx.mul_f64(1.0 / HOUR));
    let t1 = tx.add(Ext::of_int(-(h * HOUR as i64)));
    let m = cut(t1.mul_f64(1.0 / MINUTE));
    let t2 = t1.add(Ext::of_int(-(m * MINUTE as i64)));
    let s = cut(t2);
    // The join: F / r + 3600·H + S + 60·M as 5.0; F / r + ((S + 60·M) + 3600·H) as 3.0, 3.1.
    let frame = Ext::int_div_f32(f as u32, rate);
    let (hours, minutes, seconds) = (Ext::of_int(h * HOUR as i64), Ext::of_int(m * MINUTE as i64), Ext::of_int(s));
    let joined = match version {
        RenderAs::Bryce50 => frame.add(hours).add(seconds).add(minutes),
        RenderAs::Bryce30 | RenderAs::Bryce31 => frame.add(seconds.add(minutes).add(hours)),
    };
    Some(joined.to_f64())
}

/// A movie's frames: frame `first` at the start time itself, then the frames after it, each at
/// its own time k / f.
#[derive(Clone, Debug, PartialEq)]
pub struct MovieRange {
    /// a: the first frame's number, and s: its time.
    pub first: u64,
    pub start: f64,
    /// b: the last frame's number (none after `first` where it is not above it).
    pub last: u64,
    /// e, for 3.0's end test.
    pub end: f64,
    rate: f64,
    ends_before_end_time: bool,
}

/// Why a range is not made.
#[derive(Clone, Debug, PartialEq)]
pub enum RangeRefusal {
    /// A start or end time that is not a number, or a rate not above 0.
    Inputs,
    /// An end whose frame number reaches 2^31.
    TooMany(f64),
}

impl MovieRange {
    /// The range from the start time s to the end time e at the rate f, as `version` numbers
    /// it: a = ⌊s·f + 0.0005⌋ and b = ⌊e·f + 0.0005⌋ as 3.1 and 5.0, ⌊s·f⌋ and ⌊e·f⌋ as 3.0
    /// (each product and sum in the extended register, cut toward zero); s below 0 taken as 0.
    pub fn new(start: f64, end: f64, rate: f32, version: RenderAs) -> Result<MovieRange, RangeRefusal> {
        if start.is_nan() || end.is_nan() || !(rate > 0.0) || !rate.is_finite() {
            return Err(RangeRefusal::Inputs);
        }
        let start = if start <= 0.0 { 0.0 } else { start };
        let f = f64::from(rate);
        let three = version == RenderAs::Bryce30;
        let number = |t: f64| -> Result<u64, RangeRefusal> {
            let p = Ext::of_f64(t).mul_f64(f);
            let p = if three { p } else { p.add(Ext::of_f64(MARGIN)) };
            if p.to_f64() >= MOST_FRAMES {
                return Err(RangeRefusal::TooMany(t));
            }
            Ok(p.whole_toward_zero().max(0) as u64)
        };
        let first = number(start)?;
        let last = if end <= start { first } else { number(end)? };
        Ok(MovieRange { first, start, last, end, rate: f, ends_before_end_time: three })
    }

    /// The whole animation: s = 0 and e the dialog's time for the length L.
    /// (A range: its choices)
    pub fn whole_animation(length: f32, rate: f32, version: RenderAs) -> Result<MovieRange, RangeRefusal> {
        let end = dialog_time(f64::from(length).max(0.0), rate, version).ok_or(RangeRefusal::Inputs)?;
        MovieRange::new(0.0, end, rate, version)
    }

    /// The frames in order, each (number, time).
    pub fn frames(&self) -> impl Iterator<Item = (u64, f64)> + '_ {
        let after = (self.first + 1..=self.last)
            .map(move |k| (k, k as f64 / self.rate))
            .take_while(move |&(_, t)| !(self.ends_before_end_time && t >= self.end));
        std::iter::once((self.first, self.start)).chain(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_rounds_once_to_64_bits() {
        // 1/3 rounded to 64 bits, then to binary64: the same as the binary64 quotient here.
        let third = Ext::int_div_f32(1, 3.0);
        assert_eq!(third.to_f64(), 1.0 / 3.0);
        assert_eq!(Ext::of_f64(2.5).whole_toward_zero(), 2);
        assert_eq!(Ext::of_f64(-2.5).whole_toward_zero(), -2);
        assert_eq!(Ext::of_f64(1.0).add(Ext::of_f64(MARGIN)).to_f64(), 1.0005);
        assert_eq!(Ext::of_int(5).add(Ext::of_int(-7)).to_f64(), -2.0);
    }

    #[test]
    fn dialog_split_and_join() {
        let l = f64::from(4.9999f32);
        let e = dialog_time(l, 15.0, RenderAs::Bryce50).unwrap();
        assert!((e - (5.0 + 14.0 / 15.0)).abs() < 1e-12, "{e}");
        // As 3.0 and 3.1, L brought down to the whole frame below it: 4 + 14/15.
        let e = dialog_time(l, 15.0, RenderAs::Bryce31).unwrap();
        assert!((e - (4.0 + 14.0 / 15.0)).abs() < 1e-12, "{e}");
        // A whole number of frames: L itself.
        assert_eq!(dialog_time(4.0, 15.0, RenderAs::Bryce30), Some(4.0));
        assert_eq!(dialog_time(4.0, 15.0, RenderAs::Bryce50), Some(4.0));
        assert_eq!(dialog_time(3725.0, 30.0, RenderAs::Bryce31), Some(3725.0));
    }

    #[test]
    fn whole_animation_frames() {
        let numbers = |v| MovieRange::whole_animation(4.0, 15.0, v).unwrap().frames().map(|(k, _)| k).collect::<Vec<_>>();
        assert_eq!(numbers(RenderAs::Bryce31), (0..=60).collect::<Vec<_>>());
        assert_eq!(numbers(RenderAs::Bryce50), (0..=60).collect::<Vec<_>>());
        assert_eq!(numbers(RenderAs::Bryce30), (0..60).collect::<Vec<_>>());
        let r = MovieRange::whole_animation(4.0, 15.0, RenderAs::Bryce30).unwrap();
        let times: Vec<f64> = r.frames().map(|(_, t)| t).collect();
        assert_eq!(times[0], 0.0);
        assert_eq!(times[1], 1.0 / 15.0);
    }

    /// (A range: the first frame at s itself; a start above the end gives the
    /// frame at s alone; s below 0 taken as 0)
    #[test]
    fn first_frame_at_the_start_time() {
        let r = MovieRange::new(0.53, 1.0, 15.0, RenderAs::Bryce50).unwrap();
        let f: Vec<(u64, f64)> = r.frames().collect();
        assert_eq!(f[0], (7, 0.53));
        assert_eq!(f[1], (8, 8.0 / 15.0));
        assert_eq!(f.last().unwrap().0, 15);
        let r = MovieRange::new(2.0, 1.0, 15.0, RenderAs::Bryce50).unwrap();
        assert_eq!(r.frames().collect::<Vec<_>>(), vec![(30, 2.0)]);
        let r = MovieRange::new(-1.0, 0.0, 15.0, RenderAs::Bryce30).unwrap();
        assert_eq!(r.frames().collect::<Vec<_>>(), vec![(0, 0.0)]);
        assert_eq!(MovieRange::new(0.0, 2.0e9, 15.0, RenderAs::Bryce50), Err(RangeRefusal::TooMany(2.0e9)));
    }
}
