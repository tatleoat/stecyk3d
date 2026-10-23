// SPDX-License-Identifier: MIT
//! The tints of the air: the haze colour (bit 0x2000) and the fog colour (bit 0x200000) drawn
//! toward the sun's colour, and toward the moon's, by an amount that depends on the ray's
//! direction.
//!
//! , ("c toward b
//! by w"),
//!
//! Notation as: binary64 operations one Rust `f64` operation each, ⟨x⟩ `x as f32`,
//! ⊕ ⊖ ⊗ Rust `f32` operations.

use crate::geom::Vec3;

/// Bit 0x2000: the haze takes the tint.
pub const HAZE_TINT: u32 = 0x2000;
/// Bit 0x200000: the fog takes the tint.
pub const FOG_TINT: u32 = 0x20_0000;

/// The moon's tint colour: red and green ⟨μ·0.5⟩ capped at 0.374, blue ⟨μ·0.625⟩ capped at 0.438.
const MOON_TINT: [(f64, f64); 3] = [(0.5, 0.374), (0.5, 0.374), (0.625, 0.438)];
/// T' = T ⊕ q, q the singles nearest 0.374, 0.374 and 0.438.
const MOON_ADD: [f32; 3] = [0.374, 0.374, 0.438];
/// The moon's pull is further scaled by 0.85.
const MOON_PULL: f64 = 0.85;

/// What the tints read: the sun's direction S' , its colour C  and the third and
/// fourth of the fifteen numbers, N2 and N3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tints {
    pub sun: Vec3,
    pub colour: [f32; 3],
    /// N2: how far the colour takes the sun's colour overall.
    pub overall: f64,
    /// N3: how far toward the sun.
    pub toward_sun: f64,
}

/// "c toward b by w": ck ⊕ ((bk ⊖ ck) ⊗ w).
fn toward(c: [f32; 3], b: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|k| c[k] + (b[k] - c[k]) * w)
}

/// `x` where it is above or equal to 0, else 0 (below 0 or not a number).
fn not_below_zero(x: f64) -> f64 {
    if x >= 0.0 { x } else { 0.0 }
}

impl Tints {
    /// H' or F': the colour `c` tinted for a ray of direction `d` as it was sent (not made unit).
    pub fn colour_for(&self, c: [f32; 3], d: Vec3) -> [f32; 3] {
        let s = self.sun;
        // 1. The sun: a = (S'y·Dy + S'z·Dz) + S'x·Dx, 0 where below 0 or not a number.
        let a = not_below_zero((s[1] * d[1] + s[2] * d[2]) + s[0] * d[0]);
        let c1 = toward(c, self.colour, self.overall as f32);
        let c = toward(c, c1, (a * self.toward_sun) as f32);
        // 2. The moon, direction −S', whatever the sun's height; μ = −S'y.
        let m = not_below_zero(((-s[1]) * d[1] + (-s[2]) * d[2]) + (-s[0]) * d[0]);
        let mu = -s[1];
        let cap = |x: f32, limit: f64| if f64::from(x) < limit { x } else { limit as f32 };
        let t: [f32; 3] = std::array::from_fn(|k| cap((mu * MOON_TINT[k].0) as f32, MOON_TINT[k].1));
        let t1: [f32; 3] = std::array::from_fn(|k| t[k] + MOON_ADD[k]);
        let c2 = toward(c, t1, self.overall as f32);
        toward(c, c2, ((m * self.toward_sun) * MOON_PULL) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tints(overall: f64, toward_sun: f64) -> Tints {
        Tints { sun: [0.0, 0.6, 0.8], colour: [1.0, 0.5, 0.25], overall, toward_sun }
    }

    #[test]
    fn no_pull_changes_nothing() {
        let c = [0.2, 0.4, 0.6];
        assert_eq!(tints(0.0, 0.0).colour_for(c, [0.0, 0.0, 1.0]), c);
    }

    #[test]
    fn facing_the_sun_takes_its_colour() {
        let t = tints(1.0, 1.0);
        let got = t.colour_for([0.2, 0.4, 0.6], [0.0, 0.6, 0.8]);
        // The moon's pull is zero here (m is below 0: taken as 0).
        assert_eq!(got, [1.0, 0.5, 0.25]);
    }
}
