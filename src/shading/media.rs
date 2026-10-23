// SPDX-License-Identifier: MIT
//! The laws of and that do not send
//! rays: the angle law, the transmitted colour's law, the medium's factor and the weight a
//! ray in a solid's medium comes back with, a surface's share of a shadow ray, the blended
//! material's last step and its shadow colour, and Bryce 5's blurred cone.
//!
//! (The angle law; The transmitted ray, step 2; What a ray brings back joins
//! the colour; The blended material; The blurred rays; Last bits), (Outputs: the
//! share, the media; Last bits),,
//! (Media, refraction, blend and blur: media.rs)
//!
//! Pure functions of numbers. Every binary64 operation is one Rust operation; ⟨x⟩ is
//! `x as f32`. The arrangements the text leaves to the vectors are named enums with a
//! per-version `of`, and the vector tests try every candidate.

use crate::geom::Vec3;
use crate::numerics::{exp2_as, pow, rsqrt_as, sqrt_as, x87};
use crate::version::RenderAs;

use super::lighting::SumOrder;

/// K10: the double whose bits are 402cda993e7663be (about 10 log2 e).
pub const K10: f64 = f64::from_bits(0x402c_da99_3e76_63be);
/// The factor of |c| in the angle law: the double nearest 0.98.
pub const LAW_COSINE_FACTOR: f64 = 0.98;
/// E: the double whose bits are 3fe99999a0000000 (the single nearest 0.8).
pub const LAW_EXPONENT: f64 = f64::from_bits(0x3fe9_9999_a000_0000);
/// K3: the double whose bits are 40114ff58be0a23e (about 3 log2 e).
pub const K3: f64 = f64::from_bits(0x4011_4ff5_8be0_a23e);
/// K2: the double nearest 2 log2 e (doubling log2 e is exact).
pub const K2: f64 = 2.0 * std::f64::consts::LOG2_E;
/// The medium's fall with distance: the double nearest 0.02.
pub const MEDIUM_FALL: f64 = 0.02;
/// The exponent of |c| in the shadow share's second factor.
pub const SHARE_EXPONENT: f64 = 4.0;
/// B: the double whose bits are 3fe0c152382d7365, the binary64 quotient of π's double by 6
/// (one unit below the double nearest π/6). (The blurred rays, revised at log
/// entry 3285, answering)
pub const BLUR_ANGLE_BASE: f64 = f64::from_bits(0x3fe0_c152_382d_7365);
/// The distance at which the blurred ray's disc stands.
pub const BLUR_DISTANCE: f64 = 100.0;
/// The factor 12 of the cone's exponent 12 − 12·μ.
pub const BLUR_EXPONENT: f64 = 12.0;
/// Where h is below this the turn takes its own form: the double nearest 1e-10.
pub const BLUR_SMALL_H: f64 = 1e-10;
// M of the means (the double nearest 0.333333).
use super::further::MEAN_FACTOR;

/// The usual forms of a(c) = 1 − e + e·P (the text: "of the usual forms of this law, one
/// alone gives every case, in each version"; ` ` decides).
/// 1 + e·(P − 1) is not listed: it is 1 − e·(1 − P) to the bit.
/// (Media: the forms tried)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LawForm {
    /// (1 − e) + e·P
    OneLessEPlusEp,
    /// 1 − e·(1 − P)
    OneLessETimesOneLessP,
    /// (1 + e·P) − e
    OnePlusEpLessE,
    /// 1 − (e − e·P)
    OneLessELessEp,
}

impl LawForm {
    pub const ALL: [LawForm; 4] = [
        LawForm::OneLessEPlusEp,
        LawForm::OneLessETimesOneLessP,
        LawForm::OnePlusEpLessE,
        LawForm::OneLessELessEp,
    ];

    /// The form ` ` selects (tests/shading_media_vectors.rs
    /// tries all five per version).
    pub fn of(_version: RenderAs) -> LawForm {
        LawForm::OneLessETimesOneLessP
    }

