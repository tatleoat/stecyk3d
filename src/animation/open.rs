// SPDX-License-Identifier: MIT
//! The parts of the animation entries that are specified only in outline: what a document
//! holds when it is opened, and refusals for the open parts.
//!
//! , (revised, log entry
//! 1831), (the opening application is a
//! read), (Errors: a
//! document of Bryce 5's form opened as 3.1)

use super::AnimError;
use super::track::{Interpolate, TrackReader};
use crate::version::RenderAs;

/// The names of the eight view cameras (interface names of), "perspective" first.
const VIEW_CAMERA_NAMES: [&[u8]; 8] =
    [b"perspective", b"top", b"right", b"front", b"left", b"back", b"bottom", b"director"];

/// The form of a document's parts: Bryce 3D's (placement part version 1, light part 1,
/// sky part 1 or 2) or Bryce 5's (placement part 2, light part 3, sky part 7). Opening as
/// 5.0 with the animation switch set depends on it .
/// , point 1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartsForm {
    Bryce3d,
    Bryce5,
}

/// What opening reads from a document: its saved time (offset 68, binary32), the
/// lowest byte of its animation switch word (offset 92) and the form of its parts
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpeningInputs {
    pub saved_time: f32,
    pub animation_switch: u8,
    /// The opening time factor (offset 148, binary32): as 5.0 the tracks are applied at
    /// the saved time times it.
    pub opening_factor: f32,
    pub form: PartsForm,
    /// The time a frame asks for (binary64), `None` for the document as it opens at
    /// its saved time.
    pub asked_time: Option<f64>,
}

impl OpeningInputs {
    /// A document in Bryce 3D's form (a read document's own form is `Scene::form`).
    pub fn bryce3d(saved_time: f32, animation_switch: u8) -> OpeningInputs {
        OpeningInputs { saved_time, animation_switch, opening_factor: 1.0, form: PartsForm::Bryce3d, asked_time: None }
    }

    /// The same document with the opening time factor `k`.
    pub fn with_factor(self, k: f32) -> OpeningInputs {
        OpeningInputs { opening_factor: k, ..self }
    }

    /// The same document asked for at the time `t` of a frame. The programs set the animation
    /// switch while they apply a frame's tracks (and set it back after), so a frame is applied as
    /// where the switch is set, whatever the document holds. (Which tracks are
    /// applied)
    pub fn at_time(self, t: f64) -> OpeningInputs {
        OpeningInputs { asked_time: Some(t), animation_switch: 1, ..self }
    }
}

/// The time a frame's tracks are read at: the time asked, a time below 0 (and a negative
/// zero) taken as 0. (A time below 0 is the implementer's choice; taking it as 0
/// is acceptable)
pub fn frame_time(t: f64) -> f64 {
    if t <= 0.0 { 0.0 } else { t }
}

/// The time at which a freshly opened document's tracks are applied: its saved time,
/// widened exactly from binary32. A negative saved time counts as 0 (offset 68);
/// so does a negative zero (Inputs, point 3: the implementer's choice). A
/// saved time that is not a number is passed on as it is (point 3: acceptable;
/// reads such a time at the last key's time). Keys are binary64, so a document
/// saved on a key is usually a few 1e-8 away from it, and the value between keys is used.
/// Applying a track at this time is that track's first read: keep its `TrackReader` for
/// the frames after.
///
/// , point 3,
pub fn opening_time(saved_time: f32) -> f64 {
    let t = saved_time as f64;
    if t <= 0.0 { 0.0 } else { t }
}

/// The time at which 5.0 applies the tracks of a document whose switch is not 0: the saved
/// time s times the opening time factor k, the exact binary64 product (the product of two
/// binary32 numbers is exact in binary64; it is not rounded to binary32 on the way,
/// , observed); a product below 0 counts as 0, negative zero too; a product that is
/// not a number is passed on as the saved time is.
pub fn opening_time_scaled(saved_time: f32, factor: f32) -> f64 {
    let p = saved_time as f64 * factor as f64;
    if p <= 0.0 { 0.0 } else { p }
}

