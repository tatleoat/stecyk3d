// SPDX-License-Identifier: MIT
//! The further rays a shaded surface sends on (a reflected and a transmitted ray, bent where
//! the refraction values on its two sides differ), and how what they bring back joins the
//! surface's colour.
//!
//! (The media, The
//! angle law, the reflected ray steps 1 to 4, the transmitted ray steps 1 to 6, Both rays,
//! The blurred rays, What a ray brings back joins the colour, Last bits)
//! (Further rays: further.rs; Media, refraction, blend and blur)
//!
//! Two pure steps, no callbacks and no recursion, so that the renderer can run them as
//! stages over batches: `send` (a hit in, at most two rays out, in the order they are sent)
//! and `join` (the local colour and what the sent rays brought back in, the surface's
//! colour out). Everything is plain `Copy` data. The weight a ray in a solid's medium comes
//! back with, the blend and the blurred cone are `shading::media`.

use crate::geom::Vec3;
use crate::numerics::{pow, sqrt_as};
use crate::version::RenderAs;

use super::lighting::SumOrder;
use super::media::{AngleLaw, LawForm, transmitted_colour};

/// The depth limit: a ray whose depth is this or more sends no further ray.
pub const DEPTH_LIMIT: u32 = 6;
/// The cut-off K on the mean of a further ray's total weight: the double nearest 0.001.
pub const CUT_OFF: f64 = 0.001;
/// The factor M of the mean: the double nearest 0.333333 (not one third).
pub const MEAN_FACTOR: f64 = 0.333333;
/// Where a further ray starts counting surfaces: the double nearest 0.0001.
pub const RAY_START: f64 = 0.0001;
/// The exponent of the reflection's tint as 3.1 and 5.0: the double nearest 0.8 (3.0: 0).
const TINT_POWER_31: f64 = 0.8;
/// The scene's limit of total internal reflections in a row for every Bryce 3D document
/// rendered as 5.0.
pub const TIR_LIMIT_BRYCE3D: u32 = 2;

/// The bits of the scene's switch word  that reads.
pub mod switch {
    /// Reflection on.
    pub const REFLECTION: u32 = 0x1;
    /// Transmission on.
    pub const TRANSMISSION: u32 = 0x2;
    /// Refraction on: the transmitted ray is bent.
    pub const REFRACTION: u32 = 0x4;
    /// A reflected ray from a surface seen from behind its shading normal, or dipping below
    /// the real surface, turns to the real surface's normal. steps 1 and 3
    pub const TURN_TO_SURFACE: u32 = 0x10;
    /// A transmitted ray from a surface seen from behind its shading normal, or bent out of
    /// the front.
    pub const TRANSMIT_FROM_BEHIND: u32 = 0x20;
    /// As 5.0: total internal reflection turned into reflection.
    pub const TIR_TO_REFLECTION: u32 = 0x40_0000;
    /// As 5.0: the blurred reflected ray.
    pub const BLUR_REFLECTED: u32 = 0x80_0000;
    /// As 5.0: the blurred transmitted ray.
    pub const BLUR_TRANSMITTED: u32 = 0x100_0000;
}

/// What a material gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayMaterial {
    /// Diffuse colour D.
    pub diffuse: [f32; 3],
    /// Transparent colour Θ.
    pub transparent_colour: [f32; 3],
    /// Specular Halo colour H (the blurred cone's width).
    pub halo: [f32; 3],
    /// Transparency t (a plain material's A value widened;'s double for a material
    /// whose channels have textures).
    pub transparency: f64,
    /// The Transparency that decides whether the material has an angle law: t itself for a
    /// plain material, the A value of its Transparency channel for one whose channels have
    /// textures.
    pub law_transparency: f64,
    /// Reflection ρ.
    pub reflection: f64,
    /// Metallicity m.
    pub metallicity: f64,
    /// Refraction value η.
    pub refraction: f32,
    /// A blended material (flag word 0x10008) rather than a plain one.
    pub blended: bool,
}

