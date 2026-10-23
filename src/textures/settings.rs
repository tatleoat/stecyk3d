// SPDX-License-Identifier: MIT
//! A procedural texture's settings as lists them, and the surface context of.
//!
//! (Inputs (1): components, noise type, octave method and count, dimension, noise
//! space, phase, filter, colour map, blend modes, global phase, filter and colour map, outputs,
//! colour space; Limits: at most three components),,
//! , (phase amount,
//! noise, space),,
//! (Types)

/// Which outputs are asked of a texture, or produced by a component (letters `c`, `v`, `b`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputSet {
    pub colour: bool,
    pub value: bool,
    pub bump: bool,
}

impl OutputSet {
    /// From the letters of testing-interface.md; `None` for any other letter.
    pub fn from_letters(s: &str) -> Option<OutputSet> {
        let mut o = OutputSet::default();
        for ch in s.chars() {
            match ch {
                'c' => o.colour = true,
                'v' => o.value = true,
                'b' => o.bump = true,
                _ => return None,
            }
        }
        Some(o)
    }
}

/// 's dimension setting. A number other than 1, 2, 3 is kept (Errors: the
/// implementer's choice; no document holds one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    One,
    Two,
    Three,
    Other(i64),
}

impl Dimension {
    pub fn from_number(n: i64) -> Dimension {
        match n {
            1 => Dimension::One,
            2 => Dimension::Two,
            3 => Dimension::Three,
            other => Dimension::Other(other),
        }
    }
}

/// A noise as a component (or a phase) names it:'s type number,'s method and
/// octave count,'s dimension. The numbers are the document's (file-format values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseSettings {
    pub type_number: i64,
    pub method: i64,
    pub octaves: i64,
    pub dimension: Dimension,
}

/// 's inputs: two rotation angles and a frequency per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseSpace {
    pub angles: [f64; 2],
    pub frequencies: [f64; 3],
}

/// 's settings: the amount, the phase noise and its space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phase {
    pub amount: f64,
    pub noise: NoiseSettings,
    pub space: NoiseSpace,
}

impl Phase {
    /// No phase: an amount of 0 and a noise type of 0 ("A texture or
    /// a component with no phase has an amount of 0 and a noise type of 0").
    pub fn is_none(&self) -> bool {
        self.amount == 0.0 && self.noise.type_number == 0
    }
}

/// 's filter: its number, the integer n and the two singles a and b (kept bit for bit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub number: i64,
    pub n: i64,
    pub a: f32,
    pub b: f32,
}

/// The colour space setting : RGB, or hue, lightness and saturation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourSpace {
    Rgb,
    Hls,
}

/// 's colour map: the mode number and colours A, B, C (8-bit channels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColourMap {
    pub mode: i64,
    pub space: ColourSpace,
    pub a: [u8; 3],
    pub b: [u8; 3],
    pub c: [u8; 3],
}

/// One of a texture's one to three components .
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Component {
    pub outputs: OutputSet,
    pub noise: NoiseSettings,
    pub space: NoiseSpace,
    pub phase: Phase,
    pub filter: Filter,
    pub colours: ColourMap,
}

/// At most three components.
pub const MAX_COMPONENTS: usize = 3; // Limits

/// A whole texture's settings (input 1).
#[derive(Clone, Debug, PartialEq)]
pub struct TextureSettings {
    pub outputs: OutputSet,
    pub components: Vec<Component>,
    /// The second component's blend with the first, the third's with that result .
    pub blend: [i64; 2],
    pub phase: Phase,
    pub filter: Filter,
    pub colours: ColourMap,
}

impl TextureSettings {
    /// Whether a stage reads the surface context : filters 8 to 11 and 13 ,
    /// colour modes 15 and 16  and the blend weights of modes 10 to 13 , in
    /// a component or in the texture's own. Where the context is Open (a surface that sets
    /// none) such a texture cannot be evaluated as stated.
    pub fn reads_surface_context(&self) -> bool {
        let filter = |f: &Filter| matches!(f.number, 8..=11 | 13);
        let colours = |c: &ColourMap| matches!(c.mode, 15 | 16);
        let blends = self.blend.iter().take(self.components.len().saturating_sub(1));
        filter(&self.filter)
            || colours(&self.colours)
            || self.components.iter().any(|c| filter(&c.filter) || colours(&c.colours))
            || blends.into_iter().any(|b| matches!(b, 10..=13))
    }
}

/// The surface context at the point , given already computed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfaceContext {
    pub altitude: f64,
    pub slope: f64,
    pub orientation: f64,
}

impl SurfaceContext {
    /// The surface context of a material's texture at the texture-space point p, for the
    /// hit's shading normal n (world space, before the facing test and any bump): altitude
    /// py as it stands; slope 1 − |ny|; orientation 2·nx held to 0 to 1.
    pub fn at(point: [f64; 3], normal: [f64; 3]) -> SurfaceContext {
        let orientation = (2.0 * normal[0]).clamp(0.0, 1.0);
        SurfaceContext { altitude: point[1], slope: 1.0 - normal[1].abs(), orientation }
    }
}
