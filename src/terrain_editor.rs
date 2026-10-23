// SPDX-License-Identifier: MIT
//! The terrain editor's commands with measured results, as pure functions on a grid.
//! , rules 1 to 8 as revised in:
//! rules 5 to 7 at every side (measured at 64, 128, 256; the other sides by the entry's
//! fallback, the same rule), amounts (rule 8).
//! (Apply of the Filtering graph, the menu's four items, Reset; Soften and the
//! presets by the fallback),,
//! Not implemented, because the entries leave them Open: Equalize at an amount, the pencil's
//! steps, Ctrl+I's effect on the graph.
//! item 4, R1, R2, R5 (fallback only: Open), rule 4 (amount). The tools with
//! stated random rules (C1 to C8) are in terrain_tools.rs (fourth round).
//! (The terrain editor's commands)
//! Variants (5.0 Blend positions and the five menu modes);

use crate::terrain::{GridError, HeightGrid, MAX_RESIZE_SIDE, MIN_SIDE};

fn rebuild(grid: &HeightGrid, heights: Vec<u16>) -> HeightGrid {
    HeightGrid::new(grid.side(), heights).expect("same side, same count")
}

fn limited(v: i64) -> u16 {
    v.clamp(0, 65535) as u16
}

/// Raise/Lower, one click on the ball: every height 0 (rule 4: one observation taken as one
/// application; held for the owner; no vector). rule 4
pub fn raise_lower_click(grid: &HeightGrid) -> HeightGrid {
    rebuild(grid, vec![0; grid.heights().len()])
}

/// Invert: 65535 - v. rule 1
pub fn invert(grid: &HeightGrid) -> HeightGrid {
    rebuild(grid, grid.heights().iter().map(|&v| 65535 - v).collect())
}

/// Dampen: limited(floor((3v - 32768) / 2)). rule 2
pub fn dampen(grid: &HeightGrid) -> HeightGrid {
    rebuild(
        grid,
        grid.heights().iter().map(|&v| limited((3 * v as i64 - 32768).div_euclid(2))).collect(),
    )
}

/// Equalize: floor((v - lo) * 65535 / (hi - lo)); a flat grid is unchanged. rule 3
pub fn equalize(grid: &HeightGrid) -> HeightGrid {
    let lo = *grid.heights().iter().min().unwrap() as u64;
    let hi = *grid.heights().iter().max().unwrap() as u64;
    if hi == lo {
        return grid.clone();
    }
    rebuild(
        grid,
        grid.heights().iter().map(|&v| ((v as u64 - lo) * 65535 / (hi - lo)) as u16).collect(),
    )
}

/// New heights from each point's height and the sum of its eight neighbours, all from the old
/// grid; columns 0 and n - 1 kept; rows beyond the first and last repeat the edge row.
/// rules 5 and 6
fn filter3(grid: &HeightGrid, f: impl Fn(i64, i64) -> i64) -> HeightGrid {
    let n = grid.side();
    let mut out = grid.heights().to_vec();
    for r in 0..n {
        let up = r.saturating_sub(1);
        let down = (r + 1).min(n - 1);
        for c in 1..n - 1 {
            let mut s = 0i64;
            for rr in [up, r, down] {
                for cc in c - 1..=c + 1 {
                    s += grid.at(rr, cc) as i64;
                }
            }
            let v = grid.at(r, c) as i64;
            s -= v;
            out[r * n + c] = limited(f(v, s));
        }
    }
    rebuild(grid, out)
}

/// Smoothing: limited(floor((15 s + 10 v) / 128)). rule 5
pub fn smoothing(grid: &HeightGrid) -> HeightGrid {
    filter3(grid, |v, s| (15 * s + 10 * v).div_euclid(128))
}

/// Sharpening: limited(floor((1643 v - 189 s) / 128)). rule 6
pub fn sharpening(grid: &HeightGrid) -> HeightGrid {
    filter3(grid, |v, s| (1643 * v - 189 * s).div_euclid(128))
}

enum Mask {
    Ratio(u64),
    Factor(f64),
}

