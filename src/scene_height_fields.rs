// SPDX-License-Identifier: MIT
//! The terrain's own part of a height-field resource (class C3dHeightField) and the heights
//! after it, plain or as run pairs, read into the terrain component's grid . The first
//! part and the placement part before it are `scene`'s.
//!
//! (Height fields)

use std::fmt;

use crate::scene_records::{ByteOrder, double, word};
use crate::terrain::{HeightGrid, MAX_READ_SIDE, MIN_SIDE};
use crate::version::RenderAs;

/// The terrain's own part.
pub const TERRAIN_PART_LEN: usize = 260;

/// Offsets in the own part.
const BOX_Y_AT: usize = 16;
const BOX_Y_HIGH_AT: usize = 40;
const LOWER_BOUND_AT: usize = 80;
const UPPER_BOUND_AT: usize = 88;
const SIZE_AT: usize = 96;
const MIRRORED_AT: usize = 100;
const PACKING_AT: usize = 104;
const PACKED_LEN_AT: usize = 108;
const TRACKS_AT: usize = 112;
const CLOSING_AT: usize = 132;
const SOLID_AT: usize = 136;
const SMOOTH_AT: usize = 140;

/// The packing word of run pairs: the four characters rle_ as a number in the resource's
/// own order. [format]
const PACKING_RUNS: u32 = 0x726c_655f;
/// The packing word of byte planes: the four characters arle. [format]
const PACKING_PLANES: u32 = 0x6172_6c65;
/// The packing word of plain heights.
const PACKING_PLAIN: u32 = 0;
/// One run pair: a count and a height, two 16-bit numbers.
const PAIR_LEN: usize = 4;

/// The bounds as 5.0 holds them: the doubles nearest -0.00001 and 1.00001.
/// (Variants)
const BOUND_LOW_5: f64 = -0.00001;
const BOUND_HIGH_5: f64 = 1.00001;
/// The solid-faces word as 5.0 reads it.
const SOLID_FACES_5: u32 = 0x3f;

/// How the heights are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Packing {
    /// n n 16-bit numbers in the resource's own order.
    Plain,
    /// Run pairs (rle_), big-endian whatever the resource's order.
    Runs,
    /// Byte planes (arle), the same whatever the resource's order.
    Planes,
}

/// Why a terrain is refused. (Errors; the sizes outside 2 to 4,096 are the
/// implementer's choice, Height fields)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerrainRefusal {
    /// The data ends before byte `needed` of the own part.
    Short { needed: usize },
    /// A size outside 2 to 4,096. (Height fields: sizes)
    Size { n: u32 },
    /// A packing word none of 0, rle_ and arle.
    Packing(u32),
    /// Byte planes that end before the four planes hold their bytes, or whose packed length is
    /// less than they need.
    PlanesShort { n: usize, plane: usize },
    /// A record of a byte plane that gives more bytes than the plane still lacks.
    /// (Errors, Byte planes)
    PlanesOver { n: usize, plane: usize },
    /// Plain heights that end before 2 n n bytes.
    PlainShort { n: usize, needed: usize, left: usize },
    /// Run pairs that end before the pair with count 0, `got` heights given.
    /// (Errors)
    RunsEnd { n: usize, got: usize },
    /// Run pairs whose counts pass n n before the pair with count 0. (Errors:
    /// the pairs do not give n n heights before it)
    RunsOver { n: usize },
    /// The pair with count 0 after `got` heights, fewer than n n.
    RunsShort { n: usize, got: usize },
}

