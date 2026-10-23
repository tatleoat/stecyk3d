// SPDX-License-Identifier: MIT
//! The sky's own colours in a direction:'s sky as 5.0 (a switch word with bit 0x2,
//! 0x4 or 0x8) and, as 3.0 and 3.1, the sky with atmosphere (bit 0x1) with its further bits,
//! its halos and the moon's phase.
//!
//! ("The sky as 5.0", "The sky with atmosphere as 3.0 and 3.1", "Further bits
//! with bit 0x1, as 3.0 and 3.1", "The halos", "The phase's weight"), (steps 1 and 2,
//! HLS)
//! in
//!
//! Notation as: every binary64 operation one Rust `f64` operation in the order
//! written, ⟨x⟩ `x as f32`, ⊕ ⊖ ⊗ Rust `f32` operations. The air  is not laid here:
//! the caller lays `air::Air` over this colour.

use crate::geom::Vec3;
use crate::numerics::ExtValue;
use crate::numerics::x87::{cos_ext, sin_ext, tan_ext};
use crate::numerics::{acos, exp2_as, log2_as, pow, rsqrt_as, sqrt_as};
use super::stars::held;
use crate::version::RenderAs;
use super::moon_image::MoonImage;
use std::sync::Arc;

use super::colours::{
    AIR_ON, ATMOSPHERE, B, CLOUD_K, DOME_AS_STORED, GLOW_HUE_A, GLOW_HUE_B, HueOne, SunInputs, glow_as, hls_with,
    overhead,
};

/// Bit 0x400 (the named views): χ = 0 and no air.
pub const NAMED_VIEW: u32 = 0x400;
/// Bit 0x800: the plain colour and no air.
pub const PLAIN_ONLY: u32 = 0x800;
/// Bit 0x1000000 in a Bryce 5 sky's word W: a black sky.
pub const BLACK_SKY: u32 = 0x100_0000;
/// Bit 0x10000: the halos.
pub const HALOS: u32 = 0x10000;
/// Bit 0x2000000: also the halo of 46 degrees.
pub const HALO_46: u32 = 0x2000000;
/// Bit 0x40000: the moon's phase.
pub const MOON_PHASE: u32 = 0x40000;
/// Bit 0x100000: the discs grow toward the horizon.
pub const DISCS_GROW: u32 = 0x100000;
/// Bit 0x20000: the moon's disc shows the user's picture of the moon. (The moon's
/// picture; 0x20000 [format])
pub const MOON_PICTURE: u32 = 0x20000;

/// Δ, the double nearest 0.01745329251994 (not π/180).
const DELTA: f64 = 0.01745329251994;
/// The sun's disc factor and the moon's.
const SUN_DISC: f64 = 1.22;
const MOON_DISC: f64 = 1.8;
/// The cap on ce.
const CE_CAP: f64 = 0.999402948355;
/// β, added inside the discs.
const BETA: f64 = 0.3051804379339;
/// o, the moon's light offsets, red, green, blue.
const MOON_OFFSET: [f64; 3] = [0.05468833447776, 0.06640726329442, 0.12500190737777];
/// δr and δg, the moon disc's red and green edges past cm.
const EDGE_RED: f64 = 8.66138315e-5;
const EDGE_GREEN: f64 = 4.1909048e-5;
/// The haze's share: 0.2·h.
const HAZE_SHARE: f64 = 0.2;
/// The band's weight b = 0.5·χ.
const BAND: f64 = 0.5;
/// The moon's colour factors from μ = −y.
const MOON_RG: f64 = 0.5;
const MOON_B: f64 = 0.625;
/// The sun's power in step 3 and the moon's in step 6. steps 3, 6
const SUN_POWER: f64 = 85.0; // step 3
const MOON_POWER: f64 = 120.0; // step 6
/// Step 4's factor 0.7 + 0.5·a. step 4
const SIDE_BASE: f64 = 0.7; // step 4
const SIDE_SLOPE: f64 = 0.5; // step 4
/// π with bit 0x8.
const POWER_STORED: f64 = 6.0;
/// π with bit 0x4: (4 + 6·(1 − y')) − 2χ; with bit 0x2 alone: 3 − χ.
const POWER_B_BASE: f64 = 4.0;
const POWER_B_SLOPE: f64 = 6.0;
const POWER_B_HAZE: f64 = 2.0;
const POWER_A: f64 = 3.0;
/// The discs' growth exponent.
const GROWTH: f64 = 1.5;
/// k4, the double nearest 4·log2 e.
const K4: f64 = 5.7707801635558535;
/// c, the double nearest 1/180, by which φ′ is multiplied (not divided by 180).
const ONE_180TH: f64 = 1.0 / 180.0;
/// λ1 and λ2, the doubles nearest ln 2 and 1/ln 2, by which the glows' powers are multiplied.
const LAMBDA1: f64 = std::f64::consts::LN_2;
const LAMBDA2: f64 = std::f64::consts::LOG2_E;
/// φ' is taken from 360 − φ above 180; ν's exponent divides by 180.
const HALF_TURN: f64 = 180.0;
const FULL_TURN: f64 = 360.0;
/// k2, the double nearest 1.570796326795.
#[allow(clippy::approx_constant)] // not π/2: the double nearest 1.570796326795
const K2: f64 = 1.570796326795;
/// H's threshold for the frame across D.
const FRAME_TINY: f64 = 1e-7;
/// The place on the moon's picture: x = ⟨u·0.49 + 0.5⟩, y = ⟨1 − (v·0.49 + 0.5)⟩.
const PICTURE_SPREAD: f64 = 0.49;
const PICTURE_MIDDLE: f64 = 0.5;
/// The lowness tint's exponent: l = ⟨pow(h·h, 1.5)⟩.
const LOWNESS: f64 = 1.5;
/// The picture's byte floor 28.
const PICTURE_DARK: f64 = 28.0;
/// (fl, sl) as 3.0: the doubles nearest 148.3 and 0.55.
const PICTURE_FLOOR_30: f64 = 148.3;
const PICTURE_SLOPE_30: f64 = 0.55;
/// As 3.1 and 5.0: fl = 150 + 40·ν, sl = (255 − fl)·σ, σ the double nearest 1/194.
const PICTURE_FLOOR_BASE: f64 = 150.0;
const PICTURE_FLOOR_NU: f64 = 40.0;
const PICTURE_TOP: f64 = 255.0;
const PICTURE_SIGMA: f64 = 1.0 / 194.0;

