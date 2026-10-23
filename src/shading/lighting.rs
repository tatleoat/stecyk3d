// SPDX-License-Identifier: MIT
//! Light at a surface: the light terms, shadow lifting, ambient and dome light.
//!
//! ; is asked
//! through `ShadowSource` (the renderer's tracer: src/render/lit.rs)
//! (Light at a surface; Revision of)
//! spec : the material's flag bits 0x20 and 0x2000 , Bryce 5's falloff over a
//! range , light in shadow and gathered light ; beams and gradients: `lights`
//! spec :'s gathered colour C is the ambient ray's (point 5),
//! traced between the light terms and the rest: `light_terms` and `LightTerms::finish`
//! (Premium blurs and true ambience in the shading)
//!
//! Every binary64 operation is one Rust `f64` operation (no fusing, no wider intermediate);
//! ⟨x⟩ of the entry is `x as f32`. Products of three or more factors are written left to
//! right (the entry lets them be grouped in any way).

use crate::geom::Vec3;
use crate::numerics::{pow, sqrt_as};
use crate::version::RenderAs;

/// "Small": of magnitude below the double nearest 0.0001.
pub const SMALL: f64 = 0.0001;

/// A light's switch bits. step 5
pub mod light_switch {
    pub const ON: u32 = 0x100;
    pub const NO_FALL_OFF: u32 = 0x400;
    pub const NO_SHADOWS: u32 = 0x800; // step 5
    pub const SQUARE_FALL_OFF: u32 = 0x8000_0000; // [format]
    pub const RANGE_FALL_OFF: u32 = 0x2000_0000; // [format]
    pub const GRADIENT: u32 = 0x20_0000; // [format]
}

/// The material's flag bits.,
pub mod material_flag {
    pub const PLAIN: u32 = 0x1_0000;
    pub const TAKES_NO_SHADOWS: u32 = 0x20; // step 5 [format]
    pub const ADDS_TO_BEHIND: u32 = 0x2000; // [format]
}

/// The scene's switch bits. step 5,
pub mod scene_switch {
    pub const SHADOWS: u32 = 0x8; // step 5
    pub const TRANSMISSION: u32 = 0x2;
    pub const GATHERED_LIGHT: u32 = 0x400_0000; // [format]
}

/// The sky's bit 0x100: it plays no part in the dome term.
pub const SKY_DOME_ON: u32 = 0x100;

/// The 3.1 and 5.0 exponent of the metallic factor, the double nearest 0.8 (3.0: 0).
/// step 7
const METAL_POWER_31: f64 = 0.8; // step 7

/// How a three-term sum x + y + z is grouped. (Sums of three terms)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SumOrder {
    /// (x + y) + z
    XyZ,
    /// (x + z) + y
    XzY,
    /// (y + z) + x
    YzX,
}

impl SumOrder {
    pub const ALL: [SumOrder; 3] = [SumOrder::XyZ, SumOrder::XzY, SumOrder::YzX];

    /// The dot product a·b with its terms summed in this order.
    pub fn dot(self, a: Vec3, b: Vec3) -> f64 {
        let (x, y, z) = (a[0] * b[0], a[1] * b[1], a[2] * b[2]);
        match self {
            SumOrder::XyZ => (x + y) + z,
            SumOrder::XzY => (x + z) + y,
            SumOrder::YzX => (y + z) + x,
        }
    }
}

/// The orders of the three sums the entry leaves to the vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DotOrders {
    /// The facing test g·v.
    pub facing: SumOrder,
    /// λ = L·n.
    pub lambda: SumOrder,
    /// c' = ρ·v.
    pub reflect: SumOrder,
}

impl DotOrders {
    /// The orders ` ` selects for each version (its 60 grazing
    /// cases separate them; tests/shading_vectors.rs tries all 27 per version).
    pub fn of(version: RenderAs) -> DotOrders {
        use SumOrder::*;
        match version {
            RenderAs::Bryce30 => DotOrders { facing: XyZ, lambda: XyZ, reflect: XyZ },
            RenderAs::Bryce31 => DotOrders { facing: YzX, lambda: XyZ, reflect: XyZ },
            RenderAs::Bryce50 => DotOrders { facing: YzX, lambda: YzX, reflect: YzX },
        }
    }
}

