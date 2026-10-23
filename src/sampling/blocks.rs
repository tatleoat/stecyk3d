// SPDX-License-Identifier: MIT
//! The other way of making the picture: blocks traced at their corners (switch bit 0x4000).
//!
//! blocks-pictures/, blocks-order-pictures/),
//! (blocks traced at their corners)
//!
//! The way is run depth first, as the rays are asked for in this order (Effects: the
//! order decides which surface of a material is met first and which draws a ray's shading
//! takes): `walk` takes the region's blocks of side 4 in rows, each whole before the next,
//! and hands the places a block asks for to the caller's tracer. `finish` writes the
//! picture: every undivided block filled between its corners, then every traced place
//! inside the picture with its own level.

use crate::scene_records::Region;

/// The side of the blocks the region is first covered by.
pub const FIRST_SIDE: u32 = 4;
/// A block lies on an edge when two of its corners compared differ by more than this in a
/// channel, as 3.0, 3.1 and 5.0.
pub const CORNER_LIMIT: u8 = 20;
/// How far beyond the region's right and lower edges a place can lie: a block of the first
/// side whose top-left pixel is the region's last column or row reaches its corner 4 on.
/// (the grid of places)
const REACH: u32 = FIRST_SIDE;

/// A block: its top-left place and its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub x: u32,
    pub y: u32,
    pub side: u32,
}

impl Block {
    /// The four corners: top-left, top-right, bottom-left, bottom-right. A block of side 1
    /// is its top-left corner alone (the others are not asked for).
    fn corners(self) -> [(u32, u32); 4] {
        let (x, y, s) = (self.x, self.y, self.side);
        [(x, y), (x + s, y), (x, y + s), (x + s, y + s)]
    }

    /// The four blocks of half the side in the order they are taken: top-left, top-right,
    /// bottom-right, bottom-left.
    fn quarters_in_order(self) -> [Block; 4] {
        let (x, y, h) = (self.x, self.y, self.side / 2);
        [
            Block { x, y, side: h },
            Block { x: x + h, y, side: h },
            Block { x: x + h, y: y + h, side: h },
            Block { x, y: y + h, side: h },
        ]
    }
}

fn differs(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).any(|k| a[k].abs_diff(b[k]) > CORNER_LIMIT)
}

/// The test: the top-left corner against each of the other three, and the top-right against
/// the bottom-left.
pub fn corners_on_edge(tl: [u8; 3], tr: [u8; 3], bl: [u8; 3], br: [u8; 3]) -> bool {
    differs(tl, tr) || differs(tl, bl) || differs(tl, br) || differs(tr, bl)
}

/// One whole-level step of the interpolation: `a` to `b` at `i` of `s`, cut to a whole level
/// (both are levels, so the weighted sum is never below 0 and the cut is the floor).
fn between(a: u8, b: u8, i: u32, s: u32) -> u8 {
    ((a as u32 * (s - i) + b as u32 * i) / s) as u8
}

/// The level of place (x + i, y + j) of an undivided block of side `s` with corners
/// `[tl, tr, bl, br]`: along the top and the bottom rows first, each cut to a whole level,
/// then down between the two, cut again. At a corner it is the corner's level.
pub fn interpolate(c: [[u8; 3]; 4], i: u32, j: u32, s: u32) -> [u8; 3] {
    std::array::from_fn(|k| {
        let top = between(c[0][k], c[1][k], i, s);
        let bottom = between(c[2][k], c[3][k], i, s);
        between(top, bottom, j, s)
    })
}

/// The way's state for one render. (blocks traced at their
/// corners)
#[derive(Clone, Debug)]
pub struct CornerBlocks {
    width: u32,
    height: u32,
    region: Region,
    /// The grid of places a block can ask for: columns 0 to width + 3, rows 0 to height + 3.
    stride: u32,
    rows: u32,
    /// Per place of the grid: the level of its ray once traced, for a place inside the
    /// picture (a corner there is traced once, Corners).
    traced: Vec<Option<[u8; 3]>>,
    /// Per place: asked for at least once (for `traced_places`).
    asked: Vec<bool>,
    /// The blocks of side 2 or more not on an edge, to be filled, with their corners' levels
    /// (top-left, top-right, bottom-left, bottom-right), a corner beyond the picture kept for
    /// its block alone.
    filled: Vec<(Block, [[u8; 3]; 4])>,
}

