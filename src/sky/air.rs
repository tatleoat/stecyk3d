// SPDX-License-Identifier: MIT
//! The air over a ray: fog that is thickest at the ground, then haze that thickens with
//! distance, as 3.0 and 3.1, and as 5.0 with the substrate's `haze-five`.
//!
//! (revised 3e6e0f0 and"As 5.0"; answer
//! to,, haze-pictures; revised log
//! 5926, "Further rays": `AirPath`, air-mirror-pictures),
//! (a ray that meets nothing); (bit 0x80000000: the rates with Color Perspective,
//! `haze_rates_shifted`)
//! (The air; The air over a tree of rays),
//!
//! Notation as: every binary64 operation one Rust `f64` operation, ⟨x⟩ `x as f32`,
//! ⊕ ⊖ ⊗ Rust `f32` operations. Where allows binary64 or the extended register, and
//! leaves the grouping of a product or the order of a sum free, binary64 in the order
//! written is used.

use crate::geom::Vec3;
use crate::numerics::exp2_as;
use crate::version::RenderAs;

use super::colours::AIR_ON;
use super::tints::{FOG_TINT, HAZE_TINT, Tints};

/// Bits that keep the air off: 0x400 (set by the reader for the named views) and
/// 0x800.
const AIR_OFF: u32 = 0x400 | 0x800;
/// The double nearest 0.2 in θ's exponent.
const THETA_FACTOR: f64 = 0.2;
/// k in θ's exponent, by version.
const THETA_K_30: f64 = -12.0;
const THETA_K_31: f64 = -30.0;
/// The double nearest 0.333333 (not 1/3) in γ.
const GAMMA_FACTOR: f64 = 0.333333;
/// The haze's start σ.
const HAZE_START: f64 = 4.8828125;
/// The haze's scale Σ = first − second·h, by version (3.1: two by h).
const SCALE_30: (f64, f64) = (390.625, 76.171875);
const SCALE_31_ABOVE_1: (f64, f64) = (195.3125, 37.109375);
const SCALE_31_OTHERWISE: (f64, f64) = (390.625, 232.421875);
/// r is held at this from above.
const HAZE_HOLD: f64 = 1e9;
/// The fog's start φ and base β.
const FOG_START: f64 = 0.390625;
const FOG_BASE: f64 = 19.53125;
/// Below this fog, N is further multiplied by 10f.
const THIN_FOG: f64 = 0.1;
/// As 5.0, the fog's factor (2P − 0.02)·5.
const FIVE_FOG_SHIFT: f64 = 0.02;
const FIVE_FOG_FACTOR: f64 = 5.0;
/// As 5.0, the haze's shift 100·(2O − 1).
const FIVE_HAZE_SHIFT: f64 = 100.0;
/// As 5.0, a ray going up by less than this has its own hold.
const FIVE_LEVEL_RISE: f64 = 1e-10;
const FIVE_LEVEL_HOLD: f64 = 1e10;

/// The three numbers of the substrate's `haze-five`, as the user's table holds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazeFive {
    /// O, the haze offset.
    pub offset: f64,
    /// T, the haze thickness.
    pub thickness: f64,
    /// P, the fog offset.
    pub fog_offset: f64,
}

/// The sky part's numbers the air reads, and the main light's height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirInputs {
    pub switches: u32,
    /// H and h.
    pub haze_colour: [f32; 3],
    pub haze: f64,
    /// F, e and f.
    pub fog_colour: [f32; 3],
    pub fog_height: f64,
    pub fog: f64,
    /// c.
    pub cloud_height: f64,
    /// ℓ, the height of the main light's direction .
    pub light_height: f64,
}

/// The numbers fixed for a render. `None` where the air does not act at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    haze: Option<Haze>,
    fog: Option<Fog>,
    cloud_height: f64,
    /// As 5.0: `haze-five`'s numbers.
    five: Option<HazeFive>,
    /// The tints (bits 0x2000 and 0x200000): the haze's and the fog's colours drawn toward the
    /// sun's and the moon's by the ray's direction.
    tints: Option<AirTints>,
}

