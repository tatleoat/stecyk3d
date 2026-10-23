// SPDX-License-Identifier: MIT
//! Rays against a tree object (Bryce 5): the wood as the surface where the sum of its pieces'
//! fields is one half, found by a march and a halving; the leaves as meshes; both found
//! through a grid of 20 x 20 x 20 cells; what a tree remembers for a ray between asks.
//!
//! , (the boxes, the links, the set of
//! a ray),, (the cells
//! and the walk), (the hand-over of materials:
//! `Asked::crossing_taken` and `TreeHit::on`),
//!
//! Every operation is one IEEE binary64 operation in the order written , no fused
//! multiply-add: the order of each sum is part of the result's bits (`Orders`).

use std::collections::HashMap;

use super::structure::{Bounds, meets};
use super::{Part, Placement, Range, Ray, Shape, StoredMesh, row_times};
use super::mesh::{Face, mesh_hit};
use crate::geom::Vec3;
use crate::numerics::rsqrt_as;
use crate::scene_trees::{Leaf, LeafShape, WoodPiece};
use crate::version::RenderAs;

const V: RenderAs = RenderAs::Bryce50;

/// The grid's cells a side.
const CELLS: usize = 20;
/// The box of an item that is not drawn: it overlaps and meets nothing.
const EMPTY: Bounds = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
/// A cell is moved out by this on every side.
const CELL_OUT: f64 = 0.001;
/// The copy of the ray is moved ahead by this much of its direction.
const COPY_SHIFT: f64 = 0.00001;
/// The origin of the march is moved by this much of the direction.
const MARCH_SHIFT: f64 = 0.001;
/// The step is this much of the chord (one eightieth).
const STEP_OF_CHORD: f64 = 1.0 / 80.0;
/// The surface lies where the sum of the fields is one half.
const SURFACE: f64 = 0.5;
/// The halving stops on a point whose sum is above this and not above one half.
const BAND_LOW: f64 = 0.4999;
/// The normal's offsets: the tree's axes times this.
const NORMAL_OFFSET: f64 = 0.0001;
/// The field polynomial 1 - (22u - 17u^2 + 4u^3)/9 arranged as the rows of trees-scenes.jsonl
/// give it: 1 - u(22 + u(-17 + 4u)) times the double nearest 1/9. (w(u); the
/// arrangement: the rows decide),
const NINTH: f64 = 1.0 / 9.0;
/// The widening of a listing box: a hundredth of the extent, or 1e-7 where the extent is
/// within 1e-10 of 0.
const WIDEN: f64 = 0.01;
const FLAT_EXTENT: f64 = 1e-10;
const FLAT_WIDENING: f64 = 1e-7;
/// The most samples a march or halvings a crossing takes before it gives up: a guard of this
/// implementation against a field that is not a number .
const GUARD: usize = 1 << 20;

/// The groupings of the three-term sums that the vectors decide (Accuracy): 0 is
/// (a + b) + c, 1 is a + (b + c), 2 is (a + c) + b. (Orders)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orders {
    /// The sum of the three squares in a piece's field.
    pub field_squares: u8,
    /// The squares of the origin's length, of the direction's length, and the products of
    /// the dot product, in the stretch.
    pub origin_squares: u8,
    pub direction_squares: u8,
    /// The squared length whose root is the direction's length |d| in the stretch (its own
    /// grouping: the rows of the lone piece separate it from the one above).
    pub direction_length: u8,
    pub dot_products: u8,
    /// The squared length of the field's normal.
    pub field_normal: u8,
    /// The squared length in the unit-length step of the normal carried to the world, for a
    /// hit found in the ask and for a hit kept from an earlier ask.
    pub found_world: u8,
    pub kept_world: u8,
    /// The same for a leaf's normal carried into the tree's space.
    pub leaf_normal: u8,
}

impl Orders {
    /// The groupings the rows of trees.jsonl give. (Orders)
    pub const VECTORS: Orders = Orders {
        field_squares: 0,
        origin_squares: 2,
        direction_squares: 2,
        direction_length: 0,
        dot_products: 2,
        field_normal: 0,
        found_world: 2,
        kept_world: 0,
        leaf_normal: 0,
    };
}

fn sum3(g: u8, a: f64, b: f64, c: f64) -> f64 {
    match g {
        0 => (a + b) + c,
        1 => a + (b + c),
        _ => (a + c) + b,
    }
}

fn dot3(g: u8, a: Vec3, b: Vec3) -> f64 {
    sum3(g, a[0] * b[0], a[1] * b[1], a[2] * b[2])
}

/// The field polynomial w(u) = 1 - (22u - 17u^2 + 4u^3)/9, as arranged above.
fn polynomial(u: f64) -> f64 {
    1.0 - (u * (22.0 + u * (-17.0 + 4.0 * u))) * NINTH
}

