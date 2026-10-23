// SPDX-License-Identifier: MIT
//! The shape tools of the terrain editor: Basic Noise, Posterize, Mosaic and the geometry tools of
//! the green curve (Raise Edges, Blob Maker, Bubble Ridges, Cross Ridges, Cross Ridges 2), Relief
//! Noise and Subplateaus.
//! C9 (the shape tools), C10 (Relief Noise), C11 (Subplateaus) and Variants (as
//! 5.0 Subplateaus' passes come in pairs); rule 8 (the amount form for a drag over a tool
//! that takes it: `terrain_editor::at_amount` with the click's heights as f).

use crate::terrain::HeightGrid;
use crate::terrain_editor::posterize;
use crate::terrain_tools::{grid_of, mound_step_d, smooth_heights, Session};

/// The tools of C9 to C11. C9, C10, C11
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeTool {
    BasicNoise,
    Posterize,
    Mosaic,
    RaiseEdges,
    BlobMaker,
    BubbleRidges,
    CrossRidges,
    CrossRidges2,
    ReliefNoise,
    Subplateaus,
}

fn limited(v: i64) -> u16 {
    v.clamp(0, 65535) as u16
}

/// Whether C9's tools and C10's work at a side: a power of two from 8 to 1024 (5.0: up to 4096);
/// at any other side the heights are left as they were.
pub fn side_works(n: usize, five: bool) -> bool {
    n.is_power_of_two() && n >= 8 && n <= if five { 4096 } else { 1024 }
}

/// The whole-number square root, cut down. C9 (e)
fn isqrt(v: i64) -> i64 {
    let mut r = (v as f64).sqrt() as i64;
    while r * r > v {
        r -= 1;
    }
    while (r + 1) * (r + 1) <= v {
        r += 1;
    }
    r
}

/// The result of `tool` at the pointer's distances `d` (horizontal, right positive) and `e`
/// (vertical, up positive) from the press place, from the heights `grid` as they were at the
/// press; at the press, and for a click, d = e = 0. Basic Noise takes n * n numbers of `session`
/// every time it is called (when the side works). `five` says the program is 5.0 (Subplateaus' passes
/// and the sides). Relief Noise and Subplateaus do not read d and e. C9, C10, C11
pub fn apply(grid: &HeightGrid, tool: ShapeTool, d: i64, e: i64, five: bool, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    if tool == ShapeTool::Subplateaus {
        return subplateaus(grid, five, session);
    }
    if !side_works(n, five) {
        return grid.clone();
    }
    match tool {
        ShapeTool::BasicNoise => basic_noise(grid, d, session),
        // C9 (b): s = 16 |d|, 1 when d = 0
        ShapeTool::Posterize => posterize(grid, if d == 0 { 1 } else { (16 * d.abs()) as u32 }),
        ShapeTool::Mosaic => mosaic(grid, d),
        ShapeTool::ReliefNoise => relief_noise(grid),
        _ => centred(grid, tool, d, e),
    }
}

/// A drag: the press computes the tool's result at d = e = 0 and every reading of the pointer
/// computes it again from the heights before the press; the result of the last reading is kept
/// (a press with no reading is a click). Every pass takes its numbers in turn (Basic Noise: n * n
/// each). C9 (the first paragraph), (a)
pub fn drag(grid: &HeightGrid, tool: ShapeTool, readings: &[(i64, i64)], five: bool, session: &mut Session) -> HeightGrid {
    let mut result = apply(grid, tool, 0, 0, five, session);
    for &(d, e) in readings {
        result = apply(grid, tool, d, e, five, session);
    }
    result
}

/// Basic Noise: every point, in storage order, takes the next number r and becomes
/// limited(cut toward zero(v + 2 E r / 32767 + 0.5 - E)), E = 16 |d|, in binary64 in the order of
/// C6's level. C9 (a), Outputs (3)
fn basic_noise(grid: &HeightGrid, d: i64, session: &mut Session) -> HeightGrid {
    let big_e = (16 * d.abs()) as f64;
    let out = grid
        .heights()
        .iter()
        .map(|&v| {
            let r = session.next_number();
            let x = ((2.0 * big_e) * (r as f64 * (1.0 / 32767.0)) + 0.5) - big_e + v as f64;
            limited(x.trunc() as i64)
        })
        .collect();
    grid_of(grid.side(), out)
}

