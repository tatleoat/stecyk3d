// SPDX-License-Identifier: MIT
//! The layouts of the files kept beside the substrate that renders read: the moon's
//! picture, Bryce 5's star catalogue and the Tree Lab's installed leaves; and the
//! star-colours lookup a catalogue star's colour comes from.
//! (The kept files' layouts)

use std::fmt;

use crate::scene_file::{Container, Flavour, ResourceIndex};

// ---------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------

/// Why a kept file does not have the layout its role gives it. The words are free; the
/// condition is the entry's.,
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptLayoutError {
    /// : shorter than the head, or a head size below the head's own fields.
    ShortHead { head_size: Option<u32>, file_len: usize },
    /// : a count would run past the end of the file.
    CountPastEnd { at: u64 },
    /// : a thing of a kind other than 1, 2 or 3.
    UnknownThing { thing: u32, kind: u32 },
    /// : no thing of kind 1.
    NoMesh,
    /// : a face with other than three corners.
    NotATriangle { face: u32, corners: u32 },
    /// : a face names a point, normal or place that the mesh does not hold.
    NumberOutsideMesh { face: u32, what: &'static str, number: u32, held: u32 },
    /// : the file is not a composite file  or its resource tables
    /// cannot be read.
    NotComposite(String),
    /// : no resource of class ImageIV32ARGB with identity 0x44c.
    NoMoonResource,
    /// : the resource does not hold a picture in the layout of.
    NotAPicture(&'static str),
    /// : the picture is not 256 by 256.
    MoonSize { width: u32, height: u32 },
}

impl fmt::Display for KeptLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use KeptLayoutError::*;
        match self {
            ShortHead { head_size: Some(h), file_len } => write!(
                f,
                "the head size is {h}, less than the head's own fields or more than the file's {file_len} bytes"
            ),
            ShortHead { head_size: None, file_len } => {
                write!(f, "the file of {file_len} bytes ends inside the head")
            }
            CountPastEnd { at } => write!(f, "a count at byte {at} runs past the end of the file"),
            UnknownThing { thing, kind } => write!(f, "thing {thing} is of kind {kind}, not 1, 2 or 3"),
            NoMesh => write!(f, "no thing of kind 1 (no mesh)"),
            NotATriangle { face, corners } => write!(f, "face {face} has {corners} corners, not three"),
            NumberOutsideMesh { face, what, number, held } => {
                write!(f, "face {face} names {what} {number}, but the mesh holds {held}")
            }
            NotComposite(why) => write!(f, "not a composite file: {why}"),
            NoMoonResource => write!(f, "no picture resource ImageIV32ARGB of identity 0x44c"),
            NotAPicture(why) => write!(f, "the picture resource is not a picture: {why}"),
            MoonSize { width, height } => write!(f, "the picture is {width} by {height}, not 256 by 256"),
        }
    }
}

impl std::error::Error for KeptLayoutError {}

// ---------------------------------------------------------------------------------------
// The star catalogue
// ---------------------------------------------------------------------------------------

/// Bytes of one catalogue record.
pub const STAR_RECORD_LEN: usize = 52;
const CLASS_AT: usize = 0x01;
const SUBCLASS_AT: usize = 0x02;
const RIGHT_ASCENSION_AT: usize = 0x20;
const DECLINATION_AT: usize = 0x24;
const MAGNITUDE_AT: usize = 0x30;

/// One star of the catalogue; the numbers bit for bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarRecord {
    /// The spectral class letter, one ASCII byte.
    pub class: u8,
    /// Negative where the text has a minus sign (puts it in the last band).
    pub subclass: i32,
    /// The subclass text begins with a space; puts such a star in the first band.
    pub blank: bool,
    pub right_ascension_hours: f32,
    pub declination_degrees: f32,
    pub magnitude: f32,
}

fn le_f32(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]))
}

