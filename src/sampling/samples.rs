// SPDX-License-Identifier: MIT
//! Where a pixel's samples lie, and a pixel's colour from its samples.
//!
//! (answer 0902),
//! (answer 0900, : Premium places and sample numbers, the order of a pixel's
//! draws, draw-order.jsonl). The lens's own arithmetic
//! is not built.
//! (samples)

use super::batch::PointBatch;
use crate::numerics::CRand;
use crate::version::RenderAs;

/// The document's anti-aliasing setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AaSetting {
    /// 0: one sample at the pixel's centre.
    Centre,
    /// 1: one sample at the pixel's corner.
    Corner,
    /// 2: a jittered grid (4 by 4 in Bryce 3D, n by n in Bryce 5).
    Jittered,
    /// 3: one sample at the centre (no menu command gives it).
    CentreAgain,
    /// 4: Bryce 5's Premium setting .
    Premium,
}

impl AaSetting {
    /// . Numbers no document holds are the implementer's choice: the centre.
    pub fn from_document(n: i64) -> AaSetting {
        match n {
            0 => AaSetting::Centre,
            1 => AaSetting::Corner,
            2 => AaSetting::Jittered,
            3 => AaSetting::CentreAgain,
            4 => AaSetting::Premium,
            _ => AaSetting::Centre, // leaves other numbers to the implementer
        }
    }
}

/// Bryce 3D's grid: always 4 by 4.
const BRYCE3D_SIDE: u32 = 4;
/// Bryce 3D's cell side.
const BRYCE3D_CELL: f64 = 0.25;
/// Bryce 3D's jitter span: a quarter of the cell (an eighth either way).
const BRYCE3D_SPAN: f64 = 0.0625;
/// The largest draw of the generator.,
pub(crate) const RAND_MAX: f64 = 32767.0;
/// Bryce 5's Premium: at most 16 rays a side.
pub const MAX_RAYS_A_SIDE: u32 = 16;

/// A draw as a number from 0 to 1: the draw times the binary64 number nearest 1/32767,
/// the product rounded to binary64.
fn unit_draw(rng: &mut CRand) -> f64 {
    rng.next_u15() as f64 * (1.0 / RAND_MAX)
}

/// One sample of a pixel as it is placed: its offset within the pixel and, for the
/// Premium setting, its sample number (0 for every other setting).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedSample {
    /// The sample's index within its pixel, in the order the samples are taken.
    pub index: u32,
    pub dx: f64,
    pub dy: f64,
    /// The Premium sample number (the cell of the lens's, shadows', cones' and ambience's
    /// own grids). question 3 (the sample numbers)
    pub number: u32,
}

/// Places a pixel's samples. Holds the Premium setting's list of sample numbers, which
/// runs on from pixel to pixel through a render : one placer serves one render,
/// its pixels taken in the render's order.
#[derive(Clone, Debug, PartialEq)]
pub struct SamplePlacer {
    pub setting: AaSetting,
    pub version: RenderAs,
    /// Bryce 5's rays a side for the jittered and Premium settings (4 in a new document).
    pub rays_a_side: u32,
    /// The Premium sample numbers in their current order (filled 0 to n² - 1 at the
    /// first Premium pixel). question 3
    numbers: Vec<u32>,
}

impl SamplePlacer {
    /// A placer at a render's start (the sample numbers in order).
    pub fn new(setting: AaSetting, version: RenderAs, rays_a_side: u32) -> SamplePlacer {
        SamplePlacer { setting, version, rays_a_side, numbers: Vec::new() }
    }

    /// The number of samples a pixel takes.
    pub fn samples_per_pixel(&self) -> u32 {
        match self.setting {
            AaSetting::Centre | AaSetting::Corner | AaSetting::CentreAgain => 1,
            AaSetting::Jittered | AaSetting::Premium => {
                let n = self.side();
                n * n
            }
        }
    }

