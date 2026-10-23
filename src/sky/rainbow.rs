// SPDX-License-Identifier: MIT
//! The rainbow: a ring on a flat disc facing the sun, at a fixed distance from each ray's
//! origin, that moves a ray's colour toward the bow's colour.
//!
//! , ("The
//! rainbow after the air"),
//!
//! Notation as: binary64 operations one Rust `f64` operation each (the product, then the
//! sum), ⟨x⟩ `x as f32`.

use crate::geom::Vec3;
use crate::numerics::{exp2_as, pow, sqrt_as};
use crate::version::RenderAs;

use super::colours::{HueOne, hls_with};

/// The rule's small numbers: |den|, N11 and s are tested against this. (the
/// rainbow, steps 2 and 5: 10^-7)
const RAINBOW_TINY: f64 = 1e-7;
/// Bit 0x800: the plain sky; as 5.0 no bow.
const PLAIN_BIT: u32 = 0x800;
/// Bit 0x4000: the rainbow.
pub const RAINBOW_BIT: u32 = 0x4000;
/// Bit 0x4000000: the bow's colours take the main light's colour and intensity.
const LIGHT_COLOURS: u32 = 0x400_0000;
/// Bit 0x8000000: a second bow.
const SECOND_BOW: u32 = 0x800_0000;
/// Bit 0x10000000: nothing below the floor N12.
const FLOOR: u32 = 0x1000_0000;
/// Bit 0x20000000: nothing above a cloud layer's height.
const CEILING: u32 = 0x2000_0000;
/// The cloud bits: 0x40 the lower layer, 0x20 the upper.,
const LOWER_LAYER: u32 = 0x40;
const UPPER_LAYER: u32 = 0x20;
/// The upper layer's height over the lower's.
const UPPER_RISE: f64 = 24.4140625;

/// The tangents of 42 and 51 degrees, the doubles nearest them.
const T1: f64 = 0.9004040442978399;
const T2: f64 = 1.2348971565350515;
/// PER nearest 2.857142857142857 (1/0.35), E8 nearest −11.541560327111707 (−8·log2 e), M nearest
/// 0.35, T3 nearest 1/3.
const PER: f64 = 2.857142857142857;
const E8: f64 = -11.541560327111707;
const M: f64 = 0.35;
const T3: f64 = 1.0 / 3.0;
/// The first bow's hue runs 0.7 across it.
const HUE_SPAN: f64 = 0.7;
/// The bow's colour: HLS(h, L7, S8), L7 and S8 the singles nearest 0.7 and 0.8.
const BOW_LIGHTNESS: f32 = 0.7;
const BOW_SATURATION: f32 = 0.8;

/// The rainbow's numbers fixed for a render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rainbow {
    version: RenderAs,
    /// The word's bits the rainbow reads (0x4000000, 0x8000000, 0x10000000, 0x20000000, 0x20
    /// and 0x40).
    bits: u32,
    /// S', the sun's direction.
    sun: Vec3,
    /// N7 to N14, by their numbers 0 to 7.
    distance: f64,
    strength: f64,
    width1: f64,
    width2: f64,
    fade_height: f64,
    floor: f64,
    ceiling_default: f64,
    /// The lower layer's height c (the sky part's cloud height).
    cloud_height: f64,
    /// The main light's colour Λ and intensity J .
    light_colour: [f32; 3],
    light_intensity: f64,
    /// What HLS gives for a hue of 1 (the bow's hue never reaches it for finite numbers).
    hue_one: HueOne,
}

/// What `Rainbow::new` reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RainbowInputs {
    /// The word the bits are read from: the stored word as 3.0 and 3.1; W as 5.0 (a Bryce 3D sky
    /// taken over plain has bits 0x20 and 0x40 cleared there).
    pub switches: u32,
    /// The fifteen numbers of the discs and rings (offset 448); N7 to N14 are the last eight.
    pub discs: [f64; 15],
    /// S' .
    pub sun: Vec3,
    /// c, the lower layer's height.
    pub cloud_height: f64,
    /// The main light's colour and intensity .
    pub light_colour: [f32; 3],
    pub light_intensity: f64,
}

