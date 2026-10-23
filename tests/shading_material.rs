// SPDX-License-Identifier: MIT
//! Tests for: thirteen channels; constants, textures and blends; channel-by-channel
//! mixing; texture evaluation updating carried state. The blend and mix rules are Open, so
//! the rules used here are test-only and only show which inputs reach them.

use stecyk3d::geom::Rgb;
use stecyk3d::shading::material::*;
use stecyk3d::shading::open;

/// A texture whose colour is its id and whose value is a count of evaluations so far,
/// which is its carried state.
struct Counting;
impl TextureSampler for Counting {
    type State = Vec<u32>;
    fn sample(&self, t: TextureId, _at: &SamplePoint, state: &mut Vec<u32>) -> TextureSample {
        state.push(t.0);
        TextureSample { colour: Rgb::splat(t.0 as f64), value: state.len() as f64 }
    }
}

/// Test-only rules recording what reaches them as scalars.
struct Probe;
impl ChannelRules for Probe {
    fn by_altitude(&self, _s: ChannelSlot, a: TextureSample, b: TextureSample, _at: &SamplePoint) -> ChannelValue {
        ChannelValue::Colour(Rgb::new(a.colour.r, b.colour.r, -1.0))
    }
    fn blend(&self, _s: ChannelSlot, a: TextureSample, b: TextureSample, w: TextureSample) -> ChannelValue {
        ChannelValue::Colour(Rgb::new(a.colour.r, b.colour.r, w.colour.r))
    }
    fn mix(&self, _s: ChannelSlot, a: ChannelValue, b: ChannelValue, w: f64) -> ChannelValue {
        match (a, b) {
            (ChannelValue::Scalar(x), ChannelValue::Scalar(y)) => ChannelValue::Colour(Rgb::new(x, y, w)),
            _ => a,
        }
    }
}

fn at() -> SamplePoint {
    SamplePoint { position: [0.0; 3], normal: [0.0, 0.0, 1.0] }
}

fn constant_channels(base: f64) -> [ChannelSource; CHANNEL_COUNT] {
    let mut channels = [ChannelSource::Constant(ChannelValue::Scalar(0.0)); CHANNEL_COUNT];
    for (i, c) in channels.iter_mut().enumerate() {
        *c = ChannelSource::Constant(ChannelValue::Scalar(base + i as f64));
    }
    channels
}

fn constants(base: f64) -> Material {
    Material::new([None; COMPONENT_COUNT], constant_channels(base)).unwrap()
}

#[test]
fn thirteen_channels() {
    assert_eq!(CHANNEL_COUNT, 13);
    assert!(ChannelSlot::new(12).is_some());
    assert!(ChannelSlot::new(13).is_none());
    assert_eq!(ChannelSlot::all().count(), 13);
}

#[test]
fn channel_names_and_kinds() {
    let slot = |n| ChannelSlot::named(n).unwrap();
    assert_eq!(slot("Diffuse Color").index(), 0);
    assert_eq!(slot("Base Density").index(), 12);
    assert_eq!(slot("Specular Halo").kind(), ChannelKind::Colour);
    assert_eq!(slot("Diffusion").kind(), ChannelKind::Scalar);
    assert_eq!(ChannelSlot::named("Refraction"), None); // a plain setting, no texture
    assert_eq!(ChannelSlot::new(0).unwrap().range(), Err(open::CHANNEL_LIST));
    assert!(bryce5_switches().is_err());
}

#[test]
fn constants_pass_through_and_touch_no_texture() {
    let m = constants(10.0);
    let mut state = Vec::new();
    let s = evaluate(&MaterialDef::Single(m), &Probe, &Counting, &at(), &mut state);
    for slot in ChannelSlot::all() {
        assert_eq!(s.get(slot), ChannelValue::Scalar(10.0 + slot.index() as f64));
    }
    assert!(state.is_empty());
}

#[test]
fn textures_and_blends_reach_the_rules_and_update_state() {
    // Components A to D hold textures 7, 1, 2, 3.
    let components = [Some(TextureId(7)), Some(TextureId(1)), Some(TextureId(2)), Some(TextureId(3))];
    let mut channels = constant_channels(0.0);
    channels[2] = ChannelSource::Texture(Component::A);
    channels[5] = ChannelSource::Blend { first: Component::B, second: Component::C, by: Component::D };
    channels[6] = ChannelSource::ByAltitude { first: Component::A, second: Component::B };
    channels[7] = ChannelSource::Texture(Component::A); // the same texture as channel 2
    let m = Material::new(components, channels).unwrap();
    assert_eq!(m.texture_count(), 4);
    let mut state = Vec::new();
    let s = evaluate(&MaterialDef::Single(m), &Probe, &Counting, &at(), &mut state);
    // A colour channel takes the texture's colour; a number channel its value (answer 0301).
    assert_eq!(s.values[2], ChannelValue::Colour(Rgb::splat(7.0)));
    assert_eq!(s.values[5], ChannelValue::Colour(Rgb::new(1.0, 2.0, 3.0)));
    assert_eq!(s.values[6], ChannelValue::Colour(Rgb::new(7.0, 1.0, -1.0)));
    assert_eq!(s.values[7], ChannelValue::Scalar(7.0)); // the seventh evaluation
    assert_eq!(state, vec![7, 1, 2, 3, 7, 1, 7]);
}

#[test]
fn a_channel_naming_an_empty_component_is_refused() {
    let mut channels = constant_channels(0.0);
    channels[3] = ChannelSource::Texture(Component::C);
    let err = Material::new([Some(TextureId(1)), None, None, None], channels).unwrap_err();
    assert_eq!(err.component, Component::C);
    assert_eq!(err.slot.index(), 3);
}

#[test]
fn mix_is_channel_by_channel() {
    let mut weights = [0.0; CHANNEL_COUNT];
    for (i, w) in weights.iter_mut().enumerate() {
        *w = i as f64 / 100.0;
    }
    let def = MaterialDef::Mix { first: Box::new(constants(0.0)), second: Box::new(constants(100.0)), weights };
    let mut state = Vec::new();
    let s = evaluate(&def, &Probe, &Counting, &at(), &mut state);
    for slot in ChannelSlot::all() {
        let i = slot.index() as f64;
        assert_eq!(s.get(slot), ChannelValue::Colour(Rgb::new(i, 100.0 + i, i / 100.0)));
    }
}
