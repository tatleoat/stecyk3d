// SPDX-License-Identifier: MIT
//! The terrain editor's tools with stated random rules, as functions on a grid and a session.
//! C1 (Sawtooth), C2 (Subcontours), C3 (Spikes), C4 (Slope Noise), C5 (Mounds),
//! C6 (Fractal); C7 (Eroded), C8 (Erode) .
//! R1 (the session's numbers: `HeightNoiseSource`), R2 is in terrain_editor.rs.
//! rule 8 (5.0 drags),

use crate::terrain::{resize, HeightGrid};
use crate::terrain_clip::ClipRange;
use crate::terrain_editor::HeightNoiseSource;

/// The session of numbers. R1
pub type Session = HeightNoiseSource;

fn limited(v: i64) -> u16 {
    v.clamp(0, 65535) as u16
}

pub(crate) fn grid_of(side: usize, heights: Vec<u16>) -> HeightGrid {
    HeightGrid::new(side, heights).expect("a side the callers keep within TE-3's limits")
}

fn resized(heights: &[u16], side: usize, to: usize) -> Vec<u16> {
    if side == to {
        return heights.to_vec();
    }
    resize(&grid_of(side, heights.to_vec()), to).expect("TE-3 side").heights().to_vec()
}

/// x = cut toward zero((R (1/32767) - 0.5) 65535) in binary64. C3 (also C5)
fn chance_x(r: i64) -> i64 {
    ((r as f64 * (1.0 / 32767.0) - 0.5) * 65535.0).trunc() as i64
}

/// The smoothing S: along each row, then along each column of the result. Along a line of
/// length n, points 1 to n - 3 become floor((left + 2 self + right) / 4), point n - 2 becomes
/// floor((left + 3 self) / 4), points 0 and n - 1 stay; every value is of the line before the pass.
/// C2
/// S with the clipping range read: in each of its two passes a value below lo or above hi is left
/// as it is.
fn smooth_range(values: &mut [i64], n: usize, range: ClipRange) {
    let line = |line: &mut Vec<i64>| {
        let old = line.clone();
        for i in 1..n.saturating_sub(2) {
            if range.holds(old[i]) {
                line[i] = (old[i - 1] + 2 * old[i] + old[i + 1]).div_euclid(4);
            }
        }
        if n >= 3 && range.holds(old[n - 2]) {
            line[n - 2] = (old[n - 3] + 3 * old[n - 2]).div_euclid(4);
        }
    };
    for r in 0..n {
        let mut l: Vec<i64> = values[r * n..(r + 1) * n].to_vec();
        line(&mut l);
        values[r * n..(r + 1) * n].copy_from_slice(&l);
    }
    for c in 0..n {
        let mut l: Vec<i64> = (0..n).map(|r| values[r * n + c]).collect();
        line(&mut l);
        for r in 0..n {
            values[r * n + c] = l[r];
        }
    }
}

pub(crate) fn smooth_heights(h: &mut [u16], n: usize, range: ClipRange) {
    let mut v: Vec<i64> = h.iter().map(|&x| x as i64).collect();
    smooth_range(&mut v, n, range);
    for (d, s) in h.iter_mut().zip(v) {
        *d = s as u16;
    }
}

/// The stretch to the full range: if hi != lo and hi > 0, limited(cut toward zero((v - lo) 65535
/// / (hi - lo))) in binary64. C6
fn stretch_full(h: &mut [u16]) {
    let lo = *h.iter().min().unwrap();
    let hi = *h.iter().max().unwrap();
    if hi != lo && hi > 0 {
        for v in h.iter_mut() {
            let x = ((*v - lo) as f64 * 65535.0) / (hi - lo) as f64;
            *v = limited(x.trunc() as i64);
        }
    }
}

/// Sawtooth, a click: sixteen waves of three numbers a, b, d each. C1
pub fn sawtooth(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let mut h: Vec<i64> = grid.heights().iter().map(|&v| v as i64).collect();
    for _ in 0..16 {
        let a = session.next_number();
        let b = session.next_number();
        let d = session.next_number();
        for r in 0..n {
            for c in 0..n {
                let t = d * a + r as i64 * b + c as i64 * a;
                let m = (t / n as i64) % 32767;
                let wave = (m - 16383).div_euclid(2);
                let v = &mut h[r * n + c];
                *v = (*v + wave).clamp(0, 65535);
            }
        }
    }
    grid_of(n, h.into_iter().map(|v| v as u16).collect())
}

