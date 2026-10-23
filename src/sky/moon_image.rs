// SPDX-License-Identifier: MIT
//! The user's picture of the moon, as the sky reads it: three bytes a pixel and the bilinear
//! read of.
//!
//! , (the picture of
//! 256 by 256 pixels; arle unpacked as rle2),

use crate::textures::picture::Picture;

/// A picture's colour bytes: red, green, blue of each pixel in raster order, top row first (the
/// alpha is not used).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoonImage {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
}

impl MoonImage {
    /// From a decoded picture.
    pub fn from_picture(p: &Picture) -> MoonImage {
        MoonImage { width: p.width(), height: p.height(), rgb: p.pixels().iter().map(|px| px.rgb()).collect() }
    }

    /// From colour bytes in raster order (`width` times `height` of them, both at least 2 for a
    /// read to be asked). tests
    pub fn new(width: u32, height: u32, rgb: Vec<[u8; 3]>) -> MoonImage {
        assert_eq!(rgb.len(), width as usize * height as usize, "a picture holds width times height pixels");
        MoonImage { width, height, rgb }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn at(&self, column: i64, row: i64) -> [u8; 3] {
        // The places of step 1 lie between 0.01 and 0.99, so a neighbour is inside the
        // picture; held at the edge all the same (no panic on a place outside).
        let c = column.clamp(0, i64::from(self.width) - 1) as usize;
        let r = row.clamp(0, i64::from(self.height) - 1) as usize;
        self.rgb[r * self.width as usize + c]
    }

    /// The three bytes at (x, y), both binary32 (x along the columns from the left edge, y along
    /// the rows from the top): px = x·(W − 1) and py = y·(H − 1) in binary64, ix and iy their
    /// whole parts, wx and wy the whole parts of the fractions times 256; each byte is formed
    /// along the rows, top = a + ⌊(b − a)·wx/256⌋, then between the rows, top + ⌊(bottom −
    /// top)·wy/256⌋ (⌊⌋ toward minus infinity).
    pub fn read(&self, x: f32, y: f32) -> [u8; 3] {
        let px = f64::from(x) * f64::from(self.width - 1);
        let py = f64::from(y) * f64::from(self.height - 1);
        let (ix, iy) = (px.trunc(), py.trunc());
        let wx = ((px - ix) * 256.0).trunc() as i32;
        let wy = ((py - iy) * 256.0).trunc() as i32;
        let (ix, iy) = (ix as i64, iy as i64);
        let (p00, p10, p01, p11) = (self.at(ix, iy), self.at(ix + 1, iy), self.at(ix, iy + 1), self.at(ix + 1, iy + 1));
        std::array::from_fn(|k| {
            let along = |a: u8, b: u8| i32::from(a) + (i32::from(b) - i32::from(a)).saturating_mul(wx).div_euclid(256);
            let top = along(p00[k], p10[k]);
            let bottom = along(p01[k], p11[k]);
            (top + (bottom - top).saturating_mul(wy).div_euclid(256)) as u8
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_a_pixel_the_pixel() {
        let img = MoonImage::new(3, 3, (0..9).map(|i| [i * 10, i * 20, 255 - i]).collect());
        // x = 0.5 with W = 3: px = 1.0 exactly, wx = 0; the same in y.
        assert_eq!(img.read(0.5, 0.5), [40, 80, 251]);
    }

    #[test]
    fn a_falling_step_rounds_toward_minus_infinity() {
        // Two columns, 10 then 0: at wx = 128 (px = 0.5) the byte is 10 + ⌊−10·128/256⌋ = 10 − 5.
        let img = MoonImage::new(2, 2, vec![[10, 0, 0], [0, 0, 0], [10, 0, 0], [0, 0, 0]]);
        assert_eq!(img.read(0.5, 0.0)[0], 5);
        // wx = 192: 10 + ⌊−10·192/256⌋ = 10 + ⌊−7.5⌋ = 2.
        assert_eq!(img.read(0.75, 0.0)[0], 2);
    }
}