/// A map (rows and translation) with its inverse, built whether or not it is the identity, so
/// that carrying a point is always the arithmetic of.
/// `None`: the rows are singular. The inverse's translation is's, as 5.0 forms it (the
/// 4 x 4 cofactors, six products of three numbers each). (Limits:
/// a singular map is Open). (the inverse of the trees' maps)
pub fn tree_map(rows: [Vec3; 3], translation: Vec3) -> Option<Placement> {
    Placement::general(rows, translation)
}

/// The inverse of a map as 5.0 carries a point by it: its three rows and its translation.
pub fn map_inverse(m: &Placement) -> ([Vec3; 3], Vec3) {
    (m.inverse, m.inverse_translation[2])
}

/// A point carried by the inverse of `m` as 5.0 carries it: p . M^-1 + t'.
fn into(m: &Placement, p: Vec3) -> Vec3 {
    let t = m.inverse_translation[2];
    let o = row_times(p, &m.inverse);
    [o[0] + t[0], o[1] + t[1], o[2] + t[2]]
}

/// A direction carried by the inverse of `m`.
fn into_dir(m: &Placement, d: Vec3) -> Vec3 {
    row_times(d, &m.inverse)
}

/// A point carried by `m`: p . M + t.
fn out_of(m: &Placement, p: Vec3) -> Vec3 {
    let q = row_times(p, &m.rows);
    [q[0] + m.translation[0], q[1] + m.translation[1], q[2] + m.translation[2]]
}

fn bounds_of(points: impl Iterator<Item = Vec3>) -> Bounds {
    let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
    for w in points {
        for a in 0..3 {
            b.min[a] = b.min[a].min(w[a]);
            b.max[a] = b.max[a].max(w[a]);
        }
    }
    b
}

/// The eight corners of a box.
fn corners(b: &Bounds) -> [Vec3; 8] {
    let pick = |k: usize, a: usize| if k >> a & 1 == 0 { b.min[a] } else { b.max[a] };
    std::array::from_fn(|k| [pick(k, 0), pick(k, 1), pick(k, 2)])
}

/// A box moved out on each axis at each end by a hundredth of its extent (1e-7 where the extent
/// is within 1e-10 of 0).
fn widen(mut b: Bounds) -> Bounds {
    for a in 0..3 {
        let extent = b.max[a] - b.min[a];
        let d = if extent.abs() < FLAT_EXTENT { FLAT_WIDENING } else { extent * WIDEN };
        b.min[a] -= d;
        b.max[a] += d;
    }
    b
}

/// Two boxes overlap when on each axis each one's minimum is below the other's maximum.
fn overlap(a: &Bounds, b: &Bounds) -> bool {
    (0..3).all(|k| a.min[k] < b.max[k] && b.min[k] < a.max[k])
}

/// Why a stored tree cannot be drawn: each a case leaves Open (refusal with a reason
/// is acceptable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeOpen {
    /// A piece that grows out of a piece that comes after it, or does not exist.
    PieceParent { piece: usize, parent: u32 },
    /// A leaf of a shape that does not exist.
    LeafShape { leaf: usize, shape: u32 },
    /// A triangle naming a point, normal or coordinate that does not exist.
    TriangleNumber { shape: usize, triangle: usize, what: &'static str, number: u32 },
    /// A stored tree with no pieces and no leaves.
    Empty,
    /// A leaf shape with no points (its box is undefined).
    NoPoints { shape: usize },
    /// A grid cell that lists 65,536 items or more.
    CellTooFull { items: usize },
}

impl std::fmt::Display for TreeOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TreeOpen::*;
        match self {
            PieceParent { piece, parent } => {
                write!(f, "piece {piece} grows out of piece {parent}, which does not come before it (Open, GEO-22)")
            }
            LeafShape { leaf, shape } => write!(f, "leaf {leaf} has shape {shape}, which does not exist (Open, GEO-22)"),
            TriangleNumber { shape, triangle, what, number } => {
                write!(f, "triangle {triangle} of leaf shape {shape} names {what} {number}, which does not exist (Open, GEO-22.3)")
            }
            Empty => write!(f, "the stored tree has no pieces and no leaves (Open, GEO-22)"),
            NoPoints { shape } => write!(f, "leaf shape {shape} has no points (Open, GEO-22.1)"),
            CellTooFull { items } => {
                write!(f, "a grid cell lists {items} items, 65,536 or more (the original does not open such a tree, GEO-22 Limits)")
            }
        }
    }
}

impl std::error::Error for TreeOpen {}

