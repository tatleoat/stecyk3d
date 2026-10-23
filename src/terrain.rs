// SPDX-License-Identifier: MIT
//! Terrain height grids: the grid , a new relief , resizing , a relief
//! between two keys , and the part of the track rule  a relief track needs.

use crate::animation::{ReadMemory, Span, repeat_time};
use crate::numerics::CRand;

/// Smallest side a grid may have.,
pub const MIN_SIDE: usize = 2;
/// Largest side a grid may have, for every version: a reader accepts every size from 2 to
/// 4096 whose heights are present.,
pub const MAX_SIDE: usize = 4096;
/// Largest side a reader keeps (the implementer's choice above 4,096, Limits): the
/// documents of terrain-large hold sides to 8,192, and as 3.0 and 3.1 the original draws 4,100.
pub const MAX_READ_SIDE: usize = 8192;
/// Largest side, old or new, that resizing takes.
pub const MAX_RESIZE_SIDE: usize = 1024;

/// Why a grid could not be made. (Types)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridError {
    /// The side is outside `MIN_SIDE..=MAX_SIDE`.
    SideOutOfRange { side: usize },
    /// The number of heights is not `side * side`.
    WrongLength { side: usize, len: usize },
    /// A byte form whose length is not that of a square grid of 16-bit heights.
    NotSquare { len: usize },
    /// A side, old or new, outside `MIN_SIDE..=MAX_RESIZE_SIDE` for resizing.
    ResizeSideOutOfRange { side: usize },
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridError::SideOutOfRange { side } => {
                write!(f, "grid side {side} is outside {MIN_SIDE} to {MAX_SIDE}")
            }
            GridError::WrongLength { side, len } => {
                write!(f, "a grid of side {side} needs {} heights, got {len}", side * side)
            }
            GridError::NotSquare { len } => {
                write!(f, "{len} bytes are not a square grid of 16-bit heights")
            }
            GridError::ResizeSideOutOfRange { side } => write!(
                f,
                "a grid of side {side} cannot be resized: resizing takes sides {MIN_SIDE} to {MAX_RESIZE_SIDE}"
            ),
        }
    }
}

impl std::error::Error for GridError {}

/// A square grid of 16-bit heights, row after row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeightGrid {
    side: usize,
    heights: Vec<u16>,
}

impl HeightGrid {
    /// A grid from its side and its heights (row after row).
    pub fn new(side: usize, heights: Vec<u16>) -> Result<Self, GridError> {
        check_side(side)?;
        if heights.len() != side * side {
            return Err(GridError::WrongLength { side, len: heights.len() });
        }
        Ok(HeightGrid { side, heights })
    }

    /// A grid of one height everywhere.
    pub fn filled(side: usize, height: u16) -> Result<Self, GridError> {
        Self::new(side, vec![height; side * side])
    }

    /// A grid from 16-bit heights, low byte first, with the side the square root of half
    /// the length (the vectors' file form).
    pub fn from_le_bytes(bytes: &[u8]) -> Result<Self, GridError> {
        let len = bytes.len();
        if len % 2 != 0 {
            return Err(GridError::NotSquare { len });
        }
        let count = len / 2;
        let side = integer_sqrt(count);
        if side * side != count {
            return Err(GridError::NotSquare { len });
        }
        let heights = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        Self::new(side, heights)
    }

    /// The grid as 16-bit heights, low byte first.
    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.heights.iter().flat_map(|h| h.to_le_bytes()).collect()
    }

    /// Points along a side.
    pub fn side(&self) -> usize {
        self.side
    }

    /// The heights, row after row.
    pub fn heights(&self) -> &[u16] {
        &self.heights
    }

    /// The height at a row and column.
    pub fn at(&self, row: usize, col: usize) -> u16 {
        self.heights[row * self.side + col]
    }
}

fn check_side(side: usize) -> Result<(), GridError> {
    if (MIN_SIDE..=MAX_READ_SIDE).contains(&side) {
        Ok(())
    } else {
        Err(GridError::SideOutOfRange { side })
    }
}

