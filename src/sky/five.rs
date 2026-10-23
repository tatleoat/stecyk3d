// SPDX-License-Identifier: MIT
//! Bryce 5's own sky as 5.0: the settings as 5.0 (a Bryce 3D sky taken over), what the bits
//! of the word W do, the main light and the dome light for W, and the star field as 5.0 (the
//! seeded list with its strengths, the catalogue list, the field's turn) over the sky's colour.
//!
//! ("The settings as 5.0", "What the
//! bits of W do as 5.0", "Stars as 5.0", "The field's turn", "The visibility Ψ as 5.0"),
//!
//! Notation as: every binary64 operation one Rust `f64` operation in the order written,
//! ⟨x⟩ `x as f32`, ⊕ a Rust `f32` sum. The air  and the cloud layers  are not
//! laid here: the render puts the layers between `colour_seen` and `finish`, and the air after.

use std::sync::Arc;

use crate::geom::Vec3;
use crate::numerics::x87::{cos_ext, sin_ext};
use crate::numerics::{CRand, acos, rsqrt_as};
use crate::substrate_kept::StarRecord;
use crate::version::RenderAs;

use super::air::{AirInputs, haze_rates, haze_rates_shifted};
use super::colours::{PreparedSky, SkyInputs, SunInputs, dome_light_black_sky, main_light, plain_sky, prepare};
use super::sky_colours::{FiveColourSettings, SkyColours};
use super::star_grid::StarGrid;
use super::stars::{
    CatalogueGap, FieldEntry, FieldFrame, FieldSwitches, Laying, StarList, StarOrders, Strengths, catalogue_list, held,
};

/// The sky part's versions reads.,
pub const PART_BRYCE3D: u32 = 2;
pub const PART_BRYCE5: u32 = 7;

/// The bits of the word as 5.0.
pub const HAZE_SWITCH: u32 = 0x1;
pub const SKY_OWN_BITS: u32 = 0x2 | 0x4 | 0x8;
const DOME_AS_STORED: u32 = 0x8;
pub const FOG_SWITCH: u32 = 0x10;
pub const LAYER_BITS: u32 = 0x20 | 0x40;
pub const AIR_WORD: u32 = 0x80;
pub const SUN_IN_VIEW: u32 = 0x200;
pub const NAMED_VIEW: u32 = 0x400;
pub const PLAIN_ONLY: u32 = 0x800;
pub const RAINBOW: u32 = 0x4000;
pub const NO_BODIES: u32 = 0x8000;
pub const MOON_PICTURE: u32 = 0x20000;
pub const LIT_AIR: u32 = 0x80_0000;
pub const LIT_AIR_OPEN: u32 = 0x100_0000;
pub const COLOR_PERSPECTIVE: u32 = 0x8000_0000;
/// The second word: stars, comets; its values for a Bryce 3D sky.
pub const SECOND_STARS: u32 = 0x2;
pub const SECOND_COMETS: u32 = 0x4;
const SECOND_WITH_FIELD: u32 = 7;
const SECOND_WITHOUT: u32 = 1;
/// r′ becomes 0.75·r′.
const HORIZON_SHARE: f64 = 0.75;
/// The ninth of the fifteen numbers (the rainbow's).
const RAINBOW_NUMBER: usize = 8;
/// The haze's shift colour of a Bryce 3D sky: the singles nearest 0, 0.05 and 0.25.
pub const BRYCE3D_SHIFT: [f32; 3] = [0.0, 0.05, 0.25];

/// Every number of the sky part reads. For a version 2 part, the version 7 numbers are
/// 's "Values a version does not hold" (`second_switches` is then not read: the
/// takeover gives it).,
#[derive(Clone, Debug, PartialEq)]
pub struct FivePart {
    /// 2 or 7.
    pub part_version: u32,
    /// The stored switch word.
    pub switches: u32,
    pub second_switches: u32,
    pub cloud_colour: [f32; 3],
    pub sky_colour: [f32; 3],
    pub horizon_colour: [f32; 3],
    pub dome_colour: [f32; 3],
    /// P.
    pub plain_colour: [f32; 3],
    /// H.
    pub haze_colour: [f32; 3],
    pub haze: f64,
    pub fog: f64,
    pub fog_height: f64,
    /// The fifteen numbers (offset 448, 552), as stored.
    pub discs: [f64; 15],
    pub moon: [f64; 4],
    pub lit_air: f64,
    pub star_seed: i32,
    pub star_count: u32,
    pub comet_count: u32,
    pub strengths: Strengths,
    pub orientation: Vec3,
    /// The sun's and the moon's glow and disc strengths.
    pub glow_and_disc: [f64; 4],
    /// X.
    pub haze_shift: [f32; 3],
}

