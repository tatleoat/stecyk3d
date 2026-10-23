// SPDX-License-Identifier: MIT
//! Material channels and their evaluation at a hit.
//! (material.rs)
//!
//! Fixed by: thirteen channels; each a constant, a texture, or a blend of two
//! textures by a third texture's value; a material may mix two materials channel by
//! channel; evaluating a texture updates its carried state .
//! Answers 0302 and 0305: up to four texture components A to D shared by the channels;
//! modes A, AB (by altitude) and ABC (A where C is 1, B where C is 0).
//! Open (parameters here): channel names, kinds and ranges; the blend rules; the mix rule;
//! which evaluation orders are fixed.

use crate::geom::{Rgb, Vec3};

use super::open::{self, OpenItem};

/// Number of channels in a material.
pub const CHANNEL_COUNT: usize = 13;

/// A channel by position, `0..CHANNEL_COUNT`. Channels have no names here: the names and
/// ranges are Open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelSlot(u8);

impl ChannelSlot {
    pub fn new(index: usize) -> Option<ChannelSlot> {
        if index < CHANNEL_COUNT { Some(ChannelSlot(index as u8)) } else { None }
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// All slots in ascending order.
    pub fn all() -> impl Iterator<Item = ChannelSlot> {
        (0..CHANNEL_COUNT).map(|i| ChannelSlot(i as u8))
    }

    /// The channel's documented name.
    pub fn name(self) -> &'static str {
        CHANNEL_NAMES[self.index()]
    }

    /// Whether the channel holds a colour or a single number.
    pub fn kind(self) -> ChannelKind {
        if self.index() < COLOUR_CHANNELS { ChannelKind::Colour } else { ChannelKind::Scalar }
    }

    /// The channel with a documented name.
    pub fn named(name: &str) -> Option<ChannelSlot> {
        CHANNEL_NAMES.iter().position(|&n| n == name).map(|i| ChannelSlot(i as u8))
    }

    /// The channel's range, and what a value outside it gives: Open (answer 0301).
    pub fn range(self) -> Result<(f64, f64), OpenItem> {
        Err(open::CHANNEL_LIST)
    }
}

/// The thirteen channels in the user guide's order; the first five are colours. The order
/// names them only (answer 0301: it says nothing about storage or evaluation).
pub const CHANNEL_NAMES: [&str; CHANNEL_COUNT] = [
    "Diffuse Color",
    "Ambient Color",
    "Specular Color",
    "Transparent Color",
    "Specular Halo",
    "Diffusion",
    "Ambience",
    "Specularity",
    "Metallicity",
    "Bump Height",
    "Transparency",
    "Reflection",
    "Base Density",
];

/// Number of colour channels at the front of `CHANNEL_NAMES`.
const COLOUR_CHANNELS: usize = 5;

/// A texture's output as a channel's value: its colour for a colour channel, its value
/// (alpha) for a single-number channel.
pub fn texture_to_channel(slot: ChannelSlot, sample: TextureSample) -> ChannelValue {
    match slot.kind() {
        ChannelKind::Colour => ChannelValue::Colour(sample.colour),
        ChannelKind::Scalar => ChannelValue::Scalar(sample.value),
    }
}

/// Whether a channel holds a colour or a single value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Colour,
    Scalar,
}

/// A channel's value at a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChannelValue {
    Colour(Rgb),
    Scalar(f64),
}

/// A reference to a texture of the `textures` component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

/// Number of texture components a material holds.
pub const COMPONENT_COUNT: usize = 4;

/// One of a material's texture components, by its letter. A channel that names a component
/// uses the same texture as every other channel naming it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Component {
    A,
    B,
    C,
    D,
}

impl Component {
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Where a channel's value comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChannelSource {
    /// Mode A with a value or a colour.
    Constant(ChannelValue),
    /// Mode A with one texture.
    Texture(Component),
    /// Mode AB: two textures blended by altitude (rule Open).
    ByAltitude { first: Component, second: Component },
    /// Mode ABC: `first` where `by`'s value is 1, `second` where it is 0, a combination
    /// between (answer 0302; the combination itself is Open).
    Blend { first: Component, second: Component, by: Component },
}

impl ChannelSource {
    /// The components the source reads, in evaluation order.
    pub fn components(&self) -> Vec<Component> {
        match *self {
            ChannelSource::Constant(_) => vec![],
            ChannelSource::Texture(a) => vec![a],
            ChannelSource::ByAltitude { first, second } => vec![first, second],
            ChannelSource::Blend { first, second, by } => vec![first, second, by],
        }
    }
}

/// A channel names a component the material does not hold. No document saved by the
/// programs holds one (refused when the material is built).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingComponent {
    pub slot: ChannelSlot,
    pub component: Component,
}

/// One material: up to four texture components and a source per channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    components: [Option<TextureId>; COMPONENT_COUNT],
    channels: [ChannelSource; CHANNEL_COUNT],
}

impl Material {
    /// Builds a material, checking that every component a channel names is held.
    pub fn new(
        components: [Option<TextureId>; COMPONENT_COUNT],
        channels: [ChannelSource; CHANNEL_COUNT],
    ) -> Result<Material, MissingComponent> {
        for slot in ChannelSlot::all() {
            for component in channels[slot.index()].components() {
                if components[component.index()].is_none() {
                    return Err(MissingComponent { slot, component });
                }
            }
        }
        Ok(Material { components, channels })
    }