#[derive(Clone, Debug, PartialEq)]
struct Piece {
    map: Placement,
    /// B_k: the box of the cube about the piece's origin.
    own: Bounds,
    /// The listing box.
    listing: Bounds,
    /// The pieces linked with this one, in order.
    list: Vec<usize>,
    /// False for a piece whose map is singular: not drawn, no box, in no link and no cell.
    drawn: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct LeafItem {
    shape: usize,
    map: Placement,
    listing: Bounds,
    /// False for a leaf whose map is singular: not drawn.
    drawn: bool,
}

/// A stored tree as the geometry holds it: pieces, leaves, their boxes and links, and the
/// grid.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeModel {
    pieces: Vec<Piece>,
    leaves: Vec<LeafItem>,
    shapes: Vec<StoredMesh>,
    /// The tree's bounds, the union of the listing boxes.
    bounds: Bounds,
    /// The grid: the items (pieces, then leaves numbered P + j) of each cell.
    cells: Vec<Vec<u32>>,
    /// The number of the stored tree it shows, where the scene says (trees that show one stored
    /// tree share their items' marks).
    stored: Option<usize>,
}

fn cell_of(c: f64, lowest: f64, highest: f64) -> usize {
    let k = ((c - lowest) / (highest - lowest) * CELLS as f64).trunc();
    if k >= (CELLS - 1) as f64 {
        CELLS - 1
    } else if k > 0.0 {
        k as usize
    } else {
        0
    }
}

/// The plane normal of a triangle: (V1 - V0) x (V0 - V2) brought to unit length by q of the
/// sum of its squares, or the zero vector.
fn plane_normal(v: [Vec3; 3]) -> Vec3 {
    let a = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
    let b = [v[0][0] - v[2][0], v[0][1] - v[2][1], v[0][2] - v[2][2]];
    let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    if c == [0.0; 3] {
        return [0.0; 3];
    }
    let q = rsqrt_as(V, (c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]);
    [c[0] * q, c[1] * q, c[2] * q]
}

/// A leaf shape as a mesh of's kind 1, and its box.
fn shape_mesh(number: usize, s: &LeafShape, first: &LeafShape) -> Result<(StoredMesh, Bounds), TreeOpen> {
    if s.points.is_empty() {
        return Err(TreeOpen::NoPoints { shape: number });
    }
    let mut faces = Vec::with_capacity(s.triangles.len());
    for (i, t) in s.triangles.iter().enumerate() {
        let check = |what: &'static str, ns: &[u32], count: usize| -> Result<(), TreeOpen> {
            match ns.iter().find(|&&n| n as usize >= count) {
                Some(&number_) => Err(TreeOpen::TriangleNumber { shape: number, triangle: i, what, number: number_ }),
                None => Ok(()),
            }
        };
        check("point", &t[0..3], s.points.len())?;
        check("normal", &t[3..6], s.normals.len())?;
        check("coordinate", &t[6..9], s.coords.len())?;
        let p: [usize; 3] = std::array::from_fn(|k| t[k] as usize);
        faces.push(Face {
            points: p,
            normals: std::array::from_fn(|k| t[3 + k] as usize),
            coords: std::array::from_fn(|k| t[6 + k] as usize),
            // : the plane normal is made from the points of the
            // stored tree's first shape with the triangle's point numbers; a number beyond that
            // shape's points takes (0, 0, 0), the stated fallback (Open).
            plane: plane_normal(p.map(|n| first.points.get(n).copied().unwrap_or([0.0; 3]))),
            // a leaf's triangles read the corners' normals and coordinates ('s kind 1)
            words: Face::KIND_1.0,
            flags: Face::KIND_1.1,
        });
    }
    let own = widen(bounds_of(s.points.iter().copied()));
    let mesh = StoredMesh {
        points: s.points.clone(),
        normals: s.normals.clone(),
        coords: s.coords.iter().map(|c| [f64::from(c[0]), f64::from(c[1])]).collect(),
        faces,
        bounds: [own.min[0], own.min[1], own.min[2], own.max[0], own.max[1], own.max[2]],
        solid_in_group: false,
        fast: None,
    };
    Ok((mesh, own))
}

impl TreeModel {
    /// Says which stored tree (by the scene's number) the model shows, so that trees that show
    /// one stored tree share the marks of its items.
    pub fn with_stored(mut self, stored: usize) -> TreeModel {
        self.stored = Some(stored);
        self
    }

    /// The number of the stored tree the model shows, if the scene said.
    pub fn stored(&self) -> Option<usize> {
        self.stored
    }

