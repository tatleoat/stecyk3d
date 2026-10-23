// SPDX-License-Identifier: MIT
//! The noise types that read `perm` or `waves`:'s lattice value noise (types 4, 5, 6,
//! 26) and's types built on it (7 to 16, 18, 20 to 24) and type 17, as functions of
//! the user's tables; and's direction of method 11 (two samples of type 6 on warped
//! copies of the point).
//!
//! ; (points 1 and 2: the orders the entries leave Open are
//! the implementer's choice)
//! (The types that read a table)

use crate::numerics::extended::ExtValue;
use crate::numerics::{sqrt_as, to_i32, x87};
use crate::version::RenderAs;

use super::filters::exp_x87;
use super::noise::triangle;
use super::orders::Smoothstep;

/// The scale c of the lattice values (not 1/255).
const C: f64 = 0.003921568627;
/// Type 4's g.
#[allow(clippy::approx_constant)]
const CELL_G: f64 = 0.31830988618;
/// The turn by 60 degrees's types and's method 11 use.
const TURN: f64 = 0.866025;
/// The second turned copy's factor.,
const TURN2: f64 = 0.17675;
/// Type 7's k.
const T7_K: f64 = 0.03137254902;
/// Type 8's 1-D factor and its 3-D constants.
const T8_ONE: f64 = 0.00130718954248366;
const T8_LIFT: f64 = 40.0;
const T8_SWIRL: f64 = 0.2;
const T8_BASE: f64 = 2.0;
/// Type 8's swirl layers (n, g).
const T8_ONE_LAYERS: [(i32, f64); 4] = [(80, 8.0), (170, 6.0), (280, 7.0), (120, -9.0)];
const T8_TWO_LAYERS: [(i32, f64); 5] = [(80, 8.0), (170, 6.0), (280, 7.0), (120, -9.0), (320, 12.0)];
const T8_THREE_LAYERS: [(i32, f64); 3] = [(80, 8.0), (170, 6.0), (120, 9.0)];
/// Type 9's weights.
const T9_W: [f64; 4] = [0.06666666667, 0.13333333333, 0.26666666667, 0.53333333333];
const T9_C: f64 = 0.433;
/// Type 10's and method 11's weights, the warp's frequency and amplitudes.
/// (type 10),
const W_ONE: f64 = 0.33333333335;
const W_TWO: f64 = 0.66666666667;
const WARP_F: f64 = 1.2;
const WARP_M: [f64; 2] = [2.0, 3.0];
const WARP_E: f64 = 0.001953125;
const WARP_T: f64 = 125.37;
const WARP_S: f64 = 205.55;
/// Type 11's constants.
const T11_SIDE: i32 = 20;
const T11_SCALE: f64 = 2.5;
const T11_FALL: f64 = 0.95;
const T11_SHIFT: [f64; 3] = [210.31, -210.35, 210.37];
/// Type 12's thresholds.
const T12_THRESHOLDS: [f64; 3] = [0.1, 0.6, 0.9];
/// Type 13's third.
const T13_THIRD: f64 = 0.3333333333;
/// Types 14 and 16's scale.
const LINE_SCALE: f64 = 0.01176470586;
/// Type 15's constants.
const T15_W: f64 = 0.4;
const T15_GAIN: f64 = 2.3;
/// Types 15 and 20's second weight and shift.
const BAND_W: f64 = 1.5;
const BAND_SHIFT: f64 = -0.5;
/// Type 17's first frequency and amplitude and their steps.
const T17_PHI: f64 = 0.7;
const T17_PHI_STEP: f64 = 1.4285714285714286;
const T17_A: f64 = 0.2;
const T17_A_STEP: f64 = 0.7;
/// Type 18's constants.
const T18_C: f64 = 0.433018;
const T18_GAIN: f64 = 12.0;
/// Type 20's first weight.
const T20_W: f64 = 0.5;
/// Type 21's constants.
const T21_LOW: f64 = 0.2;
const T21_A: f64 = 8.0;
const T21_B: f64 = 7.0;
const T21_LIFT: f64 = 12.7;
const T21_STEP: f64 = 2.5;
const T21_TOP: f64 = 255.0;
const T21_SCALE: f64 = 0.01568627451;
const T21_FINE: f64 = 0.75;
/// Type 22's constants.
const T22_A: f64 = 77.33;
const T22_A_F: f64 = 2.4;
const T22_W: f64 = 177.66;
const T22_W_F: f64 = 0.15;
const T22_LOW: f64 = 0.0333;
const T22_SHIFT: f64 = 123.15;
const T22_WEIGHT: f64 = 0.33;
/// Type 23's constants.
#[allow(clippy::approx_constant)]
const TURN_P: f64 = 6.2831853072;
const T23_F1: f64 = 0.125;
const T23_F2: f64 = 0.06;
const T23_SPOT: f64 = 0.00390625;
const T23_HEIGHT: f64 = 0.001953125;
const T23_MUL: [i32; 2] = [42, 107];
const T23_SHIFTS: [[f64; 3]; 3] = [[9.0, 120.0, 332.0], [822.5, 821.5, 441.0], [321.5, 555.5, 842.5]];
/// Type 24's constants.
const T24_LEVELS: i32 = 3;
const T24_ABOVE: f64 = 128.0;
const T24_START: [f64; 3] = [0.5, 0.5, 0.875];
/// Method 11's reach.
const M11_REACH: f64 = 12.0;

