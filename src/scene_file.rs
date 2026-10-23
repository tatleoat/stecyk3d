// SPDX-License-Identifier: MIT
//! The composite file that holds a scene document, and its resource tables.
//!

use std::collections::HashSet;
use std::fmt;

/// The first four bytes of a Bryce 3D document.
pub const MAGIC: &[u8; 4] = b"CCmF";
/// The format name stored at 0x200.
pub const FORMAT_NAME: &[u8] = b"Resource Composite File";

/// Shortest acceptable file: one that holds every header field of the layout, the last
/// being the unused word at 0x268.
const MIN_LEN: usize = 0x26c;
/// Offset and size of the format name field.
const FORMAT_NAME_AT: usize = 0x200;
const FORMAT_NAME_LEN: usize = 32;
/// Two header words that must be zero.
const ZERO_WORDS_AT: [usize; 2] = [0x220, 0x224];
/// Directory record: 4 unused bytes, size, first piece.
const DIRECTORY_RECORD_AT: usize = 0x248;
/// Number of directory entries.
const ENTRY_COUNT_AT: usize = 0x260;
/// Bytes per directory entry.
const DIRECTORY_ENTRY_LEN: usize = 12;
/// Bytes of a piece's header (room, used, next).
const PIECE_HEADER_LEN: usize = 12;

/// The entry that lists the resource classes.
pub const CLASS_LIST_ENTRY: usize = 16;
const CLASS_RECORD_LEN: usize = 32;
const CLASS_TAG_LEN: usize = 16;
const DESCRIPTOR_COUNT_AT: usize = 12;
const RESOURCE_ROW_LEN: usize = 60;
const RESOURCE_NAME_LEN: usize = 48;

/// Which program saved the document; only the identifying text's check differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavour {
    /// Bryce 3D 3.0 and 3.1 (.br3).
    Bryce3d,
    /// Bryce 5 (.br5).
    Bryce5,
}

/// Why a file is not a composite file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContainerError {
    NoIdentifyingText,
    TooShort { len: usize },
    WrongFormatName,
    NonZeroHeaderWord { offset: usize, value: u32 },
    DirectoryDoesNotFit { size: u32, entries: u32, held: usize },
    PiecePastEnd { offset: u32 },
    ChainDoesNotEnd { start: u32 },
    EntrySizeMismatch { entry: usize, size: u32, held: usize },
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ContainerError::*;
        match self {
            NoIdentifyingText => write!(f, "not a scene document: it begins neither with CCmF nor (Bryce 5) with 94 blanks"),
            TooShort { len } => write!(f, "not a scene document: {len} bytes is shorter than its header"),
            WrongFormatName => write!(f, "not a scene document: the format name at 0x200 is not \"Resource Composite File\""),
            NonZeroHeaderWord { offset, value } => {
                write!(f, "damaged document: the header word at {offset:#x} is {value:#x}, not zero")
            }
            DirectoryDoesNotFit { size, entries, held } => write!(
                f,
                "damaged document: the directory of {entries} entries does not fit its size of {size} bytes ({held} bytes held)"
            ),
            PiecePastEnd { offset } => write!(f, "damaged document: the piece at {offset:#x} runs past the end of the file"),
            ChainDoesNotEnd { start } => write!(f, "damaged document: the chain of pieces from {start:#x} does not end"),
            EntrySizeMismatch { entry, size, held } => {
                write!(f, "damaged document: entry {entry} states {size} bytes but its pieces hold {held}")
            }
        }
    }
}

impl std::error::Error for ContainerError {}

/// One directory entry's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub in_use: bool,
    pub bytes: Vec<u8>,
}

/// A parsed composite file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub entries: Vec<Entry>,
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Blanks that begin the header's blank-led form. (Variants: 94 blank bytes,
/// 0x20 [format]),
const SECOND_FORM_SPACES: usize = 94;

/// The identifying text in either form: "CCmF" first, or 94 blanks first, whatever text
/// follows them (revised: a reader does not need the identifying text). A .br3 in the
/// blank-led form is the implementer's choice ; it is rejected here, since no program
/// writes one.
fn has_identifying_text(file: &[u8], flavour: Flavour) -> bool {
    if file.starts_with(MAGIC) {
        return true;
    }
    flavour == Flavour::Bryce5 && file[..SECOND_FORM_SPACES].iter().all(|&c| c == b' ')
}

/// Concatenates the used bytes of the chain of pieces that starts at `first`.
fn read_chain(file: &[u8], first: u32) -> Result<Vec<u8>, ContainerError> {
    let most_pieces = file.len() / PIECE_HEADER_LEN;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut at = first;
    while at != 0 {
        if !seen.insert(at) || seen.len() > most_pieces {
            return Err(ContainerError::ChainDoesNotEnd { start: first });
        }
        let o = at as usize;
        let past = ContainerError::PiecePastEnd { offset: at };
        let body = o.checked_add(PIECE_HEADER_LEN).filter(|&b| b <= file.len()).ok_or(past.clone())?;
        let used = be32(file, o + 4) as usize;
        let end = body.checked_add(used).filter(|&e| e <= file.len()).ok_or(past)?;
        out.extend_from_slice(&file[body..end]);
        at = be32(file, o + 8);
    }
    Ok(out)
}