/// What opening applies, where gives it.,
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningRule {
    /// No track is applied; every owner keeps the value its record holds and the
    /// perspective camera's rows stand as stored (3.1 and 5.0 with the switch 0).
    AsStored,
    /// Every track is applied at the saved time, the "perspective" view camera's included,
    /// and that camera's rows are made from its angles (3.0, whatever the switch).
    EveryTrack,
    /// Every owner with a track that has keys takes the track's value at time 0 (objects
    /// with their materials, lights, sky, sun; linked objects as); for the view
    /// cameras, whether their tracks are applied, and whether the perspective camera's rows
    /// are made from its angles, are Open (5.0, the switch set, saved time 0, Bryce 5's form).
    EveryTrackButViewCameras,
    /// The animation switch set, as 3.1 or 5.0 (Outputs, as revised): every owner with a
    /// track that has keys takes the track's value at the opening time (the saved time; as 5.0
    /// in a document of Bryce 3D's form time 0), linked objects as; of the view
    /// cameras the first "perspective" has its field of view, angles and place tracks applied
    /// and its rows made from its angles, no other view camera has a track applied (the
    /// observed director and top, the fallback for the rest).
    Marked {
        /// Whether the opening time is the saved time times the opening time factor (5.0, in
        /// either form of the parts) and not the saved time (3.1).
        by_factor: bool,
    },
}

impl OpeningRule {
    /// Whether the owners' tracks (the view cameras aside) are applied.
    pub fn applies_tracks(self) -> bool {
        !matches!(self, OpeningRule::AsStored)
    }

    /// Whether the sky's and the sun's tracks are applied: wherever any is.
    /// (Outputs: the sky's ambient colour and ambience as 5.0 in both forms
    /// of the parts; the sky's other tracks and the sun's Open, the fallback as the others)
    pub fn applies_sky_tracks(self) -> bool {
        self.applies_tracks()
    }
}

/// The one rule for what each version applies on opening (and so whether the perspective
/// camera's rows are made from its angles):
///
/// - 3.0: every track at the saved time, whatever the switch (it does not read it).
/// - 3.1 and 5.0 with the switch 0: no track.
/// - 3.1 and 5.0 with the switch not 0: every track (`Marked`), at the saved time as 3.1 and at
/// the saved time times the opening time factor as 5.0 .
///
/// `Opening::new` and the scene reader's view (`scene_view::view_transforms`) both decide
/// by this function, so the two cannot disagree.
///
/// , point 1, (the animation switch and the
/// version),
pub fn tracks_applied_on_opening(version: RenderAs, inputs: OpeningInputs) -> Result<OpeningRule, AnimError> {
    if inputs.asked_time.is_some() {
        // : as 3.0 all of them; as 3.1 and 5.0 where
        // the switch is not 0 all of them as gives for 5.0 in Bryce 5's form at time
        // 0, the view cameras aside, with t in the place of 0 (the fallback); where it is 0,
        // 's: none.
        // : as 3.1 and 5.0 a frame asked
        // through the time control or a movie applies every track whatever the switch holds (the
        // programs set the switch while they apply the tracks); only a document as it opens with
        // the switch 0 stands as stored.
        return Ok(match version {
            RenderAs::Bryce30 => OpeningRule::EveryTrack,
            _ => OpeningRule::EveryTrackButViewCameras,
        });
    }
    match version {
        RenderAs::Bryce30 => Ok(OpeningRule::EveryTrack),
        _ if inputs.animation_switch == 0 => Ok(OpeningRule::AsStored),
        // : 3.1 and 5.0 with the switch set apply the
        // tracks, 3.1 at the saved time and 5.0 at s times k, in either form of the parts, the
        // sky's (ambient colour, ambience) and the sun's included (the others: the fallback).
        // 3.1 in a document of Bryce 5's form: leaves it to the implementer (the
        // original 3.1 draws one colour); this reader renders it by 3.1's rules
        // the same rule (an acceptable choice).
        RenderAs::Bryce31 => Ok(OpeningRule::Marked { by_factor: false }),
        RenderAs::Bryce50 => Ok(OpeningRule::Marked { by_factor: true }),
    }
}

