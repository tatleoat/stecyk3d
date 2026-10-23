// SPDX-License-Identifier: MIT
//! The octave methods: a noise combined over its octaves, and the point each method leaves.
//!
//! (revised b2ca09a; : every method 0 to 12
//! as a formula; counts 0 to 9; Effects: the point left,
//! noise-types.jsonl, phase.jsonl)
//! (Octave methods)

use crate::numerics::extended::ExtValue;

use super::orders::{Method4Grouping, TextureOrders};

/// The octave counts specifies.
pub const MAX_OCTAVES: i64 = 9; // Limits

/// Methods 0, 1, 2: the factor F.
const SUM_FACTORS: [f64; 3] = [2.0, 1.8, 1.5];
/// Method 3's turn and shifts.
const M3_SHIFT: [f64; 3] = [425.0, 733.0, 523.0];
const M3_A: f64 = 0.656;
const M3_B: f64 = 0.11533333333;
const M3_Z: f64 = 0.6666;
const M3_WEIGHT: f64 = 1.5;
/// Method 4's turn, weight factor and offset.
const M4_COS: f64 = 0.9396;
const M4_SIN: f64 = 0.342;
const M4_WEIGHT: f64 = 0.65;
const M4_OFFSET: f64 = 0.1;
/// Methods 5, 6, 7: the scale c, and their turn.
const M5_SCALES: [f64; 3] = [0.65, 0.75, 0.6];
const M5_A: f64 = 0.984;
const M5_B: f64 = 0.173;
const M5_WEIGHT: f64 = 1.8;
/// Method 8's offset, weight factor and the base of g.
const M8_OFFSET: f64 = 0.15;
const M8_WEIGHT: f64 = 1.8;
const M8_G: f64 = 8.0;
/// Method 12's scale and shift.
const M12_SCALE: f64 = 0.95;
const M12_SHIFT: f64 = 331.33;

fn halved(p: [f64; 3]) -> [f64; 3] {
    p.map(|c| c * 0.5)
}

/// A method's value and the point it leaves (Effects). `exact`: where with an octave
/// count of 0 the result of method 4, 9 or 10 is an extended sample itself, that sample (a
/// phase multiplies it by its amount unrounded, type 12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Octaves {
    pub value: f64,
    pub left: [f64; 3],
    pub exact: Option<ExtValue>,
}

/// A sample of a noise: a double, or an extended value (type 12 with the 1-D setting)
/// that each operation taking it takes unrounded, rounding only its own result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoiseSample {
    Double(f64),
    Extended(ExtValue),
}

impl NoiseSample {
    /// Stored as a binary64 (rounded once).
    pub fn stored(self) -> f64 {
        self.d()
    }

    /// Kept as a running value: rounded to binary64.
    fn d(self) -> f64 {
        match self {
            NoiseSample::Double(v) => v,
            NoiseSample::Extended(e) => e.to_f64(),
        }
    }

    /// N·w, rounded once.
    fn times(self, w: f64) -> f64 {
        match self {
            NoiseSample::Double(v) => v * w,
            NoiseSample::Extended(e) => e.mul_to_f64(w),
        }
    }

    /// a − N, rounded once.
    fn from(self, a: f64) -> f64 {
        match self {
            NoiseSample::Double(v) => a - v,
            NoiseSample::Extended(e) => e.negated().add_to_f64(a),
        }
    }

    /// a + N, rounded once.
    fn plus(self, a: f64) -> f64 {
        match self {
            NoiseSample::Double(v) => a + v,
            NoiseSample::Extended(e) => e.add_to_f64(a),
        }
    }

    /// a < N, exactly.
    fn above(self, a: f64) -> bool {
        match self {
            NoiseSample::Double(v) => a < v,
            NoiseSample::Extended(e) => e.cmp_f64(a) == Some(std::cmp::Ordering::Greater),
        }
    }

    fn exact(self) -> Option<ExtValue> {
        match self {
            NoiseSample::Double(_) => None,
            NoiseSample::Extended(e) => Some(e),
        }
    }
}

