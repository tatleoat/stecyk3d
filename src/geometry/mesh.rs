// SPDX-License-Identifier: MIT
//! Where a ray meets a document's mesh (the arrays reads): the nearest triangle, its
//! facing, the shading normal (Phong's interpolation of the stored normals), the texture
//! coordinates and their directions.
//!
//! , (the list of a mesh held by a group,
//! (Meshes)
//!
//! Every operation is one IEEE binary64 operation in the order written , no fused
//! multiply-add; the groupings of the three-term sums are the vectors' (Accuracy),
//! `SumOrders` below.

use std::fmt;

use crate::geom::Vec3;
use crate::numerics::rsqrt_as;
use crate::scene_meshes::MeshResource;
use crate::version::RenderAs;

use super::{LocalHit, Part, Placement, Range, Ray};

/// The plane threshold on s = d·P and the edge threshold of the weights' first formula.
const NEAR_ZERO: f64 = 1e-10;

/// The weights' tolerance (lo_w, hi_w).
fn tolerance(version: RenderAs) -> (f64, f64) {
    match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (-1e-8, 1.00000001),
        RenderAs::Bryce50 => (-1e-6, 1.000001),
    }
}

/// One triangle prepared for hits: its numbers (those its corners do not read are 0 and never
/// read), its stored plane normal, and the corner words and flag word that say what each
/// corner reads.. (Meshes)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub points: [usize; 3],
    pub normals: [usize; 3],
    pub coords: [usize; 3],
    /// P, as stored.
    pub plane: Vec3,
    /// The corner words: a corner whose word has bit 0 set takes the plane normal, bit 1 set
    /// the coordinates (0, 0).
    pub words: [u32; 3],
    /// The flag word: bit 0 set mixes the corners' normals, bit 1 set takes the corners'
    /// coordinates.
    pub flags: u32,
}

impl Face {
    /// The words of's kind 1 (normals and coordinates), 2 (normals only) and 3 (neither).
    pub const KIND_1: ([u32; 3], u32) = ([0; 3], 3);

    /// Whether the shading normal mixes the corners' normals (flag word bit 0).
    fn smooth(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Whether the coordinates are the corners' (flag word bit 1).
    fn corner_coords(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// A mesh as draws it.; (Meshes)
#[derive(Clone, Debug, PartialEq)]
pub struct StoredMesh {
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// The singles of read as doubles.
    pub coords: Vec<[f64; 2]>,
    pub faces: Vec<Face>,
    /// The stored box: lowest x, y, z, then highest.
    pub bounds: [f64; 6],
    /// Whether the mesh object has the solid bit (0x2000000 of its second flag word), read for
    /// a mesh held by a combined group and by nothing else.
    pub solid_in_group: bool,
    /// A bounding-volume tree over the triangles, for a mesh of many triangles (the description's meshes carrying
    /// normals or coordinates); `None` scans every triangle as the documents' meshes do.
    /// (the description's meshes)
    pub fast: Option<Fast>,
}

/// The triangles as corners and their tree: the nearest triangle is found through the tree, then met once more by
/// 's own steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Fast {
    tris: Vec<super::Triangle>,
    accel: super::mesh_bvh::Accel,
}

impl Fast {
    /// The tree's triangles from the mesh's own points and faces.
    pub fn of(points: &[Vec3], faces: &[Face]) -> Fast {
        let tris = faces
            .iter()
            .map(|f| super::Triangle { corners: f.points.map(|k| points[k]), normal: None })
            .collect();
        Fast { tris, accel: Default::default() }
    }
}

impl StoredMesh {
    /// The own box a texture mapping takes: the stored box of as the document stores it,
    /// whatever it holds (in every observed document it is the points' bounds moved out by a
    /// hundredth of the extent)., (Where the stored box
    /// is used, revised)
    pub fn own_box(&self) -> ([f64; 3], [f64; 3]) {
        if self.points.is_empty() {
            return ([f64::NAN; 3], [f64::NAN; 3]);
        }
        let b = self.bounds;
        ([b[0], b[1], b[2]], [b[3], b[4], b[5]])
    }
}

/// Why a mesh cannot be drawn: a case leaves Open (the implementer's choice, refusal
/// with a reason being acceptable). A mesh whose triangles differ in their words is drawn
/// triangle by triangle (also the implementer's choice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshOpen {
    /// A point, normal or coordinate number out of range where a corner reads it.
    Number { triangle: usize, what: &'static str, number: u32, count: usize },
}

impl fmt::Display for MeshOpen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshOpen::Number { triangle, what, number, count } => {
                write!(f, "triangle {triangle} names {what} {number}, but the mesh holds {count} (Open, GEO-15 Errors)")
            }
        }
    }
}

