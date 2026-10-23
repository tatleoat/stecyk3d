// SPDX-License-Identifier: MIT
//! Bryce 5's Fractal button with its menu of kinds: the menu (kinds, the three switches, the four
//! tiling choices), the frame of a click of every kind but Bryce Classic (extent, position, tiling,
//! the clipping range, the selection's band) and the one kind the entry describes so far, Rounded
//! Dunes. Bryce Classic is C6 (`terrain_tools::fractal`).
//! (5.0's Fractal kinds, code only, writer W165; the kinds 14 to 25 of the generator's
//! list: Rounded Dunes),.
//!
//! The other kinds (Ridged Multifractal, Perlin Hills, Classic Fractal, Round Hills, Slickrock,
//! Ridges, Rolling Hills, Alpine Valleys, Chaotic, Pocked, Crystalline, Lava, Metamorphism,
//! Metamorphosed, Fractured, Weathered Dikes, Warped Slickrock, Warped Ridges, Zorch, Warped Zorch,
//! Mordor, Shattered Hills, Cauliflower Hills, Planet X, Mud Cracks) are "not described here yet"
//! by the entry: `click` refuses them with `Undescribed`.

use crate::numerics::{pow, Bryce5Rand};
use crate::terrain::HeightGrid;
use crate::terrain_clip::ClipRange;
use crate::textures::five::{five_value, FiveSets};

/// The 27 kinds of the menu, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractalKind {
    BryceClassic,
    RidgedMultifractal,
    PerlinHills,
    RoundedDunes,
    ClassicFractal,
    RoundHills,
    Slickrock,
    Ridges,
    RollingHills,
    AlpineValleys,
    Chaotic,
    Pocked,
    Crystalline,
    Lava,
    Metamorphism,
    Metamorphosed,
    Fractured,
    WeatheredDikes,
    WarpedSlickrock,
    WarpedRidges,
    Zorch,
    WarpedZorch,
    Mordor,
    ShatteredHills,
    CauliflowerHills,
    PlanetX,
    MudCracks,
}

/// The menu's kinds in its order (the 26th and 27th are Planet X and Mud Cracks).
pub const KINDS: [FractalKind; 27] = [
    FractalKind::BryceClassic,
    FractalKind::RidgedMultifractal,
    FractalKind::PerlinHills,
    FractalKind::RoundedDunes,
    FractalKind::ClassicFractal,
    FractalKind::RoundHills,
    FractalKind::Slickrock,
    FractalKind::Ridges,
    FractalKind::RollingHills,
    FractalKind::AlpineValleys,
    FractalKind::Chaotic,
    FractalKind::Pocked,
    FractalKind::Crystalline,
    FractalKind::Lava,
    FractalKind::Metamorphism,
    FractalKind::Metamorphosed,
    FractalKind::Fractured,
    FractalKind::WeatheredDikes,
    FractalKind::WarpedSlickrock,
    FractalKind::WarpedRidges,
    FractalKind::Zorch,
    FractalKind::WarpedZorch,
    FractalKind::Mordor,
    FractalKind::ShatteredHills,
    FractalKind::CauliflowerHills,
    FractalKind::PlanetX,
    FractalKind::MudCracks,
];

/// The four tiling choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tiling {
    North,
    South,
    East,
    West,
}

/// The three switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    RandomExtent,
    RandomPosition,
    RandomCharacter,
}

/// What the menu shows: the tab starts with Bryce Classic, the three switches on and no tiling
/// choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FractalMenu {
    pub kind: FractalKind,
    pub random_extent: bool,
    pub random_position: bool,
    pub random_character: bool,
    pub tiling: Option<Tiling>,
}

impl FractalMenu {
    /// The menu as the tab starts.
    pub fn new() -> FractalMenu {
        FractalMenu {
            kind: FractalKind::BryceClassic,
            random_extent: true,
            random_position: true,
            random_character: true,
            tiling: None,
        }
    }

    /// Choosing a kind.
    pub fn choose_kind(&mut self, kind: FractalKind) {
        self.kind = kind;
    }

    /// Choosing a switch turns it over (greyed, doing nothing, with Bryce Classic).
    pub fn choose_switch(&mut self, switch: Switch) {
        if self.kind == FractalKind::BryceClassic {
            return;
        }
        let flag = match switch {
            Switch::RandomExtent => &mut self.random_extent,
            Switch::RandomPosition => &mut self.random_position,
            Switch::RandomCharacter => &mut self.random_character,
        };
        *flag = !*flag;
    }

