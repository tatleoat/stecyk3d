// SPDX-License-Identifier: MIT
//! A procedural texture's tracks at a time: its record's settings blended between two keys,
//! and the mapping's mode and switches.
//!
//! , (the read time, the key search, the closing
//! stretch of setting 1),

use super::AnimError;
use super::lerp::PLAIN_KEY_GAP;
use super::track::{Interpolate, Key, Repeat, Span, Track};
use crate::textures::{ColourMap, Filter, NoiseSpace, Phase, TextureSettings};

/// The levels of a colour run in 256ths of the way.
const LEVEL_STEPS: f64 = 256.0;

/// ⟨(b − a)·u + a⟩ of two numbers of the record, each taken as a binary32 first (the nearest
/// one, for a whole number of version 1), the result kept as the binary64 equal to it.
fn blend_number(a: f64, b: f64, u: f64) -> f64 {
    let (a, b) = (f64::from(a as f32), f64::from(b as f32));
    f64::from(((b - a) * u + a) as f32)
}

fn blend_space(a: &NoiseSpace, b: &NoiseSpace, u: f64) -> NoiseSpace {
    NoiseSpace {
        angles: std::array::from_fn(|k| blend_number(a.angles[k], b.angles[k], u)),
        frequencies: std::array::from_fn(|k| blend_number(a.frequencies[k], b.frequencies[k], u)),
    }
}

/// The phase's amount, angles and frequencies blended; its noise words key i's.
fn blend_phase(a: &Phase, b: &Phase, u: f64) -> Phase {
    Phase { amount: blend_number(a.amount, b.amount, u), noise: a.noise, space: blend_space(&a.space, &b.space, u) }
}

/// The filter's a and b blended; its number and n key i's.
fn blend_filter(a: &Filter, b: &Filter, u: f64) -> Filter {
    let one = |x: f32, y: f32| blend_number(f64::from(x), f64::from(y), u) as f32;
    Filter { a: one(a.a, b.a), b: one(a.b, b.b), ..*a }
}

/// Each level a + ⌊(b − a)·n / 256⌋ of the three colours; the mode and space key i's.
fn blend_colours(a: &ColourMap, b: &ColourMap, n: i64) -> ColourMap {
    let level = |x: u8, y: u8| {
        let (x, y) = (i64::from(x), i64::from(y));
        (x + ((y - x) * n).div_euclid(256)) as u8
    };
    let colour = |x: [u8; 3], y: [u8; 3]| std::array::from_fn(|k| level(x[k], y[k]));
    ColourMap { a: colour(a.a, b.a), b: colour(a.b, b.b), c: colour(a.c, b.c), ..*a }
}

/// A component as the file holds an unused one: every blended number and colour level 0.
fn zero_component(x: &crate::textures::Component) -> crate::textures::Component {
    let mut z = *x;
    z.space = NoiseSpace { angles: [0.0; 2], frequencies: [0.0; 3] };
    z.phase.amount = 0.0;
    z.phase.space = z.space;
    z.filter.a = 0.0;
    z.filter.b = 0.0;
    z.colours.a = [0; 3];
    z.colours.b = [0; 3];
    z.colours.c = [0; 3];
    z
}

/// A texture's settings between key `a` and the next key's `b` at u: key i's settings with
/// the phases', filters' and colour maps' numbers blended, for the texture and for each of
/// key i's components (beyond them, nothing). A first key with more components than the
/// second is refused (Open).
///
pub fn blend_texture_settings(a: &TextureSettings, b: &TextureSettings, u: f64) -> Result<TextureSettings, AnimError> {
    let n = (LEVEL_STEPS * u).trunc() as i64;
    // (a first key with more components than the second: the second key's place
    // of such a component holds 0, so it is blended with a component of zeros; the count is the
    // first key's; what the result holds there is the implementer's choice, pictures equal)
    let components = a
        .components
        .iter()
        .enumerate()
        .map(|(k, x)| {
            let zero;
            let y = match b.components.get(k) {
                Some(y) => y,
                None => {
                    zero = zero_component(x);
                    &zero
                }
            };
            crate::textures::Component {
                space: blend_space(&x.space, &y.space, u),
                phase: blend_phase(&x.phase, &y.phase, u),
                filter: blend_filter(&x.filter, &y.filter, u),
                colours: blend_colours(&x.colours, &y.colours, n),
                ..*x
            }
        })
        .collect();
    Ok(TextureSettings {
        outputs: a.outputs,
        components,
        blend: a.blend,
        phase: blend_phase(&a.phase, &b.phase, u),
        filter: blend_filter(&a.filter, &b.filter, u),
        colours: blend_colours(&a.colours, &b.colours, n),
    })
}

/// A texture's record track at frame time t (`Ok(None)` for a track with no key), read as a
/// track not read before: the key's settings where the key is the last or the next key is
/// less than 0.0001 after it, else the blend (the closing stretch of setting 1 running to
/// the first key).
///
pub fn texture_settings_at(track: &Track<TextureSettings>, t: f64) -> Result<Option<TextureSettings>, AnimError> {
    Ok(texture_record_at(track, t)?.map(|(settings, _)| settings))
}

/// As `texture_settings_at`, with the key the record stands on: `Some(index)` where the key
/// found is the last or the next key is less than 0.0001 after it (the key's whole record is the
/// texture's, its result as saved included), `None` between keys (the settings alone are
/// written; what the texture keeps stays).
///
/// (Part 3: the record at a stand)
pub fn texture_record_at(
    track: &Track<TextureSettings>,
    t: f64,
) -> Result<Option<(TextureSettings, Option<usize>)>, AnimError> {
    let Some(s) = track.read_time(t)? else { return Ok(None) };
    match Span::locate(&track.times(), track.repeat, s) {
        Span::AtLast { index } => Ok(Some((track.keys[index].value.clone(), Some(index)))),
        Span::Between { from, to_value, t0, t1, u, .. } => {
            if t1 - t0 < PLAIN_KEY_GAP {
                return Ok(Some((track.keys[from].value.clone(), Some(from))));
            }
            blend_texture_settings(&track.keys[from].value, &track.keys[to_value].value, u).map(|b| Some((b, None)))
        }
    }
}

/// The mapping's mode and switches (two words): whole, the key at or before the time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingWords {
    pub mode: u32,
    pub switches: u32,
}

impl Interpolate for MappingWords {
    fn between(keys: &[Key<Self>], _repeat: Repeat, span: &Span) -> Self {
        keys[span.key()].value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_round_down() {
        let map = |c: u8| ColourMap { mode: 0, space: crate::textures::ColourSpace::Rgb, a: [c; 3], b: [c; 3], c: [c; 3] };
        assert_eq!(blend_colours(&map(10), &map(0), 128).a, [5; 3]);
        assert_eq!(blend_colours(&map(10), &map(1), 1).a, [9; 3]);
        assert_eq!(blend_colours(&map(10), &map(200), 256).a, [200; 3]);
        assert_eq!(blend_number(3.0, 5.0, 0.5), 4.0);
    }

    #[test]
    fn mapping_words_step() {
        let track = Track::new(
            vec![
                Key { time: 0.0, value: MappingWords { mode: 1, switches: 0 } },
                Key { time: 1.0, value: MappingWords { mode: 2, switches: 0x20 } },
            ],
            Repeat::None,
        );
        assert_eq!(track.value_at(0.9).unwrap().unwrap().mode, 1);
        assert_eq!(track.value_at(1.0).unwrap().unwrap().mode, 2);
    }
}
