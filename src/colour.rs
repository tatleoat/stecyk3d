// SPDX-License-Identifier: MIT
//! Colour as hue, lightness and saturation: the hexcone conversions to and from RGB.
//! ; sources rows O4, G47
//!
//! All values are 32-bit singles (Inputs/Outputs). The precision of each step
//! was chosen by measuring against the released vectors; see the design note. No fused
//! multiply-add is used.

use crate::version::RenderAs;

/// An RGB colour in single precision, channels in 0..1.
/// (Types)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rgb32 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb32 {
    pub const fn new(r: f32, g: f32, b: f32) -> Rgb32 {
        Rgb32 { r, g, b }
    }
}

/// Hue (a fraction of the full circle, 0..1, not degrees), lightness and saturation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Hls {
    pub hue: f32,
    pub lightness: f32,
    pub saturation: f32,
}

impl Hls {
    pub const fn new(hue: f32, lightness: f32, saturation: f32) -> Hls {
        Hls { hue, lightness, saturation }
    }
}

/// The result of converting RGB to HLS.
/// (Types). A grey has no hue, and black has no saturation either. The
/// documentation does not yet say what values they take, so this type
/// does not invent any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HlsResult {
    /// The colour has a hue: all three components are determined.
    Chromatic(Hls),
    /// The colour is a grey (all channels equal): only the lightness is determined.
    Achromatic { lightness: f32 },
}

impl HlsResult {
    pub fn lightness(&self) -> f32 {
        match *self {
            HlsResult::Chromatic(hls) => hls.lightness,
            HlsResult::Achromatic { lightness } => lightness,
        }
    }
}

/// The double nearest 1/6.
const SIXTH: f64 = 0.16666666666666666;

/// RGB to hue, lightness, saturation by the hexcone model.
/// ; (Algorithm, RGB to HLS). Single precision throughout.
pub fn rgb_to_hls(c: Rgb32) -> HlsResult {
    // : every quantity exact or a double, except those rounded
    // to a single below. Sums and differences of two singles are exact in double.
    let Rgb32 { r, g, b } = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let (v, x) = (f64::from(max), f64::from(min));
    let lightness = ((v + x) / 2.0) as f32;

    let chroma = v - x;
    if !(lightness > 0.0) || !(chroma > 0.0) {
        return HlsResult::Achromatic { lightness };
    }

    let d = if lightness <= 0.5 { f64::from((v + x) as f32) } else { (2.0 - v) - x };
    let saturation = (chroma / d) as f32;

    // Distance of each channel from the largest, each rounded to a single.
    let dist = |c: f32| f64::from(((v - f64::from(c)) / chroma) as f32);
    let (dr, dg, db) = (dist(r), dist(g), dist(b));

    // The hue in sixths, rounded to a single; then divided by 6, rounded again.
    // Ties for the largest go to red, then green., Smith 1978
    let sextants = if r == max {
        if g == min { 5.0 + db } else { 1.0 - dg }
    } else if g == max {
        if b == min { 1.0 + dr } else { 3.0 - db }
    } else if r == min {
        3.0 + dg
    } else {
        5.0 - dr
    };
    let sextants = sextants as f32;
    let hue = (f64::from(sextants) * SIXTH) as f32;

    HlsResult::Chromatic(Hls { hue, lightness, saturation })
}