impl fmt::Display for TerrainRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TerrainRefusal::*;
        match self {
            Short { needed } => {
                write!(f, "damaged document: the terrain's data ends before byte {needed} of its own part")
            }
            Size { n } => write!(
                f,
                "unsupported document: the terrain's grid is {n} points a side, outside {MIN_SIDE} to {MAX_READ_SIDE} (SCN-18 Limits)"
            ),
            Packing(w) => write!(
                f,
                "damaged document: the terrain's packing word is {w:#010x}, none of 0 (plain), rle_ (runs) and arle (byte planes) (SCN-18 Errors)"
            ),
            PlainShort { n, needed, left } => write!(
                f,
                "damaged document: a terrain of {n} by {n} needs {needed} bytes of plain heights, but its data holds {left} (SCN-18 Errors)"
            ),
            PlanesShort { n, plane } => write!(
                f,
                "damaged document: the terrain's byte planes of {n} by {n} end within plane {plane} of 4, or its packed length is less than they need (SCN-18 Errors)"
            ),
            PlanesOver { n, plane } => write!(
                f,
                "damaged document: a record in plane {plane} of the terrain's byte planes of {n} by {n} gives more bytes than the plane lacks (SCN-18 Errors)"
            ),
            RunsEnd { n, got } => write!(
                f,
                "damaged document: the terrain's run pairs end after {got} of its {} heights, before the pair with count 0 (SCN-18 Errors)",
                n * n
            ),
            RunsOver { n } => write!(
                f,
                "damaged document: the terrain's run pairs give more than its {} heights before the pair with count 0 (SCN-18 Errors)",
                n * n
            ),
            RunsShort { n, got } => write!(
                f,
                "damaged document: the terrain's run pairs close after {got} of its {} heights (SCN-18 Errors)",
                n * n
            ),
        }
    }
}

impl std::error::Error for TerrainRefusal {}

/// The terrain's own part and its heights, decoded; numbers as the file holds them, read
/// by a version through `bounds` and `solid_faces`.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPart {
    /// The n by n heights, row after row (first row at the lowest z, each row along
    /// increasing x).
    pub grid: HeightGrid,
    pub packing: Packing,
    /// The mirrored kind (a Symmetrical Lattice): the word is not 0.
    pub mirrored: bool,
    /// The mirrored word as stored; the original keeps it whole (the mirrored word).
    pub mirrored_word: u32,
    /// Shade with the smoothed normal: the word is 0.
    pub smooth: bool,
    /// The lower and the upper bound as stored.
    pub stored_bounds: [f64; 2],
    /// The own box's two y numbers as stored (low, high); the x and z numbers are not used.
    pub stored_box_y: [f64; 2],
    /// The closing-faces word (bits 0 to 5: the faces' switches).
    pub closing_faces: u32,
    /// The solid-faces word as stored.
    pub stored_solid_faces: u32,
    /// The five track identities, in's order (0: none).
    pub tracks: [u32; 5],
}

impl TerrainPart {
    /// The lower and the upper bound as `version` reads them.
    pub fn bounds(&self, version: RenderAs) -> [f64; 2] {
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => self.stored_bounds,
            // A bound that is not a number is neither below nor above: kept as stored
            // (`clamp` passes it through; Height fields).
            RenderAs::Bryce50 => self.stored_bounds.map(|b| b.clamp(BOUND_LOW_5, BOUND_HIGH_5)),
        }
    }

    /// The solid-faces word as `version` reads it.
    pub fn solid_faces(&self, version: RenderAs) -> u32 {
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => self.stored_solid_faces,
            RenderAs::Bryce50 => SOLID_FACES_5,
        }
    }

    /// Decode the own part, which begins at byte `at` of the resource's data `data`, and the
    /// heights after it. Data after the heights is ignored.
    pub fn parse(data: &[u8], at: usize, o: ByteOrder) -> Result<TerrainPart, TerrainRefusal> {
        let end = at.saturating_add(TERRAIN_PART_LEN);
        if data.len() < end {
            return Err(TerrainRefusal::Short { needed: end });
        }
        let part = &data[at..end];
        let heights = &data[end..];
        let w = |k| word(part, k, o);

        let size = w(SIZE_AT);
        if !(MIN_SIDE..=MAX_READ_SIDE).contains(&(size as usize)) {
            return Err(TerrainRefusal::Size { n: size });
        }
        let n = size as usize;
        let packing = match w(PACKING_AT) {
            PACKING_PLAIN => Packing::Plain,
            PACKING_RUNS => Packing::Runs,
            PACKING_PLANES => Packing::Planes,
            other => return Err(TerrainRefusal::Packing(other)),
        };
        let list = match packing {
            Packing::Plain => plain_heights(heights, n, o)?,
            Packing::Runs => run_heights(heights, n)?,
            Packing::Planes => plane_heights(heights, n, w(PACKED_LEN_AT) as usize)?,
        };
        let grid = HeightGrid::new(n, list).expect("the side and the count were checked above");
        Ok(TerrainPart {
            grid,
            packing,
            mirrored: w(MIRRORED_AT) != 0,
            mirrored_word: w(MIRRORED_AT),
            smooth: w(SMOOTH_AT) == 0,
            stored_bounds: [double(part, LOWER_BOUND_AT, o), double(part, UPPER_BOUND_AT, o)],
            stored_box_y: [double(part, BOX_Y_AT, o), double(part, BOX_Y_HIGH_AT, o)],
            closing_faces: w(CLOSING_AT),
            stored_solid_faces: w(SOLID_AT),
            tracks: std::array::from_fn(|k| w(TRACKS_AT + 4 * k)),
        })
    }
}

