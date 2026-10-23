// SPDX-License-Identifier: MIT
//! The grid that sorts a star field's entries into cells, and a ray's cell.
//!
//! ("The grid": 90 columns and 45 rows of 4 degrees, the place of a direction, the
//! cells an entry reaches, a ray's cell),
//! ("The grid")
//!
//! A ray meets the entries that reach its cell, in the order of the list, and no other entry.
//! For every ray of the entry's vectors the entries tested one by one in list order are the same
//! ones; the grid is the original's rule for every ray, a ray near a pole of a catalogue field
//! and a ray straight up in the field's frame (the last cell) among them.

use std::f64::consts::{PI, TAU};

use super::stars::FieldEntry;
use crate::geom::Vec3;
use crate::numerics::x87::{cos_ext, div_f64_by, sin_ext, tan_ext};
use crate::numerics::{acos, asin, atan_div};

/// The columns round the field.
const COLUMNS: i32 = 90;
/// The rows from pole to pole.
const ROWS: i32 = 45;
/// The half-width of a reach never exceeds a quarter of the circle.
const HALF_WIDTH_MOST: f64 = 0.25;
/// Below this h the half-width is the most.
const H_LEAST: f64 = 1e-7;

/// Cuts toward zero; a number that is not a number or lies outside the 32-bit integers gives
/// -2^31.
fn whole(x: f64) -> i32 {
    if x.is_nan() || !(x > -2_147_483_649.0 && x < 2_147_483_648.0) {
        return i32::MIN;
    }
    x as i32
}

/// The place (u, r) of a direction W: r the share of the way from the lowest to the highest
/// point, u the share of the way round.
fn place(w: Vec3) -> (f64, f64) {
    let a = acos(-w[1]);
    let r = a / PI;
    if r == 0.0 || r == 1.0 || r.is_nan() {
        return (0.0, r);
    }
    let t = acos(div_f64_by(w[0], sin_ext(a))) / TAU;
    (if w[2] > 0.0 { t } else { 1.0 - t }, r)
}

/// Which cell a ray lies in, as its index 90 k + c (an index of 4,050 or more is the last
/// cell, column 89 and row 44).
pub fn cell_of(w: Vec3) -> usize {
    let (u, r) = place(w);
    let column = whole(f64::from(COLUMNS) * u);
    let row = whole(f64::from(ROWS) * r);
    // taken as an unsigned 32-bit number
    let index = (row.wrapping_mul(COLUMNS)).wrapping_add(column) as u32;
    if index >= (COLUMNS * ROWS) as u32 { (COLUMNS * ROWS - 1) as usize } else { index as usize }
}

/// The cells an entry reaches, as (column, row) pairs, for its direction V and g (the cosine of
/// its reach).
fn reach(direction: Vec3, g: f64) -> Vec<usize> {
    let (u, r) = place(direction);
    if g == 0.0 {
        // the zero entry: only the one cell its V gives as a ray's
        return vec![cell_of(direction)];
    }
    let b = acos(g);
    let beta = b / PI;
    let row_first = whole(f64::from(ROWS) * if r - beta > 0.0 { r - beta } else { 0.0 });
    let row_last = whole(f64::from(ROWS) * if r + beta < 1.0 { r + beta } else { 1.0 });
    // h: a cosine less an angle, as the original forms it
    let h = cos_ext(asin(direction[1])).to_f64() - b;
    let half = if !(h > H_LEAST) {
        HALF_WIDTH_MOST
    } else {
        let w = atan_div(tan_ext(b).div_to_f64(h), TAU);
        if w > HALF_WIDTH_MOST { HALF_WIDTH_MOST } else { w }
    };
    let mut u1 = u + half;
    if u1 > 1.0 {
        u1 -= 1.0;
    }
    let mut u0 = u - half;
    if u0 < 0.0 {
        u0 += 1.0;
    }
    let mut first = whole(f64::from(COLUMNS) * u0);
    let last = whole(f64::from(COLUMNS) * u1);
    if first > last {
        first -= COLUMNS;
    }
    // A run longer than the circle, or of numbers that are not numbers, takes every cell.
    let columns: Vec<i32> = if first == i32::MIN || last == i32::MIN || last - first >= COLUMNS {
        (0..COLUMNS).collect()
    } else {
        (first..=last).collect()
    };
    let rows: Vec<i32> = if row_first == i32::MIN || row_last == i32::MIN || row_last - row_first >= ROWS {
        (0..ROWS).collect()
    } else {
        (row_first..=row_last).collect()
    };
    let mut cells = Vec::with_capacity(columns.len() * rows.len());
    for &k in &rows {
        // a row of 45 or more is k - 45 (a reach to r = 1 reaches row 0 too; none beyond 44)
        let k = if k >= ROWS { k - ROWS } else { k };
        if !(0..ROWS).contains(&k) {
            continue;
        }
        for &c in &columns {
            let c = if c < 0 {
                c + COLUMNS
            } else if c >= COLUMNS {
                c - COLUMNS
            } else {
                c
            };
            if (0..COLUMNS).contains(&c) {
                cells.push((k * COLUMNS + c) as usize);
            }
        }
    }
    cells
}