/// The subclass number: from byte 0x02, white space skipped, an optional sign, then the
/// decimal digits (none: 0); the text after them is not read. More than nine digits are
/// Open : the number wraps.;
fn subclass_number(record: &[u8]) -> i32 {
    let text = &record[SUBCLASS_AT..];
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let negative = i < text.len() && text[i] == b'-';
    if i < text.len() && (text[i] == b'-' || text[i] == b'+') {
        i += 1;
    }
    let n = text[i..].iter().take_while(|b| b.is_ascii_digit()).fold(0i32, |n, &d| n.wrapping_mul(10).wrapping_add(i32::from(d - b'0')));
    if negative { n.wrapping_neg() } else { n }
}

/// Whether the record's subclass text begins with a space ("blank").
pub fn star_record_blank(record: &[u8]) -> bool {
    record.get(SUBCLASS_AT) == Some(&b' ')
}

/// The "blank" of each whole record of a catalogue, in file order.
pub fn star_catalogue_blanks(bytes: &[u8]) -> Vec<bool> {
    bytes.chunks_exact(STAR_RECORD_LEN).map(star_record_blank).collect()
}

/// The catalogue's records in file order; bytes after the last whole record are not read.
pub fn parse_star_catalogue(bytes: &[u8]) -> Vec<StarRecord> {
    bytes
        .chunks_exact(STAR_RECORD_LEN)
        .map(|r| StarRecord {
            class: r[CLASS_AT],
            subclass: subclass_number(r),
            blank: star_record_blank(r),
            right_ascension_hours: le_f32(r, RIGHT_ASCENSION_AT),
            declination_degrees: le_f32(r, DECLINATION_AT),
            magnitude: le_f32(r, MAGNITUDE_AT),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Star colours (star-colours)
// ---------------------------------------------------------------------------------------

/// The colour a catalogue star takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StarColour {
    /// Red, green, blue bytes from the table's cell.
    Table([u8; 3]),
    /// A class letter with no row (other than R): red, green and blue all 255.
    White,
    /// Not produced since gave class R the row of M; kept for callers that match it.
    Open,
}

/// The table's row for a class letter, `None` for a letter with no row.
fn star_row(class: u8) -> Option<usize> {
    match class {
        b'O' => Some(0),
        b'B' => Some(1),
        b'A' => Some(2),
        b'F' => Some(3),
        b'G' => Some(4),
        b'K' => Some(5),
        b'M' | b'N' | b'R' | b'S' => Some(6),
        _ => None,
    }
}

/// The column: the band of the subclass number.
fn star_band(subclass: i32) -> usize {
    if subclass < 0 {
        return 4;
    }
    (subclass / 2).min(4) as usize
}

/// A star's colour from the star-colours table (105 bytes: 7 rows of 5 cells of r, g, b).
pub fn star_colour(table: &[u8; 105], class: u8, subclass: i32, blank: bool) -> StarColour {
    let subclass = if blank { 0 } else { subclass };
    match star_row(class) {
        None => StarColour::White,
        Some(row) => {
            let at = row * 15 + star_band(subclass) * 3;
            StarColour::Table([table[at], table[at + 1], table[at + 2]])
        }
    }
}

// ---------------------------------------------------------------------------------------
// An installed leaf
// ---------------------------------------------------------------------------------------

/// The head's own fields: head size, version, flags, a 200-byte name, a word.
const LEAF_HEAD_FIELDS: u32 = 4 + 4 + 4 + 200 + 4;
/// The bytes of a name that are read; those after them are passed over.
const NAME_READ: i64 = 30;
/// What a name of more than 30 bytes counts as, for the test of a thing's size.
const NAME_COUNTED_LONG: i64 = 35;
/// A skip passes over the file in pieces of this many bytes.
const SKIP_PIECE: u64 = 16_000;
const THING_MESH: u32 = 1;
const THING_BLOCK: u32 = 2;
const THING_CURVE: u32 = 3;
const LEAF_CORNERS: u32 = 3;

/// One triangle of a leaf: for each corner its point, normal and place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafTriangle {
    pub points: [u32; 3],
    pub normals: [u32; 3],
    pub places: [u32; 3],
}

/// The shape of an installed leaf, numbers bit for bit.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafShape {
    pub points: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    /// The places in the leaf's picture.
    pub places: Vec<[f64; 2]>,
    pub triangles: Vec<LeafTriangle>,
}

