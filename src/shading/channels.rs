// SPDX-License-Identifier: MIT
//! A material's thirteen channels at a hit, from its A, B and alpha values and the results of
//! the textures its references name; and the bump that tilts the shading normal.
//!
//! (A material mixed with a
//! second one),,
//! (Material channels at a hit; A material mixed with a second
//! one)
//!
//! Every binary64 operation is one Rust `f64` operation; ⟨x⟩ is `x as f32`; a sum or product
//! of two singles written ⟨a ± b⟩ or ⟨a·b⟩ is the Rust `f32` operation (the same bits).

use crate::geom::Vec3;
use crate::numerics::{exp2_as, rsqrt_as};
use crate::shading::lighting::SumOrder;
use crate::version::RenderAs;

/// K, the double nearest the single nearest 1/255: a texture's 8-bit level times K.
pub const LEVEL_SCALE: f64 = 0.003921568859368563;
/// The scene's switch bit that turns textures on.
pub const TEXTURES_ON: u32 = 0x200;
/// A channel's mix by height and by alpha.
const MIX_HEIGHT: u32 = 0x2;
const MIX_ALPHA: u32 = 0x4;
/// The material's flag bit for a bump that fades with distance.
pub const BUMP_FADES: u32 = 0x10;
/// Below this |hb| the bump changes nothing.
const BUMP_LEAST_HEIGHT: f64 = 9.999999974752427e-07;
/// The strength's factor on hb.
const BUMP_STRENGTH: f64 = 20.0;
/// The fade's constants, for a procedural texture and for a picture.
const FADE_PROCEDURAL: f64 = 0.00016;
const FADE_PICTURE: f64 = 0.0004;
/// log2 e.
const LOG2_E: f64 = std::f64::consts::LOG2_E;
/// Below this |s| the normal is unchanged.
const BUMP_LEAST_STRENGTH: f64 = 1e-10;

/// The colour channels and the value channels, by's numbers.
pub const COLOUR_CHANNELS: usize = 5;
pub const VALUE_CHANNELS: usize = 8;

/// What a texture gives at the hit, and what reads of its mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureAtHit {
    /// Three 8-bit levels.
    pub colour: [u8; 3],
    pub value: f64,
    /// The bump gradient.
    pub bump: [f64; 3],
    /// The mapping's x scale .
    pub scale_x: f64,
    /// A procedural texture (else a picture texture).
    pub procedural: bool,
}

/// One channel as the material holds it: its switch word, its A, B and alpha values, and its
/// three references (A, B, alpha) as indices into the textures' results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelSource<T> {
    pub switches: u32,
    pub a: T,
    pub b: T,
    pub alpha: f32,
    pub textures: [Option<usize>; 3],
}

impl<T> ChannelSource<T> {
    fn has_texture(&self) -> bool {
        self.textures.iter().any(Option::is_some)
    }
}

/// A material's channels.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialChannels {
    pub flags: u32,
    /// Diffuse, Ambient, Specular, Specular Halo, Transparent colour (channels 0 to 4).
    pub colours: [ChannelSource<[f32; 3]>; COLOUR_CHANNELS],
    /// Diffusion, Ambience, Specularity, Transparency, Reflection, Bump Height,
    /// Metallicity, Volume density (channels 5 to 12).
    pub values: [ChannelSource<f32>; VALUE_CHANNELS],
}

/// The hit as read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelHit {
    /// y, the hit's height in world space.
    pub height: f64,
    /// Y, the scene's top.
    pub top: f64,
    /// The shading normal before the bump.
    pub normal: Vec3,
    /// The distance from the ray's origin to the hit.
    pub distance: f64,
}

/// The quantities shading reads.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialAtHit {
    /// Whether the channels were evaluated (else every channel is its A value).
    pub evaluated: bool,
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub specular_halo: [f32; 3],
    pub transparent_colour: [f32; 3],
    pub diffusion: f64,
    pub ambience: f64,
    pub specularity: f64,
    pub transparency: f64,
    pub reflection: f64,
    pub metallicity: f64,
    pub volume_density: f64,
    /// The shading normal after.
    pub normal: Vec3,
}