/// What opening a document does with its tracks, for one version and one document.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    /// The version the document is opened as.
    pub version: RenderAs,
    /// What opening applies.
    pub rule: OpeningRule,
    /// Whether the document's tracks (the view cameras' aside) are applied on opening.
    pub applies_tracks: bool,
    /// The time they are applied at (`opening_time` of the saved time; 0 where 5.0 applies
    /// them, the only saved time for which it does).
    pub time: f64,
}

impl Opening {
    /// Decides the opening from the version and what the document holds (saved time,
    /// animation switch, form of its parts), by `tracks_applied_on_opening`. An error where
    /// leaves the opening Open.
    ///
    /// , point 1
    pub fn new(version: RenderAs, inputs: OpeningInputs) -> Result<Opening, AnimError> {
        let rule = tracks_applied_on_opening(version, inputs)?;
        let time = match inputs.asked_time {
            // : as 5.0 a frame's tracks are
            // applied at its time times the document's opening time factor (offset 148, a
            // binary32 widened), the binary64 product; as 3.0 and 3.1 at its time.
            Some(t) if version == RenderAs::Bryce50 => frame_time(frame_time(t) * f64::from(inputs.opening_factor)),
            Some(t) => frame_time(t),
            None if matches!(rule, OpeningRule::Marked { by_factor: true, .. }) => {
                opening_time_scaled(inputs.saved_time, inputs.opening_factor)
            }
            None => opening_time(inputs.saved_time),
        };
        Ok(Opening { version, rule, applies_tracks: rule.applies_tracks(), time })
    }

    /// Whether the rows of the second transform of the first camera named "perspective"
    /// are made from its two angles : as 3.0 yes, where no track is applied no,
    /// and where 5.0 applies tracks Open (point 1, point 1).
    ///
    /// , point 1,
    pub fn perspective_rows_from_angles(&self) -> Result<bool, AnimError> {
        match self.rule {
            OpeningRule::AsStored => Ok(false),
            OpeningRule::EveryTrack => Ok(true),
            OpeningRule::Marked { .. } => Ok(true),
            // With the switch not 0 the rows are made from the angles in 3.1 and 5.0 too
            // (: observed).
            OpeningRule::EveryTrackButViewCameras => Ok(true),
        }
    }

    /// Whether the tracks of a view camera (the first camera of one of's eight
    /// names) are applied on opening. `name` is the camera's name as read.
    ///
    /// - Where no track is applied, no camera's is.
    /// - As 3.0, "perspective"'s are; for the other seven names whether they are is Open
    /// (point 2).
    /// - Where 5.0 applies tracks, Open for all eight (point 1).
    ///
    /// The caller asks only when such a camera has a track with keys, so a refusal falls
    /// only on a document with such a track. A camera that is not the first of a view name
    /// is not a view camera and is not decided here. Returns `None` for a name that is not
    /// a view camera's.
    ///
    /// , points 1 and 2
    pub fn view_camera_tracks_applied(&self, name: &[u8]) -> Option<Result<bool, AnimError>> {
        let index = VIEW_CAMERA_NAMES.iter().position(|n| *n == name)?;
        Some(match self.rule {
            OpeningRule::AsStored => Ok(false),
            // : "perspective" has its tracks applied; the others
            // have none applied (observed for the director and top, the fallback for the rest)
            OpeningRule::EveryTrack | OpeningRule::Marked { .. } => Ok(index == 0),
            OpeningRule::EveryTrackButViewCameras => Err(AnimError::Unsupported(
                "an animated view camera of a document opened at a time asked, as 3.1 or 5.0 (SO-2.8: open)",
            )),
        })
    }

    /// The value an owner's setting holds when the document is opened: where tracks are
    /// applied, the track's value at the opening time (or `stored` when the track has no
    /// key); otherwise `stored`, and the reader is left unread, so the first read of the
    /// track is the first frame's. For a view camera's track, ask
    /// `view_camera_tracks_applied` first.
    ///
    /// , point 1,,
    pub fn value<V: Interpolate>(&self, reader: &mut TrackReader<'_, V>, stored: V) -> Result<V, AnimError> {
        if !self.applies_tracks {
            return Ok(stored);
        }
        Ok(reader.value_at(self.time)?.unwrap_or(stored))
    }
}