/// A field's entries sorted into the 4,050 cells, each cell's list in the order of the field's
/// list.
#[derive(Clone, Debug)]
pub struct StarGrid {
    cells: Vec<Vec<u32>>,
}

impl StarGrid {
    /// The grid of a field's entries.
    pub fn new(entries: &[FieldEntry]) -> StarGrid {
        let mut cells = vec![Vec::new(); (COLUMNS * ROWS) as usize];
        for (index, e) in entries.iter().enumerate() {
            let mut seen = reach(e.direction, e.reach_cosine);
            seen.sort_unstable();
            seen.dedup();
            for cell in seen {
                cells[cell].push(index as u32);
            }
        }
        StarGrid { cells }
    }

    /// The entries (indices into the list) a ray along `w` meets, in the order of the list.
    pub fn meets(&self, w: Vec3) -> &[u32] {
        &self.cells[cell_of(w)]
    }
}

/// A ray whose squared length (as summed here) is above this, or not a number, tests every
/// entry as 3.0 and 3.1. Not the original's: a speed-up only (`WideStarGrid`).
const WIDE_RAY_SQUARED_MOST: f64 = 1.0 + 2e-6;
/// A length no ray `candidates` answers for exceeds: sqrt(1 + 2e-6) rounded, below 1 + 2e-6.
const WIDE_RAY_LENGTH_MOST: f64 = 1.0 + 2e-6;
/// The share of |D||V| a rounded d = D·V may lie off the exact one (three products, two
/// sums: below 3.4e-16).
const WIDE_ROUNDING: f64 = 1e-12;
/// The angle (radians) added to every reach and half-width: far above the rounding of atan2,
/// acos, asin, sin and the scalings to cells (about 1e-15).
const WIDE_MARGIN: f64 = 1e-9;

/// The place of a direction for `WideStarGrid`: (ψ, φ), ψ = atan2(z, x) round the y axis in
/// [-π, π], φ the angle from -y in [0, π]; both the true angles of w whatever its length.
fn wide_place(w: Vec3) -> (f64, f64) {
    (w[2].atan2(w[0]), (w[0] * w[0] + w[2] * w[2]).sqrt().atan2(-w[1]))
}

/// The row of an angle φ from -y (clamped to 0..=44); monotone in φ.
fn wide_row(phi: f64) -> i32 {
    ((phi * (f64::from(ROWS) / PI)).floor() as i32).clamp(0, ROWS - 1)
}

/// The column (unreduced) of an angle ψ round the y axis; monotone in ψ.
fn wide_column(psi: f64) -> i32 {
    (psi * (f64::from(COLUMNS) / TAU)).floor() as i32
}

/// A ray's cell in `WideStarGrid`, its place a finite (ψ, φ).
fn wide_cell(psi: f64, phi: f64) -> usize {
    let u = if psi < 0.0 { psi + TAU } else { psi };
    let c = wide_column(u).clamp(0, COLUMNS - 1);
    (wide_row(phi) * COLUMNS + c) as usize
}