/// The two media at a surface, by their refraction values (`None`: outside every object,
/// refraction value 1): the incoming ray's and the one on the far side (for a shape that
/// encloses no volume, the shape's own material's value).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RayMedia {
    pub incoming: Option<f32>,
    pub beyond: Option<f32>,
}

impl RayMedia {
    /// r = η1 / η2.
    pub fn ratio(&self) -> f64 {
        let eta = |m: Option<f32>| m.map_or(1.0, f64::from);
        eta(self.incoming) / eta(self.beyond)
    }
}

/// A shaded hit, as the further-ray step needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The hit point p.
    pub point: Vec3,
    /// The incoming ray's direction d.
    pub ray_direction: Vec3,
    /// The incoming ray's depth (0 for a camera ray).
    pub depth: u32,
    /// The depth at which a ray sends no further ray: L in a frame (`DEPTH_LIMIT`, or as 5.0 the
    /// document's ray depth where it holds Bryce 5's part), 3 in the quick
    /// preview (component `sampling-output`).
    pub depth_limit: u32,
    /// The incoming ray's total weight T ((1, 1, 1) for a camera ray).
    pub total: [f32; 3],
    /// The geometric normal g.
    pub geometric_normal: Vec3,
    /// The shading normal n as's facing test left it.
    pub shading_normal: Vec3,
    /// Whether the object hit encloses a volume (component `geometry`; which shapes do
    /// is the renderer's table, Inputs: see `render::lit`).
    pub encloses_volume: bool,
    /// The scene's switch word .
    pub switches: u32,
    /// The media on the two sides of the surface.
    pub media: RayMedia,
    /// Whether a surface of the material had been met by a ray before (the angle law).
    pub met_before: bool,
    /// As 5.0: the incoming ray's count of total internal reflections (0 for a camera ray).
    pub tir_count: u32,
    /// As 5.0: the scene's limit of total internal reflections in a row.
    pub tir_limit: u32,
}

/// Which further ray.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RayKind {
    Reflected,
    Transmitted,
}

/// The medium a sent ray travels in, relative to the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medium {
    /// The medium of the incoming ray.
    Incoming,
    /// The medium on the surface's far side.
    Beyond,
}

/// A further ray that is sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurtherRay {
    pub kind: RayKind,
    pub origin: Vec3,
    /// Not brought to unit length.
    pub direction: Vec3,
    /// The distance from which the ray counts surfaces.
    pub start: f64,
    /// Its weight w.
    pub weight: [f32; 3],
    /// Its total weight T'.
    pub total: [f32; 3],
    pub depth: u32,
    /// The medium it travels in.
    pub medium: Medium,
    /// The count of total internal reflections it carries (as 5.0; otherwise its parent's).
    pub tir_count: u32,
}

/// The rays one hit sends, in the order they are sent (at most two).
/// (a fixed array, no allocation per hit)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sent {
    rays: [Option<FurtherRay>; 2],
}

impl Sent {
    fn push(&mut self, r: FurtherRay) {
        let slot = self.rays.iter_mut().find(|s| s.is_none()).expect("at most two further rays");
        *slot = Some(r);
    }
    /// The sent rays, in the order sent.
    pub fn iter(&self) -> impl Iterator<Item = &FurtherRay> {
        self.rays.iter().flatten()
    }
    /// The sent rays, mutably (the blurred turn changes a direction after the cut-off).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut FurtherRay> {
        self.rays.iter_mut().flatten()
    }
    pub fn len(&self) -> usize {
        self.iter().count()
    }
    pub fn is_empty(&self) -> bool {
        self.rays[0].is_none()
    }
}

/// How the product r²·(1 − c²) of k is grouped (the text: "the grouping of its product
/// changes the last bits, and refraction.jsonl decides it").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KGrouping {
    /// (r·r)·(1 − c·c)
    SquareFirst,
    /// r·(r·(1 − c·c))
    OneAtATime,
}

impl KGrouping {
    pub const ALL: [KGrouping; 2] = [KGrouping::SquareFirst, KGrouping::OneAtATime];
}

