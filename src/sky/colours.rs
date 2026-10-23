// SPDX-License-Identifier: MIT
//! The sky's colours: the main light, the dome light and the plain sky.
//!
//! (revised 8cbba36: the Scope takes
//! every switch word of the bits 0x2, 0x4, 0x8, 0x80 and 0x100; to
//! revised again: the dome light as 5.0 for every word of the Scope, HLS's hue of 1 as 5.0,
//! bit 0x1 and its further bits in the Scope)
//! (The sky's colours),
//!
//! Every binary64 operation is one Rust `f64` operation (no fusing, no wider
//! intermediate); ⟨x⟩ is `x as f32`; "a ⊕ b", "a ⊖ b", "a ⊗ b" are Rust `f32` operations,
//! which are correctly rounded (Notation).

use crate::geom::Vec3;
use crate::numerics::exp2_as;
use crate::version::RenderAs;

/// The sky switch word's dome light bit.,
pub const DOME_LIGHT_ON: u32 = 0x100;
/// The glow colour's hue follows the sun (step 2).
pub const GLOW_HUE_A: u32 = 0x2;
/// A second hue that follows the sun, above the horizon (step 2).
pub const GLOW_HUE_B: u32 = 0x4;
/// The dome light is the dome colour as stored.
pub const DOME_AS_STORED: u32 = 0x8;
/// The air (haze and fog) over every ray. Scope,
pub const AIR_ON: u32 = 0x80;
/// The sky with atmosphere (3.0, 3.1; nothing as 5.0). Scope,
pub const ATMOSPHERE: u32 = 0x1;
/// The bits state.
const SKY_BITS: u32 = GLOW_HUE_A | GLOW_HUE_B | DOME_AS_STORED | AIR_ON | DOME_LIGHT_ON; // Scope
/// As 3.0 and 3.1, also's bits. Scope,
const SCOPE_BITS: u32 = SKY_BITS | LIT_AIR_BITS; // Scope,
/// The further bits stated with bit 0x1 as 3.0 and 3.1. ("Further bits with bit
/// 0x1": 0x400, 0x800, 0x10000, 0x40000, 0x100000, 0x2000000 [format])
const FURTHER_BITS: u32 = 0x400 | 0x800 | 0x10000 | 0x20000 | 0x40000 | 0x100000 | 0x2000000;
/// The bits stated for the three versions whatever the rest of the word: the sun in the view's
/// frame (0x200), the tints (0x2000, 0x200000), the rainbow (0x4000 and its bits 0x4000000,
/// 0x8000000, 0x10000000, 0x20000000). Apart from 0x200 none changes the sky's colours or the
/// lights., (The sun's direction in the view's frame; The
/// rainbow),
pub const STATED_BITS: u32 =
    0x200 | 0x2000 | 0x20_0000 | 0x4000 | 0x400_0000 | 0x800_0000 | 0x1000_0000 | 0x2000_0000; // Scope,
/// The light in the air's bits, stated as 3.0 and 3.1.
const LIT_AIR_BITS: u32 = super::lit_air::LIT_AIR_COARSE | super::lit_air::LIT_AIR_FINE;

/// B, the double nearest 1/255.
pub(crate) const B: f64 = 1.0 / 255.0;
/// The main light's intensity factor, the double nearest 1.1255.
const SUN_FACTOR: f64 = 1.1255;
/// The moon light's red and green factor and blue factor.
const MOON_RG: f64 = 0.5;
const MOON_B: f64 = 0.625;
/// The moon light's caps: red and green, blue (the doubles nearest them, and the singles
/// nearest them).
const MOON_CAP_RG: f64 = 0.374;
const MOON_CAP_B: f64 = 0.438;
/// The overhead colour's saturation, the single nearest 240/255. step 1
const SZ: f32 = (240.0f64 / 255.0) as f32;
/// Bit 0x2's lightness above the horizon, the single nearest 200/255. step 2 (L2)
const L2: f32 = (200.0f64 / 255.0) as f32; // step 2 (L2)
/// Bit 0x2's hue below the horizon, the single nearest 220/255. step 2 (J2)
const J2: f32 = (220.0f64 / 255.0) as f32; // step 2 (J2)
/// The cloud hue colour's lightness, the single nearest 130/255. step 3
const LC: f32 = (130.0f64 / 255.0) as f32;
/// k, about 10 log2 e. step 3 (bits 402cda993e7663be)
pub(crate) const CLOUD_K: f64 = f64::from_bits(0x402c_da99_3e76_63be);
/// F, the single nearest 0.2. step 4
const F: f32 = 0.2;