/// Subcontours, a click, as 3.0 and 3.1: a mask of the heights zeroed where neighbours differ by
/// more than 960, smoothed by S; new = M + cut toward zero(40 (v - M) / 100). No numbers.
/// C2
pub fn subcontours(grid: &HeightGrid) -> HeightGrid {
    subcontours_in(grid, false, ClipRange::FULL)
}

/// Subcontours with the version's mask and the clipping range read by S. As 5.0 (code, then
/// observed) the mask of a point is set to 0 when the height of its right neighbour minus that of
/// its left neighbour is more than t or below 0, and likewise the height below minus the height
/// above, with t = cut toward zero(5.5 n) (704 at n = 128) in place of C2's 960 either way; the
/// smoothing and the last step are C2's. C2, Variants (5.0's Subcontours),
pub fn subcontours_in(grid: &HeightGrid, five: bool, range: ClipRange) -> HeightGrid {
    let n = grid.side();
    let g = |r: usize, c: usize| grid.at(r, c) as i64;
    let t = (5.5 * n as f64).trunc() as i64; // Variants (5.0's Subcontours: t = 5.5 n)
    let zeroed = |left: i64, right: i64| -> bool {
        if five { right - left > t || right - left < 0 } else { (left - right).abs() > 960 }
    };
    let mut m: Vec<i64> = grid.heights().iter().map(|&v| v as i64).collect();
    for r in 0..n {
        for c in 1..n - 1 {
            if zeroed(g(r, c - 1), g(r, c + 1)) {
                m[r * n + c] = 0;
            }
        }
    }
    for r in 1..n - 1 {
        for c in 0..n {
            if zeroed(g(r - 1, c), g(r + 1, c)) {
                m[r * n + c] = 0;
            }
        }
    }
    smooth_range(&mut m, n, range);
    let out = (0..n * n)
        .map(|i| {
            let v = grid.heights()[i] as i64;
            limited(m[i] + 40 * (v - m[i]) / 100)
        })
        .collect();
    grid_of(n, out)
}

/// Spikes, a click: blocks on a lattice, each taking one number; the field F is added where the
/// local curvature is small. C3
pub fn spikes(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let mut f = vec![0i64; n * n];
    if n > 7 {
        let k = (n - 5) / 3;
        for i in 0..k {
            for j in 0..k {
                let x = chance_x(session.next_number());
                let h: i64 = if x > 28000 {
                    65535
                } else if x > 16000 {
                    63487
                } else {
                    0
                };
                let row = 1 + 3 * i;
                let col = 1 + 3 * j + if x < 0 { 0 } else { (x % 3) as usize };
                let half = h / 2;
                f[row * n + col] = h;
                f[row * n + col + 1] = half;
                f[row * n + col - 1] = half;
                f[(row + 1) * n + col] = half;
                f[(row - 1) * n + col] = half;
            }
        }
    }
    let g = |r: usize, c: usize| grid.at(r, c) as i64;
    let mut out = vec![0u16; n * n];
    for r in 0..n {
        for c in 0..n {
            out[r * n + c] = if r == n - 1 || c == n - 1 {
                f[r * n + c] as u16
            } else {
                let k = (g(r, c + 1) - 2 * g(r, c) + g(r + 1, c)).abs();
                if k < 1024 {
                    limited(g(r, c) + 16384 * f[r * n + c] / 65535)
                } else {
                    g(r, c) as u16
                }
            };
        }
    }
    grid_of(n, out)
}

/// Slope Noise, a click: noise scaled by the smoothed slope sum; one number a point.
/// C4
pub fn slope_noise(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let at = |r: i64, c: i64| -> i64 {
        let r = r.clamp(0, n as i64 - 1) as usize;
        let c = c.clamp(0, n as i64 - 1) as usize;
        grid.at(r, c) as i64
    };
    let mut s = vec![0i64; n * n];
    for r in 0..n as i64 {
        for c in 0..n as i64 {
            let sum = (at(r, c - 1) - at(r, c + 1)).abs()
                + (at(r - 1, c) - at(r + 1, c)).abs()
                + (at(r - 1, c - 1) - at(r + 1, c + 1)).abs()
                + (at(r + 1, c - 1) - at(r - 1, c + 1)).abs();
            s[r as usize * n + c as usize] = sum % 65536;
        }
    }
    smooth_range(&mut s, n, session.range);
    let out = (0..n * n)
        .map(|i| {
            let rnd = session.next_number();
            let sv = s[i] as f64;
            let a = rnd as f64 * (1.0 / 32767.0);
            let b = a * (2.0 * sv);
            let c = b + 0.5;
            let d = c - sv;
            let e = d * 4096.0;
            let f = e * (1.0 / 65535.0);
            limited(grid.heights()[i] as i64 + f.trunc() as i64)
        })
        .collect();
    grid_of(n, out)
}