impl std::error::Error for MeshOpen {}

impl StoredMesh {
    /// The mesh of a resource, every number a corner reads in range. Each triangle is read by
    /// its own corner words and flag word (the four combinations of the documents and the
    /// others alike).
    pub fn from_resource(r: &MeshResource) -> Result<StoredMesh, MeshOpen> {
        let mut faces = Vec::with_capacity(r.triangles.len());
        for (i, tri) in r.triangles.iter().enumerate() {
            // The numbers of the corners that read them (word bit `bit` clear) in range.
            let numbers = |what: &'static str, ns: [u32; 3], count: usize, bit: u32| -> Result<[usize; 3], MeshOpen> {
                let mut out = [0usize; 3];
                for k in 0..3 {
                    if tri.corner_words[k] & bit == 0 || bit == 0 {
                        if ns[k] as usize >= count {
                            return Err(MeshOpen::Number { triangle: i, what, number: ns[k], count });
                        }
                        out[k] = ns[k] as usize;
                    }
                }
                Ok(out)
            };
            let points = numbers("point", tri.points, r.points.len(), 0)?;
            let normals = match tri.normals {
                Some(ns) => numbers("normal", ns, r.normals.len(), 1)?,
                None => [0; 3],
            };
            let coords = match tri.coords {
                Some(ns) => numbers("coordinate", ns, r.coords.len(), 2)?,
                None => [0; 3],
            };
            faces.push(Face { points, normals, coords, plane: tri.plane, words: tri.corner_words, flags: tri.flag_word });
        }
        Ok(StoredMesh {
            points: r.points.clone(),
            normals: r.normals.clone(),
            coords: r.coords.iter().map(|c| [f64::from(c[0]), f64::from(c[1])]).collect(),
            faces,
            bounds: r.bounds,
            solid_in_group: false,
            fast: None,
        })
    }
}

/// A grouping of a three-term sum a + b + c.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping {
    /// (a + b) + c
    Left,
    /// a + (b + c)
    Right,
    /// (a + c) + b
    Outer,
}

impl Grouping {
    fn sum(self, a: f64, b: f64, c: f64) -> f64 {
        match self {
            Grouping::Left => (a + b) + c,
            Grouping::Right => a + (b + c),
            Grouping::Outer => (a + c) + b,
        }
    }

    fn dot(self, a: Vec3, b: Vec3) -> f64 {
        self.sum(a[0] * b[0], a[1] * b[1], a[2] * b[2])
    }
}

/// The groupings of's three-term sums, which its vectors decide (Accuracy).
/// (Meshes: last bits)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SumOrders {
    /// s = d·P.
    pub facing: Grouping,
    /// w·P, the distance's numerator.
    pub distance: Grouping,
    /// Each component of m = n0 rho + n1 alpha + n2 beta (terms in that order).
    pub blend: Grouping,
    /// m·m, whose q brings m to unit length.
    pub blend_length: Grouping,
    /// u of kind 1: c0 rho + c1 alpha + c2 beta (the first coordinates).
    pub u: Grouping,
    /// v of kind 1, before it is turned (1 - v): the second coordinates.
    pub v: Grouping,
    /// The squared length of P carried to the world, whose q brings it to unit length.
    pub world_length: Grouping,
    /// The same for m' carried to the world.
    pub world_shade_length: Grouping,
}

