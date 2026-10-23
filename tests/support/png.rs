// SPDX-License-Identifier: MIT
// A small reader of 8-bit RGB and RGBA PNG files (ISO/IEC 15948) for the picture tests: the
// chunks, zlib's inflate (RFC 1950, RFC 1951: stored, fixed and dynamic Huffman blocks) and the
// five scanline filters. Standard library only; written from the published formats.
// a test aid to count the pixels in which a picture differs from an expected PNG.
#![allow(dead_code)]

/// A decoded picture: width, height and the levels, rows from the top, R, G, B per pixel.
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub levels: Vec<u8>,
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    bit: u32,
}

impl Bits<'_> {
    fn one(&mut self) -> u32 {
        let b = (self.data[self.at] >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.at += 1;
        }
        u32::from(b)
    }
    fn take(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |v, i| v | (self.one() << i))
    }
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.at += 1;
        }
    }
}

/// A canonical Huffman code: the count of codes of each length and the symbols in code order.
struct Code {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Code {
    fn new(lengths: &[u8]) -> Code {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for l in 1..16 {
            offs[l] = offs[l - 1] + counts[l - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Code { counts, symbols }
    }
    fn decode(&self, b: &mut Bits) -> u16 {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for l in 1..16 {
            code |= b.one() as i32;
            let count = i32::from(self.counts[l]);
            if code - count < first {
                return self.symbols[(index + (code - first)) as usize];
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        panic!("png: a bad Huffman code");
    }
}

const LEN_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn inflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut b = Bits { data, at: 2, bit: 0 }; // the zlib header's two bytes
    loop {
        let last = b.one();
        match b.take(2) {
            0 => {
                b.align();
                let len = u16::from_le_bytes([data[b.at], data[b.at + 1]]) as usize;
                b.at += 4;
                out.extend_from_slice(&data[b.at..b.at + len]);
                b.at += len;
            }
            kind => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [8u8; 288];
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    (Code::new(&l), Code::new(&[5u8; 30]))
                } else {
                    let hlit = b.take(5) as usize + 257;
                    let hdist = b.take(5) as usize + 1;
                    let hclen = b.take(4) as usize + 4;
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDER.iter().take(hclen) {
                        cl[o] = b.take(3) as u8;
                    }
                    let cc = Code::new(&cl);
                    let mut lengths = Vec::with_capacity(hlit + hdist);
                    while lengths.len() < hlit + hdist {
                        match cc.decode(&mut b) {
                            s @ 0..=15 => lengths.push(s as u8),
                            16 => {
                                let prev = *lengths.last().expect("png: a repeat with nothing before");
                                let n = 3 + b.take(2);
                                lengths.extend(std::iter::repeat_n(prev, n as usize));
                            }
                            17 => {
                                let n = 3 + b.take(3);
                                lengths.extend(std::iter::repeat_n(0, n as usize));
                            }
                            _ => {
                                let n = 11 + b.take(7);
                                lengths.extend(std::iter::repeat_n(0, n as usize));
                            }
                        }
                    }
                    (Code::new(&lengths[..hlit]), Code::new(&lengths[hlit..]))
                };
                loop {
                    let s = lit.decode(&mut b) as usize;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let k = s - 257;
                        let len = LEN_BASE[k] as usize + b.take(u32::from(LEN_EXTRA[k])) as usize;
                        let d = dist.decode(&mut b) as usize;
                        let back = DIST_BASE[d] as usize + b.take(u32::from(DIST_EXTRA[d])) as usize;
                        let start = out.len() - back;
                        for j in 0..len {
                            out.push(out[start + j]);
                        }
                    }
                }
            }
        }
        if last == 1 {
            return out;
        }
    }
}

/// Reads an 8-bit RGB or RGBA PNG (no interlace) as RGB levels.
pub fn read(bytes: &[u8]) -> Rgb {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "png: the signature");
    let mut at = 8;
    let (mut width, mut height, mut channels) = (0u32, 0u32, 0usize);
    let mut idat = Vec::new();
    while at + 8 <= bytes.len() {
        let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = &bytes[at + 4..at + 8];
        let body = &bytes[at + 8..at + 8 + n];
        match kind {
            b"IHDR" => {
                width = u32::from_be_bytes(body[0..4].try_into().unwrap());
                height = u32::from_be_bytes(body[4..8].try_into().unwrap());
                assert_eq!(body[8], 8, "png: 8 bits a channel");
                channels = match body[9] {
                    2 => 3,
                    6 => 4,
                    t => panic!("png: colour type {t}"),
                };
                assert_eq!(body[12], 0, "png: no interlace");
            }
            b"IDAT" => idat.extend_from_slice(body),
            _ => {}
        }
        at += 12 + n;
    }
    let raw = inflate(&idat);
    let stride = width as usize * channels;
    let mut prev = vec![0u8; stride];
    let mut levels = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height as usize {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let filter = row[0];
        let mut line = row[1..].to_vec();
        for x in 0..stride {
            let a = if x >= channels { i32::from(line[x - channels]) } else { 0 };
            let b = i32::from(prev[x]);
            let c = if x >= channels { i32::from(prev[x - channels]) } else { 0 };
            let add = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                f => panic!("png: filter {f}"),
            };
            line[x] = (i32::from(line[x]) + add) as u8;
        }
        for px in line.chunks(channels) {
            levels.extend_from_slice(&px[..3]);
        }
        prev = line;
    }
    Rgb { width, height, levels }
}