/// The cells of `WideStarGrid` an entry is put in: every cell holding a ray of length at most
/// 1 + 2e-6 for which the test d >= g (d = D·V as rounded) can hold, with room to spare.
///
/// d >= g gives D·V >= g - ε|D||V| (ε = `WIDE_ROUNDING`). Where g > 0 the angle θ between D
/// and V has cos θ >= g / (|D||V|) - ε >= g / (L|V|) - ε = k (L = `WIDE_RAY_LENGTH_MOST`), so
/// θ <= acos(k): D lies in the cap of that radius round V. The cap's angles from -y lie in
/// [φV - θ', φV + θ'] (θ' the radius with `WIDE_MARGIN`), and round the y axis within
/// 2 asin(sin(θ'/2) / s) of ψV, s the least sine of an angle in that range (from
/// 1 - cos θ >= sin φD sin φV (1 - cos Δψ)). Anything else (g <= 0, a cap reaching a pole, a
/// V not of about unit length, numbers that are not numbers) puts the entry in every column,
/// or every cell.
fn wide_reach(v: Vec3, g: f64) -> Vec<usize> {
    let every = || (0..(COLUMNS * ROWS) as usize).collect();
    let nv = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if !(nv > 0.5 && nv < 2.0) || !(g > 0.0) {
        return every();
    }
    let k = g / (WIDE_RAY_LENGTH_MOST * nv * (1.0 + WIDE_ROUNDING)) - WIDE_ROUNDING - WIDE_ROUNDING;
    if !(k > 0.0) {
        return every();
    }
    let theta = (if k < 1.0 { k } else { 1.0 }).acos() + WIDE_MARGIN;
    let (psi, phi) = wide_place(v);
    if !(psi.is_finite() && phi.is_finite() && theta.is_finite()) {
        return every();
    }
    let (lo, hi) = (phi - theta, phi + theta);
    let rows = wide_row(lo)..=wide_row(hi);
    let mut every_column = !(lo > 0.0 && hi < PI);
    let mut columns = 0..=COLUMNS - 1;
    if !every_column {
        let s = lo.sin().min(hi.sin());
        let a = (theta * 0.5).sin() / s * (1.0 + WIDE_ROUNDING);
        if !(s > 1e-3 && a < 0.5) {
            every_column = true;
        } else {
            let half = 2.0 * a.asin() + WIDE_MARGIN;
            let (first, last) = (wide_column(psi - half), wide_column(psi + half));
            if last - first >= COLUMNS - 1 {
                every_column = true;
            } else {
                columns = first..=last;
            }
        }
    }
    if every_column {
        columns = 0..=COLUMNS - 1;
    }
    let mut cells = Vec::new();
    for r in rows {
        for c in columns.clone() {
            cells.push((r * COLUMNS + c.rem_euclid(COLUMNS)) as usize);
        }
    }
    cells.sort_unstable();
    cells.dedup();
    cells
}

/// A widened grid for laying a field as 3.0 and 3.1, a speed-up only (not the original's,
/// which tests every entry): each cell lists, in the order of the field's list, every entry
/// whose test d >= g can hold for a ray in the cell (and perhaps some whose cannot). A ray
/// tests those entries one by one in list order, as the whole list would be tested; the
/// entries it leaves out lay nothing for that ray. (Stars and comets)
#[derive(Clone, Debug)]
pub struct WideStarGrid {
    cells: Vec<Vec<u32>>,
}

impl WideStarGrid {
    /// The widened grid of a field's entries (made once per render).
    pub fn new(entries: &[FieldEntry]) -> WideStarGrid {
        let mut cells = vec![Vec::new(); (COLUMNS * ROWS) as usize];
        for (index, e) in entries.iter().enumerate() {
            for cell in wide_reach(e.direction, e.reach_cosine) {
                cells[cell].push(index as u32);
            }
        }
        WideStarGrid { cells }
    }