/// One mound pass P(f) at side m on heights v. C5
fn mound_pass(v: &[u16], m: usize, f: f64, session: &mut Session) -> Vec<u16> {
    // (a) the seed field T
    let mut t = vec![0i64; m * m];
    if m > 10 {
        let k = (m - 8) / 3;
        for i in 0..k {
            for j in 0..k {
                let x = chance_x(session.next_number());
                if x <= 16000 {
                    continue;
                }
                let e = x % 512;
                let h = if x > 28000 { 65535 - e } else { 512 - e };
                let o = x % 3;
                let y = 5 + 3 * i as i64;
                let xx = 5 + 3 * j as i64 + o;
                let cells: &[(i64, i64)] = match o {
                    0 => &[(y, xx - 1), (y, xx), (y, xx + 1), (y + 1, xx), (y + 1, xx + 1), (y - 1, xx - 1), (y - 1, xx)],
                    1 => &[
                        (y + 1, xx - 1),
                        (y + 1, xx),
                        (y + 1, xx + 1),
                        (y + 1, xx + 2),
                        (y + 2, xx),
                        (y + 2, xx + 1),
                        (y, xx),
                        (y, xx + 1),
                    ],
                    _ => &[(y, xx - 1), (y, xx), (y, xx + 1), (y + 1, xx - 1), (y + 1, xx), (y + 1, xx + 1), (y - 1, xx)],
                };
                for &(rr, cc) in cells {
                    t[rr as usize * m + cc as usize] = h;
                }
            }
        }
    }
    // (b) smoothed; (c) U = max(cut toward zero(T f), v)
    smooth_range(&mut t, m, session.range);
    let u: Vec<i64> = (0..m * m).map(|i| ((t[i] as f64 * f).trunc() as i64).max(v[i] as i64)).collect();
    mound_step_d(v, &u, m, session)
}

/// Step (d) of C5's pass at side m: every point (r, c) with 1 <= r, c <= m - 2, in storage order,
/// takes one number and writes the new height one column to its left from U (here `u`); the
/// points it does not write (rows 0 and m - 1, columns m - 2 and m - 1) keep `base`.
/// C5 (d), C11
pub(crate) fn mound_step_d(base: &[u16], u: &[i64], m: usize, session: &mut Session) -> Vec<u16> {
    let mut out = base.to_vec();
    for r in 1..m - 1 {
        for c in 1..m - 1 {
            let x = chance_x(session.next_number());
            let c2 = if x < -16000 {
                c - 1
            } else if x < 0 {
                c + 1
            } else if x <= 16000 {
                c - 1
            } else {
                c + 1
            };
            let uu = |rr: usize, cc: usize| u[rr * m + cc];
            let best = if x < 0 {
                uu(r - 1, c).max(uu(r, c)).max(uu(r - 1, c2)).max(uu(r, c2))
            } else {
                uu(r + 1, c).max(uu(r, c)).max(uu(r + 1, c2)).max(uu(r, c2))
            };
            out[r * m + c - 1] = (best - 12).max(0) as u16;
        }
    }
    out
}

/// Mounds, a click. C5
pub fn mounds(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    if n <= 64 {
        return grid_of(n, mound_pass(grid.heights(), n, 0.5, session));
    }
    let mut side = 64;
    let mut cur = resized(grid.heights(), n, side);
    let mut f = 0.5;
    loop {
        cur = mound_pass(&cur, side, f, session);
        f /= 2.0;
        if side >= n {
            break;
        }
        let next = (side * 2).min(n);
        cur = resized(&cur, side, next);
        side = next;
    }
    grid_of(n, cur)
}