/// The halos: A, g and the middle angle in radians.
const HALO_22: (f64, f64, f64) = (22.0, 0.9, 0.38397243543868);
const HALO_46_NUMBERS: (f64, f64, f64) = (46.0, 0.06, 0.8028514559172399);
/// ci = FCOS((A − 0.2·w)·Δ), co = FCOS((0.8·w + A)·Δ).
const HALO_INNER: f64 = 0.2;
const HALO_OUTER: f64 = 0.8;
/// The halo colour's lightness and saturation (singles), and its weight toward the tint.
const HALO_LS: f32 = 0.5;
const HALO_TINT: f32 = 0.95;
/// E's blue by day: 128 + 127·(2y' − 1); its green and blue below y' = 0.5: 512·y', 256·y';
/// its red below the horizon: 255·(y' + 1).
const E_HIGH: (f64, f64) = (128.0, 127.0);
const E_LOW: (f64, f64) = (512.0, 256.0);
const E_NIGHT: f64 = 255.0;
/// The visibility of a star field: a1, a2, k5 (bits 4062089fc709fe56), the factor 1.2 and
/// the bright sky's 3.8.
const STARS_A1: f64 = 0.7853981633974;
const STARS_A2: f64 = 1.273239544735241;
const STARS_K5: f64 = f64::from_bits(0x4062_089f_c709_fe56);
const STARS_FADE: f64 = 1.2;
const STARS_HIDE: f64 = 3.8;

/// The sky part's numbers reads, named as `scene_parts::Sky` holds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkySettings {
    pub switches: u32,
    pub sky_colour: [f32; 3],
    pub horizon_colour: [f32; 3],
    /// P.
    pub plain_colour: [f32; 3],
    /// h, the haze .
    pub haze: f64,
    /// φ, λ, ψ, ε (offset 416).
    pub moon: [f64; 4],
    /// The fifteen discs and rings numbers (offset 448, `discs`): r, r', _, _,
    /// κ, w1, w2, then the rainbow's (Open).
    pub discs: [f64; 15],
}

/// One halo's numbers fixed for the render.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Halo {
    /// ci.
    inner: f64,
    /// co.
    outer: f64,
    /// m.
    middle: f64,
    /// κ·g, applied as v·κ·g·strength.
    kappa: f64,
    g: f64,
}

/// The moon's phase in its disc, with the numbers the original forms once for the render.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Phase {
    /// D = −S, the moon's direction.
    moon: Vec3,
    /// H = √(Dy·Dy + Dz·Dz), and (cy, cz) = (Dy/H, Dz/H) where H > 10^−7.
    across: f64,
    tilt: Option<(f64, f64)>,
    /// τ.
    tan_size: f64,
    /// L's x, −(cos ψ·sin φ), rounded to a double.
    light_x: f64,
    /// sin φ rounded to a double; sin ψ and −cos φ carried in the extended register (`None`
    /// where the angle is not finite: the unit's result is then not a number).
    sin_phi: f64,
    sin_psi: Option<ExtValue>,
    neg_cos_phi: Option<ExtValue>,
    lambda: f64,
    epsilon: f64,
}

/// A halo's place: where the halos of one ray make their colour; the colour last drawn there,
/// after its pull toward its tint.
/// (A halo's hue that rounds to 1)
type HaloPlace = Option<[f32; 3]>;

/// The moon's picture in the disc, with (fl, sl) of its step 4.
#[derive(Clone, Debug, PartialEq)]
struct MoonPictureDisc {
    image: Arc<MoonImage>,
    floor: f64,
    slope: f64,
}

