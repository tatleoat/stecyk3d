// SPDX-License-Identifier: MIT
//! The tree part of a Bryce 5 tree resource (class C3dTree): the stored trees, each with
//! its leaf shapes, the Tree Lab's settings, its pieces of wood, its leaves and its places
//! for leaves, and the tree object's own three words (leaves' material, stored tree).
//! The tree part's bytes come after the first part and Bryce 5's placement part, which
//! `scene` reads.
//!
//! (Trees)

use std::fmt;

use crate::geom::Vec3;
use crate::scene_records::{ByteOrder, Transform, double, single, word};

/// The tree object's own words at the end of every tree part: material code, leaves'
/// material, stored tree.
pub const OWN_WORDS_LEN: usize = 12;
/// The fields of a stored tree after its three counts and before its shapes' count: the
/// fourth count (4), the box (48), the shapes' name (256), three words (12), none used.
const STORED_UNUSED_LEN: usize = 4 + 48 + 256 + 12;
/// The fixed bytes of a stored tree: its four words, the unused fields, and the settings.
const STORED_FIXED_LEN: usize = 12 + STORED_UNUSED_LEN + 4 + SETTINGS_LEN;
/// A leaf shape's four counts, then its three unused words.
const SHAPE_HEAD_LEN: usize = 16;
const SHAPE_UNUSED_LEN: usize = 12;
/// Per point or normal (three doubles), per coordinate (two singles), per triangle (nine
/// words).
const POINT_LEN: usize = 24;
const COORD_LEN: usize = 8;
const TRIANGLE_LEN: usize = 36;
/// The settings.
pub const SETTINGS_LEN: usize = 680;
/// Per piece and per leaf (a number, then twelve doubles), per place (six doubles and a
/// number).
const PIECE_LEN: usize = 100;
const LEAF_LEN: usize = 100;
const PLACE_LEN: usize = 52;
/// A piece's parent number meaning none.
const NO_PARENT: i32 = -1;

/// Why a tree resource is refused. Every case but `Short` is one leaves Open (it
/// allows a reader to refuse each, saying why).,
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeRefusal {
    /// The data ends before byte `needed` of its parts.
    Short { needed: usize },
    NoSuchLeavesMaterial(u32),
    StoredTreeNumber { number: u32, held: usize },
    NoStoredTrees,
    /// A tree other than the first whose tree part is not its 12 own bytes.
    OtherTreePart { len: usize },
    PieceParent { tree: usize, piece: usize, parent: i32 },
    LeafShape { tree: usize, leaf: usize, shape: u32 },
}

impl fmt::Display for TreeRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TreeRefusal::*;
        let open = "what Bryce 5 does with it is Open (SCN-16)";
        match self {
            Short { needed } => write!(f, "damaged document: the tree's data ends before byte {needed} of its parts"),
            NoSuchLeavesMaterial(id) => {
                write!(f, "unsupported document: the leaves' material {id} is not in the document; {open}")
            }
            StoredTreeNumber { number, held } => write!(
                f,
                "unsupported document: the tree shows stored tree {number}, but the document holds {held}; {open}"
            ),
            NoStoredTrees => write!(f, "unsupported document: the first tree holds no stored trees; {open}"),
            OtherTreePart { len } => write!(
                f,
                "unsupported document: a tree after the first has a tree part of {len} bytes, not 12 (it holds stored trees, or is longer than its parts); {open}"
            ),
            PieceParent { tree, piece, parent } => write!(
                f,
                "unsupported document: in stored tree {tree}, piece {piece} grows out of piece {parent}, which is not in its list; {open}"
            ),
            LeafShape { tree, leaf, shape } => write!(
                f,
                "unsupported document: in stored tree {tree}, leaf {leaf} has shape {shape}, which is not in its list; {open}"
            ),
        }
    }
}

impl std::error::Error for TreeRefusal {}

/// A leaf shape, every number as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafShape {
    /// In the shape's own space.
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// Texture coordinates: u, then v.
    pub coords: Vec<[f32; 2]>,
    /// Per triangle: its three points, its three corners' normals, their three
    /// coordinates (numbers from 0 into the lists above, kept as stored).
    pub triangles: Vec<[u32; 9]>,
}