/// Multiplies each height by a mask of its place; `mask` gets the offsets (r - h, c - h) from
/// the middle point (h, h), h = n / 2, and n. rule 7
fn edge_mask(grid: &HeightGrid, mask: impl Fn(i64, i64, i64) -> Mask) -> HeightGrid {
    let n = grid.side();
    let h = (n / 2) as i64;
    let mut out = Vec::with_capacity(n * n);
    for r in 0..n {
        for c in 0..n {
            let v = grid.at(r, c);
            out.push(match mask(r as i64 - h, c as i64 - h, n as i64) {
                Mask::Ratio(m) => (v as u64 * m / h as u64) as u16,
                Mask::Factor(e) => (v as f64 * e).floor() as u16,
            });
        }
    }
    rebuild(grid, out)
}

/// Square Edges: floor(v m / h), m = max(0, h - max(|r-h|, |c-h|)), h = n / 2.
/// rule 7
pub fn square_edges(grid: &HeightGrid) -> HeightGrid {
    let h = (grid.side() / 2) as i64;
    edge_mask(grid, |dr, dc, _| Mask::Ratio((h - dr.abs().max(dc.abs())).max(0) as u64))
}

/// Round Edges: floor(v m / h), m = max(0, h - floor(D / h)), D = (r-h)^2 + (c-h)^2.
/// rule 7
pub fn round_edges(grid: &HeightGrid) -> HeightGrid {
    let h = (grid.side() / 2) as i64;
    edge_mask(grid, |dr, dc, _| Mask::Ratio((h - (dr * dr + dc * dc) / h).max(0) as u64))
}

/// Gaussian Edges: floor(v e), e = exp(-6.4 D / n^2) in binary64. rule 7
pub fn gaussian_edges(grid: &HeightGrid) -> HeightGrid {
    edge_mask(grid, |dr, dc, n| {
        let d = (dr * dr + dc * dc) as f64;
        Mask::Factor((-6.4 * d / (n * n) as f64).exp())
    })
}

/// A tool dragged by an amount: new = limited(floor(v + P (f - v) / 256)), f the tool's
/// one-click height at that point. rule 8 (Open for Equalize and other tools:
/// the entry's fallback is this form).
pub fn at_amount(grid: &HeightGrid, click: &HeightGrid, p: i64) -> HeightGrid {
    let out = grid
        .heights()
        .iter()
        .zip(click.heights())
        .map(|(&v, &f)| limited((256 * v as i64 + p * (f as i64 - v as i64)).div_euclid(256)))
        .collect();
    rebuild(grid, out)
}

/// Dampen at amount P: limited(floor(v + P (2v - 65535) / 256)); P = 64 is a click.
/// rule 8
pub fn dampen_at_amount(grid: &HeightGrid, p: i64) -> HeightGrid {
    let out = grid
        .heights()
        .iter()
        .map(|&v| limited((256 * v as i64 + p * (2 * v as i64 - 65535)).div_euclid(256)))
        .collect();
    rebuild(grid, out)
}

/// The Filtering graph: 128 output heights.
pub const GRAPH_LEN: usize = 128;

/// The default graph G[i] = floor(65535 i / 127); Reset restores it.
pub fn default_graph() -> [u16; GRAPH_LEN] {
    let mut g = [0u16; GRAPH_LEN];
    for (i, e) in g.iter_mut().enumerate() {
        *e = (65535 * i / 127) as u16;
    }
    g
}

/// Apply: every height v becomes G[floor(127 v / 65535)].
pub fn apply_filter(grid: &HeightGrid, graph: &[u16; GRAPH_LEN]) -> HeightGrid {
    rebuild(grid, grid.heights().iter().map(|&v| graph[(127 * v as usize) / 65535]).collect())
}

/// Apply remapping only the points whose height `inside` accepts (the clipping range,
/// `terrain_clip::apply_filter_in`).,
pub fn apply_filter_in_range(grid: &HeightGrid, graph: &[u16; GRAPH_LEN], inside: impl Fn(i64) -> bool) -> HeightGrid {
    rebuild(
        grid,
        grid.heights()
            .iter()
            .map(|&v| if inside(v as i64) { graph[(127 * v as usize) / 65535] } else { v })
            .collect(),
    )
}

