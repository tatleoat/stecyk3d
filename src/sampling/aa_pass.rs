// SPDX-License-Identifier: MIT
//! The anti-aliasing pass over the finished picture.
//!
//! (when the pass runs,'s switch bit 0x2000, is the render plan's
//! the seven places here are those of rays with depth of field off and take no draws)
//! (the anti-aliasing pass)

use super::batch::PointBatch;
use super::output::{PostOptions, dither_bryce5, gamma_bryce5};
use super::samples::clamp_channel;
use crate::scene_records::Region;
use crate::version::RenderAs;

/// The extra camera rays a pixel on an edge gets.
pub const EXTRA_RAYS: usize = 7;

/// The first-pass level is brought to 0 to 1 by the binary32 number nearest 1/255.
const LEVEL_TO_UNIT: f32 = 1.0 / 255.0;
/// The sum of eight channels on 0 to 1 times 255 / 8 gives the level.
const SUM_TO_LEVEL: f64 = 31.875;
/// The mean of the eight, for dithering.
const EIGHTH: f32 = 0.125;

/// The edge limit in 8-bit levels: 20 in 3.0, 15 in 3.1 and in Bryce 5.
pub fn edge_limit(version: RenderAs) -> u8 {
    match version {
        RenderAs::Bryce30 => 20,
        RenderAs::Bryce31 => 15,
        RenderAs::Bryce50 => 15,
    }
}

fn differs(a: [u8; 3], b: [u8; 3], limit: u8) -> bool {
    (0..3).any(|k| a[k].abs_diff(b[k]) > limit)
}

/// The edge test of one pixel against its right, lower and lower-right neighbours' levels:
/// any channel differs by more than the limit from any of the three, or the right differs
/// so from the lower-right (the lower is not compared with the other two).
pub fn on_edge(pixel: [u8; 3], right: [u8; 3], below: [u8; 3], diagonal: [u8; 3], limit: u8) -> bool {
    differs(pixel, right, limit)
        || differs(pixel, below, limit)
        || differs(pixel, diagonal, limit)
        || differs(right, diagonal, limit)
}

/// Which pixels are on an edge, every one tested against the first-pass levels (the pass
/// goes row by row from the top, each row from the left, and the neighbours tested are
/// not yet visited). `levels` holds R, G, B per pixel, row by row. The last row and column
/// are never refined.
pub fn edge_mask(levels: &[u8], width: u32, height: u32, limit: u8) -> Vec<bool> {
    edge_mask_in(levels, width, height, Region { top: 0, left: 0, bottom: height, right: width }, limit)
}

/// The same, keeping to a region of the picture (top, left, bottom, right; the last row and
/// column not included): only the region's pixels are tested, except its last row and last
/// column, which are never refined, so every neighbour read lies in the region. The mask
/// covers the whole picture; pixels outside the region are never on it.
/// (points 1 to 3)
pub fn edge_mask_in(levels: &[u8], width: u32, height: u32, region: Region, limit: u8) -> Vec<bool> {
    let (w, h) = (width as usize, height as usize);
    let at = |x: usize, y: usize| {
        let i = (y * w + x) * 3;
        [levels[i], levels[i + 1], levels[i + 2]]
    };
    let mut mask = vec![false; w * h];
    let (top, left) = (region.top as usize, region.left as usize);
    let (bottom, right) = ((region.bottom as usize).min(h), (region.right as usize).min(w));
    for y in top..bottom.saturating_sub(1) {
        for x in left..right.saturating_sub(1) {
            mask[y * w + x] = on_edge(at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1), limit);
        }
    }
    mask
}

/// The seven picture points of a pixel on an edge, from the substrate's `aa-offsets`
/// (the seven x, then the seven y), in the table's order.
pub fn refine_points(px: u32, py: u32, pixel: u32, aa_offsets: &[f64; 14], out: &mut PointBatch) {
    for k in 0..EXTRA_RAYS {
        out.push(px as f64 + aa_offsets[k], py as f64 + aa_offsets[EXTRA_RAYS + k], pixel);
    }
}

/// How a refined pixel's channel becomes its level.
#[derive(Clone, Copy, Debug)]
pub struct RefineRule<'a> {
    pub version: RenderAs,
    /// Bryce 5's switches (ignored in Bryce 3D).
    pub post: PostOptions,
    /// The substrate's `dither` table, needed when Bryce 5 dithers .
    pub dither_table: Option<&'a [u8; 256]>,
}

