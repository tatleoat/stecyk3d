// SPDX-License-Identifier: MIT
//! A material's channel track at a time: the key's whole channel, the blend between two keys,
//! and the second channel a track makes where its keys name different textures.
//!

use super::AnimError;
use super::track::{Span, Track};
use crate::scene_parts::{Channel, Rgb};

/// Keys within this distance of the read time (strictly) give their whole channel.
pub const CHANNEL_KEY_GAP: f64 = 0.0001;

/// ⟨x⟩: the binary32 nearest x.
fn r32(x: f64) -> f32 {
    x as f32
}

/// The numbers of a channel (A and B) and how they run between two keys.
pub trait ChannelNumbers: Copy {
    /// A between key values a and b at u.
    fn blend_a(a: Self, b: Self, u: f64) -> Self;
    /// B between key values a and b at u.
    fn blend_b(a: Self, b: Self, u: f64) -> Self;
    /// All numbers 0.
    fn zero() -> Self;
}

/// ⟨(b − a)·u + a⟩, the difference, product and sum in binary64.
fn blend_single(a: f32, b: f32, u: f64) -> f32 {
    let (a, b) = (f64::from(a), f64::from(b));
    r32((b - a) * u + a)
}

/// A value channel: A and B each ⟨(b − a)·u + a⟩.
impl ChannelNumbers for f32 {
    fn blend_a(a: f32, b: f32, u: f64) -> f32 {
        blend_single(a, b, u)
    }
    fn blend_b(a: f32, b: f32, u: f64) -> f32 {
        blend_single(a, b, u)
    }
    fn zero() -> f32 {
        0.0
    }
}

/// A colour channel.
impl ChannelNumbers for Rgb {
    fn zero() -> Rgb {
        [0.0; 3]
    }
    /// Each of the three: ⟨⟨u·(b − a)⟩ + a⟩.
    fn blend_a(a: Rgb, b: Rgb, u: f64) -> Rgb {
        std::array::from_fn(|k| {
            let (ak, bk) = (f64::from(a[k]), f64::from(b[k]));
            r32(f64::from(r32(u * (bk - ak))) + ak)
        })
    }
    /// With ū = ⟨u⟩: ⟨⟨ū·(b0 − a0)⟩ + a0⟩, ⟨⟨ū·⟨b1 − a1⟩⟩ + a1⟩, ⟨ū·⟨b2 − a2⟩ + a2⟩.
    fn blend_b(a: Rgb, b: Rgb, u: f64) -> Rgb {
        let ub = f64::from(r32(u));
        let (a0, b0) = (f64::from(a[0]), f64::from(b[0]));
        let (a1, b1) = (f64::from(a[1]), f64::from(b[1]));
        let (a2, b2) = (f64::from(a[2]), f64::from(b[2]));
        [
            r32(f64::from(r32(ub * (b0 - a0))) + a0),
            r32(f64::from(r32(ub * f64::from(r32(b1 - a1)))) + a1),
            r32(ub * f64::from(r32(b2 - a2)) + a2),
        ]
    }
}

/// The channel between two keys: key `from`'s references and switch word, the numbers
/// blended.
pub fn blend_channel<T: ChannelNumbers>(from: &Channel<T>, to: &Channel<T>, u: f64) -> Channel<T> {
    Channel {
        refs: from.refs,
        flags: from.flags,
        a: T::blend_a(from.a, to.a, u),
        b: T::blend_b(from.b, to.b, u),
        alpha: blend_single(from.alpha, to.alpha, u),
    }
}

/// The second channel a track gives between two keys whose references differ: the next key's
/// whole channel and the mix amount m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondChannel<T> {
    pub channel: Channel<T>,
    pub amount: f64,
}

/// A channel track's value at a time: the material's own channel, and a second channel where
/// the track makes one (else m = 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelAt<T> {
    pub own: Channel<T>,
    pub second: Option<SecondChannel<T>>,
}

/// A channel track's value at frame time t (`Ok(None)` for a track with no key), read as a
/// track not read before. Refused:'s Open points (a time in
/// the closing stretch of setting 1; two keys with the same references and different switch
/// words; a second channel where the track has a time map or the repeat setting moved the
/// time, where the amount may differ from u) and a time map refuses.
///
pub fn channel_at<T: ChannelNumbers>(track: &Track<Channel<T>>, t: f64) -> Result<Option<ChannelAt<T>>, AnimError> {
    channel_read(track, t, None)
}

/// A channel track's value at the time t a frame asks , as `version` reads
/// it, and the time a document opens at : as `channel_at`, except where the track
/// repeats and t lies past its last key, or has a time map. There, as 3.0, the second
/// channel's amount is m = (t − t_i) / (t_(i+1) − t_i) with t as asked (not wrapped, not taken
/// through the map), so it can be above 1; as 3.1 and 5.0,'s u of the wrapped or
/// mapped time.
pub fn channel_at_asked<T: ChannelNumbers>(
    track: &Track<Channel<T>>,
    t: f64,
    version: crate::version::RenderAs,
) -> Result<Option<ChannelAt<T>>, AnimError> {
    channel_read(track, t, Some(version))
}