/// A big-endian cursor that refuses any read past the end as "a count runs past the end".
struct BeCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BeCursor<'a> {
    fn take(&mut self, n: u64) -> Result<&'a [u8], KeptLayoutError> {
        let left = (self.bytes.len() - self.pos) as u64;
        if n > left {
            return Err(KeptLayoutError::CountPastEnd { at: self.pos as u64 });
        }
        let s = &self.bytes[self.pos..self.pos + n as usize];
        self.pos += n as usize;
        Ok(s)
    }

    /// Pass over `n` bytes in pieces of 16,000: a skip of more than 16,000 bytes passes over the
    /// smallest multiple of 16,000 at or above it; one that would run past the end of the file
    /// is refused.
    fn skip(&mut self, n: u64) -> Result<(), KeptLayoutError> {
        let n = if n > SKIP_PIECE { n.div_ceil(SKIP_PIECE) * SKIP_PIECE } else { n };
        self.take(n).map(|_| ())
    }

    /// Check that `count` items of `size` bytes fit before taking any.
    fn fits(&self, count: u32, size: u64) -> Result<(), KeptLayoutError> {
        if u64::from(count) * size > (self.bytes.len() - self.pos) as u64 {
            return Err(KeptLayoutError::CountPastEnd { at: self.pos as u64 });
        }
        Ok(())
    }

    fn word(&mut self) -> Result<u32, KeptLayoutError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Result<f64, KeptLayoutError> {
        let b = self.take(8)?;
        Ok(f64::from_bits(u64::from_be_bytes(b.try_into().expect("8 bytes"))))
    }
}

fn read_mesh(c: &mut BeCursor<'_>) -> Result<LeafShape, KeptLayoutError> {
    c.word()?;
    let p = c.word()?;
    let n = c.word()?;
    let u = c.word()?;
    let faces = c.word()?;
    c.fits(p, 24)?;
    let points = (0..p).map(|_| Ok([c.f64()?, c.f64()?, c.f64()?])).collect::<Result<Vec<_>, _>>()?;
    c.fits(n, 24)?;
    let normals = (0..n).map(|_| Ok([c.f64()?, c.f64()?, c.f64()?])).collect::<Result<Vec<_>, _>>()?;
    c.fits(u, 16)?;
    let places = (0..u).map(|_| Ok([c.f64()?, c.f64()?])).collect::<Result<Vec<_>, _>>()?;
    // A face is at least its word C.
    c.fits(faces, 4)?;
    let mut triangles = Vec::with_capacity(faces as usize);
    for face in 0..faces {
        let corners = c.word()?;
        if corners != LEAF_CORNERS {
            return Err(KeptLayoutError::NotATriangle { face, corners });
        }
        let mut three = |what: &'static str, held: u32| -> Result<[u32; 3], KeptLayoutError> {
            let mut out = [0u32; 3];
            for slot in &mut out {
                let number = c.word()?;
                if number >= held {
                    return Err(KeptLayoutError::NumberOutsideMesh { face, what, number, held });
                }
                *slot = number;
            }
            Ok(out)
        };
        let points = three("point", p)?;
        let normals = three("normal", n)?;
        let places = three("place", u)?;
        triangles.push(LeafTriangle { points, normals, places });
    }
    Ok(LeafShape { points, normals, places, triangles })
}