/// The order of the bump's sum of three squares, which ` `
/// decides per version (tests/shading_channels.rs tries all three).
pub fn bump_order(version: RenderAs) -> SumOrder {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => SumOrder::YzX,
        RenderAs::Bryce50 => SumOrder::XyZ,
    }
}

/// h(x): 0 below 0, 1 above 1; a number that is none is itself as 3.0, 0 as 3.1 and 5.0.
fn hold(version: RenderAs, x: f64) -> f64 {
    if x.is_nan() {
        return if version == RenderAs::Bryce30 { x } else { 0.0 };
    }
    x.clamp(0.0, 1.0)
}

/// Held to 0 to 1 with a number that is none passed on (step 1).
fn hold_passing(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn level(c: u8) -> f32 {
    (f64::from(c) * LEVEL_SCALE) as f32
}

/// Whether the channels are evaluated at a hit: textures on, and some channel with both a
/// switch word that is not 0 and a texture reference; else every channel is its A value.
/// `channels` gives each channel's switch word and whether it has a reference, in channel
/// order.
pub fn channels_evaluated(channels: impl IntoIterator<Item = (u32, bool)>, scene_switches: u32) -> bool {
    scene_switches & TEXTURES_ON != 0 && channels.into_iter().any(|(w, textured)| w != 0 && textured)
}

/// The channels at a hit, and the shading normal after the bump.
pub fn evaluate_channels(
    version: RenderAs,
    m: &MaterialChannels,
    textures: &[TextureAtHit],
    hit: &ChannelHit,
    scene_switches: u32,
) -> MaterialAtHit {
    evaluate_channels_ordered(version, m, textures, hit, scene_switches, bump_order(version))
}

/// `evaluate_channels` with the bump's order of squares given (for the vector test).
pub fn evaluate_channels_ordered(
    version: RenderAs,
    m: &MaterialChannels,
    textures: &[TextureAtHit],
    hit: &ChannelHit,
    scene_switches: u32,
    order: SumOrder,
) -> MaterialAtHit {
    let mut out = a_values(m, hit);
    // : textures on, and some channel with a
    // switch word that is not 0 and a texture reference.
    let (switches, textured) = words_and_references(m);
    let first = (0..ALL_CHANNELS).find(|&i| switches[i] != 0 && textured[i]);
    let Some(first) = first.filter(|_| scene_switches & TEXTURES_ON != 0) else {
        return out;
    };
    out.evaluated = true;
    // A channel with no texture reference and a word that is not 0 before the first channel
    // that has both is taken as having the word 0.
    let word = |i: usize| if i < first && !textured[i] { 0 } else { switches[i] };
    let alpha_ratio = hit.height / hit.top;
    let ctx = Ctx { version, textures, alpha_ratio };
    // Colour channels 0 to 4.
    for (i, c) in m.colours.iter().enumerate() {
        let w = word(i);
        if !(w != 0 || (version == RenderAs::Bryce30 && c.textures[0].is_some())) {
            continue;
        }
        let v = ctx.colour_channel(c, w);
        match i {
            0 => out.diffuse = v,
            1 => out.ambient = v,
            2 => out.specular = v,
            3 => out.specular_halo = v,
            _ => out.transparent_colour = v,
        }
    }
    // Value channels 5 to 12, in channel order.
    for (j, c) in m.values.iter().enumerate() {
        let i = COLOUR_CHANNELS + j;
        let w = word(i);
        if !(w != 0 || (version == RenderAs::Bryce30 && c.textures[0].is_some())) {
            continue;
        }
        if j == 5 {
            // Channel 10, Bump Height.
            out.normal = ctx.bump(c, w, None, m.flags, hit, order);
            continue;
        }
        let x = ctx.value_channel(c, w);
        let a = f64::from(c.a);
        match j {
            3 => transparency(&mut out, x, a),
            4 => reflection(version, &mut out, x, a),
            7 => {
                let v = a + (1.0 - x);
                out.volume_density = if v > 1.0 || v.is_nan() { 1.0 } else { v };
            }
            _ => set_value(&mut out, j, x),
        }
    }
    out
}

/// Every channel its A value; not evaluated.
fn a_values(m: &MaterialChannels, hit: &ChannelHit) -> MaterialAtHit {
    MaterialAtHit {
        evaluated: false,
        diffuse: m.colours[0].a,
        ambient: m.colours[1].a,
        specular: m.colours[2].a,
        specular_halo: m.colours[3].a,
        transparent_colour: m.colours[4].a,
        diffusion: f64::from(m.values[0].a),
        ambience: f64::from(m.values[1].a),
        specularity: f64::from(m.values[2].a),
        transparency: f64::from(m.values[3].a),
        reflection: f64::from(m.values[4].a),
        metallicity: f64::from(m.values[6].a),
        volume_density: f64::from(m.values[7].a),
        normal: hit.normal,
    }
}

/// The thirteen channels.
const ALL_CHANNELS: usize = COLOUR_CHANNELS + VALUE_CHANNELS;

/// Each channel's switch word and whether it has a texture reference, in channel order.
fn words_and_references(m: &MaterialChannels) -> ([u32; ALL_CHANNELS], [bool; ALL_CHANNELS]) {
    let word = |i: usize| if i < COLOUR_CHANNELS { m.colours[i].switches } else { m.values[i - COLOUR_CHANNELS].switches };
    let refs = |i: usize| {
        if i < COLOUR_CHANNELS { m.colours[i].has_texture() } else { m.values[i - COLOUR_CHANNELS].has_texture() }
    };
    (std::array::from_fn(word), std::array::from_fn(refs))
}

fn set_colour(out: &mut MaterialAtHit, i: usize, v: [f32; 3]) {
    match i {
        0 => out.diffuse = v,
        1 => out.ambient = v,
        2 => out.specular = v,
        3 => out.specular_halo = v,
        _ => out.transparent_colour = v,
    }
}

/// Channels 5, 6, 7 and 11 (value channels 0, 1, 2 and 6) take x as it is.
fn set_value(out: &mut MaterialAtHit, j: usize, x: f64) {
    match j {
        0 => out.diffusion = x,
        1 => out.ambience = x,
        2 => out.specularity = x,
        6 => out.metallicity = x,
        _ => unreachable!("channels 8, 9, 10 and 12 have their own rules"),
    }
}

fn transparency(out: &mut MaterialAtHit, x: f64, a: f64) {
    let t = (1.0 - x) + a;
    out.transparency = if t > 1.0 || t.is_nan() { 1.0 } else { t };
}

fn reflection(version: RenderAs, out: &mut MaterialAtHit, x: f64, a: f64) {
    out.reflection = if a > 0.0 { a * x } else { 0.0 };
    match version {
        RenderAs::Bryce30 => {
            let ks = out.diffusion - x;
            out.specularity = if ks < 0.0 || ks.is_nan() { 0.0 } else { ks };
        }
        RenderAs::Bryce31 | RenderAs::Bryce50 => {
            let ks = x + out.specularity;
            out.specularity = if ks > 1.0 || ks.is_nan() { 1.0 } else { ks };
            let kd = out.diffusion - x;
            out.diffusion = if kd < 0.0 || kd.is_nan() { 0.0 } else { kd };
        }
    }
}

/// A second material's channel and its mix amount m. (A material mixed with a
/// second one: a second channel and an amount m, a double)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mixed<T> {
    pub channel: ChannelSource<T>,
    pub amount: f64,
}

