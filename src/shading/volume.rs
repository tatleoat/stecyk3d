// SPDX-License-Identifier: MIT
//! Volume materials: the march through an object of a volume material, the laying of what
//! it gathers over what lies behind, and the medium colour Φ' that changes with distance and
//! height.
//!
//! (revised at: Volume
//! materials, The stretch, The samples, The step, A sample's material, The density, The depth
//! inside, A sample's opacity, Laying the samples, A sample's colour, The order of the
//! evaluations, Where marches are made (the laying), The medium colour, Last bits)
//! (Volume materials )
//!
//! Every binary64 operation is one Rust `f64` operation in the order the text writes it;
//! ⟨x⟩ is `x as f32`. P is's power (`numerics::pow`), R and Q's root and
//! reciprocal root for the version, 2^x and log2's.

use crate::geom::Vec3;
use crate::numerics::{exp2_as, log2_as, pow, rsqrt_as, sqrt_as};
use crate::version::RenderAs;

use super::lighting::{LightPlace, falloff, LightSource, PlainMaterial, Shadow, SumOrder, light_direction_ordered, light_switch};
use super::open::OpenItem;
use crate::sky::lit_air::lift_by_shadow;

/// The volume bits of a material's flag word.
pub mod bits {
    /// The flat kind.
    pub const FLAT: u32 = 0x100;
    /// The shaded kind.
    pub const SHADED: u32 = 0x200;
    /// The lit kind.
    pub const LIT: u32 = 0x400;
    /// The march is added to what lies behind (the Additive modifier).
    pub const ADDS: u32 = 0x2000;
    /// A shadow ray goes through the medium as through no medium (Cast Shadows).
    pub const NO_SHADOW_MEDIUM: u32 = 0x80;
    /// The medium colour is white.
    pub const WHITE_MEDIUM: u32 = 0x8;
    /// The medium colour comes in with the distance (Volume Blend-Distance).
    pub const BLEND_DISTANCE: u32 = 0x4000;
    /// The medium colour by height (Volume Blend-Altitude).
    pub const BLEND_ALTITUDE: u32 = 0x8000;
}

/// What leaves Open, as refusals.
pub mod open {
    use crate::shading::OpenItem;

    const fn item(subject: &'static str) -> OpenItem {
        OpenItem { entry: "SH-2.4", subject, question: "" }
    }

    pub const SKY_INTEGRATION: OpenItem =
        item("the wash (SK-1.4, bit 0x1000000) over the samples of a Sky Integration march (bit 0x2000000) as 5.0: read in the code, not pictured (SH-2.4a)");
    pub const JITTER_WITHOUT_GENERATOR: OpenItem =
        item("jittered sample places where the render's generator is not at hand (the light in the air)");
    pub const KINDS: OpenItem = item("a material with two or three of the volume bits 0x100, 0x200 and 0x400");
    pub const SHAPE: OpenItem =
        item("a volume material on an object whose depth inside is Open (a light's own object as 5.0, a group's own, a camera or path)");
    /// states the shaded kind's ambient ray (true ambience at a sample); this program
    /// traces a ray's tree in stages and cannot yet trace one from inside a march.
    /// (Revision of)
    pub const SHADED_AMBIENT: OpenItem = OpenItem {
        entry: "SH-2.4a",
        subject: "the ambient ray of true ambience at a shaded volume sample (stated; not built: a ray traced from inside a march)",
        question: "",
    };
    pub const GRADIENT: OpenItem = item("the shaded kind's normal where the density's gradient is (0, 0, 0)");
    pub const ALTITUDE_TORUS: OpenItem = item("the medium colour of bit 0x8000 in a torus (its extent in y)");
}

/// The three kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VolumeKind {
    /// Flat Shading: each sample the material's own colour.
    Flat,
    /// Full Shading: each sample lit as a surface whose normal is the density's gradient.
    Shaded,
    /// Light Sensitive: each sample's colour times the light that reaches it.
    Lit,
}

impl VolumeKind {
    /// The kind of a flag word that has one volume bit: `None` for a material with no volume
    /// bit, the Open item for two or three (the render marches them in turn: `kinds`).
    pub fn of(flags: u32) -> Option<Result<VolumeKind, OpenItem>> {
        match flags & (bits::FLAT | bits::SHADED | bits::LIT) {
            0 => None,
            bits::FLAT => Some(Ok(VolumeKind::Flat)),
            bits::SHADED => Some(Ok(VolumeKind::Shaded)),
            bits::LIT => Some(Ok(VolumeKind::Lit)),
            _ => Some(Err(open::KINDS)),
        }
    }

    /// The kinds a flag word has, in the order they are marched: flat, shaded, lit; none for a
    /// material with no volume bit. (Volume materials: "Each kind whose bit is set
    /// is marched in turn, flat, shaded, lit")
    pub fn kinds(flags: u32) -> Vec<VolumeKind> {
        [(bits::FLAT, VolumeKind::Flat), (bits::SHADED, VolumeKind::Shaded), (bits::LIT, VolumeKind::Lit)]
            .into_iter()
            .filter(|&(bit, _)| flags & bit != 0)
            .map(|(_, k)| k)
            .collect()
    }