impl Rainbow {
    /// The rainbow of a render, or `None` where bit 0x4000 is clear or the sun's height is
    /// below 0 or not a number (never drawn then). As 5.0 a word with none of bits 0x2, 0x4 and
    /// 0x8 draws it only where N8 > 0 (`five_plain_needs_strength`).
    pub fn new(version: RenderAs, i: &RainbowInputs, five_plain_needs_strength: bool) -> Option<Rainbow> {
        if i.switches & RAINBOW_BIT == 0 {
            return None;
        }
        // As 5.0 it is not drawn with bit 0x800 set.
        if version == RenderAs::Bryce50 && i.switches & PLAIN_BIT != 0 {
            return None;
        }
        // Never when the sun's height S'y is below 0 or not a number.
        if !(i.sun[1] >= 0.0) {
            return None;
        }
        if five_plain_needs_strength && !(i.discs[8] > 0.0) {
            return None;
        }
        let [_, _, _, _, _, _, _, n7, n8, n9, n10, n11, n12, _, n14] = i.discs;
        Some(Rainbow {
            version,
            bits: i.switches,
            sun: i.sun,
            distance: n7,
            strength: n8,
            width1: n9,
            width2: n10,
            fade_height: n11,
            floor: n12,
            ceiling_default: n14,
            cloud_height: i.cloud_height,
            light_colour: i.light_colour,
            light_intensity: i.light_intensity,
            hue_one: HueOne::of(version),
        })
    }

    /// The ray's colour `c` with the bow over it, for a ray of origin `o`, direction `d` as it
    /// was sent, from the parameter `lo` and, where it met a surface, up to `hi`.
    /// A continued ray ('s straight-on ray) passes its own origin, its start 0.0001 and its
    /// own hit.
    pub fn over(&self, c: [f32; 3], o: Vec3, d: Vec3, lo: f64, hi: Option<f64>) -> [f32; 3] {
        match self.blend(o, d, lo, hi) {
            None => c,
            Some((a, b)) => blend(c, b, a),
        }
    }

    /// The bow's amount and colour where the ray meets it.
    fn blend(&self, o: Vec3, d: Vec3, lo: f64, hi: Option<f64>) -> Option<(f64, [f32; 3])> {
        let version = self.version;
        let n7 = self.distance;
        // 2. The disc: n = −S', B = O + N7·n (each product, then each sum).
        let n = [-self.sun[0], -self.sun[1], -self.sun[2]];
        let b: Vec3 = std::array::from_fn(|k| o[k] + n7 * n[k]);
        let num = ((b[0] - o[0]) * n[0] + (b[1] - o[1]) * n[1]) + (b[2] - o[2]) * n[2];
        let den = (d[0] * n[0] + d[1] * n[1]) + d[2] * n[2];
        if !(den.abs() > RAINBOW_TINY) {
            return None;
        }
        let t = num / den;
        if !(t > lo) {
            return None;
        }
        if let Some(hi) = hi {
            if t >= hi {
                return None;
            }
        }
        // H = O + t·D, each Hi = t·Di + Oi, kept as a double.
        let h: Vec3 = std::array::from_fn(|k| t * d[k] + o[k]);

        // 3. Floor and ceiling.
        if self.bits & FLOOR != 0 && !(h[1] >= self.floor) {
            return None;
        }
        let q = if self.bits & CEILING != 0 && self.bits & LOWER_LAYER != 0 {
            let q = self.cloud_height;
            if h[1] > q {
                return None;
            }
            q
        } else if self.bits & CEILING != 0 && self.bits & UPPER_LAYER != 0 {
            let q = self.cloud_height + UPPER_RISE;
            if h[1] > q {
                return None;
            }
            q
        } else {
            self.ceiling_default
        };

        // 4. The ring: e = H − B, r = sqrt((ey·ey + ez·ez) + ex·ex).
        let e: Vec3 = std::array::from_fn(|k| h[k] - b[k]);
        let r = sqrt_as(version, (e[1] * e[1] + e[2] * e[2]) + e[0] * e[0]);
        let n8 = self.strength;
        let rho1 = n7 * T1;
        let w1 = n7 * self.width1;
        let half = w1 * 0.5;
        let (mut a, hue);
        if !((r - rho1).abs() >= half) {
            let p = (r - (rho1 - half)) / w1;
            let mut h1 = (1.0 - p) * HUE_SPAN;
            if !(h1 >= 0.5) {
                h1 = pow(h1 + h1, 2.0) * 0.5;
            }
            let x = if h1 >= M { (h1 - M) * PER } else { exp2_as(version, (h1 * PER) * E8) };
            a = (1.0 - x) * n8;
            hue = h1;
        } else if self.bits & SECOND_BOW != 0 {
            let rho2 = n7 * T2;
            let w2 = self.width2 * n7;
            let half2 = w2 * 0.5;
            if (r - rho2).abs() >= half2 {
                return None;
            }
            let h2 = ((r - (rho2 - half2)) / w2) * HUE_SPAN;
            a = (n8 * T3) * (1.0 - (h2 - M).abs() * PER);
            hue = h2;
        } else {
            return None;
        }

        // 5. Fades by height.
        let n11 = self.fade_height;
        if n11 > RAINBOW_TINY {
            let m = (h[1] - self.floor).abs() / n11;
            a = (if m > 1.0 { 1.0 } else { m }) * a;
        }
        let s = (q - self.floor).abs() * 0.5;
        if s > RAINBOW_TINY {
            let m = (h[1] - q).abs() / s;
            a = (if m > 1.0 { 1.0 } else { m }) * a;
        }

        // 6. The bow's colour.
        let mut colour = hls_with(self.hue_one, hue as f32, BOW_LIGHTNESS, BOW_SATURATION).unwrap_or([0.0; 3]);
        if self.bits & LIGHT_COLOURS != 0 {
            colour = std::array::from_fn(|k| ((f64::from(self.light_colour[k]) * f64::from(colour[k])) * self.light_intensity) as f32);
        }
        Some((a, colour))
    }
}

