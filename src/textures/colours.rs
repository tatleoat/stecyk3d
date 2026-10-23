// SPDX-License-Identifier: MIT
//! A value made a colour: the colour maps, the colour a bump is shown as, and the conversions
//! between red, green, blue bytes and hue, lightness, saturation bytes.
//!
//! (bump-colour.jsonl),
//! (a grey has no hue),,
//! (Colour maps, blends and the bump's colour)

use crate::colour::{Hls, HlsResult, Rgb32, hls_to_rgb, rgb_to_hls};
use crate::numerics::{sqrt_as, to_i32, x87};
use crate::version::RenderAs;

use super::catalogue::OpenPoint;
use super::lattice::Lattice;
use super::orders::Smoothstep;
use super::settings::{ColourMap, SurfaceContext};

/// A colour: red, green and blue (or hue, lightness and saturation) bytes.
pub type Colour = [u8; 3];

/// The last colour mode number.
pub const LAST_MODE: i64 = 16;
/// Mode 7's weight of its second noise and the sum's divisor.
const MODE7_W: f64 = 1.8;
const MODE7_D: f64 = 2.8;
const MODE7_R: f64 = 0.35714285714285715;
const MODE7_GAIN: f64 = 127.0;
/// Mode 8's constants above 0.2.
const MODE8_GAIN: f64 = 1.25;
const MODE8_F: f64 = 0.4;
const MODE8_LIFT: f64 = 0.85;
const MODE8_TOP: f64 = 0.96;
/// Mode 13's constants.
const MODE13_TOP: f64 = 0.95;
const MODE13_N: f64 = 0.1;
const MODE13_LOW: Colour = [224, 224, 64];
const MODE13_EDGE: Colour = [255, 224, 127];
const MODE13_Y: f64 = 0.08;
const MODE13_S: f64 = 0.12;
/// Mode 14's top.
const MODE14_TOP: f64 = 0.85;
const WHITE: Colour = [255; 3];
/// Mode 8's threshold: below it the dark ramp, from it up a noise that reads `perm`.
const MODE8_LOW: f64 = 0.2;
/// Mode 9's 2π (the double nearest the decimal, not 2π).
#[allow(clippy::approx_constant)]
const MODE9_TURN: f64 = 6.2831853072;
/// Mode 12's factors for red, green and blue.
const MODE12_K: [f64; 3] = [12.566370614, 18.849555921538578, 25.132741228718103];
/// 's term that keeps a zero gradient at the grey.
const BUMP_FLOOR: f64 = 0.005;
/// The double nearest 1/255, by which a byte becomes a single.
const BYTE: f64 = 1.0 / 255.0;

/// cut(x): toward zero to a whole number, as gives it, 0 for a value that is not a
/// number.
pub(crate) fn cut(x: f64) -> i64 {
    if x.is_nan() { 0 } else { i64::from(to_i32(x)) }
}

/// low(n): the low 8 bits.
fn low(n: i64) -> u8 {
    n.rem_euclid(256) as u8
}

/// held(n): n held to 0 to 255.
pub fn held(n: i64) -> u8 {
    n.clamp(0, 255) as u8
}

/// clamp(t): t held to 0 to 1, 0 for a value that is not a number.
pub fn clamp(t: f64) -> f64 {
    if t > 1.0 {
        1.0
    } else if t >= 0.0 {
        t
    } else {
        0.0
    }
}

/// "1 + sine, carried": FSIN's extended value plus 1, rounded once.
fn one_plus_sine(x: f64) -> f64 {
    x87::sin_ext(x).add_to_f64(1.0)
}

/// From + cut((To − From)·k) per channel.
fn toward(from: Colour, to: Colour, k: f64) -> Colour {
    std::array::from_fn(|c| {
        let f = i64::from(from[c]);
        (f + cut((f64::from(to[c]) - f64::from(from[c])) * k)) as u8
    })
}

/// G(t) (eased: E(t)), the gradient of A, B and C.
fn gradient(m: &ColourMap, t: f64, eased: Option<Smoothstep>) -> Colour {
    if t < 0.0 {
        return m.a;
    }
    if t > 1.0 {
        return m.c;
    }
    let u = 2.0 * t;
    let (from, to, k) = if u > 1.0 { (m.b, m.c, u - 1.0) } else { (m.a, m.b, u) };
    toward(from, to, eased.map_or(k, |e| e.eval(k)))
}

