// SPDX-License-Identifier: MIT
//! A track's keys, its repeat setting, and the search for the keys around a time.
//!
//! (keys that share a time; the key a read takes depends on
//! the previous read of the same track)
//! (the 1e-10 line, strictly below, on the binary64 difference
//! a fresh track counts as read from its first key; a frame time that is not a number reads
//! at the last key's time)
//! (revised, log entries 2868 and 3076: a time map changes R(t); a map that does
//! not rise from 0 to 1 is refused, the implementer's choice)

use super::AnimError;
use super::time_map;
use crate::version::RenderAs;

/// What happens after a track's last key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// 0: the value stays at the last key's.
    None,
    /// 1: a closed loop. The last stretch runs to the first key's value.
    Loop,
    /// 2: back and forth.
    PingPong,
    /// 3: starting again.
    Restart,
}

impl Repeat {
    /// The setting as the document keeps it (0 to 3). Other numbers give `None`.
    pub fn from_setting(setting: u32) -> Option<Repeat> {
        match setting {
            0 => Some(Repeat::None),
            1 => Some(Repeat::Loop),
            2 => Some(Repeat::PingPong),
            3 => Some(Repeat::Restart),
            _ => None,
        }
    }
}

/// One key: a time in seconds and a value.
#[derive(Clone, Debug, PartialEq)]
pub struct Key<V> {
    pub time: f64,
    pub value: V,
}

/// One point of a time map, as the document holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeMapPoint {
    pub part: f32,
    pub value: f32,
    pub flags: u32,
}

/// A track's time map, and the version whose arithmetic reads it (the curve's last bits
/// differ between versions).
#[derive(Clone, Debug, PartialEq)]
pub struct TimeMap {
    pub points: Vec<TimeMapPoint>,
    pub version: RenderAs,
}

/// The map R of the repeat setting, from the frame time t to the time the keys are read at.
///
///
/// A frame time that is not a number gives the last key's time, for every repeat setting.
pub fn repeat_time(first: f64, last: f64, repeat: Repeat, t: f64) -> f64 {
    if t.is_nan() {
        return last;
    }
    // (Errors: a time of infinity gives the last key's time for the settings 0, 1
    // and 3, not wrapped back, and minus infinity for the setting 2)
    if t == f64::INFINITY {
        return if repeat == Repeat::PingPong { f64::NEG_INFINITY } else { last };
    }
    // (Errors: keys out of order of time are held to the first and the last key
    // as stored): the time held to the first key, then the held time against the last key.
    let t = if t < first { first } else { t };
    let at_last_with_loop = t == last && repeat == Repeat::Loop;
    if t < last || (t == last && !at_last_with_loop) {
        return t;
    }
    if repeat == Repeat::None {
        return last;
    }
    let length = last - first;
    if length == 0.0 {
        return first;
    }
    let x = (t - last) / length;
    let whole = x.floor(); // the whole part of x (x >= 0 here)
    let r = (x - whole) * length;
    match repeat {
        Repeat::Loop | Repeat::Restart => first + r,
        Repeat::PingPong => {
            let odd = (whole % 2.0) == 1.0;
            if odd { first + r } else { last - r }
        }
        Repeat::None => last,
    }
}

/// Where a time s falls among a track's keys.
///
/// (the key at or before s, compared exactly; the key after it; setting 1's
/// closing stretch runs to the first key's value)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    /// s is at or after the last key: the last key's value.
    AtLast { index: usize },
    /// s is on the stretch from key `from` to key `from + 1`.
    Between {
        from: usize,
        /// The key whose value the stretch runs to: `from + 1`, or 0 on the closing
        /// stretch of setting 1.
        to_value: usize,
        t0: f64,
        t1: f64,
        /// u = (s - t0) / (t1 - t0).
        u: f64,
        /// True on the closing stretch of setting 1.
        closing: bool,
    },
}

/// A time whose binary64 distance from the key a track's previous read took its value from
/// is strictly below this number (the binary64 number nearest 1e-10) stands on that key
/// again; a distance equal to it does not.
pub const SAME_KEY_TIME: f64 = 1e-10;