/// 7. The blend of c toward b by a, with the roundings of each channel.
fn blend(c: [f32; 3], b: [f32; 3], a: f64) -> [f32; 3] {
    let red = ((a * (f64::from(b[0]) - f64::from(c[0]))) as f32) + c[0];
    let green = ((a * f64::from(b[1] - c[1])) as f32) + c[1];
    let blue = (a * f64::from(b[2] - c[2]) + f64::from(c[2])) as f32;
    [red, green, blue]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(switches: u32, sun: Vec3) -> RainbowInputs {
        let mut discs = [0.0; 15];
        // N7 distance, N8 strength, N9 and N10 widths, N11 fade, N12 floor, N14 ceiling.
        discs[7] = 1000.0;
        discs[8] = 0.9;
        discs[9] = 0.12;
        discs[10] = 0.12;
        discs[11] = 0.0;
        discs[12] = -1000.0;
        discs[14] = 1000.0;
        RainbowInputs { switches, discs, sun, cloud_height: 0.0, light_colour: [1.0; 3], light_intensity: 1.0 }
    }

    #[test]
    fn gates() {
        let v = RenderAs::Bryce31;
        assert!(Rainbow::new(v, &inputs(0, [0.0, 0.5, 0.8]), false).is_none());
        assert!(Rainbow::new(v, &inputs(RAINBOW_BIT, [0.0, -0.1, 0.9]), false).is_none());
        assert!(Rainbow::new(v, &inputs(RAINBOW_BIT, [0.0, f64::NAN, 0.9]), false).is_none());
        assert!(Rainbow::new(v, &inputs(RAINBOW_BIT, [0.0, 0.5, 0.8]), false).is_some());
        // As 5.0, a plain word draws it only with N8 above 0.
        let mut i = inputs(RAINBOW_BIT, [0.0, 0.5, 0.8]);
        i.discs[8] = 0.0;
        assert!(Rainbow::new(RenderAs::Bryce50, &i, true).is_none());
        assert!(Rainbow::new(RenderAs::Bryce50, &i, false).is_some());
    }

    // (step 2: no bow where the ray does not meet the disc: den = 0, t not above lo,
    // t not below hi)
    #[test]
    fn a_ray_that_misses_the_disc_is_unchanged() {
        let r = Rainbow::new(RenderAs::Bryce31, &inputs(RAINBOW_BIT, [0.0, 0.5, 0.8660254037844386]), false).unwrap();
        let c = [0.1, 0.2, 0.3];
        // Parallel to the disc.
        assert_eq!(r.over(c, [0.0; 3], [0.0, 0.8660254037844386, -0.5], 0.0, None), c);
        // The surface is nearer than the disc.
        assert_eq!(r.over(c, [0.0; 3], [0.0, 0.0, 1.0], 0.0, Some(1.0)), c);
    }

    #[test]
    fn a_zero_amount_blends_nothing() {
        assert_eq!(blend([0.1, 0.2, 0.3], [0.9, 0.8, 0.7], 0.0), [0.1, 0.2, 0.3]);
        assert_eq!(blend([0.1, 0.2, 0.3], [0.9, 0.8, 0.7], 1.0), [0.9, 0.8, 0.7]);
    }
}
