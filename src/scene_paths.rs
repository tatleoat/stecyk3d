// SPDX-License-Identifier: MIT
//! The path resource: a path of motion's knots and settings, and the words of the objects
//! (the path itself and those linked to it) that say an object is held, faces along its
//! path, or is kept out of renders.
//!
//! (the path part's layout, the knot's layout, the box, what an object's words
//! say),
//! (Paths)

use std::fmt;

use crate::animation::{Knot, Path};
use crate::geom::Vec3;
use crate::scene_records::{ByteOrder, double, single, word};

/// Where the path part starts in the resource's data: after the first part (264 bytes) and
/// Bryce 3D's placement part (376).
pub const PATH_PART_AT: usize = 640;
/// The path part's head: version, flag word, knot count and the box. (offsets 0
/// to 59)
const HEAD_LEN: usize = 60;
/// One knot's bytes.
pub const KNOT_LEN: usize = 100;
/// The data's length before the knots: 640 + 60.
pub const DATA_LEN_BEFORE_KNOTS: usize = 700;
/// The versions a path part has: 1, and 2 (one more word at offset 60, the knots from 64; 3.1 and 5.0 read both, 3.0
/// reads every part as version 1).
const PATH_VERSION: u32 = 1;
const PATH_VERSION_TWO: u32 = 2;
/// Where the path part lies in a path resource's data in Bryce 5's form of document: after its longer placement part.
pub const PATH_PART_AT_FIVE: usize = 1288;
/// Bit 0 of the path's flag word: the path is closed.
const CLOSED_BIT: u32 = 0x1;
/// Bit 0 of a knot's flag word: the stretch that starts at it is curved. (knot
/// offset 96)
const CURVED_BIT: u32 = 0x1;

/// Bit 0x800 of an object's first flag word: held to its parent path.
pub const HELD_BIT: u32 = 0x800; // [format]
/// Bit 0x400 of an object's first flag word: faces along its path.
pub const FACES_ALONG_BIT: u32 = 0x400; // [format]
/// Bit 0x400000 of the path object's second flag word: kept out of renders.,
pub const KEPT_OUT_BIT: u32 = 0x40_0000; // [format]

/// The path part as read.
#[derive(Clone, Debug, PartialEq)]
pub struct PathPart {
    /// 1.
    pub version: u32,
    /// The whole flag word (bit 0: closed; the other bits not read).
    pub flag_word: u32,
    /// The box the document holds: lowest x, y, z, then highest x, y, z, in the path's own
    /// space.
    pub stored_box: [f64; 6],
    /// The knots and whether the path is closed, as 3.1 and 5.0 read them.
    pub path: Path,
    /// As 3.0 reads a version 2 part: as version 1, its knots 4 bytes out (None where the part is version 1: the same
    /// path).
    pub path_30: Option<Path>,
    /// A version 2 part's extra word (offset 60; 0 in every document): 3.1 and 5.0 draw no picture of one that is not 0,
    /// which the render refuses as them; 3.0 draws it.
    pub extra_word: u32,
}

/// Why a path resource is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathRefusal {
    /// Data shorter than 700 bytes and 100 for each knot the part counts.
    /// (Errors)
    Short { len: usize, needed: usize },
    /// A version other than 1 and 2.
    Version(u32),
    /// A version 2 part whose extra word is not 0: 3.1 and 5.0 draw no picture of it (the implementer's choice, refusing
    /// is acceptable; the render refuses it as them, see `PathPart::extra_word`).
    ExtraWord(u32),
    /// Fewer than two knots: no result is defined; a reader may refuse, saying why.
    FewKnots(u32),
    /// Kept for callers: no longer refused (Bryce 5's form holds the same part at 1,288).
    Bryce5Form,
}

impl fmt::Display for PathRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathRefusal::Short { len, needed } => {
                write!(f, "damaged document: a path's data is {len} bytes, its knots need {needed} (SCN-21)")
            }
            PathRefusal::Version(v) => write!(f, "not supported: a path part of version {v}, not 1 or 2 (SCN-21)"),
            PathRefusal::ExtraWord(w) => {
                write!(f, "not supported: a path part of version 2 whose extra word is {w:#x}, not 0 (SCN-21: 3.1 and 5.0 draw no picture)")
            }
            PathRefusal::FewKnots(n) => {
                write!(f, "not supported: a path of {n} knots (SCN-21: fewer than two gives no defined result)")
            }
            PathRefusal::Bryce5Form => {
                write!(f, "not supported: a path in Bryce 5's form of document (SCN-21: its path part is Open)")
            }
        }
    }
}