/// L(X, Y, t), the two-colour blend.
pub fn two_colour(x: Colour, y: Colour, t: f64) -> Colour {
    if !(t > 0.0) {
        x
    } else if !(t < 1.0) {
        y
    } else {
        toward(x, y, t)
    }
}

/// The noises the modes 7, 8, 10, 13 and 14 and blend mode 14 add: functions of `perm` at
/// the first component's point q as its evaluation leaves it.,
#[derive(Clone, Copy, Debug)]
pub struct PermNoise<'a> {
    pub lattice: Lattice<'a>,
    pub q: [f64; 3],
    /// The version a render is as (mode 7's quotient).
    pub version: RenderAs,
}

impl PermNoise<'_> {
    /// A type's function with the 3-D setting, one evaluation, no octave method.
    /// (the 3-D setting),
    fn at(&self, type_number: i64, p: [f64; 3]) -> Result<f64, OpenPoint> {
        self.lattice.sample(type_number, 3, p).map(|s| s.value).map_err(|_| OpenPoint::NoTable("perm"))
    }

    /// Mode 7's N at a point: (n(p) + 1.8·n(p/2)) / 2.8 − 0.5 as 3.0 and 3.1, the sum times
    /// the double nearest 1/2.8 as 5.0; n type 6. (mode 7; the quotient by
    /// version)
    fn mode7(&self, p: [f64; 3]) -> Result<f64, OpenPoint> {
        let sum = self.at(6, p)? + MODE7_W * self.at(6, p.map(|c| c / 2.0))?;
        let scaled = match self.version {
            RenderAs::Bryce50 => sum * MODE7_R,
            _ => sum / MODE7_D,
        };
        Ok(scaled - 0.5)
    }

    /// F: type 9 at (qx, qz, qy).
    fn f(&self) -> Result<f64, OpenPoint> {
        let [x, y, z] = self.q;
        self.at(9, [x, z, y])
    }

    /// F': type 9 at (qx/8, qy/4, qz/8).
    pub fn blend14(&self) -> Result<f64, OpenPoint> {
        let [x, y, z] = self.q;
        self.at(9, [x / 8.0, y / 4.0, z / 8.0])
    }
}