/// The sun as takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunInputs {
    /// Toward the sun, of unit length.
    pub direction: Vec3,
    /// C.
    pub colour: [f32; 3],
    /// W.
    pub glow_colour: [f32; 3],
    /// J, the light part's intensity.
    pub intensity: f64,
}

/// The sky part's numbers reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyInputs {
    pub switches: u32,
    /// K, the lower cloud colour.
    pub cloud_colour: [f32; 3],
    /// U, the dome colour.
    pub dome_colour: [f32; 3],
}

/// The main light.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainLight {
    pub direction: Vec3,
    /// Λ.
    pub colour: [f32; 3],
    pub intensity: f64,
}

/// The colours the dome light is made from as 3.0 and 3.1. steps 1 to 3
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DomeParts {
    /// Z.
    pub overhead: [f32; 3],
    /// Γ.
    pub glow: [f32; 3],
    /// Ω.
    pub cloud: [f32; 3],
}

/// What a render computes once at its start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedSky {
    pub main: MainLight,
    /// G, the dome light's colour.
    pub dome: [f32; 3],
}

/// cap(x, c): x when x is less than the double nearest c, else the single nearest c.
fn cap(x: f32, c: f64) -> f32 {
    if f64::from(x) < c { x } else { c as f32 }
}

/// The main light.
pub fn main_light(sun: &SunInputs) -> MainLight {
    let [x, y, z] = sun.direction;
    let intensity = sun.intensity * SUN_FACTOR;
    if y < 0.0 {
        let u = -y;
        let rg = cap((u * MOON_RG) as f32, MOON_CAP_RG);
        let b = cap((u * MOON_B) as f32, MOON_CAP_B);
        MainLight { direction: [-x, -y, -z], colour: [rg, rg, b], intensity }
    } else {
        MainLight { direction: sun.direction, colour: sun.colour, intensity }
    }
}

/// What HLS does with a hue of 1 (n = 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HueOne {
    /// As 3.0 and 3.1: no colour; the colour it was to replace stays as it was.
    NoColour,
    /// As 5.0: taken as n = 5 with f = 1, the colour (V, M, Y).
    LastSector,
}

impl HueOne {
    /// The rule a version follows.
    pub fn of(version: RenderAs) -> HueOne {
        match version {
            RenderAs::Bryce50 => HueOne::LastSector,
            RenderAs::Bryce30 | RenderAs::Bryce31 => HueOne::NoColour,
        }
    }
}

/// 's hexcone transform with its formats, as 3.0 and 3.1. A hue of 1 gives n = 6, for
/// which HLS gives no colour (`None`): the colour it was to replace stays as it was.
pub fn hls(h: f32, l: f32, s: f32) -> Option<[f32; 3]> {
    hls_with(HueOne::NoColour, h, l, s)
}