    /// k of a sample's opacity.
    fn opacity_factor(self) -> f64 {
        match self {
            VolumeKind::Flat => 20.0,
            VolumeKind::Shaded => 40.0,
            VolumeKind::Lit => 10.0,
        }
    }
}

/// σ, φ and q (offsets 624, 628, 632, singles widened).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeNumbers {
    /// σ, the edge softness.
    pub sigma: f64,
    /// φ, the fuzzy factor.
    pub phi: f64,
    /// q, the quality.
    pub quality: f64,
}

/// How the depth inside is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DepthShape {
    /// 1 − R(p·p): the sphere (and the radial light's own object).
    Sphere,
    /// 1 everywhere: the cylinder, the torus (and the other lights' own objects) as 3.0 and 3.1;
    /// the cube, the cone and the pyramid in every version.
    Whole,
    /// 1 - |p_y|, held to 0 where that is below 0 and to 1 where it is above 1: the slab.
    Slab,
    /// As 5.0: 1 - max(rho, |y|), rho the distance from the axis.
    Cylinder,
    /// As 5.0: 1 - d/t, d the distance from the circle the tube follows.
    Torus { tube: f64, ring: f64 },
    /// As 5.0: 1 at the middle, otherwise 1 - 2m with m the largest of |x|, |y|, |z| (ties as the
    /// text reads).
    Cube,
    /// As 5.0: the smaller of (y + 1)*1.618033991 and ((1 - y) - 2*rho)*0.723606798.
    Cone,
    /// As 5.0: the smaller of (y + 0.5)*3.236068086 and (0.5 - (2m + y))*1.447213644.
    Pyramid,
    /// As 5.0, the object standing for every set of metaballs and a tree's wood: the sum of the
    /// fields of the set of the crossing last found or kept, 0 where there is none; the site
    /// answers it.,
    Field,
}

/// How a product of three factors a·b·c is grouped (Last bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grouping3 {
    /// (a·b)·c
    AbC,
    /// a·(b·c)
    ABc,
    /// (a·c)·b
    AcB,
}

impl Grouping3 {
    pub const ALL: [Grouping3; 3] = [Grouping3::AbC, Grouping3::ABc, Grouping3::AcB];

    pub fn product(self, a: f64, b: f64, c: f64) -> f64 {
        match self {
            Grouping3::AbC => (a * b) * c,
            Grouping3::ABc => a * (b * c),
            Grouping3::AcB => (a * c) * b,
        }
    }
}

/// The arrangements leaves to ` ` (Last bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VolumeOrders {
    /// The stretch's w·w.
    pub stretch: SumOrder,
    /// The sphere's p·p.
    pub depth: SumOrder,
    /// log2(q)·M·K, in that order of the factors.
    pub step: Grouping3,
    /// (E + 5)·δ·L, in that order of the factors.
    pub density: Grouping3,
    /// The lit kind's distance to a light, the square of the difference.
    pub light_distance: SumOrder,
    /// The stretch's w.w for the lit kind. (README-volume-five.md: the squares of x
    /// and z first, decided by volume-five-jitter-lit.jsonl)
    pub lit_stretch: SumOrder,
}

impl VolumeOrders {
    /// The arrangement `volume.jsonl` selects for each version (tests/shading_volume.rs
    /// tries all 81: of them only this one gives every row, in each version).
    /// (Volume materials)
    pub fn of(version: RenderAs) -> VolumeOrders {
        VolumeOrders {
            stretch: SumOrder::YzX,
            depth: SumOrder::YzX,
            step: Grouping3::AbC,
            density: Grouping3::AbC,
            light_distance: SumOrder::YzX,
            lit_stretch: if version == RenderAs::Bryce50 { SumOrder::XzY } else { SumOrder::YzX }, // (as 5.0 the squares of x and z first)
        }
    }

    /// Every arrangement, for the vector test.
    pub fn all() -> Vec<VolumeOrders> {
        let mut v = Vec::with_capacity(729);
        for stretch in SumOrder::ALL {
            for depth in SumOrder::ALL {
                for step in Grouping3::ALL {
                    for density in Grouping3::ALL {
                        for light_distance in SumOrder::ALL {
                            for lit_stretch in SumOrder::ALL {
                                v.push(VolumeOrders { stretch, depth, step, density, light_distance, lit_stretch });
                            }
                        }
                    }
                }
            }
        }
        v
    }
}

/// The orders of g·g and n'·n' of the shaded kind's normal, which no row separates: as 3.0 and
/// 3.1 x last, the original's as reads them; as 5.0 left
/// open by, the same taken.
const NORMAL_ORDER: SumOrder = SumOrder::YzX;