    fn eval(self, e: f64, p: f64) -> f64 {
        match self {
            LawForm::OneLessEPlusEp => (1.0 - e) + e * p,
            LawForm::OneLessETimesOneLessP => 1.0 - e * (1.0 - p),
            LawForm::OnePlusEpLessE => (1.0 + e * p) - e,
            LawForm::OneLessELessEp => 1.0 - (e - e * p),
        }
    }
}

/// A material's angle law, where it has one: e = 1 − 2^(−(η − 1)·K10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngleLaw {
    pub e: f64,
}

impl AngleLaw {
    /// The law of a plain material with Transparency `t` and refraction value `eta`, or
    /// `None` (t ≤ 0 or η ≤ 1, or a number that is none); for a material whose channels have
    /// textures `t` is the A value of its Transparency. (The angle law; Materials
    /// whose channels have textures)
    pub fn of(version: RenderAs, transparency: f64, eta: f32) -> Option<AngleLaw> {
        let (t, eta) = (transparency, f64::from(eta));
        if !(t > 0.0 && eta > 1.0) {
            return None;
        }
        Some(AngleLaw { e: 1.0 - exp2_as(version, -(eta - 1.0) * K10) })
    }

    /// The law in force at a surface: only once the material has been met.
    pub fn in_force(version: RenderAs, transparency: f64, eta: f32, met_before: bool) -> Option<AngleLaw> {
        if met_before { AngleLaw::of(version, transparency, eta) } else { None }
    }

    /// a(c) = 1 − e + e·P(0.98·|c|, E) in the form given.
    pub fn share(self, c: f64, form: LawForm) -> f64 {
        let p = pow(LAW_COSINE_FACTOR * c.abs(), LAW_EXPONENT);
        form.eval(self.e, p)
    }
}

/// The transmitted colour Θ'k = ⟨⟨1 + ⟨φ·⟨Dk − 1⟩⟩⟩·Θk⟩ with φ = ⟨1 − 2^(−max(0, η − 1)·K3)⟩.
pub fn transmitted_colour(version: RenderAs, diffuse: [f32; 3], transparent: [f32; 3], eta: f32) -> [f32; 3] {
    let over = (f64::from(eta) - 1.0).max(0.0);
    let phi = f64::from((1.0 - exp2_as(version, -over * K3)) as f32);
    std::array::from_fn(|k| {
        let dk = f64::from((f64::from(diffuse[k]) - 1.0) as f32);
        let lift = f64::from((phi * dk) as f32);
        let one = f64::from((1.0 + lift) as f32);
        (one * f64::from(transparent[k])) as f32
    })
}

/// The medium's factor F for a ray in a solid of refraction value η: 1 where η ≤ 1; else,
/// with u = 1 − 2^(−(η − 1)·K2), max(1 − 0.02·u·s, 1 − u) for a hit at the parameter s and
/// 1 − u for a ray that meets nothing.
pub fn medium_factor(version: RenderAs, eta: f32, hit_t: Option<f64>) -> f64 {
    let eta = f64::from(eta);
    if !(eta > 1.0) {
        return 1.0;
    }
    let u = 1.0 - exp2_as(version, -(eta - 1.0) * K2);
    let floor = 1.0 - u;
    match hit_t {
        // : the original's grouping (0.02·u)·s
        Some(s) => (1.0 - MEDIUM_FALL * u * s).max(floor),
        None => floor,
    }
}

/// A medium as the weights see it: its volume colour Φ (white for a blended material's), its
/// refraction value η, and whether the light it lets through falls with the distance in it
/// ('s factor F; a fuzzy solid's F is 1 whatever η is). (What a ray brings
/// back),
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediumLaw {
    pub volume_colour: [f32; 3],
    pub refraction: f32,
    /// false: F = 1 whatever η is (a fuzzy solid's medium).
    pub fades: bool,
}

