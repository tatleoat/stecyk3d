// SPDX-License-Identifier: MIT
//! Animation: tracks, keys, and the value of an animated setting at a time.
//!
//!
//! are implemented (as revised: 3.0 applies
//! every track on opening, 3.1 and 5.0 none where the animation switch is 0; 5.0 with the
//! switch set applies every track at 0 in a document of Bryce 5's form saved at 0, the view
//! cameras aside ; the other marked cases are refused as Open).
//! is implemented for linked objects in `link` (the chain
//! of second transforms, the bits of 0xf80000); objects and cameras held to a path and
//! cameras linked to an object (revised) in `scene_opening` and `scene::linked_view`,
//! their Open parts refused. (objects and cameras that point at
//! their targets) is in `aim` (the turn) and `scene_tracking` (when it is done).
//! (revised) are implemented in `material` and `texture`; only as far as
//! the entry specifies it. Open parts are refused with an `AnimError` that names what is
//! not supported.

mod aim;
mod ext_sine;
mod lerp;
mod link;
mod material;
mod open;
mod orient;
mod path;
mod place;
mod texture;
pub mod time_map;
mod track;

pub use ext_sine::{ExtValue, x87_sine};
pub use lerp::{lerp_f32, lerp_f32_triple, lerp_f64, lerp_f64_triple};
pub use open::{
    Opening, OpeningInputs, OpeningRule, PartsForm, frame_time, opening_time, refuse_linked_camera,
    refuse_linked_to_path, tracks_applied_on_opening,
};
pub use aim::{
    AIM_AWAY, AIM_X, AIM_Y, AIM_Z, Aim, Axis, CAMERA_BIT, LEAST_LENGTH, LEAST_LENGTH_CAMERA_LATER, LEVEL_BOUND, aim_rows, camera_turn, camera_turn_angles, direction,
    into_parent_space, turned_in_world, turned_second,
};
pub use material::{CHANNEL_KEY_GAP, ChannelAt, ChannelNumbers, SecondChannel, blend_channel, channel_at, channel_at_asked};
pub use texture::{MappingWords, blend_texture_settings, texture_record_at, texture_settings_at};
pub use link::{
    BIT_NO_PLACE, BIT_NO_SCALE, BIT_NO_SHEAR, BIT_NO_TURN, BIT_PLACE_ADDED, Decomposed, LinkArithmetic,
    PARTIAL_BITS,
};
pub use orient::{OrientKey, orientation_after, orientation_at, slerp};
pub use path::{Knot, Path, PathSample};
pub use place::{PlaceKey, place_after, place_at};
pub use track::{
    Interpolate, Key, ReadMemory, Repeat, Span, TimeMap, TimeMapPoint, Track, TrackReader,
    repeat_time,
};

/// A quaternion in the order x, y, z, w (the vectors' order). animation
pub type Quat = [f64; 4];

/// Why an animated value cannot be given.
///
/// animation (the refusals of open behaviour)
#[derive(Clone, Debug, PartialEq)]
pub enum AnimError {
    /// The track has a time map that does not rise from 0 to 1 (refused: Errors, Open).
    TimeMap,
    /// A kind of track or link that is not supported. The message names it.
    Unsupported(&'static str),
    /// The version does not open the document (Errors: as 3.0 a document with a part
    /// of Bryce 5's versions; as 3.1 one whose parts, so read, do not make a document).
    NotOpened(String),
}

impl std::fmt::Display for AnimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimError::TimeMap => {
                write!(f, "not supported: a time map whose parts do not rise from 0 to 1 (AN-1.5)")
            }
            AnimError::Unsupported(what) => write!(f, "not supported: {what}"),
            AnimError::NotOpened(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for AnimError {}