/// One sample of a type that reads a table: its value and, for type 8 with the 3-D setting,
/// the point it moved (its second coordinate y2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub value: f64,
    pub moved: Option<[f64; 3]>,
    /// Type 12 with the 1-D setting: the extended value itself (`value` is it rounded).
    pub exact: Option<ExtValue>,
}

/// Why a sample is not computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotComputed {
    /// The type needs a table the substrate lacks .
    NoTable,
    /// Not a type of this module.
    NotHere,
}

/// The tables and the settings the types of this module read.
#[derive(Clone, Copy, Debug)]
pub struct Lattice<'a> {
    pub perm: Option<&'a [i32; 256]>,
    pub waves: Option<[[f64; 3]; 6]>,
    pub version: RenderAs,
    /// : "s the smoothstep of, in the same form".
    pub smooth: Smoothstep,
}

/// The point a sample leaves where it moves the point it is taken at: type 8 with the 3-D
/// setting puts y2 = ⟨⟨y + 40⟩ − 40⟩ in its second coordinate.
pub fn moved_point(type_number: i64, d: u8, [x, y, z]: [f64; 3]) -> Option<[f64; 3]> {
    (type_number == 8 && d == 3).then_some([x, (y + T8_LIFT) - T8_LIFT, z])
}

/// Whether a type is one of this module's ('s types that read a table).
pub fn reads_a_table(type_number: i64) -> bool {
    matches!(type_number, 4..=18 | 20..=24 | 26)
}

/// The cell ⌊u⌋ of a coordinate: T(u), less 1 when u < 0.
fn cell(u: f64) -> i32 {
    let i = to_i32(u);
    if u < 0.0 { i.wrapping_sub(1) } else { i }
}

/// C(t): 0 below 0 or not a number, 1 above 1.
fn cap(t: f64) -> f64 {
    if t > 1.0 {
        1.0
    } else if t >= 0.0 {
        t
    } else {
        0.0
    }
}

/// ν(f), the distance of a fraction to the nearer whole number.
fn nearer(f: f64) -> f64 {
    if f > 0.5 { 1.0 - f } else { f }
}

/// The smaller of a running value and a new one (the first is kept unless the new one is
/// below it). (the smallest and largest of a list)
fn least(m: f64, v: f64) -> f64 {
    if v < m { v } else { m }
}

fn most(m: f64, v: f64) -> f64 {
    if v > m { v } else { m }
}

/// FSIN's and FCOS's extended values, `None` where the argument is not finite (the result is
/// then not a number).
fn sin_x(u: f64) -> Option<ExtValue> {
    u.is_finite().then(|| x87::sin_ext(u))
}

fn cos_x(u: f64) -> Option<ExtValue> {
    u.is_finite().then(|| x87::cos_ext(u))
}

/// ⟨S(u)·d⟩.
fn sin_times(u: f64, d: f64) -> f64 {
    sin_x(u).map_or(f64::NAN, |s| s.mul_to_f64(d))
}

/// ⟨Ĉ(u)⟩ and ⟨S(u)⟩, rounded once.
fn cos_d(u: f64) -> f64 {
    cos_x(u).map_or(f64::NAN, |c| c.to_f64())
}

impl<'a> Lattice<'a> {
    fn s(&self, t: f64) -> f64 {
        self.smooth.eval(t)
    }

    fn p(perm: &[i32; 256], m: i32) -> i32 {
        perm[(m & 0xff) as usize]
    }

    /// V(i), V(i, j), V(i, j, k): the lattice values (0 to 255).
    fn v1(perm: &[i32; 256], i: i32) -> i32 {
        Self::p(perm, Self::p(perm, i).wrapping_add(i >> 8))
    }

    fn v2(perm: &[i32; 256], i: i32, j: i32) -> i32 {
        Self::p(perm, Self::p(perm, i).wrapping_add(i >> 8).wrapping_add(j))
    }