/// The tints and which of the two colours take them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AirTints {
    tints: Tints,
    haze: bool,
    fog: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Haze {
    colour: [f32; 3],
    /// ρr, ρg, ρb.
    rates: [f32; 3],
    /// Σ.
    scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fog {
    colour: [f32; 3],
    height: f64,
    amount: f64,
}

/// One ray as the air takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirRay {
    /// D as the ray was sent: of unit length for a camera ray, as sends it for a
    /// further ray (never brought to unit length here).
    pub direction: Vec3,
    /// s, the height of the point its path started from.
    pub start_height: f64,
    /// L, the parameter along D from the path's start (for a D of unit length, the distance).
    pub length: f64,
}

/// The path a ray's air is measured along.
/// (The air over a tree of rays)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AirPath {
    /// The ray starts a path of its own at its origin: a camera ray, a reflected or a
    /// transmitted ray.
    #[default]
    Own,
    /// The ray continues the path of the ray that met a blended surface ('s ray straight
    /// on): the path started at the point `start` and had come `before` (that ray's L) when it
    /// met the surface.,
    Continued { start: Vec3, before: f64 },
}

impl AirPath {
    /// The ray of this path that meets a surface at the parameter `t` from its origin, whose
    /// height is `origin_height`: L = t on a path of its own, the path's L so far plus t on a
    /// continued one; s the path's start.
    pub fn to_hit(self, origin_height: f64, direction: Vec3, t: f64) -> AirRay {
        match self {
            AirPath::Own => AirRay { direction, start_height: origin_height, length: t },
            AirPath::Continued { start, before } => AirRay { direction, start_height: start[1], length: before + t },
        }
    }

    /// The ray of this path that meets nothing: L = |c/(Dy + Dy)| (+infinity where Dy = 0)
    /// with the ray's own Dy, the path so far not added; s the path's start.
    pub fn to_nothing(self, origin_height: f64, direction: Vec3, air: &Air) -> AirRay {
        let length = air.sky_length(direction[1]);
        match self {
            AirPath::Own => AirRay { direction, start_height: origin_height, length },
            AirPath::Continued { start, .. } => AirRay { direction, start_height: start[1], length },
        }
    }

    /// The path the ray sends straight on from a blended surface carries, where the
    /// ray of this path (origin height `origin_height`) met that surface at `t`.
    /// (Further rays: a continued ray continues the path of the ray that met the
    /// surface; its L is that ray's L plus its own parameter)
    pub fn continued(self, origin: Vec3, t: f64) -> AirPath {
        match self {
            AirPath::Own => AirPath::Continued { start: origin, before: t },
            AirPath::Continued { start, before } => AirPath::Continued { start, before: before + t },
        }
    }

    /// E: the point the path started from, for a ray whose origin is `origin`.
    pub fn start_point(self, origin: Vec3) -> Vec3 {
        match self {
            AirPath::Own => origin,
            AirPath::Continued { start, .. } => start,
        }
    }

    /// The length of the path already behind the ray (0 on a path of its own).
    pub fn travelled(self) -> f64 {
        match self {
            AirPath::Own => 0.0,
            AirPath::Continued { before, .. } => before,
        }
    }
}

impl Air {
    /// The air of a render as 3.0 or 3.1, or `None` where it does not act (bit 0x80 clear,
    /// bit 0x400 or 0x800 set, or neither fog nor haze).
    pub fn new(version: RenderAs, a: &AirInputs) -> Option<Air> {
        if a.switches & AIR_ON == 0 || a.switches & AIR_OFF != 0 {
            return None;
        }
        let fog = (a.fog_height > 0.0 && a.fog > 0.0).then_some(Fog {
            colour: a.fog_colour,
            height: a.fog_height,
            amount: a.fog,
        });
        let haze = (a.haze > 0.0).then(|| Haze::new(version, a));
        (fog.is_some() || haze.is_some()).then_some(Air { haze, fog, cloud_height: a.cloud_height, five: None, tints: None })
    }

