// SPDX-License-Identifier: MIT
//! Bryce 5's added noise types 27 to 48: Hermite lattice noise (27), gradient noise (28),
//! distorted noise (29, 30) and the cellular basis (31 to 48), each as a function of the point
//! in noise space, over two sets made once from the substrate table `noise-bits-five`.
//!
//! (Bryce 5's types , a section of
//! ), (the register; the orders stated
//! from code)
//!
//! Every operation is one operation in the order written (no fused multiply-add); `single` is
//! 's ⟦e⟧. Types 28 and 30 to 48 are binary64 throughout (they fold their
//! coordinates first); types 27 and 29 are written over `Arith`, binary64 at a calm point and
//! the register at 53 bits (`RegValue`) elsewhere (The arithmetic).

use std::ops::{Add, Mul, Sub};
use std::sync::OnceLock;

use crate::numerics::{Bryce5Rand, RegValue, to_i32};

use super::octaves::NoiseSample;

/// The number of vectors in the gradient set and of cells in the cell set.
const SET_SIZE: usize = 1024;
/// The places of the orders π and π' (π(m) = π(m − 1024) above 1,023).
const PLACES: usize = 2048;
/// The seed of each set's generator.
const SET_SEED: u32 = 1;
/// A draw's shift before the remainders of the orders and the cell counts.
const DRAW_SHIFT: u32 = 17;
/// The most points a cell holds (three of three numbers).
const MAX_CELL_NUMBERS: usize = 9;

/// The fold's upper end, 2^31 − 1.
const FOLD_TOP: f64 = 2_147_483_647.0;
/// The fold's period, 2^32 − 2.
const FOLD_PERIOD: f64 = 4_294_967_294.0;
/// 2^54: from here on the fold first takes 2^32 − 2 away step by step.
const FOLD_LIMIT: f64 = 18_014_398_509_481_984.0;
/// The most steps this program takes before giving up (the result there is the program's
/// choice,: refused). (The types that read a table)
const FOLD_STEPS: u32 = 1 << 24; // (2^24 steps, as the cases' bound)
/// The shift of the coordinates before the fold.
const SHIFT: f64 = 100.0;
/// The scale of the gradient noise.
const GRADIENT_SCALE: f64 = 1.5;
/// The offset of the distorted noises' second samples.
const DISTORT_STEP: f64 = 10.0;
/// A coordinate the setting does not have.
const ABSENT: f64 = 0.5;

/// The two sets read by types 28 and 30 (the gradient set) and by types 31 to 48 (the cell
/// set), each made with its own generator seeded with 1.
#[derive(Clone, Debug)]
pub struct FiveSets {
    /// The vector at each place 0 to 2,047: g_m below 1,024, (first coordinate of
    /// g_(m − 1024), 0, 0) above.
    gradients: Box<[[f64; 3]; PLACES]>,
    /// The order π at places 0 to 2,047.
    order: Box<[u16; PLACES]>,
    /// The cells 0 to 1,023.
    cells: Box<[Cell; SET_SIZE]>,
    /// The order π' at places 0 to 2,047.
    cell_order: Box<[u16; PLACES]>,
}

/// One cell of the cell set.
#[derive(Clone, Copy, Debug, Default)]
struct Cell {
    /// n_c, 1 to 3.
    count: usize,
    /// A_c, the single point (types 45 to 48).
    single_point: [f64; 3],
    /// e_0 to e_(3n_c − 1); the rest 0.
    numbers: [f64; MAX_CELL_NUMBERS],
}

impl FiveSets {
    /// Makes both sets from `noise-bits-five`.,
    pub fn new(noise_bits_five: &[u32; 64]) -> FiveSets {
        let (gradients, order) = gradient_set(noise_bits_five);
        let (cells, cell_order) = cell_set(noise_bits_five);
        FiveSets { gradients, order, cells, cell_order }
    }

    /// The vector at place `m` (0 to 2,047).
    pub fn gradient(&self, m: usize) -> [f64; 3] {
        self.gradients[m]
    }

    /// π(m) for m from 0 to 2,047.
    pub fn order(&self, m: usize) -> usize {
        self.order[m] as usize
    }

    /// π'(m) for m from 0 to 2,047.
    pub fn cell_order(&self, m: usize) -> usize {
        self.cell_order[m] as usize
    }

    /// The count n_c of cell `c` (0 to 1,023).
    pub fn cell_count(&self, c: usize) -> usize {
        self.cells[c].count
    }

    /// The single point A_c of cell `c`.
    pub fn cell_single_point(&self, c: usize) -> [f64; 3] {
        self.cells[c].single_point
    }

    /// The numbers e_0 to e_(3n_c − 1) of cell `c`.
    pub fn cell_numbers(&self, c: usize) -> &[f64] {
        let cell = &self.cells[c];
        &cell.numbers[..3 * cell.count]
    }
}

/// A single of the generator, as a double.
fn unit(g: &mut Bryce5Rand) -> f64 {
    g.next_unit() as f64
}

/// An order of 0 to 1,023 by exchanges, extended to 2,047.
fn make_order(g: &mut Bryce5Rand) -> Box<[u16; PLACES]> {
    let mut order = Box::new([0u16; PLACES]);
    for (m, place) in order.iter_mut().take(SET_SIZE).enumerate() {
        *place = m as u16;
    }
    for i in 0..SET_SIZE {
        let j = ((g.next_u32() >> DRAW_SHIFT) % SET_SIZE as u32) as usize;
        order.swap(i, j);
    }
    for m in SET_SIZE..PLACES {
        order[m] = order[m - SET_SIZE];
    }
    order
}

/// The gradient set and its order π.
fn gradient_set(bits: &[u32; 64]) -> (Box<[[f64; 3]; PLACES]>, Box<[u16; PLACES]>) {
    let mut g = Bryce5Rand::new(SET_SEED, bits);
    let mut gradients = Box::new([[0.0f64; 3]; PLACES]);
    for vector in gradients.iter_mut().take(SET_SIZE) {
        let (c, l) = loop {
            let c = [2.0 * unit(&mut g) - 1.0, 2.0 * unit(&mut g) - 1.0, 2.0 * unit(&mut g) - 1.0];
            let l = ((c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]).sqrt();
            if !(l > 1.0) {
                break (c, l);
            }
        };
        *vector = [single(c[0] / l), single(c[1] / l), single(c[2] / l)];
    }
    for m in SET_SIZE..PLACES {
        gradients[m] = [gradients[m - SET_SIZE][0], 0.0, 0.0];
    }
    let order = make_order(&mut g);
    (gradients, order)
}