/// `channel_at` (`asked` None: the amount where the read time is moved is refused) and
/// `channel_at_asked` (`asked` the version). (Frames)
fn channel_read<T: ChannelNumbers>(
    track: &Track<Channel<T>>,
    t: f64,
    asked: Option<crate::version::RenderAs>,
) -> Result<Option<ChannelAt<T>>, AnimError> {
    let Some(s) = track.read_time(t)? else { return Ok(None) };
    // ,: keys out of order of time are neither sorted nor refused;
    // the key at s is found by's walk and the key after it is the next key stored.
    let times = track.times();
    let whole = |i: usize| Ok(Some(ChannelAt { own: track.keys[i].value, second: None }));
    let (i, closing) = match Span::locate(&times, track.repeat, s) {
        Span::AtLast { index } => return whole(index),
        Span::Between { from, closing, .. } => (from, closing),
    };
    let (ti, tn) = (times[i], times[i + 1]);
    if (s - ti).abs() < CHANNEL_KEY_GAP {
        return whole(i);
    }
    if (s - tn).abs() < CHANNEL_KEY_GAP {
        return whole(i + 1);
    }
    // ('s closing stretch of the repeat setting 1 is not used here: the
    // last stretch blends from the key before the last to the last key, like any other)
    let _ = closing;
    let (from, to) = (&track.keys[i].value, &track.keys[i + 1].value);
    let u = (s - ti) / (tn - ti);
    let own = blend_channel(from, to, u);
    // (a second channel where some reference is a texture and the switch words or
    // the reference triples differ)
    if (from.refs == to.refs && from.flags == to.flags) || !from.refs.iter().chain(&to.refs).any(Option::is_some) {
        return Ok(Some(ChannelAt { own, second: None }));
    }
    let amount = match asked {
        None if track.time_map.is_some() || s != t => {
            return Err(AnimError::Unsupported(
                "a material's second channel on a track with a time map or read where it repeats (AN-1.8: the amount is open)",
            ));
        }
        None => u,
        // (the mix amount: as 3.0 on a track that repeats, past the last key, and on a
        // track with a time map, counted from the time as asked, not taken through the map or
        // the wrap),
        Some(crate::version::RenderAs::Bryce30) => {
            // : as 3.0, a time asked past
            // the last key of a track that repeats makes the original take the last key and the
            // place after it, where the track holds no key (time 0, an empty channel: no
            // references, every number 0), so m = (t - t_last) / (0 - t_last), below 0, and the
            // second channel is that empty one; the own channel stays the one found at s.
            let t_last = times[times.len() - 1];
            if track.repeat != super::track::Repeat::None && t > t_last && t_last > 0.0 {
                let empty = Channel { refs: [None; 3], flags: 0, a: T::zero(), b: T::zero(), alpha: 0.0 };
                return Ok(Some(ChannelAt {
                    own,
                    second: Some(SecondChannel { channel: empty, amount: (t - t_last) / (0.0 - t_last) }),
                }));
            }
            (t - ti) / (tn - ti)
        }
        Some(_) => u,
    };
    Ok(Some(ChannelAt { own, second: Some(SecondChannel { channel: *to, amount }) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Key, Repeat};
    use crate::scene_parts::TextureRef;
    use crate::scene_records::FourCc;

    fn channel(refs: [Option<TextureRef>; 3], flags: u32, a: f32) -> Channel<f32> {
        Channel { refs, flags, a, b: a, alpha: a }
    }

    #[test]
    fn whole_keys_blend_and_second() {
        let tex = |id| Some(TextureRef { code: FourCc::new(b"brtx"), identity: id });
        let k0 = channel([tex(1), None, None], 2, 0.0);
        let k1 = channel([tex(2), None, None], 0, 1.0);
        let track = Track::new(vec![Key { time: 0.0, value: k0 }, Key { time: 1.0, value: k1 }], Repeat::None);
        assert_eq!(channel_at(&track, 0.00005).unwrap().unwrap(), ChannelAt { own: k0, second: None });
        assert_eq!(channel_at(&track, 0.99995).unwrap().unwrap(), ChannelAt { own: k1, second: None });
        let at = channel_at(&track, 0.25).unwrap().unwrap();
        assert_eq!(at.own.refs, k0.refs);
        assert_eq!(at.own.flags, 2);
        assert_eq!(at.own.a, 0.25);
        assert_eq!(at.second, Some(SecondChannel { channel: k1, amount: 0.25 }));
        // Same references, different switch words: a second channel too.
        let k2 = channel([tex(1), None, None], 0, 1.0);
        let t2 = Track::new(vec![Key { time: 0.0, value: k0 }, Key { time: 1.0, value: k2 }], Repeat::None);
        assert_eq!(channel_at(&t2, 0.5).unwrap().unwrap().second, Some(SecondChannel { channel: k2, amount: 0.5 }));
        // Repeating back and forth past the last key: the amount is open.
        let t3 = Track::new(track.keys.clone(), Repeat::PingPong);
        assert!(channel_at(&t3, 1.5).is_err());
        assert!(channel_at(&t3, 0.5).unwrap().unwrap().second.is_some());
    }
}