/// The per-point inputs the revision of adds.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceInputs {
    /// The material's flag word .
    pub flags: u32,
    /// s0, the colour the sum starts from: black, or with flag bit 0x2000 what the ray sent
    /// straight on through the surface brings back (the caller's:, Open there).
    pub start: [f32; 3],
    /// C, the colour gathered at the point; `None` where the render gives none.
    pub gathered: Option<[f32; 3]>,
}

impl SurfaceInputs {
    /// A plain material's flag word, a black start, no gathered colour: the inputs before
    /// the revision.,
    pub const PLAIN: SurfaceInputs = SurfaceInputs { flags: material_flag::PLAIN, start: [0.0; 3], gathered: None };
}

impl Default for SurfaceInputs {
    fn default() -> Self {
        SurfaceInputs::PLAIN
    }
}

/// Bryce 5's settings of a light that the lighting reads.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bryce5Light {
    /// ρ, the range (read only with switch bit 0x20000000).
    pub range: f64,
    /// η, the shadow share.
    pub shadow_share: f64,
    /// ψ, the shadow colour.
    pub shadow_colour: [f32; 3],
}

impl Bryce5Light {
    /// η = 0 ("as every light of a Bryce 3D document"); ψ black and ρ = 0 are never
    /// read then (a Bryce 3D light has no bit 0x20000000).
    pub const NONE: Bryce5Light = Bryce5Light { range: 0.0, shadow_share: 0.0, shadow_colour: [0.0; 3] };
}

impl Default for Bryce5Light {
    fn default() -> Self {
        Bryce5Light::NONE
    }
}

/// A light with its Bryce 5 settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitLight {
    pub source: LightSource,
    pub bryce5: Bryce5Light,
}

/// What the sum reads of a light.
pub trait LightInput {
    fn source(&self) -> &LightSource;
    fn bryce5(&self) -> Bryce5Light;
}

impl LightInput for LightSource {
    fn source(&self) -> &LightSource {
        self
    }
    fn bryce5(&self) -> Bryce5Light {
        Bryce5Light::NONE
    }
}

impl LightInput for LitLight {
    fn source(&self) -> &LightSource {
        &self.source
    }
    fn bryce5(&self) -> Bryce5Light {
        self.bryce5
    }
}

/// The order of ℓ's Δ·Δ: x last in every version.
const DISTANCE_ORDER: SumOrder = SumOrder::YzX;

/// The order of e = |v·n| (step 7): z last as 3.0 and 3.1, x last as 5.0.
fn metal_order(version: RenderAs) -> SumOrder {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => SumOrder::XyZ,
        RenderAs::Bryce50 => SumOrder::YzX,
    }
}

/// The shadow ray's direction toward a placed light, Δ = q − p brought to unit length in the
/// original's form: Q(Δ·Δ)·Δ, the sum x last as 3.0 and 3.1 (3.1's Q being 1/R, this is
/// (1/R(Δ·Δ))·Δ) and y last as 5.0; Δ itself where Δ·Δ is 0.
pub fn shadow_direction(version: RenderAs, d: Vec3) -> Vec3 {
    let order = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => SumOrder::YzX,
        RenderAs::Bryce50 => SumOrder::XzY,
    };
    let dd = order.dot(d, d);
    if dd == 0.0 {
        return d;
    }
    let q = crate::numerics::rsqrt_as(version, dd);
    [q * d[0], q * d[1], q * d[2]]
}

/// The hit as shading sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePoint {
    /// p.
    pub point: Vec3,
    /// d; v = −d points toward the eye.
    pub ray_direction: Vec3,
    /// g.
    pub geometric_normal: Vec3,
    /// n.
    pub shading_normal: Vec3,
}