/// The sky's own colours, fixed for the render.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyColours {
    version: RenderAs,
    /// Z, E, Γ.
    overhead: [f32; 3],
    horizon: [f32; 3],
    glow: [f32; 3],
    /// π.
    power: f64,
    /// b.
    band: f64,
    /// S and C.
    sun: Vec3,
    sun_colour: [f32; 3],
    /// M and ν.
    moon_colour: [f32; 3],
    moon_strength: f64,
    /// cs, ce, cm.
    sun_disc: f64,
    sun_edge: f64,
    moon_disc: f64,
    halos: Vec<Halo>,
    /// The frame across the moon's disc (formed with bit 0x40000 or bit 0x20000) and the phase's
    /// light.
    phase: Option<Phase>,
    /// Bit 0x40000 is set: the blend in the disc takes the phase's weight ω (else 1).
    weighted: bool,
    /// The moon's picture, with bit 0x20000.
    picture: Option<MoonPictureDisc>,
    /// h, the haze amount, for the stars' visibility (whatever bit 0x80 holds; as only
    /// where W has bit 0x1).
    haze: f64,
    /// Steps 2 to 6 are taken (false with's bit 0x8000: nothing of the sun and moon).
    bodies: bool,
    /// Gs (the sun's glow strength), Ds and Dm (the discs' strengths); 1 as 3.0 and 3.1. The
    /// moon's Gm is in `moon_strength`.
    sun_glow: f64,
    sun_disc_strength: f64,
    moon_disc_strength: f64,
    /// What a halo's hue of 1 gives (HLS as 5.0).,
    hue_one: HueOne,
}

/// The sky part's numbers's sky as 5.0 reads, with the settings as 5.0 (W, the sky and
/// horizon colours, f in `discs[1]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiveColourSettings {
    /// W.
    pub switches: u32,
    pub sky_colour: [f32; 3],
    pub horizon_colour: [f32; 3],
    /// h, the haze.
    pub haze: f64,
    /// φ, λ, ψ, ε.
    pub moon: [f64; 4],
    /// The fifteen numbers as 5.0: r, f (the Horizon Illusion factor), …, κ, w1, w2, ….
    pub discs: [f64; 15],
    /// Gs, Gm, Ds, Dm: the sun's and the moon's glow strengths, their disc strengths.
    pub glow_and_disc: [f64; 4],
}

/// Bit 0x8000 as 5.0: nothing of the sun and the moon.
pub const NO_BODIES: u32 = 0x8000;

/// d·S as the original forms it in the three versions: (dy·Sy + dz·Sz) + dx·Sx; with −S in
/// place of S it is s′., sky 02
/// (Orders: the clouds' s is the sky's d·S)
pub(crate) fn sky_dot(d: Vec3, s: Vec3) -> f64 {
    (d[1] * s[1] + d[2] * s[2]) + d[0] * s[0]
}

/// The extended result of FCOS, FSIN or FPTAN, or `None` where the argument is not finite (the
/// unit then gives a value that is not a number). (The moon's numbers that
/// are not numbers or are infinite)
fn unit(f: fn(f64) -> ExtValue, x: f64) -> Option<ExtValue> {
    x.is_finite().then(|| f(x))
}

/// FCOS, FSIN, FPTAN rounded once to a double (a NaN for an argument that is not finite).
fn fcos(x: f64) -> f64 {
    unit(cos_ext, x).map_or(f64::NAN, ExtValue::to_f64)
}
fn fsin(x: f64) -> f64 {
    unit(sin_ext, x).map_or(f64::NAN, ExtValue::to_f64)
}
fn fptan(x: f64) -> f64 {
    unit(tan_ext, x).map_or(f64::NAN, ExtValue::to_f64)
}

/// A glow's weight 2^(((log2 a·π)·λ1)·λ2). steps 2 and 5 (the original's arrangement)
fn glow_power(version: RenderAs, a: f64, power: f64) -> f64 {
    exp2_as(version, ((log2_as(version, a) * power) * LAMBDA1) * LAMBDA2)
}

/// Zmax, the largest channel of Z: green, replaced by red where green is below red or either
/// is not a number; that, replaced by blue where blue is not below it and neither is not a
/// number.
fn largest_channel(z: [f32; 3]) -> f64 {
    let [r, g, b] = z.map(f64::from);
    let mut m = g;
    if g < r || g.is_nan() || r.is_nan() {
        m = r;
    }
    if !(b < m) && !b.is_nan() && !m.is_nan() {
        m = b;
    }
    m
}

/// ν with the moon's phase (bit 0x40000): the factor 1 − (1 − 2^x), x = (φ′·c)·(−k4), times ν
/// where this is above λ·ν, else λ·ν (so a λ·ν that is not a number is taken).
/// (Further bits: bit 0x40000; the original's own arrangement; The moon's numbers
/// that are not numbers or are infinite),
fn phased_strength(version: RenderAs, nu: f64, phi: f64, lambda: f64) -> f64 {
    let phi1 = if phi > HALF_TURN { FULL_TURN - phi } else { phi };
    let x = (phi1 * ONE_180TH) * -K4;
    let faded = (1.0 - (1.0 - exp2_as(version, x))) * nu;
    let least = lambda * nu;
    if faded > least { faded } else { least }
}