/// The orders of the three-term sums and the groupings the entry leaves to the vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RayOrders {
    /// κ = −(ν·d), step 2.
    pub kappa: SumOrder,
    /// κ = −(f·d), step 3.
    pub kappa_turned: SumOrder,
    /// r·f, step 3.
    pub dip: SumOrder,
    /// c = −(N·d) of the bending.
    pub cosine: SumOrder,
    /// k's product.
    pub k: KGrouping,
    /// The form of a(c) (decided by shadow-share.jsonl).
    pub law: LawForm,
}

impl RayOrders {
    /// The orders ` ` and `refraction.jsonl` select for each version
    /// (tests/shading_rays_vectors.rs and tests/shading_media_vectors.rs try every candidate).
    pub fn of(version: RenderAs) -> RayOrders {
        use SumOrder::*;
        let law = LawForm::of(version);
        match version {
            RenderAs::Bryce30 => {
                RayOrders { kappa: YzX, kappa_turned: YzX, dip: XyZ, cosine: YzX, k: KGrouping::SquareFirst, law }
            }
            RenderAs::Bryce31 => {
                RayOrders { kappa: XzY, kappa_turned: XyZ, dip: XyZ, cosine: XzY, k: KGrouping::SquareFirst, law }
            }
            RenderAs::Bryce50 => {
                RayOrders { kappa: YzX, kappa_turned: YzX, dip: YzX, cosine: XzY, k: KGrouping::SquareFirst, law }
            }
        }
    }
}


/// The orders no row separates, as reads them in the
/// original's code: g·v (f), v·n (step 1 of both rays) and |v·ν| (the tint) z last as 3.0 and
/// 3.1, x last as 5.0; the angle law's ν·v y last as 3.0, x last as 3.1 and 5.0, and its N·v y
/// last as 3.0 and 3.1, x last as 5.0; T·f in step 3 z last as 3.0 and 3.1, x last as 5.0, and in
/// step 4 (as 5.0) x last.
#[derive(Clone, Copy, Debug)]
struct ReadOrders {
    facing: SumOrder,
    law_reflected: SumOrder,
    law_transmitted: SumOrder,
    bent: SumOrder,
}

impl ReadOrders {
    fn of(version: RenderAs) -> ReadOrders {
        let (z_last, y_last, x_last) = (SumOrder::XyZ, SumOrder::XzY, SumOrder::YzX);
        match version {
            RenderAs::Bryce30 => ReadOrders { facing: z_last, law_reflected: y_last, law_transmitted: y_last, bent: z_last },
            RenderAs::Bryce31 => ReadOrders { facing: z_last, law_reflected: x_last, law_transmitted: y_last, bent: z_last },
            RenderAs::Bryce50 => ReadOrders { facing: x_last, law_reflected: x_last, law_transmitted: x_last, bent: x_last },
        }
    }
}

fn neg(a: Vec3) -> Vec3 {
    [-a[0], -a[1], -a[2]]
}

/// The mirror of d about ν: each axis 2κ·νk + dk, κ = −(ν·d). step 2
fn mirror(d: Vec3, nu: Vec3, order: SumOrder) -> Vec3 {
    let kappa = -order.dot(nu, d);
    let two_kappa = 2.0 * kappa;
    std::array::from_fn(|k| two_kappa * nu[k] + d[k])
}

/// The reflected ray's direction and its normal ν (step 1's, for the tint), or `None`.
fn reflected_direction(hit: &RayHit, v: Vec3, f: Vec3, orders: RayOrders, ro: ReadOrders) -> Option<(Vec3, Vec3)> {
    let turn = hit.switches & switch::TURN_TO_SURFACE != 0;
    let n = hit.shading_normal;
    // Step 1.
    let nu = if ro.facing.dot(v, n) < 0.0 {
        if !turn {
            return None;
        }
        f
    } else {
        n
    };
    // Step 2.
    let d = hit.ray_direction;
    let r = mirror(d, nu, orders.kappa);
    // Step 3.
    if orders.dip.dot(r, f) < 0.0 {
        if !turn {
            return None;
        }
        return Some((mirror(d, f, orders.kappa_turned), nu));
    }
    Some((r, nu))
}