/// Read an installed leaf file. The leaf is the mesh of the last thing of kind 1; an earlier
/// mesh is read (so its damage refuses the file) and dropped. After a body, if fewer bytes
/// than the thing's size have been read counting from its size word, a name follows (a word n
/// and n bytes), and then any bytes up to the size are skipped; a body may read past the size.
/// ; (An installed leaf)
pub fn parse_leaf(bytes: &[u8]) -> Result<LeafShape, KeptLayoutError> {
    let file_len = bytes.len();
    if file_len < 4 {
        return Err(KeptLayoutError::ShortHead { head_size: None, file_len });
    }
    let head_size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if head_size < LEAF_HEAD_FIELDS || head_size as usize > file_len {
        return Err(KeptLayoutError::ShortHead { head_size: Some(head_size), file_len });
    }
    let mut c = BeCursor { bytes, pos: head_size as usize };
    let count = c.word()?;
    // Every thing is at least its kind and size words.
    c.fits(count, 8)?;
    let mut leaf = None;
    for thing in 0..count {
        let kind = c.word()?;
        let size_at = c.pos;
        let size = c.word()?;
        match kind {
            THING_MESH => {
                leaf = Some(read_mesh(&mut c)?);
            }
            THING_BLOCK => {
                let len = c.word()?;
                c.take(u64::from(len))?;
            }
            THING_CURVE => {
                let k = c.word()?;
                c.fits(k, 24)?;
                c.take(u64::from(k) * 24)?;
            }
            _ => return Err(KeptLayoutError::UnknownThing { thing, kind }),
        }
        let body = (c.pos - size_at) as i64;
        if body < i64::from(size) {
            // a word n, taken as a signed number, then n bytes (none where n is 0 or less): the
            // first 30 are read, the rest passed over; for the test "still short of its size"
            // the name counts as 4 + n bytes where n is 30 or less, and as 35 where it is more
            let n = i64::from(c.word()? as i32);
            if n > 0 {
                c.take(n.min(NAME_READ) as u64)?;
                if n > NAME_READ {
                    c.skip((n - NAME_READ) as u64)?;
                }
            }
            let counted = if n <= NAME_READ { 4 + n } else { NAME_COUNTED_LONG };
            let left = i64::from(size) - body - counted;
            if left > 0 {
                c.skip(left as u64)?;
            }
        }
    }
    leaf.ok_or(KeptLayoutError::NoMesh)
}

// ---------------------------------------------------------------------------------------
// The moon's picture
// ---------------------------------------------------------------------------------------

/// The class tag of the moon picture's resources.
pub const MOON_CLASS: &[u8] = b"ImageIV32ARGB";
/// The identity of the picture a render takes.
pub const MOON_IDENTITY: u32 = 0x44c;
/// The moon picture's side in pixels.
pub const MOON_SIDE: u32 = 256;
const HEADER_CODES: [&[u8; 4]; 2] = [b"imge", b"iv32"];
const HEADER_MIN_LEN: usize = 24;

/// How the moon picture's pixel entry is packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoonPacking {
    /// `rle2`, the Bryce 3D files .
    Rle2,
    /// `arle`, the Bryce 5 files (unpacking: Open).
    Arle,
}

/// The moon's picture as the file holds it: the header and the pixel entry, undecoded
/// (no decoder in this crate yet; arle is Open).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoonPicture {
    pub packing: MoonPacking,
    pub width: u32,
    pub height: u32,
    pub header: Vec<u8>,
    pub pixels: Vec<u8>,
}

impl MoonPicture {
    /// The picture's pixels, unpacked (arle as rle2), as the sky reads them.
    pub fn image(&self) -> Result<crate::sky::moon_image::MoonImage, crate::scene_pictures::PictureRefusal> {
        let p = crate::scene_pictures::decode_picture_packed(&self.header, &self.pixels, true)?;
        Ok(crate::sky::moon_image::MoonImage::from_picture(&p))
    }
}