    fn v3(perm: &[i32; 256], i: i32, j: i32, k: i32) -> i32 {
        let inner = Self::p(perm, Self::p(perm, i).wrapping_add(i >> 8).wrapping_add(j));
        Self::p(perm, inner.wrapping_add(j >> 8).wrapping_add(k))
    }

    /// V at the cells of the setting's coordinates.
    fn v_at(perm: &[i32; 256], d: u8, c: [i32; 3]) -> i32 {
        match d {
            1 => Self::v1(perm, c[0]),
            2 => Self::v2(perm, c[0], c[1]),
            _ => Self::v3(perm, c[0], c[1], c[2]),
        }
    }

    /// The blend low + (high − low)·w.
    fn blend(low: f64, high: f64, w: f64) -> f64 {
        low + (high - low) * w
    }

    /// Types 5, 6 and 26, not scaled by c: the values blended along x, then y, then z, with
    /// the weights f (`smooth` false) or s(f).
    fn blended(&self, perm: &[i32; 256], d: u8, q: [f64; 3], smooth: bool) -> f64 {
        let w = |f: f64| if smooth { self.s(f) } else { f };
        let (i, j, k) = (cell(q[0]), cell(q[1]), cell(q[2]));
        let wx = w(q[0] - f64::from(i));
        let i1 = i.wrapping_add(1);
        match d {
            1 => Self::blend(f64::from(Self::v1(perm, i)), f64::from(Self::v1(perm, i1)), wx),
            2 => {
                let wy = w(q[1] - f64::from(j));
                let j1 = j.wrapping_add(1);
                let row = |j| Self::blend(f64::from(Self::v2(perm, i, j)), f64::from(Self::v2(perm, i1, j)), wx);
                Self::blend(row(j), row(j1), wy)
            }
            _ => {
                let wy = w(q[1] - f64::from(j));
                let wz = w(q[2] - f64::from(k));
                let (j1, k1) = (j.wrapping_add(1), k.wrapping_add(1));
                let row = |j, k| Self::blend(f64::from(Self::v3(perm, i, j, k)), f64::from(Self::v3(perm, i1, j, k)), wx);
                let face = |k| Self::blend(row(j, k), row(j1, k), wy);
                Self::blend(face(k), face(k1), wz)
            }
        }
    }

    /// Type 6 (= 26) at q with the setting d: L, L2, L3.
    fn l(&self, perm: &[i32; 256], d: u8, q: [f64; 3]) -> f64 {
        self.blended(perm, d, q, true) * C
    }

    /// Type 4 at q with the setting d: Z (1-D) and Z'.
    fn z(perm: &[i32; 256], d: u8, q: [f64; 3]) -> f64 {
        let i = to_i32(q[0] * CELL_G);
        let v = match d {
            1 => Self::v1(perm, i),
            2 => {
                let j = to_i32(q[1] * -CELL_G);
                Self::p(perm, Self::p(perm, i).wrapping_sub(j).wrapping_add(i >> 8))
            }
            _ => {
                let j = to_i32(q[1] * CELL_G);
                let k = to_i32(q[2] * -CELL_G);
                let inner = Self::p(perm, Self::p(perm, i).wrapping_add(i >> 8).wrapping_add(j));
                Self::p(perm, inner.wrapping_sub(k).wrapping_add(j >> 8))
            }
        };
        f64::from(v) * C
    }

    /// σ(u), and σ4, σ8 (`every` 4 or 8: a value whose i is not a multiple of it taken as 0).
    fn sigma(&self, perm: &[i32; 256], u: f64, every: i32) -> f64 {
        let i = to_i32(u);
        let f = u - f64::from(i);
        let at = |i: i32| if i % every == 0 { f64::from(Self::v1(perm, i)) } else { 0.0 };
        Self::blend(at(i), at(i.wrapping_add(1)), self.s(f))
    }

    /// λn(u), the place of u in its cell of side n.
    fn place(u: f64, n: i32) -> f64 {
        let nf = f64::from(n);
        if u > 0.0 {
            u - f64::from(n.wrapping_mul(cell(u / nf)))
        } else {
            let m = -u;
            nf - (m - f64::from(n.wrapping_mul(cell(m / nf))))
        }
    }

    /// r(v):'s square root, 3.0's own as 3.0.
    fn r(&self, v: f64) -> f64 {
        sqrt_as(self.version, v)
    }

    /// 's warp Ω with amplitude m acting on (s, t).
    fn warp(&self, perm: &[i32; 256], m: f64, (s, t): (f64, f64)) -> (f64, f64) {
        let e = m * WARP_E;
        let v = |a: f64, b: f64| self.blended(perm, 2, [a, b, 0.0], true);
        let s = v(s * WARP_F, t * WARP_F) * e + s;
        let t = v((t + WARP_T) * WARP_F, (s + WARP_S) * WARP_F) * e + t;
        (s, t)
    }