impl CornerBlocks {
    /// The region covered by blocks of side 4 from its top-left pixel. The region must lie
    /// in the picture (the render plan checks it).
    pub fn new(width: u32, height: u32, region: Region) -> CornerBlocks {
        let (stride, rows) = (width + REACH, height + REACH);
        let places = stride as usize * rows as usize;
        CornerBlocks {
            width,
            height,
            region,
            stride,
            rows,
            traced: vec![None; places],
            asked: vec![false; places],
            filled: Vec::new(),
        }
    }

    /// The width of the grid of places: a place's index is row · stride + column.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// The place of a grid index.
    pub fn place_of(&self, index: u32) -> (u32, u32) {
        (index % self.stride, index / self.stride)
    }

    fn index(&self, (x, y): (u32, u32)) -> usize {
        debug_assert!(x < self.stride && y < self.rows);
        y as usize * self.stride as usize + x as usize
    }

    fn inside(&self, (x, y): (u32, u32)) -> bool {
        x < self.width && y < self.height
    }

    /// Take the region's blocks of side 4 in rows from the top, each row from the left, each
    /// block whole before the next. `trace(places, out)` is handed the grid indices of the
    /// places a block asks for, in the order it asks, and must leave in `out` the level of
    /// each one's ray, in that order, tracing them one after another with every ray each
    /// leads to before the next. A block asks first for its corners that are not already
    /// traced inside the picture (top left, top right, bottom right, bottom left; a block of
    /// side 1 for its one corner), then, on an edge, takes its four blocks of half the side
    /// in the same order, each whole, the blocks it is divided into included, before the next.
    pub fn walk<E>(&mut self, trace: &mut dyn FnMut(&[u32], &mut Vec<[u8; 3]>) -> Result<(), E>) -> Result<(), E> {
        let mut out = Vec::with_capacity(4);
        let r = self.region;
        for y in (r.top..r.bottom).step_by(FIRST_SIDE as usize) {
            for x in (r.left..r.right).step_by(FIRST_SIDE as usize) {
                self.take(Block { x, y, side: FIRST_SIDE }, trace, &mut out)?;
            }
        }
        Ok(())
    }

    fn take<E>(
        &mut self,
        b: Block,
        trace: &mut dyn FnMut(&[u32], &mut Vec<[u8; 3]>) -> Result<(), E>,
        out: &mut Vec<[u8; 3]>,
    ) -> Result<(), E> {
        let corners = b.corners();
        // Top left, top right, bottom right, bottom left (indices into `corners`).
        let ask_order = [0usize, 1, 3, 2];
        let n = if b.side == 1 { 1 } else { 4 };
        let mut levels = [[0u8; 3]; 4];
        let mut wanted: Vec<u32> = Vec::with_capacity(4);
        let mut slots: Vec<usize> = Vec::with_capacity(4);
        for &k in &ask_order[..n] {
            let c = corners[k];
            if self.inside(c) {
                if let Some(v) = self.traced[self.index(c)] {
                    levels[k] = v;
                    continue;
                }
            }
            wanted.push(self.index(c) as u32);
            slots.push(k);
        }
        if !wanted.is_empty() {
            trace(&wanted, out)?;
            assert_eq!(out.len(), wanted.len(), "one level a place asked for");
            for ((&k, &i), &v) in slots.iter().zip(&wanted).zip(out.iter()) {
                levels[k] = v;
                self.asked[i as usize] = true;
                if self.inside(corners[k]) {
                    self.traced[i as usize] = Some(v);
                }
            }
        }
        if b.side == 1 {
            return Ok(());
        }
        let [tl, tr, bl, br] = levels;
        if corners_on_edge(tl, tr, bl, br) {
            // Those that begin beyond the last column or row are left out.
            for q in b.quarters_in_order() {
                if q.x < self.width && q.y < self.height {
                    self.take(q, trace, out)?;
                }
            }
        } else {
            self.filled.push((b, levels));
        }
        Ok(())
    }

