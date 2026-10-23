// SPDX-License-Identifier: MIT
//! The pictures a document holds: a C3dCImage resource's header (its second entry) and its
//! pixels packed as rle2 (its first entry), decoded into a `textures::picture::Picture`.
//! Finding the resource a picture texture names is `scene`'s.
//!
//! (the header, the pixel entry, the stream's runs, the planes and their
//! differences from red; Errors),
//! (Pictures)

use std::fmt;
use std::sync::Arc;

use crate::textures::picture::{Picture, Pixel};

/// The class tag of a picture resource. [format]
pub const PICTURE_RESOURCE_CLASS: &str = "C3dCImage";
/// The two codes that begin a header. [format]
const HEADER_CODES: [&[u8; 4]; 2] = [b"imge", b"iv32"];
/// The code that begins the pixel entry. [format]
const PACKING_RLE2: &[u8; 4] = b"rle2";
/// The code that begins the pixel entry of a Bryce 5 build's moon file. [format]
const PACKING_ARLE: &[u8; 4] = b"arle";
/// The header bytes a reader needs.
pub const HEADER_READ_LEN: usize = 24;
/// The pixel entry's head before the stream.
pub const PIXEL_HEAD_LEN: usize = 64;
/// The header's words.
const AT_BYTES_PER_PIXEL: usize = 8;
const AT_WIDTH: usize = 12;
const AT_BYTES_PER_ROW: usize = 16;
const AT_HEIGHT: usize = 20;
/// Bytes a pixel.
const BYTES_PER_PIXEL: u32 = 4;
/// The most pixels a picture may hold here. Pictures (Choices: 2^28)
pub const MAX_PIXELS: usize = 1 << 28;

/// Why a picture refuses the document.; the implementer's choices are
/// marked. Pictures (Refused)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PictureRefusal {
    /// The header is shorter than 24 bytes.
    HeaderShort(usize),
    /// The header does not begin with imge and iv32.
    HeaderCodes,
    /// The bytes-per-pixel word is not 4.
    BytesPerPixel(u32),
    /// The bytes-per-row word is below four times the width, or (a packed entry) not four times
    /// the width.
    RowBytes { width: u32, row: u32 },
    /// A width of 0 (a height of 0 is read: `Decoded::no_pixels`); the moon's picture of height 0.
    Empty { width: u32, height: u32 },
    /// More pixels than this reader holds (the implementer's choice).
    TooLarge { width: u32, height: u32 },
    /// A pixel entry that does not begin with rle2 (other packings: Open).
    Packing,
    /// A pixel entry whose 64-byte head is cut short (the implementer's choice).
    PixelEntryShort(usize),
    /// The stream ends before the fourth plane is complete.
    StreamEnds { plane: usize },
    /// A run would pass the end of its plane.
    RunPastPlane { plane: usize },
}

/// The planes' names, in the stream's order.
const PLANES: [&str; 4] = ["alpha", "red", "green less red", "blue less red"];

impl fmt::Display for PictureRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PictureRefusal::*;
        match self {
            HeaderShort(n) => write!(f, "damaged document: a picture's header has {n} bytes, fewer than 24"),
            HeaderCodes => write!(f, "unsupported document: a picture's header does not begin with imge and iv32"),
            BytesPerPixel(n) => write!(f, "unsupported document: a picture of {n} bytes a pixel, not 4"),
            RowBytes { width, row } => {
                write!(f, "unsupported document: a picture {width} pixels wide whose rows are {row} bytes, not four times the width")
            }
            Empty { width, height } => write!(f, "unsupported document: a picture of {width} × {height} pixels holds no pixel"),
            TooLarge { width, height } => {
                write!(f, "unsupported document: a picture of {width} × {height} pixels is larger than this reader holds")
            }
            Packing => write!(f, "unsupported document: a picture's pixels are not packed as rle2"),
            PixelEntryShort(n) => write!(f, "damaged document: a picture's pixel entry has {n} bytes, fewer than its 64-byte head"),
            StreamEnds { plane } => {
                write!(f, "damaged document: a picture's stream ends within its {} plane", PLANES[*plane])
            }
            RunPastPlane { plane } => {
                write!(f, "damaged document: a run of a picture's stream passes the end of its {} plane", PLANES[*plane])
            }
        }
    }
}

impl std::error::Error for PictureRefusal {}