fn integer_sqrt(n: usize) -> usize {
    let mut r = (n as f64).sqrt() as usize;
    while r * r > n {
        r -= 1;
    }
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

// ---------------------------------------------------------------------------------------
// Resizing
// ---------------------------------------------------------------------------------------

/// A grid brought to `new_side` points a side by bilinear interpolation, truncated.
pub fn resize(grid: &HeightGrid, new_side: usize) -> Result<HeightGrid, GridError> {
    let old = grid.side;
    for side in [old, new_side] {
        if !(MIN_SIDE..=MAX_RESIZE_SIDE).contains(&side) {
            return Err(GridError::ResizeSideOutOfRange { side });
        }
    }
    if old == new_side {
        return Ok(grid.clone());
    }
    // Per output index: the whole part, the next index (held at the last), the fraction.
    let places: Vec<(usize, usize, f64)> = (0..new_side)
        .map(|a| {
            // (place a * (old - 1) / (new - 1); its whole part is the exact
            // quotient; the fraction is the binary64 place less its whole part, which the
            // vectors require rather than the remainder over new - 1)
            let place = (a * (old - 1)) as f64 / (new_side - 1) as f64;
            let whole = place as usize;
            let frac = place - whole as f64;
            let next = if whole + 1 < old { whole + 1 } else { whole };
            (whole, next, frac)
        })
        .collect();
    let mut out = Vec::with_capacity(new_side * new_side);
    for &(r0, r1, fr) in &places {
        for &(c0, c1, fc) in &places {
            let h00 = grid.at(r0, c0) as f64;
            let h01 = grid.at(r0, c1) as f64;
            let h10 = grid.at(r1, c0) as f64;
            let h11 = grid.at(r1, c1) as f64;
            let top = h00 + (h01 - h00) * fc;
            let bottom = h10 + (h11 - h10) * fc;
            let v = top + (bottom - top) * fr;
            out.push(v as u16);
        }
    }
    HeightGrid::new(new_side, out)
}

// ---------------------------------------------------------------------------------------
// Between two keys
// ---------------------------------------------------------------------------------------

/// Fractional bits of the place scaling.
const PLACE_FRACTION_BITS: u32 = 8;
/// The part u is taken in 65536ths.
const PART_ONE: f64 = 65536.0;

/// The relief the part `u` of the way from `first` to `second`, at the larger size.
pub fn between(first: &HeightGrid, second: &HeightGrid, u: f64) -> HeightGrid {
    let side = first.side.max(second.side);
    let u = if u.is_finite() { u } else { 0.0 };
    let part = crate::numerics::to_i32(u * PART_ONE) as i64;
    let mut out = Vec::with_capacity(side * side);
    for row in 0..side {
        for col in 0..side {
            let a = sample_scaled(first, side, row, col) as i64;
            let b = sample_scaled(second, side, row, col) as i64;
            // ; the vectors require the cut
            // toward minus infinity (an arithmetic shift), not toward zero.
            let step = ((b - a) * part) >> 16;
            out.push((a + step) as u16);
        }
    }
    HeightGrid { side, heights: out }
}

/// The height of `grid` at the place of (`row`, `col`) of a grid of `side`, scaled in
/// 8-bit fixed point.
fn sample_scaled(grid: &HeightGrid, side: usize, row: usize, col: usize) -> u16 {
    if grid.side == side {
        return grid.at(row, col);
    }
    let scale = (grid.side << PLACE_FRACTION_BITS) / side;
    let r = (row * scale) >> PLACE_FRACTION_BITS;
    let c = (col * scale) >> PLACE_FRACTION_BITS;
    grid.at(r.min(grid.side - 1), c.min(grid.side - 1))
}

// ---------------------------------------------------------------------------------------
// A relief track:'s reading of the time (component animation)
// ---------------------------------------------------------------------------------------

pub use crate::animation::Repeat;

/// One key of a relief track.,
#[derive(Debug, Clone)]
pub struct ReliefKey {
    pub time: f64,
    pub grid: HeightGrid,
}

/// The relief of a track at time `t` (without a time map), or `None` for a track with no
/// key (the owner keeps its grid).
///
/// (the repeat map, and the key at or before the time with the key after it;
/// setting 1's last stretch ends on the first key), (answer 1001: a time on the
/// last key gives its grid unchanged; every other time goes through with its part u,
/// even u = 0)
pub fn relief_at(keys: &[ReliefKey], repeat: Repeat, t: f64) -> Option<HeightGrid> {
    relief_after(keys, repeat, &mut ReadMemory::new(), t)
}

/// The relief of a track at time `t`, given what earlier reads of the same track left in
/// `memory`, which this read updates. A relief track takes the key at s as every other
/// track does.
///
/// ; revised : u is's quotient however close
/// the two keys lie (no distance limit), and a relief track takes no time map (our choice,
pub fn relief_after(
    keys: &[ReliefKey],
    repeat: Repeat,
    memory: &mut ReadMemory,
    t: f64,
) -> Option<HeightGrid> {
    let first = keys.first()?;
    let last = keys.last()?;
    let s = repeat_time(first.time, last.time, repeat, t);
    let times: Vec<f64> = keys.iter().map(|k| k.time).collect();
    match memory.locate(&times, repeat, s) {
        Span::AtLast { index } => Some(keys[index].grid.clone()),
        Span::Between { from, to_value, u, .. } => Some(between(&keys[from].grid, &keys[to_value].grid, u)),
    }
}

// ---------------------------------------------------------------------------------------
// A new relief
// ---------------------------------------------------------------------------------------

/// Side of the coarse random grid.
const COARSE_SIDE: usize = 8;
/// Side of the finished relief.
pub const NEW_RELIEF_SIDE: usize = 128;
/// The largest draw of the generator.,
const DRAW_MAX: u32 = 32767;
/// The largest height.
const HEIGHT_MAX: u32 = 65535;
/// The displacement amplitude at the grid of 16.
const FIRST_AMPLITUDE: u32 = 32767;
/// The side at which the bell comes before the displacement.
const BELLED_SIDE: usize = 32;
/// The bell's factor.
const BELL_FACTOR: f64 = 6.4;

/// A new 128 by 128 relief, drawing its random numbers from `rng`.
pub fn new_relief(rng: &mut CRand) -> HeightGrid {
    let mut stages = new_relief_stages(rng);
    stages.pop().expect("the stages end with the finished relief").1
}

/// One stage of a new relief, named for what was last done to the grid.
/// (stages)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliefStage {
    /// The 8 by 8 grid of random heights.
    Coarse,
    /// Resized  to the grid's side.
    Stretched,
    /// After the bell (on the grid of 32, or on the finished grid).
    Belled,
    /// After the displacement and the zeroing of the border.
    Displaced,
    /// After the final spreading: the finished relief.
    Spread,
}