/// The settings as 5.0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyAsFive {
    /// W.
    pub switches: u32,
    pub second_switches: u32,
    pub sky_colour: [f32; 3],
    pub horizon_colour: [f32; 3],
    /// f, the second of the fifteen numbers as 5.0.
    pub horizon_factor: f64,
    /// A Bryce 3D sky taken over plain (step 2).
    pub taken_over_plain: bool,
}

/// A version 2 sky taken over (steps 1 to 4), or a version 7 sky as it stands.
pub fn as_five(p: &FivePart) -> SkyAsFive {
    if p.part_version != PART_BRYCE3D {
        return SkyAsFive {
            switches: p.switches,
            second_switches: p.second_switches,
            sky_colour: p.sky_colour,
            horizon_colour: p.horizon_colour,
            horizon_factor: p.discs[1],
            taken_over_plain: false,
        };
    }
    let w = p.switches;
    // 1. Bits 0x1 and 0x10 cleared, 0x80 set; with w's bit 0x80, 0x1 and 0x10 set.
    let mut big_w = (w & !(HAZE_SWITCH | FOG_SWITCH)) | AIR_WORD;
    if w & AIR_WORD != 0 {
        big_w |= HAZE_SWITCH | FOG_SWITCH;
    }
    // 5. A stored bit 0x1000000 is cleared and bit 0x800000 set (the lit air, in 1,000 steps).
    if big_w & LIT_AIR_OPEN != 0 {
        big_w = (big_w & !LIT_AIR_OPEN) | LIT_AIR;
    }
    // 2. Taken over plain.
    let (mut sky_colour, mut horizon_colour) = (p.sky_colour, p.horizon_colour);
    let acts = (big_w & HAZE_SWITCH != 0 && p.haze > 0.0)
        || (big_w & FOG_SWITCH != 0 && p.fog > 0.0 && p.fog_height > 0.0)
        || (big_w & RAINBOW != 0 && p.discs[RAINBOW_NUMBER] > 0.0)
        || (big_w & LIT_AIR != 0 && p.lit_air > 0.0);
    let plain = big_w & SKY_OWN_BITS == 0 && acts;
    if plain {
        big_w = (big_w & !LAYER_BITS) | DOME_AS_STORED;
        sky_colour = p.plain_colour;
        horizon_colour = p.plain_colour;
    }
    // 3. Bit 0x8000 gives the second word 7 and is cleared; set again when taken over plain.
    let second = if big_w & NO_BODIES != 0 { SECOND_WITH_FIELD } else { SECOND_WITHOUT };
    big_w &= !NO_BODIES;
    if plain {
        big_w |= NO_BODIES;
    }
    // 4. f = 0.75·r′.
    SkyAsFive {
        switches: big_w,
        second_switches: second,
        sky_colour,
        horizon_colour,
        horizon_factor: HORIZON_SHARE * p.discs[1],
        taken_over_plain: plain,
    }
}

/// The points leaves Open that a sky reaches. (Inputs, Limits, What the
/// bits of W do, Open)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiveOpen {
    /// Bit 0x1000000.
    LitAirBit,
    /// A glow or disc strength other than 1.
    Strengths,
    /// A catalogue file with fewer records than ⌈n/2⌉.
    FewRecords,
    /// A catalogue record whose colour leaves Open (class R).
    CatalogueColour,
}

impl FiveOpen {
    /// What is Open, in words.
    pub fn describe(self) -> &'static str {
        match self {
            FiveOpen::LitAirBit => "a sky switch word with bit 0x1000000",
            FiveOpen::Strengths => "a sun or moon glow or disc strength other than 1",
            FiveOpen::FewRecords => "a star catalogue with fewer records than half the star count asks",
            FiveOpen::CatalogueColour => "a catalogue star whose class has no colour (SUB-3: class R)",
        }
    }
}