/// A picture resource, read.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldPicture {
    /// The resource's identity , which a CPictTexture names.
    pub identity: u32,
    pub name: Vec<u8>,
    pub picture: Arc<Picture>,
    /// The picture as Bryce 3.0 and 3.1 read it where that differs from `picture` (as 5.0 and
    /// rle2 read it): an `arle` entry, whose planes they take as the channels themselves.
    pub as_channels: Option<Arc<Picture>>,
    /// A height of 0: the picture holds no pixel (`picture` is a single transparent pixel
    /// standing in); a render of a texture that names it is refused.
    pub no_pixels: bool,
}

impl HeldPicture {
    /// The picture a version reads. (the code arle differs: 3.0 and 3.1 take the
    /// four planes as the channels, 5.0 reads them as rle2 does)
    pub fn read_by(&self, five: bool) -> &Arc<Picture> {
        match (&self.as_channels, five) {
            (Some(c), false) => c,
            _ => &self.picture,
        }
    }
}

/// A big-endian word.
fn be_word(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Decodes a picture from its header (the resource's second entry) and its pixel entry (the
/// first).
pub fn decode_picture(header: &[u8], packed: &[u8]) -> Result<Picture, PictureRefusal> {
    decode_picture_packed(header, packed, false)
}

/// A document's picture as each version reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    /// As Bryce 5.0 reads it (and as rle2 and an unpacked entry, in every version).
    pub picture: Picture,
    /// As 3.0 and 3.1 where it differs (an arle entry: the planes are the channels themselves).
    pub as_channels: Option<Picture>,
    /// A height of 0: the picture holds no pixel and nothing of the stream is read (`picture` is
    /// then a single transparent pixel standing in; has nothing to read).
    pub no_pixels: bool,
}

/// The header's words (code, bytes per pixel, width, bytes per row, height) checked as the three
/// programs read them: the codes, 4 bytes a pixel, a width other than 0, a bytes-per-row word
/// that is 0 (four times the width) or at least four times the width. Gives (width, height,
/// bytes per row with 0 read as four times the width).
fn header_words(header: &[u8]) -> Result<(u32, u32, u64), PictureRefusal> {
    use PictureRefusal::*;
    if header.len() < HEADER_READ_LEN {
        return Err(HeaderShort(header.len()));
    }
    if &header[0..4] != HEADER_CODES[0] || &header[4..8] != HEADER_CODES[1] {
        return Err(HeaderCodes);
    }
    let bytes_per_pixel = be_word(header, AT_BYTES_PER_PIXEL);
    if bytes_per_pixel != BYTES_PER_PIXEL {
        return Err(BytesPerPixel(bytes_per_pixel));
    }
    let width = be_word(header, AT_WIDTH);
    let height = be_word(header, AT_HEIGHT);
    if width == 0 {
        return Err(Empty { width, height });
    }
    let four = u64::from(width) * u64::from(BYTES_PER_PIXEL);
    let row = match be_word(header, AT_BYTES_PER_ROW) {
        0 => four,
        r => u64::from(r),
    };
    if row < four {
        return Err(RowBytes { width, row: be_word(header, AT_BYTES_PER_ROW) });
    }
    Ok((width, height, row))
}

/// A document's picture as each version reads it: `picture` as Bryce 5.0 (and as rle2 and an
/// unpacked entry, in every version), `as_channels` as 3.0 and 3.1 where it differs (an arle
/// entry: the planes are the channels themselves).
pub fn decode_held(header: &[u8], packed: &[u8]) -> Result<Decoded, PictureRefusal> {
    use PictureRefusal::*;
    let (width, height, row) = header_words(header)?;
    // : a height of 0 is read, the picture holds no pixel, nothing of the
    // stream is read
    if height == 0 {
        let stand_in = Picture::new(1, 1, vec![Pixel([0; 4])]).expect("one pixel");
        return Ok(Decoded { picture: stand_in, as_channels: None, no_pixels: true });
    }
    let count = (width as usize).checked_mul(height as usize).filter(|&n| n <= MAX_PIXELS);
    let Some(count) = count else { return Err(TooLarge { width, height }) };
    // An entry of exactly 4 x width x height bytes is read unpacked, whatever its first word
    // (the bytes-per-row word is then not compared with the width).
    if packed.len() == 4 * count {
        let pixels = packed.chunks_exact(4).map(|w| Pixel([w[0], w[1], w[2], w[3]])).collect();
        let picture = Picture::new(width, height, pixels).expect("size checked");
        return Ok(Decoded { picture, as_channels: None, no_pixels: false });
    }
    // : a packed entry whose bytes-per-row word is not exactly four times
    // the width fails the picture
    if row != u64::from(width) * u64::from(BYTES_PER_PIXEL) {
        return Err(RowBytes { width, row: be_word(header, AT_BYTES_PER_ROW) });
    }
    let main = packed_picture(width, height, count, packed, true)?;
    if packed.len() >= 4 && &packed[..4] == PACKING_ARLE {
        let planes = unpack_planes(packed.get(PIXEL_HEAD_LEN..).unwrap_or(&[]), count)?;
        let pixels = (0..count)
            .map(|k| Pixel([planes[k], planes[count + k], planes[2 * count + k], planes[3 * count + k]]))
            .collect();
        let channels = Picture::new(width, height, pixels).expect("size checked");
        return Ok(Decoded { picture: main, as_channels: Some(channels), no_pixels: false });
    }
    Ok(Decoded { picture: main, as_channels: None, no_pixels: false })
}

/// As `decode_picture`, with `arle` allowed: a pixel entry that begins with the code arle (the
/// Bryce 5 builds' moon file) is unpacked exactly as one that begins with rle2.
pub fn decode_picture_packed(header: &[u8], packed: &[u8], arle: bool) -> Result<Picture, PictureRefusal> {
    use PictureRefusal::*;
    let (width, height, row) = header_words(header)?;
    if row != u64::from(width) * u64::from(BYTES_PER_PIXEL) {
        return Err(RowBytes { width, row: be_word(header, AT_BYTES_PER_ROW) });
    }
    if height == 0 {
        return Err(Empty { width, height });
    }
    let count = (width as usize).checked_mul(height as usize).filter(|&n| n <= MAX_PIXELS);
    let Some(count) = count else { return Err(TooLarge { width, height }) };
    packed_picture(width, height, count, packed, arle)
}

/// The pixels of a packed entry (rle2, or arle where allowed).
fn packed_picture(width: u32, height: u32, count: usize, packed: &[u8], arle: bool) -> Result<Picture, PictureRefusal> {
    use PictureRefusal::*;
    let known = |code: &[u8]| code == PACKING_RLE2 || (arle && code == PACKING_ARLE);
    if packed.len() < PACKING_RLE2.len() || !known(&packed[..4]) {
        return Err(Packing);
    }
    if packed.len() < PIXEL_HEAD_LEN {
        return Err(PixelEntryShort(packed.len()));
    }
    let planes = unpack_planes(&packed[PIXEL_HEAD_LEN..], count)?;

    let (alpha, rest) = planes.split_at(count);
    let (red, rest) = rest.split_at(count);
    let (green, blue) = rest.split_at(count);
    let pixels = (0..count)
        .map(|k| {
            let r = red[k];
            Pixel([alpha[k], r, green[k].wrapping_add(r), blue[k].wrapping_add(r)])
        })
        .collect();
    Ok(Picture::new(width, height, pixels).expect("size checked above"))
}

/// The four planes of `count` bytes each, from the stream; bytes after the fourth plane are
/// not read (Pictures).
fn unpack_planes(stream: &[u8], count: usize) -> Result<Vec<u8>, PictureRefusal> {
    use PictureRefusal::*;
    let mut out = Vec::with_capacity(4 * count);
    let mut at = 0;
    for plane in 0..PLANES.len() {
        let end = (plane + 1) * count;
        while out.len() < end {
            let Some(&c) = stream.get(at) else { return Err(StreamEnds { plane }) };
            at += 1;
            match c {
                1..=127 => {
                    let Some(&v) = stream.get(at) else { return Err(StreamEnds { plane }) };
                    at += 1;
                    let n = usize::from(c);
                    if out.len() + n > end {
                        return Err(RunPastPlane { plane });
                    }
                    out.resize(out.len() + n, v);
                }
                // : a control byte 0 is a run of no bytes that takes two
                // bytes of the stream (itself and the next), 128 a run of no bytes that takes
                // one; both are read on
                0 => {
                    if stream.get(at).is_none() {
                        return Err(StreamEnds { plane });
                    }
                    at += 1;
                }
                128 => {}
                129..=255 => {
                    let n = usize::from(c - 128);
                    if out.len() + n > end {
                        return Err(RunPastPlane { plane });
                    }
                    let Some(bytes) = stream.get(at..at + n) else { return Err(StreamEnds { plane }) };
                    at += n;
                    out.extend_from_slice(bytes);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut h = vec![0u8; 80];
        h[0..4].copy_from_slice(b"imge");
        h[4..8].copy_from_slice(b"iv32");
        h[8..12].copy_from_slice(&4u32.to_be_bytes());
        h[12..16].copy_from_slice(&width.to_be_bytes());
        h[16..20].copy_from_slice(&(4 * width).to_be_bytes());
        h[20..24].copy_from_slice(&height.to_be_bytes());
        h
    }

    fn packed(stream: &[u8]) -> Vec<u8> {
        let mut p = b"rle2".to_vec();
        p.resize(64, 0);
        p.extend_from_slice(stream);
        p
    }

    #[test]
    fn worked_example() {
        let s = [0x82, 0xFF, 0x80, 0x82, 0x10, 0x20, 0x82, 0x20, 0x10, 0x82, 0x40, 0x20];
        let p = decode_picture(&header(2, 1), &packed(&s)).unwrap();
        let words: Vec<u32> = p.pixels().iter().map(|x| x.word()).collect();
        assert_eq!(words, [0xFF103050, 0x80203040]);
    }

    #[test]
    fn runs_and_refusals() {
        // A 3 × 1 picture: runs of 3 in every plane.
        let s = [3, 9, 3, 1, 3, 0xFF, 3, 2];
        let p = decode_picture(&header(3, 1), &packed(&s)).unwrap();
        assert_eq!(p.pixels()[0].word(), 0x09010003);
        // Trailing bytes are ignored.
        let mut long = s.to_vec();
        long.extend_from_slice(&[1, 2, 3]);
        assert!(decode_picture(&header(3, 1), &packed(&long)).is_ok());
        // A run that passes its plane.
        assert_eq!(
            decode_picture(&header(3, 1), &packed(&[4, 9])),
            Err(PictureRefusal::RunPastPlane { plane: 0 })
        );
        assert_eq!(decode_picture(&header(3, 1), &packed(&s[..6])), Err(PictureRefusal::StreamEnds { plane: 3 }));
        // A control byte 0 is a run of no bytes that takes two bytes of the stream (itself and
        // the next), 128 one: both are read on (Errors).
        let mut with_zero = vec![0, 77, 0x80];
        with_zero.extend_from_slice(&s);
        assert_eq!(decode_picture(&header(3, 1), &packed(&with_zero)).unwrap().pixels()[0].word(), 0x09010003);
        assert_eq!(decode_picture(&header(3, 1), &packed(&[0])), Err(PictureRefusal::StreamEnds { plane: 0 }));
        // A height of 0 is read by a document (no pixel); a width of 0 is not.
        let held = decode_held(&header(3, 0), &[]).unwrap();
        assert!(held.no_pixels);
        // The bytes-per-row word: 0 is four times the width; below it fails; above it fails a
        // packed entry only.
        let mut h = header(3, 1);
        h[16..20].copy_from_slice(&0u32.to_be_bytes());
        assert!(decode_picture(&h, &packed(&s)).is_ok());
        h[16..20].copy_from_slice(&11u32.to_be_bytes());
        assert_eq!(decode_picture(&h, &packed(&s)), Err(PictureRefusal::RowBytes { width: 3, row: 11 }));
        h[16..20].copy_from_slice(&15u32.to_be_bytes());
        assert_eq!(decode_picture(&h, &packed(&s)), Err(PictureRefusal::RowBytes { width: 3, row: 15 }));
        assert!(decode_held(&h, &[0u8; 12]).is_ok(), "an unpacked entry takes a wider row word");
        assert_eq!(decode_picture(&header(0, 1), &packed(&s)), Err(PictureRefusal::Empty { width: 0, height: 1 }));
        assert_eq!(decode_picture(&header(3, 1)[..23], &packed(&s)), Err(PictureRefusal::HeaderShort(23)));
        let mut other = packed(&s);
        other[..4].copy_from_slice(b"rle1");
        assert_eq!(decode_picture(&header(3, 1), &other), Err(PictureRefusal::Packing));
        assert_eq!(decode_picture(&header(3, 1), &packed(&[])[..40]), Err(PictureRefusal::PixelEntryShort(40)));
        assert_eq!(
            decode_picture(&header(70000, 70000), &packed(&s)),
            Err(PictureRefusal::TooLarge { width: 70000, height: 70000 })
        );
    }
}