/// One level's run of Fractal at side m. C6
fn fractal_level(h: &mut [u16], m: usize, reach: i64, session: &mut Session) {
    for v in h.iter_mut() {
        let r = session.next_number();
        let p = (*v as i64 * reach) as i32 as i64; // a 32-bit signed product, wrapping
        let mut a = ((p / 65535) + reach - reach / 2).div_euclid(2);
        if a < 2 {
            a = 1;
        }
        let x = (((2 * a) as f64 * (r as f64 * (1.0 / 32767.0)) + 0.5) - a as f64) + *v as f64;
        *v = limited(x.trunc() as i64);
    }
    for i in 0..m {
        h[i] = 0;
        h[(m - 1) * m + i] = 0;
        h[i * m] = 0;
        h[i * m + m - 1] = 0;
    }
}

/// Fractal, a click. C6
pub fn fractal(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let mut side = 16;
    let mut cur = resized(grid.heights(), n, side);
    let mut reach = 65535i64;
    loop {
        fractal_level(&mut cur, side, reach, session);
        reach /= 2;
        if side >= n {
            break;
        }
        cur = resized(&cur, side, side * 2);
        side *= 2;
    }
    stretch_full(&mut cur);
    grid_of(side, resized(&cur, side, n))
}

/// The droplet run D(count, rate) at side m on a grid, each droplet seeing what the ones before
/// wrote. Two numbers place a droplet; each step takes the numbers C8 says. C8
pub fn droplet_run(h: &mut [u16], m: usize, count: usize, rate: i64, session: &mut Session) {
    const DX: [i64; 8] = [1, 1, 0, -1, -1, -1, 0, 1];
    const DY: [i64; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
    const MAX_POINTS: usize = 320; // C8 (Open reading: points, not steps)
    let mi = m as i64;
    let spot = |x: usize, y: usize, d: usize| -> (usize, usize) {
        (((x as i64 + DX[d]).rem_euclid(mi)) as usize, ((y as i64 + DY[d]).rem_euclid(mi)) as usize)
    };
    for _ in 0..count {
        let r1 = session.next_number() as usize;
        let r2 = session.next_number() as usize;
        let (x0, y0) = (2 + r1 % (m - 4), 2 + r2 % (m - 4));
        let mut path: Vec<(usize, usize, i64)> = vec![(x0, y0, h[y0 * m + x0] as i64)];
        let mut dir = 2usize;
        let mut flat = 0i64;
        loop {
            let (x, y, hh) = *path.last().unwrap();
            let height = |d: usize| {
                let (nx, ny) = spot(x, y, d);
                h[ny * m + nx] as i64
            };
            let mut best = height(dir);
            for d in 0..8 {
                if d % 2 == 1 && session.next_number() >= 6540 {
                    continue;
                }
                if height(d) < best {
                    dir = d;
                    best = height(d);
                }
            }
            if session.next_number() < 3270 {
                dir = (dir + 1) % 8;
            } else if session.next_number() < 3270 {
                dir = (dir + 7) % 8;
            }
            let hp = height(dir);
            flat += if hp == hh { 1 } else { -1 };
            if path.len() < MAX_POINTS && hp <= hh && flat <= 5 && hp > session.range.lo && hp <= session.range.hi {
                let (nx, ny) = spot(x, y, dir);
                path.push((nx, ny, hp));
            } else {
                break;
            }
        }
        let k = path.len() - 1;
        if k <= 8 {
            continue;
        }
        let e = path[k].2;
        let half = k / 2;
        let mut hs: Vec<i64> = path.iter().map(|p| p.2).collect();
        let mut w = 64i64;
        for j in 0..half {
            let mut t = w.div_euclid(64);
            if t > 255 {
                t = 256;
            }
            hs[j] = (hs[j] - 32 * t).max(e);
            w += rate;
        }
        for j in half..k {
            hs[j] = (hs[j] - 32 * w.div_euclid(64)).max(e);
            w -= rate;
        }
        for _ in 0..2 {
            let old = hs.clone();
            for j in 0..=k {
                let a = old[(j + 1).min(k)];
                let b = old[(j + 2).min(k)];
                hs[j] = (2 * old[j] + a + b).div_euclid(4);
            }
        }
        for (j, p) in path.iter().enumerate() {
            h[p.1 * m + p.0] = limited(hs[j]);
        }
        if k > 20 {
            for j in 10..=k {
                let mut w = (j - 10) as f64 * (1.0 / 54.0);
                if w >= 0.75 {
                    w = 0.75;
                }
                let (mut x, mut y) = (path[j].0 as i64, path[j].1 as i64);
                let steps: [(i64, i64); 4] = [(1, 0), (-2, 0), (1, 1), (0, -2)];
                for (dx, dy) in steps {
                    x = (x + dx).clamp(0, mi - 1);
                    y = (y + dy).clamp(0, mi - 1);
                    let cell = &mut h[y as usize * m + x as usize];
                    let g = *cell as i64;
                    let nv = g as f64 + (hs[j] - g) as f64 * w;
                    *cell = limited(nv.trunc() as i64);
                }
            }
        }
    }
}

/// Erode, a click: a run of 1500 droplets on a copy that is dropped (it takes numbers), then
/// `repeats` runs of 500 on the grid (1 for a click; more while the button stays down).
/// C8
pub fn erode(grid: &HeightGrid, repeats: usize, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let mut copy = grid.heights().to_vec();
    droplet_run(&mut copy, n, 1500, 24, session);
    let mut h = grid.heights().to_vec();
    for _ in 0..repeats {
        droplet_run(&mut h, n, 500, 24, session);
    }
    grid_of(n, h)
}

/// A drag over Eroded to the amount P: the press runs C7 once (its numbers taken once), and
/// every point becomes limited(cut down(v + P·(f − v)/256)), v the height before the press and
/// f the height after C7; the drag does not run C7 again.
pub fn eroded_at_amount(grid: &HeightGrid, p: i64, session: &mut Session) -> HeightGrid {
    let after = eroded(grid, session);
    crate::terrain_editor::at_amount(grid, &after, p)
}

/// The tools that take the amount form of a drag (rule 8), with the limits and signs the
/// entry gives. rule 8 (the form for other tools)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragTool {
    New,
    Invert,
    Smoothing,
    Fractal,
    Mounds,
    Sawtooth,
    Eroded,
    Subcontours,
    Spikes,
    RaiseLower,
    Equalize,
    GaussianEdges,
    SlopeNoise,
    HeightNoise,
    /// The two geometry tools of the green curve that take this form (Relief Noise and
    /// Subplateaus: they share a button with Mounds and with Equalize as 3.0 and 3.1).
    ReliefNoise,
    Subplateaus,
    Sharpening,
    Dampen,
}