/// The Open points the sky's word and numbers reach (the field's are found when it is made).
/// Bit 0x1000000 is not among them: states it (`prepare_lights`, `SkyColours`).
pub fn five_open(_p: &FivePart, _s: &SkyAsFive) -> Option<FiveOpen> {
    // Bits 0x200 (the sun in the view's frame) and 0x20000 (the moon's picture) are stated by
    // for the three versions  and are no longer Open here.
    // The glow and disc strengths other than 1 are stated now : `SkyColours`.
    None
}

/// FCOS rounded once to a double.
fn fcos(x: f64) -> f64 {
    cos_ext(x).to_f64()
}

/// The turn is not built for an angle ACOS(o′y) of this size or less.
const TURN_LEAST: f64 = 1e-10;

/// R⁻¹ of the Star Field Position, by rows: `None` where the field is not turned: with
/// o′ = o·q((ox·ox + oz·oz) + oy·oy) the turn is built only where ACOS(o′y) is more than 1e-10 (a
/// number that is not a number is not more): o′y = 1, o′y above 1 or below -1 by rounding, and an
/// orientation of length 0 (q(0) is +infinity, each part of o′ is not a number) leave the field not
/// turned. A matrix of NaNs (no entry is laid) where the axis (o′z, 0, −o′x) is zero and the angle
/// is not: an o along (0, y, 0) whose made-unit height is below 1, and straight
/// down. R is the rotation by ACOS(o′y) about the unit vector along (o′z, 0, −o′x) in the
/// original's arrangement: θ = −ACOS(y′); a = (z′, 0, −x′)·q(az·az + ax·ax); s = FSIN(θ) carried in
/// the extended register into each product, c = FCOS(θ) a double, v = 1 − c; rows (ax·ax·v + c,
/// az·s, az·ax·v), (−(az·s), c, ax·s), (az·ax·v, −(ax·s), az·az·v + c). R⁻¹ is R's inverse made by
/// the adjugate as makes the camera's (`view_sun::inverse_of`), not the transpose.
pub fn field_turn(o: Vec3) -> Result<Option<[[f64; 3]; 3]>, FiveOpen> {
    let five = RenderAs::Bryce50;
    let len2 = (o[0] * o[0] + o[2] * o[2]) + o[1] * o[1];
    let q = rsqrt_as(five, len2);
    let u = o.map(|x| x * q);
    let angle = acos(u[1]);
    if angle.is_nan() || angle <= TURN_LEAST {
        return Ok(None);
    }
    let theta = -acos(u[1]);
    let (ax0, az0) = (u[2], -u[0]);
    let qa = rsqrt_as(five, az0 * az0 + ax0 * ax0);
    let (ax, az) = (ax0 * qa, az0 * qa);
    let s = theta.is_finite().then(|| sin_ext(theta));
    let times_s = |x: f64| s.map_or(f64::NAN, |e| e.mul_to_f64(x));
    let c = fcos(theta);
    let v = 1.0 - c;
    let r = [
        [(ax * ax) * v + c, times_s(az), (az * ax) * v],
        [-times_s(az), c, times_s(ax)],
        [(az * ax) * v, -times_s(ax), (az * az) * v + c],
    ];
    Ok(Some(super::view_sun::inverse_of(r)))
}

/// Where a field's entries come from.
pub enum FieldSource<'a> {
    /// A seed that is not negative:'s list as 5.0 with the strengths.
    Seeded,
    /// A negative seed: Bryce 5's catalogue  and each record's colour (from
    /// star-colours).
    Catalogue { records: &'a [StarRecord], colour: &'a dyn Fn(&StarRecord) -> Option<[f32; 3]> },
}

/// A field as 5.0: its entries, how they are laid, its frame and switches.
#[derive(Clone, Debug)]
pub struct FiveField {
    pub entries: Arc<Vec<FieldEntry>>,
    /// The entries sorted into cells.
    pub grid: Arc<StarGrid>,
    pub laying: Laying,
    pub frame: FieldFrame,
    pub on: FieldSwitches,
}

/// Whether the sky has a field as 5.0: the second word's bit 0x2 or 0x4, and n or m not 0.
pub fn has_field(p: &FivePart, s: &SkyAsFive) -> bool {
    s.second_switches & (SECOND_STARS | SECOND_COMETS) != 0 && (p.star_count != 0 || p.comet_count != 0)
}