/// A plain material's channels as the document holds them (each channel's A value).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlainMaterial {
    /// D.
    pub diffuse: [f32; 3],
    /// kd. The values are doubles (Inputs: a plain material's singles widened, a
    /// textured material's as gives them).
    pub diffusion: f64,
    /// A.
    pub ambient: [f32; 3],
    /// ka.
    pub ambience: f64,
    /// S.
    pub specular: [f32; 3],
    /// ks.
    pub specularity: f64,
    /// H, the Specular Halo colour.
    pub halo: [f32; 3],
    /// t.
    pub transparency: f64,
    /// m.
    pub metallicity: f64,
}

/// The scene-wide inputs of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneLight {
    /// The scene's switch word .
    pub switches: u32,
    /// E.
    pub ambient_colour: [f32; 3],
    /// K.
    pub ambience: f64,
    /// s, the sky part's shadow intensity.
    pub shadow_setting: f64,
    /// The sky's switch word.
    pub sky_switches: u32,
    /// G, the dome light's colour .
    pub dome: [f32; 3],
}

/// Where a light is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightPlace {
    /// The main light: its direction .
    Sun { direction: Vec3 },
    /// A radial light at q.
    Radial { position: Vec3 },
}

/// A light as it shines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSource {
    pub place: LightPlace,
    /// c.
    pub colour: [f32; 3],
    /// I.
    pub intensity: f64,
    pub switches: u32,
}

/// What is asked for one light at one point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowQuery {
    /// The light's index in the list.
    pub light: usize,
    /// p.
    pub point: Vec3,
    /// g, as the hit gives it.
    pub geometric_normal: Vec3,
    /// n, the shading normal as leaves it after its facing test ((0, 0, 0) for a point
    /// in the air or a volume's sample).
    pub normal: Vec3,
    /// The shadow ray's direction toward the light: for a surface's point's own form of
    /// Δ brought to unit length (`shadow_direction`), the sun's direction for the sun.
    pub direction: Vec3,
    /// How far surfaces count: ℓ for a radial light, +infinity for the sun.
    pub reach: f64,
    /// Whether the ray leaves the surface's offset along g , or p itself (a point in
    /// the air).
    pub offset: bool,
}

/// The shadow (σ, α).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// σ.
    pub colour: [f32; 3],
    /// α.
    pub amount: f64,
}

/// Answers for step 5. (the seam for a staged version)
pub trait ShadowSource {
    fn shadow(&mut self, query: &ShadowQuery) -> Shadow;
}

/// The local colour and what the vectors check besides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalColour {
    pub colour: [f32; 3],
    /// The shading normal after the facing test.
    pub shading_normal: Vec3,
    /// How many lights' shadows were asked.
    pub shadows_asked: u32,
    /// Whether asked for the gathered colour (as 5.0, where its conditions hold).
    pub gathered_asked: bool,
}

fn is_small(x: f64) -> bool {
    x.abs() < SMALL
}

/// A colour is small when no channel's magnitude is above SMALL.
fn colour_is_small(c: [f32; 3]) -> bool {
    c.iter().all(|&k| !(f64::from(k).abs() > SMALL))
}

fn neg(a: Vec3) -> Vec3 {
    [-a[0], -a[1], -a[2]]
}

/// step 2: the direction toward the light, the distance term r, and (radial) ℓ.
/// step 2,
pub fn light_direction(version: RenderAs, place: LightPlace, p: Vec3) -> (Vec3, f64, f64) {
    light_direction_ordered(version, place, p, DISTANCE_ORDER)
}

/// As `light_direction`, with the terms of the distance's square summed in `order`.
pub fn light_direction_ordered(version: RenderAs, place: LightPlace, p: Vec3, order: SumOrder) -> (Vec3, f64, f64) {
    match place {
        LightPlace::Sun { direction } => (direction, f64::INFINITY, f64::INFINITY),
        LightPlace::Radial { position } => {
            let d = [position[0] - p[0], position[1] - p[1], position[2] - p[2]];
            let l = sqrt_as(version, order.dot(d, d));
            let dir = if l == 0.0 { d } else { [d[0] / l, d[1] / l, d[2] / l] };
            (dir, l + 1.0, l)
        }
    }
}

