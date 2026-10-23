// SPDX-License-Identifier: MIT
//! Picture files: PNG of 8-bit RGB levels.
//!
//! (output): stored deflate blocks, so no
//! compression library is needed (Freedom: compression and filters are free).

use std::io;
use std::path::Path;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// CRC-32 of ISO/IEC 15948 annex D (polynomial 0xEDB88320, reflected).
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for part in parts {
        for &b in *part {
            c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xFFFF_FFFF
}

/// Adler-32 of RFC 1950.
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// A zlib stream of stored deflate blocks (RFC 1950, RFC 1951 3.2.4).
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    if blocks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        z.push(if last { 1 } else { 0 });
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(raw).to_be_bytes());
    z
}

/// The bytes of a PNG holding exactly `levels` (row by row, R, G, B per pixel), with no
/// colour-profile or gamma chunk.
pub fn encode_rgb8(width: u32, height: u32, levels: &[u8]) -> Vec<u8> {
    assert_eq!(levels.len(), width as usize * height as usize * 3, "levels must hold w·h·3 bytes");
    let mut out = SIGNATURE.to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, colour type RGB, deflate, filter 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    let row = width as usize * 3;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0); // filter type None
        raw.extend_from_slice(&levels[y * row..(y + 1) * row]);
    }
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Write the picture; a file that cannot be written is an error the run reports.
pub fn write_rgb8(path: &Path, width: u32, height: u32, levels: &[u8]) -> io::Result<()> {
    std::fs::write(path, encode_rgb8(width, height, levels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_iend_is_the_published_value() {
        // The IEND chunk's CRC is AE 42 60 82 in every PNG.
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn adler_of_wikipedia() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn small_picture_layout() {
        let png = encode_rgb8(2, 1, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