/// A new relief with the grid after every stage, in order: coarse 8; for 16, 32, 64 and
/// 128 stretched, (at 32 belled), displaced; belled 128; spread 128. The last is the
/// relief `new_relief` returns.
pub fn new_relief_stages(rng: &mut CRand) -> Vec<(ReliefStage, HeightGrid)> {
    let mut stages = Vec::with_capacity(12);
    // point 1 (64 draws, row after row)
    let coarse: Vec<u16> = (0..COARSE_SIDE * COARSE_SIDE).map(|_| coarse_height(rng)).collect();
    let mut grid = HeightGrid::new(COARSE_SIDE, coarse).expect("8 is a valid side");
    stages.push((ReliefStage::Coarse, grid.clone()));
    let mut amplitude = FIRST_AMPLITUDE;
    let mut side = COARSE_SIDE;
    while side < NEW_RELIEF_SIDE {
        side *= 2; // points 2 to 4 (16, 32, 64, 128)
        grid = resize(&grid, side).expect("valid side");
        stages.push((ReliefStage::Stretched, grid.clone()));
        if side == BELLED_SIDE {
            apply_bell(&mut grid); // point 4 (bell before the displacement at 32)
            stages.push((ReliefStage::Belled, grid.clone()));
        }
        displace(&mut grid, amplitude, rng);
        zero_border(&mut grid);
        stages.push((ReliefStage::Displaced, grid.clone()));
        amplitude /= 2; // point 3 (halved in whole numbers)
    }
    apply_bell(&mut grid); // point 5 (on the finished grid)
    stages.push((ReliefStage::Belled, grid.clone()));
    spread(&mut grid);
    stages.push((ReliefStage::Spread, grid));
    stages
}