/// The falloff factor a.
pub fn falloff(version: RenderAs, switches: u32, range: f64, r: f64) -> f64 {
    if switches & light_switch::NO_FALL_OFF != 0 {
        1.0
    } else if version == RenderAs::Bryce50 && switches & light_switch::RANGE_FALL_OFF != 0 {
        // λ from r − 1 (the text's formula; ℓ itself changes no row: Open).
        let lambda = (r - 1.0) / range;
        if lambda >= 0.0 { 1.0 - lambda.min(1.0) } else { 1.0 }
    } else if switches & light_switch::SQUARE_FALL_OFF != 0 {
        1.0 / (r * r)
    } else {
        1.0 / r
    }
}

/// The light's intensity and colour after its shadow, or `None` when it adds nothing.
fn lift_shadow(version: RenderAs, s: f64, i: f64, c: [f32; 3], sh: Shadow) -> Option<(f64, [f32; 3])> {
    let (sigma, alpha) = (sh.colour, sh.amount);
    if s > 0.0 {
        let tau = alpha * i;
        let lifted = tau + s * (i - tau);
        if tau != i && is_small(lifted) {
            return None;
        }
        let q: [f32; 3] = std::array::from_fn(|k| (f64::from(sigma[k]) * f64::from(c[k])) as f32);
        let delta: [f32; 3] = std::array::from_fn(|k| (f64::from(c[k]) - f64::from(q[k])) as f32);
        let rounded = |k: usize| (f64::from((s * f64::from(delta[k])) as f32) + f64::from(q[k])) as f32;
        let colour = match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => {
                [rounded(0), rounded(1), (s * f64::from(delta[2]) + f64::from(q[2])) as f32]
            }
            RenderAs::Bryce50 => [rounded(0), rounded(1), rounded(2)],
        };
        if colour_is_small(colour) {
            return None;
        }
        Some((lifted, colour))
    } else {
        if is_small(alpha) {
            return None;
        }
        let i = i * alpha;
        if colour_is_small(sigma) {
            return None;
        }
        Some((i, std::array::from_fn(|k| (f64::from(sigma[k]) * f64::from(c[k])) as f32)))
    }
}

/// η, the highlight, per channel in the formats of each version. step 7
fn highlight(
    version: RenderAs,
    orders: DotOrders,
    m: &PlainMaterial,
    n: Vec3,
    v: Vec3,
    rho: Vec3,
    c: [f32; 3],
) -> [f64; 3] {
    let c_ref = orders.reflect.dot(rho, v);
    let h: [f64; 3] = std::array::from_fn(|k| {
        if c_ref > 0.0 {
            let exponent = 255.0 - f64::from(m.halo[k]) * 255.0; // step 7
            f64::from(pow(c_ref, exponent) as f32)
        } else {
            0.0
        }
    });
    let ks = m.specularity;
    let metal = m.metallicity;
    let mu: [f64; 3] = if metal == 0.0 {
        [ks; 3]
    } else {
        let e = metal_order(version).dot(v, n).abs();
        let x = match version {
            RenderAs::Bryce30 => 0.0,
            RenderAs::Bryce31 | RenderAs::Bryce50 => METAL_POWER_31,
        };
        let pi = pow(e, x) * metal;
        std::array::from_fn(|k| (1.0 - (1.0 - f64::from(m.diffuse[k])) * pi) * ks)
    };
    let s: [f64; 3] = m.specular.map(f64::from);
    let cd: [f64; 3] = c.map(f64::from);
    let r32 = |x: f64| f64::from(x as f32);
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => [
            r32(s[0] * r32(mu[0]) * h[0] * cd[0]),
            r32(s[1] * r32(mu[1]) * h[1] * cd[1]),
            s[2] * mu[2] * h[2] * cd[2],
        ],
        RenderAs::Bryce50 => [
            s[0] * mu[0] * h[0] * cd[0],
            r32(r32(r32(mu[1]) * s[1]) * h[1] * cd[1]),
            r32(r32(r32(mu[2]) * s[2]) * h[2] * cd[2]),
        ],
    }
}