/// "c toward b by w": ck ⊕ ((bk ⊖ ck) ⊗ w).
fn toward(c: [f32; 3], b: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|k| c[k] + (b[k] - c[k]) * w)
}

/// E without bit 0x8, from the sun's height y'.
fn horizon_from_sun(yc: f64) -> [f32; 3] {
    if yc > 0.5 {
        [1.0, 1.0, ((E_HIGH.0 + E_HIGH.1 * (2.0 * yc - 1.0)) * B) as f32]
    } else if yc >= 0.0 {
        [1.0, (E_LOW.0 * yc * B) as f32, (E_LOW.1 * yc * B) as f32]
    } else {
        [(E_NIGHT * (yc + 1.0) * B) as f32, 0.0, 0.0]
    }
}

/// The halos a word asks for: that of 22 degrees with bit 0x10000, and that of 46 after it
/// with bit 0x2000000 as well.,
fn halos_of(w: u32, discs: &[f64; 15]) -> Vec<Halo> {
    let mut halos = Vec::new();
    if w & HALOS != 0 {
        let kappa = discs[4];
        let mut add = |(a, g, middle): (f64, f64, f64), width: f64| {
            halos.push(Halo {
                inner: fcos((a - HALO_INNER * width) * DELTA),
                outer: fcos((HALO_OUTER * width + a) * DELTA),
                middle: fcos(middle),
                kappa,
                g,
            });
        };
        add(HALO_22, discs[5]);
        if w & HALO_46 != 0 {
            add(HALO_46_NUMBERS, discs[6]);
        }
    }
    halos
}

/// The moon's phase in its disc, with τ = FPTAN(`tan_angle`). (The phase's
/// weight; the original's own arrangement as 3.0 and 3.1)
fn phase_of(version: RenderAs, s_dir: Vec3, tan_angle: f64, moon: [f64; 4]) -> Phase {
    let [phi, lambda, psi, epsilon] = moon;
    let d = [-s_dir[0], -s_dir[1], -s_dir[2]];
    let h = sqrt_as(version, d[1] * d[1] + d[2] * d[2]);
    let tilt = (h > FRAME_TINY).then(|| (d[1] / h, d[2] / h));
    let tan_size = fptan(tan_angle);
    let (phi_a, psi_a) = (phi * DELTA, psi * DELTA);
    let sin_phi = fsin(phi_a);
    let light_x = -(fcos(psi_a) * sin_phi);
    Phase {
        moon: d,
        across: h,
        tilt,
        tan_size,
        light_x,
        sin_phi,
        sin_psi: unit(sin_ext, psi_a),
        neg_cos_phi: unit(cos_ext, phi_a).map(ExtValue::negated),
        lambda,
        epsilon,
    }
}

/// (u, v), from −1 to 1 across the moon's disc: e = k·d with k = 1/s″ (the ray carried to the
/// plane that touches the unit sphere at D), then u and v through H, cy and cz, each over τ.
/// s″ is (dx·Dx + dz·Dz) + dy·Dy as 3.0 and (dy·Dy + dz·Dz) + dx·Dx as 3.1 (and, our choice,
/// as 5.0).
fn disc_place(version: RenderAs, p: &Phase, d: Vec3) -> (f64, f64) {
    let m = p.moon;
    let s2 = if version == RenderAs::Bryce30 {
        (d[0] * m[0] + d[2] * m[2]) + d[1] * m[1]
    } else {
        (d[1] * m[1] + d[2] * m[2]) + d[0] * m[0]
    };
    let k = 1.0 / s2;
    let e = [k * d[0], k * d[1], k * d[2]];
    let h = p.across;
    match p.tilt {
        Some((cy, cz)) => {
            ((-m[0] * (cy * e[1] + cz * e[2]) + h * e[0]) / p.tan_size, (cz * e[1] - cy * e[2]) / p.tan_size)
        }
        None => ((-m[0] * e[2] + h * e[0]) / p.tan_size, e[1] / p.tan_size),
    }
}

impl SkyColours {
    /// The sky with the user's picture of the moon in the moon's disc (bit 0x20000): the render
    /// supplies the picture it takes (`Substrate::read_moon_picture`). Does nothing for a
    /// sky that draws no moon.
    pub fn with_moon_picture(mut self, image: Arc<MoonImage>) -> SkyColours {
        if self.phase.is_some() && self.bodies {
            let (floor, slope) = match self.version {
                RenderAs::Bryce30 => (PICTURE_FLOOR_30, PICTURE_SLOPE_30),
                _ => {
                    let floor = PICTURE_FLOOR_BASE + PICTURE_FLOOR_NU * self.moon_strength;
                    (floor, (PICTURE_TOP - floor) * PICTURE_SIGMA)
                }
            };
            self.picture = Some(MoonPictureDisc { image, floor, slope });
        }
        self
    }