/// 's hexcone transform with the given rule for a hue of 1.,
pub fn hls_with(rule: HueOne, h: f32, l: f32, s: f32) -> Option<[f32; 3]> {
    let (h, l, s) = (f64::from(h), f64::from(l), f64::from(s));
    let v = if l <= 0.5 { (s + 1.0) * l } else { (l + s) - l * s };
    if !(v > 0.0) {
        return Some([0.0; 3]);
    }
    let big_v = v as f32;
    let vd = f64::from(big_v);
    let mu = (l + l) - vd;
    let six_h = 6.0 * h;
    let mut n = six_h.trunc();
    let mut f = six_h - n;
    if n == 6.0 && rule == HueOne::LastSector {
        (n, f) = (5.0, 1.0);
    }
    let m = mu as f32;
    // : where the single-rounding form (X = <mu + (V - mu)f>) and
    // 's differ,'s is the original's: the ramp ((V - mu)/V)·f·V, then each stored
    let ramp = (((vd - mu) / vd) * f) * vd;
    let x = (ramp + mu) as f32;
    let y = (vd - ramp) as f32;
    match n as i64 {
        0 => Some([big_v, x, m]),
        1 => Some([y, big_v, m]),
        2 => Some([m, big_v, x]),
        3 => Some([m, y, big_v]),
        4 => Some([x, m, big_v]),
        5 => Some([big_v, m, y]),
        _ => None,
    }
}

/// HLS where the hues used are below 1 (steps 1 and 3; bit 0x2 of step 2).
pub(crate) fn hls_colour(h: f32, l: f32, s: f32) -> [f32; 3] {
    hls(h, l, s).unwrap_or([0.0; 3])
}

/// Z, the colour overhead. step 1
pub(crate) fn overhead(y: f64) -> [f32; 3] {
    let yc = y.clamp(-1.0, 1.0);
    if !(yc < 0.0) {
        let h = ((200.0 - 55.0 * yc) * B) as f32;
        let l = (64.0 * (yc + 1.0) * B) as f32;
        hls_colour(h, l, SZ)
    } else {
        let tau = 64.0 * (1.0 + 2.0 * yc);
        if tau > 0.0 {
            let h = ((200.0 + 55.0 * yc) * B) as f32;
            let l = (tau * B) as f32;
            hls_colour(h, l, SZ)
        } else {
            [0.0; 3]
        }
    }
}

/// W', the glow colour the sun's colour is mixed with: W, or a hue that follows the sun with
/// bits 0x2 and 0x4. step 2 (revised)
fn glow_base(sun: &SunInputs, switches: u32) -> [f32; 3] {
    let yc = sun.direction[1].clamp(-1.0, 1.0);
    let mut w = sun.glow_colour;
    if switches & GLOW_HUE_A != 0 {
        w = if !(yc < 0.0) {
            hls_colour(((160.0 + 60.0 * (1.0 - yc)) * B) as f32, L2, 1.0)
        } else {
            hls_colour(J2, ((200.0 + 128.0 * yc) * B) as f32, (255.0 * (yc + 1.0) * B) as f32)
        };
    }
    if switches & GLOW_HUE_B != 0 && yc > 0.0 {
        let h = ((140.0 + 115.0 * (1.0 - yc)) * B) as f32;
        let l = ((200.0 - 60.0 * (1.0 - yc)) * B) as f32;
        // A hue of 1 leaves the colour as it was before this bit.
        if let Some(c) = hls(h, l, 1.0) {
            w = c;
        }
    }
    w
}

/// W' as 5.0: bit 0x4's colour at every height of the sun (a hue of 1 gives a colour),
/// else bit 0x2's as 3.0 and 3.1 give it, else W.
fn glow_base_five(sun: &SunInputs, switches: u32) -> [f32; 3] {
    if switches & GLOW_HUE_B == 0 {
        return glow_base(sun, switches);
    }
    let yc = sun.direction[1].clamp(-1.0, 1.0);
    let (h, l, s) = if !(yc < 0.0) {
        (((140.0 + 115.0 * (1.0 - yc)) * B) as f32, ((200.0 - 60.0 * (1.0 - yc)) * B) as f32, 1.0)
    } else {
        (1.0, ((140.0 + 128.0 * yc) * B) as f32, (255.0 * (yc + 1.0) * B) as f32)
    };
    hls_with(HueOne::LastSector, h, l, s).unwrap_or([0.0; 3])
}