/// The second material: for each channel mixed in, its second channel (references into the
/// same texture results as the material's own); `None` for a channel with m = 0 and no second
/// channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SecondMaterial {
    pub colours: [Option<Mixed<[f32; 3]>>; COLOUR_CHANNELS],
    pub values: [Option<Mixed<f32>>; VALUE_CHANNELS],
}

impl SecondMaterial {
    /// Whether no channel is mixed (a material without a second one).
    pub fn is_empty(&self) -> bool {
        self.colours.iter().all(Option::is_none) && self.values.iter().all(Option::is_none)
    }
}

/// The channels at a hit of a material mixed with a second one, and the shading normal after
/// the bump; a material whose second one mixes no channel is `evaluate_channels`'s.
pub fn evaluate_mixed_channels(
    version: RenderAs,
    m: &MaterialChannels,
    second: &SecondMaterial,
    textures: &[TextureAtHit],
    hit: &ChannelHit,
    scene_switches: u32,
) -> MaterialAtHit {
    evaluate_mixed_channels_ordered(version, m, second, textures, hit, scene_switches, bump_order(version))
}

/// `evaluate_mixed_channels` with the bump's order of squares given.
pub fn evaluate_mixed_channels_ordered(
    version: RenderAs,
    m: &MaterialChannels,
    second: &SecondMaterial,
    textures: &[TextureAtHit],
    hit: &ChannelHit,
    scene_switches: u32,
    order: SumOrder,
) -> MaterialAtHit {
    if second.is_empty() {
        return evaluate_channels_ordered(version, m, textures, hit, scene_switches, order);
    }
    if scene_switches & TEXTURES_ON == 0 {
        return a_values(m, hit);
    }
    // The pairs: own channels 10, 11, 12 with the second material's 0, 1, 2. The bump keeps
    // its own A texture's x scale and kind.
    let bump_kind = m.values[5].textures[0];
    let mut own = *m;
    for k in 0..3 {
        let Some(paired) = &second.colours[k] else { continue };
        let slots = &mut own.values[5 + k].textures;
        for s in 0..3 {
            if slots[s].is_some() && paired.channel.textures[s].is_some() {
                slots[s] = paired.channel.textures[s];
            }
        }
    }
    let ctx = Ctx { version, textures, alpha_ratio: hit.height / hit.top };
    // : no channel has its switch word taken as 0 by the rule
    // of a material with no second one; every word counts as the material holds it.
    let (switches, _) = words_and_references(m);
    mixed_pass(&ctx, &own, second, &switches, bump_kind, hit, order)
}