    /// The disc's new colour t' from the moon's picture: the three bytes at the disc's place,
    /// the sky's lowness tint and the floor and slope. (The moon's picture, steps
    /// 1 to 4)
    fn picture_colour(&self, pic: &MoonPictureDisc, p: &Phase, d: Vec3, t: [f32; 3], h: f64) -> [f32; 3] {
        let (u, v) = disc_place(self.version, p, d);
        let x = (u * PICTURE_SPREAD + PICTURE_MIDDLE) as f32;
        let y = (1.0 - (v * PICTURE_SPREAD + PICTURE_MIDDLE)) as f32;
        let bytes = pic.image.read(x, y);
        let l = pow(h * h, LOWNESS) as f32;
        std::array::from_fn(|k| {
            let a = 1.0f32 + (self.horizon[k] - 1.0f32) * l;
            let g = (f64::from(t[k]) + f64::from(a) * BETA) as f32;
            ((((f64::from(bytes[k]) - PICTURE_DARK) * pic.slope + pic.floor) * f64::from(g)) * B) as f32
        })
    }

    /// The sky's own colours as `version`, or `None` where the plain colour P applies (as
    /// 5.0 a word with none of bits 0x2, 0x4 and 0x8; as 3.0 and 3.1 a word without bit 0x1,
    /// or with bit 0x800). The word is taken to be within's Scope (`in_scope`); as
    /// 5.0 bit 0x1 and the further bits are not read.
    pub fn new(version: RenderAs, sun: &SunInputs, sky: &SkySettings) -> Option<SkyColours> {
        let five = version == RenderAs::Bryce50;
        let w = if five {
            sky.switches & (GLOW_HUE_A | GLOW_HUE_B | DOME_AS_STORED | AIR_ON | super::colours::DOME_LIGHT_ON)
        } else {
            sky.switches
        };
        if five && w & (GLOW_HUE_A | GLOW_HUE_B | DOME_AS_STORED) == 0 {
            return None;
        }
        if !five && (w & ATMOSPHERE == 0 || w & PLAIN_ONLY != 0) {
            return None;
        }
        let s_dir = sun.direction;
        let y = s_dir[1];
        let yc = y.clamp(-1.0, 1.0);

        // χ and b.
        let chi = if w & AIR_ON != 0 && w & NAMED_VIEW == 0 { (HAZE_SHARE * sky.haze).clamp(0.0, 1.0) } else { 0.0 };
        let band = BAND * chi;

        // Z, E, Γ, π. (Numbers fixed for the render; with atmosphere: Γ as
        // 3.0 and 3.1, π = 0 with neither bit 0x2 nor 0x4)
        let (z, e, glow, power) = if w & DOME_AS_STORED != 0 {
            (sky.sky_colour, sky.horizon_colour, sun.glow_colour, POWER_STORED)
        } else {
            let power = if w & GLOW_HUE_B != 0 {
                (POWER_B_BASE + POWER_B_SLOPE * (1.0 - yc)) - POWER_B_HAZE * chi
            } else if w & GLOW_HUE_A != 0 {
                POWER_A - chi
            } else {
                0.0
            };
            (overhead(y), horizon_from_sun(yc), glow_as(version, sun, w), power)
        };

        // The moon: M and ν, with the phase (bit 0x40000).
        let mu = -y;
        let rg = (MOON_RG * mu) as f32;
        let moon_colour = [rg, rg, (MOON_B * mu) as f32];
        let zmax = largest_channel(z);
        let x = exp2_as(version, zmax * -CLOUD_K);
        let mut nu = 1.0 - (1.0 - x);
        let [phi, lambda, ..] = sky.moon;
        if w & MOON_PHASE != 0 {
            nu = phased_strength(version, nu, phi, lambda);
        }

        // The discs' size.
        let size = if five {
            None
        } else {
            let r = sky.discs[0];
            Some(if w & DISCS_GROW != 0 {
                let q = 1.0 - y.abs();
                pow(q * q, GROWTH) * (sky.discs[1] - r) + r
            } else {
                r
            })
        };
        let angle = |factor: f64| match size {
            None => factor * DELTA,
            Some(r) => r * factor * DELTA,
        };
        let sun_disc = fcos(angle(SUN_DISC));
        let moon_disc = fcos(angle(MOON_DISC));
        let sun_edge = if sun_disc < CE_CAP { sun_disc } else { CE_CAP };

        // The halos (bit 0x10000, and 0x2000000 for 46 degrees).
        let halos = halos_of(w, &sky.discs);

        // The phase's weight (bit 0x40000).
        let phase = (w & (MOON_PHASE | MOON_PICTURE) != 0).then(|| phase_of(version, s_dir, angle(MOON_DISC), sky.moon));

        Some(SkyColours {
            version,
            weighted: w & MOON_PHASE != 0,
            picture: None,
            overhead: z,
            horizon: e,
            glow,
            power,
            band,
            sun: s_dir,
            sun_colour: sun.colour,
            moon_colour,
            moon_strength: nu,
            sun_disc,
            sun_edge,
            moon_disc,
            halos,
            phase,
            haze: sky.haze,
            bodies: true,
            sun_glow: 1.0,
            sun_disc_strength: 1.0,
            moon_disc_strength: 1.0,
            hue_one: HueOne::NoColour,
        })
    }