    /// Write the picture: every pixel of every undivided block inside the picture with its
    /// interpolated level, then every traced place inside the picture with its own level
    /// (a corner keeps its ray's level). `levels` holds the whole picture, row by row, red,
    /// green, blue; `written`, when given, is set for every pixel written. Pixels no block
    /// covers keep what they held.
    pub fn finish(&self, levels: &mut [u8], mut written: Option<&mut [bool]>) {
        let (w, h) = (self.width, self.height);
        let mut put = |x: u32, y: u32, v: [u8; 3]| {
            let p = (y * w + x) as usize;
            levels[p * 3..p * 3 + 3].copy_from_slice(&v);
            if let Some(m) = written.as_deref_mut() {
                m[p] = true;
            }
        };
        for (b, c) in &self.filled {
            let c = *c;
            for j in 0..b.side {
                for i in 0..b.side {
                    let (x, y) = (b.x + i, b.y + j);
                    if x < w && y < h {
                        put(x, y, interpolate(c, i, j, b.side));
                    }
                }
            }
        }
        for y in 0..h.min(self.rows) {
            for x in 0..w.min(self.stride) {
                if let Some(v) = self.traced[self.index((x, y))] {
                    put(x, y, v);
                }
            }
        }
    }

    /// Every place asked for, as (column, row), sorted by row and then column.
    pub fn traced_places(&self) -> Vec<(u32, u32)> {
        (0..self.asked.len() as u32).filter(|&i| self.asked[i as usize]).map(|i| self.place_of(i)).collect()
    }

    /// Run the whole way with `ray(column, row)` giving each place's level, in the order the
    /// places are asked for. For tests and callers without a staged renderer.
    pub fn run(&mut self, mut ray: impl FnMut(u32, u32) -> [u8; 3]) {
        let stride = self.stride;
        let r: Result<(), ()> = self.walk(&mut |places, out| {
            out.clear();
            out.extend(places.iter().map(|&i| ray(i % stride, i / stride)));
            Ok(())
        });
        r.expect("no failure");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_block_is_filled_from_four_corners() {
        let mut b = CornerBlocks::new(4, 4, Region { top: 0, left: 0, bottom: 4, right: 4 });
        b.run(|x, _| [x as u8 * 4, 0, 0]);
        assert_eq!(b.traced_places(), vec![(0, 0), (4, 0), (0, 4), (4, 4)]);
        let mut lv = vec![0u8; 48];
        b.finish(&mut lv, None);
        assert_eq!(lv[3..6], [4, 0, 0]);
        assert_eq!(lv[9..12], [12, 0, 0]);
    }

    #[test]
    fn edge_divides_down_to_single_places() {
        let mut b = CornerBlocks::new(4, 4, Region { top: 0, left: 0, bottom: 4, right: 4 });
        b.run(|x, y| if x >= 1 && y >= 1 { [255; 3] } else { [0; 3] });
        // The three blocks of side 2 that touch the dark row or column divide to single
        // places; the bright one (2, 2) is filled.
        let t = b.traced_places();
        assert!(t.contains(&(1, 1)) && t.contains(&(3, 1)) && t.contains(&(1, 3)));
        assert!(!t.contains(&(3, 3)));
    }

    #[test]
    fn interpolation_takes_rows_first_and_cuts() {
        // From blocks.jsonl case blocks-001, pixel (1, 1): adc6d2.
        let c = [[0xb2, 0xcc, 0xd8], [0xa9, 0xca, 0xd7], [0xb3, 0xc2, 0xce], [0xaa, 0xc1, 0xce]];
        assert_eq!(interpolate(c, 1, 1, 2), [0xad, 0xc6, 0xd2]);
        assert_eq!(interpolate(c, 0, 0, 2), c[0]);
    }
}
