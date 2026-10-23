// SPDX-License-Identifier: MIT
//! Stars and comets: the seeded list of a sky's field, which entries a ray meets, and how
//! each is laid over the colour so far; the colour of a star of Bryce 5's catalogue.
//!
//! ("Stars and comets": the list as 3.0 and 3.1, the list as 5.0, which entries a
//! ray meets, what a ray sees as 3.0 and 3.1 (step 5, the layers' share), laying an entry as
//! 3.0 and 3.1, laying an entry as 5.0, catalogue stars) in;
//! in
//! (Stars and comets),
//!
//! Notation as: every binary64 operation one Rust `f64` operation in the order
//! written, ⟨x⟩ `x as f32`, ⊕ a Rust `f32` sum. Where the entry leaves a grouping free the
//! written one is taken (products of three left to right).

use std::f64::consts::LOG2_E;

use crate::geom::Vec3;
use crate::numerics::x87::cos_ext;
use crate::numerics::{CRand, acos, exp2_as, log2_as, pow, rsqrt_as};
use super::star_grid::{StarGrid, WideStarGrid};
use crate::substrate_kept::{StarColour, StarRecord, star_colour};
use crate::version::RenderAs;

/// U, the double nearest 1/32767.
const U: f64 = 1.0 / 32767.0;
/// Δ, the double nearest 0.01745329251994 (not π/180).
const DELTA: f64 = 0.01745329251994;
/// The colour's spread: 0.1 as 3.0 and 3.1, 0.2 as 5.0.
const COLOUR_SPREAD_THREE: f64 = 0.1;
const COLOUR_SPREAD_FIVE: f64 = 0.2;
/// The brightness b = 1.2 − u7·0.7.
const BRIGHT_BASE: f64 = 1.2;
const BRIGHT_SPREAD: f64 = 0.7;
/// The point's cosine c = 0.9999995 − u8·1e-6 (as 5.0 − u8·σ, σ = 0.9999995 − 0.9999985).
const POINT_TOP: f64 = 0.9999995;
const POINT_SPREAD_THREE: f64 = 1e-6;
const POINT_LOW_FIVE: f64 = 0.9999985;
/// The reach adds 1.5Δ to the point's angle.
const REACH_ADD: f64 = 1.5;
/// A comet: red ·0.8, green ·0.9, brightness ·0.32, tail 7 degrees, head 1 degree.
const COMET_RED: f64 = 0.8;
const COMET_GREEN: f64 = 0.9;
const COMET_BRIGHT: f64 = 0.32;
const COMET_TAIL: f64 = 7.0;
/// A strength above 1 widens the point's range: L = 0.9999985 − Is·7.5·10^−7.
const POINT_WIDEN: f64 = 7.5e-7;
/// The catalogue: brightness ((7 − G)·k7)·Is, k7 the double nearest 1/7; the point's cosine
/// 0.9999995 − b·10^−6.
const CATALOGUE_DIM: f64 = 7.0;
const K7: f64 = 1.0 / 7.0;
const CATALOGUE_POINT: f64 = 1e-6;

/// The stars' and the comets' strengths as 5.0 (`star_intensity`, `comet_intensity`;
/// 1 and 1 in a Bryce 3D sky).,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strengths {
    /// Is.
    pub stars: f64,
    /// Ic.
    pub comets: f64,
}

impl Strengths {
    /// A Bryce 3D sky's.,
    pub const ONE: Strengths = Strengths { stars: 1.0, comets: 1.0 };
}
/// The tail's colour factor.
const TAIL_COLOUR: f64 = 0.75;
/// The tail's numbers: r·0.2, a3, 150·e, pow(r, 0.5).
const TAIL_R: f64 = 0.2;
const A3: f64 = 0.5198603854199589;
const TAIL_GAIN: f64 = 150.0;
const TAIL_POWER: f64 = 0.5;
/// k4, the double nearest 5.7707801635558535.
const K4: f64 = 5.7707801635558535;
/// The glow's power 0.05, the haze share 0.2 and a4.
const GLOW_POWER: f64 = 0.05;
const GLOW_HAZE: f64 = 0.2;
const A4: f64 = 0.5418539921951662;
/// The sun at a right angle to a comet: |ds| below this.
const TAIL_RIGHT_ANGLE: f64 = 1e-7;
/// The glow's k is 0 for z within this of 0.
const GLOW_ZERO: f64 = 1e-10;
/// k10, the double whose bits are 402cda993e7663be (as's k). step 5
const K10: f64 = f64::from_bits(0x402c_da99_3e76_63be);