/// The octave value of `method` with count `n` (0 to 9) of the noise `noise` at p, for a noise
/// type other than 0 (type 0 gives 0.5 and leaves p: the caller's). `one_d`: the noise's
/// dimension setting is 1-D (methods 9 and 10 take another third sample then). `moves`: the
/// point a sample leaves where it moves the point it is taken at (type 8 with the 3-D setting,
/// Effects; `None`: no sample moves it); a method that goes on from a sampled point goes
/// on from the moved one. `start`: method 11's q0 (the caller forms it from `perm`).
/// `None` for a count above 9 (the implementer's choice: refused) and method 11 without its
/// q0; a method outside 1 to 12 is method 0 and a count below 0 is as Limits says. Methods 9 and 10 over a sample that moves
/// its point take their copies, and h, from the moved point (revised).
#[allow(clippy::too_many_arguments)]
pub fn octave_value(
    method: i64,
    n: i64,
    one_d: bool,
    p: [f64; 3],
    orders: &TextureOrders,
    noise: &dyn Fn([f64; 3]) -> NoiseSample,
    moves: Option<&dyn Fn([f64; 3]) -> [f64; 3]>,
    start: Option<[f64; 3]>,
) -> Option<Octaves> {
    // : a method that is not 1 to 12 is method 0 (any whole
    // number may be stored); the count is from -9 to 9 and a count above 9 never comes from a
    // document (refused here: the implementer's choice).
    if n > MAX_OCTAVES {
        return None;
    }
    let method = if (1..=12).contains(&method) { method } else { 0 };
    // : methods 9 and 10 give what a count of 5 or more
    // gives (every stage); method 4 gives N(0.5·p) and forms no octave (the loop below does not
    // run, with κ = 2^-1); method 8 gives s0 alone with g = n + 8 (below); every other method
    // gives what count 0 gives.
    let n = if n < 0 && matches!(method, 9 | 10) {
        5
    } else if n < 0 && !matches!(method, 4 | 8) {
        0
    } else {
        n
    };
    let same = |value: f64| Octaves { value, left: p, exact: None };
    let itself = |s: NoiseSample| Octaves { value: s.d(), left: p, exact: s.exact() };
    let at = |q: [f64; 3]| noise(q).d();
    let mv = |q: [f64; 3]| moves.map_or(q, |m| m(q));
    Some(match method {
        0..=2 | 11 => {
            let (f, p0) = match method {
                11 => (SUM_FACTORS[1], start?),
                _ => (SUM_FACTORS[method as usize], p),
            };
            let (mut pk, mut w, mut s, mut total) = (mv(p0), 1.0, at(p0), 1.0);
            for _ in 1..=n {
                pk = halved(pk);
                w *= f;
                s += noise(pk).times(w);
                pk = mv(pk);
                total += w;
            }
            let value = if n == 0 { s } else { s / total };
            match method {
                11 => same(value),
                _ => Octaves { value, left: pk, exact: None },
            }
        }
        3 => {
            let (mut pk, mut w, mut b) = (mv(p), 1.0, at(p));
            if n == 0 {
                return Some(Octaves { value: b, left: pk, exact: None });
            }
            let [ox, oy] = orders.method3;
            for _ in 1..=n {
                let [x, y, z] = pk;
                pk = [
                    ox.sum([M3_SHIFT[0], M3_A * x, -(M3_B * y)]),
                    oy.sum([M3_SHIFT[1], M3_A * y, M3_B * x]),
                    M3_SHIFT[2] + M3_Z * z,
                ];
                w *= M3_WEIGHT;
                let v = noise(pk).times(w);
                pk = mv(pk);
                if b < v {
                    b = v;
                }
            }
            Octaves { value: b / w, left: pk, exact: None }
        }
        4 => {
            if n == 0 {
                return Some(itself(noise(p)));
            }
            let m = (n - 1).max(1) as i32;
            let kappa = 2f64.powi(-m); // exact
            // (type 8: under method 4 the sample N(p) is taken all the same, so
            // q0 is κ times the point that sample moved)
            let mut q = mv(p).map(|c| kappa * c);
            let mut prev = at(q);
            q = mv(q);
            let (mut s, mut w, mut total) = (prev, 1.0, 1.0);
            for _ in 1..=n {
                let [x, y, z] = q;
                q = [2.0 * (M4_COS * x - M4_SIN * y), 2.0 * (M4_COS * y + M4_SIN * x), 2.0 * z];
                w *= M4_WEIGHT;
                // : ((P + 0.1)·N)·w, the sum times the
                // sample first; the other groupings are kept for the vectors' check only.
                let sample = noise(q);
                prev = match orders.method4 {
                    Method4Grouping::SumSampleFirst => sample.times(prev + M4_OFFSET) * w,
                    Method4Grouping::SampleWeightFirst => (prev + M4_OFFSET) * sample.times(w),
                    Method4Grouping::SumWeightFirst => sample.times((prev + M4_OFFSET) * w),
                };
                q = mv(q);
                s += prev;
                total += w;
            }
            same(s / total)
        }
        5..=7 => {
            let c = M5_SCALES[(method - 5) as usize];
            let (mut q, mut a, mut w, mut total) = (mv(p), at(p), 1.0, 1.0);
            let [ox, oy] = orders.method5;
            for _ in 1..=n {
                let [x, y, z] = q;
                q = [
                    c * ox.sum([M3_SHIFT[0], M5_A * x, -(M5_B * y)]),
                    c * oy.sum([M3_SHIFT[1], M5_B * x, M5_A * y]),
                    c * (M3_SHIFT[2] + z),
                ];
                let v = noise(q);
                q = mv(q);
                match method {
                    5 => {
                        w *= M5_WEIGHT;
                        a += v.times(w);
                        total += w;
                    }
                    // : A(k−1) when A(k−1) < vk, else vk (a
                    // sample that is not a number takes the kept one's place).
                    6 => {
                        if !v.above(a) {
                            a = v.d();
                        }
                    }
                    _ => a = v.times(a),
                }
            }
            match method {
                5 => same(if n == 0 { a } else { a / total }),
                6 => same(a),
                _ => Octaves { value: a, left: q, exact: None },
            }
        }
        8 => {
            let g = n as f64 + M8_G;
            let d = |a: [f64; 3], b: [f64; 3]| {
                let v = (noise(b).from(at(a)) * g).abs();
                if v >= 1.0 { 1.0 } else { v }
            };
            let (mut pk, mut rk) = (p, p.map(|c| c + M8_OFFSET));
            let mut s = d(pk, rk);
            (pk, rk) = (mv(pk), mv(rk));
            if n <= 0 {
                return Some(Octaves { value: s, left: pk, exact: None });
            }
            let (mut w, mut total) = (1.0, 1.0);
            for _ in 1..=n {
                pk = halved(pk);
                rk = halved(rk);
                w *= M8_WEIGHT;
                s += w * d(pk, rk);
                (pk, rk) = (mv(pk), mv(rk));
                total += w;
            }
            Octaves { value: s / total, left: pk, exact: None }
        }
        9 | 10 => {
            let larger = method == 9;
            // : 9 the new one when the kept one is less than it;
            // 10 the new one unless the kept one is less than it.
            let takes = |kept: f64, new: NoiseSample| if larger { new.above(kept) } else { !new.above(kept) };
            // Mj(q): the kept value (rounded), and the sample itself where the last offer won.
            // : over type 8 with the 3-D setting the later
            // samples of Mj(q) are made from q as its first sample N(q) left it, and h is p as
            // N(p) left it, halved.
            let stages = |q: [f64; 3], j: i64, one_d: bool| -> (f64, Option<NoiseSample>) {
                let mut m = at(q);
                let mut last = None;
                let [q1, q2, q3] = mv(q);
                if j >= 1 {
                    let s = noise([-q3, q2, q1]);
                    if takes(m, s) {
                        m = s.d();
                    }
                }
                if j >= 2 {
                    let s = noise(if one_d { [-q2, q1, q3] } else { [q1, q3, q2] });
                    if takes(m, s) {
                        m = s.d();
                        last = Some(s);
                    }
                }
                (m, last)
            };
            if n == 0 {
                itself(noise(p))
            } else if n <= 2 {
                same(stages(p, n, one_d).0)
            } else {
                let j = (n - 3).min(2);
                let m2 = stages(p, 2, one_d).0;
                let (mh, last) = stages(halved(mv(p)), j, one_d);
                // (type 12: with a count of 5 or more a sample that wins the last
                // offer to the second best enters the sum with M2(p) unrounded)
                let sum = match last {
                    Some(s) if n >= 5 => s.plus(m2),
                    _ => m2 + mh,
                };
                same(sum * 0.5)
            }
        }
        12 => {
            let (mut pk, mut b) = (mv(p), at(p));
            for _ in 1..=n {
                pk = pk.map(|c| M12_SCALE * c + M12_SHIFT);
                let v = noise(pk);
                pk = mv(pk);
                if v.above(b) {
                    b = v.d();
                }
            }
            Octaves { value: b, left: [p[0], pk[1], pk[2]], exact: None }
        }
        _ => return None,
    })
}