/// Hue, lightness, saturation to RGB by the hexcone model, as a render as `version` does it.
/// ; (Algorithm, HLS to RGB; Precision).
///
/// `None` means the conversion produces nothing: a colour it was to set keeps the value it
/// held . That is the result at a hue of exactly 1 for renders as 3.0 and 3.1.
/// Hues outside 0..1 other than 1 (and NaN) are the implementer's choice ; this
/// module produces nothing for them too. A largest channel not above zero gives black
/// whatever the hue, before the hue is looked at .
pub fn hls_to_rgb(version: RenderAs, c: Hls) -> Option<Rgb32> {
    let Hls { hue, lightness, saturation } = c;

    // : a value is "stored" when rounded to a single,
    // every other operation is a binary64 one.
    let (l, s) = (f64::from(lightness), f64::from(saturation));
    let m2 = if lightness <= 0.5 { (s + 1.0) * l } else { (l + s) - l * s } as f32; // stored
    if !(m2 > 0.0) {
        return Some(Rgb32::new(0.0, 0.0, 0.0));
    }
    let m2d = f64::from(m2);
    let m1 = (l + l) - m2d; // not stored: a binary64 value

    // : v = 6h exact, k its whole part cut toward zero, f = v - k.
    let v = f64::from(hue) * 6.0;
    let k = v.trunc();
    if !(hue >= 0.0 && k <= 5.0) {
        // k above 5: nothing as 3.0 and 3.1; Bryce 5 takes k = 6 as k = 5 with f = v - 5.
        // Other hues (below 0, not a number, above 7/6): the implementer's choice, nothing.
        if version == RenderAs::Bryce50 && k == 6.0 {
            return Some(sextant(5, m2, m1, v - 5.0, m2d));
        }
        return None;
    }
    Some(sextant(k as u8, m2, m1, v - k, m2d))
}

/// The channels of sextant `k`: the rising value r = (((m2 - m1)/m2)·f)·m2 + m1 and the
/// falling value q = m2 - (((m2 - m1)/m2)·f)·m2, each stored.
fn sextant(k: u8, m2: f32, m1: f64, f: f64, m2d: f64) -> Rgb32 {
    let ramp = (((m2d - m1) / m2d) * f) * m2d;
    let rising = (ramp + m1) as f32;
    let falling = (m2d - ramp) as f32;
    let low = m1 as f32;
    let (r, g, b) = match k {
        0 => (m2, rising, low),
        1 => (falling, m2, low),
        2 => (low, m2, rising),
        3 => (low, falling, m2),
        4 => (rising, low, m2),
        _ => (m2, low, falling),
    };
    Rgb32::new(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests cover only this module's own design decisions (Types).
    // Bit-exact expectations come from the released vectors in tests/colour_vectors.rs.

    #[test]
    fn grey_has_no_hue() {
        assert_eq!(
            rgb_to_hls(Rgb32::new(0.25, 0.25, 0.25)),
            HlsResult::Achromatic { lightness: 0.25 }
        );
        assert_eq!(rgb_to_hls(Rgb32::default()), HlsResult::Achromatic { lightness: 0.0 });
    }

    #[test]
    fn hue_just_above_one_is_the_last_sextant_as_5_0_only() {
        // : k above 5 gives nothing as 3.0 and 3.1; Bryce 5 takes k = 6
        // as k = 5 with f = v - 5 (the hue 1 itself is the case a single can hold within 0..1).
        let h = Hls::new(1.0, 0.5, 0.5);
        assert_eq!(hls_to_rgb(RenderAs::Bryce30, h), None);
        assert_eq!(hls_to_rgb(RenderAs::Bryce31, h), None);
        let got = hls_to_rgb(RenderAs::Bryce50, h).unwrap();
        // (m2, m1, q) with f = 1: the falling value is m1 (to within a last bit).
        assert_eq!(got.r, 0.75);
        assert!((got.b - got.g).abs() < 1e-7);
    }

    #[test]
    fn black_and_dark_have_lightness_alone() {
        // : l not above 0 stops there with l alone.
        assert_eq!(rgb_to_hls(Rgb32::new(0.0, 0.0, 0.0)), HlsResult::Achromatic { lightness: 0.0 });
    }

    #[test]
    fn hues_outside_produce_nothing() {
        // (hues outside 0..1 other than 1 are the implementer's choice)
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            for h in [-0.1f32, 1.5, f32::NAN, f32::INFINITY] {
                assert_eq!(hls_to_rgb(v, Hls::new(h, 0.5, 0.5)), None);
            }
        }
    }
}