/// The limits of P for a tool: `None` for no limit. rule 8 (the form for other tools)
fn amount_limits(tool: DragTool) -> Option<(i64, i64)> {
    match tool {
        DragTool::New | DragTool::Invert | DragTool::Smoothing => Some((0, 256)),
        DragTool::Fractal | DragTool::Mounds | DragTool::Sawtooth | DragTool::Eroded => Some((-256, 256)),
        DragTool::Subcontours => Some((-64, 256)),
        DragTool::Spikes => Some((-512, 512)),
        _ => None,
    }
}

/// The amount P of a drag over `tool` whose pointer has moved `pixels` screen pixels (positive to
/// the right): as 3.0 and 3.1 the pointer's distance from the press place at the last reading, as
/// 5.0 the sum of the pointer's moves (less to the left), in either case in pixels, never one tenth
/// of them; the tool's limits apply and Sharpening and Dampen take the opposite sign (not limited).
/// The three versions have the same limits and signs. rule 8 (How a drag becomes P; the
/// form for other tools).
pub fn drag_amount(tool: DragTool, pixels: i64) -> i64 {
    let p = if matches!(tool, DragTool::Sharpening | DragTool::Dampen) { -pixels } else { pixels };
    match amount_limits(tool) {
        Some((lo, hi)) => p.clamp(lo, hi),
        None => p,
    }
}

/// As 5.0 the distance d (and e) that the shape tools and Square and Round Edges read: the first
/// reading's distance from the press place plus the sum of the moves the program read (a drag that
/// the program reads as one move m gives 2 m). As 3.0 and 3.1 it is the pointer's distance at the
/// reading itself. rule 8 (Square and Round Edges by an amount), C9
pub fn shape_distance_five(first_reading: i64, moves: &[i64]) -> i64 {
    first_reading + moves.iter().sum::<i64>()
}

/// Invert dragged to P: f = 65535 - v, P limited to 0 or above. rule 8
pub fn invert_at_amount(grid: &HeightGrid, p: i64) -> HeightGrid {
    crate::terrain_editor::at_amount(grid, &crate::terrain_editor::invert(grid), p.max(0))
}

/// Mounds dragged to P: the press takes the click's numbers once. rule 8, C5
pub fn mounds_at_amount(grid: &HeightGrid, p: i64, session: &mut Session) -> HeightGrid {
    let click = mounds(grid, session);
    crate::terrain_editor::at_amount(grid, &click, p)
}