impl RefineRule<'_> {
    fn gamma(&self) -> bool {
        self.version == RenderAs::Bryce50 && self.post.gamma
    }

    fn dither(&self) -> Option<&[u8; 256]> {
        match self.version {
            RenderAs::Bryce50 if self.post.dither => self.dither_table,
            _ => None,
        }
    }

    /// The refined level of one channel at pixel (x, y): the first-pass level brought to
    /// 0 to 1 (level times the binary32 nearest 1/255, formed in binary64, rounded to
    /// binary32), then the seven samples on 0 to 1, each clamped as (in Bryce 5 with
    /// gamma, each corrected by instead) and added in order in binary32; the level
    /// is the whole part of the sum times 31.875 (binary64), or with Bryce 5's dither the
    /// sum times 1/8 dithered at the pixel's place.
    ///
    /// The cut (Accuracy): as 3.0 and 3.1 red (`k` 0) is cut from the exact product
    /// and green and blue from the product rounded to binary32; as 5.0 all three from the
    /// product rounded to binary32.
    pub fn channel(&self, k: usize, first_level: u8, extra: [f32; EXTRA_RAYS], x: u32, y: u32) -> u8 {
        let mut sum = (first_level as f64 * LEVEL_TO_UNIT as f64) as f32;
        for c in extra {
            sum += if self.gamma() { gamma_bryce5(c) } else { clamp_channel(c) };
        }
        match self.dither() {
            Some(table) => dither_bryce5(sum * EIGHTH, x, y, table),
            // The sum is at most 8, so the product is at most 255; the product of a binary32
            // sum and 31.875 is exact in binary64.
            None => {
                let exact = sum as f64 * SUM_TO_LEVEL;
                let red_exact = k == 0 && self.version != RenderAs::Bryce50;
                if red_exact { exact as u8 } else { (exact as f32) as u8 }
            }
        }
    }

    /// The refined levels of a pixel from its first-pass levels and its seven samples'
    /// binary32 colours, in the order of `aa-offsets`.
    pub fn pixel(&self, first: [u8; 3], extra: &[[f32; 3]; EXTRA_RAYS], x: u32, y: u32) -> [u8; 3] {
        std::array::from_fn(|k| self.channel(k, first[k], extra.map(|s| s[k]), x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_test_follows_the_entry() {
        // 3 by 2 picture: pixel (1, 0) is 21 levels redder than the rest.
        let mut lv = vec![100u8; 3 * 2 * 3];
        lv[3] = 121;
        let m = edge_mask(&lv, 3, 2, 20);
        // (0,0) differs from its right neighbour; (1,0) from its right and lower-right;
        // the last row and column are never refined.
        assert_eq!(m, vec![true, true, false, false, false, false]);
        let m = edge_mask(&lv, 3, 2, 21);
        assert!(m.iter().all(|&b| !b));
    }

    #[test]
    fn right_against_lower_right() {
        // (0,0) equals its right and lower neighbours, but right and lower-right differ.
        let mut lv = vec![50u8; 2 * 2 * 3];
        lv[(2 + 1) * 3 + 2] = 80; // pixel (1, 1), blue
        let m = edge_mask(&lv, 2, 2, 15);
        assert!(m[0]);
        // The lower neighbour is not compared with the lower-right.
        assert!(!on_edge([50; 3], [50; 3], [50; 3], [50; 3], 15));
        // Lower against lower-right 25 and against right 20; every compared pair is
        // within 15.
        assert!(!on_edge([65; 3], [60; 3], [80; 3], [55; 3], 15));
    }

    /// : only the region's pixels, its last row and column untested.
    #[test]
    fn region_mask_keeps_to_the_region() {
        // 4 by 3 picture, every pixel 21 levels apart from its right neighbour.
        let lv: Vec<u8> = (0..12).flat_map(|p| [(p % 4) as u8 * 21; 3]).collect();
        let r = Region { top: 0, left: 1, bottom: 2, right: 3 };
        let m = edge_mask_in(&lv, 4, 3, r, 20);
        let on: Vec<usize> = (0..12).filter(|&p| m[p]).collect();
        // The region holds (1..3, 0..2); its last column (2) and row (1) are not tested.
        assert_eq!(on, vec![1]);
    }

    #[test]
    fn eight_whites_give_255() {
        let rule = RefineRule { version: RenderAs::Bryce31, post: PostOptions { gamma: false, dither: false }, dither_table: None };
        assert_eq!(rule.channel(0, 255, [1.0; 7], 0, 0), 255);
        assert_eq!(rule.channel(0, 0, [0.0; 7], 0, 0), 0);
        assert_eq!(rule.channel(0, 0, [f32::NAN, -1.0, 2.0, 0.0, 0.0, 0.0, 0.0], 0, 0), 31);
    }
}