/// The ambient and dome terms added after every light.
fn ambient_and_dome(
    version: RenderAs,
    o: [f32; 3],
    m: &PlainMaterial,
    sc: &SceneLight,
    n: Vec3,
    gathered: Option<[f32; 3]>,
) -> [f32; 3] {
    let e = sc.ambient_colour.map(f64::from);
    let a = m.ambient.map(f64::from);
    let (k, ka) = (sc.ambience, m.ambience);
    let amb = |c: usize| e[c] * k * a[c] * ka + f64::from(o[c]);
    // The channel carried unrounded: blue as 3.0 and 3.1, green as 5.0.
    let wide = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => 2,
        RenderAs::Bryce50 => 1,
    };
    let mut out: [f64; 3] = std::array::from_fn(|c| if c == wide { amb(c) } else { f64::from(amb(c) as f32) });
    // Bryce 5's gathered light, after the ambient term and before the dome. `gathered` is
    // given only as 5.0 where its conditions hold, so green is the channel carried wide.
    if let Some(cg) = gathered {
        for c in 0..3 {
            out[c] = if c == wide {
                a[c] * f64::from(cg[c]) * ka + f64::from(out[c] as f32)
            } else {
                f64::from((f64::from(cg[c]) * a[c] * ka + out[c]) as f32)
            };
        }
    }
    // The dome term is added whatever bit 0x100 of the sky's switch word.
    if n[1] > 0.0 {
        let w = (m.diffusion + ka) * n[1] / 2.0;
        for c in 0..3 {
            let t = f64::from(sc.dome[c]) * w * f64::from(m.diffuse[c]) + out[c];
            out[c] = if c == wide { t } else { f64::from(t as f32) };
        }
    }
    out.map(|x| x as f32)
}

/// The surface's local colour:'s light terms, then's ambient and dome terms.
pub fn local_colour(
    version: RenderAs,
    at: &SurfacePoint,
    m: &PlainMaterial,
    sc: &SceneLight,
    lights: &[LightSource],
    shadows: &mut dyn ShadowSource,
) -> LocalColour {
    local_colour_ordered(version, DotOrders::of(version), at, m, sc, lights, shadows)
}

/// The local colour with the inputs of the revision of: the material's flag word,
/// the start colour s0, the gathered colour, and each light's Bryce 5 settings. The
/// material's channels are those gives at p (from textures or not).
#[allow(clippy::too_many_arguments)]
pub fn local_colour_with<L: LightInput>(
    version: RenderAs,
    orders: DotOrders,
    at: &SurfacePoint,
    m: &PlainMaterial,
    point: &SurfaceInputs,
    sc: &SceneLight,
    lights: &[L],
    shadows: &mut dyn ShadowSource,
) -> LocalColour {
    light_sum(version, orders, at, m, point, sc, lights, shadows)
}

/// The first half of the local colour:'s light terms summed (each light's shadow
/// asked), with what the second half needs. (Premium blurs and true
/// ambience: the lighting in two halves)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightTerms {
    /// The sum of the light terms o (binary32 channels).
    pub sum: [f32; 3],
    /// The shading normal after the facing test.
    pub shading_normal: Vec3,
    /// How many lights' shadows were asked.
    pub shadows_asked: u32,
    /// Whether asks for the gathered colour (as 5.0, where its conditions hold).
    pub gathered_asked: bool,
}

impl LightTerms {
    /// The second half:'s ambient term, the gathered colour `gathered` where it is
    /// asked and given (`None`: nothing added), and the dome term.
    pub fn finish(&self, version: RenderAs, m: &PlainMaterial, sc: &SceneLight, gathered: Option<[f32; 3]>) -> LocalColour {
        let gathered = if self.gathered_asked { gathered } else { None };
        LocalColour {
            colour: ambient_and_dome(version, self.sum, m, sc, self.shading_normal, gathered),
            shading_normal: self.shading_normal,
            shadows_asked: self.shadows_asked,
            gathered_asked: self.gathered_asked,
        }
    }
}