/// One evaluation of a mixed material with the own channels' switch words `words`.
fn mixed_pass(
    ctx: &Ctx<'_>,
    own: &MaterialChannels,
    second: &SecondMaterial,
    words: &[u32; ALL_CHANNELS],
    bump_kind: Option<usize>,
    hit: &ChannelHit,
    order: SumOrder,
) -> MaterialAtHit {
    let version = ctx.version;
    let mut out = a_values(own, hit);
    out.evaluated = true;
    // Taken where the word is not 0, or as 3.0 where A is a texture.
    let taken = |w: u32, textures: &[Option<usize>; 3]| w != 0 || (version == RenderAs::Bryce30 && textures[0].is_some());
    for (i, c) in own.colours.iter().enumerate() {
        let w = words[i];
        let v = match &second.colours[i] {
            // : the Diffuse colour (channel 0) takes its rule
            // for m = 0 where <m> is 0, every other channel where m itself is 0 (a number that is
            // none counts as 0)
            Some(mx) if counts_as_mixed(i == 0, mx.amount) => {
                let x = ctx.colour_channel(c, w);
                let y = ctx.colour_channel(&mx.channel, mx.channel.switches);
                let mf = mx.amount as f32;
                let red = f64::from(x[0]) + (f64::from(y[0]) - f64::from(x[0])) * f64::from(mf);
                [red as f32, mf * (y[1] - x[1]) + x[1], mf * (y[2] - x[2]) + x[2]]
            }
            _ if taken(w, &c.textures) => ctx.colour_channel(c, w),
            _ => continue,
        };
        set_colour(&mut out, i, v);
    }
    for (j, c) in own.values.iter().enumerate() {
        let w = words[COLOUR_CHANNELS + j];
        let mixed = second.values[j].as_ref();
        let m = mixed.map_or(0.0, |mx| mx.amount);
        let mf = f64::from(m as f32);
        // : Bump Height and Metallicity (channels 10 and 11)
        // by <m>, every other value channel by m; a number that is none counts as 0
        let mixes = counts_as_mixed(j == 5 || j == 6, m);
        let mix = |x: f64, y: f64| (y - x) * mf + x;
        let y = || {
            let mx = mixed.expect("a second channel where m is not 0");
            ctx.value_channel(&mx.channel, mx.channel.switches)
        };
        let a = f64::from(c.a);
        match j {
            3 | 4 => {
                let second_taken = mixed.is_some_and(|mx| taken(mx.channel.switches, &mx.channel.textures));
                if !(taken(w, &c.textures) || second_taken) {
                    continue;
                }
                // (Transparency and Reflection, m = 0 with the own switch word 0 and the
                // second channel's not): no x is formed; as 5.0 the channel's p stands for x (for
                // Transparency 1 - A8, for Reflection A9), as 3.0 and 3.1 a number the program does
                // not set (the implementer's choice: the A source)
                let x = if !mixes && !taken(w, &c.textures) && version == RenderAs::Bryce50 {
                    if j == 3 { 1.0 - a } else { a }
                } else {
                    ctx.value_channel(c, w)
                };
                let v = if !mixes { x } else { mix(x, y()) };
                if j == 3 { transparency(&mut out, v, a) } else { reflection(version, &mut out, v, a) }
            }
            5 => {
                if mixes {
                    let mx = mixed.expect("a second channel where m is not 0");
                    let n1 = ctx.bump(c, w, bump_kind, own.flags, hit, order);
                    let n2 = ctx.bump(&mx.channel, mx.channel.switches, None, own.flags, hit, order);
                    out.normal = std::array::from_fn(|k| (n2[k] - n1[k]) * mf + n1[k]);
                } else if taken(w, &c.textures) {
                    out.normal = ctx.bump(c, w, bump_kind, own.flags, hit, order);
                }
            }
            7 if version == RenderAs::Bryce50 => {
                // : where the own channel's switch word is not 0
                // or the second material's channel 12 has one, the textures are evaluated as for 3.1
                // but what they give is not kept: the density is (1 - (1 - A12)) + A12 in binary64,
                // or 1 where that is not below 1 or is not a number; else the A value
                let second_word = mixed.map_or(0, |mx| mx.channel.switches);
                if w != 0 || second_word != 0 {
                    let d = (1.0 - (1.0 - a)) + a;
                    out.volume_density = if d < 1.0 { d } else { 1.0 };
                }
            }
            7 => {
                if mixes {
                    out.volume_density = mix(ctx.value_channel(c, w), y());
                } else if taken(w, &c.textures) {
                    out.volume_density = ctx.value_channel(c, w);
                }
            }
            _ => {
                if mixes {
                    set_value(&mut out, j, mix(ctx.value_channel(c, w), y()));
                } else if taken(w, &c.textures) {
                    set_value(&mut out, j, ctx.value_channel(c, w));
                }
            }
        }
    }
    out
}