    /// The air of a render as 5.0: 3.1's rule (k = −30, 3.1's scales), with `haze-five`'s
    /// numbers taken in; the haze acts only when T > 0 as well. `shift` is the haze's shift
    /// colour ('s value for a Bryce 3D sky) where the word has Color Perspective: the
    /// haze then has the rates of.
    pub fn new_five(a: &AirInputs, five: HazeFive, shift: Option<[f32; 3]>) -> Option<Air> {
        if a.switches & AIR_ON == 0 || a.switches & AIR_OFF != 0 {
            return None;
        }
        let fog = (a.fog_height > 0.0 && a.fog > 0.0).then_some(Fog {
            colour: a.fog_colour,
            height: a.fog_height,
            amount: a.fog,
        });
        let haze = (a.haze > 0.0 && five.thickness > 0.0).then(|| Haze::new_with(RenderAs::Bryce50, a, true, shift));
        (fog.is_some() || haze.is_some()).then_some(Air { haze, fog, cloud_height: a.cloud_height, five: Some(five), tints: None })
    }

    /// The air of a Bryce 5 sky (sky part of version 7) as 5.0: `a.switches` is W, `five` the
    /// sky part's own haze offset, haze thickness and fog offset, `a.cloud_height` the c of
    /// the paragraph (the cloud height, or 5). The haze acts with W's bit 0x1, the fog with
    /// bit 0x10, nothing with bit 0x400 or 0x800 (bit 0x1000000 is refused before). The
    /// haze's reach is 3.1's two scales where `wide_reach` (the second word's bit 0x1), else
    /// 390.625 − 76.171875·h; `shift` is the haze's shift colour where W has Color
    /// Perspective.
    pub fn new_bryce5(a: &AirInputs, five: HazeFive, wide_reach: bool, shift: Option<[f32; 3]>) -> Option<Air> {
        // Bit 0x1000000 of a Bryce 5 sky's word: no air is laid.
        if a.switches & (AIR_OFF | 0x100_0000) != 0 {
            return None;
        }
        let fog = (a.switches & 0x10 != 0 && a.fog_height > 0.0 && a.fog > 0.0).then_some(Fog {
            colour: a.fog_colour,
            height: a.fog_height,
            amount: a.fog,
        });
        let haze = (a.switches & 0x1 != 0 && a.haze > 0.0 && five.thickness > 0.0)
            .then(|| Haze::new_with(RenderAs::Bryce50, a, wide_reach, shift));
        (fog.is_some() || haze.is_some()).then_some(Air { haze, fog, cloud_height: a.cloud_height, five: Some(five), tints: None })
    }

    /// The air with the tints of the word's bits 0x2000 (haze) and 0x200000 (fog) laid in:
    /// `tints` reads S', the sun's colour and the two numbers. A word with neither bit changes
    /// nothing.
    pub fn with_tints(mut self, switches: u32, tints: Tints) -> Air {
        let (haze, fog) = (switches & HAZE_TINT != 0, switches & FOG_TINT != 0);
        if haze || fog {
            self.tints = Some(AirTints { tints, haze, fog });
        }
        self
    }

    /// The length gives a ray that meets nothing (bit 0x1 clear): |c/(Dy + Dy)| where
    /// Dy ≠ 0, +infinity where Dy = 0.
    pub fn sky_length(&self, dy: f64) -> f64 {
        if dy != 0.0 { (self.cloud_height / (dy + dy)).abs() } else { f64::INFINITY }
    }

    /// The ray's colour after the air: the fog, then the haze.
    pub fn over(&self, version: RenderAs, colour: [f32; 3], ray: &AirRay) -> [f32; 3] {
        let mut c = colour;
        // F' and H': the colours, or tinted for the ray's direction.
        let tinted = |colour: [f32; 3], on: fn(&AirTints) -> bool| match &self.tints {
            Some(t) if on(t) => t.tints.colour_for(colour, ray.direction),
            _ => colour,
        };
        if let Some(f) = &self.fog {
            c = f.over(c, ray, self.five, tinted(f.colour, |t| t.fog));
        }
        if let Some(h) = &self.haze {
            c = h.over(version, c, ray, self.cloud_height, self.five, tinted(h.colour, |t| t.haze));
        }
        c
    }
}