    /// The sky's own colours as makes them for a word W as 5.0, or `None` where the
    /// colour is the plain colour (none of bits 0x2, 0x4 and 0x8, or bit 0x800). The caller
    /// refuses the bit leaves Open (0x1000000) before; 0x200 and 0x20000 are stated.
    /// , (The sky as 5.0, The halos, The
    /// phase's weight)
    pub fn new_bryce5(sun: &SunInputs, sky: &FiveColourSettings) -> Option<SkyColours> {
        let version = RenderAs::Bryce50;
        let w = sky.switches;
        if w & (GLOW_HUE_A | GLOW_HUE_B | DOME_AS_STORED) == 0 || w & PLAIN_ONLY != 0 {
            return None;
        }
        let s_dir = sun.direction;
        let y = s_dir[1];
        let yc = y.clamp(-1.0, 1.0);

        // χ = 0.2·h held to [0, 1] with bit 0x1 and not 0x400, else 0.
        let chi = if w & ATMOSPHERE != 0 && w & NAMED_VIEW == 0 { (HAZE_SHARE * sky.haze).clamp(0.0, 1.0) } else { 0.0 };
        let band = BAND * chi;

        // Z, E, Γ, π by bits 0x2, 0x4 and 0x8.
        let (z, e, glow, power) = if w & DOME_AS_STORED != 0 {
            (sky.sky_colour, sky.horizon_colour, sun.glow_colour, POWER_STORED)
        } else {
            let power = if w & GLOW_HUE_B != 0 {
                (POWER_B_BASE + POWER_B_SLOPE * (1.0 - yc)) - POWER_B_HAZE * chi
            } else {
                POWER_A - chi
            };
            (overhead(y), horizon_from_sun(yc), glow_as(version, sun, w), power)
        };

        // Bit 0x1000000 in a Bryce 5 sky's word, where W has bit 0x2 or 0x4 and not bit 0x8: Z and
        // E black, W' black and pi 0, so step 2 makes the glow colour half the sun's colour; with
        // bit 0x8 the bit changes none of them. (What the bits of W do: bit
        // 0x1000000; the sky's own colour there is Open)
        let (z, e, glow, power) = if w & BLACK_SKY != 0 && w & DOME_AS_STORED == 0 {
            let half_sun = std::array::from_fn(|k| 0.0 + (sun.colour[k] - 0.0) * 0.5);
            ([0.0; 3], [0.0; 3], half_sun, 0.0)
        } else {
            (z, e, glow, power)
        };

        // M and ν, with the phase (bit 0x40000).,
        let mu = -y;
        let rg = (MOON_RG * mu) as f32;
        let moon_colour = [rg, rg, (MOON_B * mu) as f32];
        let zmax = largest_channel(z);
        let x = exp2_as(version, zmax * -CLOUD_K);
        // ν = (1 − (1 − 2^(Zmax·(−k))))·Gm before the phase.
        let mut nu = (1.0 - (1.0 - x)) * sky.glow_and_disc[1];
        let [phi, lambda, ..] = sky.moon;
        if w & MOON_PHASE != 0 {
            nu = phased_strength(version, nu, phi, lambda);
        }

        // The size z (bit 0x100000: Sun/Moon size), else 1.
        let size = if w & DISCS_GROW != 0 {
            let (r, f) = (sky.discs[0], sky.discs[1]);
            let q = 1.0 - y.abs();
            pow(q * q, GROWTH) * f * r + r
        } else {
            1.0
        };
        let sun_disc = fcos(size * SUN_DISC * DELTA);
        let moon_disc = fcos(size * MOON_DISC * DELTA);
        let sun_edge = if sun_disc < CE_CAP { sun_disc } else { CE_CAP };

        // Halo Rings and Secondary Ring.
        let halos = halos_of(w, &sky.discs);

        // The phase's weight, τ = FPTAN(z·1.8·Δ).
        let phase = (w & (MOON_PHASE | MOON_PICTURE) != 0)
            .then(|| phase_of(version, s_dir, size * MOON_DISC * DELTA, sky.moon));

        Some(SkyColours {
            version,
            weighted: w & MOON_PHASE != 0,
            picture: None,
            overhead: z,
            horizon: e,
            glow,
            power,
            band,
            sun: s_dir,
            sun_colour: sun.colour,
            moon_colour,
            moon_strength: nu,
            sun_disc,
            sun_edge,
            moon_disc,
            halos,
            phase,
            haze: if w & ATMOSPHERE != 0 { sky.haze } else { 0.0 },
            bodies: w & NO_BODIES == 0,
            sun_glow: sky.glow_and_disc[0],
            sun_disc_strength: sky.glow_and_disc[2],
            moon_disc_strength: sky.glow_and_disc[3],
            hue_one: HueOne::LastSector,
        })
    }

