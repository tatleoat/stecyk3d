// SPDX-License-Identifier: MIT
//! The clipping range the editor's tools read, the editor's OK step on the bracket, Square and
//! Round Edges by an amount, and the Filtering graph's pencil.
//! , rule 8
//! (Square and Round Edges by an amount).

use crate::terrain::HeightGrid;
use crate::terrain_editor::{apply_filter_in_range, at_amount, dampen_at_amount, Bracket, GRAPH_LEN};

fn rebuild(grid: &HeightGrid, heights: Vec<u16>) -> HeightGrid {
    HeightGrid::new(grid.side(), heights).expect("same side, same count")
}

/// The clipping bracket as heights: the points the tools read are those from `lo` to `hi`.
/// lo = cut toward zero(lower * 65535) with lower held at 0 or above, hi likewise with upper held
/// at 1 or below; the bracket at its full range gives 0 and 65535.,
/// (5.0's Fractal kinds, step (d): the same two numbers)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRange {
    pub lo: i64,
    pub hi: i64,
}

impl ClipRange {
    /// The bracket at its full range.
    pub const FULL: ClipRange = ClipRange { lo: 0, hi: 65535 };

    /// Whether a height lies from lo to hi (ends inside).
    pub fn holds(&self, v: i64) -> bool {
        v >= self.lo && v <= self.hi
    }
}

impl Default for ClipRange {
    fn default() -> Self {
        ClipRange::FULL
    }
}

impl Bracket {
    /// The two heights the tools read.
    pub fn range(&self) -> ClipRange {
        ClipRange {
            lo: (self.lower.max(0.0) * 65535.0).trunc() as i64,
            hi: (self.upper.min(1.0) * 65535.0).trunc() as i64,
        }
    }

    /// The editor's OK: a lower end within 1e-10 of 0 (more than -1e-10 and less than 1e-10) is set
    /// to -0.00001 and an upper end within 1e-10 of 1 to 1.00001; every other end is kept. The same
    /// in the three versions.
    pub fn ok_step(&mut self) {
        if self.lower > -1e-10 && self.lower < 1e-10 {
            self.lower = -0.00001;
        }
        if self.upper > 1.0 - 1e-10 && self.upper < 1.0 + 1e-10 {
            self.upper = 1.00001;
        }
    }
}

/// Keeps `moved` at the points whose height before the press lies in the range and the old height
/// elsewhere.
fn only_inside(grid: &HeightGrid, moved: &HeightGrid, range: ClipRange) -> HeightGrid {
    let out = grid
        .heights()
        .iter()
        .zip(moved.heights())
        .map(|(&v, &m)| if range.holds(v as i64) { m } else { v })
        .collect();
    rebuild(grid, out)
}

/// A tool dragged by an amount inside the clipping range: only the points whose height before the
/// press lies from lo to hi change; every other keeps its height.
pub fn at_amount_in(grid: &HeightGrid, click: &HeightGrid, p: i64, range: ClipRange) -> HeightGrid {
    only_inside(grid, &at_amount(grid, click, p), range)
}

/// Dampen at amount P inside the clipping range (a click is P = 64).
pub fn dampen_at_amount_in(grid: &HeightGrid, p: i64, range: ClipRange) -> HeightGrid {
    only_inside(grid, &dampen_at_amount(grid, p), range)
}

/// Apply of the Filtering graph inside the clipping range: only points whose height lies from lo
/// to hi are remapped.,
pub fn apply_filter_in(grid: &HeightGrid, graph: &[u16; GRAPH_LEN], range: ClipRange) -> HeightGrid {
    apply_filter_in_range(grid, graph, |v| range.holds(v))
}

/// Square Edges by an amount d (positive to the right; 0 for a click, which is rule 7): k = the
/// larger of 0 and the lesser of h and cut down(m (h - 4 d) / h), new height cut down(k v / h).
/// rule 8 (Square and Round Edges by an amount)
pub fn square_edges_at(grid: &HeightGrid, d: i64) -> HeightGrid {
    let h = (grid.side() / 2) as i64;
    edge_mask_amount(grid, |dr, dc| {
        let m = (h - dr.abs().max(dc.abs())).max(0);
        (m * (h - 4 * d)).div_euclid(h)
    })
}

/// Round Edges by an amount d: as Square with the Round mask (rule 7's m) and (h + 2 d) in place
/// of (h - 4 d). rule 8 (Square and Round Edges by an amount)
pub fn round_edges_at(grid: &HeightGrid, d: i64) -> HeightGrid {
    let h = (grid.side() / 2) as i64;
    edge_mask_amount(grid, |dr, dc| {
        let q = (h - (dr * dr + dc * dc) / h).max(0);
        (q * (h + 2 * d)).div_euclid(h)
    })
}

fn edge_mask_amount(grid: &HeightGrid, scaled: impl Fn(i64, i64) -> i64) -> HeightGrid {
    let n = grid.side();
    let h = (n / 2) as i64;
    let mut out = Vec::with_capacity(n * n);
    for r in 0..n {
        for c in 0..n {
            let k = scaled(r as i64 - h, c as i64 - h).min(h).max(0);
            out.push((k * grid.at(r, c) as i64).div_euclid(h) as u16);
        }
    }
    rebuild(grid, out)
}

/// The pointer-tracking of one stroke of the Filtering graph's pencil: the graph is 128 by 128
/// screen pixels and a place is (x, y) from its left and top edges, 0 to 127.
/// (Outputs, the pencil)
pub struct Pencil<'a> {
    graph: &'a mut [u16; GRAPH_LEN],
    previous: (i64, i64),
}

impl<'a> Pencil<'a> {
    /// The press at `place`. As 5.0 the press alone sets nothing and the place before the first
    /// place read is the pressed place; as 3.0 and 3.1 the press sets the pressed column's entry
    /// from the pressed row at once and the place one pixel right and one below counts as the
    /// place before the first place read (code only).
    pub fn press(graph: &'a mut [u16; GRAPH_LEN], five: bool, place: (i64, i64)) -> Pencil<'a> {
        if five {
            return Pencil { graph, previous: place };
        }
        if (0..GRAPH_LEN as i64).contains(&place.0) {
            graph[place.0 as usize] = Self::entry(place.1.clamp(0, 127));
        }
        Pencil { graph, previous: (place.0 + 1, place.1 + 1) }
    }

    /// G = 512 (127 - y).
    fn entry(y: i64) -> u16 {
        (512 * (127 - y)) as u16
    }

    /// The pointer is read at `q`: a straight line from the previous place's column toward q's,
    /// the column of q itself set only when the pointer moves on past it. A place equal to the
    /// previous one changes nothing.
    pub fn read(&mut self, q: (i64, i64)) {
        let p = self.previous;
        if q == p {
            return;
        }
        let (a, b) = (p.0.min(q.0).clamp(0, 128), p.0.max(q.0).clamp(0, 128));
        let s = (b - a) as f64;
        for c in a..b {
            let t = (c - p.0).abs() as f64;
            let f = 1.0 - t / s;
            let (qy, py) = (q.1 as f64, p.1 as f64);
            let y = if q.1 >= p.1 { qy - (qy - py) * f } else { qy + (py - qy) * f };
            self.graph[c as usize] = Self::entry((y.trunc() as i64).clamp(0, 127));
        }
        self.previous = q;
    }
}
