// SPDX-License-Identifier: MIT
//! Straight-line interpolation of plain values between two keys.
//!

use super::track::{Interpolate, Key, Repeat, Span};

/// Keys closer in time than this take the first key's value.
pub const PLAIN_KEY_GAP: f64 = 0.0001;

/// a + (b - a) u in binary64.
pub fn lerp_f64(a: f64, b: f64, u: f64) -> f64 {
    a + (b - a) * u
}

/// `lerp_f64` for each of three numbers.
pub fn lerp_f64_triple(a: [f64; 3], b: [f64; 3], u: f64) -> [f64; 3] {
    [lerp_f64(a[0], b[0], u), lerp_f64(a[1], b[1], u), lerp_f64(a[2], b[2], u)]
}

/// Rounds a binary64 value to binary32 and widens it again.
fn r32(x: f64) -> f64 {
    x as f32 as f64
}

/// a + (b - a) u for a track of N numbers kept as binary32 (N is 1, 2 or 3), in binary64
/// on the binary32 values, with these roundings to binary32 by the number's place i:
/// - the difference b[i] - a[i] is rounded when i is above 0;
/// - its product with u is rounded when i is below N - 1;
/// - the sum is rounded once.
///
/// The pattern depends only on N and the place, never on what the track is for.
///
pub fn lerp_f32<const N: usize>(a: [f32; N], b: [f32; N], u: f64) -> [f32; N] {
    std::array::from_fn(|i| {
        let (ai, bi) = (a[i] as f64, b[i] as f64);
        let mut difference = bi - ai;
        if i > 0 {
            difference = r32(difference);
        }
        let mut product = difference * u;
        if i + 1 < N {
            product = r32(product);
        }
        (ai + product) as f32
    })
}

/// `lerp_f32` for three numbers: product rounded for the first, both for the second,
/// difference for the third.
pub fn lerp_f32_triple(a: [f32; 3], b: [f32; 3], u: f64) -> [f32; 3] {
    lerp_f32(a, b, u)
}

/// The two keys' values and u, or the first key's value when the keys are too close.
fn plain_stretch<V: Clone>(keys: &[Key<V>], span: &Span) -> Result<(V, V, f64), V> {
    let Span::Between { from, to_value, t0, t1, u, .. } = *span else {
        unreachable!("plain_stretch is called on a stretch");
    };
    if (t1 - t0).abs() < PLAIN_KEY_GAP {
        return Err(keys[from].value.clone());
    }
    Ok((keys[from].value.clone(), keys[to_value].value.clone(), u))
}

/// Tracks of three binary64 numbers.
impl Interpolate for [f64; 3] {
    fn between(keys: &[Key<Self>], _repeat: Repeat, span: &Span) -> Self {
        match plain_stretch(keys, span) {
            Ok((a, b, u)) => lerp_f64_triple(a, b, u),
            Err(first) => first,
        }
    }
}

/// Tracks of one binary64 number (a camera's field of view,'s vectors).
impl Interpolate for f64 {
    fn between(keys: &[Key<Self>], _repeat: Repeat, span: &Span) -> Self {
        match plain_stretch(keys, span) {
            Ok((a, b, u)) => lerp_f64(a, b, u),
            Err(first) => first,
        }
    }
}

/// Tracks of one, two or three binary32 numbers.
impl<const N: usize> Interpolate for [f32; N] {
    fn between(keys: &[Key<Self>], _repeat: Repeat, span: &Span) -> Self {
        match plain_stretch(keys, span) {
            Ok((a, b, u)) => lerp_f32(a, b, u),
            Err(first) => first,
        }
    }
}