    /// The two warped samples of type 10 and method 11: L(a, b, z1)·0.33333333335 and
    /// L(a', b', z2)·0.66666666667 with (a, b) and (a', b') the turned copies of (x, y), with
    /// the setting d.,
    fn warped_pair(&self, perm: &[i32; 256], d: u8, x: f64, y: f64, z1: f64, z2: f64) -> f64 {
        let (a, b) = self.warp(perm, WARP_M[0], (0.5 * x + TURN * y, 0.5 * y - TURN * x));
        // Ω acts on (b', a') in that order.
        let (b2, a2) = self.warp(perm, WARP_M[1], (TURN2 * x - TURN2 * y, -TURN2 * x - TURN2 * y));
        self.l(perm, d, [a, b, z1]) * W_ONE + self.l(perm, d, [a2, b2, z2]) * W_TWO
    }

    /// Method 11's moved point q0, from two samples of type 6 with the 3-D setting.
    pub fn method11_start(&self, p: [f64; 3]) -> Option<[f64; 3]> {
        let perm = self.perm?;
        let [x, y, z] = p;
        let (a, b, z1) = (x * 0.25, y * 0.25, z * 0.25);
        let (u, v) = self.warp(perm, WARP_M[0], (0.5 * a + TURN * b, 0.5 * b - TURN * a));
        let big_a = self.l(perm, 3, [u, v, z1]) * W_ONE;
        let (v2, u2) = self.warp(perm, WARP_M[1], (TURN2 * a - TURN2 * b, -TURN2 * a - TURN2 * b));
        let theta = (big_a + self.l(perm, 3, [u2, v2, z1 * 0.25]) * W_TWO) * TURN_P;
        let c12 = cos_x(theta).map_or(f64::NAN, |c| c.mul_to_f64(M11_REACH));
        let s12 = sin_times(theta, M11_REACH);
        Some([(c12 + x) * 0.5, (s12 + y) * 0.5, z * 0.5])
    }

    /// Type 9 with the setting d.
    fn type9(&self, perm: &[i32; 256], d: u8, [x, y, z]: [f64; 3]) -> f64 {
        let l = |q| self.l(perm, d, q);
        l([2.0 * x, 2.0 * y, 2.0 * z]) * T9_W[0]
            + l([0.5 * x + TURN * y, 0.5 * y - TURN * x, z]) * T9_W[1]
            + l([T9_C * y - 0.25 * x, -0.25 * y - T9_C * x, 0.5 * z]) * T9_W[2]
            + l([-TURN2 * x - TURN2 * y, TURN2 * x - TURN2 * y, 0.25 * z]) * T9_W[3]
    }

    /// A swirl layer (n, g) at (a, b): its displacement (δ, δ').
    fn swirl(&self, perm: &[i32; 256], (n, g): (i32, f64), a: f64, b: f64) -> (f64, f64) {
        let nf = f64::from(n);
        let (h, h4) = (n / 2, n / 4);
        let e = Self::v2(perm, cell(a / nf), cell(b / nf));
        let alpha = e % h + h4;
        let beta = Self::p(perm, e.wrapping_add(1)) % h + h4;
        let da = Self::place(a, n) - f64::from(alpha);
        let db = Self::place(b, n) - f64::from(beta);
        let (ha, hb) = (h - alpha, h - beta);
        let big_r = f64::from(to_i32(f64::from(h) - self.r(f64::from(ha * ha + hb * hb))));
        let d = self.r(da * da + db * db);
        if d > big_r {
            return (0.0, 0.0);
        }
        let t = (big_r - d) / big_r;
        let w = t * t;
        let theta = (w * w) * g;
        let gamma = cos_d(theta);
        let dx = (gamma * da - sin_times(theta, db)) - da;
        let dy = (sin_times(theta, da) + gamma * db) - db;
        (dx, dy)
    }

    /// Δ and Δ' over the layers at (a, b), each layer adding its δ to Δ (δ + Δ) and δ' to Δ';
    /// `start` the sums so far (none: the first layer's own).
    fn swirls(&self, perm: &[i32; 256], layers: &[(i32, f64)], a: f64, b: f64, start: (Option<f64>, Option<f64>)) -> (Option<f64>, Option<f64>) {
        layers.iter().fold(start, |(sx, sy), &layer| {
            let (dx, dy) = self.swirl(perm, layer, a, b);
            (Some(sx.map_or(dx, |s| dx + s)), Some(sy.map_or(dy, |s| dy + s)))
        })
    }

