// SPDX-License-Identifier: MIT
//! A colour channel to an 8-bit level, gamma correction and ordered dithering.
//!
//! (levels-50.tsv)
//! (output)

use crate::numerics::pow;
use crate::version::RenderAs;

/// Bryce 5's gamma exponent.
const GAMMA_EXPONENT: f64 = 0.625;
/// The full scale of a dithered channel.
const DITHER_SCALE: f64 = 65535.0;

/// Bryce 3D: the whole part of a channel on the scale 0 to 255 (cut off, never rounded).
pub fn level_bryce3d(c: f32) -> u8 {
    // `as` truncates toward zero and saturates; inputs are already clamped .
    c as u8
}

/// Bryce 5 without gamma or dithering: the whole part of the binary32 product c · 255.
pub fn level_bryce5(c: f32) -> u8 {
    (c * 255.0f32) as u8
}

/// Bryce 5's gamma: c^0.625 by's power, stored as binary32; below 0 or not a
/// number gives 0, above 1 gives 1.
pub fn gamma_bryce5(c: f32) -> f32 {
    if c.is_nan() || c <= 0.0 {
        return 0.0;
    }
    if c >= 1.0 {
        return 1.0;
    }
    pow(c as f64, GAMMA_EXPONENT) as f32
}

/// Bryce 5's ordered dither of one channel at pixel (x, y).
pub fn dither_bryce5(c: f32, x: u32, y: u32, table: &[u8; 256]) -> u8 {
    // The product of a binary32 and 65535 is formed exactly, in binary64.
    let p = c as f64 * DITHER_SCALE;
    let p = if p.is_nan() || p < 0.0 { 0.0 } else if p > DITHER_SCALE { DITHER_SCALE } else { p };
    let v = p as u32; // cut to a whole number
    let t = table[(16 * (x % 16) + (y % 16)) as usize] as u32;
    ((v - v / 256 + t) / 256) as u8
}

/// The switches of Bryce 5's post-processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostOptions {
    pub gamma: bool,
    pub dither: bool,
}

/// A pixel's channel (on the version's scale) to its level. Renders as 3.0 and 3.1
/// apply neither gamma nor dithering, whatever `options` says (Variants).
pub fn channel_level(
    version: RenderAs,
    c: f32,
    x: u32,
    y: u32,
    options: PostOptions,
    dither_table: Option<&[u8; 256]>,
) -> u8 {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => level_bryce3d(c),
        RenderAs::Bryce50 => {
            let c = if options.gamma { gamma_bryce5(c) } else { c };
            match (options.dither, dither_table) {
                (true, Some(t)) => dither_bryce5(c, x, y, t),
                // Without the table the render is refused before it starts (
                // Errors, component substrate); a caller that gets here has no table.
                _ => level_bryce5(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dither_index_and_scaling() {
        let mut table = [0u8; 256];
        for (i, t) in table.iter_mut().enumerate() {
            *t = i as u8;
        }
        // c = 0.5: v = 32767 (cut), v - v/256 = 32640; t = 16·1 + 2 = 18: 32658 / 256 = 127.
        assert_eq!(dither_bryce5(0.5, 1, 2, &table), 127);
        // t = 255 at (15, 15): 32895 / 256 = 128.
        assert_eq!(dither_bryce5(0.5, 15, 15, &table), 128);
        assert_eq!(dither_bryce5(1.0, 0, 0, &table), 255);
        assert_eq!(dither_bryce5(f32::NAN, 0, 0, &table), 0);
    }

    /// : renders as 3.0 and 3.1 apply neither gamma nor dithering.
    #[test]
    fn bryce3d_ignores_post_options() {
        let table = [255u8; 256];
        let on = PostOptions { gamma: true, dither: true };
        let off = PostOptions { gamma: false, dither: false };
        for version in [RenderAs::Bryce30, RenderAs::Bryce31] {
            for c in [0.0f32, 0.5, 63.7, 127.25, 200.9, 255.0] {
                assert_eq!(channel_level(version, c, 3, 5, on, Some(&table)), channel_level(version, c, 3, 5, off, None));
            }
        }
    }
}
