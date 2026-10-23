// SPDX-License-Identifier: MIT
//! Orientations between keys: spherical linear interpolation with extra half turns.
//!

use super::ext_sine::x87_sine;
use super::track::{Key, ReadMemory, Span, Track, TrackReader};
use super::{AnimError, Quat};
use crate::numerics::acos;
use crate::version::RenderAs;

/// Keys closer in time than this take the first key's orientation.
pub const ORIENT_KEY_GAP: f64 = 1e-7;

/// Below this value of 1 - d the weights are 1 - u and u.
pub const NEAR_EQUAL: f64 = 1e-10;

/// pi as the binary64 nearest it, for h * pi. animation (slerp.jsonl: all
/// versions match with binary64 pi)
const PI: f64 = std::f64::consts::PI;

/// An orientation key's value: a unit quaternion and a number of extra half turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrientKey {
    pub orientation: Quat,
    pub half_turns: f32,
}

/// The dot product of two quaternions, its sums in the order the version takes them.
///
/// (Variants: "the three versions compute the dot product's last bits
/// differently"); animation (slerp.jsonl: the order that matches each version)
fn dot(version: RenderAs, a: &Quat, b: &Quat) -> f64 {
    let p = [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]];
    let [x, y, z, w] = p;
    match version {
        RenderAs::Bryce30 => ((z + w) + y) + x,
        RenderAs::Bryce31 => ((x + z) + w) + y,
        RenderAs::Bryce50 => ((x + z) + y) + w,
    }
}

/// Spherical linear interpolation from q0 to q1 with h extra half turns, at part u.
///
/// (Outputs: the weights, the shorter way, the near-equal case, the
/// reciprocal of sin θ rounded to binary64; the sine is's, used at extended
/// precision by the product)
pub fn slerp(version: RenderAs, q0: Quat, q1: Quat, half_turns: f32, u: f64) -> Quat {
    let mut d = dot(version, &q0, &q1);
    let mut q1 = q1;
    if d < 0.0 {
        d = -d;
        q1 = [-q1[0], -q1[1], -q1[2], -q1[3]];
    }
    let (w0, w1) = if 1.0 - d < NEAR_EQUAL {
        (1.0 - u, u)
    } else {
        let theta = acos(d);
        let phi = ((half_turns as f64) * PI + theta) * u;
        let reciprocal = x87_sine(theta).recip_to_f64();
        (
            x87_sine(theta - phi).mul_to_f64(reciprocal),
            x87_sine(phi).mul_to_f64(reciprocal),
        )
    };
    [
        q0[0] * w0 + q1[0] * w1,
        q0[1] * w0 + q1[1] * w1,
        q0[2] * w0 + q1[2] * w1,
        q0[3] * w0 + q1[3] * w1,
    ]
}

/// Orientation keys run by slerp. The half turns are those of the key the stretch runs
/// to: on the closing stretch of setting 1, the first key's.
///
/// ; animation (track-orient.jsonl)
fn orient_between(version: RenderAs, keys: &[Key<OrientKey>], span: &Span) -> OrientKey {
    let Span::Between { from, to_value, t0, t1, u, .. } = *span else {
        unreachable!("orient_between is called on a stretch");
    };
    let a = keys[from].value;
    // (The big release: the gap as a distance, keys out of order)
    if (t1 - t0).abs() < ORIENT_KEY_GAP {
        return a;
    }
    let b = keys[to_value].value;
    OrientKey {
        orientation: slerp(version, a.orientation, b.orientation, b.half_turns, u),
        half_turns: b.half_turns,
    }
}

/// The orientation of a track at frame time t: `Ok(None)` when the track has no key.
///
pub fn orientation_at(
    version: RenderAs,
    track: &Track<OrientKey>,
    t: f64,
) -> Result<Option<Quat>, AnimError> {
    orientation_after(version, track, &mut ReadMemory::new(), t)
}

impl TrackReader<'_, OrientKey> {
    /// The orientation at frame time t, after this reader's earlier reads.
    pub fn orientation_at(&mut self, version: RenderAs, t: f64) -> Result<Option<Quat>, AnimError> {
        orientation_after(version, self.track, &mut self.memory, t)
    }
}

/// The orientation of a track at frame time t, given what earlier reads of this track
/// left in `memory`, which this read updates.
///
pub fn orientation_after(
    version: RenderAs,
    track: &Track<OrientKey>,
    memory: &mut ReadMemory,
    t: f64,
) -> Result<Option<Quat>, AnimError> {
    let Some(s) = track.read_time(t)? else {
        return Ok(None);
    };
    let span = memory.locate(&track.times(), track.repeat, s);
    if version != RenderAs::Bryce30 {
        // : a key's orientation as it is within 1e-10 of its time
        if let Some(k) = span.key_held_as_is(s) {
            return Ok(Some(track.keys[k].value.orientation));
        }
    }
    Ok(Some(match span {
        Span::AtLast { index } => track.keys[index].value.orientation,
        span => orient_between(version, &track.keys, &span).orientation,
    }))
}