/// The kind of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Star,
    Comet,
}

/// One entry of a field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldEntry {
    pub kind: FieldKind,
    /// V.
    pub direction: Vec3,
    /// (κr, κg, κb).
    pub colour: [f32; 3],
    /// b.
    pub brightness: f64,
    /// c: within it the star is drawn at full strength (a comet's head).
    pub point_cosine: f64,
    /// g: within it the glow (a comet's tail) shows.
    pub reach_cosine: f64,
}

/// The three orders of a sum of three terms a·a′ + b·b′ + c·c′ (x, y, z): which pair is
/// added first. (Stars and comets: `Order3`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order3 {
    /// (x + y) + z.
    XyZ,
    /// (x + z) + y.
    XzY,
    /// (y + z) + x.
    YzX,
}

impl Order3 {
    pub const ALL: [Order3; 3] = [Order3::XyZ, Order3::XzY, Order3::YzX];

    /// The sum of the three products, in this order.
    pub fn dot(self, a: Vec3, b: Vec3) -> f64 {
        let (x, y, z) = (a[0] * b[0], a[1] * b[1], a[2] * b[2]);
        match self {
            Order3::XyZ => (x + y) + z,
            Order3::XzY => (x + z) + y,
            Order3::YzX => (y + z) + x,
        }
    }
}

/// The orders's Accuracy leaves to the vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarOrders {
    /// The point's squared length as 3.0 and 3.1.
    pub point_three: Order3,
    /// The point's squared length as 5.0.
    pub point_five: Order3,
    /// d = D·V.
    pub d: Order3,
    /// The ray's squared length as 5.0.
    pub ray_five: Order3,
}

impl StarOrders {
    /// The orders that give every row of stars.jsonl and star-rays.jsonl (found by trying
    /// the three of each against them: tests/sky_star_vectors.rs).
    pub const DECIDED: StarOrders =
        StarOrders { point_three: Order3::YzX, point_five: Order3::XyZ, d: Order3::YzX, ray_five: Order3::YzX };
}

/// A field's list and where it left its generator.
#[derive(Clone, Debug, PartialEq)]
pub struct StarList {
    pub entries: Vec<FieldEntry>,
    /// The generator after the list's last draw: the program's one generator, reseeded by the
    /// making with the seed word, stands here when the render's own draws begin (
    /// Effects). (Where the render's draws start)
    pub generator_after: CRand,
}

/// FCOS rounded once to a double.
fn fcos(x: f64) -> f64 {
    cos_ext(x).to_f64()
}

/// The next draw as u = r × U.
fn draw(g: &mut CRand) -> f64 {
    f64::from(g.next_u15()) * U
}

impl StarList {
    /// The list a seed and two counts make as `version`, with the decided orders.
    pub fn seeded(version: RenderAs, seed: u32, stars: u32, comets: u32) -> StarList {
        StarList::seeded_with(version, seed, stars, comets, StarOrders::DECIDED)
    }

    /// `seeded` with the orders given (for the tests that try each).
    pub fn seeded_with(version: RenderAs, seed: u32, stars: u32, comets: u32, orders: StarOrders) -> StarList {
        StarList::seeded_strengths(version, seed, stars, comets, Strengths::ONE, orders)
    }

    /// The seeded list as 5.0 with the stars' and the comets' strengths (Bryce 5's Random
    /// field).,
    pub fn seeded_five(seed: u32, stars: u32, comets: u32, strengths: Strengths) -> StarList {
        StarList::seeded_strengths(RenderAs::Bryce50, seed, stars, comets, strengths, StarOrders::DECIDED)
    }