    /// Type 11's layer value at (u, v, w).
    fn blob(&self, perm: &[i32; 256], d: u8, [u, v, w]: [f64; 3]) -> f64 {
        let side = f64::from(T11_SIDE);
        let off = |e: i32| f64::from(e % 10 + 5);
        let dist = match d {
            1 => {
                let e = Self::v1(perm, cell(u / side));
                (Self::place(u, T11_SIDE) - off(e)).abs()
            }
            2 => {
                let e = Self::v2(perm, cell(u / side), cell(v / side));
                let du = Self::place(u, T11_SIDE) - off(e);
                let dv = Self::place(v, T11_SIDE) - off(Self::p(perm, e.wrapping_add(1)));
                self.r(du * du + dv * dv)
            }
            _ => {
                let kw = cell(w / side);
                let e = Self::v3(perm, cell(u / side), cell(v / side), kw);
                let e2 = Self::p(perm, kw.wrapping_add(e).wrapping_add(1));
                let du = Self::place(u, T11_SIDE) - off(e);
                let dv = Self::place(v, T11_SIDE) - off(e2);
                let dw = Self::place(w, T11_SIDE) - off(Self::p(perm, e2.wrapping_add(1)));
                let (a, b, c) = (du * du, dv * dv, dw * dw);
                let sum = match self.version {
                    RenderAs::Bryce50 => (b + c) + a,
                    _ => (a + b) + c,
                };
                self.r(sum)
            }
        };
        exp_x87(-(dist * T11_FALL))
    }

    /// Type 23's swirl W.
    fn swirl23(&self, perm: &[i32; 256], [x, y, z]: [f64; 3]) -> [f64; 3] {
        let t1 = self.l(perm, 3, [T23_F1 * x, T23_F1 * y, T23_F1 * z]) * TURN_P;
        let x1 = cos_x(t1).map_or(f64::NAN, |c| c.add_to_f64(x));
        let y1 = sin_x(t1).map_or(f64::NAN, |s| s.negated().add_to_f64(y));
        let t2 = self.l(perm, 3, [T23_F2 * x1, T23_F2 * y1, T23_F2 * z]) * TURN_P;
        let x2 = cos_x(t2).map_or(f64::NAN, |c| c.add_to_f64(x1));
        let y2 = sin_x(t2).map_or(f64::NAN, |s| s.negated().add_to_f64(y1));
        [x2, y2, z]
    }

    /// Type 23's spot G.
    fn spot(&self, perm: &[i32; 256], d: u8, [x, y, z]: [f64; 3]) -> f64 {
        let (cx, cy, cz) = (cell(0.5 * x), cell(0.5 * y), cell(0.5 * z));
        let pt = match d {
            1 => [x - f64::from(Self::v1(perm, cx)) * T23_SPOT, y, z],
            2 => {
                let j = f64::from(Self::p(perm, T23_MUL[0].wrapping_mul(cx ^ cy))) * T23_SPOT;
                [x - j, y - j, z]
            }
            _ => {
                let m = T23_MUL[0].wrapping_mul(cx ^ cy).wrapping_sub(T23_MUL[1].wrapping_mul(cz ^ cx));
                let j = f64::from(Self::p(perm, m)) * T23_SPOT;
                [x - j, y - j, z - j]
            }
        };
        let n = d as usize;
        let cells = [cell(pt[0]), cell(pt[1]), cell(pt[2])];
        if cells[..n].iter().any(|c| c % 2 != 0) {
            return 0.0;
        }
        let h = 0.5 + T23_HEIGHT * f64::from(Self::v_at(perm, d, cells));
        let psi = |u: f64| {
            let a = u + 0.5;
            nearer(a - f64::from(cell(a)))
        };
        let sq = |u: f64| {
            let p = psi(u);
            p * p
        };
        let sum = match d {
            1 => sq(pt[0]),
            2 => sq(pt[0]) + sq(pt[1]),
            _ => match self.version {
                RenderAs::Bryce50 => (sq(pt[2]) + sq(pt[1])) + sq(pt[0]),
                _ => (sq(pt[1]) + sq(pt[0])) + sq(pt[2]),
            },
        };
        let rho = 4.0 * sum;
        if !(rho < 1.0) {
            return 0.0;
        }
        let mut v = (1.0 - rho).sqrt();
        if v < 0.5 {
            v = (v + 0.5) * v;
        }
        v * h
    }

    /// Type 24's K.
    fn grid(perm: &[i32; 256], d: u8, q: [f64; 3], level: i32) -> f64 {
        let cells = q.map(cell);
        if level < T24_LEVELS && f64::from(Self::v_at(perm, d, cells)) > T24_ABOVE {
            return Self::grid(perm, d, q.map(|c| 2.0 * c), level + 1);
        }
        let n = d as usize;
        (0..n).map(|a| nearer(q[a] - f64::from(cells[a]))).reduce(least).unwrap_or(f64::NAN)
    }