/// n n plain heights in the resource's order; the data is compared with 2 n n before the
/// list is made.
fn plain_heights(b: &[u8], n: usize, o: ByteOrder) -> Result<Vec<u16>, TerrainRefusal> {
    let needed = 2 * n * n; // n is at most 4,096 here
    if b.len() < needed {
        return Err(TerrainRefusal::PlainShort { n, needed, left: b.len() });
    }
    Ok(b[..needed]
        .chunks_exact(2)
        .map(|h| match o {
            ByteOrder::Big => u16::from_be_bytes([h[0], h[1]]),
            ByteOrder::Little => u16::from_le_bytes([h[0], h[1]]),
        })
        .collect())
}

/// Run pairs, each number big-endian, up to the pair with count 0, which must come after
/// exactly n n heights. The list grows run by run, never past n n.
fn run_heights(b: &[u8], n: usize) -> Result<Vec<u16>, TerrainRefusal> {
    let total = n * n;
    let mut list: Vec<u16> = Vec::new();
    let mut pairs = b.chunks_exact(PAIR_LEN);
    loop {
        let Some(p) = pairs.next() else {
            return Err(TerrainRefusal::RunsEnd { n, got: list.len() });
        };
        let count = u16::from_be_bytes([p[0], p[1]]) as usize;
        let height = u16::from_be_bytes([p[2], p[3]]);
        if count == 0 {
            break;
        }
        if count > total - list.len() {
            return Err(TerrainRefusal::RunsOver { n });
        }
        list.resize(list.len() + count, height);
    }
    if list.len() != total {
        return Err(TerrainRefusal::RunsShort { n, got: list.len() });
    }
    Ok(list)
}