/// Whether a channel takes the rule for a second material's amount that is not 0: for the Diffuse
/// colour, Bump Height and Metallicity (`by_single`) where the amount's binary32 is not 0, for every
/// other channel where the amount itself is not 0; an amount that is not a number counts as 0.
fn counts_as_mixed(by_single: bool, m: f64) -> bool {
    if m.is_nan() {
        return false;
    }
    if by_single { (m as f32) != 0.0 } else { m != 0.0 }
}

struct Ctx<'a> {
    version: RenderAs,
    textures: &'a [TextureAtHit],
    /// α = y / Y.
    alpha_ratio: f64,
}

impl Ctx<'_> {
    fn texture(&self, r: Option<usize>) -> Option<&TextureAtHit> {
        r.map(|i| &self.textures[i])
    }

    fn colour_channel(&self, c: &ChannelSource<[f32; 3]>, w: u32) -> [f32; 3] {
        let src = |r: Option<usize>, own: [f32; 3]| self.texture(r).map_or(own, |t| t.colour.map(level));
        let a = src(c.textures[0], c.a);
        if w & (MIX_HEIGHT | MIX_ALPHA) == 0 {
            return a;
        }
        let b = src(c.textures[1], c.b);
        let d64 = |k: usize| f64::from(b[k]) - f64::from(a[k]);
        if w & MIX_HEIGHT != 0 {
            let wt = hold(self.version, self.alpha_ratio * self.alpha_ratio) as f32;
            return std::array::from_fn(|k| wt * (b[k] - a[k]) + a[k]);
        }
        let alpha_tex = self.texture(c.textures[2]);
        let alpha = alpha_tex.map_or(f64::from(c.alpha), |t| hold(self.version, t.value));
        match (self.version, alpha_tex.is_some()) {
            (RenderAs::Bryce30, _) => [
                ((alpha * d64(0)) as f32) + a[0],
                ((alpha * f64::from(b[1] - a[1])) as f32) + a[1],
                (alpha * f64::from(b[2] - a[2]) + f64::from(a[2])) as f32,
            ],
            (_, false) => std::array::from_fn(|k| ((alpha * f64::from(b[k] - a[k])) as f32) + a[k]),
            (RenderAs::Bryce31, true) => std::array::from_fn(|k| ((alpha * d64(k)) as f32) + a[k]),
            (RenderAs::Bryce50, true) => std::array::from_fn(|k| (alpha as f32) * (b[k] - a[k]) + a[k]),
        }
    }

    /// x of a value channel.
    fn value_channel(&self, c: &ChannelSource<f32>, w: u32) -> f64 {
        let src = |r: Option<usize>, own: f32| self.texture(r).map_or(f64::from(own), |t| t.value);
        let a = src(c.textures[0], c.a);
        if w & (MIX_HEIGHT | MIX_ALPHA) == 0 {
            return a;
        }
        let b = src(c.textures[1], c.b);
        let lambda = if w & MIX_HEIGHT != 0 {
            self.alpha_ratio * self.alpha_ratio
        } else {
            src(c.textures[2], c.alpha)
        };
        (b - a) * hold(self.version, lambda) + a
    }

    /// The shading normal after the bump. `kind` is the texture whose x scale and kind the
    /// fade takes: the channel's own A texture, which a pair of's mixed material keeps
    /// while it reads another's results (`None`: the A texture read).
    /// (A material mixed with a second one)
    fn bump(
        &self,
        c: &ChannelSource<f32>,
        w: u32,
        kind: Option<usize>,
        flags: u32,
        hit: &ChannelHit,
        order: SumOrder,
    ) -> Vec3 {
        let n = hit.normal;
        let hb = f64::from(c.a);
        let Some(read) = self.texture(c.textures[0]) else { return n };
        let at = self.texture(kind).unwrap_or(read);
        if hb.abs() < BUMP_LEAST_HEIGHT {
            return n;
        }
        // Step 1.
        let mut g = read.bump;
        if w & (MIX_HEIGHT | MIX_ALPHA) != 0 {
            let b = self.texture(c.textures[1]).map_or([0.0; 3], |t| t.bump);
            let lambda = if w & MIX_ALPHA != 0 {
                self.texture(c.textures[2]).map_or(0.0, |t| hold_passing(t.value))
            } else {
                hold_passing(self.alpha_ratio * self.alpha_ratio)
            };
            g = std::array::from_fn(|k| (b[k] - g[k]) * lambda + g[k]);
        }
        // Step 2.
        let mut s = hb * BUMP_STRENGTH;
        if flags & BUMP_FADES != 0 {
            let k = if at.procedural { FADE_PROCEDURAL } else { FADE_PICTURE };
            let f = at.scale_x * hit.distance * k;
            let f = if f > 0.0 { f } else { 0.0 };
            s *= exp2_as(self.version, -f * LOG2_E);
        }
        // Step 3.
        if s.abs() < BUMP_LEAST_STRENGTH {
            return n;
        }
        // Step 4.
        let np: Vec3 = std::array::from_fn(|k| n[k] - s * g[k]);
        let q = rsqrt_as(self.version, order.dot(np, np));
        np.map(|x| x * q)
    }
}