/// ρ, the haze's three rates as singles, formed whatever bit 0x80 holds (a star's glow reads
/// them).,
pub fn haze_rates(version: RenderAs, a: &AirInputs) -> [f32; 3] {
    Haze::new(version, a).rates
}

/// The factor of the haze shift colour's squares in the rates.
const SHIFT_FACTOR: f64 = 250.0;

/// ρ as 5.0 (3.1's) with Bryce 5's Color Perspective (bit 0x80000000 of the word as 5.0): each
/// rate takes ⟨Xk·Xk·250 + ρ̂k⟩ in place of ρ̂k before θ thins it, ρk = ⟨θ·⟨Xk·Xk·250 + ρ̂k⟩⟩,
/// with X the haze's shift colour and ρ̂ = ⟨(a − 1)·γ + 1⟩, 1, ⟨(b − 1)·γ + 1⟩. Here the rates
/// reach only the stars' glows.,
pub fn haze_rates_shifted(a: &AirInputs, shift: [f32; 3]) -> [f32; 3] {
    let version = RenderAs::Bryce50;
    let theta = 1.0 - exp2_as(version, a.haze * THETA_FACTOR * THETA_K_31 * std::f64::consts::LOG2_E);
    let [hr, hg, hb] = a.haze_colour.map(f64::from);
    let gamma = (hr + hg + hb) * GAMMA_FACTOR;
    let q = 4.0 * a.light_height;
    let low = if 2.0 - q > 0.5 { 2.0 - q } else { 0.5 };
    let high = if q > 0.5 { q } else { 0.5 };
    let before = [f64::from(((low - 1.0) * gamma + 1.0) as f32), 1.0, f64::from(((high - 1.0) * gamma + 1.0) as f32)];
    std::array::from_fn(|k| {
        let x = f64::from(shift[k]);
        let shifted = f64::from((x * x * SHIFT_FACTOR + before[k]) as f32);
        (theta * shifted) as f32
    })
}

impl Haze {
    fn new(version: RenderAs, a: &AirInputs) -> Haze {
        Haze::new_with(version, a, true, None)
    }

    /// A Bryce 5 sky's haze: `wide_reach` clear gives the scale 390.625 − 76.171875·h whatever
    /// h is; `shift` gives the rates of Color Perspective. (As 5.0, a Bryce 5
    /// sky),
    fn new_with(version: RenderAs, a: &AirInputs, wide_reach: bool, shift: Option<[f32; 3]>) -> Haze {
        let h = a.haze;
        // As 5.0, 3.1's k and scales (As 5.0).
        let k = match version {
            RenderAs::Bryce30 => THETA_K_30,
            _ => THETA_K_31,
        };
        let theta = 1.0 - exp2_as(version, h * THETA_FACTOR * k * std::f64::consts::LOG2_E);
        let [hr, hg, hb] = a.haze_colour.map(f64::from);
        let gamma = (hr + hg + hb) * GAMMA_FACTOR;
        let q = 4.0 * a.light_height;
        let low = if 2.0 - q > 0.5 { 2.0 - q } else { 0.5 };
        let high = if q > 0.5 { q } else { 0.5 };
        let rate = |m: f64| (theta * f64::from(((m - 1.0) * gamma + 1.0) as f32)) as f32;
        let rates = match shift {
            Some(x) => haze_rates_shifted(a, x),
            None => [rate(low), theta as f32, rate(high)],
        };
        let (first, second) = match version {
            RenderAs::Bryce30 => SCALE_30,
            _ if !wide_reach => SCALE_30,
            _ if h > 1.0 => SCALE_31_ABOVE_1,
            _ => SCALE_31_OTHERWISE,
        };
        Haze { colour: a.haze_colour, rates, scale: first - second * h }
    }