impl SumOrders {
    /// The groupings the vectors give for each version. (Meshes)
    pub fn of(version: RenderAs) -> SumOrders {
        use Grouping::{Left, Outer, Right};
        let (blend_length, u, v, world_length, world_shade_length) = match version {
            RenderAs::Bryce30 => (Right, Left, Outer, Right, Right),
            RenderAs::Bryce31 => (Right, Right, Right, Right, Right),
            RenderAs::Bryce50 => (Left, Outer, Outer, Left, Outer),
        };
        SumOrders { facing: Right, distance: Right, blend: Left, blend_length, u, v, world_length, world_shade_length }
    }

    /// The orders of a mesh's list (a mesh held by a group), where they differ from the hit's:
    /// s as 3.1 (x + z) + y; t's numerator as 3.1 (x + y) + z and as 5.0 (x + z) + y; kind 1's u
    /// and v as 3.0 both (c1 alpha + c2 beta) + c0 rho, as 3.1 and 5.0 u (c0 rho + c2 beta) +
    /// c1 alpha and v (c0 rho + c1 alpha) + c2 beta; the shading normal's squared length after
    /// it is carried, as 5.0 (x^2 + y^2) + z^2.
    pub fn of_list(version: RenderAs) -> SumOrders {
        use Grouping::{Left, Outer, Right};
        let hit = SumOrders::of(version);
        match version {
            RenderAs::Bryce30 => SumOrders { u: Right, v: Right, ..hit },
            RenderAs::Bryce31 => SumOrders { facing: Outer, distance: Left, u: Outer, v: Left, ..hit },
            RenderAs::Bryce50 => SumOrders { distance: Outer, u: Outer, v: Left, world_shade_length: Left, ..hit },
        }
    }
}