    /// The list a seed and two counts make; as 5.0 with the strengths given (as 3.0 and 3.1
    /// they are not read).,
    pub fn seeded_strengths(
        version: RenderAs,
        seed: u32,
        stars: u32,
        comets: u32,
        strengths: Strengths,
        orders: StarOrders,
    ) -> StarList {
        let mut g = CRand::new(seed);
        let five = version == RenderAs::Bryce50;
        // σ = 0.9999995 − L: L the double nearest 0.9999985 where Is is not above 1, else
        // 0.9999985 − Is·7.5·10^−7.
        let low = if strengths.stars > 1.0 { POINT_LOW_FIVE - strengths.stars * POINT_WIDEN } else { POINT_LOW_FIVE };
        let sigma = POINT_TOP - low;
        let count = if five { u64::from(stars) + u64::from(comets) } else { u64::from(stars.max(comets)) };
        let mut entries = Vec::with_capacity(count.min(1 << 20) as usize);
        let mut comets_left = comets;
        while (entries.len() as u64) < count {
            // The point on the sphere.
            let (p, mu) = if five {
                loop {
                    let p = [1.0 - (draw(&mut g) * 2.0), 1.0 - (draw(&mut g) * 2.0), 1.0 - (draw(&mut g) * 2.0)];
                    let mu = rsqrt_as(version, orders.point_five.dot(p, p));
                    if !(mu < 1.0) {
                        break (p, mu);
                    }
                }
            } else {
                let p = [1.0 - (draw(&mut g) * 2.0), 1.0 - (draw(&mut g) * 2.0), 1.0 - (draw(&mut g) * 2.0)];
                (p, rsqrt_as(version, orders.point_three.dot(p, p)))
            };
            let v: Vec3 = p.map(|pk| mu * pk);
            // The colour, the brightness, the point's cosine.
            let spread = if five { COLOUR_SPREAD_FIVE } else { COLOUR_SPREAD_THREE };
            let mut colour: [f32; 3] = std::array::from_fn(|_| (1.0 - draw(&mut g) * spread) as f32);
            let mut b = BRIGHT_BASE - draw(&mut g) * BRIGHT_SPREAD;
            let u8 = draw(&mut g);
            let mut c = if five { POINT_TOP - u8 * sigma } else { POINT_TOP - u8 * POINT_SPREAD_THREE };
            let mut reach = fcos(acos(c) + REACH_ADD * DELTA);
            let mut kind = FieldKind::Star;
            let drawn = b;
            if five {
                b = drawn * strengths.stars;
            }
            if comets_left != 0 {
                comets_left -= 1;
                kind = FieldKind::Comet;
                colour[0] = (f64::from(colour[0]) * COMET_RED) as f32;
                colour[1] = (f64::from(colour[1]) * COMET_GREEN) as f32;
                b = if five { drawn * strengths.comets } else { drawn };
                b *= COMET_BRIGHT;
                reach = fcos(COMET_TAIL * DELTA);
                c = fcos(DELTA);
            }
            if v[1].abs() < reach {
                entries.push(FieldEntry { kind, direction: v, colour, brightness: b, point_cosine: c, reach_cosine: reach });
            }
        }
        StarList { entries, generator_after: g }
    }
}

/// The singles nearest π/12, π/180 and π/2 (a15, aD, p2).
fn catalogue_angles() -> (f64, f64, f64) {
    use std::f64::consts::PI;
    (f64::from((PI / 12.0) as f32), f64::from((PI / 180.0) as f32), f64::from((PI / 2.0) as f32))
}

/// FSIN rounded once to a double.
fn fsin(x: f64) -> f64 {
    crate::numerics::x87::sin_ext(x).to_f64()
}

/// One star of Bryce 5's catalogue from its record, with the stars' strength Is and its
/// colour (from star-colours,; the caller's). (Stars as 5.0: the catalogue
/// list)
pub fn catalogue_entry(record: &StarRecord, strength: f64, colour: [f32; 3]) -> FieldEntry {
    let (a15, a_d, p2) = catalogue_angles();
    let g = f64::from(record.magnitude);
    let b = ((CATALOGUE_DIM - g) * K7) * strength;
    // α = A·a15 and D·aD are exact products of singles; θ's difference in binary64.
    let alpha = f64::from(record.right_ascension_hours) * a15;
    let theta = p2 - f64::from(record.declination_degrees) * a_d;
    let (ca, sa, st) = (fcos(alpha), fsin(alpha), fsin(theta));
    let direction = [
        f64::from((ca * st) as f32),
        f64::from(fcos(f64::from(theta as f32)) as f32),
        f64::from((sa * st) as f32),
    ];
    let c = POINT_TOP - b * CATALOGUE_POINT;
    let reach = fcos(acos(c) + b * (REACH_ADD * DELTA));
    FieldEntry { kind: FieldKind::Star, direction, colour, brightness: b, point_cosine: c, reach_cosine: reach }
}