/// 's light terms at a point, its shadows asked through `shadows`;'s terms
/// follow with `LightTerms::finish`.
#[allow(clippy::too_many_arguments)]
pub fn light_terms<L: LightInput>(
    version: RenderAs,
    orders: DotOrders,
    at: &SurfacePoint,
    m: &PlainMaterial,
    point: &SurfaceInputs,
    sc: &SceneLight,
    lights: &[L],
    shadows: &mut dyn ShadowSource,
) -> LightTerms {
    light_terms_sum(version, orders, at, m, point, sc, lights, shadows)
}

/// `local_colour` with the three sums' orders given (the vector test tries each, to check
/// the table `DotOrders::of`).
pub fn local_colour_ordered(
    version: RenderAs,
    orders: DotOrders,
    at: &SurfacePoint,
    m: &PlainMaterial,
    sc: &SceneLight,
    lights: &[LightSource],
    shadows: &mut dyn ShadowSource,
) -> LocalColour {
    light_sum(version, orders, at, m, &SurfaceInputs::PLAIN, sc, lights, shadows)
}

/// 's light terms, then's ambient, gathered and dome terms.
#[allow(clippy::too_many_arguments)]
fn light_sum<L: LightInput>(
    version: RenderAs,
    orders: DotOrders,
    at: &SurfacePoint,
    m: &PlainMaterial,
    point: &SurfaceInputs,
    sc: &SceneLight,
    lights: &[L],
    shadows: &mut dyn ShadowSource,
) -> LocalColour {
    light_terms_sum(version, orders, at, m, point, sc, lights, shadows).finish(version, m, sc, point.gathered)
}

