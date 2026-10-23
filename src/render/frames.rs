// SPDX-License-Identifier: MIT
//! Frames: a document rendered as it stands at a chosen time, the frames of a range one after
//! another, and the files of a series.
//!
//! (the time of a frame; a frame at the time t; which tracks are applied; a range;
//! what carries from one frame to the next, the fallback; the quick preview), (the
//! files of a series: one PNG a frame, named by this project's design), (through
//! `animation::channel_at_asked`),
//! (Frames)

use std::fmt;

use super::lit::{MetMaterials, render_lit_frame};
use super::movie::{MovieRange, RangeRefusal};
use super::{OpenChoices, Picture, RenderCounts, RenderError};
use crate::numerics::CRand;
use crate::scene::Scene;
use crate::version::RenderAs;

/// Why a frame number or a range is refused. (Errors: a frame number that is not
/// a whole number, or below 0; the implementer's choice of the words)
#[derive(Clone, Debug, PartialEq)]
pub enum FrameRefusal {
    /// A frame number that is not a whole number, or below 0.
    Number(f64),
    /// A frame rate that is not above 0 (no time can be made from it). frames
    Rate(f32),
    /// A time that is not a number. frames
    Time(f64),
}

impl fmt::Display for FrameRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameRefusal::Number(k) => write!(f, "frame {k} is not a whole number from 0 (SO-2.8)"),
            FrameRefusal::Rate(r) => write!(f, "the document's frame rate {r} is not above 0 (SO-2.8)"),
            FrameRefusal::Time(t) => write!(f, "a frame asked at the time {t}, which is not a number (SO-2.8)"),
        }
    }
}

impl std::error::Error for FrameRefusal {}

impl From<FrameRefusal> for RenderError {
    fn from(r: FrameRefusal) -> Self {
        RenderError::Refused { entry: "SO-2.8", reason: r.to_string() }
    }
}

/// The time of frame `k` at `rate` frames a second: k / f, the rate a binary32 widened, the
/// quotient in binary64.
pub fn frame_time(k: f64, rate: f32) -> Result<f64, FrameRefusal> {
    if !(k >= 0.0 && k.fract() == 0.0) {
        return Err(FrameRefusal::Number(k));
    }
    if !(rate > 0.0) {
        return Err(FrameRefusal::Rate(rate));
    }
    Ok(k / f64::from(rate))
}

/// The whole animation as a movie of `scene` as `version` makes it: from the time 0 to the
/// dialog's time for the animation's length (offset 60) at the document's rate.
pub fn whole_animation(scene: &Scene, version: RenderAs) -> Result<MovieRange, FrameRefusal> {
    let s = &scene.settings;
    if !(s.frame_rate >= 1.0) || !s.frame_rate.is_finite() {
        // A rate whose whole part is 0 counts no frame within a second (the dialog's R).
        return Err(FrameRefusal::Rate(s.frame_rate));
    }
    MovieRange::whole_animation(s.animation_length, s.frame_rate, version).map_err(|e| match e {
        RangeRefusal::Inputs => FrameRefusal::Time(f64::from(s.animation_length)),
        RangeRefusal::TooMany(t) => FrameRefusal::Time(t),
    })
}

/// The file of frame `k` of a series named `name`: the name, a hyphen, the frame number with
/// leading zeros to at least four digits, and ".png".
pub fn frame_file_name(name: &str, k: u64) -> String {
    format!("{name}-{k:04}.png")
}

/// A run of frames of one document, each rendered from what the frames before it left: the
/// generator goes on from the last draw of the frame before, and the materials met stay met;
/// the quick preview renders once, before the first frame, on the document as it opens at its
/// saved time. The first frame starts as a first render of a program just started (the
/// generator as `choices.generator` gives it, seed 1 when `None`). (What carries
/// from one frame to the next: the fallback; The quick preview)
///
/// What the textures keep  is not carried: this renderer refuses every read of
/// what an earlier evaluation left (`render::textured`), so no picture depends on it.
/// (Frames)
pub struct FrameSeries<'a> {
    scene: &'a Scene,
    version: RenderAs,
    choices: OpenChoices,
    met: MetMaterials,
    generator: Option<CRand>,
    frames: usize,
    /// The levels the picture holds before the next frame: the caller's picture, or white, for
    /// the first; then the frame before, whose pixels outside the region are the same in every
    /// frame., point 6
    before: Option<Vec<u8>>,
    /// The frames are a movie's: as 5.0 the whole picture whatever region the document
    /// holds.
    movie: bool,
}