/// Γ, half way from W' to the sun's colour, as `version` forms W'. step 2,
/// (The dome light as 5.0)
pub(crate) fn glow_as(version: RenderAs, sun: &SunInputs, switches: u32) -> [f32; 3] {
    let wp = match version {
        RenderAs::Bryce50 => glow_base_five(sun, switches),
        RenderAs::Bryce30 | RenderAs::Bryce31 => glow_base(sun, switches),
    };
    std::array::from_fn(|k| {
        let (c, w) = (sun.colour[k], wp[k]);
        w + (c - w) * 0.5
    })
}

/// Step 3's three results: the hue colour Hc, the weight ω (a double) and the cloud colour Ω.
/// takes all three for the lower cloud layer's colours. step 3,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudStep {
    /// Hc.
    pub hue_colour: [f32; 3],
    /// ω, before it is rounded to a single.
    pub weight: f64,
    /// Ω.
    pub colour: [f32; 3],
}

/// Step 3 for the sun's height `y` and the lower cloud colour K. step 3
pub fn cloud_step(version: RenderAs, y: f64, cloud_colour: [f32; 3]) -> CloudStep {
    let j = (255.0 + 150.0 * y).trunc() as i64;
    let i = j.rem_euclid(255);
    let hc = hls_colour((i as f64 * B) as f32, LC, 1.0);
    let weight = exp2_as(version, y.abs() * -CLOUD_K);
    let omega = weight as f32;
    let colour = std::array::from_fn(|k| {
        let kk = cloud_colour[k];
        let pre = kk + (hc[k] - kk) * omega;
        if -y < 0.125 {
            (f64::from(pre) * (1.0 + 3.0 * y)) as f32
        } else {
            pre * 0.625
        }
    });
    CloudStep { hue_colour: hc, weight, colour }
}

/// Ω, the cloud colour drawn toward a hue that follows the sun. step 3
fn cloud(version: RenderAs, y: f64, cloud_colour: [f32; 3]) -> [f32; 3] {
    cloud_step(version, y, cloud_colour).colour
}

/// Steps 1 to 3 of the dome light as 3.0 and 3.1, formed whenever bit 0x8 is clear,
/// whatever bit 0x100 (bit 0x80 changes none of them). steps 1 to 3
pub fn dome_parts(version: RenderAs, sun: &SunInputs, sky: &SkyInputs) -> DomeParts {
    let y = sun.direction[1];
    DomeParts { overhead: overhead(y), glow: glow_as(version, sun, sky.switches), cloud: cloud(version, y, sky.cloud_colour) }
}

/// Whether the dome colour is black at the levels of 8 bits: for each channel, the whole part
/// of 255·Uk (the exact binary64 product), cut toward zero, is a multiple of 256 (0 among them).
/// No bound on the size (observed up to 2^32 and beyond, and for 10^17); a channel that is not a
/// number, or infinite, counts as black. Not the product rounded to a single.
/// , and its update (point 7)
pub fn dome_colour_black(u: [f32; 3]) -> bool {
    u.iter().all(|&c| {
        let t = (f64::from(c) * 255.0).trunc();
        // fmod of a binary64 is exact at any size.
        !t.is_finite() || t % 256.0 == 0.0
    })
}

/// G, the dome light's colour. (The dome light as 3.0 and 3.1, step 4; The
/// dome light as 5.0; The dome light and a black dome colour)
pub fn dome_light(version: RenderAs, sun: &SunInputs, sky: &SkyInputs, main: &MainLight) -> [f32; 3] {
    let w = sky.switches;
    // : in every version and word
    // before any other rule (U as stored included).
    if dome_colour_black(sky.dome_colour) {
        return [0.0; 3];
    }
    // Bit 0x100 plays no part.
    if w & DOME_AS_STORED != 0 {
        return sky.dome_colour;
    }
    let five = version == RenderAs::Bryce50;
    if five && w & (GLOW_HUE_A | GLOW_HUE_B) == 0 {
        return sky.dome_colour;
    }
    dome_step4(version, dome_parts(version, sun, sky), sky, main)
}