/// Why a catalogue list cannot be made.,
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueGap {
    /// The file holds fewer records than ⌈n/2⌉ (Open).
    FewRecords,
    /// A record whose colour leaves Open (class R).
    ColourOpen,
}

/// The catalogue list: one star from each of the first ⌈n/2⌉ records, in file order, kept
/// whatever its height. `colour` gives each record's colour (`None`: a colour leaves
/// Open).
pub fn catalogue_list(
    records: &[StarRecord],
    stars: u32,
    strength: f64,
    mut colour: impl FnMut(&StarRecord) -> Option<[f32; 3]>,
) -> Result<Vec<FieldEntry>, CatalogueGap> {
    let take = u64::from(stars).div_ceil(2).min(records.len() as u64);
    records[..take as usize]
        .iter()
        .map(|r| Ok(catalogue_entry(r, strength, colour(r).ok_or(CatalogueGap::ColourOpen)?)))
        .collect()
}

/// The frame a field is laid in: the turn of Bryce 5's Star Field Position (R⁻¹, applied to a
/// ray's direction before the entries are tested), and whether a ray straight up in the field's
/// frame meets nothing (a seeded list) or is tested like any other (a catalogue list:
/// Open, every entry tested).,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldFrame {
    /// R⁻¹ by rows, `None` where the field is not turned.
    pub inverse_turn: Option<[[f64; 3]; 3]>,
    pub up_meets_nothing: bool,
}

impl FieldFrame {
    /// A field that is not turned, whose list is seeded.
    pub const UPRIGHT: FieldFrame = FieldFrame { inverse_turn: None, up_meets_nothing: true };
}

/// Held to [0, 1]: below 0 or not a number to 0, above 1 to 1.
pub fn held(x: f64) -> f64 {
    if x > 1.0 {
        1.0
    } else if x >= 0.0 {
        x
    } else {
        0.0
    }
}

/// Ψ after a cloud layer that acts: (1 − y)·Ψ, y = 1 − 2^(κ·(−k10)) held to [0, 1]; κ the
/// layer's cover as the caller holds it (the upper layer's held to [0, 1], the lower layer's
/// held to 1 from above, and only where above 0).
pub fn seen_after_layer(version: RenderAs, seen: f64, kappa: f64) -> f64 {
    let y = held(1.0 - exp2_as(version, kappa * -K10));
    (1.0 - y) * seen
}

/// Ψ after the layers that acted, the upper then the lower (its κ held to 1 from above).
pub fn seen_after_layers(version: RenderAs, mut seen: f64, covers: super::clouds::LayerCovers) -> f64 {
    if let Some(k) = covers.upper {
        seen = seen_after_layer(version, seen, k);
    }
    if let Some(k) = covers.lower {
        seen = seen_after_layer(version, seen, if k > 1.0 { 1.0 } else { k });
    }
    seen
}

/// The numbers the laying reads beside the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Laying {
    pub version: RenderAs,
    /// S, the sun's direction (the sun's even when the main light is the moon).
    pub sun: Vec3,
    /// h (as 5.0 h5, the haze that counts).
    pub haze: f64,
    /// ρ,'s haze rates.
    pub rates: [f32; 3],
    /// H, the haze colour.
    pub haze_colour: [f32; 3],
    pub orders: StarOrders,
}

/// Which kinds a ray's laying shows: (stars, comets). As 3.0 and 3.1 both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSwitches {
    pub stars: bool,
    pub comets: bool,
}

impl FieldSwitches {
    pub const BOTH: FieldSwitches = FieldSwitches { stars: true, comets: true };
}

/// The whole-difference blend toward three doubles.
fn whole_blend(c: [f32; 3], t: [f64; 3], a: f64) -> [f32; 3] {
    let e: [f64; 3] = std::array::from_fn(|k| t[k] - f64::from(c[k]));
    [c[0] + (a * e[0]) as f32, c[1] + (a * e[1]) as f32, (a * e[2] + f64::from(c[2])) as f32]
}