/// L, the double nearest log2 e.
const L: f64 = std::f64::consts::LOG2_E;
/// M, the double nearest log10 2.
const M: f64 = std::f64::consts::LOG10_2;
/// K, the double nearest 8·log2 e.
const K: f64 = 11.541560327111707;
/// The step's base and span: h = 0.001 + 0.999·2^(...).
const STEP_BASE: f64 = 0.001;
const STEP_SPAN: f64 = 0.999;
/// The window around q = 0 where log10 q counts as −10.
const Q_WINDOW: f64 = 1e-10;
const LOG10_FLOOR: f64 = -10.0;
/// Where a stretch without an end ends: o + 1,000,000·d.
pub const FAR_END: f64 = 1_000_000.0;
/// Samples lie below 200 units from the stretch's start.
const SAMPLE_REACH: f64 = 200.0;
/// The density fades from 100 units: ρ1·(1 − (s − 100)·0.01).
const FADE_FROM: f64 = 100.0;
const FADE_RATE: f64 = 0.01;
/// E = 2^((1 − σ)·6·L); the edge term's 5.
const EDGE_SIX: f64 = 6.0;
const EDGE_FIVE: f64 = 5.0;
/// The march ends where α is above 0.999.
const OPAQUE_ENOUGH: f64 = 0.999;
/// The shaded kind's gradient offset, the double nearest 0.001.
const GRADIENT_STEP: f64 = 0.001;
/// As 5.0 a gradient whose parts all lie strictly within this is N itself.
const GRADIENT_TINY: f64 = 1e-10;
/// The lit kind's least intensity, the double nearest 0.0001.
const LEAST_INTENSITY: f64 = 0.0001;
/// κ for the sun and for every other light.
/// E, the double nearest 1/32,767: a draw r is r*E.
const JITTER_UNIT: f64 = 1.0 / 32767.0;
const KAPPA_SUN: f64 = 0.02;
const KAPPA_OTHER: f64 = 0.09;

/// A sample's material:'s channels at the sample (D, A, kd, ka and the rest as
/// reads them) and v, the Volume density channel's value. (A sample's
/// material)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleChannels {
    pub light: PlainMaterial,
    /// v.
    pub density_value: f64,
}

/// The scene as a march sees it.
pub trait MarchSite {
    /// A world point carried into the object's own space by the inverse of its matrix .
    fn to_own(&self, p: Vec3) -> Vec3;
    /// A point of the object's own space carried into the world by its matrix .
    fn to_world(&self, x: Vec3) -> Vec3;
    /// A vector carried to the world as a normal (the transpose of the inverse), not
    /// brought to unit length.
    fn normal_to_world(&self, n: Vec3) -> Vec3;
    /// The material's channels at a point (`world` X, `own` x) of distance `s` (its textures
    /// evaluated there).
    fn channels(&mut self, world: Vec3, own: Vec3, s: f64) -> Result<SampleChannels, OpenItem>;
    /// The shaded kind: the local colour of at `world` for the ray marched,
    /// both normals `normal`, the channels with Transparency 0, s0 black, its shadows cast as
    /// says.
    fn shade(&mut self, world: Vec3, normal: Vec3, channels: &SampleChannels) -> Result<[f32; 3], OpenItem>;
    /// The lights in's order.
    fn light_count(&self) -> usize;
    /// Light `i` as gives it (its colour before a gel).
    fn light(&self, i: usize) -> LightSource;
    /// Light `i`'s beam factor at `world` (The lights with a beam): `Some(1.0)` for a light
    /// with no beam, `None` outside the beam.
    fn beam_factor_at(&self, _i: usize, _world: Vec3) -> Result<Option<f64>, OpenItem> {
        Ok(Some(1.0))
    }
    /// Light `i`'s colour at `world` (a gel's evaluated there).
    fn light_colour_at(&mut self, i: usize, world: Vec3) -> Result<[f32; 3], OpenItem>;
    /// The scene's shadow setting s, as the render takes it.
    fn shadow_setting(&self) -> f64 {
        1.0
    }
    /// Light `i`'s range (read with switch bit 0x20000000, as 5.0).
    fn light_range(&self, _i: usize) -> f64 {
        0.0
    }
    /// Light `i`'s shadow share η and shadow colour ψ (Bryce 5's light in shadow).,
    fn light_shadow_share(&self, _i: usize) -> (f64, [f32; 3]) {
        (0.0, [0.0; 3])
    }
    /// The depth inside of the object standing for every set at `world`: the sum of the fields
    /// of the set last found or kept (0 where none).
    fn field_depth(&self, _world: Vec3) -> f64 {
        0.0
    }
    /// The place of the sun's object, the translation of its matrix. (the lit kind,
    /// step 1: a sun whose switch word lacks bit 0x400 has a place for the falloff)
    fn sun_place(&self) -> Vec3 {
        [0.0; 3]
    }
    /// The distance the textures are given at the sample whose world place is `world`, its
    /// distance along the stretch being `s`: as 3.0 and 3.1 `s`; as 5.0 the world distance from
    /// the ray's origin (the site's), its squares summed in the order of `kind`'s march.
    fn texture_distance(&self, _kind: VolumeKind, _world: Vec3, s: f64) -> f64 {
        s
    }
    /// The next draw r (0 to 32,767) of the render's generator, for a jittered sample's place.
    fn draw(&mut self) -> Result<u32, OpenItem> {
        panic!("a jittered march needs the render's generator")
    }
    /// 's shadow toward light `i` from `world` itself, counting surfaces from 0.0001, in
    /// the medium of the ray marched.
    fn shadow(&mut self, i: usize, world: Vec3, direction: Vec3, reach: f64) -> Result<Shadow, OpenItem>;
    /// Whether the march's samples take the sky's air (Sky Integration, bit 0x2000000, as 5.0,
    /// for a ray that is not a shadow ray).
    fn sky_integrated(&self) -> bool {
        false
    }
    /// A sample's colour after the sky's air has been laid over it: the sample at `world`, its
    /// colour as formed (before W).
    fn sky_over(&mut self, _world: Vec3, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        Ok(colour)
    }
}