    /// The sample of a type of this module at q with the setting d (1, 2 or 3).
    pub fn sample(&self, type_number: i64, d: u8, q: [f64; 3]) -> Result<Sample, NotComputed> {
        if !reads_a_table(type_number) {
            return Err(NotComputed::NotHere);
        }
        let plain = |value: f64| Ok(Sample { value, moved: None, exact: None });
        if type_number == 17 {
            let waves = self.waves.ok_or(NotComputed::NoTable)?;
            return plain(self.type17(&waves, d, q));
        }
        let perm = self.perm.ok_or(NotComputed::NoTable)?;
        let [x, y, z] = q;
        let l = |p: [f64; 3]| self.l(perm, d, p);
        let sg = |u: f64| self.sigma(perm, u, 1);
        match type_number {
            4 => plain(Self::z(perm, d, q)),
            5 => plain(self.blended(perm, d, q, false) * C),
            6 | 26 => plain(l(q)),
            7 => {
                let v = match d {
                    1 => x + T7_K * sg(x),
                    2 => (x + T7_K * (sg(y) + sg(x))) + y,
                    _ => {
                        let sum = (sg(z) + sg(y)) + sg(x);
                        match self.version {
                            RenderAs::Bryce31 => ((z + T7_K * sum) + x) + y,
                            _ => ((x + T7_K * sum) + y) + z,
                        }
                    }
                };
                let t = triangle(v);
                // Below 0 taken as 0, above 1 as 1 (not a number stays).
                plain(t.clamp(0.0, 1.0))
            }
            8 => Ok(self.type8(perm, d, q)),
            9 => plain(self.type9(perm, d, q)),
            10 => plain(self.warped_pair(perm, d, x, y, z, 0.25 * z)),
            11 => {
                let u = q.map(|c| T11_SCALE * c);
                let shifted = [u[0] + T11_SHIFT[0], u[1] + T11_SHIFT[1], u[2] + T11_SHIFT[2]];
                let s = self.blob(perm, d, u) + self.blob(perm, d, shifted);
                plain(if s > 1.0 { 1.0 } else { s })
            }
            12 => {
                let mut p = q.map(|c| (c + 1.0) * 0.5);
                for t in T12_THRESHOLDS {
                    if Self::z(perm, d, p) > t {
                        p = p.map(|c| c * 2.0);
                    } else {
                        break;
                    }
                }
                let [u, v, w] = p;
                let product = |a: f64, b: f64, c: f64| -> f64 {
                    match (sin_x(a), sin_x(b), sin_x(c)) {
                        (Some(a), Some(b), Some(c)) => a.mul_to_f64(b.mul_ext_to_f64(c)),
                        _ => f64::NAN,
                    }
                };
                let value = match d {
                    1 => {
                        return Ok(match sin_x(u) {
                            Some(s) => {
                                let e = s.magnitude();
                                Sample { value: e.to_f64(), moved: None, exact: Some(e) }
                            }
                            None => Sample { value: f64::NAN, moved: None, exact: None },
                        });
                    }
                    2 => match (sin_x(u), sin_x(v)) {
                        (Some(a), Some(b)) => a.mul_ext_to_f64(b),
                        _ => f64::NAN,
                    },
                    // The first factor is the one multiplied by the inner product.
                    _ => match self.version {
                        RenderAs::Bryce30 => product(v, u, w),
                        RenderAs::Bryce31 => product(w, v, u),
                        RenderAs::Bryce50 => product(u, w, v),
                    },
                };
                plain(value.abs())
            }
            13 => plain(match d {
                1 => Self::z(perm, 1, [x, 0.0, 0.0]),
                _ => {
                    let zz = |u: f64| Self::z(perm, 1, [u, 0.0, 0.0]);
                    (zz(x) + zz(0.5 * x + TURN * y) + zz(0.5 * x - TURN * y)) * T13_THIRD
                }
            }),
            14 => {
                let m = match d {
                    1 => sg(x),
                    2 => least(sg(x), sg(y)),
                    _ => least(least(sg(x), sg(y)), sg(z)),
                };
                let t = m * LINE_SCALE;
                plain(1.0 - exp_x87(-(t * t)))
            }
            15 => {
                let g = (l(q) * T15_W + l(q.map(|c| 0.25 * c)) * BAND_W) + BAND_SHIFT;
                let t = self.s(self.s(self.s(cap(g))));
                plain(self.s(self.s(cap(((t - 0.5) * T15_GAIN).abs()))))
            }
            16 => {
                let mut m = sg(x);
                if d >= 2 {
                    m = least(m, sg(0.5 * x + TURN * y));
                    m = least(m, sg(0.5 * x - TURN * y));
                }
                if d == 3 {
                    m = least(m, sg(z));
                    m = least(m, sg(0.5 * z - TURN * y));
                    m = least(m, sg(0.5 * z + TURN * y));
                }
                let t = m * LINE_SCALE;
                plain(1.0 - exp_x87(-(t * t)))
            }
            18 => {
                let t = (l([-0.25 * x + T18_C * y, -0.25 * y - T18_C * x, z]) + l(q)) * T18_GAIN;
                plain(cap(sg(t) * C))
            }
            20 => {
                let g = (l(q.map(|c| 0.25 * c)) * BAND_W + l(q) * T20_W) + BAND_SHIFT;
                plain(self.s(self.s(self.s(cap(g)))))
            }
            21 => plain(self.type21(perm, d, q)),
            22 => plain(self.type22(perm, d, q)),
            23 => {
                let g = |shift: [f64; 3]| {
                    let p = [x + shift[0], y + shift[1], z + shift[2]];
                    self.spot(perm, d, self.swirl23(perm, p))
                };
                let mut m = most(g([0.0; 3]), g(T23_SHIFTS[0]));
                if d == 3 {
                    m = most(m, g(T23_SHIFTS[1]));
                    m = most(m, g(T23_SHIFTS[2]));
                }
                plain(m)
            }
            24 => {
                let start = [T24_START[0] + 0.25 * x, T24_START[1] + 0.25 * y, T24_START[2] + 0.25 * z];
                plain(2.0 * Self::grid(perm, d, start, 1))
            }
            _ => Err(NotComputed::NotHere),
        }
    }

