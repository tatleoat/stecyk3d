// SPDX-License-Identifier: MIT
//! The point in a component's noise space:'s map from two angles and three frequencies.
//!
//! (revised d67ee0c..b2ca09a; vectors:
//! noise-space.jsonl),
//! (Noise space)

use crate::numerics::extended::ExtValue;
use crate::numerics::x87;

use super::orders::{Order3, ThreeFactor};
use super::settings::NoiseSpace;

/// The degrees-to-radians factor, the double nearest 0.01745329252 (not π/180).
const DEGREES: f64 = 0.01745329252;

/// The map's rows, each entry a binary64, and the orders of each row's three terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceMap {
    rows: [[f64; 3]; 3],
    /// The order of the three terms of q1 and of q2 (q3's first term is a zero).
    terms: [Order3; 2],
}

/// The orders leaves to the two factors of a three-factor entry are
/// multiplied first, and the order of q1's and q2's three terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpaceOrders {
    pub three_factor: ThreeFactor,
    pub terms: [Order3; 2],
}

/// An entry of two factors: the frequency times an extended trig value, rounded once.
fn ext_times(e: ExtValue, s: f64) -> f64 {
    e.mul_to_f64(s)
}

/// An entry of three factors s·e·d (s a frequency, e an extended value, d a double), as two
/// binary64 multiplications in the order `g` says.
fn three(g: ThreeFactor, s: f64, e: ExtValue, d: f64) -> f64 {
    match g {
        // (s·e)·d
        ThreeFactor::FirstPair => e.mul_to_f64(s) * d,
        // s·(e·d)
        ThreeFactor::LastPair => s * e.mul_to_f64(d),
        // (s·d)·e
        ThreeFactor::OuterPair => e.mul_to_f64(s * d),
    }
}

type MapSlot = Option<([u64; 5], SpaceOrders, SpaceMap)>;
thread_local! {
    static MAP_MEMO: std::cell::RefCell<Vec<MapSlot>> = std::cell::RefCell::new(vec![None; 64]);
}

impl SpaceMap {
    /// 's map for angles A, B (degrees) and frequencies sx, sy, sz.
    pub fn new(angles: [f64; 2], frequencies: [f64; 3], orders: SpaceOrders) -> SpaceMap {
        // The original builds a component's map once, when the texture is read (the recomp
        // stores it in the component); here every sample asks again, so the answer is kept per
        // thread, keyed on the exact bits of everything the map is computed from.
        let key = [
            angles[0].to_bits(),
            angles[1].to_bits(),
            frequencies[0].to_bits(),
            frequencies[1].to_bits(),
            frequencies[2].to_bits(),
        ];
        let slot = (key.iter().fold(0u64, |h, k| (h ^ k).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (h >> 31)) >> 58) as usize;
        if let Some(hit) = MAP_MEMO.with(|m| match &m.borrow()[slot] {
            Some((k, o, v)) if *k == key && *o == orders => Some(*v),
            _ => None,
        }) {
            return hit;
        }
        let v = SpaceMap::compute(angles, frequencies, orders);
        MAP_MEMO.with(|m| m.borrow_mut()[slot] = Some((key, orders, v)));
        v
    }

    fn compute(angles: [f64; 2], frequencies: [f64; 3], orders: SpaceOrders) -> SpaceMap {
        let alpha = angles[0] * DEGREES;
        let beta = angles[1] * DEGREES;
        let (ca, sa) = (x87::cos_ext(alpha), x87::sin_ext(alpha));
        let (cb, sb) = (x87::cos_f64(beta), x87::sin_f64(beta));
        let [sx, sy, sz] = frequencies;
        let g = orders.three_factor;
        let rows = [
            [ext_times(ca, sx), three(g, sx, sa, cb), -three(g, sx, sa, sb)],
            [-ext_times(sa, sy), three(g, sy, ca, cb), three(g, sy, ca, sb)],
            [0.0, sz * sb, sz * cb],
        ];
        SpaceMap { rows, terms: orders.terms }
    }

    /// A component's noise space.
    pub fn of(space: &NoiseSpace, orders: SpaceOrders) -> SpaceMap {
        SpaceMap::new(space.angles, space.frequencies, orders)
    }

    /// The map with every entry multiplied by f (each product rounded once). (M'
    /// is 0.002777777778 times's map)
    pub fn scaled(&self, f: f64) -> SpaceMap {
        SpaceMap { rows: self.rows.map(|r| r.map(|e| e * f)), terms: self.terms }
    }

    /// Column j of M (the j-th entry of each row).
    pub fn column(&self, j: usize) -> [f64; 3] {
        [self.rows[0][j], self.rows[1][j], self.rows[2][j]]
    }

    /// q = M·p: each coordinate three binary64 products and two binary64 sums.
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let products = |r: usize| [self.rows[r][0] * p[0], self.rows[r][1] * p[1], self.rows[r][2] * p[2]];
        let [a, b, c] = products(2);
        [self.terms[0].sum(products(0)), self.terms[1].sum(products(1)), (a + b) + c]
    }
}