/// A march's stretch in the world, with what decides its step and its colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarchRay {
    /// A, where the ray starts counting surfaces.
    pub a: Vec3,
    /// B, the point met, or o + λ1·d, or o + 1,000,000·d.
    pub b: Vec3,
    /// The ray's direction d (as traced).
    pub direction: Vec3,
    /// W, the medium colour (Phi') laid on every sample's colour (as 5.0 only).
    pub weight: [f32; 3],
    /// Uniform Density (bit 0x1000000, as 5.0 only): delta = 1 at every sample.
    pub uniform_density: bool,
    /// Whether the lit kind's lights send shadow rays (as 5.0: the scene's switch word has bit 0x8
    /// and the material lacks 0x20; before 5.0 always).
    pub light_shadows: bool,
    /// Jittered sample places (bit 0x800000, as 5.0 only).
    pub jitter: bool,
    /// The flat kind lit by the scene's lights (bit 0x400000, as 5.0 only): the scene's ambient
    /// colour and amount E and Ea.
    pub flat_lit: Option<([f32; 3], f64)>,
    /// The depth that sets the step ('s; a shadow ray's by its stretches).
    pub depth: u32,
    pub shadow: bool,
}

/// A and B of a ray o + λ·d counting from λ0 that met `met`, or has the end λ1, or neither.
pub fn stretch_ends(origin: Vec3, direction: Vec3, from: f64, to: Option<f64>, met: Option<Vec3>) -> (Vec3, Vec3) {
    let at = |l: f64| -> Vec3 { std::array::from_fn(|k| origin[k] + l * direction[k]) };
    let b = match (met, to) {
        (Some(p), _) => p,
        (None, Some(l1)) => at(l1),
        (None, None) => at(FAR_END),
    };
    (at(from), b)
}

/// A march's C and α.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marched {
    pub colour: [f32; 3],
    pub alpha: f64,
}

/// The step h for a quality and a depth.
pub fn step_length(version: RenderAs, orders: VolumeOrders, quality: f64, depth: u32) -> f64 {
    let exponent = if quality > -Q_WINDOW && quality < Q_WINDOW {
        LOG10_FLOOR * K
    } else {
        orders.step.product(log2_as(version, quality), M, K)
    };
    let h = STEP_BASE + STEP_SPAN * exp2_as(version, exponent);
    if version == RenderAs::Bryce50 {
        // : h' = (c + 1)*h
        (f64::from(depth) + 1.0) * h
    } else if depth >= 2 {
        h * f64::from(depth)
    } else {
        h
    }
}

/// The depth inside at a world point.
fn depth_inside(version: RenderAs, orders: VolumeOrders, shape: DepthShape, site: &dyn MarchSite, world: Vec3) -> f64 {
    match shape {
        DepthShape::Whole => 1.0,
        DepthShape::Slab => {
            // : 1 - |p_y| held to 0 below and to 1 above
            let p = site.to_own(world);
            let d = 1.0 - p[1].abs();
            if d < 0.0 { 0.0 } else if d > 1.0 { 1.0 } else { d }
        }
        DepthShape::Sphere => {
            let p = site.to_own(world);
            1.0 - sqrt_as(version, orders.depth.dot(p, p))
        }
        DepthShape::Cylinder => {
            // : 1 - max(rho, |y|), rho = R(z*z + x*x)
            let p = site.to_own(world);
            let rho = sqrt_as(version, p[2] * p[2] + p[0] * p[0]);
            1.0 - rho.max(p[1].abs())
        }
        DepthShape::Cube => {
            // : 1 where p = (0, 0, 0); else 1 - 2m, m = |x| where |x| > |y| and
            // |x| > |z|, else |y| where |y| > |x| and |y| > |z|, else |z|
            let p = site.to_own(world);
            if p == [0.0; 3] {
                return 1.0;
            }
            let (x, y, z) = (p[0].abs(), p[1].abs(), p[2].abs());
            let m = if x > y && x > z {
                x
            } else if y > x && y > z {
                y
            } else {
                z
            };
            1.0 - 2.0 * m
        }
        DepthShape::Cone => {
            // : min((y + 1)*1.618033991, ((1 - y) - 2*rho)*0.723606798)
            let p = site.to_own(world);
            let rho = sqrt_as(version, p[2] * p[2] + p[0] * p[0]);
            let a = (p[1] + 1.0) * 1.618033991;
            let b = ((1.0 - p[1]) - 2.0 * rho) * 0.723606798;
            if b < a { b } else { a }
        }
        DepthShape::Pyramid => {
            // : min((y + 0.5)*3.236068086, (0.5 - (2m + y))*1.447213644)
            let p = site.to_own(world);
            let m = if p[0].abs() > p[2].abs() { p[0].abs() } else { p[2].abs() };
            let a = (p[1] + 0.5) * 3.236068086;
            let b = (0.5 - (2.0 * m + p[1])) * 1.447213644;
            if b < a { b } else { a }
        }
        DepthShape::Torus { tube, ring } => {
            // : 1 - d/t, d = R(((z*z + x*x) + y*y) - 2*rho*r + r*r)
            let p = site.to_own(world);
            let rho = sqrt_as(version, p[2] * p[2] + p[0] * p[0]);
            let d = sqrt_as(version, (((p[2] * p[2] + p[0] * p[0]) + p[1] * p[1]) - 2.0 * rho * ring) + ring * ring);
            1.0 - d / tube
        }
        DepthShape::Field => site.field_depth(world),
    }
}