    /// Lays the halos around a body over `c`: σ the cosine of the step, `tint` and
    /// `strength` the body's.
    /// `place` is the ray's halo place (`HaloPlace`), carried from the sun's halos to the moon's.
    fn halos_over(&self, mut c: [f32; 3], sigma: f64, tint: [f32; 3], strength: f64, place: &mut HaloPlace) -> [f32; 3] {
        for h in &self.halos {
            if !(h.outer < sigma && sigma < h.inner) {
                continue;
            }
            let hue = ((sigma - h.outer) / (h.inner - h.outer)) as f32;
            // As 3.0 and 3.1 a hue that rounds to 1 gives no colour: the colour pulled toward
            // the tint is the one last left in the halos' place (where no halo of this ray drew
            // before, the tint itself: our choice under the Freedom); as's 5.0 HLS gives
            // a colour for it.
            let ring = hls_with(self.hue_one, hue, HALO_LS, HALO_LS).unwrap_or_else(|| place.unwrap_or(tint));
            let colour = toward(ring, tint, HALO_TINT);
            *place = Some(colour);
            let v = if sigma < h.middle {
                1.0 - (h.middle - sigma) / (h.middle - h.outer)
            } else {
                1.0 - (sigma - h.middle) / (h.inner - h.middle)
            };
            c = toward(c, colour, (v * h.kappa * h.g * strength) as f32);
        }
        c
    }

    /// ω, the phase's weight at direction d in the moon's disc.
    /// As 3.0 and 3.1 in the original's own arrangement: ρ = √(v·v + u·u); z = −FCOS(ρ·k2)
    /// carried in the extended register into z·z; |(u, v, z)|² = (z·z + v·v) + u·u; N by's
    /// q; ℓ = (Lx·Nx + Ly·Ny) + Lz·Nz with sin ψ and −cos φ carried into their products; ω =
    /// ((1 − λ)·pow(ℓ, ε)) + λ. As 5.0 the same (our choice). (The phase's weight;
    /// the original's own arrangement)
    fn phase_weight(&self, p: &Phase, d: Vec3) -> f64 {
        let version = self.version;
        let (u, v) = disc_place(version, p, d);
        let rho = sqrt_as(version, v * v + u * u);
        let z_ext = unit(cos_ext, rho * K2).map(ExtValue::negated);
        let z = z_ext.map_or(f64::NAN, ExtValue::to_f64);
        let zz = z_ext.map_or(f64::NAN, |e| e.mul_to_f64(z));
        let q = rsqrt_as(version, (zz + v * v) + u * u);
        let n = [q * u, q * v, q * z];
        let light_y = -p.sin_psi.map_or(f64::NAN, |e| e.mul_to_f64(p.sin_phi));
        let along_z = p.neg_cos_phi.map_or(f64::NAN, |e| e.mul_to_f64(n[2]));
        let l = (p.light_x * n[0] + light_y * n[1]) + along_z;
        let l = if l >= 0.0 { l } else { 0.0 }; // below 0 or not a number
        ((1.0 - p.lambda) * pow(l, p.epsilon)) + p.lambda
    }

    /// The sky's colour in the direction `d` (of unit length), before the air:'s C.
    /// steps 1 to 7, with the halos and the phase
    pub fn colour(&self, d: Vec3) -> [f32; 3] {
        // 7. step 7
        self.colour_seen(d).0.map(|ck| ck + 0.0f32)
    }