/// The key at s when s is above time(k), k the previous read's key: the keys after k taken
/// one by one in the order stored; the first whose time equals s is the key at s; at the
/// first whose time is above s the walk stops and the key before it is the key at s; when
/// none stops it, the last key. For keys in order of time: the first key at s, else the last
/// key before s.
///
fn key_at_after(times: &[f64], k: usize, s: f64) -> usize {
    for (j, &tj) in times.iter().enumerate().skip(k + 1) {
        if tj == s {
            return j;
        }
        if tj > s {
            return j - 1;
        }
    }
    times.len() - 1
}

/// The key at s when s is not above time(k): the keys before k taken one by one going back;
/// the first whose time equals s or is below s is the key at s; when none is, the first key.
/// For keys in order of time: the last key at s, else the last key before s.
///
fn key_at_before(times: &[f64], k: usize, s: f64) -> usize {
    (0..k).rev().find(|&j| times[j] <= s).unwrap_or(0)
}

/// What a track carries from one read to the next since its document was opened: the key
/// its previous read took its value from. A fresh memory stands for a track not
/// read before, which counts as read from its first key. A read of a track with
/// no key, or a refused read, leaves the memory as it was (question 3 (b)). One memory belongs to one track: keep it beside the track and pass it to
/// every read of that track.
///
/// ("Reads and their history")
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadMemory {
    previous_key: Option<usize>,
}

impl ReadMemory {
    /// A track not read before since its document was opened.
    pub fn new() -> Self {
        ReadMemory { previous_key: None }
    }

    /// The key the previous read took its value from, if the track was read before.
    pub fn previous_key(&self) -> Option<usize> {
        self.previous_key
    }

    /// Forgets every earlier read (as when the document is opened again, or, by our own
    /// choice, when a track's keys are changed: question 3 (c)).
    pub fn forget(&mut self) {
        self.previous_key = None;
    }

    /// Locates s (the time after the repeat map) among ascending key times (at least one
    /// key), given the previous read, and remembers the key this read takes.
    ///
    /// : with k the previous read's key
    /// (the first key for a track not read before), |time(k) - s| computed in binary64
    /// strictly below the binary64 1e-10 gives k again; otherwise s above time(k) gives the
    /// first key at s, else the last key before s; otherwise the last key at s, else the
    /// last key before s.
    pub fn locate(&mut self, times: &[f64], repeat: Repeat, s: f64) -> Span {
        // A remembered key beyond the keys (keys changed between reads) is a fresh start:
        // our choice, question 3 (c).
        let k = self.previous_key.filter(|&k| k < times.len()).unwrap_or(0);
        let tk = times[k];
        let i = if (tk - s).abs() < SAME_KEY_TIME {
            k
        } else if s > tk {
            key_at_after(times, k, s)
        } else {
            // s not above time(k). A key time that is not a number also lands here: our
            // choice, question 2 .
            key_at_before(times, k, s)
        };
        let span = Span::from_key(times, repeat, i, s);
        self.previous_key = Some(span.key());
        span
    }
}

impl Span {
    /// Locates s among ascending key times (at least one key) for a track not read
    /// before: the same as a fresh `ReadMemory`'s `locate`.
    ///
    /// ; animation (linear search; Freedom)
    pub fn locate(times: &[f64], repeat: Repeat, s: f64) -> Span {
        ReadMemory::new().locate(times, repeat, s)
    }

    /// As 3.1 and 5.0, a place or orientation track takes, before any other rule of its own,
    /// the value of the key at s as it is when s lies within 1e-10 of that key's time, and
    /// else the value of the key after it as it is when s lies within 1e-10 of that key's
    /// time (each distance in binary64, strictly below the binary64 1e-10). This gives the
    /// index of that key on a stretch, if either holds.
    ///
    pub fn key_held_as_is(&self, s: f64) -> Option<usize> {
        let Span::Between { from, t0, t1, .. } = *self else { return None };
        if (s - t0).abs() < SAME_KEY_TIME {
            Some(from)
        } else if (s - t1).abs() < SAME_KEY_TIME {
            Some(from + 1) // the key after it as stored (also on setting 1's closing stretch)
        } else {
            None
        }
    }