/// The density from v, the depth inside δ and the distance s.
pub fn density(version: RenderAs, orders: VolumeOrders, numbers: VolumeNumbers, v: f64, delta: f64, s: f64) -> f64 {
    let rho0 = 1.0 - v;
    if !(rho0 > 0.0) {
        return rho0;
    }
    let e = exp2_as(version, ((1.0 - numbers.sigma) * EDGE_SIX) * L);
    let mut rho1 = rho0 * (1.0 - exp2_as(version, -orders.density.product(e + EDGE_FIVE, delta, L)));
    if !(rho1 > 0.0) {
        return 0.0;
    }
    if !(s < FADE_FROM) {
        rho1 *= 1.0 - (s - FADE_FROM) * FADE_RATE;
    }
    if numbers.phi != 1.0 {
        // : where P(rho1, phi) is above 1 the density is 1
        let p = pow(rho1, numbers.phi);
        if p > 1.0 { 1.0 } else { p }
    } else {
        rho1
    }
}

/// A sample's opacity τ = 1 − P(1 − ρ, k·e).
fn opacity(kind: VolumeKind, rho: f64, e: f64) -> f64 {
    1.0 - pow(1.0 - rho, kind.opacity_factor() * e)
}

/// The density at a point of the object's own space with its channels.
fn density_at(
    version: RenderAs,
    orders: VolumeOrders,
    numbers: VolumeNumbers,
    shape: DepthShape,
    site: &mut dyn MarchSite,
    own: Vec3,
    s: f64,
    texture_s: f64,
    uniform: bool,
) -> Result<(f64, SampleChannels, Vec3), OpenItem> {
    let world = site.to_world(own);
    let ch = site.channels(world, own, texture_s)?;
    let v = ch.density_value;
    // : a value outside 0 to 1 is used as it is.
    // : delta is 1 and the shape is not asked
    let delta = if uniform { 1.0 } else { depth_inside(version, orders, shape, site, world) };
    Ok((density(version, orders, numbers, v, delta, s), ch, world))
}