    fn over(
        &self,
        version: RenderAs,
        c: [f32; 3],
        ray: &AirRay,
        cloud_height: f64,
        five: Option<HazeFive>,
        colour: [f32; 3],
    ) -> [f32; 3] {
        let dy = ray.direction[1];
        let mut l = ray.length;
        if five.is_some() && dy > 0.0 && dy < FIVE_LEVEL_RISE {
            if l >= FIVE_LEVEL_HOLD {
                l = FIVE_LEVEL_HOLD;
            }
        } else if dy > 0.0 {
            let kappa = (cloud_height / (dy + dy)).abs();
            if l >= kappa {
                l = kappa;
            }
        }
        let mut r = match five {
            None => l - HAZE_START,
            Some(f) => (2.0 * f.thickness * l - HAZE_START) + FIVE_HAZE_SHIFT * (2.0 * f.offset - 1.0),
        };
        if !(r > 0.0) {
            return c;
        }
        if r > HAZE_HOLD {
            r = HAZE_HOLD;
        }
        let x = r / self.scale;
        std::array::from_fn(|k| {
            let w = (1.0 - exp2_as(version, -f64::from(self.rates[k]) * x * std::f64::consts::LOG2_E)) as f32;
            c[k] + (colour[k] - c[k]) * w
        })
    }
}

impl Fog {
    fn over(&self, c: [f32; 3], ray: &AirRay, five: Option<HazeFive>, colour: [f32; 3]) -> [f32; 3] {
        let (dy, s, l) = (ray.direction[1], ray.start_height, ray.length);
        let r = l - FOG_START;
        if !(r > 0.0) {
            return c;
        }
        let depth = |t: f64| {
            let d = 1.0 - (t * dy + s) / self.height;
            if d > 0.0 { d } else { 0.0 } // not above 0, or not a number
        };
        let m = (depth(FOG_START) + depth(l)) * 0.5;
        let mut n = match five {
            None => r * m,
            Some(f) => r * m * (2.0 * f.fog_offset - FIVE_FOG_SHIFT) * FIVE_FOG_FACTOR,
        };
        if self.amount < THIN_FOG {
            n *= 10.0 * self.amount;
        }
        if !(n > 0.0) {
            return c;
        }
        let a = n / (FOG_BASE + (1.0 - FOG_BASE) * self.amount);
        let alpha = (if a < 1.0 { a } else { 1.0 }) as f32;
        std::array::from_fn(|k| c[k] + (colour[k] - c[k]) * alpha)
    }
}

#[cfg(test)]
mod bryce5_tests {
    // A Bryce 5 sky's air.
    use super::*;

    fn inputs(w: u32, haze: f64, c: f64) -> AirInputs {
        AirInputs {
            switches: w,
            haze_colour: [1.0, 1.0, 1.0],
            haze,
            fog_colour: [0.5; 3],
            fog_height: 0.0,
            fog: 0.0,
            cloud_height: c,
            light_height: 0.0,
        }
    }

    const NUMBERS: HazeFive = HazeFive { offset: 0.5, thickness: 0.4, fog_offset: 0.5 };

    fn ray(dy: f64, length: f64) -> AirRay {
        AirRay { direction: [0.0, dy, 0.0], start_height: 0.0, length }
    }

    /// With the second word's bit 0x1 clear the scale is 390.625 - 76.171875*h whatever h is
    /// (here h > 1, where 3.1 would take its other pair).
    #[test]
    fn clear_reach_scale() {
        let h = 1.5;
        let air = Air::new_bryce5(&inputs(0x1 | 0x2, h, 5.0), NUMBERS, false, None).expect("acts");
        let r = ray(-0.5, 100.0);
        let out = air.over(RenderAs::Bryce50, [0.0; 3], &r);
        let theta = 1.0 - (-6.0 * h).exp();
        let r_len = (2.0 * 0.4 * 100.0 - 4.8828125) + 100.0 * (2.0 * 0.5 - 1.0);
        let x = r_len / (390.625 - 76.171875 * h);
        // gamma = 3 * 0.333333, light_height 0: low = 2
        let gamma = 3.0 * 0.333333;
        let rate_r = theta * ((2.0 - 1.0) * gamma + 1.0);
        let want = 1.0 - (-rate_r * x).exp();
        assert!((f64::from(out[0]) - want).abs() < 1e-5, "{} {}", out[0], want);
        let wide = Air::new_bryce5(&inputs(0x1 | 0x2, h, 5.0), NUMBERS, true, None).unwrap();
        assert_ne!(wide.over(RenderAs::Bryce50, [0.0; 3], &r), out);
    }