    /// The key at s that the span starts at: the key a later read remembers.
    pub fn key(&self) -> usize {
        match *self {
            Span::AtLast { index } => index,
            Span::Between { from, .. } => from,
        }
    }

    /// The span that starts at key i, the key at s.
    ///
    /// (the key after it; at the last key, the last key's value; setting 1's
    /// closing stretch runs to key 0's value)
    fn from_key(times: &[f64], repeat: Repeat, i: usize, s: f64) -> Span {
        let n = times.len();
        if i + 1 >= n {
            return Span::AtLast { index: n - 1 };
        }
        let closing = repeat == Repeat::Loop && i + 1 == n - 1;
        let t0 = times[i];
        let t1 = times[i + 1];
        Span::Between {
            from: i,
            to_value: if closing { 0 } else { i + 1 },
            t0,
            t1,
            u: (s - t0) / (t1 - t0),
            closing,
        }
    }
}

/// How a kind of track runs between its keys. animation
pub trait Interpolate: Sized + Clone {
    /// The value on the stretch `span` (always `Span::Between`), given all the keys.
    fn between(keys: &[Key<Self>], repeat: Repeat, span: &Span) -> Self;
}

/// A track: keys in ascending time, a repeat setting, and perhaps a time map.
///
#[derive(Clone, Debug, PartialEq)]
pub struct Track<V> {
    pub keys: Vec<Key<V>>,
    pub repeat: Repeat,
    pub time_map: Option<TimeMap>,
}

impl<V> Track<V> {
    /// A track without a time map.
    pub fn new(keys: Vec<Key<V>>, repeat: Repeat) -> Self {
        Track { keys, repeat, time_map: None }
    }

    /// The keys' times, in order.
    pub fn times(&self) -> Vec<f64> {
        self.keys.iter().map(|k| k.time).collect()
    }

    /// The time s = R(t) at which the keys are read. Gives `None` for a track with no key.
    ///
    /// With a time map, the time the map gives for R(t): first + L · m(u). A map that does not
    /// rise from 0 to 1 is refused (`AnimError::TimeMap`), the implementer's choice there.
    ///
    pub fn read_time(&self, t: f64) -> Result<Option<f64>, AnimError> {
        if self.keys.is_empty() {
            return Ok(None);
        }
        let first = self.keys[0].time;
        let last = self.keys[self.keys.len() - 1].time;
        let r = repeat_time(first, last, self.repeat, t);
        match &self.time_map {
            None => Ok(Some(r)),
            Some(map) if time_map::binds(map) => Ok(Some(time_map::mapped_time(map.version, map, first, last, r))),
            Some(_) => Err(AnimError::TimeMap),
        }
    }
}

impl<V: Interpolate> Track<V> {
    /// The track's value at frame time t. `Ok(None)` means the track has no key, and the
    /// owner keeps its value.
    ///
    pub fn value_at(&self, t: f64) -> Result<Option<V>, AnimError> {
        self.value_after(&mut ReadMemory::new(), t)
    }

    /// The track's value at frame time t, given what earlier reads of this track left in
    /// `memory`, which this read updates.
    ///
    pub fn value_after(&self, memory: &mut ReadMemory, t: f64) -> Result<Option<V>, AnimError> {
        let Some(s) = self.read_time(t)? else {
            return Ok(None);
        };
        let span = memory.locate(&self.times(), self.repeat, s);
        Ok(Some(match span {
            Span::AtLast { index } => self.keys[index].value.clone(),
            Span::Between { .. } => V::between(&self.keys, self.repeat, &span),
        }))
    }
}

/// A track together with what it carries from one read to the next: the evaluator a
/// document keeps for each track from the moment it is opened. Every read goes
/// through `read`, which updates the carried state.
///
/// ; ("Reads and their history")
#[derive(Clone, Debug)]
pub struct TrackReader<'a, V> {
    pub track: &'a Track<V>,
    pub memory: ReadMemory,
}