impl MediumLaw {
    /// The medium of a solid: a blended material's Φ counts as (1, 1, 1) and its η counts.
    pub fn of_solid(volume_colour: [f32; 3], refraction: f32, blended: bool) -> MediumLaw {
        MediumLaw { volume_colour: if blended { [1.0; 3] } else { volume_colour }, refraction, fades: true }
    }

    /// The medium of a fuzzy solid: Φ and η its own as a plain material's (Φ white where the
    /// material is also blended), F = 1.
    pub fn of_fuzzy_solid(volume_colour: [f32; 3], refraction: f32, blended: bool) -> MediumLaw {
        MediumLaw { fades: false, ..MediumLaw::of_solid(volume_colour, refraction, blended) }
    }

    /// The medium's factor F for a ray in this medium whose hit lies at the parameter `hit_t`,
    /// or that meets nothing (`None`)., (The
    /// medium of a fuzzy solid: F = 1)
    pub fn factor(&self, version: RenderAs, hit_t: Option<f64>) -> f64 {
        if self.fades { medium_factor(version, self.refraction, hit_t) } else { 1.0 }
    }

    /// W = ⟨⟨F·wk⟩·Φk⟩: the weight a ray in this medium comes back with.
    pub fn weight_back(&self, version: RenderAs, w: [f32; 3], hit_t: Option<f64>) -> [f32; 3] {
        let f = self.factor(version, hit_t);
        std::array::from_fn(|k| {
            let fw = f64::from((f * f64::from(w[k])) as f32);
            (fw * f64::from(self.volume_colour[k])) as f32
        })
    }
}

/// How the share's product (1 + e·P(|c|, 4))·a(c)·t is grouped (Last bits: the
/// vectors decide). Written with A = 1 + e·P(|c|, 4), B = a(c), t.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShareGrouping {
    /// (A·B)·t
    AbT,
    /// A·(B·t)
    ABt,
    /// (A·t)·B
    AtB,
}

impl ShareGrouping {
    pub const ALL: [ShareGrouping; 3] = [ShareGrouping::AbT, ShareGrouping::ABt, ShareGrouping::AtB];
}

/// The arrangements of a surface's share of a shadow ray that the vectors decide: the orders
/// of the two cosines (that under P(|c|, 4) and that of a(c)), the product's grouping and the
/// form of a(c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShareOrders {
    /// c under P(|c|, 4).
    pub second: SumOrder,
    /// c of a(c).
    pub law: SumOrder,
    pub grouping: ShareGrouping,
    pub form: LawForm,
}

impl ShareOrders {
    /// The arrangements ` ` selects (tests/shading_media_vectors.rs
    /// tries every combination per version).
    pub fn of(version: RenderAs) -> ShareOrders {
        use SumOrder::*;
        let (second, law) = match version {
            RenderAs::Bryce30 => (YzX, XyZ),
            RenderAs::Bryce31 | RenderAs::Bryce50 => (YzX, YzX),
        };
        ShareOrders { second, law, grouping: ShareGrouping::ABt, form: LawForm::of(version) }
    }
}

/// A surface's share t' of a shadow ray: t, or with the angle law in force
/// (1 + e·P(|c|, 4))·a(c)·t, c the cosine between the shadow ray's direction and the shading
/// normal of the surface met.
pub fn shadow_share(law: Option<AngleLaw>, transparency: f64, direction: Vec3, normal: Vec3, orders: ShareOrders) -> f64 {
    let t = transparency;
    let Some(law) = law else { return t };
    let c2 = orders.second.dot(direction, normal);
    let a_big = 1.0 + law.e * pow(c2.abs(), SHARE_EXPONENT);
    let b = law.share(orders.law.dot(direction, normal), orders.form);
    match orders.grouping {
        ShareGrouping::AbT => (a_big * b) * t,
        ShareGrouping::ABt => a_big * (b * t),
        ShareGrouping::AtB => (a_big * t) * b,
    }
}

