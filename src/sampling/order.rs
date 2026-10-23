// SPDX-License-Identifier: MIT
//! The passes of a render and the order of pixels.
//!
//! (order of pixels)

/// One rendered pixel: the top-left pixel of a block, with the block's side.
/// Coordinates count from the corner of the region rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelVisit {
    pub x: u32,
    pub y: u32,
    pub block: u32,
}

/// Where a saved render stood, as the document keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedPlace {
    pub block: u32,
    pub mask: u32,
    pub column: u32,
    pub row: u32,
}

/// The block sizes of a progressive render.
const PROGRESSIVE_BLOCKS: [u32; 5] = [16, 8, 4, 2, 1];
/// A stored block size with all 32 bits set.
const ALL_BITS: u32 = u32::MAX;
const SINGLE_PASS: [u32; 1] = [1];

/// Which places a pass leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Skip {
    Nothing,
    /// Column and row both multiples of this (the previous pass's block).
    Multiples(u32),
    /// Column and row both with no bit in common with this mask (a resumed pass).
    Mask(u32),
}

impl Skip {
    fn leaves_out(self, x: u32, y: u32) -> bool {
        match self {
            Skip::Nothing => false,
            Skip::Multiples(m) => x % m == 0 && y % m == 0,
            Skip::Mask(m) => x & m == 0 && y & m == 0,
        }
    }
}

/// The pixels of a render in the order they are rendered.
#[derive(Clone, Debug)]
pub struct RenderOrder {
    width: u32,
    height: u32,
    blocks: Vec<u32>,
    /// The places a later pass leaves out are those with no bit in common with the previous
    /// block size less one (the passes from the block size 2^32 - 1), not the multiples of it.
    mask_skips: bool,
    pass: usize,
    skip: Skip,
    x: u32,
    y: u32,
    done: bool,
}

impl RenderOrder {
    /// . `saved` is where a render the program makes on opening a document
    /// stood; a render that is asked for starts over (pass `None`).
    pub fn new(width: u32, height: u32, progressive: bool, saved: Option<SavedPlace>) -> RenderOrder {
        let blocks: Vec<u32> = if progressive { PROGRESSIVE_BLOCKS.to_vec() } else { SINGLE_PASS.to_vec() };
        let mut order = RenderOrder {
            width,
            height,
            blocks,
            mask_skips: false,
            pass: 0,
            skip: Skip::Nothing,
            x: 0,
            y: 0,
            done: width == 0 || height == 0,
        };
        if let Some(s) = saved {
            if !progressive {
                // Block 1 whatever is stored; nothing left out.
                order.x = s.column;
                order.y = s.row;
            } else if s.block == ALL_BITS {
                // : the passes of the block sizes 2^32 - 1,
                // 2^31 - 1, ..., 15, 7, 3, 1, each half the one before with the remainder
                // dropped; the first leaves out the places with no bit in common with the stored
                // mask, a later one those with none in common with the previous block size less
                // one; the exception for the block size 16 does not apply.
                let mut b = ALL_BITS;
                let mut v = vec![b];
                while b > 1 {
                    b /= 2;
                    v.push(b);
                }
                order.blocks = v;
                order.mask_skips = true;
                order.x = s.column;
                order.y = s.row;
                order.skip = Skip::Mask(s.mask);
            } else if let Some(p) = order.blocks.iter().position(|&b| b == s.block) {
                order.pass = p;
                order.x = s.column;
                order.y = s.row;
                // Block 16 is taken as the first pass and leaves out nothing.
                order.skip = if p == 0 { Skip::Nothing } else { Skip::Mask(s.mask) };
            }
            // A stored block that is no pass size: Open; the render starts over.
        }
        order
    }

    fn block(&self) -> u32 {
        self.blocks[self.pass]
    }

    /// Bring (x, y) to a place inside the picture: the next row when past the right
    /// edge, the next pass when past the bottom. Returns false when the render is done.
    fn settle(&mut self) -> bool {
        loop {
            if self.done {
                return false;
            }
            if self.x >= self.width {
                self.x = 0;
                self.y = self.y.saturating_add(self.block());
                continue;
            }
            if self.y >= self.height {
                self.pass += 1;
                if self.pass >= self.blocks.len() {
                    self.done = true;
                    return false;
                }
                let previous = self.blocks[self.pass - 1];
                self.skip = if self.mask_skips { Skip::Mask(previous - 1) } else { Skip::Multiples(previous) };
                self.x = 0;
                self.y = 0;
                continue;
            }
            return true;
        }
    }
}

impl Iterator for RenderOrder {
    type Item = PixelVisit;

    fn next(&mut self) -> Option<PixelVisit> {
        while self.settle() {
            let (x, y, b) = (self.x, self.y, self.block());
            self.x = self.x.saturating_add(b);
            if !self.skip.leaves_out(x, y) {
                return Some(PixelVisit { x, y, block: b });
            }
        }
        None
    }
}

/// A draw from 0 below `n`, without bias: 15-bit draws joined until they can reach `n - 1`,
/// the last one shortened to the bits that remain, and a draw at or above `n` redrawn. The
/// draws land uniformly in 0 to 2^k - 1 with `n` more than half of that, so a draw is kept
/// with probability above a half: under 2 draws expected for any `n`. (Joining four full
/// draws instead — a number from 0 to 2^60 — would redraw almost always once `n` falls
/// below the picture's pixel count, which is where the shuffle ends.)
fn below(rng: &mut crate::numerics::CRand, n: u64) -> u64 {
    // Bits that can reach n - 1; 0 for n = 1, which gives 0 and returns at once.
    let bits = 64 - (n - 1).leading_zeros();
    loop {
        let mut r = 0u64;
        let mut left = bits;
        while left > 0 {
            let take = left.min(15);
            r = (r << take) | ((rng.next_u15() as u64) & ((1u64 << take) - 1));
            left -= take;
        }
        if r < n {
            return r;
        }
    }
}

/// The region's pixels row by row from the top, each a visit of block 1 (the live picture's rows order).
pub fn row_pixels(width: u32, height: u32) -> Vec<PixelVisit> {
    let n = width as u64 * height as u64;
    (0..n).map(|p| PixelVisit { x: (p % width as u64) as u32, y: (p / width as u64) as u32, block: 1 }).collect()
}

/// The `width` by `height` region's pixels in a fixed random order: a Fisher-Yates shuffle
/// of the row order by a generator seeded with `seed`, each pixel a visit of block 1. The
/// same seed and size always give the same permutation. (the live
/// picture: the picture developed out of noise)
pub fn scattered_pixels(width: u32, height: u32, seed: u32) -> Vec<PixelVisit> {
    let n = width as u64 * height as u64;
    let mut v: Vec<PixelVisit> = (0..n)
        .map(|p| PixelVisit { x: (p % width as u64) as u32, y: (p / width as u64) as u32, block: 1 })
        .collect();
    let mut rng = crate::numerics::CRand::new(seed);
    for i in (1..v.len()).rev() {
        let j = below(&mut rng, i as u64 + 1) as usize;
        v.swap(i, j);
    }
    v
}