/// A coarse height: r * 65535 / 32767 rounded, a half going up. point 1
fn coarse_height(rng: &mut CRand) -> u16 {
    let r = rng.next_u15() as u32;
    round_ratio(r as u64 * HEIGHT_MAX as u64, DRAW_MAX as u64) as u16
}

/// n / d rounded to the nearest whole number, a half going up (n, d not negative).
fn round_ratio(n: u64, d: u64) -> u64 {
    (2 * n + d) / (2 * d)
}

/// The displacement with amplitude `a`: one draw per point, row after row, border
/// included. point 3
fn displace(grid: &mut HeightGrid, a: u32, rng: &mut CRand) {
    let a = a as u64;
    let half_up = a - a / 2; // point 3 (a minus a / 2, cut off)
    for h in grid.heights.iter_mut() {
        let r = rng.next_u15() as u64;
        let hv = *h as u64;
        let reach = ((hv * a / HEIGHT_MAX as u64 + half_up) / 2).max(1); // point 3
        let d = round_ratio(2 * reach * r, DRAW_MAX as u64); // point 3 (D)
        let moved = hv as i64 + d as i64 - reach as i64;
        *h = moved.clamp(0, HEIGHT_MAX as i64) as u16; // point 3 (held, not wrapped)
    }
}

/// Multiplies each height by exp(-6.4 d^2 / s^2), d from whole point s / 2 in both
/// directions, and cuts the product off. point 5
fn apply_bell(grid: &mut HeightGrid) {
    let side = grid.side;
    let middle = (side / 2) as i64; // point 5 (s / 2, cut off)
    let s2 = (side * side) as f64; // point 5 (s itself, not s - 1)
    for row in 0..side {
        for col in 0..side {
            let dr = row as i64 - middle;
            let dc = col as i64 - middle;
            let d2 = (dr * dr + dc * dc) as f64; // point 5 (whole numbers)
            let factor = (-BELL_FACTOR * d2 / s2).exp();
            let h = &mut grid.heights[row * side + col];
            *h = (*h as f64 * factor) as u16; // point 5 (cut off)
        }
    }
}

/// Sets the first and last rows and columns to zero. point 3
fn zero_border(grid: &mut HeightGrid) {
    let side = grid.side;
    let h = &mut grid.heights;
    for i in 0..side {
        h[i] = 0;
        h[(side - 1) * side + i] = 0;
        h[i * side] = 0;
        h[i * side + side - 1] = 0;
    }
}

/// The final spreading: (v - L - 1) * 65535 / (H - L) cut toward zero, held to 0..65535;
/// all heights equal give 0 everywhere. point 6
fn spread(grid: &mut HeightGrid) {
    let lo = *grid.heights.iter().min().expect("a grid has points") as i64;
    let hi = *grid.heights.iter().max().expect("a grid has points") as i64;
    for h in grid.heights.iter_mut() {
        *h = if hi == lo {
            0
        } else {
            let v = (*h as i64 - lo - 1) * HEIGHT_MAX as i64 / (hi - lo); // cut toward zero
            v.clamp(0, HEIGHT_MAX as i64) as u16
        };
    }
}