    /// Type 8.
    fn type8(&self, perm: &[i32; 256], d: u8, [x, y, z]: [f64; 3]) -> Sample {
        match d {
            1 => {
                let (_, dy) = self.swirls(perm, &T8_ONE_LAYERS, x, y, (None, None));
                let u = y + dy.unwrap_or(0.0);
                let value = (self.sigma(perm, 0.5 * u, 1) + 2.0 * self.sigma(perm, 0.25 * u, 1)) * T8_ONE;
                Sample { value, moved: None, exact: None }
            }
            2 => {
                let (dx, dy) = self.swirls(perm, &T8_TWO_LAYERS, x, y, (None, None));
                let p = [x + dx.unwrap_or(0.0), y + dy.unwrap_or(0.0), z];
                Sample { value: self.type9(perm, 2, p), moved: None, exact: None }
            }
            _ => {
                let theta0 = self.r(x * x + z * z) * T8_SWIRL + T8_BASE;
                let gamma0 = cos_d(theta0);
                let y1 = y + T8_LIFT;
                let y2 = y1 - T8_LIFT;
                let first = (gamma0 * x - sin_times(theta0, z)) * T8_SWIRL;
                let s = self.swirls(perm, &T8_THREE_LAYERS, z, y1, (Some(first), None));
                let (dx, dy) = self.swirls(perm, &T8_TWO_LAYERS, x, y2, s);
                let dz = (sin_times(theta0, x) + gamma0 * z) * T8_SWIRL;
                let p = [x + dx.unwrap_or(0.0), y2 + dy.unwrap_or(0.0), z + dz];
                debug_assert_eq!(moved_point(8, 3, [x, y, z]), Some([x, y2, z]));
                Sample { value: self.type9(perm, 3, p), moved: Some([x, y2, z]), exact: None }
            }
        }
    }

    /// Type 17 over the six centres of `waves`.
    fn type17(&self, waves: &[[f64; 3]; 6], d: u8, q: [f64; 3]) -> f64 {
        let (mut phi, mut a) = (T17_PHI, T17_A);
        let mut sum: Option<f64> = None;
        for c in waves {
            let sq = |k: usize| {
                let t = q[k] - c[k];
                t * t
            };
            // The order of the three squares in 3-D is Open and differs between 3.0, 3.1 and
            // 5.0 . Chosen against the table cases' results : as
            // written for 3.0 and 3.1 (every type 17 3-D case passed with it, feedback on
            // acc1a01), and (dy² + dz²) + dx² for 5.0 (with the written order 2 of its 4 cases
            // failed; this is type 11's 5.0 form, to be checked by waves-orders.jsonl).
            let s2 = match (d, self.version) {
                (1, _) => sq(0),
                (2, _) => sq(0) + sq(1),
                (_, RenderAs::Bryce50) => (sq(1) + sq(2)) + sq(0),
                _ => (sq(0) + sq(1)) + sq(2),
            };
            let dk = s2.sqrt();
            let term = sin_x(dk * phi).map_or(f64::NAN, |s| s.add_to_f64(1.0)) * a;
            sum = Some(sum.map_or(term, |s| s + term));
            phi *= T17_PHI_STEP;
            a *= T17_A_STEP;
        }
        let s = sum.unwrap_or(0.0);
        if s > 1.0 { 1.0 } else { s }
    }