/// The cell set and its order π'.
fn cell_set(bits: &[u32; 64]) -> (Box<[Cell; SET_SIZE]>, Box<[u16; PLACES]>) {
    let mut g = Bryce5Rand::new(SET_SEED, bits);
    let mut cells = Box::new([Cell::default(); SET_SIZE]);
    for cell in cells.iter_mut() {
        let d1 = g.next_u32();
        let d2 = g.next_u32();
        cell.count = ((d2 >> DRAW_SHIFT) % 3 + 1) as usize;
        cell.single_point = [unit(&mut g), unit(&mut g), unit(&mut g)];
        let unused = 3 * ((d1 >> DRAW_SHIFT) % 2 + 1);
        for _ in 0..unused {
            g.next_u32();
        }
        for e in cell.numbers.iter_mut().take(3 * cell.count) {
            *e = unit(&mut g);
        }
    }
    let order = make_order(&mut g);
    (cells, order)
}

/// ⟦e⟧: e rounded to the nearest binary32 (ties to even) and taken again as a double.
fn single(e: f64) -> f64 {
    (e as f32) as f64
}

/// The fold F. From 2^54 up, 2^32 − 2 is taken away again and again, each difference rounded
/// to binary64, until the number is below 2^54. `None` where no result is specified (an
/// infinity, not a number, a number the steps never bring below 2^54) and, by this program's
/// choice, after 2^24 steps.
fn fold(v: f64) -> Option<f64> {
    let mut a = v.abs();
    if !a.is_finite() {
        return None;
    }
    let mut steps = 0;
    while a >= FOLD_LIMIT {
        let next = a - FOLD_PERIOD;
        if next == a || steps == FOLD_STEPS {
            return None;
        }
        a = next;
        steps += 1;
    }
    if a <= FOLD_TOP {
        return Some(a);
    }
    let r = a % FOLD_PERIOD; // exact remainder of doubles
    Some(if r <= FOLD_TOP { r } else { FOLD_PERIOD - r })
}

/// The whole part of a folded coordinate as a signed 32-bit number (types 28 and 30).
fn whole_signed(v: f64) -> i64 {
    to_i32(v) as i64
}

/// The whole part of a folded coordinate as an unsigned 32-bit number (types 31 to 48).
fn whole_unsigned(v: f64) -> i64 {
    to_i32(v) as u32 as i64
}

/// The arithmetic a type is formed in: binary64 (`f64`) or the register at 53 bits
/// (`RegValue`). Each operation rounds once in its own format.
/// (The register)
trait Arith: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> {
    /// A double, exactly.
    fn of(v: f64) -> Self;
    /// ⟦e⟧.
    fn single(self) -> Self;
    /// Stored as a binary64 (the point handed to a lattice).
    fn stored(self) -> f64;
}

impl Arith for f64 {
    fn of(v: f64) -> f64 {
        v
    }
    fn single(self) -> f64 {
        single(self)
    }
    fn stored(self) -> f64 {
        self
    }
}

impl Arith for RegValue {
    fn of(v: f64) -> RegValue {
        RegValue::from_f64(v)
    }
    fn single(self) -> RegValue {
        RegValue::single(self)
    }
    fn stored(self) -> f64 {
        self.to_f64()
    }
}

/// The smoothstep S(t) = (3 − 2t)·(t·t).
fn smooth<T: Arith>(t: T) -> T {
    (T::of(3.0) - T::of(2.0) * t) * (t * t)
}

/// Hermite's rise ρ(f) = (3 − 2f)·f², with f² as the weight takes it.
fn rise<T: Arith>(f: T, f2: T) -> T {
    (T::of(3.0) - T::of(2.0) * f) * f2
}

/// Hermite's near slope weight ν(f) = (f − 2)·f² + f.
fn near<T: Arith>(f: T, f2: T) -> T {
    (f - T::of(2.0)) * f2 + f
}

/// Hermite's far slope weight φ(f) = f²·f − f².
fn far<T: Arith>(f: T, f2: T) -> T {
    f2 * f - f2
}

/// A blend of u and v by w: u + (v − u)·w.
fn blend(u: f64, v: f64, w: f64) -> f64 {
    u + (v - u) * w
}

// ---------------------------------------------------------------------------------------
// Type 27 and its lattices.

/// The lattice cell of a coordinate: T(u), less 1 when u < 0 or u is not a number.
fn lattice_cell(u: f64) -> i64 {
    let i = to_i32(u) as i64;
    if u < 0.0 || u.is_nan() { i - 1 } else { i }
}

/// A whole number as a word: its low 32 bits.
fn word(i: i64) -> u32 {
    i as u32
}

/// The hash H(w).
fn hash(bits: &[u32; 64], w: u32) -> u32 {
    let t = w ^ w.rotate_left(11) ^ w.rotate_left(21);
    let b = |m: u32| bits[(m % 64) as usize];
    b(t >> 18).rotate_left(3) ^ b(t >> 12).rotate_left(2) ^ b(t >> 6).rotate_left(1) ^ b(t)
}

/// H2(i, j) = H(rot(H(j), 4) ⊕ i).
fn hash2(bits: &[u32; 64], i: u32, j: u32) -> u32 {
    hash(bits, hash(bits, j).rotate_left(4) ^ i)
}

/// H3(i, j, k) = H(rot(H(rot(H(k), 4) ⊕ j), 4) ⊕ i).
fn hash3(bits: &[u32; 64], i: u32, j: u32, k: u32) -> u32 {
    hash(bits, hash(bits, hash(bits, k).rotate_left(4) ^ j).rotate_left(4) ^ i)
}

/// The 1-D lattice's r at X.
fn lattice1<T: Arith>(bits: &[u32; 64], x: f64) -> T {
    let i = lattice_cell(x);
    let f = T::of(x) - T::of(i as f64);
    let a = hash(bits, word(i));
    let b = hash(bits, word(i + 1));
    let v = |h: u32| T::of((h >> 16) as f64 / 65_536.0);
    let d = |h: u32| T::of((h & 0xffff) as f64 / 32_768.0 - 1.0);
    let f2 = f * f;
    let q = ((near(f, f2) * d(a) + rise(f, f2) * (v(b) - v(a))) + far(f, f2) * d(b)) + v(a);
    smooth(T::of(0.75) * q + T::of(0.125))
}