/// The reflection's tint τ with ν as step 1 chose it.
fn tint(version: RenderAs, m: &RayMaterial, v: Vec3, nu: Vec3) -> [f64; 3] {
    let metal = m.metallicity;
    if metal > 0.0 {
        let x = match version {
            RenderAs::Bryce30 => 0.0, // step 4
            RenderAs::Bryce31 | RenderAs::Bryce50 => TINT_POWER_31,
        };
        // : |v·ν| in the facing order; the product grouped
        // as (1 − Dk)·(P(|v·ν|, x)·m)
        let p = pow(ReadOrders::of(version).facing.dot(v, nu).abs(), x);
        std::array::from_fn(|k| f64::from((1.0 - (1.0 - f64::from(m.diffuse[k])) * (p * metal)) as f32))
    } else {
        [1.0; 3]
    }
}

/// T' = ⟨T·w⟩, and whether its mean is above the cut-off.
/// The mean adds T'r last, (T'g + T'b) + T'r; as 5.0 its blue term is not rounded to binary32:
/// Tb·(the blue weight before its rounding), a binary64 product of the singles, while the ray
/// is sent with the rounded T'b.
fn total_weight(version: RenderAs, total: [f32; 3], w: [f32; 3], blue: f64) -> Option<[f32; 3]> {
    let t: [f32; 3] = std::array::from_fn(|k| (f64::from(total[k]) * f64::from(w[k])) as f32);
    let blue_term = if version == RenderAs::Bryce50 { f64::from(total[2]) * blue } else { f64::from(t[2]) };
    let sum = (f64::from(t[1]) + blue_term) + f64::from(t[0]);
    (sum * MEAN_FACTOR > CUT_OFF).then_some(t)
}

/// The bending by Snell's law in Heckbert's form about N: T = r·d + (r·c − R(k))·N, or
/// `None` where k ≥ 0 does not hold (total internal reflection, or k not a number).
fn bend(version: RenderAs, d: Vec3, normal: Vec3, r: f64, orders: RayOrders) -> Option<Vec3> {
    let c = -orders.cosine.dot(normal, d);
    let q = 1.0 - c * c;
    let k = 1.0
        - match orders.k {
            KGrouping::SquareFirst => (r * r) * q,
            KGrouping::OneAtATime => r * (r * q),
        };
    if !(k >= 0.0) {
        return None;
    }
    let m = r * c - sqrt_as(version, k);
    Some(std::array::from_fn(|i| r * d[i] + m * normal[i]))
}

/// What step 3 (and step 4) of the transmitted ray give.
enum Direction {
    /// The ray's direction.
    Along(Vec3),
    /// Bent out of the front without bit 0x20: no transmitted ray.
    None,
    /// As 5.0 with bit 0x400000 below the limit: no transmitted ray; the reflected ray takes
    /// ρ' = ρ + t and the rays after carry the count plus 1.
    TurnedToReflection,
}

/// The transmitted ray's direction.
fn transmitted_direction(
    version: RenderAs,
    hit: &RayHit,
    m: &RayMaterial,
    normal: Vec3,
    f: Vec3,
    orders: RayOrders,
) -> Direction {
    let d = hit.ray_direction;
    let r = hit.media.ratio();
    if hit.switches & switch::REFRACTION == 0 || r == 1.0 || r.is_nan() || m.blended {
        return Direction::Along(d);
    }
    // At a total internal reflection: step 4 as 5.0 with the bit and below the limit, else d.
    let tir = || {
        if version == RenderAs::Bryce50
            && hit.switches & switch::TIR_TO_REFLECTION != 0
            && hit.tir_count < hit.tir_limit
        {
            Direction::TurnedToReflection
        } else {
            Direction::Along(d)
        }
    };
    let Some(t) = bend(version, d, normal, r, orders) else { return tir() };
    if ReadOrders::of(version).bent.dot(t, f) > 0.0 {
        if hit.switches & switch::TRANSMIT_FROM_BEHIND == 0 {
            return Direction::None;
        }
        return match bend(version, d, f, r, orders) {
            Some(t2) => Direction::Along(t2),
            None => tir(),
        };
    }
    Direction::Along(t)
}