/// The Tree Lab's settings a stored tree was grown from, the fields a reader uses. The
/// names are this crate's; each field names its setting as gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthSettings {
    /// The deepest level of branching. (offset 0)
    pub max_depth: u32,
    /// The most branches of the trunk at one height. (4)
    pub trunk_branches_max: u32,
    /// Branches at one height of a branch. (8)
    pub branch_branches: u32,
    /// The range of the number of heights the trunk branches at: low, high. (12)
    pub segments: [u32; 2],
    /// Leaves at each place. (20)
    pub leaves_per_place: u32,
    /// The angle between a branch and what it grows out of for the thickest branch,
    /// radians. (24)
    pub angle_at_thickest: f64,
    /// The same for the thinnest branch, radians. (32)
    pub angle_at_thinnest: f64,
    /// The trunk's thickness. (40)
    pub trunk_thickness: f64,
    /// The least thickness. (48)
    pub min_thickness: f64,
    /// The range of a branch's share of its parent's radius: low, high. (56)
    pub radius_share: [f64; 2],
    /// How far drawn numbers move turns and places. (72)
    pub randomness: f64,
    /// The power that spreads the branching heights along a branch. (80)
    pub spread_power: f64,
    /// The leaves' size. (88)
    pub leaf_size: f64,
    /// The kind of the leaves' shape (which number is which arrangement: Open). (96)
    pub leaf_arrangement: u32,
    /// The turn from one branching height to the next, radians. (100)
    pub turn_per_height: f64,
    /// Gravity. (108)
    pub gravity: f64,
    /// The seed of the drawn numbers. (116)
    pub seed: u32,
    /// The tree's outline: four points of a curve, each across, up, a third number. (120)
    pub outline: [Vec3; 4],
}

impl GrowthSettings {
    fn read(b: &[u8], o: ByteOrder) -> GrowthSettings {
        let w = |at| word(b, at, o);
        let d = |at| double(b, at, o);
        GrowthSettings {
            max_depth: w(0),
            trunk_branches_max: w(4),
            branch_branches: w(8),
            segments: [w(12), w(16)],
            leaves_per_place: w(20),
            angle_at_thickest: d(24),
            angle_at_thinnest: d(32),
            trunk_thickness: d(40),
            min_thickness: d(48),
            radius_share: [d(56), d(64)],
            randomness: d(72),
            spread_power: d(80),
            leaf_size: d(88),
            leaf_arrangement: w(96),
            turn_per_height: d(100),
            gravity: d(108),
            seed: w(116),
            outline: std::array::from_fn(|k| {
                let at = 120 + 24 * k;
                [d(at), d(at + 8), d(at + 16)]
            }),
        }
    }
}

/// A piece of wood.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WoodPiece {
    /// The number (from 0) of the piece it grows out of; `None` for -1.
    pub parent: Option<u32>,
    /// Carries the piece's own space into the tree's ('s layout).
    pub transform: Transform,
}

/// A leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    /// The number (from 0) of its shape among the stored tree's leaf shapes.
    pub shape: u32,
    /// Carries the shape's space into the tree's.
    pub transform: Transform,
}

/// A place for leaves, in the tree's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafPlace {
    pub direction: Vec3,
    pub point: Vec3,
    /// The number (from 0) of a piece.
    pub piece: u32,
}

/// A tree as it was grown, with the settings it was grown from.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredTree {
    pub shapes: Vec<LeafShape>,
    pub settings: GrowthSettings,
    pub pieces: Vec<WoodPiece>,
    pub leaves: Vec<Leaf>,
    pub places: Vec<LeafPlace>,
}

/// A tree resource's tree part, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct TreePart {
    /// The stored trees, in the order held: `Some` only for the tree that holds them.
    pub stored_trees: Option<Vec<StoredTree>>,
    /// The identity of the leaves' material (class C3dBaseMaterial), `None` where the type code
    /// is 0 (Errors: any other code is taken as `base`).
    pub leaves_material: Option<u32>,
    /// The number of the stored tree this object shows.
    pub stored_tree: u32,
}

/// Reads forward through a resource's data; every read checks the length first.
/// (Trees: counts checked before allocating)
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
    o: ByteOrder,
}

impl<'a> Cursor<'a> {
    fn need(&self, n: usize) -> Result<(), TreeRefusal> {
        let needed = self.at.saturating_add(n);
        if needed > self.b.len() { Err(TreeRefusal::Short { needed }) } else { Ok(()) }
    }

