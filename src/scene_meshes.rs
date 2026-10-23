// SPDX-License-Identifier: MIT
//! The mesh part of a mesh resource (class C3dTriMeshType2): the box, the corner points,
//! the normals, the texture coordinates and the triangles, every number as stored except
//! the coordinates of a big-endian resource. The mesh part's bytes come after the first
//! part and the placement part (either version), which `scene` reads.
//!
//! (Meshes)

use std::fmt;

use crate::geom::Vec3;
use crate::scene_records::{ByteOrder, double, word};

/// The mesh part's header: version, four counts, the box, the unused bytes.
pub const HEADER_LEN: usize = 220;
/// The version word of every mesh observed; 1 alone has the shorter triangle and every other
/// value is read as this one.
const SHORT_TRIANGLE_VERSION: u32 = 1;
/// Offsets in the header.
const POINTS_AT: usize = 4;
const NORMALS_AT: usize = 8;
const TRIANGLES_AT: usize = 12;
const COORDS_AT: usize = 16;
const BOX_AT: usize = 20;
/// Per point and per normal (three doubles), per coordinate (two singles), per triangle.
const POINT_LEN: usize = 24;
const COORD_LEN: usize = 8;
const TRIANGLE_LEN: usize = 76;
/// A triangle of a part of version 1: without its three coordinate numbers, whose offsets are
/// the plane normal's at 24, the corner flags' at 48 and the flag word's at 60.
const TRIANGLE_LEN_V1: usize = 64;
const TRI_PLANE_AT_V1: usize = 24;
const TRI_CORNERS_AT_V1: usize = 48;
const TRI_FLAG_WORD_AT_V1: usize = 60;
/// Offsets in a triangle.
const TRI_NORMALS_AT: usize = 12;
const TRI_COORDS_AT: usize = 24;
const TRI_PLANE_AT: usize = 36;
const TRI_CORNERS_AT: usize = 60;
const TRI_FLAG_WORD_AT: usize = 72;

/// Why a mesh resource is refused. Every case but `Short` is one leaves Open (it
/// allows a reader to refuse each, saying why).,
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshRefusal {
    /// The mesh part needs `needed` bytes (header and the arrays its counts give, as far as
    /// read), more than the `held` bytes after the placement.,
    Short { needed: u64, held: usize },
    /// A triangle of a part of version 1 whose corners use texture coordinates, which that
    /// triangle does not hold: no defined result (Errors).
    NoCoordinates { triangle: usize },
    PointNumber { triangle: usize, number: u32, count: usize },
    NormalNumber { triangle: usize, number: u32, count: usize },
    CoordNumber { triangle: usize, number: u32, count: usize },
}

impl fmt::Display for MeshRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MeshRefusal::*;
        let open = "what Bryce does with it is Open (SCN-17)";
        match self {
            Short { needed, held } => write!(
                f,
                "damaged document: the mesh part's counts need {needed} bytes, but {held} follow the placement"
            ),
            NoCoordinates { triangle } => write!(
                f,
                "unsupported document: triangle {triangle} of a mesh part of version 1 uses texture coordinates, which its 64 bytes do not hold; the original's result is not given by the scene (SCN-17 Errors)"
            ),
            PointNumber { triangle, number, count } => write!(
                f,
                "unsupported document: triangle {triangle} names corner point {number}, but the mesh has {count}; {open}"
            ),
            NormalNumber { triangle, number, count } => write!(
                f,
                "unsupported document: triangle {triangle} uses normal {number}, but the mesh has {count}; {open}"
            ),
            CoordNumber { triangle, number, count } => write!(
                f,
                "unsupported document: triangle {triangle} uses texture coordinate {number}, but the mesh has {count}; {open}"
            ),
        }
    }
}

impl std::error::Error for MeshRefusal {}