/// A blended material's colour toward a shadow ray: ⟨⟨⟨1 − Θk⟩·α⟩ + Θk⟩, α the share's
/// amount rounded to a single.
pub fn blended_shadow_colour(transparent: [f32; 3], amount: f64) -> [f32; 3] {
    let alpha = f64::from(amount as f32);
    std::array::from_fn(|k| {
        let theta = f64::from(transparent[k]);
        let less = f64::from((1.0 - theta) as f32);
        let scaled = f64::from((less * alpha) as f32);
        (scaled + theta) as f32
    })
}

/// The blended material's last step: what the ray behind brings back, c, replaces o by the
/// share t: or = or + ⟨t·(cr − or)⟩ (binary32 sum), og = ⟨t·⟨cg − og⟩⟩ + og (binary32), and
/// ob = ⟨t·⟨cb − ob⟩ + ob⟩.
pub fn blend(colour: [f32; 3], behind: [f32; 3], transparency: f32) -> [f32; 3] {
    blend_share(colour, behind, f64::from(transparency))
}

/// `blend` with the share t given as a double (the formulas take t in binary64; a vector of
/// 's further rays gives shares that are not singles). (The blended
/// material),
pub fn blend_share(colour: [f32; 3], behind: [f32; 3], t: f64) -> [f32; 3] {
    let (o, c) = (colour.map(f64::from), behind.map(f64::from));
    let red = colour[0] + (t * (c[0] - o[0])) as f32;
    let green = (t * f64::from((c[1] - o[1]) as f32)) as f32 + colour[1];
    let blue = (t * f64::from((c[2] - o[2]) as f32) + o[2]) as f32;
    [red, green, blue]
}

/// The blurred cone's radius ρb = 100·tan(P(B, 12 − 12·μ)), μ = (Hr + Hg + Hb)·M, the
/// tangent FPTAN's and the product by 100 rounded once.
pub fn blur_radius(halo: [f32; 3]) -> f64 {
    let h = halo.map(f64::from);
    let mu = ((h[1] + h[2]) + h[0]) * MEAN_FACTOR;
    let angle = pow(BLUR_ANGLE_BASE, BLUR_EXPONENT - BLUR_EXPONENT * mu);
    x87::tan_ext(angle).mul_to_f64(BLUR_DISTANCE)
}

/// How the frame's term α'·e1k is formed for the axes y and z, e1k = −δx·δk/h (blur.jsonl
/// decides). (Media: the blurred turn)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameTerm {
    /// −(((α'·δx)·δk)/h)
    LeftToRight,
    /// −((α'·(δx·δk))/h)
    DirectionsFirst,
    /// −(α'·((δx·δk)/h))
    FrameFirst,
    /// −((α'·δx)·(δk/h))
    SplitQuotient,
    /// −(((α'·δk)·δx)/h): the coordinate's own component first ('s ambient ray).
    OwnAxisFirst,
}

impl FrameTerm {
    pub const ALL: [FrameTerm; 5] = [
        FrameTerm::LeftToRight,
        FrameTerm::DirectionsFirst,
        FrameTerm::FrameFirst,
        FrameTerm::SplitQuotient,
        FrameTerm::OwnAxisFirst,
    ];
}

/// How β'·e2k is formed, e2 = (0, δz/h, −δy/h).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HaloTerm {
    /// (β'·δj)/h
    ProductFirst,
    /// β'·(δj/h)
    QuotientFirst,
}

impl HaloTerm {
    pub const ALL: [HaloTerm; 2] = [HaloTerm::ProductFirst, HaloTerm::QuotientFirst];
}

/// The order of each axis's sum P + A + B, P = γ·δk, A = α'·e1k, B = β'·e2k.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameSum {
    /// (P + A) + B
    InOrder,
    /// P + (A + B)
    FrameFirst,
    /// (P + B) + A
    SecondFirst,
}

impl FrameSum {
    pub const ALL: [FrameSum; 3] = [FrameSum::InOrder, FrameSum::FrameFirst, FrameSum::SecondFirst];
}