/// The stored-difference blend toward three singles.
fn stored_blend(c: [f32; 3], t: [f32; 3], a: f64) -> [f32; 3] {
    let er = f64::from(t[0]) - f64::from(c[0]);
    let eg = f64::from(t[1] - c[1]);
    let eb = f64::from(t[2] - c[2]);
    [c[0] + (a * er) as f32, c[1] + (a * eg) as f32, (a * eb + f64::from(c[2])) as f32]
}

/// 5.0's single blend with the differences given: colour ⊕ ⟨a′·e⟩.
fn single_blend_e(c: [f32; 3], e: [f32; 3], a: f64) -> [f32; 3] {
    let a1 = a as f32;
    std::array::from_fn(|k| c[k] + a1 * e[k])
}

/// 5.0's single blend toward three singles.
fn single_blend(c: [f32; 3], t: [f32; 3], a: f64) -> [f32; 3] {
    single_blend_e(c, std::array::from_fn(|k| t[k] - c[k]), a)
}

fn scaled(v: Vec3, s: f64) -> Vec3 {
    v.map(|x| x * s)
}

impl Laying {
    /// The colour so far with the entries `dir` meets laid over it, in list order, with the
    /// visibility `seen` (Ψ, held by the caller). (Which entries a ray meets;
    /// Laying an entry as 3.0 and 3.1; as 5.0)
    pub fn lay(&self, list: &[FieldEntry], dir: Vec3, colour: [f32; 3], seen: f64, on: FieldSwitches) -> [f32; 3] {
        self.lay_in(list, None, &FieldFrame::UPRIGHT, dir, colour, seen, on)
    }

    /// `lay` as 3.0 and 3.1 with the entries looked up through a widened grid made from
    /// `list`: the same entries tested in the same order, with the same colour.
    /// (Stars and comets)
    pub fn lay_wide(
        &self,
        list: &[FieldEntry],
        wide: &WideStarGrid,
        dir: Vec3,
        colour: [f32; 3],
        seen: f64,
        on: FieldSwitches,
    ) -> [f32; 3] {
        self.lay_with(list, None, Some(wide), &FieldFrame::UPRIGHT, dir, colour, seen, on)
    }

    /// `lay` in a field's frame: as 5.0 the ray's direction D is given to the field as
    /// D′ = R⁻¹·D (then made unit again, as D is); the comet's tail takes the sun as it stands.
    pub fn lay_in(
        &self,
        list: &[FieldEntry],
        grid: Option<&StarGrid>,
        frame: &FieldFrame,
        dir: Vec3,
        colour: [f32; 3],
        seen: f64,
        on: FieldSwitches,
    ) -> [f32; 3] {
        self.lay_with(list, grid, None, frame, dir, colour, seen, on)
    }

    #[allow(clippy::too_many_arguments)]
    fn lay_with(
        &self,
        list: &[FieldEntry],
        grid: Option<&StarGrid>,
        wide: Option<&WideStarGrid>,
        frame: &FieldFrame,
        dir: Vec3,
        colour: [f32; 3],
        seen: f64,
        on: FieldSwitches,
    ) -> [f32; 3] {
        let five = self.version == RenderAs::Bryce50;
        let dir = match (five, frame.inverse_turn) {
            (true, Some(m)) => super::view_sun::apply_inverse(true, &m, dir),
            _ => dir,
        };
        // : a ray straight up meets no entry of a seeded list as 5.0; as 3.0 and
        // 3.1 Open, and a program may lay nothing (chosen). A catalogue list: Open,
        // every entry tested .
        if frame.up_meets_nothing && dir[0] == 0.0 && dir[2] == 0.0 && dir[1] > 0.0 {
            return colour;
        }
        let d_ray = if five {
            scaled(dir, rsqrt_as(self.version, self.orders.ray_five.dot(dir, dir)))
        } else {
            dir
        };
        let mut c = colour;
        let mut lay_one = |e: &FieldEntry| {
            let d = self.orders.d.dot(d_ray, e.direction);
            if !(d >= e.reach_cosine) {
                return;
            }
            c = if five { self.lay_five(e, d_ray, d, c, seen, on) } else { self.lay_three(e, d_ray, d, c, seen) };
        };
        match grid {
            // : the entries that reach the ray's cell, in the order of the list
            Some(g) if five => g.meets(d_ray).iter().for_each(|&i| lay_one(&list[i as usize])),
            // As 3.0 and 3.1 the widened grid gives every entry whose test can hold, in list
            // order (the others lay nothing): the same as testing the whole list.
            _ => match wide.filter(|_| !five).and_then(|w| w.candidates(d_ray)) {
                Some(ids) => ids.iter().for_each(|&i| lay_one(&list[i as usize])),
                None => list.iter().for_each(lay_one),
            },
        }
        c
    }