/// The sky as 5.0 for a ray that meets nothing, before the air: the sky's own colour (or the
/// plain colour), and the field laid over it.
#[derive(Clone, Debug)]
pub struct FiveSky {
    /// `None`: the plain colour, with no entry laid over it.
    pub colours: Option<SkyColours>,
    plain: [f32; 3],
    /// The field, where there is one and the colour is not plain.
    pub field: Option<FiveField>,
}

/// What preparing the sky as 5.0 gives: the sky, and where a seeded list left the generator.
#[derive(Clone, Debug)]
pub struct FivePrepared {
    pub sky: FiveSky,
    /// `Some` where a seeded list was made.,
    pub list_end: Option<CRand>,
}

/// The main light and the dome light for W.
pub fn prepare_lights(sun: &SunInputs, p: &FivePart, s: &SkyAsFive) -> PreparedSky {
    let inputs = SkyInputs { switches: s.switches, cloud_colour: p.cloud_colour, dome_colour: p.dome_colour };
    if s.switches & LIT_AIR_OPEN != 0 {
        // : Z and Γ are black and the main
        // light is the sun's whatever its height.
        let mut main = main_light(sun);
        main.direction = sun.direction;
        main.colour = sun.colour;
        return PreparedSky { main, dome: dome_light_black_sky(sun, &inputs, &main) };
    }
    prepare(RenderAs::Bryce50, sun, &inputs)
}

impl FiveSky {
    /// Prepares the sky as 5.0, making the field's list where there is a field (whether or not
    /// the colour is plain). `make_list` false: no list is made (the quick preview).
    pub fn prepare(
        sun: &SunInputs,
        p: &FivePart,
        s: &SkyAsFive,
        source: FieldSource<'_>,
        make_list: bool,
    ) -> Result<FivePrepared, FiveOpen> {
        let w = s.switches;
        let mut discs = p.discs;
        discs[1] = s.horizon_factor;
        let colours = SkyColours::new_bryce5(
            sun,
            &FiveColourSettings {
                switches: w,
                sky_colour: s.sky_colour,
                horizon_colour: s.horizon_colour,
                haze: p.haze,
                moon: p.moon,
                discs,
                glow_and_disc: p.glow_and_disc,
            },
        );
        let mut sky = FiveSky { colours, plain: p.plain_colour, field: None };
        if !has_field(p, s) || !make_list {
            return Ok(FivePrepared { sky, list_end: None });
        }
        let frame_turn = field_turn(p.orientation)?;
        let (entries, list_end, up_meets_nothing) = match source {
            FieldSource::Seeded => {
                let list = StarList::seeded_five(p.star_seed as u32, p.star_count, p.comet_count, p.strengths);
                (list.entries, Some(list.generator_after), true)
            }
            FieldSource::Catalogue { records, colour } => {
                let list = catalogue_list(records, p.star_count, p.strengths.stars, colour).map_err(|g| match g {
                    CatalogueGap::FewRecords => FiveOpen::FewRecords,
                    CatalogueGap::ColourOpen => FiveOpen::CatalogueColour,
                })?;
                (list, None, false)
            }
        };
        if sky.colours.is_some() {
            let light_height = main_light(sun).direction[1];
            let air_in = AirInputs {
                switches: w,
                haze_colour: p.haze_colour,
                haze: p.haze,
                fog_colour: [0.0; 3],
                fog_height: p.fog_height,
                fog: p.fog,
                cloud_height: 0.0,
                light_height,
            };
            let rates = if w & COLOR_PERSPECTIVE != 0 {
                haze_rates_shifted(&air_in, p.haze_shift)
            } else {
                haze_rates(RenderAs::Bryce50, &air_in)
            };
            let laying = Laying {
                version: RenderAs::Bryce50,
                sun: sun.direction,
                haze: if w & HAZE_SWITCH != 0 { p.haze } else { 0.0 },
                rates,
                haze_colour: p.haze_colour,
                orders: StarOrders::DECIDED,
            };
            sky.field = Some(FiveField {
                grid: Arc::new(StarGrid::new(&entries)),
                entries: Arc::new(entries),
                laying,
                frame: FieldFrame { inverse_turn: frame_turn, up_meets_nothing },
                on: FieldSwitches {
                    stars: s.second_switches & SECOND_STARS != 0,
                    comets: s.second_switches & SECOND_COMETS != 0,
                },
            });
        }
        Ok(FivePrepared { sky, list_end })
    }