impl<'a> FrameSeries<'a> {
    /// A series of `scene` as `version`, in a program just started: frames asked one after
    /// another, each over the region the document holds.
    pub fn new(scene: &'a Scene, version: RenderAs, choices: OpenChoices) -> FrameSeries<'a> {
        let met = MetMaterials::none(scene.objects.len());
        let generator = choices.generator;
        let before = choices.earlier_picture.clone();
        FrameSeries { scene, version, choices, met, generator, frames: 0, before, movie: false }
    }

    /// The frames of a movie: as `new`, except that as 5.0 every frame renders the whole
    /// picture (the program sets the region to the whole picture when the movie starts).
    pub fn movie(scene: &'a Scene, version: RenderAs, choices: OpenChoices) -> FrameSeries<'a> {
        FrameSeries { movie: true, ..FrameSeries::new(scene, version, choices) }
    }

    /// The next frame, at the time `t` in seconds (a time below 0 is taken as 0,'s
    /// acceptable choice).
    pub fn frame(&mut self, t: f64) -> Result<(Picture, RenderCounts), RenderError> {
        if t.is_nan() {
            return Err(FrameRefusal::Time(t).into());
        }
        let mut at = self.scene.at_time(crate::animation::frame_time(t));
        if self.movie && self.version == RenderAs::Bryce50 {
            let s = &mut at.settings;
            s.region = crate::scene_records::Region { top: 0, left: 0, bottom: s.picture_height, right: s.picture_width };
        }
        let opened = super::plan::opened_with_haze_five(&at, self.version, self.choices.haze_five)?;
        let preview_doc = if self.frames == 0 && !self.choices.no_preview && !self.choices.go_on {
            Some(super::plan::opened_with_haze_five(self.scene, self.version, self.choices.haze_five)?)
        } else {
            None
        };
        // : the first frame of a series
        // that finds no list made makes it (reseeding the generator); every later frame
        // finds the list made and starts where the frame before left the generator.
        let held = self.choices.star_list_held || self.frames > 0;
        let choices = OpenChoices {
            generator: self.generator,
            earlier_picture: self.before.take(),
            star_list_held: held,
            ..self.choices.clone()
        };
        let (picture, counts, generator) =
            render_lit_frame(&opened, preview_doc.as_deref().map(|p| (p, self.scene)), self.version, &choices, &self.met)?;
        self.generator = Some(generator);
        self.before = Some(picture.levels.clone());
        self.frames += 1;
        Ok((picture, counts))
    }

    /// Frame number `k` at the document's frame rate.
    pub fn frame_number(&mut self, k: u64) -> Result<(Picture, RenderCounts), RenderError> {
        let t = frame_time(k as f64, self.scene.settings.frame_rate)?;
        self.frame(t)
    }
}

/// One frame at the time `t`, as the first render of a program just started (the quick preview
/// before it).
pub fn render_frame_lit(
    scene: &Scene,
    version: RenderAs,
    t: f64,
    choices: &OpenChoices,
) -> Result<(Picture, RenderCounts), RenderError> {
    FrameSeries::new(scene, version, choices.clone()).frame(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Opening, OpeningInputs};

    /// : as 5.0 a frame's tracks at t times the opening
    /// time factor (a factor of 0 gives the time 0); as 3.0 and 3.1 at t.
    #[test]
    fn five_applies_the_time_factor() {
        let doc = OpeningInputs::bryce3d(0.0, 1).with_factor(0.5);
        let time = |v, d: OpeningInputs| Opening::new(v, d.at_time(3.0)).map(|o| o.time).ok();
        assert_eq!(time(RenderAs::Bryce50, doc), Some(1.5));
        assert_eq!(time(RenderAs::Bryce50, doc.with_factor(0.0)), Some(0.0));
        assert_eq!(time(RenderAs::Bryce31, doc), Some(3.0));
        assert_eq!(time(RenderAs::Bryce30, doc), Some(3.0));
    }

    #[test]
    fn times_and_names() {
        assert_eq!(frame_time(0.0, 15.0), Ok(0.0));
        assert_eq!(frame_time(15.0, 15.0), Ok(1.0));
        assert_eq!(frame_time(1.0, 15.0), Ok(1.0 / 15.0));
        assert_eq!(frame_time(1.5, 15.0), Err(FrameRefusal::Number(1.5)));
        assert_eq!(frame_time(-1.0, 15.0), Err(FrameRefusal::Number(-1.0)));
        assert_eq!(frame_file_name("movie", 7), "movie-0007.png");
        assert_eq!(frame_file_name("movie", 12345), "movie-12345.png");
    }
}