impl PathPart {
    /// The path part from a C3dPath resource's whole data (first part, placement part, path
    /// part), in the byte order `order`. The version is checked before the length of the
    /// knots.
    pub fn parse(data: &[u8], order: ByteOrder) -> Result<PathPart, PathRefusal> {
        Self::parse_at(data, order, PATH_PART_AT)
    }

    /// The same with the part lying at `at` (640 in Bryce 3D's form, 1,288 in Bryce 5's).
    pub fn parse_at(data: &[u8], order: ByteOrder, at: usize) -> Result<PathPart, PathRefusal> {
        let head_end = at + HEAD_LEN;
        if data.len() < head_end {
            return Err(PathRefusal::Short { len: data.len(), needed: at + HEAD_LEN });
        }
        let b = &data[at..];
        let version = word(b, 0, order);
        if version != PATH_VERSION && version != PATH_VERSION_TWO {
            return Err(PathRefusal::Version(version));
        }
        // version 2: one more word at 60, the knots from 64
        let head = if version == PATH_VERSION_TWO { HEAD_LEN + 4 } else { HEAD_LEN };
        let flag_word = word(b, 4, order);
        let count = word(b, 8, order);
        let needed = (count as usize).checked_mul(KNOT_LEN).and_then(|k| k.checked_add(at + head));
        match needed {
            Some(needed) if data.len() >= needed => {}
            needed => return Err(PathRefusal::Short { len: data.len(), needed: needed.unwrap_or(usize::MAX) }),
        }
        let extra_word = if version == PATH_VERSION_TWO { word(b, HEAD_LEN, order) } else { 0 };
        // ,: a path of one knot is read (it is never met); a
        // path of no knot stops the original, and is refused here.
        if count < 1 {
            return Err(PathRefusal::FewKnots(count));
        }
        let stored_box = std::array::from_fn(|k| double(b, 12 + 8 * k, order));
        let knots_at = |first: usize| -> Path {
            let knots = (0..count as usize)
                .map(|k| {
                    let at = first + k * KNOT_LEN;
                    let flags = word(b, at + 96, order);
                    // : bit 4 of the flag word, a vector at 72 (the first inner point's
                    // handle); bit 2, a vector at 48 (the second's); 3.1 and 5.0 read them, 3.0 does not (the path is
                    // then built without: `Path::stretch` leaves them out as 3.0)
                    let vector = |off: usize| -> Vec3 { std::array::from_fn(|j| double(b, at + off + 8 * j, order)) };
                    Knot {
                        handle_out: (flags & 4 != 0).then(|| vector(72)),
                        handle_in: (flags & 2 != 0).then(|| vector(48)),
                        place: std::array::from_fn(|j| double(b, at + 8 * j, order)),
                        tension: single(b, at + 24, order),
                        continuity: single(b, at + 28, order),
                        bias: single(b, at + 32, order),
                        roll: single(b, at + 40, order),
                        curved: word(b, at + 96, order) & CURVED_BIT != 0,
                    }
                })
                .collect();
            Path { knots, closed: flag_word & CLOSED_BIT != 0 }
        };
        let path = knots_at(head);
        // 3.0 does not look at the version word: its knots start at 60 whatever the part is (the last knot's flag word
        // then lies 4 bytes before the data's end at most: the data is long enough, it holds 4 bytes more)
        let path_30 = (head != HEAD_LEN).then(|| knots_at(HEAD_LEN));
        Ok(PathPart { version, flag_word, stored_box, path, path_30, extra_word })
    }

    /// The path as `version` reads this part: 3.0 reads every part as version 1.
    pub fn path_as(&self, version: crate::version::RenderAs) -> &Path {
        match (&self.path_30, version) {
            (Some(p), crate::version::RenderAs::Bryce30) => p,
            _ => &self.path,
        }
    }
}

/// A path object of the document: its object and its part.
#[derive(Clone, Debug, PartialEq)]
pub struct PathObject {
    /// Index into `Scene::objects`.
    pub object: usize,
    pub part: PathPart,
}