/// Mosaic: q = |d|, 1 when d = 0; each point takes the height of the point at row
/// q cut down(r / q) and column q cut down(c / q). C9 (c)
fn mosaic(grid: &HeightGrid, d: i64) -> HeightGrid {
    let n = grid.side();
    let q = if d == 0 { 1 } else { d.unsigned_abs() as usize };
    let mut out = Vec::with_capacity(n * n);
    for r in 0..n {
        for c in 0..n {
            out.push(grid.at((q * (r / q)).min(n - 1), (q * (c / q)).min(n - 1)));
        }
    }
    grid_of(n, out)
}

/// Raise Edges, Blob Maker, Bubble Ridges, Cross Ridges and Cross Ridges 2: the centre column
/// c0 = h + d and row r0 = h + e (Raise Edges: both h), dc = |c - c0| and dr = |r - r0|, h = n / 2.
/// C9 (d), (e), (f)
fn centred(grid: &HeightGrid, tool: ShapeTool, d: i64, e: i64) -> HeightGrid {
    let n = grid.side();
    let ni = n as i64;
    let h = ni / 2;
    let (c0, r0) = if tool == ShapeTool::RaiseEdges { (h, h) } else { (h + d, h + e) };
    let big_r = match tool {
        ShapeTool::BlobMaker => h - d,
        ShapeTool::BubbleRidges => h,
        _ => c0.min(ni - c0).min(r0).min(ni - r0),
    };
    let mut out = Vec::with_capacity(n * n);
    for r in 0..ni {
        for c in 0..ni {
            let v = grid.at(r as usize, c as usize) as i64;
            let (dc, dr) = ((c - c0).abs(), (r - r0).abs());
            let gap = 65535 - v;
            let x = match tool {
                ShapeTool::RaiseEdges => {
                    let w = (((h.min(dc.max(dr))) * 4 * d).div_euclid(h)).clamp(0, h);
                    v + (gap * w).div_euclid(h)
                }
                ShapeTool::BlobMaker | ShapeTool::BubbleRidges => {
                    let a = isqrt(dr * dr + dc * dc).max(0).min(big_r);
                    v + ((big_r - a) * gap).div_euclid(h)
                }
                ShapeTool::CrossRidges => {
                    let m = dc.min(dr).max(0).min(big_r);
                    v + (gap * m).div_euclid(h)
                }
                _ => {
                    let m = dc.min(dr).max(0).min(big_r);
                    v + ((big_r - m) * gap).div_euclid(h)
                }
            };
            out.push(limited(x));
        }
    }
    grid_of(n, out)
}

/// Relief Noise, a click: every point of columns 1 to n - 2 becomes limited(cut down(sum of
/// weight * height / 2048)) over its 3 by 3 neighbours (above row 0 and below row n - 1 the edge
/// row itself); columns 0 and n - 1 keep their heights. No numbers. C10
fn relief_noise(grid: &HeightGrid) -> HeightGrid {
    const ABOVE: [i64; 3] = [2330, 258, -1147]; // C10
    const SAME: [i64; 3] = [924, -4474, 925]; // C10
    const BELOW: [i64; 3] = [1147, 258, 2330]; // C10
    let n = grid.side();
    let mut out = grid.heights().to_vec();
    for r in 0..n {
        let rows = [(r.saturating_sub(1), ABOVE), (r, SAME), ((r + 1).min(n - 1), BELOW)];
        for c in 1..n - 1 {
            let mut sum = 0i64;
            for (rr, weights) in rows {
                for (k, w) in weights.iter().enumerate() {
                    sum += w * grid.at(rr, c + k - 1) as i64;
                }
            }
            out[r * n + c] = limited(sum.div_euclid(2048));
        }
    }
    grid_of(n, out)
}

/// Subplateaus, a click: four passes of C5's step (d) (as 5.0 the passes come in pairs, two up to
/// side 128 and two more for each doubling above it), each on the result of the one before with U
/// the heights, then the smoothing S once. C11
fn subplateaus(grid: &HeightGrid, five: bool, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    if n < 4 {
        return grid.clone();
    }
    let passes = if !five {
        4
    } else {
        let mut side = 128;
        let mut passes = 2;
        while side < n {
            side *= 2;
            passes += 2;
        }
        passes
    };
    let mut h = grid.heights().to_vec();
    for _ in 0..passes {
        let u: Vec<i64> = h.iter().map(|&v| v as i64).collect();
        h = mound_step_d(&h, &u, n, session);
    }
    smooth_heights(&mut h, n, session.range);
    grid_of(n, h)
}