/// Soften (the button the Filtering tab labels Smooth), one pass: G'[i] = cut down((G[i - 1] +
/// 2 G[i] + G[i + 1]) / 4) for i = 1 to 126, every G' from the graph before the pass; G[0] and G[127]
/// are kept. A press gives one pass at once and one more each time the editor finds the button
/// still down, so the number of passes depends on how long the button is held (the caller's).
/// ; answers
pub fn soften(graph: &[u16; GRAPH_LEN]) -> [u16; GRAPH_LEN] {
    let mut out = *graph;
    for i in 1..GRAPH_LEN - 1 {
        out[i] = ((graph[i - 1] as u32 + 2 * graph[i] as u32 + graph[i + 1] as u32) / 4) as u16;
    }
    out
}

/// The number of presets in a family's table, and the length of a row. (nine rows
/// of 128)
pub const PRESETS: usize = 9;

/// A preset's graph from its family's table (terrain-filter-presets-three as 3.0 and 3.1,
/// -five as 5.0): two passes of Soften over 256 times the row's levels. `None` for a row
/// outside 0 to 8 or a table that is not 1,152 bytes (the editor then says which table is missing and
/// leaves the graph as it is)., (Outputs: a preset's graph is
/// G[i] = 256 L[i] and then two passes of Soften)
pub fn preset_graph(table: &[u8], row: usize) -> Option<[u16; GRAPH_LEN]> {
    if row >= PRESETS || table.len() != PRESETS * GRAPH_LEN {
        return None;
    }
    let levels = &table[row * GRAPH_LEN..(row + 1) * GRAPH_LEN];
    let raw: [u16; GRAPH_LEN] = std::array::from_fn(|i| 256 * levels[i] as u16);
    Some(soften(&soften(&raw)))
}

/// The resolution selector:'s grid with the rows' difference. Where blends the base
/// row with the row below, the editor takes the row below to be the base row itself unless
/// base + 1 <= old - 2; a row exactly on the last row takes it. Columns as.
/// Sides 2 to 1024 as's Limits.
pub fn change_resolution(grid: &HeightGrid, new_side: usize) -> Result<HeightGrid, GridError> {
    change_resolution_up_to(grid, new_side, MAX_RESIZE_SIDE)
}

/// The same change as 5.0's resolution menu, which adds the sides 2048 and 4096: sides 2 to
/// 4096, the rule unchanged.,
pub fn change_resolution_five(grid: &HeightGrid, new_side: usize) -> Result<HeightGrid, GridError> {
    change_resolution_up_to(grid, new_side, crate::terrain::MAX_SIDE)
}

fn change_resolution_up_to(grid: &HeightGrid, new_side: usize, max_side: usize) -> Result<HeightGrid, GridError> {
    let old = grid.side();
    for side in [old, new_side] {
        if !(MIN_SIDE..=max_side).contains(&side) {
            return Err(GridError::ResizeSideOutOfRange { side });
        }
    }
    if old == new_side {
        return Ok(grid.clone());
    }
    // (whole part, next index, fraction) per output index, as's place.
    // Rows: the next row is whole + 1 only if whole + 1 <= old - 2 ; columns: only if
    // whole + 1 <= old - 1 .
    let place = |a: usize, rows: bool| {
        let p = (a * (old - 1)) as f64 / (new_side - 1) as f64;
        let whole = p as usize;
        let last_next = if rows { old - 2 } else { old - 1 };
        let next = if whole + 1 <= last_next { whole + 1 } else { whole };
        (whole, next, p - whole as f64)
    };
    let rows: Vec<_> = (0..new_side).map(|a| place(a, true)).collect();
    let cols: Vec<_> = (0..new_side).map(|a| place(a, false)).collect();
    let mut out = Vec::with_capacity(new_side * new_side);
    for &(r0, r1, fr) in &rows {
        for &(c0, c1, fc) in &cols {
            let h00 = grid.at(r0, c0) as f64;
            let h01 = grid.at(r0, c1) as f64;
            let h10 = grid.at(r1, c0) as f64;
            let h11 = grid.at(r1, c1) as f64;
            let top = h00 + (h01 - h00) * fc;
            let bottom = h10 + (h11 - h10) * fc;
            out.push((top + (bottom - top) * fr) as u16);
        }
    }
    HeightGrid::new(new_side, out)
}