/// The march over a ray's stretch: its colour C and opacity α.
pub fn march(
    version: RenderAs,
    orders: VolumeOrders,
    kind: VolumeKind,
    numbers: VolumeNumbers,
    shape: DepthShape,
    ray: &MarchRay,
    site: &mut dyn MarchSite,
) -> Result<Marched, OpenItem> {
    if ray.shadow && kind == VolumeKind::Lit {
        return Ok(Marched { colour: [1.0; 3], alpha: 0.0 });
    }
    let a = site.to_own(ray.a);
    let b = site.to_own(ray.b);
    let w = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let stretch_order = if kind == VolumeKind::Lit { orders.lit_stretch } else { orders.stretch };
    let len = sqrt_as(version, stretch_order.dot(w, w));
    let u = [w[0] / len, w[1] / len, w[2] / len];
    let h = step_length(version, orders, numbers.quality, ray.depth);
    let five = version == RenderAs::Bryce50;
    let uniform = five && ray.uniform_density;
    let weight = ray.weight.map(f64::from);
    let mut c = [0.0f64; 3];
    let mut alpha = 0.0f64;
    let mut s = 0.0f64;
    let mut step = h;
    let mut e = h;
    // : a sample only while s is not above ℓ and is below 200.
    while !(s > len) && s < SAMPLE_REACH {
        // : one draw per place, before anything of the sample;
        // the point is moved along the stretch, the density and the next distance take s
        let sj = if five && ray.jitter {
            let r = f64::from(site.draw()?);
            ((r * JITTER_UNIT) * (e / 4.0)) + s
        } else {
            s
        };
        let x = [sj * u[0] + a[0], sj * u[1] + a[1], sj * u[2] + a[2]];
        // : as 5.0 the distance in the world
        // from the ray's origin to the sample's place; the sample's own for its moved points
        let texture_s = if five { site.texture_distance(kind, site.to_world(x), sj) } else { s };
        let (rho, ch, world) = density_at(version, orders, numbers, shape, site, x, s, texture_s, uniform)?;
        let (colour, tau) = match kind {
            VolumeKind::Flat => {
                let m = &ch.light;
                let colour: [f64; 3] = match version {
                    RenderAs::Bryce50 if ray.flat_lit.is_some() => {
                        flat_lit_colour(version, orders, ray, site, m, rho, world)?
                    }
                    RenderAs::Bryce30 => m.diffuse.map(f64::from),
                    _ => std::array::from_fn(|k| {
                        f64::from(m.diffuse[k]) * m.diffusion + f64::from(m.ambient[k]) * m.ambience
                    }),
                };
                (colour, opacity(kind, rho, e))
            }
            VolumeKind::Shaded => {
                let tau = opacity(kind, rho, e);
                if ray.shadow {
                    (ch.light.diffuse.map(f64::from), tau)
                } else {
                    let moved = |i: usize| -> Vec3 {
                        let mut p = x;
                        p[i] -= GRADIENT_STEP;
                        p
                    };
                    let mut g = [0.0; 3];
                    for (i, gi) in g.iter_mut().enumerate() {
                        let (rho_i, _, _) =
                            density_at(version, orders, numbers, shape, site, moved(i), s, texture_s, uniform)?;
                        *gi = rho_i - rho;
                    }
                    if five && g.iter().all(|&x| x > -GRADIENT_TINY && x < GRADIENT_TINY) {
                        // : N is g as it stands
                        (site.shade(world, g, &ch)?.map(f64::from), tau)
                    } else {
                        // : as 3.0 the formula as it stands with
                        // Q(0) finite ; as 3.1 Q(0) is +infinity, so the normal is not a
                        // number and the lighting  gives the ambient term alone,
                        // whatever the sample's opacity
                        let qg = rsqrt_as(version, NORMAL_ORDER.dot(g, g));
                        let gh = [g[0] * qg, g[1] * qg, g[2] * qg];
                        let n1 = site.normal_to_world(gh);
                        let qn = rsqrt_as(version, NORMAL_ORDER.dot(n1, n1));
                        let n = [n1[0] * qn, n1[1] * qn, n1[2] * qn];
                        (site.shade(world, n, &ch)?.map(f64::from), tau)
                    }
                }
            }
            VolumeKind::Lit => {
                let mut colour = ch.light.diffuse.map(f64::from);
                let mut beta = 0.0f64;
                if rho > 0.0 {
                    for i in 0..site.light_count() {
                        let light = site.light(i);
                        if light.switches & light_switch::ON == 0 {
                            continue;
                        }
                        // , (The lights
                        // with a beam: I times the beam's factor at X, 0 outside the beam)
                        let mut intensity = match site.beam_factor_at(i, world)? {
                            Some(phi) => light.intensity * phi,
                            None => 0.0,
                        };
                        if intensity.abs() < LEAST_INTENSITY {
                            continue;
                        }
                        let mut l = site.light_colour_at(i, world)?;
                        let (direction, mut r, reach) = light_direction_ordered(version, light.place, world, orders.light_distance);
                        let sun = matches!(light.place, LightPlace::Sun { .. });
                        // Step 1: the falloff.
                        if light.switches & light_switch::NO_FALL_OFF == 0 {
                            if sun {
                                // : the
                                // sun's light has a place for this, the place of the sun's object; its
                                // shadow ray keeps the sun's direction and no end.
                                let place = LightPlace::Radial { position: site.sun_place() };
                                r = light_direction_ordered(version, place, world, orders.light_distance).1;
                            }
                            intensity = if five && light.switches & light_switch::RANGE_FALL_OFF != 0 {
                                // :'s ranged falloff
                                falloff(version, light.switches, site.light_range(i), r) * intensity
                            } else if light.switches & light_switch::SQUARE_FALL_OFF != 0 {
                                (1.0 / (r * r)) * intensity
                            } else {
                                (1.0 / r) * intensity
                            };
                        }
                        // Step 2: the shadow ray.
                        if light.switches & light_switch::NO_SHADOWS == 0 && (!five || ray.light_shadows) {
                            let sh = site.shadow(i, world, direction, reach)?;
                            l = std::array::from_fn(|k| (f64::from(sh.colour[k]) * f64::from(l[k])) as f32);
                            intensity *= sh.amount; // a'·I (a product of two: either order)
                        }
                        // Step 3.
                        for k in 0..3 {
                            colour[k] *= f64::from(l[k]);
                        }
                        let kappa = if sun { KAPPA_SUN } else { KAPPA_OTHER };
                        beta += kappa * intensity * rho;
                        if beta > 1.0 {
                            beta = 1.0;
                        }
                    }
                }
                (colour, opacity(kind, beta, e))
            }
        };
        if tau > 0.0 {
            // : after the sample's colour is formed, before W
            let colour = if five && !ray.shadow && site.sky_integrated() {
                site.sky_over(world, colour.map(|x| x as f32))?.map(f64::from)
            } else {
                colour
            };
            let wgt = tau * (1.0 - alpha);
            for k in 0..3 {
                if five {
                    // : (W_k*c_k)*w + C_k
                    c[k] = (weight[k] * colour[k]) * wgt + c[k];
                } else {
                    c[k] += wgt * colour[k];
                }
            }
            alpha += wgt;
            if alpha > OPAQUE_ENOUGH {
                alpha = 1.0;
                break;
            }
        }
        // : the next distance.
        if s + step > len {
            step = len - s;
        }
        if step == 0.0 {
            break;
        }
        s += step;
        e = step;
    }
    if ray.shadow {
        return Ok(Marched { colour: [1.0; 3], alpha: alpha.min(1.0) });
    }
    let colour = c.map(|ck| if ck > 1.0 { 1.0 } else { ck as f32 });
    Ok(Marched { colour, alpha: if alpha > 1.0 { 1.0 } else { alpha } })
}

