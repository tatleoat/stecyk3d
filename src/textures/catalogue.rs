// SPDX-License-Identifier: MIT
//! Which entry each numbered setting falls under, and the points the released entries still
//! leave Open (each refused, never guessed).
//!
//! , (types 0 to 26; the types that
//! read no table stated; type 17's sums Open; the functions of 7 to 16, 18, 20 to 24 Open),
//! (numbers outside the known range: the
//! implementer's choice),,
//! (What an evaluation needs)

use std::fmt;

use crate::version::RenderAs;

/// The last noise type number of Bryce 3D's menu.
pub const LAST_TYPE_THREE: i64 = 26;
/// The last noise type number of Bryce 5's menu.
pub const LAST_TYPE_FIVE: i64 = 49;
/// The last octave method number.
pub const LAST_METHOD: i64 = 12;
/// The method whose direction comes from two samples of type 6 (reads `perm`).
pub const PERM_METHOD: i64 = 11;

/// The entry a noise type number falls under, for a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseFamily {
    /// Types 0 to 26 .
    Three,
    /// Types 27 to 49 as 5.0 .
    Five,
    /// Outside the known range for the version (the implementer's choice).
    Outside,
}

pub fn noise_family(type_number: i64, version: RenderAs) -> NoiseFamily {
    match type_number {
        0..=LAST_TYPE_THREE => NoiseFamily::Three,
        27..=LAST_TYPE_FIVE if version == RenderAs::Bryce50 => NoiseFamily::Five,
        _ => NoiseFamily::Outside,
    }
}

/// One point a released entry leaves Open (or leaves to the implementer, where this program
/// refuses), with the setting that reaches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenPoint {
    /// A type 0 to 26 with an Open part (none since's revision,
    /// and : every type 0 to 26 is computed).
    NoiseType(i64),
    /// : Bryce 5's types 27 to 49 (their values Open; type 49's function not stated).
    NoiseTypeFive(i64),
    /// : a type number outside the version's range (implementer's choice: refused).
    NoiseTypeOutside(i64),
    /// : a dimension setting other than 1, 2 or 3 (implementer's choice: refused).
    Dimension(i64),
    /// : method 11 over a type other than 0 (its direction from samples of type 6, whose
    /// blend is Open), or a number outside 0 to 12 (implementer's choice: refused).
    OctaveMethod(i64),
    /// Limits: an octave count outside 0 to 9 (Open).
    OctaveCount(i64),
    /// : a filter number outside 0 to 13 (implementer's choice: refused).
    Filter(i64),
    /// : a colour mode's function and its rounding to 8 bits.
    ColourMode(i64),
    /// : a blend mode's integer arithmetic.
    Blend(i64),
    /// : a grey or black colour converted to hue, lightness and saturation
    /// bytes (no hue is produced: not determined).
    GreyToHls,
    /// : hue, lightness and saturation bytes the conversion gives no colour for
    /// (a hue of 1 as 3.0 and 3.1).
    HueOne,
    /// : a component's bump gradient where a global phase applies (the
    /// three points it moves beside the point: leaves them Open).
    BumpGradient,
    /// : as 3.0, a colour or value made from the gradient
    /// of a component whose bump a render turned off (it gives none: what the texture kept,
    /// , Open; from a second or third component, not specified).
    BumpOffKept,
    /// : a table the evaluation reads that the substrate lacks (the evaluation is refused).
    NoTable(&'static str),
    /// A stated particular this program does not compute yet: the entry, and what it is.
    NotBuilt(&'static str, &'static str),
}

impl OpenPoint {
    /// The entry that leaves the point Open.
    pub fn entry(self) -> &'static str {
        match self {
            OpenPoint::NoiseType(4 | 5 | 6 | 26) => "TX-2.1",
            OpenPoint::NoiseType(_) => "TX-2.2",
            OpenPoint::NoiseTypeFive(_) => "TX-2.3",
            OpenPoint::NoiseTypeOutside(_) => "TX-2.4",
            OpenPoint::Dimension(_) => "TX-2.1",
            OpenPoint::OctaveMethod(_) | OpenPoint::OctaveCount(_) => "TX-3.1",
            OpenPoint::Filter(_) => "TX-4.1",
            OpenPoint::ColourMode(_) => "TX-6.1",
            OpenPoint::Blend(_) => "TX-6.2",
            OpenPoint::GreyToHls => "TX-6.4a",
            OpenPoint::HueOne => "TX-6.4b",
            OpenPoint::BumpGradient => "TX-6.2",
            OpenPoint::BumpOffKept => "TX-6.2",
            OpenPoint::NoTable(_) => "SUB-8",
            OpenPoint::NotBuilt(entry, _) => entry,
        }
    }
}

impl fmt::Display for OpenPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let e = self.entry();
        match *self {
            OpenPoint::NoiseType(n) | OpenPoint::NoiseTypeFive(n) => write!(f, "{e} noise type {n}"),
            OpenPoint::NoiseTypeOutside(n) => write!(f, "{e} noise type {n} outside the version's range"),
            OpenPoint::Dimension(n) => write!(f, "{e} dimension setting {n}"),
            OpenPoint::OctaveMethod(n) => write!(f, "{e} octave method {n}"),
            OpenPoint::OctaveCount(n) => write!(f, "{e} octave count {n}"),
            OpenPoint::Filter(n) => write!(f, "{e} filter {n}"),
            OpenPoint::ColourMode(n) => write!(f, "{e} colour mode {n}"),
            OpenPoint::Blend(n) => write!(f, "{e} blend mode {n}"),
            OpenPoint::GreyToHls => write!(f, "{e} a grey colour converted to hue, lightness, saturation"),
            OpenPoint::HueOne => write!(f, "{e} a hue byte the conversion gives no colour for"),
            OpenPoint::BumpGradient => write!(f, "{e} bump gradient under a global phase"),
            OpenPoint::BumpOffKept => write!(f, "{e} as 3.0, a colour or value made from a bump the render turned off"),
            OpenPoint::NoTable(t) => write!(f, "{e} the substrate lacks the table {t}"),
            OpenPoint::NotBuilt(_, what) => write!(f, "{e} not computed by this program yet: {what}"),
        }
    }
}