/// A camera of a linked list as an object: its view is's (`scene::linked_view`);
/// its own transform as an object, and what objects linked below it take, are open.
pub fn refuse_linked_camera() -> AnimError {
    AnimError::Unsupported("an object linked below a camera that is itself linked (AN-1.9: open)")
}

/// An object linked to a path where leaves it open: a path linked below another
/// object, or an object with bits of 0xf80000 linked to a path.
pub fn refuse_linked_to_path() -> AnimError {
    AnimError::Unsupported("an object linked to a path below another object, or with bits of 0xf80000 (AN-1.9: open)")
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Key, Repeat, Track};

    fn b3(t: f32, switch: u8) -> OpeningInputs {
        OpeningInputs::bryce3d(t, switch)
    }

    fn b5(t: f32, switch: u8) -> OpeningInputs {
        OpeningInputs { saved_time: t, animation_switch: switch, opening_factor: 1.0, form: PartsForm::Bryce5, asked_time: None }
    }

    #[test]
    fn saved_time() {
        // offset 68 (a negative time counts as 0), point 3
        assert_eq!(opening_time(-2.5).to_bits(), 0.0f64.to_bits());
        assert_eq!(opening_time(-0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(opening_time(0.1), 0.1f32 as f64);
        assert!(opening_time(f32::NAN).is_nan());
    }

    #[test]
    fn versions_and_switch() {
        for switch in [0u8, 1, 255] {
            for inputs in [b3(1.0, switch), b5(0.0, switch)] {
                let o = Opening::new(RenderAs::Bryce30, inputs).unwrap();
                assert!(o.applies_tracks && o.perspective_rows_from_angles() == Ok(true));
                assert_eq!(o.rule, OpeningRule::EveryTrack);
            }
        }
        for v in [RenderAs::Bryce31, RenderAs::Bryce50] {
            for inputs in [b3(1.0, 0), b5(0.0, 0), b5(1.0, 0)] {
                let o = Opening::new(v, inputs).unwrap();
                assert!(!o.applies_tracks && o.perspective_rows_from_angles() == Ok(false));
                assert_eq!(o.view_camera_tracks_applied(b"top"), Some(Ok(false)));
                assert_eq!(o.view_camera_tracks_applied(b"perspective"), Some(Ok(false)));
            }
        }
        let o = Opening::new(RenderAs::Bryce30, b3(1.0, 0)).unwrap();
        assert_eq!(o.view_camera_tracks_applied(b"perspective"), Some(Ok(true)));
        assert_eq!(o.view_camera_tracks_applied(b"director"), Some(Ok(false)));
        assert_eq!(o.view_camera_tracks_applied(b".1 saved"), None);
    }

    #[test]
    fn a_frame_applies_every_track_whatever_the_switch() {
        // : as 3.1 and 5.0 too where the
        // document's switch is 0; a document as it opens with the switch 0 stands as stored.
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let o = Opening::new(v, b3(1.0, 0).at_time(2.0)).unwrap();
            assert!(o.applies_tracks, "{v:?}");
            assert_eq!(o.time, 2.0);
        }
        assert!(!Opening::new(RenderAs::Bryce31, b3(1.0, 0)).unwrap().applies_tracks);
    }

    #[test]
    fn marked_documents() {
        // : 3.1 with the switch set applies every track at the
        // saved time; 5.0, in either form, at the saved time times the opening time factor (a
        // product below 0 counting as 0), the sky of Bryce 3D's form as stored.
        let on_5 = OpeningRule::Marked { by_factor: true };
        let on_31 = OpeningRule::Marked { by_factor: false };
        for t in [0.0f32, -0.0, -3.0] {
            let o = Opening::new(RenderAs::Bryce50, b5(t, 1)).unwrap();
            assert_eq!(o.rule, on_5);
            assert!(o.applies_tracks);
            assert_eq!(o.time.to_bits(), 0.0f64.to_bits());
            assert_eq!(o.perspective_rows_from_angles(), Ok(true));
            for name in VIEW_CAMERA_NAMES {
                assert_eq!(o.view_camera_tracks_applied(name), Some(Ok(name == b"perspective")));
            }
            assert_eq!(o.view_camera_tracks_applied(b"camera 2"), None);
        }
        let o = Opening::new(RenderAs::Bryce50, b5(0.5, 1)).unwrap();
        assert_eq!((o.rule, o.time), (on_5, 0.5));
        let o = Opening::new(RenderAs::Bryce31, b3(1.0, 1)).unwrap();
        assert_eq!((o.rule, o.time), (on_31, 1.0));
        // The factor: 5.0 only, either form; k = 0 gives time 0, k = 0.5 half the saved time.
        for (k, want) in [(0.0f32, 0.0), (0.5, 1.0), (1.0, 2.0), (2.0, 4.0), (-1.0, 0.0)] {
            for inputs in [b3(2.0, 1).with_factor(k), b5(2.0, 1).with_factor(k)] {
                assert_eq!(Opening::new(RenderAs::Bryce50, inputs).unwrap().time, want, "k = {k}");
                assert_eq!(Opening::new(RenderAs::Bryce31, inputs).unwrap().time, 2.0);
                assert_eq!(Opening::new(RenderAs::Bryce30, inputs).unwrap().time, 2.0);
            }
        }
        // The smallest binary32: the product is exact in binary64 and positive.
        let tiny = f32::from_bits(1);
        assert_eq!(opening_time_scaled(2.0, tiny), 2.0 * tiny as f64);
        assert_eq!(opening_time_scaled(-0.0, 1.0).to_bits(), 0.0f64.to_bits());
        let o = Opening::new(RenderAs::Bryce50, b3(1.0, 1)).unwrap();
        assert_eq!((o.rule, o.time), (OpeningRule::Marked { by_factor: true }, 1.0));
        // : the sky's tracks are applied as 5.0 in Bryce 3D's form too
        assert!(o.rule.applies_sky_tracks() && on_31.applies_sky_tracks());
        // The product is not rounded to binary32: 5.0 reads at the exact product .
        let (s, k) = (1.0f32 + f32::EPSILON, 1.0f32 + 2049.0 * f32::EPSILON / 2.0);
        assert_eq!(opening_time_scaled(s, k), s as f64 * k as f64);
        assert_ne!(opening_time_scaled(s, k), (s as f64 * k as f64) as f32 as f64);
        assert!(Opening::new(RenderAs::Bryce50, b5(f32::NAN, 1)).unwrap().time.is_nan());
    }

    #[test]
    fn value_reads_only_where_applied() {
        // point 1,
        let track = Track::new(vec![Key { time: 0.0, value: 1.0f64 }, Key { time: 2.0, value: 3.0 }], Repeat::None);
        let o = Opening::new(RenderAs::Bryce30, b3(1.0, 0)).unwrap();
        let mut r = TrackReader::new(&track);
        assert_eq!(o.value(&mut r, 9.0).unwrap(), 2.0);
        assert_eq!(r.memory.previous_key(), Some(0));
        let o = Opening::new(RenderAs::Bryce31, b3(1.0, 0)).unwrap();
        let mut r = TrackReader::new(&track);
        assert_eq!(o.value(&mut r, 9.0).unwrap(), 9.0);
        assert_eq!(r.memory.previous_key(), None);
        let empty: Track<f64> = Track::new(vec![], Repeat::None);
        let o = Opening::new(RenderAs::Bryce30, b3(1.0, 0)).unwrap();
        assert_eq!(o.value(&mut TrackReader::new(&empty), 9.0).unwrap(), 9.0);
        // 5.0, marked, Bryce 5's form at 0: the track's value at time 0.
        let o = Opening::new(RenderAs::Bryce50, b5(0.0, 1)).unwrap();
        let mut r = TrackReader::new(&track);
        assert_eq!(o.value(&mut r, 9.0).unwrap(), 1.0);
        assert_eq!(r.memory.previous_key(), Some(0));
    }
}