/// The further rays a hit sends, in the order they are sent, with the orders the vectors
/// select for `version`.
pub fn send(version: RenderAs, hit: &RayHit, material: &RayMaterial) -> Sent {
    send_ordered(version, RayOrders::of(version), hit, material)
}

/// `send` with the sums' orders given (the vector tests try every combination).
pub fn send_ordered(version: RenderAs, orders: RayOrders, hit: &RayHit, material: &RayMaterial) -> Sent {
    let mut sent = Sent::default();
    if hit.depth >= hit.depth_limit {
        return sent;
    }
    let five = version == RenderAs::Bryce50;
    let d = hit.ray_direction;
    let v = neg(d);
    let g = hit.geometric_normal;
    let ro = ReadOrders::of(version);
    let f = if ro.facing.dot(g, v) < 0.0 { neg(g) } else { g };
    let t = material.transparency;
    let rho = material.reflection;
    // (The angle law; a blended material has none; for a material whose channels
    // have textures, decided by its Transparency's A value, and changing a ray only where t at
    // the hit is above 0)
    let law = if material.blended {
        None
    } else {
        AngleLaw::in_force(version, material.law_transparency, material.refraction, hit.met_before).filter(|_| t > 0.0)
    };
    // `blue`: the blue weight before its rounding to binary32 (as 5.0 the cut-off's blue term).
    let make = |kind, direction, weight: [f32; 3], blue: f64, medium, tir_count| {
        total_weight(version, hit.total, weight, blue).map(|total| FurtherRay {
            kind,
            origin: hit.point,
            direction,
            start: RAY_START,
            weight,
            total,
            depth: hit.depth + 1,
            medium,
            tir_count,
        })
    };
    let far = if hit.encloses_volume { Medium::Beyond } else { Medium::Incoming };

    // The transmitted ray (worked out first: as 5.0 it decides the reflected ray's amount and
    // the counts the rays carry).
    let mut turned_to_reflection = false;
    let mut count_after = hit.tir_count;
    let blended_30 = material.blended && version == RenderAs::Bryce30;
    let transmitted = 'tr: {
        if hit.switches & switch::TRANSMISSION == 0 || !(t > 0.0) || (material.blended && !blended_30) {
            break 'tr None;
        }
        // Step 1.
        let normal = if ro.facing.dot(v, hit.shading_normal) < 0.0 {
            if hit.switches & switch::TRANSMIT_FROM_BEHIND == 0 {
                break 'tr None;
            }
            f
        } else {
            hit.shading_normal
        };
        // Step 6: a blended material as 3.0, η taken as 1 and no angle law, not sent when
        // t = 1 and Θ = (1, 1, 1).
        if blended_30 {
            if t == 1.0 && material.transparent_colour == [1.0; 3] {
                break 'tr None;
            }
            let w = material.transparent_colour.map(|c| (t * f64::from(c)) as f32);
            let blue = t * f64::from(material.transparent_colour[2]);
            break 'tr make(RayKind::Transmitted, d, w, blue, far, hit.tir_count);
        }
        // Step 2.
        let amount = match law {
            Some(l) => l.share(ro.law_transmitted.dot(normal, v), orders.law) * t,
            None => t,
        };
        if !(amount > 0.0) {
            break 'tr None;
        }
        let colour = transmitted_colour(version, material.diffuse, material.transparent_colour, material.refraction);
        let w = colour.map(|c| (amount * f64::from(c)) as f32);
        // Steps 3 and 4.
        let direction = match transmitted_direction(version, hit, material, normal, f, orders) {
            Direction::None => break 'tr None,
            Direction::TurnedToReflection => {
                turned_to_reflection = true;
                count_after = hit.tir_count + 1;
                break 'tr None;
            }
            Direction::Along(dir) => dir,
        };
        if five && ro.bent.dot(direction, f) < 0.0 {
            count_after = 0; // step 4 (as 5.0: the count 0 from this point)
        }
        // Step 5.
        make(RayKind::Transmitted, direction, w, amount * f64::from(colour[2]), far, if five { count_after } else { hit.tir_count })
    };

    // The reflected ray.
    let amount = if turned_to_reflection {
        Some(rho + t) // step 4 (as 5.0 after a total internal reflection)
    } else if law.is_none() {
        Some(rho)
    } else {
        None // with the law: ρ' needs ν, below
    };
    let reflected = 'rf: {
        if hit.switches & switch::REFLECTION == 0 {
            break 'rf None;
        }
        // Without the law ρ' = ρ, and nothing is made where it is not above 0.
        if amount.is_some_and(|a| !(a > 0.0)) {
            break 'rf None;
        }
        let Some((r, nu)) = reflected_direction(hit, v, f, orders, ro) else { break 'rf None };
        let rho_dash = match (amount, law) {
            (Some(a), _) => a,
            // : the original's order,
            // (t − a(ν·v)·t) + ρ, the share lost by the transmitted ray formed first
            (None, Some(l)) => (t - l.share(ro.law_reflected.dot(nu, v), orders.law) * t) + rho,
            (None, None) => rho,
        };
        if !(rho_dash > 0.0) {
            break 'rf None;
        }
        let tau = tint(version, material, v, nu);
        let w = tau.map(|tk| (rho_dash * tk) as f32);
        make(RayKind::Reflected, r, w, rho_dash * tau[2], Medium::Incoming, if five { count_after } else { hit.tir_count })
    };

    // As 3.0 and 3.1 the reflected ray is sent first, as 5.0 the transmitted ray.
    let order = if five { [transmitted, reflected] } else { [reflected, transmitted] };
    for r in order.into_iter().flatten() {
        sent.push(r);
    }
    sent
}