    /// The bits that keep the air off, and the haze and fog switches.
    #[test]
    fn when_it_acts() {
        let a = |w| Air::new_bryce5(&inputs(w, 0.5, 5.0), NUMBERS, true, None);
        assert!(a(0x1 | 0x2).is_some());
        assert!(a(0x1 | 0x2 | 0x400).is_none());
        assert!(a(0x1 | 0x2 | 0x800).is_none());
        assert!(a(0x2).is_none(), "no haze switch");
        assert!(a(0x1 | 0x2 | 0x80).is_some(), "bit 0x80 has no say");
        let flat = HazeFive { thickness: 0.0, ..NUMBERS };
        assert!(Air::new_bryce5(&inputs(0x3, 0.5, 5.0), flat, true, None).is_none(), "T = 0");
        // the fog needs bit 0x10, a fog height and 2P - 0.02 above 0
        let mut f = inputs(0x2 | 0x10, 0.0, 5.0);
        f.fog = 0.5;
        f.fog_height = 2.0;
        let air = Air::new_bryce5(&f, NUMBERS, true, None).expect("fog acts");
        assert_ne!(air.over(RenderAs::Bryce50, [0.0; 3], &ray(-0.5, 50.0)), [0.0; 3]);
        let none = HazeFive { fog_offset: 0.01, ..NUMBERS };
        let air = Air::new_bryce5(&f, none, true, None).expect("built");
        assert_eq!(air.over(RenderAs::Bryce50, [0.0; 3], &ray(-0.5, 50.0)), [0.0; 3], "2P - 0.02 not above 0");
        f.switches = 0x2;
        assert!(Air::new_bryce5(&f, NUMBERS, true, None).is_none(), "no fog switch");
    }

    /// Color Perspective: the rates are's, here in the air.
    #[test]
    fn colour_perspective_rates() {
        let a = inputs(0x1 | 0x2, 0.5, 5.0);
        let shift = [0.1f32, 0.2, 0.3];
        let air = Air::new_bryce5(&a, NUMBERS, true, Some(shift)).unwrap();
        assert_eq!(air.haze.unwrap().rates, haze_rates_shifted(&a, shift));
        let plain = Air::new_bryce5(&a, NUMBERS, true, None).unwrap();
        assert_ne!(plain.haze.unwrap().rates, air.haze.unwrap().rates);
    }

    /// Color Perspective in a Bryce 3D sky taken over (As 5.0):'s shift colour
    /// (0, 0.05, 0.25) gives the haze's rates; red's rate is as without the bit (its
    /// shift colour is 0), the fog is unchanged, and the reach is 3.1's two scales.
    #[test]
    fn colour_perspective_bryce3d_sky() {
        let a = inputs(0x1 | 0x80, 0.5, 5.0);
        let five = HazeFive { offset: 0.4, thickness: 0.7, fog_offset: 0.3 };
        let shift = [0.0f32, 0.05, 0.25];
        let with = Air::new_five(&a, five, Some(shift)).unwrap().haze.unwrap();
        let without = Air::new_five(&a, five, None).unwrap().haze.unwrap();
        assert_eq!(with.rates, haze_rates_shifted(&a, shift));
        assert_eq!(with.rates[0], without.rates[0], "red's shift colour is 0");
        assert!(with.rates[1] > without.rates[1] && with.rates[2] > without.rates[2]);
        assert_eq!(with.scale, without.scale);
    }
}