    /// Type 21.
    fn type21(&self, perm: &[i32; 256], d: u8, [x, y, z]: [f64; 3]) -> f64 {
        let l2 = |p| self.l(perm, 2, p);
        let l3 = |p| self.l(perm, 3, p);
        let s4 = |u| self.sigma(perm, u, 4);
        let s8 = |u| self.sigma(perm, u, 8);
        let b0 = l2([0.5 * x, 0.5 * z, 0.0]);
        let (xl, yl, zl) = ((0.5 * x) * T21_LOW, (0.5 * y) * T21_LOW, (0.5 * z) * T21_LOW);
        let a = b0 + l3([zl, xl, yl]) * T21_A;
        let b = b0 + l3([xl, zl, yl + T21_LIFT]) * T21_B;
        let mut m = most(s4(b + 0.5 * y), s8((a + 0.5 * y) + T21_STEP));
        if d >= 2 {
            m = most(m, s4(0.5 * x + b));
        }
        if d == 3 {
            m = most(m, s4(0.5 * z + a));
        }
        let t = (T21_TOP - m) * T21_SCALE;
        let v = ((1.0 - exp_x87(-(t * t))) + -l3([x, y, z]) * T21_LOW)
            + -((s4((a + x) * T21_LOW) * C) * l3([T21_FINE * x, T21_FINE * z, T21_FINE * y]));
        if v > 0.0 { v } else { 0.0 }
    }

    /// Type 22.
    fn type22(&self, perm: &[i32; 256], d: u8, [x, y, z]: [f64; 3]) -> f64 {
        let l = |p| self.l(perm, d, p);
        if d == 1 {
            let h = 0.5 * x;
            return Self::blend(l([x, 0.0, 0.0]), l([T22_A + T22_A_F * h, 0.0, 0.0]), l([T22_W + T22_W_F * h, 0.0, 0.0]));
        }
        let (a, b, cz) = ((0.5 * x + TURN * y) * T22_LOW, (0.5 * y - TURN * x) * T22_LOW, T22_LOW * z);
        let f1 = (l([a, b, cz]) + 1.0) * 0.5;
        let f2 = (l([a + T22_SHIFT, b + T22_SHIFT, cz + T22_SHIFT]) + 2.0) * 0.5;
        let low = l([f2 * x, f2 * y, f2 * z]);
        let high = l([(0.5 * x - TURN * y) * f1, (TURN * x + 0.5 * y) * f1, z * f1]);
        let w = l([(0.5 * x + TURN * y) * T22_WEIGHT, (0.5 * x - TURN * y) * T22_WEIGHT, T22_WEIGHT * z]);
        Self::blend(low, high, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_and_places() {
        // : a negative whole number lies in the cell below it, at the fraction 1.
        assert_eq!(cell(-3.0), -4);
        assert_eq!(cell(2.5), 2);
        assert_eq!(cell(-0.5), -1);
        // : λn(u) for u > 0 and otherwise.
        assert_eq!(Lattice::place(45.0, 20), 5.0);
        assert_eq!(Lattice::place(-5.0, 20), 15.0);
        assert_eq!(Lattice::place(0.0, 20), 20.0);
        assert_eq!(nearer(0.75), 0.25);
        assert_eq!(nearer(0.5), 0.5);
    }

    #[test]
    fn lattice_values_hash_through_perm() {
        let mut perm = [0i32; 256];
        for (k, e) in perm.iter_mut().enumerate() {
            *e = ((k * 7 + 3) % 256) as i32;
        }
        // : V(i) = P(P(i) + ⌊i/256⌋); ⌊−1/256⌋ = −1.
        assert_eq!(Lattice::v1(&perm, 1), perm[perm[1] as usize]);
        assert_eq!(Lattice::v1(&perm, -1), perm[((perm[255] - 1) & 255) as usize]);
        // A lattice point's type 5 and 6 values are its own value times c.
        let lat = Lattice { perm: Some(&perm), waves: None, version: RenderAs::Bryce30, smooth: Smoothstep::Nested };
        let at = lat.sample(5, 3, [2.0, 3.0, 4.0]).unwrap().value;
        assert_eq!(at, f64::from(Lattice::v3(&perm, 2, 3, 4)) * C);
        assert_eq!(lat.sample(6, 2, [2.0, 3.0, 0.0]), lat.sample(26, 2, [2.0, 3.0, 0.0]));
        assert_eq!(lat.sample(3, 1, [0.0; 3]), Err(NotComputed::NotHere));
        let none = Lattice { perm: None, ..lat };
        assert_eq!(none.sample(9, 3, [0.0; 3]), Err(NotComputed::NoTable));
    }
}