    pub fn channel(&self, slot: ChannelSlot) -> &ChannelSource {
        &self.channels[slot.index()]
    }

    /// The texture of a component (present for every component a channel names).
    pub fn texture(&self, c: Component) -> Option<TextureId> {
        self.components[c.index()]
    }

    /// Number of textures the material holds; at most four.
    pub fn texture_count(&self) -> usize {
        self.components.iter().flatten().count()
    }
}

/// A material as used at a hit: one material, or two mixed channel by channel.
/// per-channel weights represent "mix two materials channel by channel"; how the
/// original gives the weight is Open.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialDef {
    Single(Material),
    Mix { first: Box<Material>, second: Box<Material>, weights: [f64; CHANNEL_COUNT] },
}

/// What a texture sampler sees of the hit. Textures may read height, slope and facing
/// (sources.md E18); the sampler derives those from these fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplePoint {
    pub position: Vec3,
    pub normal: Vec3,
}

/// A texture's output at a point: a colour and a value (the value is what a blend reads).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSample {
    pub colour: Rgb,
    pub value: f64,
}

/// The `textures` component, as far as shading needs it (not released).
/// Sampling may update carried state, which is passed explicitly.
pub trait TextureSampler {
    type State;
    fn sample(&self, texture: TextureId, at: &SamplePoint, state: &mut Self::State) -> TextureSample;
}

/// The rules calls the original's own and which is still open. All
/// are parameters.
pub trait ChannelRules {
    /// Two textures blended by altitude at the point (Open:; for the
    /// altitude).
    fn by_altitude(
        &self,
        slot: ChannelSlot,
        first: TextureSample,
        second: TextureSample,
        at: &SamplePoint,
    ) -> ChannelValue;
    /// Two textures blended by a third's value: `first` at 1, `second` at 0 (answer 0302;
    /// the combination between is Open).
    fn blend(
        &self,
        slot: ChannelSlot,
        first: TextureSample,
        second: TextureSample,
        by: TextureSample,
    ) -> ChannelValue;
    /// Two materials' values for one channel, mixed (Open).
    fn mix(&self, slot: ChannelSlot, first: ChannelValue, second: ChannelValue, weight: f64)
    -> ChannelValue;
}

/// Every channel's value at a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSample {
    pub values: [ChannelValue; CHANNEL_COUNT],
}

impl MaterialSample {
    pub fn get(&self, slot: ChannelSlot) -> ChannelValue {
        self.values[slot.index()]
    }
}

/// Bryce 5 material switches (haze inside volumes, jittered volume samples): Open
///.
pub fn bryce5_switches() -> Result<(), OpenItem> {
    Err(open::MATERIAL_SWITCHES)
}

/// One channel's value. the components are sampled `first`, `second`, `by`, each
/// once per channel that names it (order and repeats Open).
pub fn evaluate_channel<S: TextureSampler, R: ChannelRules>(
    slot: ChannelSlot,
    material: &Material,
    rules: &R,
    sampler: &S,
    at: &SamplePoint,
    state: &mut S::State,
) -> ChannelValue {
    let sample = |c: Component, state: &mut S::State| {
        let texture = material.texture(c).expect("checked by Material::new");
        sampler.sample(texture, at, state)
    };
    match *material.channel(slot) {
        ChannelSource::Constant(v) => v,
        ChannelSource::Texture(a) => texture_to_channel(slot, sample(a, state)),
        ChannelSource::ByAltitude { first, second } => {
            let a = sample(first, state);
            let b = sample(second, state);
            rules.by_altitude(slot, a, b, at)
        }
        ChannelSource::Blend { first, second, by } => {
            let a = sample(first, state);
            let b = sample(second, state);
            let w = sample(by, state);
            rules.blend(slot, a, b, w)
        }
    }
}

/// All channels of one material. slot order 0..12.
pub fn evaluate_material<S: TextureSampler, R: ChannelRules>(
    material: &Material,
    rules: &R,
    sampler: &S,
    at: &SamplePoint,
    state: &mut S::State,
) -> MaterialSample {
    let mut values = [ChannelValue::Scalar(0.0); CHANNEL_COUNT];
    for slot in ChannelSlot::all() {
        values[slot.index()] = evaluate_channel(slot, material, rules, sampler, at, state);
    }
    MaterialSample { values }
}

/// A material definition at a hit . for a mix, all of `first` is
/// evaluated, then all of `second`, then each channel is mixed.
pub fn evaluate<S: TextureSampler, R: ChannelRules>(
    def: &MaterialDef,
    rules: &R,
    sampler: &S,
    at: &SamplePoint,
    state: &mut S::State,
) -> MaterialSample {
    match def {
        MaterialDef::Single(m) => evaluate_material(m, rules, sampler, at, state),
        MaterialDef::Mix { first, second, weights } => {
            let a = evaluate_material(first, rules, sampler, at, state);
            let b = evaluate_material(second, rules, sampler, at, state);
            let mut values = a.values;
            for slot in ChannelSlot::all() {
                let i = slot.index();
                values[i] = rules.mix(slot, a.values[i], b.values[i], weights[i]);
            }
            MaterialSample { values }
        }
    }
}
