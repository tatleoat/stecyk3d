// SPDX-License-Identifier: MIT
//! Places between keys: straight stretches, and curved stretches shaped by tension,
//! continuity and bias (Kochanek and Bartels 1984), drawn as cubic Bézier curves by de
//! Casteljau's construction.
//!
//! (the seam rules and roundings the vectors fix)

use super::AnimError;
use super::track::{Key, ReadMemory, Repeat, Span, Track, TrackReader};
use crate::geom::Vec3;
use crate::version::RenderAs;

/// Keys closer in time than this take the first key's place.
pub const PLACE_KEY_GAP: f64 = 1e-7;

/// The factor k of the curved rules.
pub const CURVE_K: f64 = 0.19;

/// 3.1 and later make a stretch straight when its ends are this close in every coordinate.
pub const SAME_PLACE: f64 = 0.0001;

/// A place key's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceKey {
    pub place: Vec3,
    /// Whether the stretch that starts at this key is curved (the document's word is 2).
    pub curved: bool,
    /// Tension, continuity and bias, in the order the document keeps them.
    pub tcb: [f32; 3],
}

impl PlaceKey {
    /// From the document's passing word (2 curved; 0 or 1 straight).
    pub fn from_passing(place: Vec3, passing: u32, tcb: [f32; 3]) -> PlaceKey {
        PlaceKey { place, curved: passing == 2, tcb }
    }
}

/// (b - a) u + a for each coordinate: one step of de Casteljau's construction.
fn towards(a: Vec3, b: Vec3, u: f64) -> Vec3 {
    [(b[0] - a[0]) * u + a[0], (b[1] - a[1]) * u + a[1], (b[2] - a[2]) * u + a[2]]
}

/// The cubic Bézier curve p0, c1, c2, p1 at u, by de Casteljau's construction. Also gives
/// the two points of the second round, whose difference is the curve's direction at u.
///
pub(super) fn bezier(p0: Vec3, c1: Vec3, c2: Vec3, p1: Vec3, u: f64) -> (Vec3, Vec3, Vec3) {
    let a = towards(p0, c1, u);
    let b = towards(c1, c2, u);
    let c = towards(c2, p1, u);
    let d = towards(a, b, u);
    let e = towards(b, c, u);
    (towards(d, e, u), d, e)
}

/// Whether 3.1's rule makes the stretch from p0 to p1 straight: every coordinate within
/// 0.0001 (a coordinate that is not a number counts as within).
fn same_place(p0: Vec3, p1: Vec3) -> bool {
    (0..3).all(|j| !((p1[j] - p0[j]).abs() >= SAME_PLACE))
}

fn r32(x: f64) -> f64 {
    x as f32 as f64
}