    /// The tree of a stored tree's parts.
    pub fn new(shapes: &[LeafShape], pieces: &[WoodPiece], leaves: &[Leaf]) -> Result<TreeModel, TreeOpen> {
        if pieces.is_empty() && leaves.is_empty() {
            return Err(TreeOpen::Empty);
        }
        let mut meshes = Vec::new();
        let mut shape_boxes = Vec::new();
        for (n, s) in shapes.iter().enumerate() {
            let (m, b) = shape_mesh(n, s, &shapes[0])?;
            meshes.push(m);
            shape_boxes.push(b);
        }
        let cube = Bounds { min: [-1.0; 3], max: [1.0; 3] };
        let mut ps: Vec<Piece> = Vec::with_capacity(pieces.len());
        for (k, p) in pieces.iter().enumerate() {
            if let Some(g) = p.parent {
                if g as usize >= k {
                    return Err(TreeOpen::PieceParent { piece: k, parent: g });
                }
            }
            let Some(map) = tree_map(p.transform.rows, p.transform.translation) else {
                // : the piece is not drawn, the others
                // are drawn where the rules put them
                ps.push(Piece { map: Placement::IDENTITY, own: EMPTY, listing: EMPTY, list: Vec::new(), drawn: false });
                continue;
            };
            let own = bounds_of(corners(&cube).into_iter().map(|c| out_of(&map, c)));
            ps.push(Piece { map, own, listing: widen(own), list: Vec::new(), drawn: true });
        }
        // (the links: k itself where its box overlaps itself, then g_k where
        // linked, then the pieces that grow out of k and are linked with it, by number)
        for k in 0..ps.len() {
            let mut list = Vec::new();
            if overlap(&ps[k].own, &ps[k].own) {
                list.push(k);
            }
            if let Some(g) = pieces[k].parent {
                if overlap(&ps[k].own, &ps[g as usize].own) {
                    list.push(g as usize);
                }
            }
            for (c, p) in pieces.iter().enumerate().skip(k + 1) {
                if p.parent == Some(k as u32) && overlap(&ps[c].own, &ps[k].own) {
                    list.push(c);
                }
            }
            ps[k].list = list;
        }
        let mut ls = Vec::with_capacity(leaves.len());
        for (j, l) in leaves.iter().enumerate() {
            let sb = shape_boxes.get(l.shape as usize).ok_or(TreeOpen::LeafShape { leaf: j, shape: l.shape })?;
            let Some(map) = tree_map(l.transform.rows, l.transform.translation) else {
                // : a leaf with a singular map is not drawn
                ls.push(LeafItem { shape: l.shape as usize, map: Placement::IDENTITY, listing: EMPTY, drawn: false });
                continue;
            };
            let own = bounds_of(corners(sb).into_iter().map(|c| out_of(&map, c)));
            ls.push(LeafItem { shape: l.shape as usize, map, listing: widen(own), drawn: true });
        }
        let all = ps.iter().filter(|p| p.drawn).map(|p| p.listing).chain(ls.iter().filter(|l| l.drawn).map(|l| l.listing));
        let mut bounds = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
        for b in all {
            for a in 0..3 {
                bounds.min[a] = bounds.min[a].min(b.min[a]);
                bounds.max[a] = bounds.max[a].max(b.max[a]);
            }
        }
        // step 1: each item in every cell from its listing box's lowest corner's
        // cell to its highest corner's, a cell listing its items in the items' order.
        let mut cells = vec![Vec::new(); CELLS * CELLS * CELLS];
        let listings = ps.iter().map(|p| (p.drawn, &p.listing)).chain(ls.iter().map(|l| (l.drawn, &l.listing)));
        for (item, (drawn, b)) in listings.enumerate() {
            if !drawn {
                continue;
            }
            let lo: Vec<usize> = (0..3).map(|a| cell_of(b.min[a], bounds.min[a], bounds.max[a])).collect();
            let hi: Vec<usize> = (0..3).map(|a| cell_of(b.max[a], bounds.min[a], bounds.max[a])).collect();
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        cells[(x * CELLS + y) * CELLS + z].push(item as u32);
                    }
                }
            }
        }
        // : a cell's count of items cannot pass 65,535; refused with a reason
        if let Some(full) = cells.iter().map(Vec::len).max().filter(|&m| m >= 65_536) {
            return Err(TreeOpen::CellTooFull { items: full });
        }
        Ok(TreeModel { pieces: ps, leaves: ls, shapes: meshes, bounds, cells, stored: None })
    }

    /// True where nothing of the tree is drawn (every piece and every leaf has a singular map).
    pub fn is_blank(&self) -> bool {
        !self.pieces.iter().any(|p| p.drawn) && !self.leaves.iter().any(|l| l.drawn)
    }

    /// Whether the tree holds the point `p` ('s test of the eye): the tree asks each of its
    /// pieces, handing it the point itself and not the point carried into the tree's own space; a
    /// piece answers yes where `p` carried by the inverse of the piece's map lies less than one half
    /// from the piece's centre or where that distance is not a number, and a piece whose map is
    /// singular (its inverse carries every point to numbers that are not numbers) holds every point.
    pub fn holds_point(&self, p: Vec3) -> bool {
        self.pieces.iter().any(|k| {
            if !k.drawn {
                return true;
            }
            let q = into(&k.map, p);
            let d = ((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]).sqrt();
            d < 0.5 || d.is_nan()
        })
    }

    /// The number of pieces.
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    /// The inverse of piece `k`'s map.
    pub fn piece_inverse(&self, k: usize) -> ([Vec3; 3], Vec3) {
        map_inverse(&self.pieces[k].map)
    }

    /// The inverse of leaf `j`'s map.
    pub fn leaf_inverse(&self, j: usize) -> ([Vec3; 3], Vec3) {
        map_inverse(&self.leaves[j].map)
    }

    /// The number of stored leaf shapes.
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// Shape `s`'s box (its points' bounds moved out by a hundredth of the extent): lowest x,
    /// y, z, then highest.,
    pub fn shape_box(&self, s: usize) -> [f64; 6] {
        self.shapes[s].bounds
    }

    /// The number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// The tree's bounds , in the tree's space.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The box the structure lists the tree object by: the 8 corners of the tree's bounds
    /// carried by `w`, not widened.
    pub fn world_box(&self, w: &Placement) -> Bounds {
        bounds_of(corners(&self.bounds).into_iter().map(|c| out_of(w, c)))
    }

    /// The field of piece `k` at the point `p` of the tree's space.
    pub fn field(&self, k: usize, p: Vec3, o: &Orders) -> f64 {
        let q = into(&self.pieces[k].map, p);
        let u = sum3(o.field_squares, q[0] * q[0], q[1] * q[1], q[2] * q[2]);
        if u > 1.0 { 0.0 } else { polynomial(u) }
    }

    /// The sum of the fields of `set`, added in the set's order.
    fn sum(&self, set: &[usize], p: Vec3, o: &Orders) -> f64 {
        let mut s = 0.0;
        for &k in set {
            s = s + self.field(k, p, o);
        }
        s
    }

    /// The set of pieces a ray works with for piece `k0`.
    fn set_of(&self, k0: usize, ray: &Ray, lo: f64, far: f64) -> Vec<usize> {
        let mut visited = Vec::new();
        self.visit(k0, ray, lo, far, &mut visited);
        visited.reverse();
        visited
    }

    fn visit(&self, j: usize, ray: &Ray, lo: f64, far: f64, visited: &mut Vec<usize>) {
        if visited.contains(&j) || meets(&self.pieces[j].own, ray, lo, far).is_none() {
            return;
        }
        visited.push(j);
        for &n in &self.pieces[j].list {
            if n != j {
                self.visit(n, ray, lo, far, visited);
            }
        }
    }

    /// The stretch of the set: T, U and the chord C.
    fn stretch(&self, set: &[usize], ray: &Ray, o: &Orders) -> Option<(f64, f64, f64)> {
        let (mut t, mut u, mut chord) = (0.0f64, 0.0f64, 0.0f64);
        for &k in set {
            let m = &self.pieces[k].map;
            let org = into(m, ray.origin);
            let d = into_dir(m, ray.dir);
            let od = dot3(o.dot_products, org, d);
            let dd = dot3(o.direction_squares, d, d);
            let oo = dot3(o.origin_squares, org, org);
            let c = -(od / dd);
            let len = dot3(o.direction_length, d, d).sqrt();
            let a = c * len;
            let x2 = a * a;
            let under = x2 - (oo - 1.0);
            if (c < 0.0 && oo > 1.0) || !(under >= 0.0) {
                continue;
            }
                        let h = under.sqrt() / len;
            let (t0, t1) = (c - h, h + c);
            if t == 0.0 || t.is_nan() || t0 <= t {
                t = t0;
            }
            if t < 0.0 || t.is_nan() {
                t = 0.0;
            }
            if u == 0.0 || u.is_nan() {
                u = t1;
            } else if t1 > u {
                u = t1;
            }
            if chord == 0.0 || chord.is_nan() || 2.0 * h <= chord {
                chord = 2.0 * h;
            }
        }
        if chord == 0.0 || u < t { None } else { Some((t, u, chord)) }
    }

    /// Where a ray meets the surface of a set of pieces.
    fn crossing(&self, set: &[usize], ray: &Ray, o: &Orders) -> Option<Crossing> {
        let (t, u, chord) = self.stretch(set, ray, o)?;
        // Step 2: the shifted origin.
        let origin = [
            ray.origin[0] + MARCH_SHIFT * ray.dir[0],
            ray.origin[1] + MARCH_SHIFT * ray.dir[1],
            ray.origin[2] + MARCH_SHIFT * ray.dir[2],
        ];
        let shifted = Ray { origin, dir: ray.dir };
        let s_at = |dist: f64| self.sum(set, shifted.at(dist), o);
        // Step 3: the march.
        if !(t < u) {
            return None;
        }
        let inside_at_start = s_at(t) > SURFACE;
        let g = chord * STEP_OF_CHORD;
        // step 3 : sample k at (g + T) + a_k, g + T rounded
        // once and kept; taken while a_k + T is below U (the first always: T < U above).
        let gt = g + t;
        let mut a = 0.0f64;
        let mut met = None;
        for _ in 0..GUARD {
            if !(a + t < u) {
                break;
            }
            let dist = gt + a;
            if (s_at(dist) > SURFACE) != inside_at_start {
                met = Some((a, dist));
                break;
            }
            a = a + g;
        }
        let (mut near, first) = met?;
        // Step 4: the halving.
        let in_band = |s: f64| s <= SURFACE && s > BAND_LOW;
        let mut dist = first;
        let mut s = g;
        let mut found = in_band(s_at(dist));
        let mut n = 0;
        while !found {
            n += 1;
            if n > GUARD / 1024 {
                return None;
            }
            s = s / 2.0;
            dist = (near + s) + t;
            let sum = s_at(dist);
            if in_band(sum) {
                found = true;
            } else if (sum > SURFACE) == inside_at_start {
                near = near + s;
            }
        }
        let p = shifted.at(dist);
        // Step 5: the normal by central differences.
        let d = |axis: usize, sign: f64| {
            let mut q = p;
            q[axis] = if sign < 0.0 { q[axis] - NORMAL_OFFSET } else { q[axis] + NORMAL_OFFSET };
            self.sum(set, q, o)
        };
        let n3 = [d(0, -1.0) - d(0, 1.0), d(1, -1.0) - d(1, 1.0), d(2, -1.0) - d(2, 1.0)];
        let q = rsqrt_as(V, dot3(o.field_normal, n3, n3));
        Some(Crossing { t: dist, point: p, normal: [n3[0] * q, n3[1] * q, n3[2] * q] })
    }
}