/// Which numbers a triangle's corners use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerUse {
    /// Corner flags 0, 0, 0 and flag word 3.
    NormalsAndCoords,
    /// Corner flags 0, 0, 0 or 2, 2, 2 and flag word 1.
    NormalsOnly,
    /// Corner flags 3, 3, 3 and flag word 0.
    Neither,
    /// Any other combination, corners that differ included: which numbers are used is Open
    /// . The reader gives neither number and checks neither; a drawing must not
    /// take this as `Neither` without a released rule. Meshes
    Open,
}

impl CornerUse {
    /// The four combinations states; `Open` for any other.
    pub fn of(corner_words: [u32; 3], flag_word: u32) -> CornerUse {
        let [a, b, c] = corner_words;
        if a != b || b != c {
            return CornerUse::Open;
        }
        match (a, flag_word) {
            (0, 3) => CornerUse::NormalsAndCoords,
            (0 | 2, 1) => CornerUse::NormalsOnly,
            (3, 0) => CornerUse::Neither,
            _ => CornerUse::Open,
        }
    }
}

/// A triangle of a mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTriangle {
    /// Its three corner points (numbers from 0 into `MeshResource::points`).
    pub points: [u32; 3],
    /// The normals at its three corners; `None` where the corners use none (the stored
    /// numbers are not kept, Meshes).
    pub normals: Option<[u32; 3]>,
    /// The texture coordinates at its three corners; `None` where the corners use none.
    pub coords: Option<[u32; 3]>,
    /// The plane normal, as stored.
    pub plane: Vec3,
    /// Which numbers its corners use, from `corner_words` and `flag_word`.
    pub corner_use: CornerUse,
    pub corner_words: [u32; 3],
    pub flag_word: u32,
}

/// A mesh object's mesh part.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshResource {
    /// Index into `Scene::objects`. Meshes
    pub object: usize,
    /// The part's version word (2: another is refused).
    pub version: u32,
    /// The box as stored: the lowest x, y, z, then the highest x, y, z.
    pub bounds: [f64; 6],
    /// In the mesh's own space.
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// u, then v; a big-endian resource's after's rule (`coordinate`).
    pub coords: Vec<[f32; 2]>,
    pub triangles: Vec<MeshTriangle>,
}

/// A texture coordinate's single from its four stored bytes. (the coordinates of
/// a big-endian resource)
pub fn coordinate(stored: [u8; 4], order: ByteOrder) -> f32 {
    match order {
        ByteOrder::Little => f32::from_bits(u32::from_le_bytes(stored)),
        ByteOrder::Big => {
            let [b0, b1, b2, b3] = stored;
            let bump = b3 & 0x7F == 0x7F
                && b2 & 0xC0 == 0x80
                && (b2 & 0x3F != 0 || b1 != 0 || b0 != 0);
            let b2 = if bump { b2 | 0x40 } else { b2 };
            let bits = u32::from_be_bytes([b0, b1, b2, b3]);
            // : a signalling not-a-number single (exponent all ones, bit 22 clear,
            // some lower bit set) is read with bit 22 set
            let signalling = bits & 0x7f80_0000 == 0x7f80_0000 && bits & 0x0040_0000 == 0 && bits & 0x003f_ffff != 0;
            f32::from_bits(if signalling { bits | 0x0040_0000 } else { bits })
        }
    }
}

/// A double of the box or the first two numbers of a plane normal as the processor's loading
/// makes it in a big-endian resource: a signalling not-a-number (exponent all ones, bit 51
/// clear, some lower bit set) becomes the quiet one with bit 51 set; every other pattern as
/// stored. A little-endian resource's numbers are as stored. (the not-a-number
/// patterns)
fn quiet_double(v: f64, order: ByteOrder) -> f64 {
    let bits = v.to_bits();
    let signalling = bits & 0x7ff0_0000_0000_0000 == 0x7ff0_0000_0000_0000
        && bits & 0x0008_0000_0000_0000 == 0
        && bits & 0x0007_ffff_ffff_ffff != 0;
    if order == ByteOrder::Big && signalling { f64::from_bits(bits | 0x0008_0000_0000_0000) } else { v }
}