impl Container {
    /// Reads a composite file, rejecting it for any condition of `Errors`.
    pub fn parse(file: &[u8], flavour: Flavour) -> Result<Self, ContainerError> {
        if file.len() < MIN_LEN {
            return Err(ContainerError::TooShort { len: file.len() });
        }
        if !has_identifying_text(file, flavour) {
            return Err(ContainerError::NoIdentifyingText);
        }
        let name = &file[FORMAT_NAME_AT..FORMAT_NAME_AT + FORMAT_NAME_LEN];
        let (text, pad) = name.split_at(FORMAT_NAME.len());
        if text != FORMAT_NAME || pad.iter().any(|&c| c != 0) {
            return Err(ContainerError::WrongFormatName);
        }
        for offset in ZERO_WORDS_AT {
            let value = be32(file, offset);
            if value != 0 {
                return Err(ContainerError::NonZeroHeaderWord { offset, value });
            }
        }

        let dir_size = be32(file, DIRECTORY_RECORD_AT + 4);
        let dir_first = be32(file, DIRECTORY_RECORD_AT + 8);
        let count = be32(file, ENTRY_COUNT_AT);
        let directory = read_chain(file, dir_first)?;
        let needed = (count as u64) * DIRECTORY_ENTRY_LEN as u64;
        if directory.len() != dir_size as usize || needed > dir_size as u64 {
            return Err(ContainerError::DirectoryDoesNotFit { size: dir_size, entries: count, held: directory.len() });
        }

        let entries = directory
            .chunks_exact(DIRECTORY_ENTRY_LEN)
            .take(count as usize)
            .enumerate()
            .map(|(i, rec)| {
                let in_use = rec[0] != 0;
                if !in_use {
                    return Ok(Entry { in_use, bytes: Vec::new() });
                }
                let size = be32(rec, 4);
                let bytes = read_chain(file, be32(rec, 8))?;
                if bytes.len() != size as usize {
                    return Err(ContainerError::EntrySizeMismatch { entry: i, size, held: bytes.len() });
                }
                Ok(Entry { in_use, bytes })
            })
            .collect::<Result<_, _>>()?;
        Ok(Container { entries })
    }

    /// The content of entry `n`, or `None` past the directory.
    pub fn entry(&self, n: usize) -> Option<&[u8]> {
        self.entries.get(n).map(|e| e.bytes.as_slice())
    }
}

/// Why the resource tables cannot be read. `Errors` is Open; these are provisional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    NoSuchEntry { entry: u32 },
    ShortDescriptor { class: usize },
    ShortTable { class: usize, count: u32, held: usize },
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IndexError::NoSuchEntry { entry } => write!(f, "damaged document: entry {entry} is past the directory"),
            IndexError::ShortDescriptor { class } => write!(f, "damaged document: class {class}'s descriptor is too short"),
            IndexError::ShortTable { class, count, held } => {
                write!(f, "damaged document: class {class}'s table holds {held} bytes, too few for {count} resources")
            }
        }
    }
}

impl std::error::Error for IndexError {}

/// One resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub identity: u32,
    pub entries: [u32; 2],
    /// The name's bytes before its terminating zero.
    pub name: Vec<u8>,
}

/// One class of resources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// The class tag's bytes before its terminating zero (a file tag).
    pub tag: Vec<u8>,
    pub resources: Vec<Resource>,
}

/// The classes listed by entry 16.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceIndex {
    pub classes: Vec<Class>,
}

fn up_to_zero(b: &[u8]) -> Vec<u8> {
    b.iter().take_while(|&&c| c != 0).copied().collect()
}

impl ResourceIndex {
    pub fn read(c: &Container) -> Result<Self, IndexError> {
        let entry = |n: u32| c.entry(n as usize).ok_or(IndexError::NoSuchEntry { entry: n });
        let list = entry(CLASS_LIST_ENTRY as u32)?;
        let mut classes = Vec::new();
        for (i, rec) in list.chunks_exact(CLASS_RECORD_LEN).enumerate() {
            let tag = up_to_zero(&rec[..CLASS_TAG_LEN]);
            if tag.is_empty() {
                continue;
            }
            let table = entry(be32(rec, 16))?;
            let descriptor = entry(be32(rec, 20))?;
            if descriptor.len() < DESCRIPTOR_COUNT_AT + 4 {
                return Err(IndexError::ShortDescriptor { class: i });
            }
            let count = be32(descriptor, DESCRIPTOR_COUNT_AT);
            if (count as u64) * RESOURCE_ROW_LEN as u64 > table.len() as u64 {
                return Err(IndexError::ShortTable { class: i, count, held: table.len() });
            }
            let resources = table
                .chunks_exact(RESOURCE_ROW_LEN)
                .take(count as usize)
                .map(|row| Resource {
                    identity: be32(row, 0),
                    entries: [be32(row, 4), be32(row, 8)],
                    name: up_to_zero(&row[12..12 + RESOURCE_NAME_LEN]),
                })
                .collect();
            classes.push(Class { tag, resources });
        }
        Ok(ResourceIndex { classes })
    }

    /// The class with the given tag, if the document has one.
    pub fn class(&self, tag: &[u8]) -> Option<&Class> {
        self.classes.iter().find(|c| c.tag == tag)
    }
}