/// The 2-D lattice's r at (X, Y).
fn lattice2<T: Arith>(bits: &[u32; 64], x: f64, y: f64) -> T {
    let i = lattice_cell(x);
    let j = lattice_cell(y);
    let fx = T::of(x) - T::of(i as f64);
    let fy = (T::of(y) - T::of(j as f64)).single();
    let fx2 = fx * fx;
    let (rx, nx, px) = (rise(fx, fx2).single(), near(fx, fx2), far(fx, fx2).single());
    let fy2 = (fy * fy).single();
    let (ry, ny, py) = (rise(fy, fy2), near(fy, fy2), far(fy, fy2).single());
    let (iw, jw) = (word(i), word(j));
    let c00 = hash2(bits, iw, jw);
    let c10 = hash2(bits, word(i + 1), jw);
    let c01 = hash2(bits, iw, word(j + 1));
    let c11 = hash2(bits, word(i + 1), word(j + 1));
    let p = |h: u32| 3.0 + (h >> 21) as f64 / 2048.0;
    let pp = |h: u32| 2.0 + ((h >> 7) & 127) as f64 / 64.0;
    let o = |h: u32| 2.0 + ((h >> 14) & 127) as f64 / 64.0;
    let op = |h: u32| 2.0 + (h & 127) as f64 / 64.0;
    let row = |a: u32, b: u32, w: &dyn Fn(u32) -> f64, w1: &dyn Fn(u32) -> f64| {
        ((T::of(w(b) - w(a)) * rx + T::of(w1(a)) * nx) + T::of(w1(b)) * px) + T::of(w(a))
    };
    let r0 = row(c00, c10, &p, &pp);
    let r1 = row(c01, c11, &p, &pp);
    let one = T::of(1.0);
    let kappa = (((nx + px) + one) * T::of(3.0)) * ((ny + py) + one) - T::of(single(0.3));
    let g = (((r1 - r0) * ry + (r0 - kappa)) + row(c01, c11, &o, &op) * py) + row(c00, c10, &o, &op) * ny;
    (T::of(single(1.17188)) - g * T::of(single(0.48828))) * (g * g)
}

/// The single nearest the decimal of four places nearest n/15 (n from −30 to 30).
fn fifteenth(n: i32) -> f64 {
    // n·10000/15 = n·2000/3 is never half-way between whole numbers.
    let scaled = n as i64 * 2000;
    let q = scaled.div_euclid(3);
    let rem = scaled.rem_euclid(3);
    let tenthousandths = if rem == 2 { q + 1 } else { q };
    let sign = if tenthousandths < 0 { "-" } else { "" };
    let m = tenthousandths.abs();
    let text = format!("{sign}{}.{:04}", m / 10_000, m % 10_000);
    text.parse::<f32>().expect("a decimal of four places") as f64
}

/// λ(d) = d/15 at index d + 15, η(d) = 2d/15 at index d + 15 (d from −15 to 15).
struct NibbleNumbers {
    lambda: [f64; 31],
    eta: [f64; 31],
}

fn nibble_numbers() -> &'static NibbleNumbers {
    static NUMBERS: OnceLock<NibbleNumbers> = OnceLock::new();
    NUMBERS.get_or_init(|| {
        let mut lambda = [0.0; 31];
        let mut eta = [0.0; 31];
        for d in -15i32..=15 {
            lambda[(d + 15) as usize] = fifteenth(d);
            eta[(d + 15) as usize] = fifteenth(2 * d);
        }
        NibbleNumbers { lambda, eta }
    })
}

/// The 3-D lattice's r at (X, Y, Z).
fn lattice3<T: Arith>(bits: &[u32; 64], x: f64, y: f64, z: f64) -> T {
    let nn = nibble_numbers();
    let lambda = |d: i32| T::of(nn.lambda[(d + 15) as usize]);
    let eta = |d: i32| T::of(nn.eta[(d + 15) as usize]);
    let mu = |n: i32| T::of(nn.lambda[(2 * n - 15 + 15) as usize]); // μ(n) = (2n − 15)/15 = λ(2n − 15)
    let nib = |h: u32, m: u32| ((h >> (4 * m)) & 15) as i32;

    let (i, j, k) = (lattice_cell(x), lattice_cell(y), lattice_cell(z));
    let fx = (T::of(x) - T::of(i as f64)).single();
    let fy = (T::of(y) - T::of(j as f64)).single();
    let fz = (T::of(z) - T::of(k as f64)).single();
    let fx2 = fx * fx;
    let (rx, nx, px) = (rise(fx, fx2).single(), near(fx, fx2).single(), far(fx, fx2).single());
    let fy2 = (fy * fy).single();
    let (ry, ny, py) = (rise(fy, fy2), near(fy, fy2), far(fy, fy2));
    let fz2 = (fz * fz).single();
    let (rz, nz, pz) = (rise(fz, fz2), near(fz, fz2).single(), far(fz, fz2).single());

    // es[dy][dz][s], the four singles of the edge along x at (dy, dz).
    let mut es = [[[T::of(0.0); 4]; 2]; 2];
    for (dy, plane) in es.iter_mut().enumerate() {
        for (dz, edge) in plane.iter_mut().enumerate() {
            let jw = word(j + dy as i64);
            let kw = word(k + dz as i64);
            let a = hash3(bits, word(i), jw, kw);
            let b = hash3(bits, word(i + 1), jw, kw);
            edge[0] = (((lambda(nib(b, 7) - nib(a, 7)) * rx + mu(nib(a, 6)) * nx) + mu(nib(b, 6)) * px)
                + lambda(nib(a, 7)))
            .single();
            for (s, m) in [(1usize, 5u32), (2, 3), (3, 1)] {
                edge[s] = (((eta(nib(b, m) - nib(a, m)) * rx + mu(nib(a, m - 1)) * nx) + mu(nib(b, m - 1)) * px)
                    + mu(nib(a, m)))
                .single();
            }
        }
    }
    let u = |dy: usize, s: usize, s1: usize| {
        (((es[dy][1][s] - es[dy][0][s]) * rz + es[dy][1][s1] * pz) + es[dy][0][s1] * nz) + es[dy][0][s]
    };
    let front = u(0, 0, 2);
    let back = u(1, 0, 2);
    let q = ((ry * (back - front) + u(0, 1, 3) * ny) + u(1, 1, 3) * py) + front;
    smooth(T::of(0.625) * q + T::of(0.1875))
}

/// The signed lattice value 2r − 1 of the 2-D lattice.
fn signed2<T: Arith>(bits: &[u32; 64], x: f64, y: f64) -> T {
    T::of(2.0) * lattice2::<T>(bits, x, y) - T::of(1.0)
}

/// The signed lattice value 2r − 1 of the 3-D lattice.
fn signed3<T: Arith>(bits: &[u32; 64], x: f64, y: f64, z: f64) -> T {
    T::of(2.0) * lattice3::<T>(bits, x, y, z) - T::of(1.0)
}