/// The dome light of a sky as 5.0 with bit 0x1000000 (with bit 0x2 or 0x4 and not 0x8): Z and
/// W' are black, so Γ is C times 0.5 (the other parts as stored).,
pub fn dome_light_black_sky(sun: &SunInputs, sky: &SkyInputs, main: &MainLight) -> [f32; 3] {
    let w = sky.switches;
    if dome_colour_black(sky.dome_colour) {
        return [0.0; 3];
    }
    if w & DOME_AS_STORED != 0 ||w & (GLOW_HUE_A | GLOW_HUE_B) == 0 {
        return sky.dome_colour;
    }
    let mut p = dome_parts(RenderAs::Bryce50, sun, sky);
    p.overhead = [0.0; 3];
    // W' is black, so step 2 makes the glow colour half the sun's colour.
    p.glow = std::array::from_fn(|k| 0.0 + (sun.colour[k] - 0.0) * 0.5);
    dome_step4(RenderAs::Bryce50, p, sky, main)
}

/// Step 4 of the dome light from its parts.
fn dome_step4(version: RenderAs, p: DomeParts, sky: &SkyInputs, main: &MainLight) -> [f32; 3] {
    let five = version == RenderAs::Bryce50;
    let (z, g, o, l, u) = (p.overhead, p.glow, p.cloud, main.colour, sky.dome_colour);
    // Red: binary64 throughout, rounded once at the end (all versions). step 4
    let d = |v: f32| f64::from(v);
    let r = (((2.0 * d(z[0]) + d(g[0]) + d(o[0]) + d(l[0])) * d(F) + d(u[0])) * 0.5) as f32;
    let single = |k: usize| {
        let t = 2.0f32 * z[k] + g[k] + o[k] + l[k];
        if five {
            ((f64::from(t) * d(F) + d(u[k])) as f32) * 0.5
        } else {
            // Each sum and product in binary32. step 4
            (t * F + u[k]) * 0.5
        }
    };
    [r, single(1), single(2)]
}

/// The main light and the dome light, as a render prepares them.
pub fn prepare(version: RenderAs, sun: &SunInputs, sky: &SkyInputs) -> PreparedSky {
    let main = main_light(sun);
    PreparedSky { main, dome: dome_light(version, sun, sky, &main) }
}

/// The colour of a ray that meets nothing, whatever its direction: each channel P ⊕ 0
/// (before the air of, where bit 0x80 lays it over).
pub fn plain_sky(colour: [f32; 3]) -> [f32; 3] {
    colour.map(|p| p + 0.0f32)
}

/// Whether state the sky of this switch word as `version`: every word made
/// of 0x2, 0x4, 0x8, 0x80 and 0x100, with or without bit 0x1; as 3.0 and 3.1 also's
/// bits, and with bit 0x1 the further bits 0x400, 0x800, 0x10000, 0x40000, 0x100000 and
/// 0x2000000.
pub fn in_scope(version: RenderAs, switches: u32) -> bool {
    let allowed = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 if switches & ATMOSPHERE != 0 => SCOPE_BITS | ATMOSPHERE | FURTHER_BITS,
        RenderAs::Bryce30 | RenderAs::Bryce31 => SCOPE_BITS,
        // (As 5.0: "the sky colour that belongs to the switch word is's"
        // for words with bit 0x8 and the lit-air bits; with none of bits 0x2, 0x4 and 0x8
        // Open). (Light in the air revised)
        RenderAs::Bryce50 if switches & (GLOW_HUE_A | GLOW_HUE_B | DOME_AS_STORED) != 0 => {
            SKY_BITS | ATMOSPHERE | LIT_AIR_BITS
        }
        RenderAs::Bryce50 => SKY_BITS | ATMOSPHERE,
    };
    switches & !(allowed | STATED_BITS) == 0
}