/// A crossing with the wood, in the tree's space.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Crossing {
    t: f64,
    point: Vec3,
    /// Of unit length; it points where the field falls.
    normal: Vec3,
}

/// What a hit is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The wood: reported for the stand-in.
    Wood,
    /// A leaf of the shape of this number: reported for the shape object.
    Leaf { shape: usize },
}

/// A hit found or kept, in the tree's space (distances on the tree's copy of the ray).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Found {
    t: f64,
    point: Vec3,
    normal: Vec3,
    uv: [f64; 2],
    du: Vec3,
    dv: Vec3,
    on: Surface,
}

/// A hit on a tree, in the world.,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeHit {
    pub t: f64,
    pub point: Vec3,
    pub normal: Vec3,
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
    pub on: Surface,
}

/// What the items of one stored tree remember for one ray: every tree that shows the stored
/// tree has the same items, so these marks and the leaves' kept hits are shared by them.
#[derive(Clone, Debug, Default)]
pub struct ItemMarks {
    /// Per item: bit 0 the missed mark, bit 1 the kept mark. Sized at the first ask that needs it.
    items: Vec<u8>,
    leaf_kept: HashMap<usize, Found>,
}

/// The one crossing the pieces keep for a ray: one for the whole scene, whichever tree and
/// whichever stored tree kept it.
#[derive(Clone, Copy, Debug, Default)]
pub struct PieceCrossing(Option<Crossing>);