// ---- third round: items 2 and 6, items 3, 4 and R1, R2, R4, R5 ----

/// Posterize at a step s: cut down(v / s) * s. A click is s = 1. item 4
pub fn posterize(grid: &HeightGrid, step: u32) -> HeightGrid {
    let s = step.max(1) as u32;
    rebuild(grid, grid.heights().iter().map(|&v| ((v as u32 / s) * s) as u16).collect())
}

/// The session's random numbers: whole numbers 0 to 32767. R1
pub struct HeightNoiseSource {
    x: u32,
    taken: u64,
    /// The clipping range the tools of the session read (full unless the bracket was dragged).
    pub range: crate::terrain_clip::ClipRange,
}

impl HeightNoiseSource {
    /// A new session: x is 1. R1
    pub fn new() -> Self {
        HeightNoiseSource { x: 1, taken: 0, range: crate::terrain_clip::ClipRange::FULL }
    }
    /// The next number. R1
    pub fn next_number(&mut self) -> i64 {
        self.x = self.x.wrapping_mul(214013).wrapping_add(2531011);
        self.taken += 1;
        ((self.x >> 16) & 0x7fff) as i64
    }
    /// How many numbers the session has taken (the README's `numbers_taken_before`).
    /// terrain-editor-round4
    pub fn taken(&self) -> u64 {
        self.taken
    }
    /// Takes `count` numbers and drops them: the session as it is after a click that took that
    /// many. R1 (a second click goes on where the first stopped)
    pub fn skip(&mut self, count: u64) {
        for _ in 0..count {
            self.next_number();
        }
    }
}

impl Default for HeightNoiseSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Height Noise, taking the numbers from `source` one a point in reading order.
/// R2 (a continuing click is the entry's fallback)
pub fn height_noise_with(grid: &HeightGrid, source: &mut HeightNoiseSource) -> HeightGrid {
    let out = grid
        .heights()
        .iter()
        .map(|&v| {
            let r = source.next_number();
            let h = (v / 2) as i64;
            let add = (640 * (2 * h * r + 32767)).div_euclid(65535 * 32767);
            limited(v as i64 + add)
        })
        .collect();
    rebuild(grid, out)
}

/// Height Noise, one click, the first of a new session. R2
pub fn height_noise(grid: &HeightGrid) -> HeightGrid {
    height_noise_with(grid, &mut HeightNoiseSource::new())
}

/// Raise/Lower dragged to the amount P: limited(floor(v + P v / 256)). A positive P raises, a
/// negative one lowers. rule 4 (cases-raise-lower.jsonl)
pub fn raise_lower(grid: &HeightGrid, p: i64) -> HeightGrid {
    rebuild(
        grid,
        grid.heights().iter().map(|&v| limited((256 * v as i64 + p * v as i64).div_euclid(256))).collect(),
    )
}

// Spikes (C3) and Erode (C8) are in terrain_tools (terrain-editor-round4).

/// Basic Noise: the rule is Open; fallback: noise of R1's numbers scaled by the point's second
/// differences, so flat grids and ramps stay unchanged. R5 (fallback)
pub fn basic_noise_fallback(grid: &HeightGrid) -> HeightGrid {
    let n = grid.side();
    let g = |r: usize, c: usize| grid.at(r, c) as i64;
    let mut src = HeightNoiseSource::new();
    let mut out = grid.heights().to_vec();
    for r in 0..n {
        for c in 0..n {
            let rnd = src.next_number();
            if r == 0 || c == 0 || r == n - 1 || c == n - 1 {
                continue;
            }
            let v = g(r, c);
            let d = (g(r - 1, c) + g(r + 1, c) - 2 * v).abs() + (g(r, c - 1) + g(r, c + 1) - 2 * v).abs();
            out[r * n + c] = limited(v + (d * (2 * rnd - 32767)).div_euclid(4 * 32767));
        }
    }
    rebuild(grid, out)
}