impl<'a, V> TrackReader<'a, V> {
    /// A reader of a track not read before.
    pub fn new(track: &'a Track<V>) -> Self {
        TrackReader { track, memory: ReadMemory::new() }
    }

    /// Reads the keys at frame time t: the span the value comes from, or `None` for a
    /// track with no key. Updates the carried state.
    ///
    pub fn read(&mut self, t: f64) -> Result<Option<Span>, AnimError> {
        let Some(s) = self.track.read_time(t)? else {
            return Ok(None);
        };
        Ok(Some(self.memory.locate(&self.track.times(), self.track.repeat, s)))
    }
}

impl<V: Interpolate> TrackReader<'_, V> {
    /// The value at frame time t (`Ok(None)` for a track with no key).
    pub fn value_at(&mut self, t: f64) -> Result<Option<V>, AnimError> {
        self.track.value_after(&mut self.memory, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_key_times_and_history() {
        let times = [0.0, 1.0, 1.0, 1.0, 2.0];
        assert_eq!(Span::locate(&times, Repeat::None, 1.0).key(), 1);
        for (earlier, want) in [(2.0, 3), (1.5, 3), (0.0, 1), (0.5, 1)] {
            let mut m = ReadMemory::new();
            m.locate(&times, Repeat::None, earlier);
            assert_eq!(m.locate(&times, Repeat::None, 1.0).key(), want, "after {earlier}");
        }
        // Within 1e-10 of the remembered key: that key again.
        let mut m = ReadMemory::new();
        m.locate(&times, Repeat::None, 1.0);
        assert_eq!(m.locate(&times, Repeat::None, 1.0 + 1e-11).key(), 1);
    }

    /// The key after a read on key `on` (at its own time) and then a read at s.
    fn key_after_read_on(times: &[f64], on: f64, s: f64) -> usize {
        let mut m = ReadMemory::new();
        m.locate(times, Repeat::None, on);
        m.locate(times, Repeat::None, s).key()
    }

    #[test]
    fn the_line_is_strictly_below_on_the_binary64_difference() {
        let times = [0.0, 1.0, 1.0, 2.0];
        let up = |n: f64| 1.0 + n * 2f64.powi(-52);
        let down = |n: f64| 1.0 - n * 2f64.powi(-53);
        assert_eq!(key_after_read_on(&times, 1.0, up(450359.0)), 1); // within: first key at 1
        assert_eq!(key_after_read_on(&times, 1.0, up(450360.0)), 2); // not: last key at 1
        assert_eq!(key_after_read_on(&times, 1.0, down(900719.0)), 1);
        assert_eq!(key_after_read_on(&times, 1.0, down(900720.0)), 0);
        let far = [0.0, 1000.25, 2000.0];
        let step = 2f64.powi(-43);
        assert_eq!(key_after_read_on(&far, 1000.25, 1000.25 + 879.0 * step), 1);
        assert_eq!(key_after_read_on(&far, 1000.25, 1000.25 + 880.0 * step), 1); // last before
        assert_eq!(key_after_read_on(&far, 1000.25, 1000.25 - 879.0 * step), 1);
        assert_eq!(key_after_read_on(&far, 1000.25, 1000.25 - 880.0 * step), 0);
        // Beside a key at 0, 1e-10 itself is not within; the next number toward 0 is.
        let zero = [0.0, 1e-10, 1.0];
        assert_eq!(Span::locate(&zero, Repeat::None, 1e-10).key(), 1);
        let inside = f64::from_bits(1e-10f64.to_bits() - 1);
        let zero2 = [0.0, inside, 1.0];
        assert_eq!(Span::locate(&zero2, Repeat::None, inside).key(), 0);
    }

    #[test]
    fn a_fresh_track_counts_as_read_from_its_first_key() {
        assert_eq!(Span::locate(&[0.0, 1e-11, 1.0], Repeat::None, 1e-11).key(), 0);
    }

    #[test]
    fn a_time_that_is_not_a_number_reads_at_the_last_key() {
        for r in [Repeat::None, Repeat::Loop, Repeat::PingPong, Repeat::Restart] {
            assert_eq!(repeat_time(0.5, 3.0, r, f64::NAN), 3.0);
        }
    }

    #[test]
    fn reads_without_a_value_leave_the_memory() {
        let before = ReadMemory { previous_key: Some(1) };
        let mut t = Track::new(
            vec![Key { time: 0.0, value: [0.0; 3] }, Key { time: 1.0, value: [1.0; 3] }],
            Repeat::None,
        );
        t.time_map = Some(TimeMap { points: Vec::new(), version: RenderAs::Bryce30 });
        let mut refused = TrackReader::new(&t);
        refused.memory = before;
        assert_eq!(refused.read(0.0), Err(AnimError::TimeMap));
        assert_eq!(refused.memory, before);
        let empty: Track<[f64; 3]> = Track::new(Vec::new(), Repeat::None);
        let mut reader = TrackReader::new(&empty);
        reader.memory = before;
        assert_eq!(reader.read(1.0), Ok(None));
        assert_eq!(reader.memory, before);
    }

    #[test]
    fn repeat_map_cases() {
        assert_eq!(repeat_time(1.0, 3.0, Repeat::Loop, 0.5), 1.0);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::None, 2.5), 2.5);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::None, 9.0), 3.0);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::Restart, 3.0), 3.0);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::Loop, 3.0), 1.0);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::Restart, 3.5), 1.5);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::PingPong, 3.5), 2.5);
        assert_eq!(repeat_time(1.0, 3.0, Repeat::PingPong, 5.5), 1.5);
        assert_eq!(repeat_time(2.0, 2.0, Repeat::Restart, 7.0), 2.0);
        // (Errors: a time of infinity; keys out of order held to the first key,
        // then to the last, as stored)
        for r in [Repeat::None, Repeat::Loop, Repeat::Restart] {
            assert_eq!(repeat_time(0.0, 30.0, r, f64::INFINITY), 30.0);
        }
        assert_eq!(repeat_time(0.0, 30.0, Repeat::PingPong, f64::INFINITY), f64::NEG_INFINITY);
        assert_eq!(repeat_time(2.0, 1.0, Repeat::None, 0.5), 1.0);
        assert_eq!(repeat_time(0.0, 1.0, Repeat::None, 1.5), 1.0);
    }

    #[test]
    fn keys_out_of_order_walk_from_the_remembered_key() {
        // : from the key remembered, the keys
        // after it in the order stored (stop at the first above s), or those before it going
        // back (the first at or below s)
        let times = [2.0, 0.0, 1.0];
        let mut m = ReadMemory::new();
        // s = 1 below key 0's time: none before it, the first key; the stretch to key 1.
        let span = m.locate(&times, Repeat::None, 1.0);
        assert!(matches!(span, Span::Between { from: 0, u, .. } if u == 0.5));
        let times = [0.0, 2.0, 1.0];
        // s = 1 above key 0's time: key 1 (2) stops the walk: key 0.
        assert_eq!(Span::locate(&times, Repeat::None, 1.0).key(), 0);
        // From key 1 remembered, s = 1.5 not above 2: going back, key 0.
        let mut m = ReadMemory { previous_key: Some(1) };
        assert_eq!(m.locate(&times, Repeat::None, 1.5).key(), 0);
        // s above every key after k: the last key.
        assert_eq!(Span::locate(&[0.0, 1.0, 2.0], Repeat::None, 5.0).key(), 2);
    }

    #[test]
    fn empty_track_and_time_map() {
        let empty: Track<[f64; 3]> = Track::new(Vec::new(), Repeat::None);
        assert_eq!(empty.value_at(1.0), Ok(None));
        let mut t = Track::new(vec![Key { time: 0.0, value: [1.0, 2.0, 3.0] }], Repeat::None);
        assert_eq!(t.value_at(5.0), Ok(Some([1.0, 2.0, 3.0])));
        t.time_map = Some(TimeMap { points: Vec::new(), version: RenderAs::Bryce30 });
        assert_eq!(t.value_at(5.0), Err(AnimError::TimeMap));
    }
}