/// Type 27's value.
fn type27<T: Arith>(bits: &[u32; 64], dimension: u8, x: f64, y: f64, z: f64) -> T {
    let r: T = match dimension {
        1 => lattice1(bits, x),
        2 => lattice2(bits, x, y),
        _ => lattice3(bits, x, y, z),
    };
    ((T::of(2.0) * r - T::of(1.0)) + T::of(1.0)) * T::of(0.5)
}

/// Type 29's value. The offsets are singles (⟦N2⟧) and the points handed to the lattices are
/// doubles (The arithmetic); a sum of a single-valued coordinate and a single, or
/// with 10 or 0.5, never leaves binary64's normal range, so it is formed in binary64.
fn type29<T: Arith>(bits: &[u32; 64], dimension: u8, x: f64, y: f64, z: f64) -> T {
    let off = |v: T| v.single().stored();
    let n: T = if dimension == 3 {
        let ox = off(signed2::<T>(bits, x, y));
        let oy = off(signed2::<T>(bits, x + DISTORT_STEP, y));
        let oz = off(signed2::<T>(bits, x + DISTORT_STEP, y + DISTORT_STEP));
        signed3(bits, single((x + ox) + 0.5), single((y + oy) + 0.5), (z + oz) + 0.5)
    } else {
        let ox = off(signed2::<T>(bits, x, y));
        let oy = off(signed2::<T>(bits, single(x + DISTORT_STEP), y));
        signed2(bits, single((x + ox) + 0.5), (y + oy) + 0.5)
    };
    (n + T::of(1.0)) * T::of(0.5)
}

// ---------------------------------------------------------------------------------------
// Types 28 and 30: gradient noise.

/// b(i) = i mod 1024.
fn low_place(i: i64) -> usize {
    i.rem_euclid(SET_SIZE as i64) as usize
}

/// G2(X, Y).
fn gradient2(sets: &FiveSets, x: f64, y: f64) -> Option<f64> {
    let vx = fold(single(x + SHIFT))?;
    let vy = fold(y + SHIFT)?;
    let (i, j) = (whole_signed(vx), whole_signed(vy));
    let x0 = single(vx - i as f64);
    let x1 = single(x0 - 1.0);
    let y0 = vy - j as f64;
    let y1 = y0 - 1.0;
    let term = |di: i64, dj: i64, dx: f64, dy: f64| {
        let place = sets.order(sets.order(low_place(i + di)) + low_place(j + dj));
        let g = sets.gradient(place);
        g[0] * dx + g[1] * dy
    };
    let sx = smooth(x0);
    let sy = smooth(y0);
    let a = single(blend(term(0, 0, x0, y0), term(1, 0, x1, y0), sx));
    let b = blend(term(0, 1, x0, y1), term(1, 1, x1, y1), sx);
    Some(GRADIENT_SCALE * blend(a, b, sy))
}

/// G3(X, Y, Z).
fn gradient3(sets: &FiveSets, x: f64, y: f64, z: f64) -> Option<f64> {
    let vx = single(fold(single(x + SHIFT))?);
    let vy = fold(single(y + SHIFT))?;
    let vz = fold(z + SHIFT)?;
    let (i, j, k) = (whole_signed(vx), whole_signed(vy), whole_signed(vz));
    let x0 = single(vx - i as f64);
    let x1 = single(x0 - 1.0);
    let y0 = single(vy - j as f64);
    let y1 = single(y0 - 1.0);
    let z0 = vz - k as f64;
    let z1 = z0 - 1.0;
    let term = |di: i64, dj: i64, dk: i64| {
        let place = sets.order(sets.order(low_place(i + di)) + low_place(j + dj)) + low_place(k + dk);
        let g = sets.gradient(place);
        let (dx, dy, dz) = (if di == 0 { x0 } else { x1 }, if dj == 0 { y0 } else { y1 }, if dk == 0 { z0 } else { z1 });
        (g[2] * dz + g[1] * dy) + g[0] * dx
    };
    let sx = smooth(x0);
    let sy = smooth(y0);
    let sz = smooth(z0);
    let u0 = blend(term(0, 0, 0), term(1, 0, 0), sx);
    let u1 = blend(term(0, 1, 0), term(1, 1, 0), sx);
    let c = single(blend(u0, u1, sy));
    let v0 = blend(term(0, 0, 1), term(1, 0, 1), sx);
    let v1 = blend(single(term(0, 1, 1)), term(1, 1, 1), sx);
    let d = blend(v0, v1, sy);
    Some(single(GRADIENT_SCALE * blend(c, d, sz)))
}

/// Type 28's value.
fn type28(sets: &FiveSets, dimension: u8, x: f64, y: f64, z: f64) -> Option<f64> {
    let g = if dimension == 3 { gradient3(sets, x, y, z)? } else { gradient2(sets, x, y)? };
    Some((g + 1.0) * 0.5)
}

/// Type 30's value.
fn type30(sets: &FiveSets, dimension: u8, x: f64, y: f64, z: f64) -> Option<f64> {
    let g = if dimension == 3 {
        let x1 = single(x + DISTORT_STEP);
        let y1 = single(y + DISTORT_STEP);
        let ox = gradient3(sets, x, y, z)?;
        let oy = gradient3(sets, x1, y, z)?;
        let oz = gradient3(sets, x1, y1, z)?;
        gradient3(sets, single((x + ox) + 0.5), single((y + oy) + 0.5), single((z + oz) + 0.5))?
    } else {
        let ox = single(gradient2(sets, x, y)?);
        let oy = single(gradient2(sets, single(x + DISTORT_STEP), y)?);
        gradient2(sets, single((x + ox) + 0.5), single((y + oy) + 0.5))?
    };
    Some((g + 1.0) * 0.5)
}

// ---------------------------------------------------------------------------------------
// Types 31 to 48: the cellular basis.

/// The steps to the cells looked at in 3-D, in Worley's groups (the point's own cell, across a
/// face, across an edge, across a corner), each group in the stated order.
const STEPS_3D: [[i64; 3]; 27] = [
    [0, 0, 0],
    [-1, 0, 0], [0, -1, 0], [0, 0, -1], [1, 0, 0], [0, 1, 0], [0, 0, 1],
    [-1, -1, 0], [-1, 1, 0], [1, -1, 0], [1, 1, 0],
    [-1, 0, -1], [-1, 0, 1], [1, 0, -1], [1, 0, 1],
    [0, -1, -1], [0, -1, 1], [0, 1, -1], [0, 1, 1],
    [-1, -1, -1], [1, -1, -1], [-1, 1, -1], [1, 1, -1],
    [-1, -1, 1], [1, -1, 1], [-1, 1, 1], [1, 1, 1],
];