// ---- the Pictures tab as 3.0 and 3.1 (rules P1 to P4) is `terrain_picture_tab` ; the
// power-curve fallback of the fourth round is gone: the canvas colours come from the substrate's
// table terrain-default-gradient-three, never from a made-up curve.

/* fifth round: Variants (5.0);*/

/// Apply on the Pictures tab as 5.0, slot B white, Blend as the tab opens: each point is
/// cut down(a / 2) + cut down(b / 2), a the height, b = 65535: cut down(v / 2) + 32767.
/// Variants (te5y-12, te5y-13, te5d-05, 06, 08, 10)
pub fn pictures_apply_white_v50(grid: &HeightGrid) -> HeightGrid {
    rebuild(grid, grid.heights().iter().map(|&v| (v / 2 + 32767) as u16).collect())
}

/// Apply as 5.0 after Copy under slot A and Paste under slot B: b = 256 cut down(v / 256), the
/// result cut down(v / 2) + 128 cut down(v / 256), each half cut down on its own.
/// Variants (te5y-14, te5d-07, 09, 11)
pub fn pictures_apply_copy_v50(grid: &HeightGrid) -> HeightGrid {
    rebuild(grid, grid.heights().iter().map(|&v| (v / 2 + 128 * (v / 256)) as u16).collect())
}

/// The Pictures tab's menu modes as 5.0, in the menu's order. Variants (writer w94c)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PicturesMode {
    First,
    Second,
    Third,
    Fourth,
    Fifth,
}

/// The Blend control's position s (of 256) after a drag of `pixels_right` pixels from where the tab
/// opens (128; half a unit per pixel; 148 after 40 left, 108 after 40 right). Variants
pub fn pictures_blend_position_v50(pixels_right: i32) -> f64 {
    128.0 - pixels_right as f64 / 2.0
}

/// Apply on the Pictures tab as 5.0 at Blend position s with a menu mode: a is the terrain's height,
/// b the height slot B gives at that point. w = s / 256, u = 1 - w, "cut" is toward zero. The modes
/// are stated for a white slot B (b = 65535); a general b is the same arithmetic with b for 65535
/// (Open for a non-white slot B). s = 128 with the first mode is the halving of the entry.
/// Variants (5.0 Blend positions and menu modes)
pub fn pictures_blend_v50(a_grid: &HeightGrid, b_grid: &HeightGrid, s: f64, mode: PicturesMode) -> HeightGrid {
    let w = s / 256.0;
    let u = 1.0 - w;
    let heights = a_grid
        .heights()
        .iter()
        .zip(b_grid.heights())
        .map(|(&a, &b)| {
            let (a, b) = (a as f64, b as f64);
            let v = match mode {
                PicturesMode::First => (b * u).trunc() + (a * w).trunc(),
                PicturesMode::Second => {
                    let d = (2.0 * u - 1.0).abs();
                    let e = 1.0 - d;
                    let lesser = a.min(b);
                    if u >= 0.5 { (b * d + lesser * e).trunc() } else { (a * d + lesser * e).trunc() }
                }
                PicturesMode::Third => (a - b * u).trunc(),
                PicturesMode::Fourth => (b * u + a).trunc(),
                PicturesMode::Fifth => (a * b * (u * (1.0 / 65535.0)) + a * w).trunc(), // Variants: a b (u c) + a w, c = 1/65535
            };
            limited(v as i64)
        })
        .collect();
    rebuild(a_grid, heights)
}

/// The four items of the Filtering tab's menu below the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMenuItem {
    ApplyVertical,
    ApplyHorizontal,
    ApplyVerticalAdd,
    ApplyHorizontalAdd,
}