    /// The plain colour, Pk ⊕ 0, where the sky has no colour of its own.
    pub fn plain(&self) -> Option<[f32; 3]> {
        self.colours.is_none().then(|| plain_sky(self.plain))
    }

    /// The colour after's step 6 (or step 1 with bit 0x8000) and Ψ beside it, before
    /// the layers; `None` where the colour is plain.
    pub fn colour_seen(&self, d: Vec3) -> Option<([f32; 3], f64)> {
        self.colours.as_ref().map(|c| c.colour_seen(d))
    }

    /// The entries of the field laid over the colour (after the layers) with Ψ held to [0, 1],
    /// then step 7., step 7
    pub fn finish(&self, d: Vec3, colour: [f32; 3], seen: f64) -> [f32; 3] {
        let c = match &self.field {
            None => colour,
            Some(f) => f.laying.lay_in(&f.entries, Some(&f.grid), &f.frame, d, colour, held(seen), f.on),
        };
        c.map(|x| x + 0.0f32)
    }

    /// The colour of a ray that meets nothing, before the air, with no cloud layer.
    pub fn colour(&self, d: Vec3) -> [f32; 3] {
        match self.colour_seen(d) {
            None => plain_sky(self.plain),
            Some((c, seen)) => self.finish(d, c, seen),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(version: u32, switches: u32) -> FivePart {
        FivePart {
            part_version: version,
            switches,
            second_switches: 0,
            cloud_colour: [0.0; 3],
            sky_colour: [0.1; 3],
            horizon_colour: [0.2; 3],
            dome_colour: [0.0; 3],
            plain_colour: [0.3; 3],
            haze_colour: [0.0; 3],
            haze: 0.0,
            fog: 0.0,
            fog_height: 0.0,
            discs: [0.0; 15],
            moon: [0.0; 4],
            lit_air: 0.0,
            star_seed: 0,
            star_count: 0,
            comet_count: 0,
            strengths: Strengths::ONE,
            orientation: [0.0, 1.0, 0.0],
            glow_and_disc: [1.0; 4],
            haze_shift: BRYCE3D_SHIFT,
        }
    }

    #[test]
    fn takeover_word() {
        // The README's first case: 0x021000a0 with haze 2.25 becomes 0x02108099, taken over plain.
        let mut p = part(PART_BRYCE3D, 0x0210_00a0);
        p.haze = 2.25;
        let s = as_five(&p);
        assert_eq!(s.switches, 0x0210_8099);
        assert_eq!(s.second_switches, 1);
        assert!(s.taken_over_plain);
        assert_eq!(s.sky_colour, p.plain_colour);
        // 's probe: 0x186 as 5.0 is 0x197.
        assert_eq!(as_five(&part(PART_BRYCE3D, 0x186)).switches, 0x197);
        // A field in a Bryce 3D sky gives the second word 7.
        assert_eq!(as_five(&part(PART_BRYCE3D, 0x8188)).second_switches, 7);
    }

    #[test]
    fn turn_carries_up_to_the_pole() {
        let o = [0.3, 0.9, 0.1];
        let inv = field_turn(o).unwrap().unwrap();
        // R⁻¹·o′ is straight up.
        let len = (0.09f64 + 0.81 + 0.01).sqrt();
        let u = o.map(|x| x / len);
        let up: Vec<f64> = (0..3).map(|i| inv[i][0] * u[0] + inv[i][1] * u[1] + inv[i][2] * u[2]).collect();
        assert!((up[0]).abs() < 1e-12 && (up[1] - 1.0).abs() < 1e-12 && up[2].abs() < 1e-12, "{up:?}");
        assert_eq!(field_turn([0.0, 1.0, 0.0]), Ok(None));
        // : length 0 or not a number leaves the field not turned,
        // straight down turns it into nothing (a matrix of NaNs: no entry is laid)
        assert_eq!(field_turn([0.0, 0.0, 0.0]), Ok(None));
        assert_eq!(field_turn([f64::NAN, 1.0, 0.0]), Ok(None));
        let down = field_turn([0.0, -1.0, 0.0]).unwrap().unwrap();
        assert!(down.iter().flatten().all(|x| x.is_nan()));
    }
}
