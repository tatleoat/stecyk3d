// SPDX-License-Identifier: MIT
//! The object track list: which object tracks which.
//!
//! , (the codes that name an
//! object),
//! (the object track list)

use std::fmt;

use crate::scene_records::{ByteOrder, FourCc, ObjectKind, word};

/// The class tag of the resource.
pub const TRACK_LIST: &str = "C3dObjTrackList";

/// Bytes before the rows, and per row.
const HEAD: usize = 12;
const ROW: usize = 24;

/// An object as a row names it: its kind code, class code and identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListedObject {
    pub kind: FourCc,
    pub class: FourCc,
    pub identity: u32,
}

impl ListedObject {
    /// The kind's table gives for the two codes, if any.
    pub fn object_kind(&self) -> Option<ObjectKind> {
        ObjectKind::from_codes(self.kind, self.class)
    }
}

impl fmt::Display for ListedObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{} {}", self.kind, self.class, self.identity)
    }
}

/// One row: the tracker (the object that turns) and its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackListRow {
    pub tracker: ListedObject,
    pub target: ListedObject,
}

/// The list as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackList {
    /// The byte order the order mark gives.
    pub order: ByteOrder,
    pub rows: Vec<TrackListRow>,
}

/// Why a track list is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackListRefusal {
    /// The first word is not 1 (or the data cannot hold it).
    FirstWord(Option<u32>),
    /// The data is not 12 + 24 x rows bytes long.
    Length { len: usize, rows: u32 },
}

impl fmt::Display for TrackListRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackListRefusal::FirstWord(Some(w)) => {
                write!(f, "damaged document: the object track list begins with {w}, not 1 (SCN-22)")
            }
            TrackListRefusal::FirstWord(None) => {
                write!(f, "damaged document: the object track list is shorter than its 12-byte head (SCN-22)")
            }
            TrackListRefusal::Length { len, rows } => write!(
                f,
                "damaged document: the object track list is {len} bytes, not 12 + 24 x {rows} (SCN-22)"
            ),
        }
    }
}

impl std::error::Error for TrackListRefusal {}

impl TrackList {
    /// Reads the resource's data.
    pub fn parse(b: &[u8]) -> Result<TrackList, TrackListRefusal> {
        if b.len() < HEAD {
            // Too short for the first word, the mark and the count: refused (the first-word
            // rule cannot be met).
            let first = (b.len() >= 4).then(|| word(b, 0, ByteOrder::Big));
            return Err(TrackListRefusal::FirstWord(first));
        }
        let order = if b[4..8] == [1, 0, 0, 0] { ByteOrder::Little } else { ByteOrder::Big };
        let first = word(b, 0, order);
        if first != 1 {
            return Err(TrackListRefusal::FirstWord(Some(first)));
        }
        let rows = word(b, 8, order);
        let expected = (rows as usize).checked_mul(ROW).and_then(|n| n.checked_add(HEAD));
        if expected != Some(b.len()) {
            return Err(TrackListRefusal::Length { len: b.len(), rows });
        }
        let object = |r: &[u8], at: usize| ListedObject {
            kind: FourCc::from_word(&r[at..at + 4], order),
            class: FourCc::from_word(&r[at + 4..at + 8], order),
            identity: word(r, at + 8, order),
        };
        let rows = b[HEAD..]
            .chunks_exact(ROW)
            .map(|r| TrackListRow { tracker: object(r, 0), target: object(r, 12) })
            .collect();
        Ok(TrackList { order, rows })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    #[test]
    fn rows_and_refusals() {
        let code = |c: &[u8; 4]| u32::from_be_bytes(*c);
        let b = be(&[1, 0, 1, code(b"camr"), code(b"base"), 7, code(b"3dob"), code(b"sphr"), 9]);
        let l = TrackList::parse(&b).unwrap();
        assert_eq!(l.order, ByteOrder::Big);
        assert_eq!(l.rows[0].tracker.object_kind(), Some(ObjectKind::Camera));
        assert_eq!(l.rows[0].target.identity, 9);
        assert_eq!(TrackList::parse(&be(&[2, 0, 0])), Err(TrackListRefusal::FirstWord(Some(2))));
        assert_eq!(TrackList::parse(&be(&[1, 0, 0, 0])), Err(TrackListRefusal::Length { len: 16, rows: 0 }));
        // Little-endian: the mark 01 00 00 00, the numbers in that order.
        let mut b: Vec<u8> = [1u32, 1, 0].iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(TrackList::parse(&b).unwrap().order, ByteOrder::Little);
        b[0] = 0;
        assert!(TrackList::parse(&b).is_err());
    }
}