/// The steps in 1-D and 2-D: the own cell, the faces, the corners.
const STEPS_2D: [[i64; 3]; 9] = [
    [0, 0, 0],
    [-1, 0, 0], [0, -1, 0], [1, 0, 0], [0, 1, 0],
    [-1, -1, 0], [-1, 1, 0], [1, -1, 0], [1, 1, 0],
];

fn steps_2d() -> &'static [[i64; 3]] {
    &STEPS_2D
}

fn steps_3d() -> &'static [[i64; 3]] {
    &STEPS_3D
}

/// δ in 3-D from the three distances, in the order the step and the type take: (dz² + dy²) +
/// dx², but (dy² + dx²) + dz² at (0, 1, −1) for types 31 to 44 and at (0, 1, 1) for 45 to 48,
/// and (dx² + dz²) + dy² at (1, 1, 1) for 31 to 44.
fn delta3(step: [i64; 3], single_points: bool, dx: f64, dy: f64, dz: f64) -> f64 {
    let (x2, y2, z2) = (dx * dx, dy * dy, dz * dz);
    match (step, single_points) {
        ([0, 1, -1], false) | ([0, 1, 1], true) => (y2 + x2) + z2,
        ([1, 1, 1], false) => (x2 + z2) + y2,
        _ => (z2 + y2) + x2,
    }
}

/// One of the four nearest points kept: ⟦δ⟧ and the cell number.
#[derive(Clone, Copy, Debug)]
struct Kept {
    delta: f64,
    cell: usize,
}

/// The four nearest feature points, in order.
fn nearest_four(sets: &FiveSets, dimension: u8, x: f64, y: f64, z: f64, single_points: bool) -> Option<[Kept; 4]> {
    let three = dimension == 3;
    let (i, j, k, fx, fy, fz);
    if three {
        let vx = single(fold(single(x + SHIFT))?);
        let vy = fold(single(y + SHIFT))?;
        let vz = fold(z + SHIFT)?;
        i = whole_unsigned(vx);
        j = whole_unsigned(vy);
        k = whole_unsigned(vz);
        fx = single(vx - i as f64);
        fy = single(vy - j as f64);
        fz = single(vz - k as f64);
    } else {
        let vx = fold(single(x + SHIFT))?;
        let vy = fold(y + SHIFT)?;
        i = whole_unsigned(vx);
        j = whole_unsigned(vy);
        k = 0;
        fx = single(vx - i as f64);
        fy = single(vy - j as f64);
        fz = 0.0;
    }
    let order = |m: usize| sets.cell_order(m);
    let mut kept = [Kept { delta: f64::INFINITY, cell: 0 }; 4];
    let mut offer = |delta: f64, cell: usize| {
        // : a new point goes before the first kept one whose δ is
        // greater than its own; 31 to 44 compare ⟦δ⟧, 45 to 48 the δ as formed; ⟦δ⟧ is kept.
        let kept_delta = single(delta);
        let compared = if single_points { delta } else { kept_delta };
        if let Some(p) = kept.iter().position(|kp| kp.delta > compared) {
            kept.copy_within(p..3, p + 1);
            kept[p] = Kept { delta: kept_delta, cell };
        }
    };
    let list = if three { steps_3d() } else { steps_2d() };
    for &[sx, sy, sz] in list {
        let place = order(low_place(i + sx)) + low_place(j + sy);
        let c = if three {
            order(order(place) + low_place(k + sz)) % 256
        } else if single_points {
            order(place) % 256
        } else {
            order(place)
        };
        let cell = &sets.cells[c];
        let (sxf, syf, szf) = (sx as f64, sy as f64, sz as f64);
        let mut look = |p: [f64; 3]| {
            let dx = (p[0] - fx) + sxf;
            let dy = (p[1] - fy) + syf;
            let delta = if three {
                let dz = (p[2] - fz) + szf;
                delta3([sx, sy, sz], single_points, dx, dy, dz)
            } else {
                dx * dx + dy * dy
            };
            offer(delta, c);
        };
        if single_points {
            look(cell.single_point);
        } else {
            for n in 0..cell.count {
                let e = &cell.numbers;
                if three {
                    look([e[3 * n], e[3 * n + 1], e[3 * n + 2]]);
                } else {
                    look([e[2 * n], e[2 * n + 1], 0.0]);
                }
            }
        }
    }
    Some(kept)
}

/// 1 where a value is above 1.
fn cap(v: f64) -> f64 {
    if v > 1.0 { 1.0 } else { v }
}

/// Types 31 to 48's value.
fn cellular(sets: &FiveSets, type_number: i64, dimension: u8, x: f64, y: f64, z: f64) -> Option<f64> {
    let single_points = type_number >= 45;
    let kept = nearest_four(sets, dimension, x, y, z, single_points)?;
    let delta = |n: i64| kept[(n - 1) as usize].delta;
    Some(match type_number {
        31..=34 => {
            let d = delta(type_number - 30);
            if d > 1.0 { 1.0 } else { d.sqrt() }
        }
        35..=38 => cap(delta(type_number - 34)),
        39 | 41 | 43 => {
            let n = (type_number - 39) / 2 + 2;
            cap(single(delta(n).sqrt()) - delta(n - 1).sqrt())
        }
        40 | 42 | 44 => {
            let n = (type_number - 40) / 2 + 2;
            cap(delta(n) - delta(n - 1))
        }
        _ => kept[(type_number - 45) as usize].cell as f64 * single(0.0039215689),
    })
}

// ---------------------------------------------------------------------------------------

/// The value of noise type `type_number` (27 to 48) with the dimension setting `dimension`
/// (1, 2 or 3) at the point `q` in noise space, over `noise-bits-five` (`bits`) and the sets
/// made from it. `None` for a type outside 27 to 48, a dimension setting outside 1 to 3, and
/// an input whose result leaves to the program: for 28 and 30 to 48 a fold that never
/// ends (an infinity, not a number, a coordinate above 2^85) or, by this program's choice,
/// takes more than 2^24 steps. The value as the octave method stores it (binary64).
///
pub fn five_value(type_number: i64, dimension: u8, q: [f64; 3], bits: &[u32; 64], sets: &FiveSets) -> Option<f64> {
    five_sample(type_number, dimension, q, bits, sets).map(NoiseSample::stored)
}

/// Whether a point is calm: each coordinate 0 or of magnitude at least 2^-40 and below 2^26.
pub fn is_calm(q: [f64; 3]) -> bool {
    const LOW: f64 = 1.0 / 1_099_511_627_776.0;
    const HIGH: f64 = 67_108_864.0;
    q.iter().all(|&c| c == 0.0 || (c.abs() >= LOW && c.abs() < HIGH))
}

