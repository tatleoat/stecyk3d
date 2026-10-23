// SPDX-License-Identifier: MIT
//! A light at a point: the lights with a beam and Bryce 5's gradients.
//!
//! (revised at: "The lights
//! with a beam", "Bryce 5's gradients", "The gradient at (t, o)"; revised at: a gel
//! with a gradient, the gradient first, then the gel's Diffuse colour)
//! (Revision of; Bryce 5's lights in the render)
//!
//! Every binary64 operation is one Rust `f64` operation; ⟨x⟩ of the entry is `x as f32`.

use crate::geom::Vec3;
use crate::numerics::{X87_INDEFINITE, pow, rsqrt_as, sqrt_as, sqrt_single_30};
use crate::shading::lighting::{SumOrder, light_switch};
use crate::shading::open::OpenItem;
use crate::version::RenderAs;

/// The cone's rim: the double nearest 0.8944271909999.
pub const CONE_RIM: f64 = 0.8944271909999;
/// 1/(1 − C): the double nearest 9.472135954998153.
pub const CONE_SCALE: f64 = 9.472135954998153;
/// The cone's apex height, and the cylinder's top.
const CONE_APEX: f64 = 1.0;
/// The pyramid's apex height, and the box's top and half width.
const PYRAMID_APEX: f64 = 0.5;

/// The gradient's place wraps at its ends (switch bit 0x2).
pub const GRADIENT_WRAP: u32 = 0x2; // [format]
/// The offset place wraps (switch bit 0x1).
pub const GRADIENT_OFFSET_WRAP: u32 = 0x1; // [format]

/// A light's beam, with its exponents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Beam {
    /// Light from a point within a cone (the spot light).
    Cone { exponent: f64 },
    /// Parallel light within a cylinder.
    Cylinder { exponent: f64 },
    /// Light from a point within a pyramid of square section.
    Pyramid { ex: f64, ez: f64 },
    /// Parallel light within a square prism (the entry's "box").
    Boxed { ex: f64, ez: f64 },
}

/// A colour key of a gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColourKey {
    pub colour: [f32; 3],
    pub place: f32,
    pub midpoint: f32,
}

/// A value key of a gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueKey {
    pub value: f32,
    pub place: f32,
    pub midpoint: f32,
}

/// A list of keys with its switch word.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyList<K> {
    pub switches: u32,
    pub keys: Vec<K>,
}

/// A light's gradient (as 5.0).
#[derive(Clone, Debug, PartialEq)]
pub struct LightGradient {
    /// g.
    pub range: f64,
    pub offset: f64,
    /// c0, the light's own colour ('s, before its material's Diffuse colour).
    pub own_colour: [f32; 3],
    /// D, the light's material's Diffuse colour.
    pub material_diffuse: [f32; 3],
    pub colours: KeyList<ColourKey>,
    pub values: KeyList<ValueKey>,
}

/// A light as reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct LightAtPoint {
    /// `None` for a radial light.
    pub beam: Option<Beam>,
    /// q, the light's place (its matrix's translation for a light with a beam).
    pub position: Vec3,
    /// c as gives it.
    pub colour: [f32; 3],
    /// I as stored.
    pub intensity: f64,
    pub switches: u32,
    /// Read as 5.0 where the switch word has bit 0x200000.
    pub gradient: Option<LightGradient>,
}

/// The light's c and I at p.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightAt {
    pub colour: [f32; 3],
    pub intensity: f64,
}

/// The order of the cone's w·w, per version.
pub struct ConeOrder;

impl ConeOrder {
    /// The order ` ` selects (tests/shading_lights.rs tries all three).
    pub fn of(version: RenderAs) -> SumOrder {
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 | RenderAs::Bryce50 => SumOrder::YzX,
        }
    }
}

/// The order of the gradient's Δ·Δ: z last, read in 5.0's code. (Bryce 5's
/// gradients)
const GRADIENT_DISTANCE_ORDER: SumOrder = SumOrder::XyZ;

/// P(x, y), with the x87 indefinite NaN where x < 0 and y is not whole.
fn beam_pow(x: f64, y: f64) -> f64 {
    if x < 0.0 && y.is_finite() && y.trunc() != y {
        return X87_INDEFINITE;
    }
    pow(x, y)
}

// The single exponent of a spot light or a cylinder: a number that is none acts as 0 (the
// original tests 0 first and a not-a-number is equal to it). (A beam exponent that
// is not a number)
fn single_exponent(e: f64) -> f64 {
    if e.is_nan() { 0.0 } else { e }
}

// The product P(x, ex)·P(z, ez) of the pyramid and the box: the light whole where each exponent
// is 0 or not a number, x·z where each is 1 or not a number, else the powers (a NaN exponent
// as given).
fn beam_pair(x: f64, ex: f64, z: f64, ez: f64) -> f64 {
    let zero = |e: f64| e == 0.0 || e.is_nan();
    let one = |e: f64| e == 1.0 || e.is_nan();
    if zero(ex) && zero(ez) {
        1.0
    } else if one(ex) && one(ez) {
        x * z
    } else {
        beam_pow(x, ex) * beam_pow(z, ez)
    }
}