    fn side(&self) -> u32 {
        match self.version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => BRYCE3D_SIDE,
            // Premium: 1 to 16 (Limits: another number is the implementer's choice;
            // the render refuses it, so the clamp here is never reached by a render).
            // Bryce 3D has no Premium setting; a placer asked for it as 3.0 or 3.1 uses
            // 4 by 4 (design; the render refuses it).
            RenderAs::Bryce50 if self.setting == AaSetting::Premium => self.rays_a_side.clamp(1, MAX_RAYS_A_SIDE),
            // Setting 2 as 5.0: the document's number a side as it is, with no bound; with 0
            // no sample is taken and the pixel is black.
            RenderAs::Bryce50 => self.rays_a_side,
        }
    }

    /// A pixel's samples in the order they are taken. Each sample's places are drawn
    /// (x, then y), and for the Premium setting the sample numbers shuffled after the
    /// first sample's two draws, before `each` is called for it; `each` gets the generator
    /// for whatever that sample's lens and shading draw, before the next sample's places
    /// are drawn., questions 1, 3 and 4
    pub fn pixel_samples(&mut self, rng: &mut CRand, mut each: impl FnMut(&mut CRand, PlacedSample)) {
        match self.setting {
            AaSetting::Centre | AaSetting::CentreAgain => {
                each(rng, PlacedSample { index: 0, dx: 0.5, dy: 0.5, number: 0 })
            }
            AaSetting::Corner => each(rng, PlacedSample { index: 0, dx: 0.0, dy: 0.0, number: 0 }),
            AaSetting::Jittered | AaSetting::Premium => {
                let n = self.side();
                let premium = self.setting == AaSetting::Premium;
                if premium && self.numbers.len() != (n * n) as usize {
                    self.numbers = (0..n * n).collect();
                }
                for index in 0..n * n {
                    let (col, row) = (index % n, index / n); // cells row by row
                    let ax = unit_draw(rng); // x drawn before y
                    let ay = unit_draw(rng);
                    let s = if premium {
                        if index == 0 {
                            self.shuffle_numbers(rng);
                        }
                        let (dx, dy) = (premium_offset(col, ax, n), premium_offset(row, ay, n));
                        PlacedSample { index, dx, dy, number: self.numbers[index as usize] }
                    } else {
                        let (dx, dy) = (self.cell_offset(col, ax, n), self.cell_offset(row, ay, n));
                        PlacedSample { index, dx, dy, number: 0 }
                    };
                    each(rng, s);
                }
            }
        }
    }

    /// The Premium shuffle: n² / 2 (whole division) swaps, each of two places drawn as
    /// trunc((draw · (1/32767)) · (n² - 1)), the first place from the first draw. The
    /// list runs on from the pixel before. question 3 (the sample numbers)
    fn shuffle_numbers(&mut self, rng: &mut CRand) {
        let count = self.numbers.len();
        let top = (count - 1) as f64;
        for _ in 0..count / 2 {
            let a = (unit_draw(rng) * top) as usize;
            let b = (unit_draw(rng) * top) as usize;
            self.numbers.swap(a, b);
        }
    }

    /// One coordinate of a jittered sample of setting 2. (Bryce 3D: centre of
    /// the cell moved by up to an eighth of the cell either way); (Bryce 5: by
    /// (d - 1/2) of a cell, kept in binary64, the centre not rounded)
    fn cell_offset(&self, i: u32, a: f64, n: u32) -> f64 {
        match self.version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => {
                (i as f64 + 0.5) * BRYCE3D_CELL + (a - 0.5) * BRYCE3D_SPAN
            }
            RenderAs::Bryce50 => {
                let c = 1.0 / n as f64;
                (i as f64 + 0.5) * c + (a - 0.5) * c
            }
        }
    }

    /// Append the picture points of pixel (px, py), tagged `pixel`, to `out`, drawing the
    /// places only (for renders whose lens and shading take no draws).
    pub fn place_pixel(&mut self, px: u32, py: u32, pixel: u32, rng: &mut CRand, out: &mut PointBatch) {
        let (fx, fy) = (px as f64, py as f64);
        self.pixel_samples(rng, |_, s| out.push(fx + s.dx, fy + s.dy, pixel));
    }
}