/// What a tree, its items and its pieces remember for one ray.. The tree's own
/// missed mark and kept hit are its own; the items' marks and the kept crossing are held here
/// while a tree is asked and exchanged with the shared ones before and after (`exchange`).
#[derive(Clone, Debug, Default)]
pub struct RayMarks {
    missed: bool,
    kept: Option<Found>,
    shared: ItemMarks,
    /// The one kept crossing of the pieces.
    crossing: Option<Crossing>,
}

const MISSED: u8 = 1;
const KEPT: u8 = 2;

/// The answer to one ask of a tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asked {
    pub hit: Option<TreeHit>,
    /// The ray's far end after the ask (the kept distance where the ask kept a hit).
    pub far_end: f64,
    /// Whether a piece's crossing was taken in the ask (found as the hit of a cell or answered
    /// from the kept crossing): the stand-in is handed the wood material.
    pub crossing_taken: bool,
}

impl RayMarks {
    pub fn new() -> RayMarks {
        RayMarks::default()
    }

    /// The distance of the hit the tree holds for the ray, if any.
    pub fn kept_distance(&self) -> Option<f64> {
        self.kept.map(|k| k.t)
    }

    /// Swaps the shared marks of the stored tree's items and the scene's kept crossing with the
    /// ones held here; called before an ask with the shared ones and after it again, so that
    /// trees that show one stored tree answer from the same marks.
    pub fn exchange(&mut self, items: &mut ItemMarks, crossing: &mut PieceCrossing) {
        std::mem::swap(&mut self.shared, items);
        std::mem::swap(&mut self.crossing, &mut crossing.0);
    }

    /// Whether the tree holds the missed mark for the ray.
    pub fn missed(&self) -> bool {
        self.missed
    }
}

/// One cell's walk state and the ask's.
struct Walk<'a> {
    model: &'a TreeModel,
    marks: &'a mut RayMarks,
    /// The tree's copy of the ray.
    copy: Ray,
    /// F' (drawn in by kept crossings and hits).
    far: f64,
    taken: bool,
    o: &'a Orders,
}