/// Fractal dragged to P. rule 8, C6
pub fn fractal_at_amount(grid: &HeightGrid, p: i64, session: &mut Session) -> HeightGrid {
    let click = fractal(grid, session);
    crate::terrain_editor::at_amount(grid, &click, p)
}

/// Slope Noise dragged to P (to the left the original saved no change: Open, same form).
/// rule 8, C4
pub fn slope_noise_at_amount(grid: &HeightGrid, p: i64, session: &mut Session) -> HeightGrid {
    let click = slope_noise(grid, session);
    crate::terrain_editor::at_amount(grid, &click, p)
}

/// Eroded, a click. C7
pub fn eroded(grid: &HeightGrid, session: &mut Session) -> HeightGrid {
    let n = grid.side();
    let s = (n / 4).max(16);
    let mut side = s;
    let mut h = resized(grid.heights(), n, side);
    for i in 0..side {
        h[i] = 0;
        h[(side - 1) * side + i] = 0;
        h[i * side] = 0;
        h[i * side + side - 1] = 0;
    }
    smooth_heights(&mut h, side, session.range);
    smooth_heights(&mut h, side, session.range);
    let (mut count, mut rate) = (200usize, 200i64);
    loop {
        stretch_full(&mut h);
        smooth_heights(&mut h, side, session.range);
        droplet_run(&mut h, side, count, rate, session);
        for v in h.iter_mut() {
            *v = 65535 - *v;
        }
        smooth_heights(&mut h, side, session.range);
        droplet_run(&mut h, side, count, rate, session);
        for v in h.iter_mut() {
            *v = 65535 - *v;
        }
        count *= 2;
        rate = ((rate as f64) * (2.0 / 3.0)).trunc() as i64;
        if side >= n {
            break;
        }
        let next = (side * 2).min(n);
        h = resized(&h, side, next);
        side = next;
    }
    smooth_heights(&mut h, side, session.range);
    grid_of(side, h)
}

/* fifth round: R7 (editor sessions), Variants of C1, C3, C5, C6, C8;
   */

/// The program version a run is of. R7
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    V30,
    V31,
    V50,
}

/// One run of the program with one document: the heights it was opened with, the heights it would
/// save, the open editor session's working grid, and the numbers of R1, which go on from click to
/// click in every version. R7
pub struct EditorRun {
    program: Program,
    opened: HeightGrid,
    saved: HeightGrid,
    working: Option<HeightGrid>,
    /// Whether the document's animation switch is not 0 , by which 3.1's editor keys each
    /// result into the relief track. R7 (Where a session starts)
    animation_switch: bool,
    /// The run's numbers of R1.
    pub numbers: Session,
}

impl EditorRun {
    /// A run that has just opened a document. R7
    pub fn new(program: Program, opened: HeightGrid) -> Self {
        EditorRun { program, saved: opened.clone(), opened, working: None, animation_switch: false, numbers: Session::new() }
    }

    /// Sets the document's animation switch (not 0): as 3.1 each result is then keyed into the
    /// relief track and the next session starts from it, as 3.0 always does. R7
    pub fn with_animation_switch(mut self, on: bool) -> Self {
        self.animation_switch = on;
        self
    }

    /// The editor is opened: 3.0 and 5.0 start from the heights the session before left, 3.1 from
    /// the heights the document was opened with (Open for 3.1: opening or first session's start;
    /// the opening is taken). R7
    pub fn open_editor(&mut self) {
        self.working = Some(match self.program {
            Program::V31 if !self.animation_switch => self.opened.clone(),
            Program::V31 => self.saved.clone(),
            Program::V30 | Program::V50 => self.saved.clone(),
        });
    }

    /// A click of any tool: it works on the working grid (the previous click's result in the same
    /// session; Open for 3.0, same fallback) and takes the numbers it needs. R7
    pub fn click(&mut self, tool: impl FnOnce(&HeightGrid, &mut Session) -> HeightGrid) -> &HeightGrid {
        let work = self.working.as_ref().expect("the editor is open");
        let out = tool(work, &mut self.numbers);
        self.working.insert(out)
    }

    /// The editor is closed with OK and the document saved: the terrain holds the working grid.
    /// R7
    pub fn close_editor(&mut self) {
        if let Some(w) = self.working.take() {
            self.saved = w;
        }
    }

    /// The heights the document then holds.
    pub fn saved(&self) -> &HeightGrid {
        &self.saved
    }
}