    /// Choosing a tiling choice other than the current one makes it current and turns the three
    /// switches off; choosing the current one again leaves none.
    pub fn choose_tiling(&mut self, tiling: Tiling) {
        if self.kind == FractalKind::BryceClassic {
            return;
        }
        if self.tiling == Some(tiling) {
            self.tiling = None;
        } else {
            self.tiling = Some(tiling);
            self.random_extent = false;
            self.random_position = false;
            self.random_character = false;
        }
    }
}

impl Default for FractalMenu {
    fn default() -> Self {
        FractalMenu::new()
    }
}

/// What the last click of a kind other than Bryce Classic left for the next: the extent and
/// position (E = 1 and P0 = P1 = 0 before the first), the kind and its character.
#[derive(Clone, Copy, Debug)]
pub struct FractalState {
    extent: f32,
    p0: f32,
    p1: f32,
    last_kind: Option<FractalKind>,
    character: f32,
}

impl FractalState {
    /// The state before the first such click.
    pub fn new() -> FractalState {
        FractalState { extent: 1.0, p0: 0.0, p1: 0.0, last_kind: None, character: 0.0 }
    }
}

impl Default for FractalState {
    fn default() -> Self {
        FractalState::new()
    }
}

/// A selection on the grid: the rectangle of C points along i (rows) by R along j (columns) with
/// its corner at (row0, col0).
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub row0: usize,
    pub col0: usize,
    pub c: usize,
    pub r: usize,
}

/// Why a click is not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractalRefusal {
    /// The kind's arithmetic is not given by the entry yet (Open).
    Undescribed(FractalKind),
    /// Bryce Classic is C6 (`terrain_tools::fractal`), not this frame.
    Classic,
    /// The type 27 noise gave no value at a point.
    NoValue,
}

/// A single ⟦x⟧: a binary64 value rounded to binary32.
fn single(x: f64) -> f64 {
    x as f32 as f64
}