    /// The colour after's step 6 (step 7 not yet taken) and the visibility Ψ of a
    /// star field formed alongside it, before the cloud layers (steps 1 to 4; not
    /// yet held to [0, 1] after step 4's product, which keeps it there).
    /// steps 1 to 6;
    pub fn colour_seen(&self, d: Vec3) -> ([f32; 3], f64) {
        let version = self.version;
        let sun = self.sun;
        let moon = [-sun[0], -sun[1], -sun[2]];

        // 1. The band. step 1
        let s = sky_dot(d, sun);
        let a = (s + 1.0) * 0.5;
        let q = 1.0 - d[1].abs();
        let mut w = self.band * (q * q);
        if w > 1.0 {
            w = 1.0;
        }
        let mut c = toward(self.overhead, self.horizon, w as f32);

        // Ψ step 1: stars fade toward the horizon with the haze. (What a ray
        // sees, 1: h whatever bit 0x80 holds)
        let e = {
            let x = (acos(d[1].abs()) - STARS_A1) * STARS_A2;
            if x > 0.0 { x } else { 0.0 }
        };
        let t = ((1.0 - exp2_as(version, (self.haze * HAZE_SHARE) * -STARS_K5)) * e) * STARS_FADE;
        let mut seen = 1.0 - held(t);

        if !self.bodies {
            let brightest = c.iter().map(|&ck| f64::from(ck)).fold(f64::NEG_INFINITY, f64::max);
            seen *= 1.0 - held(STARS_HIDE * brightest);
            return (c, seen);
        }

        // 2. The sun's glow, then its halos. step 2, The halos
        let glow = glow_power(version, a, self.power);
        c = toward(c, self.glow, (glow * self.sun_glow) as f32);
        let mut place: HaloPlace = None;
        c = self.halos_over(c, s, self.glow, self.sun_glow, &mut place);

        // 3. The sun. step 3
        if s < self.sun_edge {
            c = toward(c, self.sun_colour, (pow(a * a, SUN_POWER) * self.sun_glow) as f32);
        } else {
            let gs = self.sun_glow as f32;
            c = std::array::from_fn(|k| c[k] + ((self.sun_colour[k] - c[k]) * gs));
            seen = 0.0;
            if s >= self.sun_disc {
                c = c.map(|ck| (f64::from(ck) + self.sun_disc_strength * BETA) as f32);
            }
        }

        // 4. The sun's side brighter. step 4
        let side = SIDE_BASE + SIDE_SLOPE * a;
        c = c.map(|ck| (side * f64::from(ck)) as f32);

        // 5. The moon's glow, then its halos. step 5, The halos
        let s1 = sky_dot(d, moon);
        let a1 = (s1 + 1.0) * 0.5;
        let e1 = glow_power(version, a1, self.power);
        let nu = self.moon_strength;
        c = toward(c, self.moon_colour, ((e1 * e1) * nu) as f32);
        c = self.halos_over(c, s1, self.moon_colour, nu, &mut place);

        // 6. The moon's light and disc. step 6, bit 0x40000
        let n = pow(a1, MOON_POWER) * nu;
        let qk: [f64; 3] =
            std::array::from_fn(|k| (f64::from(self.moon_colour[k]) + MOON_OFFSET[k]) * n + f64::from(c[k]));
        c = if s1 < self.moon_disc {
            qk.map(|v| v as f32)
        } else {
            seen = 0.0;
            let t = qk.map(|v| v as f32);
            let lift = |tk: f32, on: bool| if on { (f64::from(tk) + self.moon_disc_strength * BETA) as f32 } else { tk };
            // (The moon's picture, 5: the disc's new colour is the picture's and
            // the small red and green additions do not apply to it)
            let t1 = match (&self.picture, &self.phase) {
                (Some(pic), Some(p)) => self.picture_colour(pic, p, d, t, q),
                _ => [
                    lift(t[0], s1 >= self.moon_disc + EDGE_RED),
                    lift(t[1], s1 >= self.moon_disc + EDGE_GREEN),
                    lift(t[2], true),
                ],
            };
            match (&self.phase, self.weighted) {
                (Some(p), true) => toward(t, t1, self.phase_weight(p, d) as f32),
                _ => std::array::from_fn(|k| t[k] + (t1[k] - t[k])),
            }
        };

        // Ψ step 4: a bright sky hides the stars.
        let brightest = c.iter().map(|&ck| f64::from(ck)).fold(f64::NEG_INFINITY, f64::max);
        seen *= 1.0 - held(STARS_HIDE * brightest);
        (c, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_channel_with_numbers_that_are_not() {
        assert_eq!(largest_channel([0.3, 0.5, 0.9]), f64::from(0.9f32));
        assert!(largest_channel([f32::NAN, 0.5, 0.9]).is_nan());
        assert_eq!(largest_channel([f32::INFINITY, 0.5, 0.9]), f64::INFINITY);
        assert_eq!(largest_channel([0.3, f32::NEG_INFINITY, 0.9]), f64::from(0.9f32));
        // Green not a number: red is taken, then blue where it is not below red.
        assert_eq!(largest_channel([0.3, f32::NAN, 0.9]), f64::from(0.9f32));
        assert_eq!(largest_channel([0.3, 0.5, f32::NAN]), f64::from(0.5f32));
        // A Zmax that is not finite gives ν = +0 (2^x of a NaN or an infinity is +0).
        for z in [[f32::NAN, 0.5, 0.9], [f32::INFINITY, 0.5, 0.9]] {
            let x = exp2_as(RenderAs::Bryce50, largest_channel(z) * -CLOUD_K);
            assert_eq!(1.0 - (1.0 - x), 0.0);
        }
    }

    #[test]
    fn moon_phase_strength_with_numbers_that_are_not() {
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            // φ not a number or +∞: the first term is +0 and ν = λ·ν.
            assert_eq!(phased_strength(v, 0.8, f64::NAN, 0.2), 0.2 * 0.8);
            assert_eq!(phased_strength(v, 0.8, f64::INFINITY, 0.2), 0.2 * 0.8);
            assert_eq!(phased_strength(v, 0.8, 180.0, 0.2), phased_strength(v, 0.8, 180.0, 0.2));
            // λ not a number: ν is not a number.
            assert!(phased_strength(v, 0.8, 60.0, f64::NAN).is_nan());
            // φ = 0: the factor is 1.
            assert_eq!(phased_strength(v, 0.8, 0.0, 0.2), 0.8);
        }
    }

    /// via: FCOS, FSIN and FPTAN of an argument that is not finite are not
    /// numbers, so L is not a number and ℓ is taken as 0.
    #[test]
    fn unit_functions_of_arguments_that_are_not_finite() {
        for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(fcos(x).is_nan() && fsin(x).is_nan() && fptan(x).is_nan());
        }
        assert_eq!(sky_dot([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), (10.0 + 18.0) + 4.0);
    }
}
