// SPDX-License-Identifier: MIT
//! A picture as a texture: the colour, the value and the bump gradient a user's picture gives
//! at a point of the texture space.
//!
//! (the function: steps 1 to 4; Variants: the neighbours as 3.0 and 3.1 and as
//! 5.0; Errors: the points left Open), (the picture: width, height, pixels as alpha,
//! red, green, blue in raster order, top row first)
//! (Picture textures)

use std::fmt;

use crate::numerics::to_i32;
use crate::version::RenderAs;

/// A pixel: alpha, red, green, blue, 8 bits each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pixel(pub [u8; 4]);

impl Pixel {
    /// From the word alpha × 2^24 + red × 2^16 + green × 2^8 + blue.
    pub fn from_word(w: u32) -> Pixel {
        Pixel(w.to_be_bytes())
    }

    /// The word alpha × 2^24 + red × 2^16 + green × 2^8 + blue.
    pub fn word(self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn alpha(self) -> u8 {
        self.0[0]
    }

    /// Red, green, blue.
    pub fn rgb(self) -> [u8; 3] {
        [self.0[1], self.0[2], self.0[3]]
    }
}

/// A decoded picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}

/// A picture whose pixel count is not its width times its height, or whose size is 0.
/// Picture textures (a picture has at least one pixel)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureShape {
    pub width: u32,
    pub height: u32,
    pub pixels: usize,
}

impl fmt::Display for PictureShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a picture of {} × {} pixels given {} pixels", self.width, self.height, self.pixels)
    }
}

impl std::error::Error for PictureShape {}

impl Picture {
    /// `pixels` in raster order, top row first; `width` and `height` at least 1.
    pub fn new(width: u32, height: u32, pixels: Vec<Pixel>) -> Result<Picture, PictureShape> {
        let count = (width as usize).checked_mul(height as usize);
        if width == 0 || height == 0 || count != Some(pixels.len()) {
            return Err(PictureShape { width, height, pixels: pixels.len() });
        }
        Ok(Picture { width, height, pixels })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[Pixel] {
        &self.pixels
    }

    /// The pixel in column `c` and row `r` (row 0 the top), both in range.
    fn at(&self, c: i64, r: i64) -> Result<Pixel, PicturePointOpen> {
        // : an index outside the picture has
        // no pixel, and what the original gives there is not specified.
        if c < 0 || r < 0 || c >= i64::from(self.width) || r >= i64::from(self.height) {
            return Err(PicturePointOpen { xf: c as f64, yf: r as f64 });
        }
        Ok(self.pixels[r as usize * self.width as usize + c as usize])
    }
}

/// How a pixel's neighbours are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Neighbours {
    /// 3.0 and 3.1: c1 = W(c0 + 1, w), r1 = W(r0 + 1, h); the lower pair of the last column
    /// is the last two columns.
    Paired,
    /// 5.0: c1 = W(ix + 1, w), r1 = W(iy + 1, h).
    Plain,
}

impl Neighbours {
    pub fn of(version: RenderAs) -> Neighbours {
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => Neighbours::Paired,
            RenderAs::Bryce50 => Neighbours::Plain,
        }
    }
}

/// What a picture gives at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureSample {
    /// Red, green, blue levels.
    pub colour: [u8; 3],
    pub value: f64,
    /// (g1, g2, 0).
    pub gradient: [f64; 3],
}

/// A point leaves Open: w·t1 or h − t2·h not a finite number of less than 2^31 in size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PicturePointOpen {
    pub xf: f64,
    pub yf: f64,
}

impl fmt::Display for PicturePointOpen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a picture read at ({}, {}) in its pixels, beyond the integer conversion's range (TX-4.3 Open)",
            self.xf, self.yf
        )
    }
}

impl std::error::Error for PicturePointOpen {}

/// K, the single nearest 1/255 as a double.
pub const VALUE_SCALE: f64 = 0.003921568859368563;
/// q, the double nearest 1/255 (one division).
pub const GRADIENT_SCALE: f64 = 1.0 / 255.0;

/// W(i, n): i brought into 0 to n − 1, one short for negative i.
fn wrap(i: i64, n: i64) -> i64 {
    if i == i64::from(i32::MIN) {
        // : -2^31 negated is -2^31 again in 32
        // bits, so W(-2^31, n) is n - 1 + (2^31 mod n), outside 0 to n - 1 unless n is a power of two.
        return n - 1 + (1_i64 << 31) % n;
    }
    if i >= 0 { i % n } else { n - ((-i) % n) - 1 }
}