/// A click of a kind other than Bryce Classic on a grid of side n: the extent and position
/// (a), the kind's character (b), the evaluation of every point in storage order (c), the clipping
/// range (d) and the selection's band or the whole grid (e). `generator` is Bryce 5's one generator
/// ; its state at the program's start is free. Nothing follows (no resizing, no stretch).
#[allow(clippy::too_many_arguments)]
pub fn click(
    grid: &HeightGrid,
    menu: &FractalMenu,
    state: &mut FractalState,
    generator: &mut Bryce5Rand,
    range: ClipRange,
    selection: Option<Selection>,
    bits: &[u32; 64],
    sets: &FiveSets,
) -> Result<HeightGrid, FractalRefusal> {
    let kind = menu.kind;
    if kind == FractalKind::BryceClassic {
        return Err(FractalRefusal::Classic);
    }
    if kind != FractalKind::RoundedDunes {
        return Err(FractalRefusal::Undescribed(kind));
    }
    let n = grid.side();
    // (a) the extent and the position
    let mut extent_before_rounding = None;
    if menu.random_extent {
        let s = generator.next_unit() as f64;
        let e_prime = pow(s, 1.7) * 4.0 + 0.5; // : E' = pow(s, 1.7) 4 + 0.5
        extent_before_rounding = Some(e_prime);
        state.extent = e_prime as f32;
    }
    if menu.random_position {
        let s1 = generator.next_unit() as f64;
        let s2 = generator.next_unit() as f64;
        state.p0 = single(100.0 * s1) as f32; // : P0 = ⟦100 s1⟧
        state.p1 = single(100.0 * s2) as f32;
    }
    let e = state.extent as f64;
    match menu.tiling {
        Some(Tiling::North) => state.p0 = single(state.p0 as f64 + e) as f32,
        Some(Tiling::South) => state.p0 = single(state.p0 as f64 - e) as f32,
        Some(Tiling::East) => state.p1 = single(state.p1 as f64 + e) as f32,
        Some(Tiling::West) => state.p1 = single(state.p1 as f64 - e) as f32,
        None => {}
    }
    // (b) the character: Rounded Dunes' one draw, when Random Character is on or the kind is not
    // the kind of the last such click
    if menu.random_character || state.last_kind != Some(kind) {
        let s = generator.next_unit() as f64;
        state.character = single(1.9 * s + 0.1) as f32; // : A = ⟦1.9 s + 0.1⟧
    }
    state.last_kind = Some(kind);
    let a_char = state.character as f64;
    // (c) the step between points
    let denom = (n - 1) as f64;
    let a = single(match extent_before_rounding {
        Some(e_prime) => e_prime / denom,
        None => e / denom,
    });
    let (p0, p1) = (state.p0 as f64, state.p1 as f64);
    let value_at = |i: usize, j: usize| -> Result<u16, FractalRefusal> {
        let x = single(single(i as f64 * a) + p0);
        let y = single(j as f64 * a + p1);
        let f = rounded_dunes(x, y, a_char, bits, sets).ok_or(FractalRefusal::NoValue)?;
        Ok((f * 65535.0).trunc() as u16)
    };
    let mut out = grid.heights().to_vec();
    match selection {
        None => {
            for i in 0..n {
                for j in 0..n {
                    let v = grid.at(i, j) as i64;
                    // (d) a point is evaluated and changed only inside the clipping range
                    if range.holds(v) {
                        out[i * n + j] = value_at(i, j)?;
                    }
                }
            }
        }
        Some(sel) => {
            let (c_len, r_len) = (sel.c as i64, sel.r as i64);
            let b_c = (0.2 * c_len as f64).trunc() as i64;
            let b_r = (0.2 * r_len as f64).trunc() as i64;
            for i in 0..sel.c {
                for j in 0..sel.r {
                    let (row, col) = (sel.row0 + i, sel.col0 + j);
                    if row >= n || col >= n {
                        continue;
                    }
                    let v = grid.at(row, col) as i64;
                    if !range.holds(v) {
                        continue;
                    }
                    let h = value_at(i, j)? as i64;
                    let (ii, jj) = (i as i64, j as i64);
                    let inside_i = b_c <= ii && ii <= c_len - b_c;
                    let inside_j = b_r <= jj && jj <= r_len - b_r;
                    let nv = if inside_i && inside_j {
                        h
                    } else {
                        let di = if ii < b_c { ii } else { c_len - ii };
                        let dj = if jj < b_r { jj } else { r_len - jj };
                        let t = if (inside_i && !inside_j)
                            || (!inside_i && !inside_j && (di > dj || (di == dj && ii < b_c)))
                        {
                            dj as f64 / b_r as f64
                        } else {
                            di as f64 / b_c as f64
                        };
                        ((h - v) as f64 * t + v as f64).trunc() as i64
                    };
                    out[row * n + col] = nv.clamp(0, 65535) as u16;
                }
            }
        }
    }
    Ok(HeightGrid::new(n, out).expect("same side"))
}

/// Rounded Dunes at (x, y, z = 0.5) with the character A: X = 2x and Y = 2y; N(X, Y) the noise
/// type 27 in 2-D ; d0 = ⟦2 N(X, Y) - 1⟧ and d1 = ⟦2 N(⟦X + 10⟧, Y) - 1⟧;
/// X' = ⟦(X + ⟦d0 A⟧) + 0.5⟧ and Y' = (Y + ⟦d1 A⟧) + 0.5 in binary64; g = 2 N(X', Y') - 1 in binary64;
/// f = ⟦(g + 1) 0.5⟧ held from 0 to 1.
pub fn rounded_dunes(x: f64, y: f64, a: f64, bits: &[u32; 64], sets: &FiveSets) -> Option<f64> {
    let noise = |px: f64, py: f64| five_value(27, 2, [px, py, 0.0], bits, sets);
    let (big_x, big_y) = (2.0 * x, 2.0 * y);
    let d0 = single(2.0 * noise(big_x, big_y)? - 1.0);
    let d1 = single(2.0 * noise(single(big_x + 10.0), big_y)? - 1.0);
    let x2 = single((big_x + single(d0 * a)) + 0.5);
    let y2 = (big_y + single(d1 * a)) + 0.5;
    let g = 2.0 * noise(x2, y2)? - 1.0;
    let raw = (g + 1.0) * 0.5;
    let f = single(raw);
    Some(if raw < 0.0 {
        0.0
    } else if raw > 1.0 {
        1.0
    } else {
        f
    })
}