    /// Room for `count` items of `size` bytes each.
    fn need_items(&self, count: u32, size: usize) -> Result<usize, TreeRefusal> {
        let count = count as usize;
        self.need(count.saturating_mul(size))?;
        Ok(count)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], TreeRefusal> {
        self.need(n)?;
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn word(&mut self) -> Result<u32, TreeRefusal> {
        let o = self.o;
        Ok(word(self.take(4)?, 0, o))
    }

    fn vec3(&mut self) -> Result<Vec3, TreeRefusal> {
        let o = self.o;
        let b = self.take(POINT_LEN)?;
        Ok([double(b, 0, o), double(b, 8, o), double(b, 16, o)])
    }

    fn transform(&mut self) -> Result<Transform, TreeRefusal> {
        let o = self.o;
        Ok(Transform::read(self.take(96)?, 0, o))
    }
}

impl TreePart {
    /// Decode the tree part, which begins at byte `at` of the resource's data `data` and must
    /// end where the data does. `holds` says whether this tree holds the stored trees (the
    /// first tree of the object lists; Trees, by order).
    pub fn parse(data: &[u8], at: usize, o: ByteOrder, holds: bool) -> Result<TreePart, TreeRefusal> {
        let len = data.len().saturating_sub(at);
        let mut c = Cursor { b: data, at, o };
        let stored_trees = if holds {
            if len == OWN_WORDS_LEN {
                return Err(TreeRefusal::NoStoredTrees);
            }
            let count = c.word()?;
            if count == 0 {
                return Err(TreeRefusal::NoStoredTrees);
            }
            let count = c.need_items(count, STORED_FIXED_LEN)?;
            let mut trees = Vec::with_capacity(count);
            for k in 0..count {
                trees.push(stored_tree(&mut c, k)?);
            }
            Some(trees)
        } else {
            c.need(OWN_WORDS_LEN)?;
            if len != OWN_WORDS_LEN {
                return Err(TreeRefusal::OtherTreePart { len });
            }
            None
        };
        // : the code is not tested by the reader; 0 gives the leaves no
        // material and any other code is taken as `base`. Bytes after the last field are not read.
        let code = c.word()?;
        let identity = c.word()?;
        let leaves_material = if code == 0 { None } else { Some(identity) };
        let stored_tree = c.word()?;
        Ok(TreePart { stored_trees, leaves_material, stored_tree })
    }
}

/// One stored tree; `k` is its number, for refusals.
fn stored_tree(c: &mut Cursor<'_>, k: usize) -> Result<StoredTree, TreeRefusal> {
    let pieces = c.word()?;
    let leaves = c.word()?;
    let places = c.word()?;
    c.take(STORED_UNUSED_LEN)?;
    let shape_count = c.word()?;
    let shape_count = c.need_items(shape_count, SHAPE_HEAD_LEN + SHAPE_UNUSED_LEN)?;
    let mut shapes = Vec::with_capacity(shape_count);
    for _ in 0..shape_count {
        shapes.push(leaf_shape(c)?);
    }
    let o = c.o;
    let settings = GrowthSettings::read(c.take(SETTINGS_LEN)?, o);

    let n = c.need_items(pieces, PIECE_LEN)?;
    let mut list = Vec::with_capacity(n);
    for piece in 0..n {
        let parent = c.word()? as i32;
        let parent = match parent {
            NO_PARENT => None,
            p if p >= 0 && (p as usize) < n => Some(p as u32),
            p => return Err(TreeRefusal::PieceParent { tree: k, piece, parent: p }),
        };
        list.push(WoodPiece { parent, transform: c.transform()? });
    }
    let pieces = list;

    let n = c.need_items(leaves, LEAF_LEN)?;
    let mut list = Vec::with_capacity(n);
    for leaf in 0..n {
        let shape = c.word()?;
        if shape as usize >= shapes.len() {
            return Err(TreeRefusal::LeafShape { tree: k, leaf, shape });
        }
        list.push(Leaf { shape, transform: c.transform()? });
    }
    let leaves = list;

    let n = c.need_items(places, PLACE_LEN)?;
    let mut list = Vec::with_capacity(n);
    for _ in 0..n {
        let direction = c.vec3()?;
        let point = c.vec3()?;
        // : a place's piece number, in range or not,
        // changes no picture; it is kept as read
        let piece = c.word()?;
        list.push(LeafPlace { direction, point, piece });
    }
    Ok(StoredTree { shapes, settings, pieces, leaves, places: list })
}

/// One leaf shape.
fn leaf_shape(c: &mut Cursor<'_>) -> Result<LeafShape, TreeRefusal> {
    let points = c.word()?;
    let normals = c.word()?;
    let coords = c.word()?;
    let triangles = c.word()?;
    c.take(SHAPE_UNUSED_LEN)?;
    let o = c.o;
    let n = c.need_items(points, POINT_LEN)?;
    let points = (0..n).map(|_| c.vec3()).collect::<Result<_, _>>()?;
    let n = c.need_items(normals, POINT_LEN)?;
    let normals = (0..n).map(|_| c.vec3()).collect::<Result<_, _>>()?;
    let n = c.need_items(coords, COORD_LEN)?;
    let coords = (0..n)
        .map(|_| c.take(COORD_LEN).map(|b| [single(b, 0, o), single(b, 4, o)]))
        .collect::<Result<_, _>>()?;
    let n = c.need_items(triangles, TRIANGLE_LEN)?;
    let triangles = (0..n)
        .map(|_| c.take(TRIANGLE_LEN).map(|b| std::array::from_fn(|j| word(b, 4 * j, o))))
        .collect::<Result<_, _>>()?;
    Ok(LeafShape { points, normals, coords, triangles })
}