/// The arrangements of the blurred turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlurOrders {
    /// The order of α² + β² + 100² (x = α², y = β², z = 100²).
    pub norm: SumOrder,
    pub term: FrameTerm,
    pub halo: HaloTerm,
    pub sum: FrameSum,
}

impl BlurOrders {
    /// The arrangement blur.jsonl selects (tests/shading_media_vectors.rs tries all 90; the
    /// fifth α-term form came with's ambient ray and changes nothing here).
    pub const BRYCE50: BlurOrders = BlurOrders {
        norm: SumOrder::XyZ,
        term: FrameTerm::DirectionsFirst,
        halo: HaloTerm::ProductFirst,
        sum: FrameSum::FrameFirst,
    };
}

/// The blurred ray's direction δ turned toward the point (α, β) of the disc that stands at
/// the distance 100 along it.
pub fn blur_turn(delta: Vec3, alpha: f32, beta: f32, orders: BlurOrders) -> Vec3 {
    let version = RenderAs::Bryce50;
    let (a, b) = (f64::from(alpha), f64::from(beta));
    let sq = [a * a, b * b, BLUR_DISTANCE * BLUR_DISTANCE];
    let sum = match orders.norm {
        SumOrder::XyZ => (sq[0] + sq[1]) + sq[2],
        SumOrder::XzY => (sq[0] + sq[2]) + sq[1],
        SumOrder::YzX => (sq[1] + sq[2]) + sq[0],
    };
    let s = rsqrt_as(version, sum);
    frame_turn(delta, [s * a, s * b, s * BLUR_DISTANCE], orders)
}

/// The turn T(m; α, β, γ) about the direction m: γ along m, α along e1 = (h, −my·mx/h,
/// −mz·mx/h) and β along e2 = (0, mz/h, −my/h), h = R(mz·mz + my·my); where |h| < 1e-10,
/// (γ, β, α) with γ negated where mx < 0. The arrangement of the second and third
/// coordinates' terms and sums is `orders`' `term`, `halo` and `sum` (its `norm` is not
/// read here). point 3,
pub fn frame_turn(delta: Vec3, abg: [f64; 3], orders: BlurOrders) -> Vec3 {
    let version = RenderAs::Bryce50;
    let [a1, b1, gamma] = abg;
    let [dx, dy, dz] = delta;
    // point 3 (h = R(mz·mz + my·my): the order of the two squares is the entry's)
    let h = sqrt_as(version, dz * dz + dy * dy);
    if h.abs() < BLUR_SMALL_H {
        let g = if dx < 0.0 { -gamma } else { gamma };
        return [g, b1, a1];
    }
    let term = |dk: f64| {
        -match orders.term {
            FrameTerm::LeftToRight => ((a1 * dx) * dk) / h,
            FrameTerm::DirectionsFirst => (a1 * (dx * dk)) / h,
            FrameTerm::FrameFirst => a1 * ((dx * dk) / h),
            FrameTerm::SplitQuotient => (a1 * dx) * (dk / h),
            FrameTerm::OwnAxisFirst => ((a1 * dk) * dx) / h,
        }
    };
    let halo = |dj: f64| match orders.halo {
        HaloTerm::ProductFirst => (b1 * dj) / h,
        HaloTerm::QuotientFirst => b1 * (dj / h),
    };
    // e1 = (h, −δx·δy/h, −δx·δz/h), e2 = (0, δz/h, −δy/h).
    let a_terms = [a1 * h, term(dy), term(dz)];
    let b_terms = [b1 * 0.0, halo(dz), -halo(dy)];
    std::array::from_fn(|k| {
        let (p, r, t) = (gamma * delta[k], a_terms[k], b_terms[k]);
        match orders.sum {
            FrameSum::InOrder => (p + r) + t,
            FrameSum::FrameFirst => p + (r + t),
            FrameSum::SecondFirst => (p + t) + r,
        }
    })
}