    /// The entries (indices into the list, ascending) a ray along `w` may meet; `None` where
    /// the ray is longer than the grid answers for (or not a number): then every entry.
    pub fn candidates(&self, w: Vec3) -> Option<&[u32]> {
        let dd = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
        if !(dd <= WIDE_RAY_SQUARED_MOST) {
            return None;
        }
        let (psi, phi) = wide_place(w);
        Some(&self.cells[wide_cell(psi, phi)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The widened grid leaves out no entry the whole list's test d >= g lays, for rays near
    // and far from each entry, across a seam and near the poles.
    #[test]
    fn the_widened_grid_leaves_out_no_laid_entry() {
        use super::super::stars::{Order3, StarList};
        use crate::version::RenderAs;
        let list = StarList::seeded(RenderAs::Bryce30, 4242, 3000, 40).entries;
        let grid = WideStarGrid::new(&list);
        let mut g = crate::numerics::CRand::new(99);
        let mut tested = 0;
        for _ in 0..200_000 {
            let mut p = [0.0; 3];
            for x in &mut p {
                *x = 1.0 - f64::from(g.next_u15()) / 16383.5;
            }
            let n = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if n < 1e-3 {
                continue;
            }
            // pull towards an entry so many rays lie near one
            let e = &list[usize::from(g.next_u15()) % list.len()];
            let t = f64::from(g.next_u15()) / 32767.0;
            let w: Vec3 = std::array::from_fn(|k| (p[k] / n) * (1.0 - t) * 0.05 + e.direction[k]);
            let m = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            let w: Vec3 = w.map(|x| x / m);
            let Some(c) = grid.candidates(w) else { continue };
            assert!(c.windows(2).all(|p| p[0] < p[1]));
            for (i, e) in list.iter().enumerate() {
                if Order3::YzX.dot(w, e.direction) >= e.reach_cosine {
                    tested += 1;
                    assert!(c.binary_search(&(i as u32)).is_ok(), "entry {i} left out for {w:?}");
                }
            }
        }
        assert!(tested > 1000);
        for w in [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [1.0, 0.0, -1e-300], [1.0, 0.0, 0.0], [-1.0, 0.0, -0.0]] {
            let c = grid.candidates(w).unwrap();
            for (i, e) in list.iter().enumerate() {
                if Order3::YzX.dot(w, e.direction) >= e.reach_cosine {
                    assert!(c.binary_search(&(i as u32)).is_ok());
                }
            }
        }
        assert!(grid.candidates([f64::NAN, 0.0, 0.0]).is_none());
        assert!(grid.candidates([1.01, 0.0, 0.0]).is_none());
    }

    // : a ray exactly straight up lies in the last cell (r = 1, u = 0, an
    // index of 90 * 45 or more)
    #[test]
    fn straight_up_is_the_last_cell() {
        assert_eq!(cell_of([0.0, 1.0, 0.0]), 4049);
    }

    // : straight down is r = 0 and u = 0: the first cell
    #[test]
    fn straight_down_is_the_first_cell() {
        assert_eq!(cell_of([0.0, -1.0, 0.0]), 0);
    }

    // : a place round the field: +z gives u = 1/4 of the way (acos(0)/2pi),
    // the horizon r = 1/2
    #[test]
    fn the_horizon_ahead() {
        let cell = cell_of([0.0, 0.0, 1.0]);
        assert_eq!((cell / 90, cell % 90), (22, 22)); // r = 0.5 -> row 22; u = 0.25 -> column 22
        // and -z lies on the other side: u = 1 - 0.25
        let cell = cell_of([0.0, 0.0, -1.0]);
        assert_eq!((cell / 90, cell % 90), (22, 67));
    }

    // : an entry reaches the cell of its own direction, and an entry whose
    // reach covers the upper pole reaches the last row and (a reach to r = 1) row 0 as well
    #[test]
    fn an_entry_reaches_its_own_cell() {
        let v = [0.3, 0.5, 0.8124038404635961];
        let g = 0.9999;
        let cells = reach(v, g);
        assert!(cells.contains(&cell_of(v)));
        let pole = reach([0.0, 1.0, 0.0], 0.9);
        assert!(pole.iter().any(|&c| c / 90 == 44), "the pole's own row");
        assert!(pole.iter().all(|&c| c < 4050));
    }
}