/// 's light terms (steps 1 to 8 for each light). (Bryce 5's light
/// in shadow; when the gathered colour is asked)
#[allow(clippy::too_many_arguments)]
fn light_terms_sum<L: LightInput>(
    version: RenderAs,
    orders: DotOrders,
    at: &SurfacePoint,
    m: &PlainMaterial,
    point: &SurfaceInputs,
    sc: &SceneLight,
    lights: &[L],
    shadows: &mut dyn ShadowSource,
) -> LightTerms {
    let p = at.point;
    let v = neg(at.ray_direction);
    // The facing test.
    let n = if orders.facing.dot(at.geometric_normal, v) < 0.0 { neg(at.shading_normal) } else { at.shading_normal };
    let kd = m.diffusion;
    let ks = m.specularity;
    let t = m.transparency;
    let s = sc.shadow_setting;
    // The sum starts at s0.
    let mut o = point.start;
    let mut asked = 0;
    if kd > 0.0 || ks > 0.0 {
        for (index, input) in lights.iter().enumerate() {
            let light = input.source();
            let b5 = input.bryce5();
            if light.switches & light_switch::ON == 0 {
                continue;
            }
            let mut c = light.colour;
            let mut i = light.intensity;
            // Step 1.
            if is_small(i) || i.is_nan() || colour_is_small(c) {
                continue;
            }
            // Step 2.
            let (l_dir, r, reach) = light_direction(version, light.place, p);
            // Step 3.
            let mut lambda = orders.lambda.dot(l_dir, n);
            let mut behind = false;
            if lambda < 0.0 {
                if !(t > 0.0) {
                    continue;
                }
                behind = true;
                lambda = -lambda;
                i *= t;
            }
            let a = falloff(version, light.switches, b5.range, r);
            // Step 4, as 5.0 only.
            // "Where the light falls off": a light with bit 0x400 is not tested again
            // (lighting-more.jsonl: more-043, more-068 ask its shadow with I·t small).
            if version == RenderAs::Bryce50 && light.switches & light_switch::NO_FALL_OFF == 0 {
                i *= a;
                if is_small(i) {
                    continue;
                }
            }
            // Step 5: the shadow.
            if s != 1.0
                && sc.switches & scene_switch::SHADOWS != 0
                && light.switches & light_switch::NO_SHADOWS == 0
                && point.flags & material_flag::TAKES_NO_SHADOWS == 0
            {
                asked += 1;
                // (Outputs: the shadow ray's direction, Δ brought to unit length in
                // the original's form, Last bits); the sun's its own direction.
                let direction = match light.place {
                    LightPlace::Sun { .. } => l_dir,
                    LightPlace::Radial { position } => {
                        shadow_direction(version, [position[0] - p[0], position[1] - p[1], position[2] - p[2]])
                    }
                };
                let query = ShadowQuery { light: index, point: p, geometric_normal: at.geometric_normal, normal: n, direction, reach, offset: true };
                let sh = shadows.shadow(&query);
                // Bryce 5's light in shadow, before the lifting.
                if version == RenderAs::Bryce50 && !is_small(b5.shadow_share) && !colour_is_small(b5.shadow_colour) {
                    let f = (1.0 - s) * (1.0 - sh.amount) * b5.shadow_share;
                    for (ok, psi) in o.iter_mut().zip(b5.shadow_colour) {
                        *ok = (f64::from(psi) * f + f64::from(*ok)) as f32;
                    }
                }
                match lift_shadow(version, s, i, c, sh) {
                    Some((ni, nc)) => (i, c) = (ni, nc),
                    None => continue,
                }
            }
            // Step 6: the diffuse term.
            let delta: [f64; 3] = std::array::from_fn(|k| {
                if kd > 0.0 { f64::from((f64::from(m.diffuse[k]) * lambda * kd) as f32) } else { 0.0 }
            });
            // Step 7: the highlight.
            let eta = if ks > 0.0 {
                let rho = if behind {
                    neg(l_dir)
                } else {
                    std::array::from_fn(|k| 2.0 * lambda * n[k] - l_dir[k])
                };
                highlight(version, orders, m, n, v, rho, c)
            } else {
                [0.0; 3]
            };
            // Step 8: the term added.
            for k in 0..3 {
                let sigma = eta[k] + delta[k];
                let ck = f64::from(c[k]);
                let term = match version {
                    RenderAs::Bryce30 | RenderAs::Bryce31 => ck * i * a * sigma,
                    RenderAs::Bryce50 => ck * i * sigma,
                };
                o[k] = (term + f64::from(o[k])) as f32;
            }
        }
    }
    // Bryce 5's gathered light is asked where its conditions hold.
    let gathered_asked = version == RenderAs::Bryce50
        && sc.switches & scene_switch::GATHERED_LIGHT != 0
        && !is_small(m.ambience)
        && !colour_is_small(m.ambient);
    LightTerms { sum: o, shading_normal: n, shadows_asked: asked, gathered_asked }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : the shadow ray's direction as 3.1 is (1/R(Δ·Δ))·Δ with the
    /// sum x last; as 5.0 Q(Δ·Δ)·Δ with the sum y last; Δ itself where Δ·Δ is 0.
    #[test]
    fn shadow_direction_forms() {
        let d: Vec3 = [0.3, -1.7, 2.9];
        let x_last = (d[1] * d[1] + d[2] * d[2]) + d[0] * d[0];
        let r = x_last.sqrt();
        assert_eq!(shadow_direction(RenderAs::Bryce31, d), [(1.0 / r) * d[0], (1.0 / r) * d[1], (1.0 / r) * d[2]]);
        let y_last = (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1];
        let q = 1.0 / y_last.sqrt();
        assert_eq!(shadow_direction(RenderAs::Bryce50, d), [q * d[0], q * d[1], q * d[2]]);
        assert_eq!(shadow_direction(RenderAs::Bryce30, [0.0; 3]), [0.0; 3]);
    }

    /// : as 5.0 a sun with the range bit and without 0x400 has a = 0 for a
    /// range of 0 or above (it adds nothing); with 0x400 the bit is not read.
    #[test]
    fn sun_with_the_range_bit() {
        let r = f64::INFINITY;
        let ranged = light_switch::ON | light_switch::RANGE_FALL_OFF;
        assert_eq!(falloff(RenderAs::Bryce50, ranged, 0.0, r), 0.0);
        assert_eq!(falloff(RenderAs::Bryce50, ranged, 5.0, r), 0.0);
        assert_eq!(falloff(RenderAs::Bryce50, ranged | light_switch::NO_FALL_OFF, 5.0, r), 1.0);
    }
}