impl TreeModel {
    /// Asks the tree about the world ray `ray` with the lower bound `lo`, the leaf bound `bl`
    /// and the far end `far`, `w` being the tree object's map; `marks` is what the tree holds
    /// for this ray from earlier asks.
    pub fn ask(&self, w: &Placement, marks: &mut RayMarks, ray: &Ray, lo: f64, bl: f64, far: f64, o: &Orders) -> Asked {
        let mut asked = Asked { hit: None, far_end: far, crossing_taken: false };
        // : a tree that holds a kept hit answers it where lo <= t <= BL.
        if let Some(k) = marks.kept {
            if lo <= k.t && k.t <= bl {
                asked.hit = Some(self.report(w, &k, o.kept_world, o));
            }
            return asked;
        }
        if marks.missed {
            return asked;
        }
        // Step 2: the copy of the ray.
        let tray = Ray { origin: into(w, ray.origin), dir: into_dir(w, ray.dir) };
        let copy = Ray {
            origin: [
                tray.origin[0] + COPY_SHIFT * tray.dir[0],
                tray.origin[1] + COPY_SHIFT * tray.dir[1],
                tray.origin[2] + COPY_SHIFT * tray.dir[2],
            ],
            dir: tray.dir,
        };
        let world_box = bounds_of(corners(&self.bounds).into_iter().map(|c| out_of(w, c)));
        let (lo2, bl2) = match meets(&world_box, ray, lo, far) {
            Some((e, x)) => (e, x),
            None => (lo, bl),
        };
        if marks.shared.items.is_empty() {
            marks.shared.items = vec![0; self.pieces.len() + self.leaves.len()];
        }
        let mut walk = Walk { model: self, marks, copy, far, taken: false, o };
        let found = walk.run(lo2, bl2);
        asked.crossing_taken = walk.taken;
        // Step 5: the answer.
        match found {
            None => walk.marks.missed = true,
            Some(h) => {
                let t = h.t;
                if lo < t && t < bl {
                    asked.hit = Some(self.report(w, &h, o.found_world, o));
                } else if t <= lo {
                    walk.marks.missed = true;
                } else if bl <= t && t <= far {
                    walk.marks.kept = Some(h);
                    asked.far_end = t;
                } else {
                    walk.marks.missed = true;
                }
            }
        }
        asked
    }

    /// The hit in the world: the point carried by W, the normal by the transpose of W's inverse
    /// and brought to unit length by q of its squared length.
    fn report(&self, w: &Placement, h: &Found, grouping: u8, _o: &Orders) -> TreeHit {
        let c = w.carry_normal(h.normal, V);
        let q = rsqrt_as(V, dot3(grouping, c, c));
        TreeHit {
            t: h.t,
            point: out_of(w, h.point),
            normal: [c[0] * q, c[1] * q, c[2] * q],
            uv: h.uv,
            du: h.du,
            dv: h.dv,
            on: h.on,
        }
    }
}