fn single(x: f64) -> f64 {
    f64::from(x as f32)
}

/// The beam's factor φ at u (the point in the light's own space), `None` outside.
pub fn beam_factor(version: RenderAs, beam: Beam, u: Vec3, cone_order: SumOrder) -> Result<Option<f64>, OpenItem> {
    let [ux, uy, uz] = u;
    Ok(match beam {
        Beam::Cone { exponent } => {
            if uy > CONE_APEX {
                return Ok(None);
            }
            let w = [ux, uy - CONE_APEX, uz];
            let k = -(rsqrt_as(version, cone_order.dot(w, w)) * w[1]);
            // A k that is not a number is outside.
            if !(k >= CONE_RIM) {
                return Ok(None);
            }
            Some(beam_pow((k - CONE_RIM) * CONE_SCALE, single_exponent(exponent)))
        }
        Beam::Cylinder { exponent } => {
            if uy > CONE_APEX {
                return Ok(None);
            }
            let sum = uz * uz + ux * ux;
            let rho = match version {
                RenderAs::Bryce30 => sqrt_single_30(sum as f32),
                RenderAs::Bryce31 => sqrt_as(version, sum),
                RenderAs::Bryce50 => sqrt_as(version, sum as f32 as f64),
            };
            if rho > 1.0 {
                return Ok(None);
            }
            Some(beam_pow(1.0 - rho * rho, single_exponent(exponent)))
        }
        Beam::Pyramid { ex, ez } => {
            if uy > PYRAMID_APEX {
                return Ok(None);
            }
            let h = uy - PYRAMID_APEX;
            let m = -0.5 * h;
            // Outside unless |ux| ≤ m (a comparison with a number that is none: outside).
            if !(ux.abs() <= m) {
                return Ok(None);
            }
            let (ax, az) = (single(ux.abs()), single(uz.abs()));
            if az > m {
                return Ok(None);
            }
            let a = 2.0 * (ax / -h);
            let b = 2.0 * (az / -h);
            Some(beam_pair(1.0 - a * a, ex, 1.0 - b * b, ez))
        }
        Beam::Boxed { ex, ez } => {
            let (ax, az) = (single(ux.abs()), single(uz.abs()));
            if uy > PYRAMID_APEX || ax > PYRAMID_APEX || az > PYRAMID_APEX {
                return Ok(None);
            }
            let a = 2.0 * ax;
            let b = 2.0 * az;
            Some(beam_pair(1.0 - a * a, ex, 1.0 - b * b, ez))
        }
    })
}

/// What steps 1 and 2 need of a key.
trait GradientKey {
    fn place(&self) -> f64;
    fn midpoint(&self) -> f64;
}

impl GradientKey for ColourKey {
    fn place(&self) -> f64 {
        f64::from(self.place)
    }
    fn midpoint(&self) -> f64 {
        f64::from(self.midpoint)
    }
}

impl GradientKey for ValueKey {
    fn place(&self) -> f64 {
        f64::from(self.place)
    }
    fn midpoint(&self) -> f64 {
        f64::from(self.midpoint)
    }
}

/// Where t falls among the keys.
enum Span {
    /// Keys a and b (indices) with the weight W.
    Between(usize, usize, f64),
    First,
    Last,
    /// The colour or value stays as it started.
    Stays,
}

/// Step 1: the place, from t and o.
fn place<K: GradientKey>(list: &KeyList<K>, t: f32, o: f32) -> f32 {
    let f = list.keys[0].place();
    let g = list.keys[list.keys.len() - 1].place();
    let wrap = list.switches & GRADIENT_WRAP != 0;
    let mut t = f64::from(t);
    if !(t >= f) {
        t = if wrap { single((t - f) % (g - f) + g) } else { f };
    }
    if t > g {
        t = if wrap { single((t - g) % (g - f) + f) } else { g };
    }
    let o = f64::from(o);
    if o != 0.0 && !o.is_nan() {
        t = single(o % (g - f) + t);
        if list.switches & GRADIENT_OFFSET_WRAP != 0 {
            if !(t >= f) {
                t = single((t - f) + g);
            }
            if t > g {
                t = single((t - g) + f);
            }
        } else {
            // Held to F, then to G, one after the other (keys out of order, F > G, can take
            // both:, grad-061).
            if !(t >= f) {
                t = f;
            }
            if t > g {
                t = g;
            }
        }
    }
    t as f32
}

/// Step 2 (and 4): the span holding t and its weight.
fn span<K: GradientKey>(keys: &[K], t: f32) -> Span {
    let t = f64::from(t);
    for (i, pair) in keys.windows(2).enumerate() {
        let (pa, pb) = (pair[0].place(), pair[1].place());
        if pa <= t && t <= pb {
            let mu = pair[1].midpoint();
            let m = (pb - pa) * mu + pa;
            let w = if t < m {
                if m - pa == 0.0 { 0.0 } else { ((t - pa) / (m - pa)) * 0.5 }
            } else if pb - m == 0.0 {
                1.0
            } else {
                ((t - m) / (pb - m) + 1.0) * 0.5
            };
            return Span::Between(i, i + 1, w);
        }
    }
    let f = keys[0].place();
    let g = keys[keys.len() - 1].place();
    if !(t >= f) {
        Span::First
    } else if t > g {
        Span::Last
    } else {
        Span::Stays
    }
}