/// B(a, b, t), channel by channel.
fn blend(a: Pixel, b: Pixel, t: f64) -> Pixel {
    if !(t > 0.0) {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    Pixel(std::array::from_fn(|k| {
        let (ca, cb) = (i32::from(a.0[k]), i32::from(b.0[k]));
        // The level lies between ca and cb, since 0 < t < 1 (the fraction of a place).
        (ca + to_i32(f64::from(cb - ca) * t)) as u8
    }))
}

impl Picture {
    /// The colour, value and gradient at the texture point (t1, t2) as `version` reads it.
    pub fn sample(&self, version: RenderAs, t1: f64, t2: f64) -> Result<PictureSample, PicturePointOpen> {
        self.sample_switched(version, t1, t2, false)
    }

    /// `sample` where, with `single` (the switch 0x40 as 5.0), the colour and the value are those
    /// of the pixel p00 alone, not blended.
    pub fn sample_switched(
        &self,
        version: RenderAs,
        t1: f64,
        t2: f64,
        single: bool,
    ) -> Result<PictureSample, PicturePointOpen> {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        // Step 1: the place in the picture.
        let xf = w * t1;
        let yf = h - t2 * h;
        // : every point is read by the same
        // steps;'s conversion gives index 0 for a number that is not one, an infinity, or
        // 2^63 and more, and the low 32 bits otherwise.
        let (ix, iy) = (to_i32(xf), to_i32(yf));
        let fx = xf - f64::from(ix);
        let fy = yf - f64::from(iy);
        let (ix, iy) = (i64::from(ix), i64::from(iy));
        let (wn, hn) = (i64::from(self.width), i64::from(self.height));

        // Step 2: the four pixels.
        let c0 = wrap(ix, wn);
        let r0 = wrap(iy, hn);
        let (p00, p10, p01, p11) = match Neighbours::of(version) {
            Neighbours::Plain => {
                // The integer ix + 1 in 32 bits (2^31 - 1 + 1 is -2^31).
                let c1 = wrap(i64::from((ix as i32).wrapping_add(1)), wn);
                let r1 = wrap(i64::from((iy as i32).wrapping_add(1)), hn);
                (self.at(c0, r0)?, self.at(c1, r0)?, self.at(c0, r1)?, self.at(c1, r1)?)
            }
            Neighbours::Paired => {
                let c1 = wrap(c0 + 1, wn);
                let r1 = wrap(r0 + 1, hn);
                let (b0, b1) = if c0 == wn - 1 && wn >= 2 { (wn - 2, wn - 1) } else { (c0, c1) };
                (self.at(c0, r0)?, self.at(c1, r0)?, self.at(b0, r1)?, self.at(b1, r1)?)
            }
        };

        // Step 3: the blend.
        let pixel = if single { p00 } else { blend(blend(p00, p10, fx), blend(p01, p11, fx), fy) };

        // Step 4: the outputs.
        let a = |p: Pixel| i32::from(p.alpha());
        Ok(PictureSample {
            colour: pixel.rgb(),
            value: f64::from(pixel.alpha()) * VALUE_SCALE,
            gradient: [
                f64::from(a(p00) - a(p10)) * GRADIENT_SCALE,
                f64::from(a(p01) - a(p00)) * GRADIENT_SCALE,
                0.0,
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_one_short() {
        assert_eq!(wrap(-1, 5), 3);
        assert_eq!(wrap(-5, 5), 4);
        assert_eq!(wrap(7, 5), 2);
        assert_eq!(wrap(-1, 1), 0);
    }

    #[test]
    fn worked_example() {
        let px = [0xFFFF0000, 0xC800FF00, 0x640000FF, 0x00FFFF00].map(Pixel::from_word).to_vec();
        let p = Picture::new(2, 2, px).unwrap();
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let s = p.sample(v, 0.25, 0.75).unwrap();
            assert_eq!(s.colour, [0x80, 0x7F, 0x40]);
            assert_eq!(s.value, 139.0 * VALUE_SCALE);
            assert_eq!(s.gradient, [55.0 * GRADIENT_SCALE, -155.0 * GRADIENT_SCALE, 0.0]);
        }
    }

    #[test]
    fn points_beyond_the_conversion() {
        let p = Picture::new(1, 1, vec![Pixel::default()]).unwrap();
        assert!(p.sample(RenderAs::Bryce50, f64::NAN, 0.0).is_ok());
        assert!(p.sample(RenderAs::Bryce50, 3e9, 0.0).is_ok());
        assert!(p.sample(RenderAs::Bryce50, 0.0, f64::INFINITY).is_ok());
        // An index of -2^31 with a side that is not a power of two has no pixel.
        let q = Picture::new(3, 1, vec![Pixel::default(); 3]).unwrap();
        assert!(q.sample(RenderAs::Bryce50, -2147483648.0 / 3.0, 0.0).is_err());
        let r = Picture::new(4, 1, vec![Pixel::default(); 4]).unwrap();
        assert!(r.sample(RenderAs::Bryce50, -2147483648.0 / 4.0, 0.0).is_ok());
    }
}