/// The place on the curved stretch from key `from`, given the keys and the repeat setting.
///
/// animation (track-place.jsonl: inner points from the stretch's own keys; the
/// neighbours across setting 1's seam; the rounding of 1 - T0)
fn curved_place(keys: &[Key<PlaceKey>], repeat: Repeat, from: usize, end: Vec3, u: f64) -> Vec3 {
    let n = keys.len();
    let k0 = &keys[from].value;
    let k1 = &keys[from + 1].value;
    let q0 = k0.place;
    let q1 = k1.place;
    let p0 = q0;
    if n == 2 {
        let c1 = [0, 1, 2].map(|j| q1[j] + (q0[j] - q1[j]) / 2.0);
        let c2 = [0, 1, 2].map(|j| q0[j] + (q1[j] - q0[j]) / 2.0);
        return bezier(p0, c1, c2, end, u).0;
    }
    let looped = repeat == Repeat::Loop;
    // The key before Q0; across setting 1's seam it is key n-2.
    let (before, seam_before) = if from > 0 {
        (Some(keys[from - 1].value.place), false)
    } else if looped {
        (Some(keys[n - 2].value.place), true)
    } else {
        (None, false)
    };
    // The key after Q1; with setting 1 the last key stands for key 0, and the closing
    // stretch has none.
    let after = if from + 2 < n {
        if looped && from + 2 == n - 1 {
            Some(keys[0].value.place)
        } else {
            Some(keys[from + 2].value.place)
        }
    } else {
        None
    };
    let [t0, c0, b0] = k0.tcb.map(|v| v as f64);
    let [t1, c1p, b1] = k1.tcb.map(|v| v as f64);
    let dd = [0, 1, 2].map(|j| q1[j] - q0[j]);

    let mut c1 = None;
    if let Some(before) = before {
        let om = if seam_before { 1.0 - t0 } else { r32(1.0 - t0) };
        let wa = om * (1.0 + c0) * (1.0 + b0) * CURVE_K;
        let wb = om * (1.0 - c0) * (1.0 - b0) * CURVE_K;
        let e = [0, 1, 2].map(|j| q0[j] - before[j]);
        c1 = Some([0, 1, 2].map(|j| q0[j] + wa * e[j] + wb * dd[j]));
    }
    let mut c2 = None;
    if let Some(after) = after {
        let om = 1.0 - t1;
        let wa = om * (1.0 - c1p) * (1.0 + b1) * CURVE_K;
        let wb = om * (1.0 + c1p) * (1.0 - b1) * CURVE_K;
        let f = [0, 1, 2].map(|j| after[j] - q1[j]);
        c2 = Some([0, 1, 2].map(|j| q1[j] - wa * dd[j] - wb * f[j]));
    }
    let (c1, c2) = match (c1, c2) {
        (Some(c1), Some(c2)) => (c1, c2),
        (None, Some(c2)) => {
            let c1 =
                [0, 1, 2].map(|j| q0[j] + ((q1[j] + (1.0 + c0) * (c2[j] - q1[j])) - q0[j]) * (t0 + CURVE_K));
            (c1, c2)
        }
        (Some(c1), None) => {
            let c2 = [0, 1, 2]
                .map(|j| q1[j] + ((q0[j] + (c1[j] - q0[j]) * (1.0 + c1p)) - q1[j]) * (t1 + CURVE_K));
            (c1, c2)
        }
        (None, None) => unreachable!("three or more keys have a neighbour on one side"),
    };
    bezier(p0, c1, c2, end, u).0
}

/// The place of a track at frame time t: `Ok(None)` when the track has no key.
///
pub fn place_at(
    version: RenderAs,
    track: &Track<PlaceKey>,
    t: f64,
) -> Result<Option<Vec3>, AnimError> {
    place_after(version, track, &mut ReadMemory::new(), t)
}

impl TrackReader<'_, PlaceKey> {
    /// The place at frame time t, after this reader's earlier reads.
    pub fn place_at(&mut self, version: RenderAs, t: f64) -> Result<Option<Vec3>, AnimError> {
        place_after(version, self.track, &mut self.memory, t)
    }
}

/// The place of a track at frame time t, given what earlier reads of this track left in
/// `memory`, which this read updates.
///
pub fn place_after(
    version: RenderAs,
    track: &Track<PlaceKey>,
    memory: &mut ReadMemory,
    t: f64,
) -> Result<Option<Vec3>, AnimError> {
    let Some(s) = track.read_time(t)? else {
        return Ok(None);
    };
    let keys = &track.keys;
    let span = memory.locate(&track.times(), track.repeat, s);
    let Span::Between { from, to_value, t0, t1, u, .. } = span else {
        let Span::AtLast { index } = span else { unreachable!() };
        return Ok(Some(keys[index].value.place));
    };
    if version != RenderAs::Bryce30 {
        // : a key's place as it is within 1e-10 of its time
        if let Some(k) = span.key_held_as_is(s) {
            return Ok(Some(keys[k].value.place));
        }
    }
    let p0 = keys[from].value.place;
    if (t1 - t0).abs() < PLACE_KEY_GAP {
        return Ok(Some(p0));
    }
    let end = keys[to_value].value.place;
    let mut curved = keys[from].value.curved;
    if curved && version != RenderAs::Bryce30 && same_place(p0, end) {
        curved = false;
    }
    if !curved {
        return Ok(Some(towards(p0, end, u)));
    }
    Ok(Some(curved_place(keys, track.repeat, from, end, u)))
}