/// The gradient's colour at (t, o), starting from `start`.
fn gradient_colour(list: &KeyList<ColourKey>, t: f32, o: f32, start: [f32; 3]) -> [f32; 3] {
    if list.keys.len() < 2 {
        return start;
    }
    let t = place(list, t, o);
    match span(&list.keys, t) {
        Span::Between(a, b, w) => {
            let (a, b) = (list.keys[a].colour, list.keys[b].colour);
            let w = single(w);
            std::array::from_fn(|k| {
                let d = single(f64::from(b[k]) - f64::from(a[k]));
                (f64::from(a[k]) + single(d * w)) as f32
            })
        }
        Span::First => list.keys[0].colour,
        Span::Last => list.keys[list.keys.len() - 1].colour,
        Span::Stays => start,
    }
}

/// The gradient's value at (t, o), starting from `start`.
fn gradient_value(list: &KeyList<ValueKey>, t: f32, o: f32, start: f32) -> f32 {
    if list.keys.len() < 2 {
        return start;
    }
    let t = place(list, t, o);
    match span(&list.keys, t) {
        Span::Between(a, b, w) => {
            let (va, vb) = (f64::from(list.keys[a].value), f64::from(list.keys[b].value));
            ((vb - va) * w + va) as f32
        }
        Span::First => list.keys[0].value,
        Span::Last => list.keys[list.keys.len() - 1].value,
        Span::Stays => start,
    }
}

/// The gradient's colour and value at the place t and offset o, the colour starting from
/// `start` and the value from 1. (The gradient at (t, o); vectors
/// `gradient.jsonl`, kind `gradient`)
pub fn gradient_at(colours: &KeyList<ColourKey>, values: &KeyList<ValueKey>, t: f32, o: f32, start: [f32; 3]) -> ([f32; 3], f32) {
    (gradient_colour(colours, t, o, start), gradient_value(values, t, o, 1.0))
}

/// The light's colour c and intensity I at the world point p, whose place in the light's
/// own space is u (p carried by the inverse of the light's matrix,; for a radial
/// light u is not read).
pub fn light_at(version: RenderAs, light: &LightAtPoint, p: Vec3, u: Vec3) -> Result<LightAt, OpenItem> {
    light_at_ordered(version, ConeOrder::of(version), light, p, u)
}

/// `light_at` with the cone's order of w·w given (the vector test tries each).
pub fn light_at_ordered(
    version: RenderAs,
    cone_order: SumOrder,
    light: &LightAtPoint,
    p: Vec3,
    u: Vec3,
) -> Result<LightAt, OpenItem> {
    let mut intensity = light.intensity;
    if let Some(beam) = light.beam {
        match beam_factor(version, beam, u, cone_order)? {
            Some(phi) => intensity *= phi,
            // Outside: the colour is c and I is 0 (the gradient is not used).
            None => return Ok(LightAt { colour: light.colour, intensity: 0.0 }),
        }
    }
    let gradient = match &light.gradient {
        Some(g) if version == RenderAs::Bryce50 && light.switches & light_switch::GRADIENT != 0 => g,
        _ => return Ok(LightAt { colour: light.colour, intensity }),
    };
    let before = gradient_light(version, gradient, light.position, p, intensity);
    Ok(LightAt { colour: tinted_where_lit(before, gradient.material_diffuse), intensity: before.intensity })
}

/// The gradient's colour and strength at p, before the Diffuse colour D is applied: the
/// colour starts from c0 and, where g is not 0 and is a number, is the gradient's colour at
/// (t, o) and I becomes ⟨v⟩·I (`intensity` is I as the beam left it). (Bryce 5's
/// gradients; Gels: "the gradient acts first, on c0 and I")
pub fn gradient_light(version: RenderAs, gradient: &LightGradient, q: Vec3, p: Vec3, intensity: f64) -> LightAt {
    let mut colour = gradient.own_colour;
    let mut intensity = intensity;
    let g = gradient.range;
    if g != 0.0 && !g.is_nan() {
        let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let l = sqrt_as(version, GRADIENT_DISTANCE_ORDER.dot(d, d));
        let t = (l / g) as f32;
        let o = gradient.offset as f32;
        colour = gradient_colour(&gradient.colours, t, o, colour);
        let v = gradient_value(&gradient.values, t, o, 1.0);
        intensity *= f64::from(v);
    }
    LightAt { colour, intensity }
}

/// Where I is not 0 (and is a number), each channel ⟨Dk·ck⟩; else the colour as it is.
pub fn tinted_where_lit(light: LightAt, diffuse: [f32; 3]) -> [f32; 3] {
    if light.intensity != 0.0 && !light.intensity.is_nan() {
        std::array::from_fn(|k| (f64::from(diffuse[k]) * f64::from(light.colour[k])) as f32)
    } else {
        light.colour
    }
}