    /// lit = (κ·b), each a double.
    fn lit(e: &FieldEntry) -> [f64; 3] {
        e.colour.map(|k| f64::from(k) * e.brightness)
    }

    /// The tail's v where along > −1, else `None`.
    fn tail_weight(&self, e: &FieldEntry, dir: Vec3, d: f64) -> Option<f64> {
        let version = self.version;
        let v = e.direction;
        let s = self.sun;
        let ds = Order3::XyZ.dot(s, v);
        let w: Vec3 = std::array::from_fn(|k| dir[k] / d - v[k]);
        let p = scaled(w, rsqrt_as(version, Order3::XyZ.dot(w, w)));
        let w1: Vec3 = std::array::from_fn(|k| v[k] - s[k] / ds);
        let mut t = scaled(w1, rsqrt_as(version, Order3::XyZ.dot(w1, w1)));
        if ds < 0.0 {
            t = t.map(|x| -x);
        }
        // (A comet first has its tail on: where the sun's direction is at
        // a right angle to the comet's, |ds| < 10^-7, t is the opposite of the sun's direction)
        if ds.abs() < TAIL_RIGHT_ANGLE {
            t = s.map(|x| -x);
        }
        let along = Order3::XyZ.dot(t, p);
        if !(along > -1.0) {
            return None;
        }
        let r = (1.0 - d) / (1.0 - e.reach_cosine);
        let ee = exp2_as(version, (log2_as(version, r * TAIL_R) * A3) * LOG2_E);
        let w = ((1.0 - along) * 0.5) * (1.0 + TAIL_GAIN * ee);
        let mut vv = (1.0 - w) * pow(r, TAIL_POWER) + w;
        if vv > 1.0 {
            vv = 1.0;
        }
        Some(vv)
    }

    /// The comet head's e.
    fn head_e(&self, e: &FieldEntry, d: f64) -> f64 {
        exp2_as(self.version, ((1.0 - d) / (1.0 - e.point_cosine)) * -K4)
    }

    /// The glow's p and k.
    fn glow_weight(&self, e: &FieldEntry, d: f64) -> (f64, f64) {
        let version = self.version;
        let p = pow((e.point_cosine - d) / (e.point_cosine - e.reach_cosine), GLOW_POWER);
        let z = self.haze * GLOW_HAZE;
        let k = if z > -GLOW_ZERO && z < GLOW_ZERO {
            0.0
        } else {
            exp2_as(version, (log2_as(version, z) * A4) * LOG2_E)
        };
        (p, k)
    }

    fn lay_three(&self, e: &FieldEntry, dir: Vec3, d: f64, mut c: [f32; 3], seen: f64) -> [f32; 3] {
        let lit = Self::lit(e);
        match e.kind {
            FieldKind::Comet => {
                if let Some(v) = self.tail_weight(e, dir, d) {
                    let tail = lit.map(|l| (TAIL_COLOUR * l) as f32);
                    c = stored_blend(c, tail, (1.0 - v) * seen);
                }
                if d > e.point_cosine {
                    let ee = self.head_e(e, d);
                    c = whole_blend(c, lit, (1.0 - (1.0 - ee)) * seen);
                }
            }
            FieldKind::Star => {
                if d > e.point_cosine {
                    c = whole_blend(c, lit, seen);
                } else {
                    let (rho, h) = (self.rates.map(f64::from), self.haze_colour.map(f64::from));
                    let glow = [
                        (lit[0] * rho[0] * h[0] * seen) as f32,
                        (f64::from((lit[1] * rho[1] * h[1]) as f32) * seen) as f32,
                        (f64::from((lit[2] * rho[2] * h[2]) as f32) * seen) as f32,
                    ];
                    let (p, k) = self.glow_weight(e, d);
                    c = stored_blend(c, glow, ((1.0 - p) * k) * seen);
                }
            }
        }
        c
    }