/// One coordinate of a Premium sample: the cell's centre f32((i + 1/2) · (1/n)), then
/// f32(centre + (1/n) · (d - 1/2)), the product and the sum in binary64.
/// question 3
pub(crate) fn premium_offset(i: u32, d: f64, n: u32) -> f64 {
    let c = 1.0 / n as f64;
    let centre = ((i as f64 + 0.5) * c) as f32 as f64;
    (centre + c * (d - 0.5)) as f32 as f64
}

/// How a pixel's colour is carried before it becomes levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelScale {
    /// Bryce 3D: 0 to 255.
    Levels255,
    /// Bryce 5: 0 to 1.
    Unit,
}

impl ChannelScale {
    pub fn for_version(v: RenderAs) -> ChannelScale {
        match v {
            RenderAs::Bryce30 | RenderAs::Bryce31 => ChannelScale::Levels255,
            RenderAs::Bryce50 => ChannelScale::Unit,
        }
    }
}

/// The binary32 sum of a pixel's samples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelSum {
    pub sum: [f32; 3],
    pub count: u32,
}

/// A sample's binary32 channel clamped to 0 to 1: below 0 gives +0, above 1 gives 1, not a
/// number counts as 0; minus zero may stay (it adds as zero).
pub fn clamp_channel(c: f32) -> f32 {
    if c.is_nan() || c < 0.0 {
        0.0
    } else if c > 1.0 {
        1.0
    } else {
        c
    }
}

/// A colour rule's result as the binary32 channels the shading hands back.
pub fn to_binary32(rgb: [f64; 3]) -> [f32; 3] {
    rgb.map(|c| c as f32)
}

impl PixelSum {
    /// Add one sample, in the order samples are taken: each addition rounded to binary32,
    /// the first added to 0.
    pub fn add(&mut self, rgb: [f32; 3]) {
        for (s, c) in self.sum.iter_mut().zip(rgb) {
            *s += clamp_channel(c);
        }
        self.count += 1;
    }

    /// The pixel's channels on the version's scale: the binary32 sum times a binary64
    /// scale (255 / n in Bryce 3D, 1 / n in Bryce 5, the quotient rounded to binary64), the
    /// product rounded to binary64 and then to binary32.
    pub fn colour(&self, scale: ChannelScale) -> [f32; 3] {
        let n = self.count.max(1) as f64;
        let k = match scale {
            ChannelScale::Levels255 => 255.0 / n,
            ChannelScale::Unit => 1.0 / n,
        };
        self.sum.map(|s| (s as f64 * k) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : setting 2 as 5.0 takes the document's number a side as it is,
    /// with no bound; with 0 no sample is taken and the pixel is black.
    #[test]
    fn setting_two_as_five_takes_any_number_a_side() {
        let five = |n| SamplePlacer::new(AaSetting::Jittered, RenderAs::Bryce50, n).samples_per_pixel();
        assert_eq!(five(0), 0);
        assert_eq!(five(1), 1);
        assert_eq!(five(20), 400);
        assert_eq!(SamplePlacer::new(AaSetting::Jittered, RenderAs::Bryce31, 20).samples_per_pixel(), 16);
        let mut rng = CRand::new(1);
        let mut taken = 0;
        SamplePlacer::new(AaSetting::Jittered, RenderAs::Bryce50, 0).pixel_samples(&mut rng, |_, _| taken += 1);
        assert_eq!(taken, 0);
        assert_eq!(PixelSum::default().colour(ChannelScale::Unit), [0.0; 3]);
    }
}