fn be_word(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Find the moon's picture in the kept file of role 1 and check its header.
pub fn find_moon_picture(bytes: &[u8]) -> Result<MoonPicture, KeptLayoutError> {
    use KeptLayoutError::*;
    // Flavour::Bryce5 accepts both forms of the start (either form).
    let container = Container::parse(bytes, Flavour::Bryce5).map_err(|e| NotComposite(e.to_string()))?;
    let index = ResourceIndex::read(&container).map_err(|e| NotComposite(e.to_string()))?;
    let resource = index
        .class(MOON_CLASS)
        .and_then(|c| c.resources.iter().find(|r| r.identity == MOON_IDENTITY))
        .ok_or(NoMoonResource)?;
    let pixels = container.entry(resource.entries[0] as usize).ok_or(NotAPicture("no pixel entry"))?;
    let header = container.entry(resource.entries[1] as usize).ok_or(NotAPicture("no header entry"))?;
    if header.len() < HEADER_MIN_LEN {
        return Err(NotAPicture("the header is shorter than 24 bytes"));
    }
    if &header[0..4] != HEADER_CODES[0] || &header[4..8] != HEADER_CODES[1] {
        return Err(NotAPicture("the header does not begin with imge iv32"));
    }
    let width = be_word(header, 12);
    let height = be_word(header, 20);
    if be_word(header, 8) != 4 {
        return Err(NotAPicture("bytes per pixel is not 4"));
    }
    if u64::from(be_word(header, 16)) != 4 * u64::from(width) {
        return Err(NotAPicture("bytes per row is not four times the width"));
    }
    let packing = match pixels.get(0..4) {
        Some(b"rle2") => MoonPacking::Rle2,
        Some(b"arle") => MoonPacking::Arle,
        _ => return Err(NotAPicture("the pixel entry begins with neither rle2 nor arle")),
    };
    if width != MOON_SIDE || height != MOON_SIDE {
        return Err(MoonSize { width, height });
    }
    Ok(MoonPicture { packing, width, height, header: header.to_vec(), pixels: pixels.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_colour_cells() {
        let table: [u8; 105] = std::array::from_fn(|i| i as u8);
        assert_eq!(star_colour(&table, b'O', 0, false), StarColour::Table([0, 1, 2]));
        assert_eq!(star_colour(&table, b'O', 1, false), StarColour::Table([0, 1, 2]));
        assert_eq!(star_colour(&table, b'B', 2, false), StarColour::Table([18, 19, 20]));
        assert_eq!(star_colour(&table, b'K', 7, false), StarColour::Table([84, 85, 86]));
        assert_eq!(star_colour(&table, b'M', 9, false), StarColour::Table([102, 103, 104]));
        assert_eq!(star_colour(&table, b'N', 8, false), StarColour::Table([102, 103, 104]));
        assert_eq!(star_colour(&table, b'S', 0, false), StarColour::Table([90, 91, 92]));
        assert_eq!(star_colour(&table, b'Z', 3, false), StarColour::White);
        assert_eq!(star_colour(&table, b'o', 3, false), StarColour::White);
        assert_eq!(star_colour(&table, b'R', 3, false), StarColour::Table([93, 94, 95])); // R: the row of M
        assert_eq!(star_colour(&table, b'G', -4, false), StarColour::Table([72, 73, 74])); // negative: last band
        assert_eq!(star_colour(&table, b'G', 9, true), StarColour::Table([60, 61, 62])); // blank: first band
    }

    #[test]
    fn subclass_digits() {
        let mut r = [0u8; 52];
        r[2..6].copy_from_slice(b"12ab");
        assert_eq!(subclass_number(&r), 12);
        r[2] = b'+';
        assert_eq!(subclass_number(&r), 2);
        r[2..8].copy_from_slice(b" -  7x");
        assert_eq!(subclass_number(&r), 0);
        r[2..8].copy_from_slice(b"  -7x ");
        assert_eq!(subclass_number(&r), -7);
        assert!(star_record_blank(&r));
        r[2..8].copy_from_slice(b"x12345");
        assert_eq!(subclass_number(&r), 0);
        assert!(!star_record_blank(&r));
    }

    #[test]
    fn short_leaf_files() {
        assert!(matches!(parse_leaf(&[0, 0]), Err(KeptLayoutError::ShortHead { head_size: None, .. })));
        let mut b = vec![0u8; 216];
        b[3] = 200;
        assert!(matches!(parse_leaf(&b), Err(KeptLayoutError::ShortHead { head_size: Some(200), .. })));
        b[3] = 216;
        assert!(matches!(parse_leaf(&b), Err(KeptLayoutError::CountPastEnd { .. })));
        b.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(parse_leaf(&b), Err(KeptLayoutError::NoMesh));
    }

    /// A file of a head of 216 bytes, then `things`, each built from words.
    fn leaf_file(things: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 216];
        b[3] = 216;
        b.extend_from_slice(&(things.len() as u32).to_be_bytes());
        for t in things {
            b.extend_from_slice(t);
        }
        b
    }

    fn words(w: &[u32]) -> Vec<u8> {
        w.iter().flat_map(|x| x.to_be_bytes()).collect()
    }

    /// An empty mesh (kind 1) of size 24 (the size word and its five words).
    fn empty_mesh() -> Vec<u8> {
        words(&[1, 24, 0, 0, 0, 0, 0])
    }

    /// : a name counts as 4 + n bytes where n is 30 or
    /// less and 35 where it is more; skips pass over the file in pieces of 16,000 bytes; a name
    /// of n bytes where n is 0 or less has no bytes.
    #[test]
    fn end_of_a_thing() {
        // A block (kind 2) of length 0 has 4 + 4 + 4 = 12 bytes from its size word on (size word,
        // length word): 8 read. Size 12 leaves 4 for a name word n = 0, nothing else.
        let mut t = words(&[2, 12, 0]);
        t.extend_from_slice(&words(&[0]));
        assert!(parse_leaf(&leaf_file(&[t, empty_mesh()])).is_ok());
        // A name of 5 bytes counts 9: size 8 + 4 + 5 = 17 is exactly read, nothing skipped.
        let mut t = words(&[2, 17, 0, 5]);
        t.extend_from_slice(b"abcde");
        assert!(parse_leaf(&leaf_file(&[t, empty_mesh()])).is_ok());
        // A name of 40 bytes counts 35: with size 100 the bytes skipped after it are
        // 100 - 8 - 35 = 57, so the next thing begins 57 + 4 + 40 - (100 - 8) = 9 after the end
        // the size gives, that is right after the 57 bytes.
        let mut t = words(&[2, 100, 0, 40]);
        t.extend_from_slice(&[7u8; 40]);
        t.extend_from_slice(&[0u8; 57]);
        assert!(parse_leaf(&leaf_file(&[t, empty_mesh()])).is_ok());
        // A skip of more than 16,000 bytes is rounded up to 32,000: a file with the 16,001 asked
        // but only that many present is refused.
        let mut t = words(&[2, 16_000 + 8 + 4, 0, 0]);
        t.extend_from_slice(&vec![0u8; 16_000]);
        assert!(parse_leaf(&leaf_file(&[t.clone(), empty_mesh()])).is_ok(), "exactly 16,000 is not rounded");
        let mut t2 = words(&[2, 16_001 + 8 + 4, 0, 0]);
        t2.extend_from_slice(&vec![0u8; 16_001]);
        t2.extend_from_slice(&empty_mesh());
        assert!(matches!(parse_leaf(&leaf_file(&[t2])), Err(KeptLayoutError::CountPastEnd { .. })), "32,000 are passed over");
        // A name word that is negative as a signed number has no bytes (it counts as 4 + n: 3).
        let mut t = words(&[2, 11, 0]);
        t.extend_from_slice(&words(&[u32::MAX]));
        assert!(parse_leaf(&leaf_file(&[t, empty_mesh()])).is_ok());
    }
}
