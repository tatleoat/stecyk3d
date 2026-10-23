// SPDX-License-Identifier: MIT
//! The tree's text: the bytes whose SHA-256 a tree case's result line gives.
//!
//! (Kind `tree`: "The tree's text", its grammar)
//! (Architecture: written to any byte sink)

use super::shape::ShapeTriangle;
use super::{GrownTree, Placement};
use crate::sha256::Sha256;

/// Where the text goes: a buffer, or a hash.
pub trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

impl Sink for Sha256 {
    fn put(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// A line under construction, handed to the sink whole.
struct Line(Vec<u8>);

impl Line {
    fn new(word: &str) -> Line {
        let mut v = Vec::with_capacity(256);
        v.extend_from_slice(word.as_bytes());
        Line(v)
    }

    fn hex(&mut self, bits: u64, digits: u32) {
        self.0.push(b' ');
        for i in (0..digits).rev() {
            self.0.push(HEX[((bits >> (4 * i)) & 0xf) as usize]);
        }
    }

    /// A double: 16 lowercase hexadecimal digits of its bits.
    fn double(&mut self, x: f64) {
        self.hex(x.to_bits(), 16);
    }

    /// A single: 8 lowercase hexadecimal digits of its bits.
    fn single(&mut self, x: f32) {
        self.hex(u64::from(x.to_bits()), 8);
    }

    /// A whole number in decimal, no sign, no leading zero.
    fn whole(&mut self, n: u64) {
        self.0.push(b' ');
        self.0.extend_from_slice(n.to_string().as_bytes());
    }

    /// A piece's number or `none`. (from)
    fn from(&mut self, p: Option<u32>) {
        match p {
            Some(n) => self.whole(u64::from(n)),
            None => self.0.extend_from_slice(b" none"),
        }
    }

    fn placement(&mut self, p: &Placement) {
        for v in p {
            for &x in v {
                self.double(x);
            }
        }
    }

    fn end(mut self, sink: &mut impl Sink) {
        self.0.push(b'\n');
        sink.put(&self.0);
    }
}

/// Write the tree's text: head, pieces, places, leaves, and the shape where `shape` is given.
/// (The tree's text)
pub fn write_text(tree: &GrownTree, shape: Option<&[ShapeTriangle]>, sink: &mut impl Sink) {
    let mut head = Line::new("tree");
    head.whole(tree.pieces.len() as u64);
    head.whole(tree.places.len() as u64);
    head.whole(tree.leaves.len() as u64);
    head.end(sink);
    for p in &tree.pieces {
        let mut l = Line::new("piece");
        l.from(p.from);
        l.placement(&p.placement);
        l.end(sink);
    }
    for p in &tree.places {
        let mut l = Line::new("place");
        for &x in p.point.iter().chain(&p.direction) {
            l.double(x);
        }
        l.from(p.piece);
        l.end(sink);
    }
    for leaf in &tree.leaves {
        let mut l = Line::new("leaf");
        l.placement(&leaf.placement);
        l.end(sink);
    }
    if let Some(triangles) = shape {
        let mut l = Line::new("shape");
        l.whole(triangles.len() as u64);
        l.end(sink);
        for t in triangles {
            let mut l = Line::new("triangle");
            for &n in &t.numbers {
                l.whole(u64::from(n));
            }
            for c in &t.corners {
                for &x in c {
                    l.double(x);
                }
            }
            for uv in &t.uvs {
                for &x in uv {
                    l.single(x);
                }
            }
            for &x in &t.plane {
                l.double(x);
            }
            for c in &t.corner_normals {
                for &x in c {
                    l.double(x);
                }
            }
            l.end(sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Leaf, Piece, Place};
    use super::*;

    #[test]
    fn grammar_of_each_line() {
        let a = [1.0, -0.0, 0.5];
        let tree = GrownTree {
            pieces: vec![
                Piece { from: None, placement: [a; 4] },
                Piece { from: Some(0), placement: [[0.0; 3]; 4] },
            ],
            places: vec![Place { point: [2.0, 0.0, 0.0], direction: [0.0, 1.0, 0.0], piece: Some(10) }],
            leaves: vec![Leaf { placement: [[f64::from_bits(0x7ff8_0000_0000_0001); 3]; 4] }],
        };
        let mut out = Vec::new();
        write_text(&tree, None, &mut out);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert!(text.ends_with('\n') && !text.contains("\n\n") && !text.contains('\r'));
        assert_eq!(lines[0], "tree 2 1 1");
        let one = "3ff0000000000000 8000000000000000 3fe0000000000000";
        assert_eq!(lines[1], format!("piece none {one} {one} {one} {one}"));
        assert_eq!(lines[2], format!("piece 0{}", " 0000000000000000".repeat(12)));
        assert_eq!(lines[3], "place 4000000000000000 0000000000000000 0000000000000000 0000000000000000 3ff0000000000000 0000000000000000 10");
        assert_eq!(lines[4], format!("leaf{}", " 7ff8000000000001".repeat(12)));
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn shape_lines() {
        let t = ShapeTriangle {
            numbers: [0, 19, 7],
            corners: [[1.0; 3]; 3],
            uvs: [[0.5f32, -1.0]; 3],
            plane: [0.0, 0.0, -1.0],
            corner_normals: [[0.0; 3]; 3],
        };
        let mut out = Vec::new();
        write_text(&GrownTree::default(), Some(&[t]), &mut out);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines[0], "tree 0 0 0");
        assert_eq!(lines[1], "shape 1");
        let expect = format!(
            "triangle 0 19 7{}{}{}{}",
            " 3ff0000000000000".repeat(9),
            " 3f000000 bf800000".repeat(3),
            " 0000000000000000 0000000000000000 bff0000000000000",
            " 0000000000000000".repeat(9)
        );
        assert_eq!(lines[2], expect);
    }
}