/// A menu item at once on the heights, from the graph as it stands (no Apply): the graph value g
/// at the place along the graph of the point's row (Vertical) or column (Horizontal), then the
/// blend (plain items) or the add (Add items). The product is exact in binary64 for the power of
/// two sides the editor has; the side is taken from the grid.
pub fn apply_filter_menu(grid: &HeightGrid, graph: &[u16; GRAPH_LEN], item: FilterMenuItem) -> HeightGrid {
    let n = grid.side();
    let vertical = matches!(item, FilterMenuItem::ApplyVertical | FilterMenuItem::ApplyVerticalAdd);
    let add = matches!(item, FilterMenuItem::ApplyVerticalAdd | FilterMenuItem::ApplyHorizontalAdd);
    let plain_vertical = item == FilterMenuItem::ApplyVertical;
    let g_at = |k: usize| -> i64 {
        let (x, i) = if plain_vertical {
            let x = 128.0 * k as f64 / n as f64;
            (x, x.floor() as usize)
        } else {
            let x = 127.0 * k as f64 / n as f64;
            (x, (x + 0.5).floor() as usize)
        };
        let at = |j: usize| graph[j.min(GRAPH_LEN - 1)] as i64; // G[128] is G[127]
        let (a, b) = (at(i), at(i + 1));
        let product = ((b - a) as f64 * (x - i as f64)).trunc() as i64;
        (a + product).max(0)
    };
    let gs: Vec<i64> = (0..n).map(g_at).collect();
    let heights = grid
        .heights()
        .iter()
        .enumerate()
        .map(|(idx, &v)| {
            let g = gs[if vertical { idx / n } else { idx % n }];
            let v = v as i64;
            if add {
                limited(v + g.div_euclid(2))
            } else {
                limited((v + g.max(v)).div_euclid(2))
            }
        })
        .collect();
    rebuild(grid, heights)
}

/// The clipping bracket's two ends, fractions of the full height range (lower and upper),
/// with the drag arithmetic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bracket {
    pub lower: f64,
    pub upper: f64,
}

/// Which part a press took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BracketPart {
    Lower,
    Upper,
    Middle,
}

/// The least gap between the ends: 0.069 in binary32.
const BRACKET_MIN_GAP: f64 = 0.069f32 as f64;

impl Bracket {
    /// The part a press at `place` (a fraction of the range) takes: the lower end if it is less
    /// than a tenth of the length above the lower end, the upper end if more than that tenth
    /// below the upper end, else the middle.
    pub fn press(&self, place: f64) -> BracketPart {
        let tenth = (self.upper - self.lower) / 10.0;
        if place - self.lower < tenth {
            BracketPart::Lower
        } else if self.upper - place < tenth {
            BracketPart::Upper
        } else {
            BracketPart::Middle
        }
    }

    /// One move of the pointer by `d` pixels (downward positive) on a bracket spanning
    /// `span` pixels: the taken end changes to end - d k with k = 1/span in binary32, the end in
    /// binary64; then the taken end is held (lower within 0 and upper - gap, upper within
    /// lower + gap and 1). A middle drag moves the upper end when the pointer goes up and the
    /// lower end when it goes down; the other end follows by the actual change, unheld (the
    /// measured middle drag ends at 1.00002).
    pub fn drag(&mut self, part: BracketPart, d: i32, span: u32) {
        let k = (1.0f32 / span as f32) as f64;
        let step = d as f64 * k;
        let move_lower = |b: &mut Bracket| -> f64 {
            let before = b.lower;
            b.lower = (b.lower - step).clamp(0.0, (b.upper - BRACKET_MIN_GAP).max(0.0));
            b.lower - before
        };
        let move_upper = |b: &mut Bracket| -> f64 {
            let before = b.upper;
            let mut u = b.upper - step;
            if u < b.lower + BRACKET_MIN_GAP {
                u = b.lower + BRACKET_MIN_GAP;
            }
            if u > 1.0 {
                u = 1.0;
            }
            b.upper = u;
            b.upper - before
        };
        match part {
            BracketPart::Lower => {
                move_lower(self);
            }
            BracketPart::Upper => {
                move_upper(self);
            }
            BracketPart::Middle => {
                if d < 0 {
                    let change = move_upper(self);
                    self.lower += change;
                } else {
                    let change = move_lower(self);
                    self.upper += change;
                }
            }
        }
    }
}