/// The flat kind's sample colour lit by the scene's lights. (The flat kind lit by the
/// scene's lights)
fn flat_lit_colour(
    version: RenderAs,
    orders: VolumeOrders,
    ray: &MarchRay,
    site: &mut dyn MarchSite,
    m: &PlainMaterial,
    rho: f64,
    world: Vec3,
) -> Result<[f64; 3], OpenItem> {
    let (ambient_colour, ambience) = ray.flat_lit.expect("checked by the caller");
    let kd = m.diffusion;
    let d = m.diffuse.map(f64::from);
    let mut sums = [0.0f64; 3];
    if ray.shadow {
        // nothing is asked of any light; the sums start at D*kd
        for k in 0..3 {
            sums[k] = d[k] * kd;
        }
    } else if rho > LEAST_INTENSITY {
        for i in 0..site.light_count() {
            let light = site.light(i);
            if light.switches & light_switch::ON == 0 {
                continue;
            }
            let mut l = site.light_colour_at(i, world)?;
            let mut intensity = light.intensity;
            let counts = intensity.abs() >= LEAST_INTENSITY && l.iter().any(|&c| f64::from(c).abs() > LEAST_INTENSITY);
            if !counts {
                continue;
            }
            let (direction, r, reach) = light_direction_ordered(version, light.place, world, orders.light_distance);
            if light.switches & light_switch::NO_FALL_OFF == 0 {
                intensity = falloff(version, light.switches, site.light_range(i), r) * intensity;
                if intensity.abs() < LEAST_INTENSITY {
                    continue;
                }
            }
            // (The flat kind lit by the scene's lights, observed: fl-s2 of
            // ` `): where the scene's shadows are on, the
            // light casts and the material takes them, the light's shadow from the sample's
            // place lifts its colour and intensity as lifts a light of the lit air
            if light.switches & light_switch::NO_SHADOWS == 0 && ray.light_shadows {
                let sh = site.shadow(i, world, direction, reach)?;
                // Bryce 5's light in shadow first: where |eta| is not below 0.0001 and one channel
                // of psi is above it in magnitude, psi_k * f is added to channel k of the sums,
                // f = ((1 - s) * (1 - alpha)) * eta, each sum staying a binary64 number.
                let (eta, psi) = site.light_shadow_share(i);
                if eta.abs() >= LEAST_INTENSITY && psi.iter().any(|&c| f64::from(c).abs() > LEAST_INTENSITY) {
                    let f = ((1.0 - site.shadow_setting()) * (1.0 - sh.amount)) * eta;
                    for k in 0..3 {
                        sums[k] += f64::from(psi[k]) * f;
                    }
                }
                match lift_by_shadow(l, intensity, sh, site.shadow_setting()) {
                    Some((lifted, i2)) => {
                        l = lifted;
                        intensity = i2;
                    }
                    None => continue,
                }
            }
            for k in 0..3 {
                sums[k] += ((f64::from(l[k]) * d[k]) * intensity) * kd;
            }
        }
    }
    // after the lights, for every ray
    let a = m.ambient.map(f64::from);
    for k in 0..3 {
        sums[k] += ((f64::from(ambient_colour[k]) * a[k]) * ambience) * m.ambience;
    }
    Ok(sums)
}

/// What the ray that was marched met: it decides how the march is laid over the colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// It met nothing: the sky's colour.
    Miss,
    /// It met a surface of a material that is not a volume material.
    Surface,
    /// It met a volume material's surface: what the ray that goes on brought back.
    VolumeSurface,
}

/// A march to lay over the colour of the ray marched, last. (Where marches are
/// made)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Laying {
    pub marched: Marched,
    /// The material's flag word has bit 0x2000: the march is added.
    pub adds: bool,
    pub after: After,
    /// A render as 5.0:'s join.
    pub five: bool,
}

impl Laying {
    /// The colour with the march laid over it.
    pub fn over(&self, c: [f32; 3]) -> [f32; 3] {
        let a = self.marched.alpha;
        let m = self.marched.colour.map(f64::from);
        let cd = c.map(f64::from);
        let r = |x: f64| f64::from(x as f32);
        // : covered or added where α ≠ 0 (an α of 0 changes nothing)
        if a == 0.0 {
            return c;
        }
        if self.adds {
            return [
                (cd[0] + r(a * m[0])) as f32,
                (cd[1] + r(a * m[1])) as f32,
                (cd[2] + a * m[2]) as f32,
            ];
        }
        if self.five {
            // : c_k becomes (1 - alpha)*c_k + C_k, the product and the sum rounded
            let k = 1.0 - a;
            return std::array::from_fn(|i| (r(k * cd[i]) + m[i]) as f32);
        }
        let ra = r(a);
        match self.after {
            After::Miss | After::VolumeSurface => {
                std::array::from_fn(|k| (cd[k] + r(r(m[k] - cd[k]) * ra)) as f32)
            }
            // (covered after a surface: red ⟨o_r + ⟨α·(C_r − o_r)⟩⟩, green
            // ⟨⟨α·⟨C_g − o_g⟩⟩ + o_g⟩, blue ⟨α·⟨C_b − o_b⟩ + o_b⟩)
            After::Surface => [
                (cd[0] + r(a * (m[0] - cd[0]))) as f32,
                (r(a * r(m[1] - cd[1])) + cd[1]) as f32,
                (a * r(m[2] - cd[2]) + cd[2]) as f32,
            ],
        }
    }
}