/// How a ray that goes straight on through a surface replaces the surface's colour with what
/// it brings back, after the air (its weight is not used). (The blended
/// material),
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Replace {
    /// The blended material's ray: by the share t (`media::blend`).
    Blend(f32),
    /// The blended ray's share as a double (a Light-mode surface's V is not a single).
    Share(f64),
    /// A fuzzy surface's continuing ray: b + F·(o − b) (`fuzzy::mix`).
    Fuzzy(f64),
    /// A fuzzy surface met from inside its solid: the continuing ray's colour itself.
    Through,
}

/// What a sent ray brought back: its colour C and the weight W it came back with; for a ray
/// that goes straight on through a blended or fuzzy surface, how its colour replaces the
/// surface's.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BroughtBack {
    pub colour: [f32; 3],
    pub weight: [f32; 3],
    /// `Some` for the blended ray and a fuzzy surface's continuing ray.
    pub replace: Option<Replace>,
}

impl BroughtBack {
    /// A reflected or transmitted ray's colour and weight.
    pub fn further(colour: [f32; 3], weight: [f32; 3]) -> BroughtBack {
        BroughtBack { colour, weight, replace: None }
    }
}

/// One sent ray's colour added to the surface's: or = ⟨Cr·Wr + or⟩, and for green and blue
/// ok = ⟨⟨Wk·Ck⟩ + ok⟩; the blended ray's colour replaces it by the share t (The
/// blended material: `media::blend`), a fuzzy surface's continuing ray by F .
pub fn join_one(o: [f32; 3], b: &BroughtBack) -> [f32; 3] {
    match b.replace {
        Some(Replace::Blend(t)) => return super::media::blend(o, b.colour, t),
        Some(Replace::Share(t)) => return super::media::blend_share(o, b.colour, t),
        Some(Replace::Fuzzy(f)) => return super::fuzzy::mix(o, b.colour, f),
        Some(Replace::Through) => return b.colour,
        None => {}
    }
    let (c, w) = (b.colour.map(f64::from), b.weight.map(f64::from));
    let red = (c[0] * w[0] + f64::from(o[0])) as f32;
    let other = |k: usize| (f64::from((w[k] * c[k]) as f32) + f64::from(o[k])) as f32;
    [red, other(1), other(2)]
}

/// The surface's colour: the local colour o with what each sent ray brought back, in the
/// order the rays were sent.
pub fn join(local: [f32; 3], brought_back: &[BroughtBack]) -> [f32; 3] {
    brought_back.iter().fold(local, join_one)
}