/// The value of a type as it is handed to the octave method : types 27 and 29 at a
/// point that is not calm are formed in the register at 53 bits and handed on unrounded to
/// binary64 (an extended sample) where finite; everything else is a double.
pub fn five_sample(type_number: i64, dimension: u8, q: [f64; 3], bits: &[u32; 64], sets: &FiveSets) -> Option<NoiseSample> {
    if !(1..=3).contains(&dimension) {
        return None;
    }
    let x = single(q[0]);
    let y = if dimension >= 2 { single(q[1]) } else { ABSENT };
    let z = if dimension == 3 { single(q[2]) } else { ABSENT };
    let wide = |v: RegValue| match v {
        RegValue::Finite(r) => NoiseSample::Extended(r.to_ext()),
        RegValue::Special(d) => NoiseSample::Double(d),
    };
    let calm = is_calm(q);
    match type_number {
        27 if calm => Some(NoiseSample::Double(type27::<f64>(bits, dimension, x, y, z))),
        27 => Some(wide(type27::<RegValue>(bits, dimension, x, y, z))),
        29 if calm => Some(NoiseSample::Double(type29::<f64>(bits, dimension, x, y, z))),
        29 => Some(wide(type29::<RegValue>(bits, dimension, x, y, z))),
        28 => type28(sets, dimension, x, y, z).map(NoiseSample::Double),
        30 => type30(sets, dimension, x, y, z).map(NoiseSample::Double),
        31..=48 => cellular(sets, type_number, dimension, x, y, z).map(NoiseSample::Double),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up `noise-bits-five`: a test fixture for structural properties only.
    fn fixture(seed: u32) -> [u32; 64] {
        let mut t = [0u32; 64];
        let mut x = seed;
        for v in t.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *v = x;
        }
        t
    }

    fn is_permutation(f: impl Fn(usize) -> usize) -> bool {
        let mut seen = [false; SET_SIZE];
        for m in 0..SET_SIZE {
            let v = f(m);
            if v >= SET_SIZE || seen[v] {
                return false;
            }
            seen[v] = true;
        }
        true
    }

    #[test]
    fn gradient_set_shape() {
        for seed in [0x2545_f491u32, 0x9e37_79b9, 12345] {
            let sets = FiveSets::new(&fixture(seed));
            for m in 0..SET_SIZE {
                let g = sets.gradient(m);
                let l = ((g[0] * g[0] + g[1] * g[1]) + g[2] * g[2]).sqrt();
                assert!((l - 1.0).abs() < 1e-6, "place {m}: length {l}");
                for c in g {
                    assert_eq!(c, single(c), "a single");
                }
                let h = sets.gradient(m + SET_SIZE);
                assert_eq!(h, [g[0], 0.0, 0.0]);
            }
            assert!(is_permutation(|m| sets.order(m)));
            for m in SET_SIZE..PLACES {
                assert_eq!(sets.order(m), sets.order(m - SET_SIZE));
            }
        }
    }

    #[test]
    fn cell_set_shape() {
        for seed in [0x2545_f491u32, 0x9e37_79b9, 12345] {
            let sets = FiveSets::new(&fixture(seed));
            let mut counts = [0usize; 4];
            for c in 0..SET_SIZE {
                let n = sets.cell_count(c);
                assert!((1..=3).contains(&n));
                counts[n] += 1;
                assert_eq!(sets.cell_numbers(c).len(), 3 * n);
                for e in sets.cell_numbers(c).iter().chain(sets.cell_single_point(c).iter()) {
                    assert!((0.0..1.0).contains(e));
                    assert_eq!(*e, single(*e));
                }
            }
            assert!(counts[1] > 0 && counts[2] > 0 && counts[3] > 0);
            assert!(is_permutation(|m| sets.cell_order(m)));
            for m in SET_SIZE..PLACES {
                assert_eq!(sets.cell_order(m), sets.cell_order(m - SET_SIZE));
            }
        }
    }

    #[test]
    fn sets_follow_the_draws() {
        // The cell set's first cell from a generator of its own seeded with 1.
        let bits = fixture(777);
        let sets = FiveSets::new(&bits);
        let mut g = Bryce5Rand::new(1, &bits);
        let d1 = g.next_u32();
        let d2 = g.next_u32();
        let n = ((d2 >> 17) % 3 + 1) as usize;
        assert_eq!(sets.cell_count(0), n);
        let a = [g.next_unit() as f64, g.next_unit() as f64, g.next_unit() as f64];
        assert_eq!(sets.cell_single_point(0), a);
        for _ in 0..3 * ((d1 >> 17) % 2 + 1) {
            g.next_u32();
        }
        let e: Vec<f64> = (0..3 * n).map(|_| g.next_unit() as f64).collect();
        assert_eq!(sets.cell_numbers(0), &e[..]);
        // The gradient set's first vector.
        let mut g = Bryce5Rand::new(1, &bits);
        let c = loop {
            let c = [2.0 * g.next_unit() as f64 - 1.0, 2.0 * g.next_unit() as f64 - 1.0, 2.0 * g.next_unit() as f64 - 1.0];
            let l = ((c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]).sqrt();
            if l <= 1.0 {
                break [single(c[0] / l), single(c[1] / l), single(c[2] / l)];
            }
        };
        assert_eq!(sets.gradient(0), c);
    }

    #[test]
    fn the_fold() {
        assert_eq!(fold(2_147_483_647.0), Some(2_147_483_647.0));
        assert_eq!(fold(-2_147_483_647.0), Some(2_147_483_647.0));
        assert_eq!(fold(-5.25), Some(5.25));
        assert_eq!(fold(-0.0), Some(0.0));
        // 2^31: r = 2^31 > 2^31 − 1, so 2^32 − 2 − 2^31.
        assert_eq!(fold(2_147_483_648.0), Some(2_147_483_646.0));
        // 2^32: r = 2.
        assert_eq!(fold(4_294_967_296.0), Some(2.0));
        assert_eq!(fold(-4_294_967_296.5), Some(2.5));
        // 2^32 − 2: r = 0.
        assert_eq!(fold(4_294_967_294.0), Some(0.0));
        // 3·2^31 = 6442450944: r = 2147483650, so 2147483644.
        assert_eq!(fold(6_442_450_944.0), Some(2_147_483_644.0));
        let below = 18_014_398_509_481_982.0; // 2^54 − 2
        let f = fold(below).unwrap();
        assert!((0.0..=FOLD_TOP).contains(&f));
        // 2^54: one step takes 2^32 − 2 away (exactly, at this magnitude's spacing of 4).
        let once = FOLD_LIMIT - FOLD_PERIOD;
        assert_eq!(fold(FOLD_LIMIT), fold(once));
        assert!(fold(-FOLD_LIMIT).is_some());
        // From about 2^85 a step no longer changes the number: no result.
        assert_eq!(fold(2f64.powi(90)), None);
        assert_eq!(fold(f64::INFINITY), None);
        assert_eq!(fold(f64::NEG_INFINITY), None);
        assert_eq!(fold(f64::NAN), None);
        // The whole part of 2^31 (a fold rounded to a single): signed and unsigned.
        let v = single(2_147_483_647.0);
        assert_eq!(v, 2_147_483_648.0);
        assert_eq!(whole_signed(v), -2_147_483_648);
        assert_eq!(whole_unsigned(v), 2_147_483_648);
        assert_eq!(whole_signed(7.75), 7);
        assert_eq!(whole_unsigned(7.75), 7);
    }

    #[test]
    fn hash_structure() {
        let zero = [0u32; 64];
        assert_eq!(hash(&zero, 0xdead_beef), 0);
        // Only B(0) = 1: H(0) reads B(0) four times: rot(1,3) ⊕ rot(1,2) ⊕ rot(1,1) ⊕ 1.
        let mut one = [0u32; 64];
        one[0] = 1;
        assert_eq!(hash(&one, 0), 0b1111);
        // The formula written out for a made-up table.
        let bits = fixture(99);
        for w in [0u32, 1, 0x8000_0000, 0x1234_5678, u32::MAX] {
            let t = w ^ w.rotate_left(11) ^ w.rotate_left(21);
            let b = |m: u32| bits[(m % 64) as usize];
            let h = b(t >> 18).rotate_left(3) ^ b(t >> 12).rotate_left(2) ^ b(t >> 6).rotate_left(1) ^ b(t);
            assert_eq!(hash(&bits, w), h);
            assert_eq!(hash2(&bits, w, 5), hash(&bits, hash(&bits, 5).rotate_left(4) ^ w));
            assert_eq!(
                hash3(&bits, w, 5, 9),
                hash(&bits, hash(&bits, hash(&bits, 9).rotate_left(4) ^ 5).rotate_left(4) ^ w)
            );
        }
        // Cells: a negative whole number lies in the cell below it.
        assert_eq!(lattice_cell(-3.0), -4);
        assert_eq!(lattice_cell(-0.5), -1);
        assert_eq!(lattice_cell(2.5), 2);
        assert_eq!(word(-1), u32::MAX);
    }

    #[test]
    fn type27_on_the_lattice() {
        // At a whole X the 1-D weights are 0 and q = v(H(i)).
        let bits = fixture(5);
        let sets = FiveSets::new(&bits);
        for i in [0i64, 1, 7, 1000] {
            let h = hash(&bits, word(i));
            let v = (h >> 16) as f64 / 65_536.0;
            let r = smooth(0.75 * v + 0.125);
            let want = ((2.0 * r - 1.0) + 1.0) * 0.5;
            assert_eq!(five_value(27, 1, [i as f64, 3.0, 4.0], &bits, &sets), Some(want));
        }
    }

    #[test]
    fn nibble_numbers_are_four_place_decimals() {
        assert_eq!(fifteenth(1), 0.0667f32 as f64);
        assert_eq!(fifteenth(-1), -0.0667f32 as f64);
        assert_eq!(fifteenth(2), 0.1333f32 as f64);
        assert_eq!(fifteenth(15), 1.0);
        assert_eq!(fifteenth(-30), -2.0);
        assert_eq!(fifteenth(0), 0.0);
        assert_eq!(fifteenth(7), 0.4667f32 as f64);
    }

    #[test]
    fn worley_order_of_cells() {
        let s3 = steps_3d();
        assert_eq!(s3.len(), 27);
        assert_eq!(s3[0], [0, 0, 0]);
        let moved = |s: &[i64; 3]| s.iter().filter(|&&v| v != 0).count();
        let groups: Vec<usize> = s3.iter().map(moved).collect();
        assert!(groups.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(groups.iter().filter(|&&g| g == 1).count(), 6);
        assert_eq!(groups.iter().filter(|&&g| g == 3).count(), 8);
        let s2 = steps_2d();
        assert_eq!(s2.len(), 9);
        assert_eq!(s2[0], [0, 0, 0]);
        // : the order within each group as stated
        assert_eq!(&s3[1..7], &[[-1, 0, 0], [0, -1, 0], [0, 0, -1], [1, 0, 0], [0, 1, 0], [0, 0, 1]]);
        assert_eq!(s3[7], [-1, -1, 0]);
        assert_eq!(s3[18], [0, 1, 1]);
        assert_eq!(s3[19], [-1, -1, -1]);
        assert_eq!(s3[22], [1, 1, -1]);
        assert_eq!(s3[26], [1, 1, 1]);
        assert_eq!(&s2[1..], &[[-1, 0, 0], [0, -1, 0], [1, 0, 0], [0, 1, 0], [-1, -1, 0], [-1, 1, 0], [1, -1, 0], [1, 1, 0]]);
        let mut all: Vec<[i64; 3]> = s3.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 27, "every step once");
    }

    // : the 3-D δ's order by step and type
    #[test]
    fn delta_forms_by_step() {
        let (dx, dy, dz) = (0.1, 0.7, 1e-17);
        assert_eq!(delta3([0, 0, 0], false, dx, dy, dz), (dz * dz + dy * dy) + dx * dx);
        assert_eq!(delta3([0, 1, -1], false, dx, dy, dz), (dy * dy + dx * dx) + dz * dz);
        assert_eq!(delta3([0, 1, -1], true, dx, dy, dz), (dz * dz + dy * dy) + dx * dx);
        assert_eq!(delta3([1, 1, 1], false, dx, dy, dz), (dx * dx + dz * dz) + dy * dy);
        assert_eq!(delta3([0, 1, 1], true, dx, dy, dz), (dy * dy + dx * dx) + dz * dz);
        assert_eq!(delta3([0, 1, 1], false, dx, dy, dz), (dz * dz + dy * dy) + dx * dx);
    }

    // : where every value stays in binary64's normal range the
    // register gives binary64's results, so at calm points both forms agree bit for bit.
    #[test]
    fn register_form_agrees_at_calm_points() {
        let bits = fixture(11);
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f64 / u32::MAX as f64 - 0.5) * 2000.0
        };
        for _ in 0..300 {
            let q = [next(), next(), next()];
            assert!(is_calm(q));
            let (x, y, z) = (single(q[0]), single(q[1]), single(q[2]));
            for d in 1..=3u8 {
                let a = type27::<f64>(&bits, d, x, y, z);
                let b = type27::<RegValue>(&bits, d, x, y, z).to_f64();
                assert_eq!(a.to_bits(), b.to_bits(), "type 27 {d}-D at {q:?}");
                let a = type29::<f64>(&bits, d, x, y, z);
                let b = type29::<RegValue>(&bits, d, x, y, z).to_f64();
                assert_eq!(a.to_bits(), b.to_bits(), "type 29 {d}-D at {q:?}");
            }
        }
    }

    // : at a point that is not calm a value keeps the wider
    // exponent, and is handed on as an extended sample.
    #[test]
    fn far_points_hand_on_a_wide_sample() {
        let bits = fixture(3);
        let sets = FiveSets::new(&bits);
        assert!(!is_calm([1e30, 0.0, 0.0]));
        assert!(!is_calm([1e-13, 0.0, 0.0]));
        assert!(is_calm([0.0, 1.0, -67_108_863.0]));
        let mut wide_seen = 0;
        for x in [1e12, 3e15, -7e20, 1e30, 2e38] {
            match five_sample(27, 1, [x, 0.0, 0.0], &bits, &sets) {
                Some(NoiseSample::Extended(_)) => wide_seen += 1,
                Some(NoiseSample::Double(_)) => {}
                None => panic!("type 27 gives a value at {x}"),
            }
            assert!(five_value(27, 1, [x, 0.0, 0.0], &bits, &sets).is_some());
        }
        assert_eq!(wide_seen, 5, "finite register values are handed on unrounded");
        // An infinity stays a double.
        assert!(matches!(five_sample(27, 1, [f64::INFINITY, 0.0, 0.0], &bits, &sets), Some(NoiseSample::Double(_))));
    }

    fn points() -> Vec<[f64; 3]> {
        let mut out = vec![
            [0.0, 0.0, 0.0],
            [-0.0, 1.0, -1.0],
            [0.5, 0.25, 0.125],
            [-3.0, -2.0, -1.0],
            [1e-30, 2.0, 3.0],
            [1e8, -1e8, 3e7],
            [2147483547.0, 2147483548.0, -2147483648.0],
            [4.3e9, -4.3e9, 1e15],
            [1e17, 1.0, 1.0],
            [1e40, 1.0, 1.0],
            [f64::INFINITY, 0.0, 0.0],
            [0.0, f64::NAN, 0.0],
            [0.0, 0.0, f64::NEG_INFINITY],
        ];
        let mut s = 0x1234_5678u32;
        for _ in 0..300 {
            let mut c = [0.0; 3];
            for v in c.iter_mut() {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                *v = (s as f64 / 4_294_967_296.0 - 0.5) * 200.0;
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn values_and_refusals() {
        let bits = fixture(0x0bad_cafe);
        let sets = FiveSets::new(&bits);
        let unit = single(0.0039215689);
        for q in points() {
            for d in 1..=3u8 {
                let v = |t: i64| five_value(t, d, q, &bits, &sets);
                assert_eq!(v(26), None);
                assert_eq!(v(49), None);
                assert_eq!(five_value(31, 0, q, &bits, &sets), None);
                assert_eq!(five_value(31, 4, q, &bits, &sets), None);
                assert!(v(27).is_some() && v(29).is_some(), "{q:?}");
                for t in 31..=34 {
                    if let Some(x) = v(t) {
                        assert!((0.0..=1.0).contains(&x), "type {t} at {q:?}: {x}");
                    }
                }
                for t in 35..=44 {
                    if let Some(x) = v(t) {
                        assert!(x <= 1.0, "type {t} at {q:?}: {x}");
                        assert!(x >= -1e-6, "type {t} at {q:?}: {x}");
                    }
                }
                if let (Some(a), Some(b), Some(c), Some(e)) = (v(35), v(36), v(37), v(38)) {
                    assert!(a <= b && b <= c && c <= e);
                }
                for t in 45..=48 {
                    if let Some(x) = v(t) {
                        let n = (x / unit).round();
                        assert!((0.0..=255.0).contains(&n));
                        assert_eq!(n * unit, x);
                    }
                }
                // A coordinate the fold cannot take.
                let infinite = q.iter().take(d as usize).any(|c| !single(*c).is_finite());
                if infinite {
                    for t in [28, 30, 31, 40, 45] {
                        assert_eq!(v(t), None, "type {t} at {q:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn absent_coordinates_play_no_part() {
        let bits = fixture(31);
        let sets = FiveSets::new(&bits);
        for t in 27..=48 {
            let a = five_value(t, 1, [3.3, 1.0, 2.0], &bits, &sets);
            let b = five_value(t, 1, [3.3, 7.0, -5.0], &bits, &sets);
            assert_eq!(a, b, "type {t}");
            let a = five_value(t, 2, [3.3, 1.0, 2.0], &bits, &sets);
            let b = five_value(t, 2, [3.3, 1.0, -5.0], &bits, &sets);
            assert_eq!(a, b, "type {t}");
        }
    }

    /// The points of the 5.0 table cases (inputs only): no type, setting or point panics.
    #[test]
    fn table_case_points_do_not_panic() {
        // Reference cases are optional: with STECYK3D_REFERENCE_VECTORS (a folder holding table-cases/) unset the test has nothing to read.
        let Some(dir) = std::env::var_os("STECYK3D_REFERENCE_VECTORS") else { return };
        let path = std::path::Path::new(&dir).join("table-cases/textures-5.0.jsonl");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let bits = fixture(0x5eed);
        let sets = FiveSets::new(&bits);
        let key = "\"point\":[";
        let mut seen = 0;
        for line in text.lines() {
            let Some(at) = line.find(key) else { continue };
            let rest = &line[at + key.len()..];
            let Some(end) = rest.find(']') else { continue };
            let words: Vec<f64> = rest[..end]
                .split(',')
                .filter_map(|w| u64::from_str_radix(w.trim().trim_matches('"'), 16).ok())
                .map(f64::from_bits)
                .collect();
            if words.len() != 3 {
                continue;
            }
            seen += 1;
            let q = [words[0], words[1], words[2]];
            for scale in [1.0, 16.0, 250.0] {
                let q = [q[0] * scale, q[1] * scale, q[2] * scale];
                for d in 1..=3u8 {
                    for t in 27..=48 {
                        let _ = five_value(t, d, q, &bits, &sets);
                    }
                }
            }
        }
        assert!(seen > 0);
    }
}