/// The y extent of a shape's own space for bit 0x8000.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    Y(f64, f64),
    /// The torus: Open.
    Open,
}

/// What the medium colour Φ' reads of a material: Φ, the flag word's bits, the shape's
/// extent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediumTint {
    /// Bit 0x4000: the colour comes in with the distance.
    pub distance: bool,
    /// Bit 0x8000: the colour by height, with the shape's extent.
    pub altitude: Option<Extent>,
    /// Inside a combined group, bit 0x8000 takes the group's matrix and the group's own extent
    /// in y, not the member's, in all three versions.
    pub group: Option<GroupSpace>,
}

/// The space of a combined group a medium of one of its members is read in: the group's map and
/// the bounds in y of the box that holds its members' boxes, in the group's space and widened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupSpace {
    pub placement: crate::geometry::Placement,
    pub y0: f64,
    pub y1: f64,
}

/// What Φ' reads of a ray's hit: its parameter t and the hit point's y in the solid's own
/// space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediumHit {
    pub t: f64,
    pub own_y: f64,
}

impl MediumTint {
    /// The tint of a material's flag word, `None` where neither bit is set. Bit 8 (white)
    /// is the caller's (it overrides).
    pub fn of(flags: u32, extent: Extent) -> Option<MediumTint> {
        let distance = flags & bits::BLEND_DISTANCE != 0;
        let altitude = (flags & bits::BLEND_ALTITUDE != 0).then_some(extent);
        (distance || altitude.is_some()).then_some(MediumTint { distance, altitude, group: None })
    }

    /// Φ' from Φ for a ray with `hit` (or a miss).
    pub fn colour(&self, version: RenderAs, phi: [f32; 3], hit: Option<MediumHit>) -> Result<[f32; 3], OpenItem> {
        let toward_white = |c: [f32; 3], kappa: f64| -> [f32; 3] {
            std::array::from_fn(|k| {
                let ck = f64::from(c[k]);
                let one_less = f64::from((1.0 - ck) as f32);
                (f64::from((one_less * kappa) as f32) + ck) as f32
            })
        };
        let mut out = phi;
        if self.distance {
            out = match hit {
                Some(h) => {
                    let f = ((h.t * h.t) * DISTANCE_RATE).min(1.0);
                    let kappa = f64::from((1.0 - f) as f32);
                    toward_white(phi, kappa)
                }
                None => match version {
                    RenderAs::Bryce30 => [1.0; 3],
                    _ => phi,
                },
            };
        }
        if let Some(extent) = self.altitude {
            match hit {
                // : as 3.1 and 5.0 bit 0x8000 changes nothing for a ray that met
                // nothing. As 3.0 the original takes the place from a hit point that such a ray never sets (a value
                // left over from earlier work: "a program of ours is free in it"), so this program takes the middle
                // of the extent (y = 0, kappa 1/2 in every symmetric shape) instead of refusing. The original's
                // pictures of al-sphere differ from it in one or two levels in 134 pixels (not exact; no rule gives
                // more). The recomp never drew such a ray either (DEVELOPER_DIARY, finding 0046: stale stack).
                None => {
                    if version == RenderAs::Bryce30 {
                        if let Extent::Y(y0, y1) = extent {
                            let kappa = f64::from(((0.0f64.clamp(y0, y1) + y1) / (y1 - y0)) as f32);
                            out = toward_white(out, kappa);
                        }
                    }
                }
                Some(h) => {
                    // : kappa = (y + y1)/(y1 - y0), y held to the
                    // shape's own extent; inside a combined group the group's box in y, not
                    // held, so that kappa may lie outside 0 to 1.
                    let (y0, y1, held) = match (self.group, extent) {
                        (Some(g), _) => (g.y0, g.y1, false),
                        (None, Extent::Y(y0, y1)) => (y0, y1, true),
                        (None, Extent::Open) => return Err(open::ALTITUDE_TORUS),
                    };
                    if version == RenderAs::Bryce30 || !self.distance {
                        let y = if held { h.own_y.clamp(y0, y1) } else { h.own_y };
                        let kappa = f64::from(((y + y1) / (y1 - y0)) as f32);
                        out = toward_white(out, kappa);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// f = (t·t)·0.03 for bit 0x4000.
const DISTANCE_RATE: f64 = 0.03;

#[cfg(test)]
mod medium_tests {
    use super::*;

    /// A ray that met nothing in a medium with bit 0x8000 as 3.0: drawn (the place is the middle of the extent),
    /// not refused; as 3.1 and 5.0 the bit changes nothing.
    #[test]
    fn a_miss_in_an_altitude_medium_is_drawn_as_3_0() {
        let tint = MediumTint::of(bits::BLEND_ALTITUDE, Extent::Y(-1.0, 1.0)).unwrap();
        let phi = [0.1f32, 0.9, 0.2];
        let as_30 = tint.colour(RenderAs::Bryce30, phi, None).expect("no refusal");
        // kappa = (0 + 1) / 2: halfway to white
        assert_eq!(as_30, [0.55, 0.95, 0.6]);
        assert_eq!(tint.colour(RenderAs::Bryce31, phi, None).unwrap(), phi);
        assert_eq!(tint.colour(RenderAs::Bryce50, phi, None).unwrap(), phi);
    }
}