    fn lay_five(&self, e: &FieldEntry, dir: Vec3, d: f64, mut c: [f32; 3], seen: f64, on: FieldSwitches) -> [f32; 3] {
        let lit = Self::lit(e);
        let lit1 = lit.map(|l| l as f32);
        match e.kind {
            FieldKind::Comet if on.comets => {
                if let Some(v) = self.tail_weight(e, dir, d) {
                    let tail = lit.map(|l| (TAIL_COLOUR * l) as f32);
                    c = single_blend(c, tail, (1.0 - v) * seen);
                }
                if d > e.point_cosine {
                    let ee = self.head_e(e, d);
                    c = single_blend(c, lit1, (1.0 - (1.0 - ee)) * seen);
                }
            }
            FieldKind::Star if on.stars => {
                if d > e.point_cosine {
                    let diff = [(lit[0] - f64::from(c[0])) as f32, lit1[1] - c[1], lit1[2] - c[2]];
                    c = single_blend_e(c, diff, seen);
                } else {
                    let (rho, h) = (self.rates.map(f64::from), self.haze_colour.map(f64::from));
                    let gr = (lit[0] * rho[0] * h[0] * seen) as f32;
                    let gg = (f64::from((f64::from(lit1[1]) * rho[1] * h[1]) as f32) * seen) as f32;
                    let gb = (f64::from((f64::from(lit1[2]) * rho[2] * h[2]) as f32) * seen) as f32;
                    let (p, k) = self.glow_weight(e, d);
                    let a = ((1.0 - p) * k) * seen;
                    let er = f64::from(gr) - f64::from(c[0]);
                    let eg = f64::from(gg - c[1]);
                    let eb = f64::from(gb - c[2]);
                    c = [
                        (f64::from((er * a) as f32) + f64::from(c[0])) as f32,
                        c[1] + (eg * a) as f32,
                        (a * eb + f64::from(c[2])) as f32,
                    ];
                }
            }
            _ => {}
        }
        c
    }
}

/// The colour of a star of Bryce 5's catalogue from the star-colours table: ξk = ⟨bk·B′⟩
/// (B′ the single nearest 1/255), channel k ⟨(1 − ξk)·0.5 + ξk⟩. `None` where leaves
/// the class's colour Open (class R).,
pub fn catalogue_colour(table: &[u8; 105], record: &StarRecord) -> Option<[f32; 3]> {
    let bytes = match star_colour(table, record.class, record.subclass, record.blank) {
        StarColour::Table(b) => b,
        StarColour::White => [255; 3],
        StarColour::Open => return None,
    };
    let b1 = (1.0f64 / 255.0) as f32;
    Some(bytes.map(|b| {
        let xi = f64::from((f64::from(b) * f64::from(b1)) as f32);
        ((1.0 - xi) * 0.5 + xi) as f32
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comets_come_first() {
        let l = StarList::seeded(RenderAs::Bryce31, 12345, 10, 3);
        assert_eq!(l.entries.len(), 10);
        let kinds: Vec<FieldKind> = l.entries.iter().map(|e| e.kind).collect();
        let first_star = kinds.iter().position(|k| *k == FieldKind::Star).unwrap();
        assert!(kinds[first_star..].iter().all(|k| *k == FieldKind::Star));
    }

    #[test]
    fn catalogue_white_and_open() {
        let table = [0u8; 105];
        let rec = |class| StarRecord {
            class,
            subclass: 0,
            blank: false,
            right_ascension_hours: 0.0,
            declination_degrees: 0.0,
            magnitude: 0.0,
        };
        assert_eq!(catalogue_colour(&table, &rec(b'R')), Some([0.5; 3]));
        assert_eq!(catalogue_colour(&table, &rec(b'Q')), Some([1.0; 3]));
        assert_eq!(catalogue_colour(&table, &rec(b'G')), Some([0.5; 3]));
    }
}