impl Walk<'_> {
    /// Steps 3 and 4 of: the cells of the tree's bounds in the order the copy enters
    /// them. `lo` and `bl` are lo' and BL'. The hit found, if any.
    fn run(&mut self, lo: f64, bl: f64) -> Option<Found> {
        let b = self.model.bounds;
        let (near, _) = meets(&b, &self.copy, lo, self.far)?;
        let (o, d) = (self.copy.origin, self.copy.dir);
        let n = CELLS;
        let width: Vec<f64> = (0..3).map(|a| (b.max[a] - b.min[a]) / n as f64).collect();
        let mut cell = [0usize; 3];
        for a in 0..3 {
            let s = if near <= 0.0 { o[a] } else { o[a] + near * d[a] };
            let s = s.max(b.min[a]).min(b.max[a]);
            cell[a] = cell_of(s, b.min[a], b.max[a]);
        }
        let line = |a: usize, k: usize| b.min[a] + k as f64 * width[a];
        loop {
            let mut cb = Bounds { min: [0.0; 3], max: [0.0; 3] };
            for a in 0..3 {
                cb.min[a] = line(a, cell[a]) - CELL_OUT;
                cb.max[a] = line(a, cell[a] + 1) + CELL_OUT;
            }
            if let Some((e, x)) = meets(&cb, &self.copy, lo, self.far) {
                let list = &self.model.cells[(cell[0] * n + cell[1]) * n + cell[2]];
                if let Some(h) = self.cell(list, lo.max(e), bl.min(x)) {
                    return Some(h);
                }
                if bl < x {
                    return None;
                }
            }
            // The next cell: the scene grid's order of the cells, with 20 cells on each axis
            // (step 3;"The order of the cells")
            cell = super::structure::next_cell(&cell, n, o, d, &line)?;
        }
    }

    /// A cell: its items asked in the list's order over the lower bound `lower` and the leaf
    /// bound `upper`. The nearest hit.
    fn cell(&mut self, list: &[u32], lower: f64, mut upper: f64) -> Option<Found> {
        let p = self.model.pieces.len();
        let mut best = None;
        for &item in list {
            let item = item as usize;
            let flags = self.marks.shared.items[item];
            if flags & MISSED != 0 {
                continue;
            }
            if flags & KEPT != 0 {
                let kept_t = if item < p { self.marks.crossing.map(|c| c.t) } else { self.marks.shared.leaf_kept.get(&(item - p)).map(|k| k.t) };
                match kept_t {
                    Some(t) if t < lower || t > self.far => {
                        self.marks.shared.items[item] |= MISSED;
                        continue;
                    }
                    _ => {}
                }
            } else {
                let listing = if item < p { &self.model.pieces[item].listing } else { &self.model.leaves[item - p].listing };
                if meets(listing, &self.copy, lower, self.far).is_none() {
                    self.marks.shared.items[item] |= MISSED;
                    continue;
                }
            }
            let answer = if item < p { self.ask_piece(item, lower, upper) } else { self.ask_leaf(item - p, lower, upper) };
            if let Some(h) = answer {
                upper = h.t;
                self.far = h.t;
                best = Some(h);
            }
        }
        best
    }

    /// A piece asked: the crossing with its set, and what the comparisons do with it.
    fn ask_piece(&mut self, k0: usize, lower: f64, upper: f64) -> Option<Found> {
        if self.marks.shared.items[k0] & KEPT != 0 {
            let c = self.marks.crossing?;
            if lower <= c.t && c.t <= upper {
                self.taken = true;
                return Some(self.wood(c, k0));
            }
            return None;
        }
        let set = self.model.set_of(k0, &self.copy, lower, self.far);
        if set.is_empty() {
            return None;
        }
        let Some(c) = self.model.crossing(&set, &self.copy, self.o) else {
            for &k in &set {
                self.marks.shared.items[k] |= MISSED;
            }
            return None;
        };
        if lower < c.t && c.t < upper {
            self.taken = true;
            Some(self.wood(c, k0))
        } else if c.t <= lower {
            for &k in &set {
                self.marks.shared.items[k] |= MISSED;
            }
            None
        } else if upper <= c.t && c.t <= self.far {
            for &k in &set {
                self.marks.shared.items[k] |= KEPT;
            }
            self.marks.crossing = Some(c);
            self.far = c.t;
            None
        } else {
            for &k in &set {
                self.marks.shared.items[k] |= MISSED;
            }
            None
        }
    }

    /// The hit on the wood at crossing `c`, its texture coordinates at the piece asked.
    fn wood(&self, c: Crossing, asked: usize) -> Found {
        let p = into(&self.model.pieces[asked].map, c.point);
        let tex = super::texture(Shape::Sphere, Part::Curved, p, V);
        Found { t: c.t, point: c.point, normal: c.normal, uv: tex.uv, du: tex.du, dv: tex.dv, on: Surface::Wood }
    }

    /// A leaf asked.
    fn ask_leaf(&mut self, j: usize, lower: f64, upper: f64) -> Option<Found> {
        let item = self.model.pieces.len() + j;
        if self.marks.shared.items[item] & KEPT != 0 {
            let k = *self.marks.shared.leaf_kept.get(&j)?;
            return (lower <= k.t && k.t <= upper).then_some(k);
        }
        let leaf = &self.model.leaves[j];
        let local = Ray { origin: into(&leaf.map, self.copy.origin), dir: into_dir(&leaf.map, self.copy.dir) };
        let mesh = &self.model.shapes[leaf.shape];
        let Some(h) = mesh_hit(mesh, &local, Range { near: 0.0, far: f64::INFINITY }, local.dir, V) else {
            self.marks.shared.items[item] |= MISSED;
            return None;
        };
        assert!(h.shade.is_some(), "a leaf's triangles are of kind 1");
        let m = super::world_normals(&Placement::IDENTITY, &h, V, &super::SumOrders::of(V)).1;
        let c = leaf.map.carry_normal(m, V);
        let q = rsqrt_as(V, dot3(self.o.leaf_normal, c, c));
        let found = Found {
            t: h.t,
            point: out_of(&leaf.map, h.point),
            normal: [c[0] * q, c[1] * q, c[2] * q],
            uv: h.uv,
            du: h.du,
            dv: h.dv,
            on: Surface::Leaf { shape: leaf.shape },
        };
        if lower < found.t && found.t < upper {
            Some(found)
        } else if found.t <= lower {
            self.marks.shared.items[item] |= MISSED;
            None
        } else if upper <= found.t && found.t <= self.far {
            self.marks.shared.items[item] |= KEPT;
            self.marks.shared.leaf_kept.insert(j, found);
            self.far = found.t;
            None
        } else {
            self.marks.shared.items[item] |= MISSED;
            None
        }
    }
}