/// Byte planes (the code arle): the heights two at a time, 2k and 2k + 1, as the 32-bit number
/// h(2k + 1) x 65536 + h(2k); q = n n / 2 of them; four planes one after another, the highest
/// byte of each number first, then the next, the next and the lowest; each plane a list of
/// records: a count byte c; below 128 the next byte taken c times; 128 or more the next c - 128
/// bytes each once; a plane ends when it holds q bytes. `packed` is the packed length word:
/// the planes must fit in it. A last height that is in no pair (n n odd) is 0 (the original
/// gives it no value of the document's: Freedom).
fn plane_heights(b: &[u8], n: usize, packed: usize) -> Result<Vec<u16>, TerrainRefusal> {
    let total = n * n;
    let q = total / 2;
    let b = &b[..packed.min(b.len())];
    let mut planes: [Vec<u8>; 4] = Default::default();
    let mut at = 0;
    for (k, plane) in planes.iter_mut().enumerate() {
        plane.reserve(q.min(b.len().saturating_mul(255)));
        while plane.len() < q {
            let short = || TerrainRefusal::PlanesShort { n, plane: k };
            let c = *b.get(at).ok_or_else(short)? as usize;
            at += 1;
            if c < 128 {
                let v = *b.get(at).ok_or_else(short)?;
                at += 1;
                if c > q - plane.len() {
                    return Err(TerrainRefusal::PlanesOver { n, plane: k });
                }
                plane.resize(plane.len() + c, v);
            } else {
                let len = c - 128;
                if len > q - plane.len() {
                    return Err(TerrainRefusal::PlanesOver { n, plane: k });
                }
                let bytes = b.get(at..at + len).ok_or_else(short)?;
                at += len;
                plane.extend_from_slice(bytes);
            }
        }
    }
    let mut list = vec![0u16; total];
    for j in 0..q {
        list[2 * j + 1] = u16::from_be_bytes([planes[0][j], planes[1][j]]);
        list[2 * j] = u16::from_be_bytes([planes[2][j], planes[3][j]]);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An own part of size `n` with the packing word `packing`, then `heights`.
    fn part(n: u32, packing: u32, o: ByteOrder, heights: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; TERRAIN_PART_LEN];
        let put = |b: &mut Vec<u8>, at: usize, v: u32| {
            let w = match o {
                ByteOrder::Big => v.to_be_bytes(),
                ByteOrder::Little => v.to_le_bytes(),
            };
            b[at..at + 4].copy_from_slice(&w);
        };
        put(&mut b, SIZE_AT, n);
        put(&mut b, PACKING_AT, packing);
        b.extend_from_slice(heights);
        b
    }

    #[test]
    fn runs_are_big_endian_in_either_order() {
        // 2 by 2: a run of 3 of 0x0102, a run of 1 of 7, the closing pair.
        let pairs = [0, 3, 1, 2, 0, 1, 0, 7, 0, 0, 0, 0];
        for o in [ByteOrder::Little, ByteOrder::Big] {
            let t = TerrainPart::parse(&part(2, PACKING_RUNS, o, &pairs), 0, o).unwrap();
            assert_eq!(t.grid.heights(), &[0x0102, 0x0102, 0x0102, 7]);
            assert_eq!(t.packing, Packing::Runs);
        }
    }

    /// A part with byte planes and the packed length word set to the planes' length.
    fn planes_part(n: u32, o: ByteOrder, planes: &[u8]) -> Vec<u8> {
        let mut b = part(n, PACKING_PLANES, o, planes);
        let w = match o {
            ByteOrder::Big => (planes.len() as u32).to_be_bytes(),
            ByteOrder::Little => (planes.len() as u32).to_le_bytes(),
        };
        b[PACKED_LEN_AT..PACKED_LEN_AT + 4].copy_from_slice(&w);
        b
    }

    /// : the highest byte of h(2k + 1) x 65536 + h(2k) first.
    #[test]
    fn byte_planes() {
        for o in [ByteOrder::Little, ByteOrder::Big] {
            // 2 by 2: h = 0x0102, 0x0304, 0x0506, 0x0708; q = 2 numbers, so the planes are
            // [03 07], [04 08], [01 05], [02 06], each as one literal record (128 + 2).
            let planes = [130, 3, 7, 130, 4, 8, 130, 1, 5, 130, 2, 6];
            let t = TerrainPart::parse(&planes_part(2, o, &planes), 0, o).unwrap();
            assert_eq!(t.grid.heights(), &[0x0102, 0x0304, 0x0506, 0x0708]);
            assert_eq!(t.packing, Packing::Planes);
            // Runs: every height 0x0a0a, each plane one record of count 2.
            let planes = [2, 0x0a, 2, 0x0a, 2, 0x0a, 2, 0x0a];
            let t = TerrainPart::parse(&planes_part(2, o, &planes), 0, o).unwrap();
            assert_eq!(t.grid.heights(), &[0x0a0a; 4]);
        }
        // A record that gives more than its plane lacks; planes that end early; a packed
        // length shorter than the planes need.
        let o = ByteOrder::Little;
        let r = |b: Vec<u8>| TerrainPart::parse(&b, 0, o).unwrap_err();
        assert_eq!(r(planes_part(2, o, &[3, 9])), TerrainRefusal::PlanesOver { n: 2, plane: 0 });
        assert_eq!(r(planes_part(2, o, &[129 + 2, 1, 2, 3])), TerrainRefusal::PlanesOver { n: 2, plane: 0 });
        assert_eq!(r(planes_part(2, o, &[2, 1, 2, 1])), TerrainRefusal::PlanesShort { n: 2, plane: 2 });
        let mut b = planes_part(2, o, &[2, 1, 2, 1, 2, 1, 2, 1]);
        b[PACKED_LEN_AT] = 7;
        assert_eq!(r(b), TerrainRefusal::PlanesShort { n: 2, plane: 3 });
    }

    #[test]
    fn refusals() {
        let o = ByteOrder::Little;
        let r = |b: Vec<u8>| TerrainPart::parse(&b, 0, o).unwrap_err();
        assert_eq!(r(part(2, 0, o, &[])[..259].to_vec()), TerrainRefusal::Short { needed: 260 });
        assert_eq!(r(part(1, 0, o, &[0; 2])), TerrainRefusal::Size { n: 1 });
        assert_eq!(r(part(8193, 0, o, &[])), TerrainRefusal::Size { n: 8193 });
        assert_eq!(r(part(u32::MAX, 0, o, &[])), TerrainRefusal::Size { n: u32::MAX });
        assert_eq!(r(part(2, 1, o, &[0; 8])), TerrainRefusal::Packing(1));
        assert_eq!(r(part(2, 0, o, &[0; 7])), TerrainRefusal::PlainShort { n: 2, needed: 8, left: 7 });
        // A huge grid with almost no data: refused before anything is sized from n.
        assert_eq!(
            r(part(4096, 0, o, &[0; 3])),
            TerrainRefusal::PlainShort { n: 4096, needed: 2 * 4096 * 4096, left: 3 }
        );
        assert_eq!(r(part(2, PACKING_RUNS, o, &[0, 3, 0, 1])), TerrainRefusal::RunsEnd { n: 2, got: 3 });
        assert_eq!(r(part(2, PACKING_RUNS, o, &[0, 3, 0, 1, 0, 1])), TerrainRefusal::RunsEnd { n: 2, got: 3 });
        assert_eq!(r(part(2, PACKING_RUNS, o, &[0, 5, 0, 1, 0, 0, 0, 0])), TerrainRefusal::RunsOver { n: 2 });
        assert_eq!(r(part(2, PACKING_RUNS, o, &[0, 3, 0, 1, 0, 0, 0, 0])), TerrainRefusal::RunsShort { n: 2, got: 3 });
        assert_eq!(
            r(part(4096, PACKING_RUNS, o, &[0xff, 0xff, 0, 1])),
            TerrainRefusal::RunsEnd { n: 4096, got: 65535 }
        );
    }

    #[test]
    fn five_point_zero_rules() {
        let o = ByteOrder::Big;
        let mut b = part(2, 0, o, &[0; 8]);
        b[LOWER_BOUND_AT..LOWER_BOUND_AT + 8].copy_from_slice(&(-5.0f64).to_be_bytes());
        b[UPPER_BOUND_AT..UPPER_BOUND_AT + 8].copy_from_slice(&0.5f64.to_be_bytes());
        b[SOLID_AT + 3] = 3;
        let t = TerrainPart::parse(&b, 0, o).unwrap();
        assert_eq!(t.bounds(RenderAs::Bryce30), [-5.0, 0.5]);
        assert_eq!(t.bounds(RenderAs::Bryce50), [-0.00001, 0.5]);
        assert_eq!(t.solid_faces(RenderAs::Bryce31), 3);
        assert_eq!(t.solid_faces(RenderAs::Bryce50), 0x3f);
    }

    #[test]
    fn own_box_y_numbers_are_kept() {
        let o = ByteOrder::Little;
        let mut b = part(2, 0, o, &[0; 8]);
        b[BOX_Y_AT..BOX_Y_AT + 8].copy_from_slice(&(-1234.5f64).to_le_bytes());
        b[BOX_Y_HIGH_AT..BOX_Y_HIGH_AT + 8].copy_from_slice(&4321.25f64.to_le_bytes());
        let t = TerrainPart::parse(&b, 0, o).unwrap();
        assert_eq!(t.stored_box_y, [-1234.5, 4321.25]);
    }
}