/// The colour of a map at value v. `ease` is the arrangement of ease the vectors decide;
/// `noise` gives the modes that read `perm` their noises. The sums of modes 7, 8, 10, 13 and
/// 14 are Open (Arrangements): each is formed as written, left to right (point
/// 2).
pub fn map_colour(
    m: &ColourMap,
    v: f64,
    s: SurfaceContext,
    ease: Smoothstep,
    noise: &PermNoise<'_>,
) -> Result<Colour, OpenPoint> {
    Ok(match m.mode {
        0 => m.a,
        1..=3 => {
            let mut c = m.a;
            c[(m.mode - 1) as usize] = low(cut(255.0 * v));
            c
        }
        4 => gradient(m, v, Some(ease)),
        5 => std::array::from_fn(|k| {
            let (a, b) = (f64::from(m.a[k]), f64::from(m.b[k]));
            low(cut((b - a) * v + a))
        }),
        6 => gradient(m, v, None),
        8 if v < MODE8_LOW => {
            let w = 5.0 * v;
            [low(cut(16.0 * w)), low(cut(16.0 + 64.0 * w)), low(cut(64.0 + 92.0 * w))]
        }
        9 => {
            let w = MODE9_TURN * v;
            let s1 = one_plus_sine(w) * 0.5;
            let s2 = one_plus_sine(2.0 * w) * 0.5;
            two_colour(two_colour(m.a, m.c, s1), m.b, s2)
        }
        11 => m.a.map(|x| low(cut(127.0 * one_plus_sine(f64::from(x / 2) * v)))),
        12 => {
            let g = gradient(m, v, None);
            std::array::from_fn(|k| held(cut(f64::from(g[k]) + 64.0 * x87::sin_f64(v * MODE12_K[k]))))
        }
        15 | 16 => {
            let s = if m.mode == 15 { s.slope } else { s.orientation };
            gradient(m, clamp((2.0 * s - 0.25) * v), Some(ease))
        }
        7 => {
            let g = gradient(m, v, None);
            let [x, y, z] = noise.q;
            let turned = [[x, y, z], [z, x, y], [y, z, x]];
            let mut c = [0u8; 3];
            for k in 0..3 {
                c[k] = held(cut(f64::from(g[k]) + MODE7_GAIN * noise.mode7(turned[k])?));
            }
            c
        }
        8 => {
            let f = noise.f()?;
            if (((v - MODE8_LOW) * MODE8_GAIN + MODE8_F * f) + MODE8_LIFT) - s.slope > MODE8_TOP {
                WHITE
            } else {
                gradient(m, v, None)
            }
        }
        10 => gradient(m, clamp((noise.f()? - 0.5) + v), None),
        13 => {
            let n = noise.f()?;
            if (s.altitude + MODE8_F * n) - s.slope > MODE13_TOP {
                WHITE
            } else {
                let y = s.altitude + MODE13_N * n;
                if !(y > 0.0) {
                    MODE13_LOW
                } else if y < MODE13_Y && s.slope < MODE13_S {
                    MODE13_EDGE
                } else {
                    gradient(m, v, Some(ease))
                }
            }
        }
        14 => {
            // : σ taken off twice, each subtraction rounded
            // (not 2σ at once, which differs in the last bit for some inputs)
            if ((s.altitude + MODE8_F * noise.f()?) - s.slope) - s.slope > MODE14_TOP {
                WHITE
            } else {
                gradient(m, v, Some(ease))
            }
        }
        // : a mode outside 0 to 16 (17, 100, 0xFFFFFF80 ...) gives the
        // colour A as mode 0 does; the quarter move elsewhere counts it as a mode other than 0.
        _ => m.a,
    })
}

/// 's colour of a gradient: held(128 + cut(128·gk/ℓ)), ℓ = r(gx² + gy² + gz² + 0.005).
/// The sum is ((gy² + gz²) + gx²) + 0.005 as 3.0 and 3.1 and ((gx² + gy²) + gz²) + 0.005 as
/// 5.0; the root by version.,
pub fn bump_colour(g: [f64; 3], version: RenderAs) -> Colour {
    let [x2, y2, z2] = g.map(|k| k * k);
    let sum = match version {
        RenderAs::Bryce50 => (x2 + y2) + z2,
        _ => (y2 + z2) + x2,
    };
    let l = sqrt_as(version, sum + BUMP_FLOOR);
    g.map(|k| held(128 + cut(128.0 * k / l)))
}

/// A byte as the single nearest k times the double nearest 1/255.
fn single(k: u8) -> f32 {
    (f64::from(k) * BYTE) as f32
}

/// A single back to a byte: low(cut(255·s + 0.5)).
fn byte(s: f32) -> u8 {
    low(cut(255.0 * f64::from(s) + 0.5))
}

/// Red, green, blue bytes to hue, lightness, saturation bytes (on singles). A grey or
/// black produces no hue : Open, refused.,
pub fn to_hls_bytes(c: Colour) -> Result<Colour, OpenPoint> {
    match rgb_to_hls(Rgb32 { r: single(c[0]), g: single(c[1]), b: single(c[2]) }) {
        HlsResult::Chromatic(h) => Ok([byte(h.hue), byte(h.lightness), byte(h.saturation)]),
        HlsResult::Achromatic { .. } => Err(OpenPoint::GreyToHls),
    }
}

/// Hue, lightness, saturation bytes to red, green, blue bytes. Where the conversion gives no
/// colour (a hue of 1 as 3.0 and 3.1): Open, refused.
pub fn from_hls_bytes(c: Colour, version: RenderAs) -> Result<Colour, OpenPoint> {
    let hls = Hls { hue: single(c[0]), lightness: single(c[1]), saturation: single(c[2]) };
    match hls_to_rgb(version, hls) {
        Some(Rgb32 { r, g, b }) => Ok([byte(r), byte(g), byte(b)]),
        None => Err(OpenPoint::HueOne),
    }
}