fn minus(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// A triangle met: t, the point and the weights of corners 1 and 2. (Meeting a
/// triangle)
struct Met {
    t: f64,
    point: Vec3,
    alpha: f64,
    beta: f64,
}

/// 's steps 1 to 6 for one triangle.
fn meet(mesh: &StoredMesh, face: &Face, r: &Ray, range: Range, tol: (f64, f64), o: &SumOrders) -> Option<Met> {
    let p = face.plane;
    // Step 1: a ray in or nearly in the plane's direction meets nothing (NaN included).
    let s = o.facing.dot(r.dir, p);
    if !(s <= -NEAR_ZERO || s >= NEAR_ZERO) {
        return None;
    }
    // Step 2.
    let v0 = mesh.points[face.points[0]];
    let w = minus(v0, r.origin);
    let t = o.distance.dot(w, p) / s;
    // Step 3: both ends included; a NaN fails.
    if !(range.near <= t && t <= range.far) {
        return None;
    }
    // Step 4.
    let q = r.at(t);
    // Step 5: the axis on which P is largest, every tie z.
    let (ax, ay, az) = (p[0].abs(), p[1].abs(), p[2].abs());
    let (i, j) = if ax > ay && ax > az {
        (1, 2)
    } else if ax < ay && az < ay {
        (0, 2)
    } else {
        (0, 1)
    };
    let v1 = mesh.points[face.points[1]];
    let v2 = mesh.points[face.points[2]];
    let a = v1[i] - v0[i];
    let b = v2[i] - v0[i];
    let c = v1[j] - v0[j];
    let e = v2[j] - v0[j];
    let u0 = q[i] - v0[i];
    let u1 = q[j] - v0[j];
    let (alpha, beta) = if a <= -NEAR_ZERO || a >= NEAR_ZERO {
        let beta = (a * u1 - c * u0) / (e * a - c * b);
        (((u0 - beta * b) / a), beta)
    } else {
        let beta = u0 / b;
        (((u1 - beta * e) / c), beta)
    };
    // Step 6: every comparison false for a NaN.
    let (lo_w, hi_w) = tol;
    let sum = alpha + beta;
    let inside = beta >= lo_w && beta <= hi_w && alpha >= lo_w && sum <= hi_w;
    inside.then_some(Met { t, point: q, alpha, beta })
}

/// The range cut to the ray's stretch in the stored box (closed; an axis with direction 0
/// needs the origin between the box's planes), or `None` where the ray misses the box.
fn box_cut(bounds: &[f64; 6], r: &Ray, range: Range) -> Option<Range> {
    let (mut near, mut far) = (range.near, range.far);
    for a in 0..3 {
        let (lo, hi, o, d) = (bounds[a], bounds[3 + a], r.origin[a], r.dir[a]);
        if d == 0.0 {
            if !(lo <= o && o <= hi) {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((lo - o) / d, (hi - o) / d);
        let (enter, leave) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        near = near.max(enter);
        far = far.min(leave);
    }
    (near <= far).then_some(Range { near, far })
}

/// dv of a kind 1 crossing in a mesh's list as 5.0 (which marks the crossing for `list_tangent`).
pub const LIST_KIND1_DV: Vec3 = [0.0, -1.0, 0.0];

/// du of a kind 1 crossing in a mesh's list as 5.0: from the crossing's shading normal n in the
/// world (carried and brought to unit length), g = (n_z r_z + n_y r_y) + n_x r_x with r the ray's
/// direction in the world; where g < 0 or is not a number n is kept, otherwise each component is
/// negated; then du = (-(0 - n_z), -0, -(n_x - 0)) with no other case. Only the sign of g is used.
pub fn list_tangent(n: Vec3, ray_dir: Vec3) -> Vec3 {
    let g = (n[2] * ray_dir[2] + n[1] * ray_dir[1]) + n[0] * ray_dir[0];
    let n = if g < 0.0 || g.is_nan() { n } else { [-n[0], -n[1], -n[2]] };
    [-(0.0 - n[2]), -0.0, -(n[0] - 0.0)]
}

/// A direction component of at most this magnitude counts as no component in the list's box test.
const LIST_BOX_SLOP: f64 = 1e-7;

/// step 1: whether the line of `r` (in the mesh's own space) passes the list's test through
/// the stored box (`bounds` as `x0 y0 z0 x1 y1 z1`, b0 the lowest corner and b1 the highest), axis
/// by axis. Where the origin's coordinate is not above b0's (or is not a number) the direction's
/// component must be at least 1e-7 (one that is not a number fails) and the line enters at
/// (b0 - o) / d; where it is not below b1's the component must be at most -1e-7 or not a number
/// and the line enters at (b1 - o) / d; strictly between them a component above 1e-7 enters at
/// (b0 - o) / d, one below -1e-7 (or not a number) at (b1 - o) / d, and one from -1e-7 to 1e-7
/// enters at minus infinity. The greatest entering distance (by "greater than" from x on) gives
/// the point o + d t, which must lie from b0 - 1e-7 to b1 + 1e-7 on each of the two other axes (a
/// coordinate that is not a number passes). Nothing else is tested: the entering distance may be
/// negative or lie beyond a leaving distance of another axis.
/// (the box test of a mesh's list)
fn list_box_met(bounds: &[f64; 6], r: &Ray) -> bool {
    let mut enter = [0.0f64; 3];
    for a in 0..3 {
        let (b0, b1, o, d) = (bounds[a], bounds[3 + a], r.origin[a], r.dir[a]);
        enter[a] = if !(o > b0) {
            if !(d >= LIST_BOX_SLOP) {
                return false;
            }
            (b0 - o) / d
        } else if !(o < b1) {
            if !(d <= -LIST_BOX_SLOP || d.is_nan()) {
                return false;
            }
            (b1 - o) / d
        } else if d > LIST_BOX_SLOP {
            (b0 - o) / d
        } else if d < -LIST_BOX_SLOP || d.is_nan() {
            (b1 - o) / d
        } else {
            f64::NEG_INFINITY
        };
    }
    let (mut axis, mut t) = (0, enter[0]);
    for a in 1..3 {
        if enter[a] > t {
            (axis, t) = (a, enter[a]);
        }
    }
    (0..3).filter(|&a| a != axis).all(|a| {
        let p = r.origin[a] + r.dir[a] * t;
        p.is_nan() || (p >= bounds[a] - LIST_BOX_SLOP && p <= bounds[3 + a] + LIST_BOX_SLOP)
    })
}

/// The nearest triangle of `mesh` the ray (in the mesh's own space) meets in lo <= t <= hi,
/// on equal t the greater number; with the plane normal P as `normal`, the shading normal m'
/// (kinds 1 and 2) as `shade`, and u, v, du, dv. `world_dir` is the ray's direction as given
/// in the world (the tangent's side).
pub fn mesh_hit(mesh: &StoredMesh, r: &Ray, range: Range, world_dir: Vec3, version: RenderAs) -> Option<LocalHit> {
    mesh_hit_with(mesh, r, range, world_dir, version, &SumOrders::of(version))
}

/// `mesh_hit` with the groupings given (for the vector test that searches them).
pub fn mesh_hit_with(
    mesh: &StoredMesh,
    r: &Ray,
    range: Range,
    world_dir: Vec3,
    version: RenderAs,
    orders: &SumOrders,
) -> Option<LocalHit> {
    let tol = tolerance(version);
    // : a ray that does not meet
    // the stored box (closed, cut by the range) gives no hit, and a crossing outside the box
    // gives no hit: the range is cut to the ray's stretch in the box.
    let range = box_cut(&mesh.bounds, r, range)?;
    let mut best: Option<(usize, Met)> = None;
    let probe = mesh.fast.as_ref().and_then(|f| super::mesh_bvh::Probe::new(r.origin, r.dir).map(|p| (f, p)));
    if let Some((f, p)) = probe {
        // the tree gives the nearest triangle (the lowest number on equal t); it is met again for its weights
        let hit = f.accel.get(&f.tris).nearest(&p, range.near, range.far, |k| {
            meet(mesh, &mesh.faces[k], r, range, tol, orders).map(|m| m.t)
        });
        if let Some((k, _)) = hit {
            best = meet(mesh, &mesh.faces[k], r, range, tol, orders).map(|m| (k, m));
        }
    } else {
        for (k, face) in mesh.faces.iter().enumerate() {
            // Meshes: a triangle at the best t so far replaces it (the greater number).
            let far = best.as_ref().map_or(range.far, |(_, m)| m.t);
            if let Some(m) = meet(mesh, face, r, Range { near: range.near, far }, tol, orders) {
                best = Some((k, m));
            }
        }
    }
    let (k, met) = best?;
    Some(finish_hit(mesh, k, &met, world_dir, version, orders))
}

/// The record gives the hit on triangle `k` met as `met`: the plane normal, the shading
/// normal, the texture coordinates and their directions. Shared by the hit and by a group
/// member's list (step 3).
fn finish_hit(mesh: &StoredMesh, k: usize, met: &Met, world_dir: Vec3, version: RenderAs, orders: &SumOrders) -> LocalHit {
    let face = &mesh.faces[k];
    let rho = 1.0 - (met.alpha + met.beta);
    let weights = [rho, met.alpha, met.beta];
    // : the flag word's bit 0 clear gives the plane normal
    // for the shading normal (no separate shading normal); set mixes the corners' normals.
    let shade = face.smooth().then(|| shading_normal(mesh, face, weights, version, orders));
    let mut hit = LocalHit::new(met.t, met.point, face.plane, Part::Triangle { index: k });
    hit.shade = shade;
    match face.corner_coords() {
        true => {
            // a corner whose own word has bit 1 set takes (0, 0)
            let c: [[f64; 2]; 3] =
                std::array::from_fn(|j| if face.words[j] & 2 != 0 { [0.0; 2] } else { mesh.coords[face.coords[j]] });
            let [w0, w1, w2] = weights;
            let u = orders.u.sum(c[0][0] * w0, c[1][0] * w1, c[2][0] * w2);
            let v = orders.v.sum(c[0][1] * w0, c[1][1] * w1, c[2][1] * w2);
            hit.uv = [u, 1.0 - v];
            (hit.du, hit.dv) = match version {
                RenderAs::Bryce30 => ([0.0; 3], [0.0; 3]),
                RenderAs::Bryce31 | RenderAs::Bryce50 => {
                    // The shading normal of the tangent: for a triangle with corner coordinates
                    // and no mixed normals, the plane normal brought to unit length (the
                    // implementer's choice: no combination of the documents is there).
                    let n = shade.unwrap_or_else(|| {
                        let p = face.plane;
                        let q = rsqrt_as(version, orders.blend_length.dot(p, p));
                        [p[0] * q, p[1] * q, p[2] * q]
                    });
                    tangents(n, world_dir, version)
                }
            };
        }
        false => {
            let [bx0, _, bz0, bx1, _, bz1] = mesh.bounds;
            let [x, _, z] = met.point;
            hit.uv = [(x + bx1) / (bx1 - bx0), (bz1 - z) / (bz1 - bz0)];
            hit.du = [1.0, 0.0, 0.0];
            hit.dv = [0.0, 0.0, -1.0];
        }
    }
    hit
}

/// The distance of the second crossing a mesh without the solid bit adds to each crossing: the
/// sum t + 0.00001 rounded once.
const SHEET_THICKNESS: f64 = 0.00001;

/// : every crossing of the ray's line (in the mesh's own space) with the mesh, nearest
/// first: the stored box must be met, each triangle meets by its steps 1, 2, 4, 5 and 6
/// (no range) gives one crossing, and unless the mesh object has the solid bit (0x2000000)
/// each is followed by a partner at t + 0.00001 that never shows. Equal t keep the order of
/// the triangles (our choice: step 5 leaves it to the implementer). The list's own
/// orders of the sums: `SumOrders::of_list`.
pub fn mesh_list(
    mesh: &StoredMesh,
    r: &Ray,
    solid: bool,
    world_dir: Vec3,
    version: RenderAs,
) -> Vec<super::crossings::LocalCrossing> {
    use super::crossings::LocalCrossing;
    let line = Range { near: f64::NEG_INFINITY, far: f64::INFINITY };
    // Step 1: the line is tested through the stored box by the list's own test .
    if !list_box_met(&mesh.bounds, r) {
        return Vec::new();
    }
    let (tol, orders) = (tolerance(version), SumOrders::of_list(version));
    let mut list = Vec::new();
    for (k, face) in mesh.faces.iter().enumerate() {
        // Step 2: no lo and no hi, a negative t counts.
        let Some(m) = meet(mesh, face, r, line, tol, &orders) else { continue };
        let mut hit = finish_hit(mesh, k, &m, world_dir, version, &orders);
        if face.corner_coords() {
            // : a kind 1 crossing has zero du and dv as 3.0 and 3.1; as 5.0 dv
            // is (0, -1, 0) and du is formed from the crossing's world shading normal where the
            // crossing is carried (`list_tangent`, applied by the scene's `finish`).
            (hit.du, hit.dv) = if version == RenderAs::Bryce50 { ([0.0; 3], LIST_KIND1_DV) } else { ([0.0; 3], [0.0; 3]) };
        }
        list.push(LocalCrossing::shown(hit));
        if !solid {
            list.push(LocalCrossing::hidden(m.t + SHEET_THICKNESS)); // Step 4
        }
    }
    // Step 5: nearest first (a stable sort).
    list.sort_by(|a, b| a.hit.t.total_cmp(&b.hit.t));
    list
}

/// The hit's normal and shading normal in the world: P, and m' where the kind has one,
/// each carried by the placement's inverse  and brought to unit length by q of
/// its squared length.
pub fn world_normals(placement: &Placement, h: &LocalHit, version: RenderAs, o: &SumOrders) -> (Vec3, Vec3) {
    let unit = |n: Vec3, g: Grouping| {
        let c = placement.carry_normal(n, version);
        let q = rsqrt_as(version, g.dot(c, c));
        [c[0] * q, c[1] * q, c[2] * q]
    };
    let normal = unit(h.normal, o.world_length);
    (normal, h.shade.map_or(normal, |m| unit(m, o.world_shade_length)))
}

/// m = n0 rho + n1 alpha + n2 beta brought to unit length by q of its squared length: the
/// shading normal in the mesh's own space.
fn shading_normal(mesh: &StoredMesh, face: &Face, w: [f64; 3], version: RenderAs, o: &SumOrders) -> Vec3 {
    // a corner whose own word has bit 0 set takes the plane normal (as stored)
    let n: [Vec3; 3] = std::array::from_fn(|j| if face.words[j] & 1 != 0 { face.plane } else { mesh.normals[face.normals[j]] });
    let m: Vec3 = std::array::from_fn(|c| o.blend.sum(n[0][c] * w[0], n[1][c] * w[1], n[2][c] * w[2]));
    let q = rsqrt_as(version, o.blend_length.dot(m, m));
    [m[0] * q, m[1] * q, m[2] * q]
}

/// Kind 1's du and dv as 3.1 and 5.0: n is m' turned against the world ray (kept where
/// r·n < 0), du = (-(0 - n_z), -0, -(n_x - 0)), except as 5.0 where n_x and n_z are both zero:
/// (-0, -(n_z - 0), -(0 - n_y)); dv = (0, -1, 0). The zeros and signs as written.
fn tangents(m: Vec3, world_dir: Vec3, version: RenderAs) -> (Vec3, Vec3) {
    // The order of this sum is not separated by any vector (only its sign is read).
    let g = world_dir[0] * m[0] + world_dir[1] * m[1] + world_dir[2] * m[2];
    let n = if g < 0.0 { m } else { [-m[0], -m[1], -m[2]] };
    let du = if version == RenderAs::Bryce50 && n[0] == 0.0 && n[2] == 0.0 {
        [-0.0, -(n[2] - 0.0), -(0.0 - n[1])]
    } else {
        [-(0.0 - n[2]), -0.0, -(n[0] - 0.0)]
    };
    (du, [0.0, -1.0, 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: [f64; 6] = [-1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

    fn ray(origin: Vec3, dir: Vec3) -> Ray {
        Ray { origin, dir }
    }

    /// step 1, each requirement as the entry states it.
    #[test]
    fn list_box_test_follows_the_entry() {
        // enters through the low z face; x and y parallel and strictly inside: entering -inf there
        assert!(list_box_met(&BOX, &ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0])));
        // below the low plane with a component that points away: the first requirement fails
        assert!(!list_box_met(&BOX, &ray([0.0, 0.0, -5.0], [0.0, 0.0, -1.0])));
        // outside a plane of x by 5e-8 and parallel to it: a component under 1e-7 is no component
        assert!(!list_box_met(&BOX, &ray([1.0 + 5e-8, 0.0, -5.0], [0.0, 0.0, 1.0])));
        // inside on every axis with no direction at all: the entering distance is minus infinity on
        // x, and the point's other coordinates are not numbers (0 times infinity): they pass
        assert!(list_box_met(&BOX, &ray([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])));
        // the point at the greatest entering distance lies outside on another axis
        assert!(!list_box_met(&BOX, &ray([-5.0, 3.0, 0.0], [1.0, 0.0, 0.0])));
        // a negative entering distance is allowed: the origin inside, the line enters behind it
        assert!(list_box_met(&BOX, &ray([0.0, 0.0, 0.0], [1.0, 0.5, 0.25])));
        // a component that is not a number fails the first requirement
        assert!(!list_box_met(&BOX, &ray([-5.0, 0.0, 0.0], [f64::NAN, 0.0, 0.0])));
    }

    /// step 3 as 5.0: du from the world shading normal and ray, signs of zeros as
    /// written.
    #[test]
    fn list_tangent_as_written() {
        // g = 1 (not below 0): n negated to (-0, -0, -1); du = (-(0 - -1), -0, -(-0 - 0)) = (-1, -0, 0)
        let du = list_tangent([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert_eq!(du.map(f64::to_bits), [(-1.0f64).to_bits(), (-0.0f64).to_bits(), 0.0f64.to_bits()]);
        // g = -1: n kept (0, 0, 1); du = (-(0 - 1), -0, -(0 - 0)) = (1, -0, -0)
        let du = list_tangent([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]);
        assert_eq!(du.map(f64::to_bits), [1.0f64.to_bits(), (-0.0f64).to_bits(), (-0.0f64).to_bits()]);
    }
}