impl MeshResource {
    /// Reads the mesh part `b` (the bytes after the placement part) of the object `object`,
    /// in the resource's order. Bytes after the arrays are ignored (Meshes).
    pub fn parse(b: &[u8], order: ByteOrder, object: usize) -> Result<MeshResource, MeshRefusal> {
        let short = |needed: u64| MeshRefusal::Short { needed, held: b.len() };
        if b.len() < 4 {
            return Err(short(HEADER_LEN as u64));
        }
        // : the version word is compared with 1 alone; every other value is
        // read as 2
        let version = word(b, 0, order);
        let short_triangle = version == SHORT_TRIANGLE_VERSION;
        let (triangle_len, plane_at, corners_at, flag_at) = if short_triangle {
            (TRIANGLE_LEN_V1, TRI_PLANE_AT_V1, TRI_CORNERS_AT_V1, TRI_FLAG_WORD_AT_V1)
        } else {
            (TRIANGLE_LEN, TRI_PLANE_AT, TRI_CORNERS_AT, TRI_FLAG_WORD_AT)
        };
        if b.len() < HEADER_LEN {
            return Err(short(HEADER_LEN as u64));
        }
        let nv = word(b, POINTS_AT, order);
        let nn = word(b, NORMALS_AT, order);
        let nt = word(b, TRIANGLES_AT, order);
        let nu = word(b, COORDS_AT, order);
        // Every count compared with the data before anything is sized (Errors). The
        // sum of four words times at most 76 fits easily in 64 bits.
        let needed = HEADER_LEN as u64
            + POINT_LEN as u64 * (u64::from(nv) + u64::from(nn))
            + COORD_LEN as u64 * u64::from(nu)
            + triangle_len as u64 * u64::from(nt);
        if needed > b.len() as u64 {
            return Err(short(needed));
        }
        let (nv, nn, nt, nu) = (nv as usize, nn as usize, nt as usize, nu as usize);
        // : in a
        // big-endian resource the box's six numbers and the first two numbers of each plane
        // normal are taken as the processor's own loading of a double makes them
        let loaded = |at: usize| quiet_double(double(b, at, order), order);
        let bounds: [f64; 6] = std::array::from_fn(|k| loaded(BOX_AT + 8 * k));
        let vec3 = |at: usize| -> Vec3 { [double(b, at, order), double(b, at + 8, order), double(b, at + 16, order)] };
        let words3 = |at: usize| -> [u32; 3] { [word(b, at, order), word(b, at + 4, order), word(b, at + 8, order)] };

        let mut at = HEADER_LEN;
        let points: Vec<Vec3> = (0..nv).map(|k| vec3(at + POINT_LEN * k)).collect();
        at += POINT_LEN * nv;
        let normals: Vec<Vec3> = (0..nn).map(|k| vec3(at + POINT_LEN * k)).collect();
        at += POINT_LEN * nn;
        let stored = |at: usize| -> [u8; 4] { [b[at], b[at + 1], b[at + 2], b[at + 3]] };
        let coords: Vec<[f32; 2]> = (0..nu)
            .map(|k| {
                let c = at + COORD_LEN * k;
                [coordinate(stored(c), order), coordinate(stored(c + 4), order)]
            })
            .collect();
        at += COORD_LEN * nu;

        let mut triangles = Vec::with_capacity(nt);
        for t in 0..nt {
            let base = at + triangle_len * t;
            let points3 = words3(base);
            let corner_words = words3(base + corners_at);
            let flag_word = word(b, base + flag_at, order);
            let corner_use = CornerUse::of(corner_words, flag_word);
            // : any combination is read by what each bit means. The
            // flag word's bit 0 set mixes the corners' normals (a corner whose own word has
            // bit 0 set takes the plane normal instead); its bit 1 set takes the corners'
            // coordinates (a corner whose word has bit 1 set takes (0, 0)).
            let (uses_normals, uses_coords) = (flag_word & 1 != 0, flag_word & 2 != 0);
            // The used numbers in range (Open; refused here, Meshes).
            for &number in &points3 {
                if number as usize >= nv {
                    return Err(MeshRefusal::PointNumber { triangle: t, number, count: nv });
                }
            }
            let normals3 = uses_normals.then(|| words3(base + TRI_NORMALS_AT));
            // only the numbers of the corners that read them are checked
            let read_by = |bit: u32, k: usize| corner_words[k] & bit == 0;
            if let Some(ns) = normals3 {
                if let Some(k) = (0..3).find(|&k| read_by(1, k) && ns[k] as usize >= nn) {
                    return Err(MeshRefusal::NormalNumber { triangle: t, number: ns[k], count: nn });
                }
            }
            // a part of version 1 holds no coordinate numbers (Errors)
            if short_triangle && uses_coords && (0..3).any(|k| read_by(2, k)) {
                return Err(MeshRefusal::NoCoordinates { triangle: t });
            }
            let coords3 = (uses_coords && !short_triangle).then(|| words3(base + TRI_COORDS_AT));
            if let Some(ns) = coords3 {
                if let Some(k) = (0..3).find(|&k| read_by(2, k) && ns[k] as usize >= nu) {
                    return Err(MeshRefusal::CoordNumber { triangle: t, number: ns[k], count: nu });
                }
            }
            triangles.push(MeshTriangle {
                points: points3,
                normals: normals3,
                coords: coords3,
                plane: [loaded(base + plane_at), loaded(base + plane_at + 8), double(b, base + plane_at + 16, order)],
                corner_use,
                corner_words,
                flag_word,
            });
        }
        Ok(MeshResource { object, version, bounds, points, normals, coords, triangles })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_rule() {
        // vector: stored 3F 80 80 7F big-endian gives 3F80C07F.
        assert_eq!(coordinate([0x3F, 0x80, 0x80, 0x7F], ByteOrder::Big).to_bits(), 0x3F80_C07F);
        // vector: stored 7F 80 80 3F little-endian gives 3F80807F.
        assert_eq!(coordinate([0x7F, 0x80, 0x80, 0x3F], ByteOrder::Little).to_bits(), 0x3F80_807F);
        // Not changed: bit 6 set, fraction zero, low byte 0x7E, bit 7 clear.
        assert_eq!(coordinate([0x3F, 0x80, 0xC0, 0x7F], ByteOrder::Big).to_bits(), 0x3F80_C07F);
        assert_eq!(coordinate([0x00, 0x00, 0x80, 0xFF], ByteOrder::Big).to_bits(), 0x0000_80FF);
        assert_eq!(coordinate([0x3F, 0x80, 0x80, 0x7E], ByteOrder::Big).to_bits(), 0x3F80_807E);
        assert_eq!(coordinate([0x3F, 0x80, 0x00, 0x7F], ByteOrder::Big).to_bits(), 0x3F80_007F);
        // Changed with the low byte 0xFF and with only b2's low bits set.
        assert_eq!(coordinate([0x00, 0x00, 0x81, 0xFF], ByteOrder::Big).to_bits(), 0x0000_C1FF);
    }

    /// A mesh part of the given version: one point, one normal, `nt` triangles of `tri` bytes.
    fn part(version: u32, triangle: &[u8], nt: u32) -> Vec<u8> {
        let mut b = vec![0u8; HEADER_LEN];
        b[0..4].copy_from_slice(&version.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[12..16].copy_from_slice(&nt.to_le_bytes());
        b.extend_from_slice(&[0u8; 48]); // the point and the normal
        for _ in 0..nt {
            b.extend_from_slice(triangle);
        }
        b
    }

    /// : version 1 has triangles of 64 bytes; coordinates are none.
    #[test]
    fn version_one_triangles() {
        let o = ByteOrder::Little;
        let mut t = vec![0u8; 64];
        t[24..32].copy_from_slice(&1.0f64.to_le_bytes()); // plane normal x at 24
        t[48..52].copy_from_slice(&3u32.to_le_bytes());
        t[52..56].copy_from_slice(&3u32.to_le_bytes());
        t[56..60].copy_from_slice(&3u32.to_le_bytes());
        // corner flags 3, 3, 3 and flag word 0: neither normals nor coordinates
        let m = MeshResource::parse(&part(1, &t, 2), o, 0).unwrap();
        assert_eq!(m.triangles.len(), 2);
        assert_eq!(m.triangles[1].plane, [1.0, 0.0, 0.0]);
        assert_eq!(m.triangles[1].corner_use, CornerUse::Neither);
        // flag word 3 with corner flags 0 would use coordinates the 64 bytes do not hold
        let mut c = t.clone();
        c[48..60].fill(0);
        c[60..64].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(MeshResource::parse(&part(1, &c, 1), o, 0).unwrap_err(), MeshRefusal::NoCoordinates { triangle: 0 });
        // any other version is read with the 76-byte triangle
        let mut t76 = vec![0u8; 76];
        for k in 0..3 {
            t76[60 + 4 * k..64 + 4 * k].copy_from_slice(&3u32.to_le_bytes());
        }
        let m = MeshResource::parse(&part(5, &t76, 1), o, 0).unwrap();
        assert_eq!(m.triangles[0].corner_use, CornerUse::Neither);
    }

    /// : in a big-endian resource a signalling
    /// not-a-number of the box or of a plane normal's first two numbers, and a coordinate, is
    /// read quiet; the points, the normals and the third plane number as stored.
    #[test]
    fn signalling_not_a_numbers() {
        let snan = 0x7ff0_0000_0000_0001u64;
        assert_eq!(quiet_double(f64::from_bits(snan), ByteOrder::Big).to_bits(), snan | (1 << 51));
        assert_eq!(quiet_double(f64::from_bits(snan), ByteOrder::Little).to_bits(), snan);
        let qnan = 0x7ff8_0000_0000_0001u64;
        assert_eq!(quiet_double(f64::from_bits(qnan), ByteOrder::Big).to_bits(), qnan);
        let inf = 0x7ff0_0000_0000_0000u64;
        assert_eq!(quiet_double(f64::from_bits(inf), ByteOrder::Big).to_bits(), inf);
        // a coordinate: 0x7f800001 (signalling) gets bit 22; little-endian as stored
        assert_eq!(coordinate(0x7f80_0001u32.to_be_bytes(), ByteOrder::Big).to_bits(), 0x7fc0_0001);
        assert_eq!(coordinate(0x7f80_0001u32.to_le_bytes(), ByteOrder::Little).to_bits(), 0x7f80_0001);
        assert_eq!(coordinate(0x7f80_0000u32.to_be_bytes(), ByteOrder::Big).to_bits(), 0x7f80_0000);
    }

    #[test]
    fn corner_use() {
        assert_eq!(CornerUse::of([0, 0, 0], 3), CornerUse::NormalsAndCoords);
        assert_eq!(CornerUse::of([0, 0, 0], 1), CornerUse::NormalsOnly);
        assert_eq!(CornerUse::of([2, 2, 2], 1), CornerUse::NormalsOnly);
        assert_eq!(CornerUse::of([3, 3, 3], 0), CornerUse::Neither);
        assert_eq!(CornerUse::of([2, 2, 2], 3), CornerUse::Open);
        assert_eq!(CornerUse::of([0, 0, 0], 0), CornerUse::Open);
        assert_eq!(CornerUse::of([0, 0, 2], 1), CornerUse::Open);
    }
}
