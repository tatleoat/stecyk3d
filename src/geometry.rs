// SPDX-License-Identifier: MIT
//! Rays against the primitive shapes.
//!
//! (03-crossings.md: the range-free per-shape pieces `flat_at`, `round_cap_at`,
//! `sphere_roots`, `pyramid_side`, `torus_roots`, used by `crossings.rs`),
//! as
//! revised , (08-groups.md: a combined group as an object; its list is
//! `group.rs`).
//! Where an entry leaves an arrangement Open (the order of a sum, a coefficient's form), the
//! choice is the one that gives and is marked "vector-provisional"
//! ("Sources of the rules").
//!
//! Every operation is one IEEE binary64 operation in the order written , with no
//! fused multiply-add: the order of each sum below is part of the result's bits.

mod crossings;
pub mod group;
pub mod light;
pub mod metaball;
mod mesh;
mod mesh_bvh; // (Speed on mesh scenes)
pub mod path;
pub mod relief;
mod roots;
pub mod smooth;
pub mod structure;
pub mod tree;
pub mod wide;

pub use crossings::{Crossing, LocalCrossing, SumOrder, carry, crossings, local_crossings};
pub use group::{Combination, GroupEntry, Groups, Member, Role, Source};
pub use mesh::{Face, Fast, Grouping, MeshOpen, StoredMesh, SumOrders, mesh_hit, mesh_hit_with, world_normals};
pub use relief::{ListedFace, Relief, ReliefFace, ReliefPart, ReliefSwitches};

use crate::geom::{Vec3, dot};
use crate::numerics::x87::{div_f64_by, sin_ext};
use crate::numerics::{X87_INDEFINITE, acos, atan_add, atan_div, quieted, rsqrt_as, sqrt_as};
use crate::version::RenderAs;

/// A ray in world space. `dir` need not be unit length; distances are in units of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    /// `o + t·d`, per component.;
    pub fn at(&self, t: f64) -> Vec3 {
        [self.origin[0] + t * self.dir[0], self.origin[1] + t * self.dir[1], self.origin[2] + t * self.dir[2]]
    }
}

/// The open interval of ray parameters searched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub near: f64,
    pub far: f64,
}

impl Range {
    fn holds(&self, t: f64) -> bool {
        t > self.near && t < self.far
    }
}

/// The primitive forms, each in its own space with y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Plane,
    Slab,
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Pyramid,
    /// `tube` is r, `ring` is R.
    Torus { tube: f64, ring: f64 },
    Square,
    Disc,
    /// A triangle soup: index into `Scene::meshes`. It reaches the geometry only through the
    /// Blender description (component `blender-addon`); a document's mesh is `StoredMesh`
    /// , whose rules do not apply to this one.
    /// (the description's meshes)
    Mesh(usize),
    /// A document's mesh ('s arrays, drawn by): index into
    /// `Scene::stored_meshes`.. (Meshes)
    StoredMesh(usize),
    /// A terrain (a height field drawn as a relief): index into `Scene::terrains`.
    /// . (Drawing a terrain)
    Terrain(usize),
    /// A drawn path : index into `Scene::paths`. Its answer is not held to the range.
    /// . (Drawn paths)
    Path(usize),
    /// A combined group : index into `Scene::groups.combinations`. Its placement is
    /// not read: its members carry their own..
    /// (Combined groups)
    Group(usize),
    /// A light's own object : its kind's shape, met only by the rays names, with
    /// the endless spot light's long cone.
    Light(light::LightObject),
    /// A tree object : index into `Scene::trees` and the geometry object that a hit on
    /// one of its leaves is reported for (`leaves`, a `TreeLeaves` object). A hit on the wood is
    /// reported for this object, which stands in for the stand-in of (one per tree; a
    /// scene of one tree or of trees that share no wood material cannot tell them apart).
    /// . (The skins)
    Tree { model: usize, leaves: usize },
    /// The object a hit on a leaf of stored shape `shape` of tree model `model` is reported for
    /// (the shape object): never met itself, it holds the tree's leaves' material and encloses a
    /// volume as a mesh does.. (The skins),
    TreeLeaves { model: usize, shape: usize },
    /// A metaball : index into `Scene::metaballs`'s balls. It is listed in the structure
    /// by its box and asked through the metaballs' own walk; a hit is reported for `MetaSet`.
    Metaball(usize),
    /// The one object that stands for every set of metaballs : its map is the
    /// identity, it is never listed or asked, and every hit on a set is reported for it.
    MetaSet,
}

impl Shape {
    /// The shape a solid-reading rule sees: a light's object reads as the plain shape of its
    /// kind (the sphere or the cone); every other shape as itself.
    pub fn plain(&self) -> Shape {
        match self {
            Shape::Light(l) => l.plain(),
            other => *other,
        }
    }
}

/// What shapes need from outside the scene's objects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeContext {
    pub version: RenderAs,
    /// The substrate table `pyramid`: the up component, then the outward component of a
    /// side's unit normal.
    pub pyramid_normal: [f64; 2],
    /// Whether the ray asked is a shadow ray : a light's own object is not met by one
    /// and its list's marks read it.
    pub shadow: bool,
}

/// An object-to-world map: `world = local · rows + translation` (row vectors), with its
/// inverse. (placement convention)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    rows: [Vec3; 3],
    translation: Vec3,
    inverse: [Vec3; 3],
    /// The inverse map's translation, by version: `[0]` as 3.0, `[1]` as 3.1, `[2]` as 5.0
    /// (`inverse_translation_of`). A ray is carried as o·M⁻¹ + t', d·M⁻¹ (`to_local_as`).
    /// (placement convention: the inverse's translation)
    inverse_translation: [Vec3; 3],
    /// The identity map: carrying a point (`point_to_local`, `to_world` for a finite point) or a
    /// ray by `to_local` is then exact and is skipped. A ray asked of a shape (`to_local_as`) is
    /// carried all the same ('s disc reads the zeros it makes). A normal is still carried by
    /// the cofactor inverse, whose signed zeros the torus's normal keeps (
    /// , the normal).
    /// (placement convention)
    identity: bool,
    /// The 3 x 3 part's determinant is 0: the inverse is formed all the same (k an infinity with
    /// the zero's sign, the numbers infinities or not numbers).
    singular: bool,
}

pub(crate) fn row_times(v: Vec3, m: &[Vec3; 3]) -> Vec3 {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2],
    ]
}


/// The inverse of the 3 x 3 part: the adjugate's nine entries, each a difference of two
/// products, each multiplied by k, the reciprocal of the determinant formed once; with the
/// adjugate and k.
/// (the inverse map)
fn adjugate(m: &[Vec3; 3]) -> Option<([f64; 9], f64)> {
    let [m0, m1, m2] = m[0];
    let [m3, m4, m5] = m[1];
    let [m6, m7, m8] = m[2];
    let det = ((m8 * m4 - m7 * m5) * m0 - (m8 * m3 - m5 * m6) * m1) + (m7 * m3 - m6 * m4) * m2;
    // : a determinant of 0 is not tested; k = 1 / det is then an infinity
    // with the zero's sign.
    if !det.is_finite() {
        return None;
    }
    let a = [
        m8 * m4 - m5 * m7,
        -(m8 * m1 - m2 * m7),
        m5 * m1 - m4 * m2,
        -(m8 * m3 - m5 * m6),
        m8 * m0 - m6 * m2,
        -(m5 * m0 - m2 * m3),
        m7 * m3 - m6 * m4,
        -(m7 * m0 - m1 * m6),
        m4 * m0 - m3 * m1,
    ];
    Some((a, 1.0 / det))
}

/// The inverse map's translation: the 4 x 4 cofactors of the affine map, each six products
/// of three numbers (three added, three taken away), multiplied by k; the grouping is
/// by version., `which`: 0 = 3.0,
/// 1 = 3.1, 2 = 5.0. (the inverse map)
fn inverse_translation_of(m: &[Vec3; 3], t: Vec3, k: f64, which: usize) -> Vec3 {
    let [m0, m1, m2] = m[0];
    let [m3, m4, m5] = m[1];
    let [m6, m7, m8] = m[2];
    let [m9, m10, m11] = t;
    let (t0, t1, t2) = match which {
        0 => (
            -(((((((m9 * m8) * m4) + ((m11 * m7) * m3)) + ((m10 * m5) * m6)) - ((m9 * m7) * m5)) - ((m11 * m6) * m4))
                - ((m10 * m8) * m3)),
            ((((((m10 * m2) * m6) + ((m9 * m8) * m1)) + ((m11 * m7) * m0)) - ((m9 * m7) * m2)) - ((m11 * m6) * m1))
                - ((m10 * m8) * m0),
            -(((((((m9 * m5) * m1) + ((m11 * m4) * m0)) + ((m10 * m2) * m3)) - ((m9 * m4) * m2)) - ((m11 * m3) * m1))
                - ((m10 * m5) * m0)),
        ),
        1 => (
            -(((((((m10 * m5) * m6) + ((m9 * m8) * m4)) + ((m11 * m7) * m3)) - ((m9 * m7) * m5)) - ((m11 * m6) * m4))
                - ((m10 * m8) * m3)),
            ((((((m9 * m8) * m1) + ((m11 * m7) * m0)) + ((m10 * m2) * m6)) - ((m9 * m7) * m2)) - ((m11 * m6) * m1))
                - ((m10 * m8) * m0),
            -(((((((m11 * m4) * m0) + ((m10 * m2) * m3)) + ((m9 * m5) * m1)) - ((m9 * m4) * m2)) - ((m11 * m3) * m1))
                - ((m10 * m5) * m0)),
        ),
        _ => (
            -(((((((m4 * m8) * m9) + ((m5 * m6) * m10)) + ((m3 * m7) * m11)) - ((m5 * m7) * m9)) - ((m4 * m6) * m11))
                - ((m3 * m8) * m10)),
            ((((((m1 * m8) * m9) + ((m2 * m6) * m10)) + ((m0 * m7) * m11)) - ((m2 * m7) * m9)) - ((m1 * m6) * m11))
                - ((m0 * m8) * m10),
            -(((((((m1 * m5) * m9) + ((m2 * m3) * m10)) + ((m0 * m4) * m11)) - ((m2 * m4) * m9)) - ((m1 * m3) * m11))
                - ((m0 * m5) * m10)),
        ),
    };
    [t0 * k, t1 * k, t2 * k]
}

/// The twelve numbers of a matrix's inverse map as `version` forms them : the
/// three rows of the 3 x 3 part, then the translation. `None` when singular.
pub fn inverse_map(rows: &[Vec3; 3], translation: Vec3, version: RenderAs) -> Option<[f64; 12]> {
    let (a, k) = adjugate(rows)?;
    let which = match version {
        RenderAs::Bryce30 => 0,
        RenderAs::Bryce31 => 1,
        RenderAs::Bryce50 => 2,
    };
    let t = inverse_translation_of(rows, translation, k, which);
    let mut out = [0.0; 12];
    for i in 0..9 {
        out[i] = a[i] * k;
    }
    out[9..].copy_from_slice(&t);
    Some(out)
}

#[cfg(test)]
fn cofactor_inverse(m: &[Vec3; 3]) -> Option<[Vec3; 3]> {
    let (a, k) = adjugate(m)?;
    Some([[a[0] * k, a[1] * k, a[2] * k], [a[3] * k, a[4] * k, a[5] * k], [a[6] * k, a[7] * k, a[8] * k]])
}


/// The cofactor inverse, with the cofactors `c` (`c[i][j]`, the adjugate's `[j][i]`) and
/// 1 / det that make it.
pub(crate) fn cofactors(m: &[Vec3; 3]) -> Option<([Vec3; 3], [Vec3; 3], f64)> {
    let others = |k: usize| match k {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let mut c = [[0.0; 3]; 3];
    for i in 0..3 {
        let (a, b) = others(i);
        for j in 0..3 {
            let (cc, d) = others(j);
            let minor = m[a][cc] * m[b][d] - m[a][d] * m[b][cc];
            c[i][j] = if (i + j) % 2 == 1 { -minor } else { minor };
        }
    }
    let det = dot(m[0], c[0]);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let inv = 1.0 / det;
    Some((
        [
            [c[0][0] * inv, c[1][0] * inv, c[2][0] * inv],
            [c[0][1] * inv, c[1][1] * inv, c[2][1] * inv],
            [c[0][2] * inv, c[1][2] * inv, c[2][2] * inv],
        ],
        c,
        inv,
    ))
}

const IDENTITY_ROWS: [Vec3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// `cofactor_inverse(IDENTITY_ROWS)`, bit for bit (checked by a unit test).
const IDENTITY_INVERSE: [Vec3; 3] = [[1.0, -0.0, 0.0], [-0.0, 1.0, -0.0], [0.0, -0.0, 1.0]];

impl Placement {
    pub const IDENTITY: Placement =
        Placement {
            rows: IDENTITY_ROWS,
            translation: [0.0; 3],
            inverse: IDENTITY_INVERSE,
            inverse_translation: [[0.0; 3]; 3],
            identity: true,
            singular: false,
        };

    /// Builds the placement from the document's three rows and translation. `None` when the
    /// rows are singular (no inverse).
    pub fn new(rows: [Vec3; 3], translation: Vec3) -> Option<Placement> {
        if rows == IDENTITY_ROWS && translation == [0.0; 3] {
            return Some(Placement::IDENTITY);
        }
        Placement::general_any(rows, translation)
    }

    /// The placement built whether or not it is the identity, so that carrying a point is
    /// always the arithmetic of. Used by the trees' maps.
    pub(crate) fn general(rows: [Vec3; 3], translation: Vec3) -> Option<Placement> {
        Placement::general_any(rows, translation).filter(|p| !p.singular)
    }

    /// `general` for any matrix whose determinant is a number: a determinant of 0 is inverted as
    /// any other, with no test.
    fn general_any(rows: [Vec3; 3], translation: Vec3) -> Option<Placement> {
        let (a, k) = adjugate(&rows)?;
        let inverse = [[a[0] * k, a[1] * k, a[2] * k], [a[3] * k, a[4] * k, a[5] * k], [a[6] * k, a[7] * k, a[8] * k]];
        let inverse_translation = std::array::from_fn(|w| inverse_translation_of(&rows, translation, k, w));
        Some(Placement { rows, translation, inverse, inverse_translation, identity: false, singular: k.is_infinite() })
    }

    /// Whether the 3 x 3 part's determinant is 0 (k an infinity): the map's inverse carries a
    /// point to numbers that are not numbers.
    pub fn is_singular(&self) -> bool {
        self.singular
    }

    /// The three rows as the document gives them.
    pub fn rows(&self) -> [Vec3; 3] {
        self.rows
    }

    /// The translation.
    pub fn translation(&self) -> Vec3 {
        self.translation
    }

    /// The inverse's three rows (the cofactor inverse; `IDENTITY_INVERSE` for the identity).
    ///
    pub fn inverse_rows(&self) -> [Vec3; 3] {
        self.inverse
    }

    pub fn to_world(&self, p: Vec3) -> Vec3 {
        // : an infinite coordinate multiplied by the
        // identity's zeros makes the others not numbers, so only a finite point is shortcut.
        if self.identity && p.iter().all(|c| c.is_finite()) {
            return p;
        }
        let q = row_times(p, &self.rows);
        [q[0] + self.translation[0], q[1] + self.translation[1], q[2] + self.translation[2]]
    }

    /// The ray in local space; the direction keeps its length as carried.
    pub fn to_local(&self, r: &Ray) -> Ray {
        if self.identity {
            return *r;
        }
        let o = [r.origin[0] - self.translation[0], r.origin[1] - self.translation[1], r.origin[2] - self.translation[2]];
        Ray { origin: row_times(o, &self.inverse), dir: row_times(r.dir, &self.inverse) }
    }

    /// The ray in local space as `version` carries it: o·M⁻¹ + t' and d·M⁻¹, M⁻¹ and t' the
    /// inverse map's (`inverse_translation_of`); the direction keeps its length as carried.
    /// The identity is carried too, by its cofactor inverse with its signed zeros: the shape's
    /// own-space ray is that arithmetic's, whose zeros's disc reads (flat-zero-signs.jsonl).
    pub fn to_local_as(&self, version: RenderAs, r: &Ray) -> Ray {
        let t = self.inverse_translation[match version {
            RenderAs::Bryce30 => 0,
            RenderAs::Bryce31 => 1,
            RenderAs::Bryce50 => 2,
        }];
        let o = row_times(r.origin, &self.inverse);
        Ray { origin: [o[0] + t[0], o[1] + t[1], o[2] + t[2]], dir: row_times(r.dir, &self.inverse) }
    }

    /// A world point in the shape's own space: carried by the inverse of the object's matrix
    /// , as a ray's origin is in `to_local`.; (the eye
    /// carried into the shape's own space)
    pub fn point_to_local(&self, p: Vec3) -> Vec3 {
        if self.identity {
            return p;
        }
        let o = [p[0] - self.translation[0], p[1] - self.translation[1], p[2] - self.translation[2]];
        row_times(o, &self.inverse)
    }

    /// A local normal carried to world space by the inverse transpose, then brought to unit
    /// length as every shape's normal is (`unit_as`).;
    pub fn normal_to_world(&self, n: Vec3, version: RenderAs) -> Vec3 {
        unit_as(version, self.carry_normal(n, version))
    }

    /// A local normal carried to world space by the transpose of the inverse's 3 x 3 part, not
    /// yet brought to unit length: coordinate j sums n_x i(3j), n_y i(3j + 1), n_z i(3j + 2) as
    /// (y + z) + x, as 3.0 and 3.1 for every j and as 5.0 for j = 0; as 5.0 coordinates 1 and 2
    /// as (x + y) + z. (Outputs, Variants: the carrying of a normal, revised log
    /// 16837),
    pub fn carry_normal(&self, n: Vec3, version: RenderAs) -> Vec3 {
        let m = &self.inverse;
        let yz_first = |r: Vec3| (n[1] * r[1] + n[2] * r[2]) + n[0] * r[0];
        let xy_first = |r: Vec3| (n[0] * r[0] + n[1] * r[1]) + n[2] * r[2];
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => [yz_first(m[0]), yz_first(m[1]), yz_first(m[2])],
            RenderAs::Bryce50 => [yz_first(m[0]), xy_first(m[1]), xy_first(m[2])],
        }
    }
}

/// One object of the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Object {
    pub shape: Shape,
    pub placement: Placement,
}

/// Which part of a shape a hit is on: it chooses the texture coordinates' rule.
/// (texture coordinates)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// A flat face across `axis` (plane, slab, square, disc, a cube face, the pyramid's base).
    Face { axis: usize },
    /// A round end: the cylinder's caps, the cone's base.
    Cap,
    /// The curved surface: the sphere, the torus, the cylinder's or the cone's wall.
    Curved,
    /// A side of the pyramid, facing along `axis` (0: x, 2: z).
    Side { axis: usize },
    /// The triangle of a document's mesh, by its number.
    Triangle { index: usize },
    /// A terrain's triangle or closing face.
    Relief(ReliefPart),
    /// A combined group's hit: the crossing of geometry object `surface` (its point and normal
    /// in that object's own space, `order` the grouping of's last step on the normal,
    /// as its list gives them), showing the material of geometry object `material`.
    /// . (Combined groups)
    Grouped { surface: usize, material: usize, carried: usize, order: SumOrder },
    /// A tree's hit: its point and normal are already the world's (unit length), and the hit is
    /// reported for geometry object `object` (the tree's own object for the wood, the leaves'
    /// object for a leaf).. (The skins)
    Reported { object: usize },
}

/// Texture coordinates (u, v) and their two directions du, dv, in the shape's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Texture {
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
}

/// A shape's nearest hit in its own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalHit {
    pub t: f64,
    /// The point in the shape's own space (on a flat face, its coordinate across the face is
    /// the face's own value).
    pub point: Vec3,
    /// The shape's normal before brings it to unit length in world space. Flat faces
    /// of the cube, slab and pyramid base give the positive axis .
    pub normal: Vec3,
    /// The part of the shape met.
    pub part: Part,
    /// u, v, du, dv of the hit , filled once the nearest crossing is chosen.
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
    /// The shading normal in the shape's own space where it is not the normal: a document
    /// mesh's m' (kinds 1 and 2), a terrain's shading normal , of
    /// unit length; `None` for every other hit.
    pub shade: Option<Vec3>,
    /// A hit on a set of metaballs: the number of what the hit shows in the scene's record
    /// (`metaball::MetaScene::showing`).
    pub meta: Option<u32>,
}

impl LocalHit {
    /// A crossing whose texture coordinates are not yet computed (zeros).
    fn new(t: f64, point: Vec3, normal: Vec3, part: Part) -> LocalHit {
        LocalHit { t, point, normal, part, uv: [0.0; 2], du: [0.0; 3], dv: [0.0; 3], shade: None, meta: None }
    }
}

/// What a ray meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f64,
    pub point: Vec3,
    pub object: usize,
    /// Unit world normal, as the shape reports it (see `LocalHit::normal`).
    pub normal: Vec3,
    /// Unit world normal that shading reads ('s n before any bump): a document mesh's
    /// m' carried and brought to unit length as the normal is ; for every other
    /// shape, and a mesh of kind 3, the same as `normal`.
    pub shading_normal: Vec3,
    /// Whether the ray runs against `normal` (the side the normal points to).
    pub from_outside: bool,
    /// The point in the object's own space (texture coordinates derive from it).
    pub local_point: Vec3,
    /// Texture coordinates u, v and their directions du, dv, in the object's own space.
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
    /// A hit on a set of metaballs (reported for `Shape::MetaSet`): what it shows.
    pub meta: Option<u32>,
    /// The object whose space a texture is placed by, where it is not `object` (the cut of a
    /// negative member with bit 0x80 shows the carried member's material, `object`, with the
    /// texture placed by the surface member). `local_point`, `uv`, `du` and `dv` are in that
    /// object's space.
    pub texture_object: Option<usize>,
    /// A combined group's hit: the member whose surface it is and the member a ray that goes
    /// in through it travels in the material of as 5.0 (`GroupEntry::carried`); as 3.0 and
    /// 3.1 the first.
    pub group_members: Option<(usize, usize)>,
}

/// One triangle of a mesh, in the mesh's own space (gives the
/// description's triangles in the world's space, so the placement of a mesh object is the
/// identity). `normal`, when given, is the shading normal; else the face's own.
/// (the description's meshes)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub corners: [Vec3; 3],
    pub normal: Option<Vec3>,
}

/// A mesh: a triangle soup, the Blender description's shape (not a document's mesh, which
/// is `StoredMesh`). (the description's meshes)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub triangles: Vec<Triangle>,
    /// The bounding-volume tree over `triangles`, built on the first ray. Make a mesh with
    /// `Mesh::new`; a mesh whose `triangles` change after a ray was asked must be made again.
    pub accel: mesh_bvh::Accel,
}

impl Mesh {
    pub fn new(triangles: Vec<Triangle>) -> Mesh {
        Mesh { triangles, accel: Default::default() }
    }
}

/// The scene's objects in document order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub objects: Vec<Object>,
    /// The meshes `Shape::Mesh` indexes. Empty for every document.
    pub meshes: Vec<Mesh>,
    /// The document meshes `Shape::StoredMesh` indexes.
    pub stored_meshes: Vec<StoredMesh>,
    /// The terrains `Shape::Terrain` indexes, each as the render's version draws it.
    pub terrains: Vec<Relief>,
    /// The drawn paths `Shape::Path` indexes.
    pub paths: Vec<path::PathChords>,
    /// The stored trees `Shape::Tree` indexes..
    pub trees: Vec<tree::TreeModel>,
    /// The walk that decides which object a ray reports ; `None`: every object in
    /// list order, the earlier on equal t ('s search, used where a scene is not a
    /// render's)..
    pub structure: Option<structure::Structure>,
    /// The combined groups `Shape::Group` indexes, and the objects drawn only through them.
    /// . (Combined groups)
    pub groups: Groups,
    /// The scene's metaballs , if it has any. (Metaballs)
    pub metaballs: Option<metaball::MetaScene>,
}

impl Scene {
    /// The nearest hit in `range`; on equal distance the earlier object wins.
    /// . (ties)
    pub fn intersect(&self, ray: &Ray, range: Range, ctx: &ShapeContext) -> Option<Hit> {
        if let Some(s) = &self.structure {
            // : the object the walk reaches first on a tie.
            return s.walk(self, ray, range, ctx).map(|(i, h)| {
                let mut hit = self.finish(ray, i, h, ctx);
                // : a hit on the wood is shaded with the material the
                // stand-in holds after the last ask of the ray, as the latest hand-over left it
                // (a later tree that shows the same stored tree can hand it another wood material).
                if let (Some(ms), Some(id)) = (&self.metaballs, hit.meta)
                    && hit.object == ms.stand
                    && matches!(ms.showing(id), metaball::Showing::Wood(_))
                    && let Some(latest) = ms.latest_id()
                {
                    hit.meta = Some(latest);
                }
                hit
            });
        }
        let mut best: Option<(usize, LocalHit)> = None;
        let mut limit = range;
        for i in 0..self.objects.len() {
            // : a member that takes part is met only through its group.
            if self.groups.is_through(i) {
                continue;
            }
            let met = self.ask(i, ray, limit, ctx);
            if let Some(h) = met {
                // : on equal t the earlier object is kept; only
                // the disc's crossing at +infinity  can equal the far end it was given.
                if best.is_some() && h.t == limit.far {
                    continue;
                }
                limit.far = h.t;
                best = Some((i, h));
            }
        }
        best.map(|(i, h)| self.finish(ray, i, h, ctx))
    }

    /// Object `i`'s nearest crossing of the world ray in `range`, by each shape's own rule
    /// for the range (open for the primitives,'s closed range for a document mesh).
    pub(crate) fn ask(&self, i: usize, ray: &Ray, range: Range, ctx: &ShapeContext) -> Option<LocalHit> {
        let obj = &self.objects[i];
        let local = obj.placement.to_local_as(ctx.version, ray);
        match obj.shape {
            Shape::Mesh(k) => self.meshes.get(k).and_then(|m| intersect_mesh(m, &local, range, ctx)),
            Shape::StoredMesh(k) => {
                self.stored_meshes.get(k).and_then(|m| mesh_hit(m, &local, range, ray.dir, ctx.version))
            }
            Shape::Terrain(k) => {
                let t = self.terrains.get(k)?;
                // : the scene's test of the terrain's world box comes first.
                if !t.world_box_met(ray, range) {
                    return None;
                }
                t.hit(&local, range)
            }
            // : the first chord met, whatever its t (the range is not asked); the
            // normal is the local ray's -d and the surface coordinates are 0.
            Shape::Path(k) => self.paths.get(k).and_then(|p| p.hit(local.origin, local.dir)).map(|h| {
                let d = local.dir;
                LocalHit::new(h.t, h.point, [-d[0], -d[1], -d[2]], Part::Curved)
            }),
            // : the first entry of the group's list that shows, in the range.
            Shape::Group(k) => self.group_hit(k, ray, range, ctx).map(|e| LocalHit {
                part: Part::Grouped { surface: e.surface, material: e.material, carried: e.carried, order: e.crossing.unit_order },
                ..e.crossing.hit
            }),
            // : a metaball is asked through the walk (`structure`), which keeps the
            // marks and the kept crossing of the ray; the one object that stands for the sets is
            // never asked.
            Shape::Metaball(_) | Shape::MetaSet => None,
            // : a lone ask with no marks held (a scene without a walk).
            Shape::Tree { model, .. } => {
                let mut marks = tree::RayMarks::new();
                self.ask_tree(i, model, &mut marks, ray, range.near, range.far, range.far).0
            }
            shape => intersect_shape(shape, &local, range, ctx),
        }
    }

    /// Object `i`, a tree of `Scene::trees[model]`, asked with the lower bound `lo`, the leaf
    /// bound `bl` and the far end `far`: the hit as a local hit, and the far end after the ask.
    /// . (The skins)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ask_tree(
        &self,
        i: usize,
        model: usize,
        marks: &mut tree::RayMarks,
        ray: &Ray,
        lo: f64,
        bl: f64,
        far: f64,
    ) -> (Option<LocalHit>, f64) {
        let Shape::Tree { leaves, .. } = self.objects[i].shape else { return (None, far) };
        let asked = self.trees[model].ask(&self.objects[i].placement, marks, ray, lo, bl, far, &tree::Orders::VECTORS);
        // : where the scene holds metaballs the stand-in is the one
        // object of the sets: a taken crossing hands it this tree's wood material, and a hit on
        // the wood is reported for it, naming the showing it was handed.
        if asked.crossing_taken {
            if let Some(ms) = &self.metaballs {
                // : the pieces belong to the stored tree and carry the
                // wood material of the tree read last of those that show it.
                let mut carrier = i;
                if let Some(stored) = self.trees[model].stored() {
                    for (k, o) in self.objects.iter().enumerate() {
                        if let Shape::Tree { model: m, .. } = o.shape
                            && self.trees[m].stored() == Some(stored)
                        {
                            carrier = k;
                        }
                    }
                }
                ms.hand_over_wood(carrier);
            }
        }
        let hit = asked.hit.map(|h| {
            let mut meta = None;
            let object = match h.on {
                tree::Surface::Wood => match &self.metaballs {
                    Some(ms) => {
                        meta = Some(ms.record_wood_hit(i));
                        ms.stand
                    }
                    None => i,
                },
                // (Fuzzy wood and fuzzy leaves): one shape object per
                // leaf shape, consecutive from the first.
                tree::Surface::Leaf { shape } => leaves + shape,
            };
            LocalHit {
                t: h.t,
                point: h.point,
                normal: h.normal,
                part: Part::Reported { object },
                uv: h.uv,
                du: h.du,
                dv: h.dv,
                shade: None,
                meta,
            }
        });
        (hit, asked.far_end)
    }

    /// The world hit of object `i`'s local hit.,
    fn finish(&self, ray: &Ray, i: usize, h: LocalHit, ctx: &ShapeContext) -> Hit {
        // : a group's hit is its member's crossing record, carried as's list
        // carries it, and reported on the member whose material shows.
        // (Combined groups)
        if let Part::Grouped { surface, material, carried, order } = h.part {
            let s = &self.objects[surface];
            // : a mesh's or a terrain's crossing carries its shading
            // normal; each normal is carried as's and's hits are, with the lists'
            // own orders of the squares (Accuracy, step 2).
            let (normal, shading_normal) = match s.shape {
                Shape::StoredMesh(_) => mesh::world_normals(&s.placement, &h, ctx.version, &SumOrders::of_list(ctx.version)),
                Shape::Terrain(_) => {
                    let n = relief::world_normal_listed(&s.placement, h.normal, ctx.version);
                    (n, h.shade.map_or(n, |m| relief::world_normal_listed(&s.placement, m, ctx.version)))
                }
                _ => {
                    let n = order.unit(ctx.version, s.placement.carry_normal(h.normal, ctx.version));
                    (n, n)
                }
            };
            // : a kind 1 crossing's du is the tangent of its world
            // shading normal against the world ray (dv marks the crossing).
            let du = if matches!(s.shape, Shape::StoredMesh(_)) && ctx.version == RenderAs::Bryce50 && h.dv == mesh::LIST_KIND1_DV {
                mesh::list_tangent(shading_normal, ray.dir)
            } else {
                h.du
            };
            return Hit {
                t: h.t,
                point: crossings::carry_point(s.shape, &s.placement, h.point),
                object: material,
                normal,
                shading_normal,
                from_outside: dot(ray.dir, normal) < 0.0,
                local_point: h.point,
                uv: h.uv,
                du,
                dv: h.dv,
                meta: None,
                texture_object: (surface != material).then_some(surface),
                group_members: Some((surface, carried)),
            };
        }
        // : a hit on a set is reported for the one object that stands for every
        // set, whose map is the identity (the hit's point in its own space is the world point);
        // its normal is the crossing's, not turned toward the ray.
        if let Shape::Metaball(_) = self.objects[i].shape {
            let stand = self.metaballs.as_ref().map_or(i, |m| m.stand);
            return Hit {
                t: h.t,
                point: h.point,
                object: stand,
                normal: h.normal,
                shading_normal: h.normal,
                from_outside: dot(ray.dir, h.normal) < 0.0,
                local_point: h.point,
                uv: [0.0; 2],
                du: [0.0; 3],
                dv: [0.0; 3],
                meta: h.meta,
                texture_object: None,
                group_members: None,
            };
        }
        // : a tree's hit comes in the world already.
        if let Part::Reported { object } = h.part {
            // The point in the object's own space, which the texture's object space mapping reads , is the
            // point in the tree's space (before the tree's placement): the original records the hit's world point
            // and, beside it, the tree-space point it found (Trees). The wood
            // and the leaves are reported for objects whose map is the identity, but this record is not that map.
            let tree = &self.objects[i].placement;
            let own = crate::shading::fuzzy::to_own_space(&tree.inverse_rows(), tree.translation(), h.point);
            return Hit {
                t: h.t,
                point: h.point,
                object,
                normal: h.normal,
                shading_normal: h.normal,
                from_outside: dot(ray.dir, h.normal) < 0.0,
                local_point: own,
                uv: h.uv,
                du: h.du,
                dv: h.dv,
                meta: h.meta,
                texture_object: None,
                group_members: None,
            };
        }
        let obj = &self.objects[i];
        let (normal, shading_normal) = match obj.shape {
            // : P and m' carried and brought to unit length (its orders).
            Shape::StoredMesh(_) => mesh::world_normals(&obj.placement, &h, ctx.version, &SumOrders::of(ctx.version)),
            // : the normal and the shading normal each carried as.
            Shape::Terrain(_) => {
                let n = relief::world_normal(&obj.placement, h.normal, ctx.version);
                let s = h.shade.map_or(n, |s| relief::world_normal(&obj.placement, s, ctx.version));
                (n, s)
            }
            _ => {
                let n = obj.placement.normal_to_world(h.normal, ctx.version);
                (n, n)
            }
        };
        Hit {
            t: h.t,
            point: obj.placement.to_world(h.point),
            object: i,
            normal,
            shading_normal,
            from_outside: dot(ray.dir, normal) < 0.0,
            local_point: h.point,
            uv: h.uv,
            du: h.du,
            dv: h.dv,
            meta: None,
            texture_object: None,
            group_members: None,
        }
    }
}

impl Hit {
    /// The geometry object whose material is the medium of a ray that goes in through this
    /// hit's surface: the object itself, except at a combined group's hit, where it is the
    /// member whose surface it is (as 3.0 and 3.1) or the member carried to the crossing (as
    /// 5.0).
    pub fn medium_object(&self, version: crate::version::RenderAs) -> usize {
        match self.group_members {
            None => self.object,
            Some((surface, carried)) => {
                if version == crate::version::RenderAs::Bryce50 { carried } else { surface }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Arithmetic shared by the shapes. (rules the vectors show)
// ---------------------------------------------------------------------------------------

/// `(a0·b0 + a1·b1) + a2·b2`.
fn dot_left(a: Vec3, b: Vec3) -> f64 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// `a0·b0 + (a1·b1 + a2·b2)`.
fn dot_right(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + (a[1] * b[1] + a[2] * b[2])
}

/// Real roots of c3 s³ + c2 s² + c1 s + c0 by's cubic solver (Schwarze's form of
/// Cardano's formula), the largest first when there are three.;
pub fn cubic_roots(version: RenderAs, c: [f64; 4]) -> Vec<f64> {
    roots::cubic(version, c)
}

/// `v · q(v·v)`, with q the version's reciprocal square root  and the sum taken
/// `x² + (y² + z²)`.'s last step on every normal.
pub fn unit_as(version: RenderAs, v: Vec3) -> Vec3 {
    let q = rsqrt_as(version, dot_right(v, v));
    [v[0] * q, v[1] * q, v[2] * q]
}

/// `a · b` where a 0 times an infinity is the x87 indefinite value.
fn mul_x87(a: f64, b: f64) -> f64 {
    let p = a * b;
    if p.is_nan() && !a.is_nan() && !b.is_nan() { X87_INDEFINITE } else { p }
}

/// Grazing limit of the plane and the slab.
fn plane_limit(v: RenderAs) -> f64 {
    match v {
        RenderAs::Bryce30 => 0.0001,
        RenderAs::Bryce31 | RenderAs::Bryce50 => 0.00001,
    }
}

/// Grazing limit of the square and the disc, all versions (a magnitude equal to it misses);
/// also the cylinder's caps and the cone's base.
const FLAT_PIECE_LIMIT: f64 = 0.00001;

/// As 3.0 and 3.1 the square and the disc meet only crossings below this, the double nearest
/// 1.7e308.
const FLAT_PIECE_FAR_BOUND_3X: f64 = f64::from_bits(0x7fee_42d1_3077_3b76);

/// A cube's faces across an axis, the pyramid's base and its sides are met only where the
/// direction's component (or its dot product with the side's normal) is above this in
/// magnitude.
const SOLID_FACE_LIMIT: f64 = 1e-6;

/// A wall's quadratic has no roots where |a| is below this.
const WALL_A_LIMIT: f64 = 1e-10;

/// As Bryce 5, a discriminant above minus this and below 0 is taken as 0.
const WALL_DISC_BAND: f64 = 1e-10;

/// The roots of the wall's quadratic a t² + b t + c, ascending.
///
/// : no roots where |a| < 1e-10; as 3.0 and 3.1 roots only where b² is above 4ac;
/// as 5.0 a discriminant in (-1e-10, 0) is taken as 0, and none at or below -1e-10 or NaN;
/// the roots are (-b ∓ √(b² - 4ac)) · k with k = 1 / (2a) rounded once.
fn wall_roots(v: RenderAs, a: f64, b: f64, c: f64) -> Option<[f64; 2]> {
    if !(a.abs() >= WALL_A_LIMIT) {
        return None;
    }
    let bb = b * b;
    let four_ac = 4.0 * a * c;
    let disc = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => {
            if !(bb > four_ac) {
                return None;
            }
            bb - four_ac
        }
        RenderAs::Bryce50 => {
            let d = bb - four_ac;
            if d >= 0.0 {
                d
            } else if d > -WALL_DISC_BAND {
                0.0
            } else {
                return None; // at or below -1e-10, or not a number
            }
        }
    };
    let s = sqrt_as(v, disc);
    let k = 1.0 / (2.0 * a);
    let (t0, t1) = ((-b - s) * k, (-b + s) * k);
    Some(if t0 <= t1 { [t0, t1] } else { [t1, t0] })
}

/// Keeps the nearer of two candidate hits; on equal t the first.
fn nearer(a: Option<LocalHit>, b: Option<LocalHit>) -> Option<LocalHit> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if y.t < x.t { y } else { x }),
        (x, None) => x,
        (None, y) => y,
    }
}

/// The ray against one shape in its own space, with the hit's texture coordinates.
/// The nearest crossing of a mesh: each triangle tested by the Möller–Trumbore rule, the
/// nearest inside `range` kept. Both sides of a triangle meet the ray, and the normal is
/// turned to face the ray (a mesh's winding is the modeller's, not ours
/// blender-addon.md). Texture coordinates are zeros: a description material is plain .
/// (the description's meshes)
fn intersect_mesh(mesh: &Mesh, r: &Ray, range: Range, _ctx: &ShapeContext) -> Option<LocalHit> {
    // The nearest accepted t wins, the earliest triangle on equal t (each later triangle must be
    // strictly nearer than the best so far): the tree finds that same triangle without testing
    // all of them. (Speed on mesh scenes)
    let tris = &mesh.triangles;
    let probe = if tris.len() > 8 { mesh_bvh::Probe::new(r.origin, r.dir) } else { None };
    let (i, t) = match probe {
        Some(p) => {
            mesh.accel.get(tris).nearest(&p, range.near, range.far, |i| triangle_t(&tris[i], r, range))?
        }
        None => {
            let mut best: Option<(usize, f64)> = None;
            let mut limit = range;
            for (i, tri) in tris.iter().enumerate() {
                if let Some(t) = triangle_t(tri, r, limit) {
                    limit.far = t;
                    best = Some((i, t));
                }
            }
            best?
        }
    };
    let tri = &tris[i];
    let [a, b, c] = tri.corners;
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let face = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let n = tri.normal.unwrap_or(face);
    // turn the normal to face the ray (two-sided)
    let n = if dot(r.dir, n) > 0.0 { [-n[0], -n[1], -n[2]] } else { n };
    let point = r.at(t);
    Some(LocalHit::new(t, point, n, Part::Face { axis: 1 }))
}

/// The Möller-Trumbore `t` of one triangle if the ray meets it strictly inside `range`.
#[inline]
fn triangle_t(tri: &Triangle, r: &Ray, range: Range) -> Option<f64> {
    let [a, b, c] = tri.corners;
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let p = [
        r.dir[1] * e2[2] - r.dir[2] * e2[1],
        r.dir[2] * e2[0] - r.dir[0] * e2[2],
        r.dir[0] * e2[1] - r.dir[1] * e2[0],
    ];
    let det = dot(e1, p);
    if det == 0.0 || !det.is_finite() {
        return None; // parallel, or a degenerate triangle
    }
    let inv = 1.0 / det;
    let s = [r.origin[0] - a[0], r.origin[1] - a[1], r.origin[2] - a[2]];
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = [
        s[1] * e1[2] - s[2] * e1[1],
        s[2] * e1[0] - s[0] * e1[2],
        s[0] * e1[1] - s[1] * e1[0],
    ];
    let v = dot(r.dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    if !(t > range.near && t < range.far) || !t.is_finite() {
        return None;
    }
    Some(t)
}

pub fn intersect_shape(shape: Shape, r: &Ray, range: Range, ctx: &ShapeContext) -> Option<LocalHit> {
    crossing(shape, r, range, ctx).map(|mut h| {
        let tex = texture_on(shape, h.part, h.point, r, h.t, ctx.version);
        (h.uv, h.du, h.dv) = (tex.uv, tex.du, tex.dv);
        h
    })
}

/// The nearest crossing, its texture coordinates not yet computed.
fn crossing(shape: Shape, r: &Ray, range: Range, ctx: &ShapeContext) -> Option<LocalHit> {
    let v = ctx.version;
    match shape {
        // A mesh's triangles live in the scene's side table; `Scene::intersect` brings them.
        Shape::Mesh(_) | Shape::StoredMesh(_) | Shape::Terrain(_) | Shape::Path(_) => None,
        // A tree's hits are's (`Scene::ask`); the leaves' object is never met.
        Shape::Tree { .. } | Shape::TreeLeaves { .. } => None,
        // A combined group's hit is its list's : `Scene::ask` brings it.
        Shape::Group(_) => None,
        // A metaball is asked through the walk .
        Shape::Metaball(_) | Shape::MetaSet => None,
        // : a ray that meets a light's object meets the plain shape's hit.
        Shape::Light(l) => {
            if !l.met(ctx.shadow) {
                return None;
            }
            if l.endless() { endless_hit(l.plain(), r, range, ctx) } else { crossing(l.plain(), r, range, ctx) }
        }
        // : a magnitude equal to the limit meets them. ("The quotient
        // and the point"): the quotient has a wide exponent range, a crossing counts where
        // lo < T and T is below hi or unordered with it, whatever the range of the doubles.
        Shape::Plane => {
            if r.dir[1].abs() < plane_limit(v) {
                return None;
            }
            let f = plane_face(r, 0.0, FaceForm::Rounded);
            f.counts(range.near, range.far).then(|| f.hit())
        }
        // : the top face is reported where both are at the same t; the bottom face
        // counts when it is below the nearer of hi and the top face's t (as 5.0 by its quotient,
        // as 3.0 and 3.1 by its rounded t).
        Shape::Slab => {
            if r.dir[1].abs() < plane_limit(v) {
                return None;
            }
            let top = plane_face(r, 1.0, FaceForm::WideSum);
            let top_counts = top.counts(range.near, range.far);
            let bottom_form = if v == RenderAs::Bryce50 { FaceForm::WideSum } else { FaceForm::WideSumOfT };
            let bottom = plane_face(r, -1.0, bottom_form);
            let bound = if top_counts { top.t } else { range.far };
            let lo_ok = bottom.big_t.cmp_f64(range.near) == Some(std::cmp::Ordering::Greater);
            let below = if v == RenderAs::Bryce50 {
                !matches!(bottom.big_t.cmp_f64(bound), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
            } else {
                !(bottom.t >= bound)
            };
            if lo_ok && below {
                Some(bottom.hit())
            } else if top_counts {
                Some(top.hit())
            } else {
                None
            }
        }
        Shape::Square | Shape::Disc => {
            // : an origin whose y is infinite meets neither shape, in any version.
            if r.dir[1].abs() <= FLAT_PIECE_LIMIT || r.origin[1].is_infinite() {
                return None;
            }
            let h = match flat(r, 1, 0.0, range) {
                Some(h) => h,
                // : as 5.0 the disc meets a crossing at t = +infinity where hi
                // is +infinity (lo < t still required).
                None if shape == Shape::Disc && v == RenderAs::Bryce50 => crossing_at_infinity(r, 1, 0.0, range)?,
                None => return None,
            };
            // : a zero coordinate of the point is reported as +0,
            // in every version (`x + 0.0` turns -0 into +0 and leaves every other value).
            let h = LocalHit { point: h.point.map(|c| c + 0.0), ..h };
            // : as 3.0 and 3.1 the crossing must be below the double nearest
            // 1.7e308, whatever hi is.
            if matches!(v, RenderAs::Bryce30 | RenderAs::Bryce31) && !(h.t < FLAT_PIECE_FAR_BOUND_3X) {
                return None;
            }
            let [x, _, z] = h.point;
            let inside = if shape == Shape::Square {
                x.abs() <= 1.0 && z.abs() <= 1.0
            } else {
                x * x + z * z <= 1.0
            };
            inside.then_some(h)
        }
        Shape::Cube => cube(r, range),
        Shape::Sphere => sphere(r, range, v),
        Shape::Cylinder => cylinder(r, range, v),
        Shape::Cone => cone(r, range, v),
        Shape::Pyramid => pyramid(r, range, ctx.pyramid_normal),
        Shape::Torus { tube, ring } => torus(r, range, tube, ring, v),
    }
}

/// How the point of a plane's or a slab's face is formed from the quotient T.
/// ("The quotient and the point")
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaceForm {
    /// Each product d T rounded to a double first, then added to the origin: the plane, and the
    /// slab's faces in a list.
    Rounded,
    /// The product and the sum formed with the wide exponent range, only the sum rounded: the
    /// slab's hit, the top face, and the bottom face as 5.0.
    WideSum,
    /// As `WideSum` with T rounded to a double (t) in T's place: the slab's bottom face as 3.0
    /// and 3.1.
    WideSumOfT,
}

/// A face of the plane or the slab (y = `c`) met by the ray's line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlaneFace {
    /// The quotient, with a wide exponent range.
    pub big_t: wide::Wide,
    /// The quotient rounded to a double.
    pub t: f64,
    /// The point: y is `c`, x and z by the form.
    pub point: Vec3,
}

impl PlaneFace {
    /// Whether the crossing counts for the range: lo < T, and T below hi or unordered with it.
    pub fn counts(&self, lo: f64, hi: f64) -> bool {
        use std::cmp::Ordering::*;
        self.big_t.cmp_f64(lo) == Some(Greater) && !matches!(self.big_t.cmp_f64(hi), Some(Greater | Equal))
    }

    pub fn hit(&self) -> LocalHit {
        LocalHit::new(self.t, self.point, [0.0, 1.0, 0.0], Part::Face { axis: 1 })
    }
}

/// The crossing of the plane y = `c` (c = 0 is the plane, 1 and -1 the slab's faces): the
/// numerator c - o_y rounded to a double (-o_y for the plane), T = numerator / d_y with a double's
/// significand and a far wider exponent range, t = T rounded; the point's x and z as `form`
/// says, a zero coordinate at t = +infinity reported as +0.
pub(crate) fn plane_face(r: &Ray, c: f64, form: FaceForm) -> PlaneFace {
    use wide::Wide;
    let numerator = if c == 0.0 { -r.origin[1] } else { c - r.origin[1] };
    let big_t = Wide::from_f64(numerator).div(Wide::from_f64(r.dir[1]));
    let t = big_t.to_f64();
    let factor = if form == FaceForm::WideSumOfT { Wide::from_f64(t) } else { big_t };
    let coordinate = |k: usize| {
        let p = Wide::from_f64(r.dir[k]).mul(factor);
        let x = match form {
            FaceForm::Rounded => r.origin[k] + p.to_f64(),
            FaceForm::WideSum | FaceForm::WideSumOfT => p.add(Wide::from_f64(r.origin[k])).to_f64(),
        };
        if t.is_infinite() { x + 0.0 } else { x }
    };
    PlaneFace { big_t, t, point: [coordinate(0), c, coordinate(2)] }
}

/// A crossing at t = +infinity of the plane where coordinate `axis` equals `h`: met only
/// where `flat`'s t overflows to +infinity, hi is +infinity and lo is below it (the plane and
/// the slab in every version,; the disc as 5.0). The point is the origin's
/// other coordinates with that one set to `h`, a zero reported as +0 (`o + 0.0`), and the
/// normal the positive axis. Only where the direction's other components are both zero (of
/// either sign): where one leans, the point and texture coordinates are Open (Limits,
/// ), so no hit is made of it rather than a guessed point. For the disc this is the
/// same as before: a leaning component's point would be infinite and fail the disc's test.
fn crossing_at_infinity(r: &Ray, axis: usize, h: f64, range: Range) -> Option<LocalHit> {
    let t = if h == 0.0 { -r.origin[axis] / r.dir[axis] } else { (h - r.origin[axis]) / r.dir[axis] };
    if !(t == f64::INFINITY && range.far == f64::INFINITY && range.near < t) {
        return None;
    }
    if (0..3).any(|k| k != axis && r.dir[k] != 0.0) {
        return None; // (far crossings): the leaning case is Open
    }
    let mut point = r.origin.map(|c| c + 0.0);
    point[axis] = h;
    let mut normal = [0.0; 3];
    normal[axis] = 1.0;
    Some(LocalHit::new(t, point, normal, Part::Face { axis }))
}

/// The plane where coordinate `axis` equals `h`: `t = (h - o)/d` (`-o/d` where h is 0,
/// ), the point `o + t·d` with that coordinate set to `h` , and the
/// positive axis as normal . The form `(h - o)/d` for h ≠ 0 is
/// vector-provisional (the entries do not state it).
fn flat(r: &Ray, axis: usize, h: f64, range: Range) -> Option<LocalHit> {
    flat_at(r, axis, h).filter(|hit| range.holds(hit.t))
}

/// `t` of the plane where coordinate `axis` equals `h`: `-o/d` where h is 0, else `(h - o)/d`.
fn flat_t(r: &Ray, axis: usize, h: f64) -> f64 {
    if h == 0.0 { -r.origin[axis] / r.dir[axis] } else { (h - r.origin[axis]) / r.dir[axis] }
}

/// `flat` without a range: any t, NaN and infinities included . `None` only where the
/// direction's component is zero.
fn flat_at(r: &Ray, axis: usize, h: f64) -> Option<LocalHit> {
    if r.dir[axis] == 0.0 {
        return None;
    }
    let t = flat_t(r, axis, h);
    let mut point = r.at(t);
    point[axis] = h;
    let mut normal = [0.0; 3];
    normal[axis] = 1.0;
    Some(LocalHit::new(t, point, normal, Part::Face { axis }))
}

/// A cap of radius 1 at y = h whose normal is (0, h, 0): the cylinder's ends, the cone's base.
/// Missed when |d_y| <= FLAT_PIECE_LIMIT; counts where x² + z² <= 1.
fn round_cap(r: &Ray, range: Range, h: f64) -> Option<LocalHit> {
    round_cap_at(r, h).filter(|hit| range.holds(hit.t))
}

/// `round_cap` without a range.
fn round_cap_at(r: &Ray, h: f64) -> Option<LocalHit> {
    if r.dir[1].abs() <= FLAT_PIECE_LIMIT {
        return None;
    }
    let mut hit = flat_at(r, 1, h)?;
    let [x, _, z] = hit.point;
    hit.normal = [0.0, h, 0.0];
    hit.part = Part::Cap;
    (x * x + z * z <= 1.0).then_some(hit)
}

/// : the faces across an axis are met only where |d| along it is above 1e-6; a
/// face counts where the other two coordinates lie within ±0.5, ends included; on equal t
/// the first face in the order x, y, z is kept (`nearer` keeps the earlier on a tie).
fn cube(r: &Ray, range: Range) -> Option<LocalHit> {
    let mut best: Option<LocalHit> = None;
    for axis in 0..3 {
        if !(r.dir[axis].abs() > SOLID_FACE_LIMIT) {
            continue;
        }
        for side in [-0.5, 0.5] {
            let Some(h) = flat(r, axis, side, range) else { continue };
            if (0..3).filter(|&k| k != axis).all(|k| h.point[k].abs() <= 0.5) {
                best = nearer(best, Some(h));
            }
        }
    }
    best
}

/// : the direction brought to unit length by q of its
/// squared length (u = d·q(d·d)); with b = o·u the half chord's square is h = 1 - (o·o - b²),
/// written (b² - o·o) + 1 (the same bits: a - b is a + (-b)); no crossing where h < 0 or NaN;
/// the crossings (-b ∓ √h)·q(d·d); a tangent (h = 0) counts. The normal is the point itself.
/// The sums' orders by version are vector-provisional (Freedom: Open).
fn sphere(r: &Ray, range: Range, v: RenderAs) -> Option<LocalHit> {
    sphere_roots(r, v)?.into_iter().find(|&t| range.holds(t)).map(|t| {
        let p = r.at(t);
        LocalHit::new(t, p, p, Part::Curved)
    })
}

/// 's hit record at the crossing `t` (one of `sphere_roots`), with its texture
/// coordinates: what `intersect_shape` gives for a range whose nearest crossing is `t`.
/// 's rule S chooses the crossing itself.
pub(crate) fn sphere_hit_at(r: &Ray, t: f64, v: RenderAs) -> LocalHit {
    let p = r.at(t);
    let mut h = LocalHit::new(t, p, p, Part::Curved);
    let tex = texture(Shape::Sphere, h.part, h.point, v);
    (h.uv, h.du, h.dv) = (tex.uv, tex.du, tex.dv);
    h
}

/// The sphere's two crossings `(-b - √h)·il`, `(-b + √h)·il` of, or `None` where `h` is
/// below 0 or not a number.
pub(crate) fn sphere_roots(r: &Ray, v: RenderAs) -> Option<[f64; 2]> {
    let (o, d) = (r.origin, r.dir);
    let (dd, ou_dot): (fn(Vec3, Vec3) -> f64, fn(Vec3, Vec3) -> f64) = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (dot_right, dot_left),
        RenderAs::Bryce50 => (dot_left, dot_right),
    };
    let il = rsqrt_as(v, dd(d, d));
    let u = [d[0] * il, d[1] * il, d[2] * il];
    let b = ou_dot(o, u);
    let disc = (b * b - ou_dot(o, o)) + 1.0;
    if !(disc >= 0.0) {
        return None;
    }
    let s = sqrt_as(v, disc);
    Some([(-b - s) * il, (-b + s) * il])
}

/// The cylinder wall's quadratic a, b, c (vector-provisional, see `cylinder`).
fn cylinder_wall(r: &Ray) -> (f64, f64, f64) {
    let (o, d) = (r.origin, r.dir);
    let a = d[0] * d[0] + d[2] * d[2];
    let b = 2.0 * (o[0] * d[0] + o[2] * d[2]);
    let c = (o[0] * o[0] + o[2] * o[2]) - 1.0;
    (a, b, c)
}

/// The cylinder wall's normal `(x, 0, z)·q(x² + z²)`.
fn cylinder_wall_normal(p: Vec3, v: RenderAs) -> Vec3 {
    let q = rsqrt_as(v, p[0] * p[0] + p[2] * p[2]);
    [p[0] * q, 0.0, p[2] * q]
}

/// The cone wall's quadratic a, b, c (vector-provisional, see `cone`).
fn cone_wall(r: &Ray) -> (f64, f64, f64) {
    let (o, d) = (r.origin, r.dir);
    let a = (d[0] * d[0] + d[2] * d[2]) - d[1] * d[1] * 0.25;
    let b = 2.0 * ((o[0] * d[0] + o[2] * d[2]) - (o[1] * d[1] - d[1]) * 0.25);
    let c = (o[0] * o[0] + o[2] * o[2]) - ((o[1] * o[1] - 2.0 * o[1]) + 1.0) * 0.25;
    (a, b, c)
}

/// The cone wall's normal (see `cone`).
fn cone_wall_normal(p: Vec3, v: RenderAs) -> Vec3 {
    let q = rsqrt_as(v, p[0] * p[0] + p[2] * p[2]);
    let h = [mul_x87(p[0], q), 0.5, mul_x87(p[2], q)];
    let k = rsqrt_as(v, (h[0] * h[0] + h[2] * h[2]) + h[1] * h[1]);
    [h[0] * k, h[1] * k, h[2] * k]
}

/// How far down the endless spot light's wall goes. leaves the depth Open (a depth far
/// below the base); our choice, as's padding depth.
pub(crate) const ENDLESS_DEPTH: f64 = 1_000_000.0; // (the depth: Open)

/// : a hit on the endless object of a kind: the lower end (the
/// cone's base, the cube's lower face, the cylinder's lower cap, the pyramid's base) is never met,
/// and the wall or sides count down to `-ENDLESS_DEPTH` (E) in place of the old floor; roots,
/// apex, normals and the other faces as the plain shape's. The sphere is not changed.
fn endless_hit(shape: Shape, r: &Ray, range: Range, ctx: &ShapeContext) -> Option<LocalHit> {
    let v = ctx.version;
    match shape {
        Shape::Cone => {
            let (a, b, c) = cone_wall(r);
            wall_roots(v, a, b, c).and_then(|ts| {
                ts.into_iter().find_map(|t| {
                    let p = r.at(t);
                    (range.holds(t) && (-ENDLESS_DEPTH..=1.0).contains(&p[1]))
                        .then(|| LocalHit::new(t, p, cone_wall_normal(p, v), Part::Curved))
                })
            })
        }
        Shape::Cylinder => {
            let top = round_cap(r, range, 1.0);
            let (a, b, c) = cylinder_wall(r);
            let wall = wall_roots(v, a, b, c).and_then(|ts| {
                ts.into_iter().find_map(|t| {
                    let p = r.at(t);
                    (range.holds(t) && (-ENDLESS_DEPTH..=1.0).contains(&p[1]))
                        .then(|| LocalHit::new(t, p, cylinder_wall_normal(p, v), Part::Curved))
                })
            });
            nearer(top, wall)
        }
        Shape::Cube => {
            let mut best: Option<LocalHit> = None;
            for axis in 0..3 {
                if !(r.dir[axis].abs() > SOLID_FACE_LIMIT) {
                    continue;
                }
                for side in [-0.5, 0.5] {
                    if axis == 1 && side == -0.5 {
                        continue;
                    }
                    let Some(h) = flat(r, axis, side, range) else { continue };
                    let inside = (0..3).filter(|&k| k != axis).all(|k| {
                        let c = h.point[k];
                        if k == 1 { (-ENDLESS_DEPTH..=0.5).contains(&c) } else { c.abs() <= 0.5 }
                    });
                    if inside {
                        best = nearer(best, Some(h));
                    }
                }
            }
            best
        }
        Shape::Pyramid => {
            let mut best = None;
            for side in PYRAMID_SIDES {
                best = nearer(best, pyramid_side_from(r, side, ctx.pyramid_normal, -ENDLESS_DEPTH).filter(|h| range.holds(h.t)));
            }
            best
        }
        other => crossing(other, r, range, ctx),
    }
}

/// : the caps first (a cap wins a tie with the wall, the top cap before the
/// bottom), then the wall's quadratic solved as, a root counting where -1 <= y <= 1.
/// The wall's normal is (x, 0, z)·q(x² + z²). The coefficients' arrangement
/// a = dx² + dz², b = 2(ox·dx + oz·dz), c = (ox² + oz²) - 1 is vector-provisional.
fn cylinder(r: &Ray, range: Range, v: RenderAs) -> Option<LocalHit> {
    let caps = nearer(round_cap(r, range, 1.0), round_cap(r, range, -1.0));
    let (a, b, c) = cylinder_wall(r);
    let wall = wall_roots(v, a, b, c).and_then(|ts| {
        ts.into_iter().find_map(|t| {
            let p = r.at(t);
            (range.holds(t) && (-1.0..=1.0).contains(&p[1]))
                .then(|| LocalHit::new(t, p, cylinder_wall_normal(p, v), Part::Curved))
        })
    });
    nearer(caps, wall)
}

/// : the base first (it wins a tie at its edge), then the wall's quadratic solved
/// as, a root counting where -1 <= y <= 1. The
/// wall's normal: the horizontal unit vector (x, z)·q(x² + z²) with 0.5 as its y, brought to
/// unit length by q of the horizontal part's squared length plus 0.25. The coefficients'
/// arrangement, from the expanded form x² + z² - (y² - 2y + 1)/4 = 0, is vector-provisional:
/// a = (dx² + dz²) - dy²·0.25, b = 2((ox·dx + oz·dz) - (oy·dy - dy)·0.25),
/// c = (ox² + oz²) - ((oy² - 2·oy) + 1)·0.25.
fn cone(r: &Ray, range: Range, v: RenderAs) -> Option<LocalHit> {
    let base = round_cap(r, range, -1.0);
    let (a, b, c) = cone_wall(r);
    let wall = wall_roots(v, a, b, c).and_then(|ts| {
        ts.into_iter().find_map(|t| {
            let p = r.at(t);
            (range.holds(t) && (-1.0..=1.0).contains(&p[1]))
                .then(|| LocalHit::new(t, p, cone_wall_normal(p, v), Part::Curved))
        })
    });
    nearer(base, wall)
}

/// . The base: y = -0.5, met only where |d_y| > 1e-6, counting where x and z lie
/// within ±0.5; normal (0, 1, 0). The sides: the planes through the apex (0, 0.5, 0) whose
/// normals are, in this order, (0, up, -out), (-out, up, 0), (0, up, out), (out, up, 0) from
/// the table , each met only where |n·d| > 1e-6; a side counts where -0.5 <= y <= 0.5,
/// the coordinate along the axis it faces lies within ±0.5 and the other horizontal
/// coordinate within ±(0.5 - y)/2. On equal t the first in the order base, -z, -x, +z, +x is
/// reported. The arithmetic of a side's t, (n·apex - n·o) / (n·d), is our own (no vectors
/// reach the sides).
fn pyramid(r: &Ray, range: Range, table: [f64; 2]) -> Option<LocalHit> {
    let mut best = pyramid_base(r).filter(|h| range.holds(h.t));
    for side in PYRAMID_SIDES {
        best = nearer(best, pyramid_side(r, side, table).filter(|h| range.holds(h.t)));
    }
    best
}

/// The pyramid's sides as (facing axis k, sign s), in the order -z, -x, +z, +x.
const PYRAMID_SIDES: [(usize, f64); 4] = [(2, -1.0), (0, -1.0), (2, 1.0), (0, 1.0)];

/// The pyramid's base without a range: y = -0.5 where |d_y| > 1e-6, counting where x and z
/// lie within ±0.5.
fn pyramid_base(r: &Ray) -> Option<LocalHit> {
    if !(r.dir[1].abs() > SOLID_FACE_LIMIT) {
        return None;
    }
    flat_at(r, 1, -0.5).filter(|h| h.point[0].abs() <= 0.5 && h.point[2].abs() <= 0.5)
}

/// One side of the pyramid without a range (see `pyramid`).
fn pyramid_side(r: &Ray, side: (usize, f64), table: [f64; 2]) -> Option<LocalHit> {
    pyramid_side_from(r, side, table, -0.5)
}

/// A side of the pyramid counting down to height `floor` (-0.5 for the plain pyramid, E for the
/// endless one).
pub(crate) fn pyramid_side_from(r: &Ray, (k, s): (usize, f64), table: [f64; 2], floor: f64) -> Option<LocalHit> {
    let [up, out] = table;
    let other = 2 - k;
    let horizontal = s * out;
    let denom = up * r.dir[1] + horizontal * r.dir[k];
    if !(denom.abs() > SOLID_FACE_LIMIT) {
        return None;
    }
    let t = (up * 0.5 - (up * r.origin[1] + horizontal * r.origin[k])) / denom;
    let p = r.at(t);
    let half = (0.5 - p[1]) / 2.0;
    if (floor..=0.5).contains(&p[1]) && p[k].abs() <= 0.5 && p[other].abs() <= half {
        let mut n = [0.0, up, 0.0];
        n[k] = horizontal;
        Some(LocalHit::new(t, p, n, Part::Side { axis: k }))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------
// Texture coordinates.;
// ---------------------------------------------------------------------------------------

/// pi, the double nearest it. geometry (a constant of mathematics)
const PI: f64 = std::f64::consts::PI;
/// 2 pi, the double nearest it (exactly twice PI). geometry
const TAU: f64 = 2.0 * PI;

/// The torus's u where z is 0 or not a number and x >= 0 or not a number: "the double
/// nearest 0.25000000000081224".
const TORUS_U_EDGE: f64 = 0.250_000_000_000_812_24;

/// Plane and slab: the coordinate times the double nearest 0.1.
const PLANE_SCALE: f64 = 0.1;

/// `a / b` where 0/0 (or infinity/infinity) gives the x87 indefinite value.
fn div_x87(a: f64, b: f64) -> f64 {
    let q = a / b;
    if q.is_nan() && !a.is_nan() && !b.is_nan() { X87_INDEFINITE } else { q }
}

/// 's arccosine with's NaNs: a NaN argument is passed on (quiet), and an argument
/// beyond ±1 gives the x87 indefinite value (fdlibm's (x - x) / (x - x) on the unit).
fn acos_x87(c: f64) -> f64 {
    if c.is_nan() {
        quieted(c)
    } else if c.abs() > 1.0 {
        X87_INDEFINITE
    } else {
        acos(c)
    }
}

/// An angle `a` from's arccosine as a fraction of a turn: `a / (2 pi)` on the first
/// side, `1 - a / (2 pi)` otherwise (not wrapped further).
fn turn_fraction(a: f64, first_side: bool) -> f64 {
    let f = a / TAU;
    if first_side { f } else { 1.0 - f }
}

/// u, v, du, dv of a hit on `part` of `shape` at the local point `p`.
pub fn texture(shape: Shape, part: Part, p: Vec3, version: RenderAs) -> Texture {
    let [x, y, z] = p;
    let tex = |uv: [f64; 2], du: Vec3, dv: Vec3| Texture { uv, du, dv };
    match (shape, part) {
        // : u = x/10, v = z/10 as products with 0.1.
        (Shape::Plane | Shape::Slab, _) => {
            tex([x * PLANE_SCALE, z * PLANE_SCALE], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        }
        // : ((x + 1) / 2, (z + 1) / 2).
        (Shape::Square, _) => tex([(x + 1.0) / 2.0, (z + 1.0) / 2.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        (Shape::Disc, _) => disc_texture(x, z, version),
        // : each face's two other coordinates plus 0.5.
        (Shape::Cube, Part::Face { axis: 0 }) => tex([z + 0.5, y + 0.5], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        (Shape::Cube, Part::Face { axis: 1 }) => tex([x + 0.5, z + 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        (Shape::Cube, _) => tex([x + 0.5, y + 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        // : the sides' u is across the side, scaled by its width at that
        // height; 0.5 - y, the quotient and the sum each rounded once . At the apex the
        // quotient is 0/0, the indefinite value (pyramid-sides.jsonl).
        (Shape::Pyramid, Part::Side { axis: 2 }) => {
            tex([div_x87(x, 0.5 - y) + 0.5, y + 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        }
        (Shape::Pyramid, Part::Side { .. }) => {
            tex([div_x87(z, 0.5 - y) + 0.5, y + 0.5], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0])
        }
        (Shape::Pyramid, _) => tex([x + 0.5, z + 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        // : every round end alike.
        (Shape::Cylinder | Shape::Cone, Part::Cap) => tex([0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        (Shape::Cylinder, _) => {
            let u = turn_fraction(acos_x87(x), z >= 0.0);
            tex([u, (y + 1.0) / 2.0], [0.0, -z, x], [0.0, 1.0, 0.0])
        }
        (Shape::Cone, _) => {
            // : w = 1 - v is the wall's radius at that height.
            let v = (y + 1.0) / 2.0;
            let w = 1.0 - v;
            let uv = if w == 0.0 || w.is_nan() {
                [0.0, 1.0]
            } else {
                [turn_fraction(acos_x87(div_x87(x, w)), z >= 0.0), v]
            };
            tex(uv, [0.0, -z, x], [0.0, 1.0, 0.0])
        }
        (Shape::Sphere, _) => sphere_texture(x, y, z, version),
        (Shape::Torus { tube, ring }, _) => torus_texture(x, y, z, tube, ring, version),
        // A mesh's hit carries its own zeros ; a description material is plain.
        (Shape::Mesh(_), _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // A document mesh's coordinates are's, computed with its hit (geometry/mesh.rs);
        // this function is not asked for them.
        (Shape::StoredMesh(_), _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // A terrain's coordinates are's, computed with its hit (geometry/relief.rs).
        (Shape::Terrain(_) | Shape::Path(_), _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // A group's entries carry their members' coordinates ; this is not asked.
        (Shape::Group(_), _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // A tree's coordinates are's, computed with its hit.
        (Shape::Tree { .. } | Shape::TreeLeaves { .. }, _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // A set of metaballs: no document's metaball has a texture (Open); zeros.
        (Shape::Metaball(_) | Shape::MetaSet, _) => tex([0.0, 0.0], [0.0; 3], [0.0; 3]),
        // : the plain shape's coordinates, the endless wall's included.
        (Shape::Light(l), part) => texture(l.plain(), part, p, version),
    }
}

/// : polar about the centre, u from z / s, v = 1 - s.
/// The texture coordinates of a crossing at `t` of the local ray `r`: `texture`'s, except the
/// disc's, which are formed from the ray (`disc_texture_on`).,
pub(crate) fn texture_on(shape: Shape, part: Part, p: Vec3, r: &Ray, t: f64, version: RenderAs) -> Texture {
    match shape {
        // : a crossing at t = +infinity has's u, v, du, dv at its
        // (finite) reported point.
        Shape::Disc if t.is_finite() => disc_texture_on(r, t, version),
        _ => texture(shape, part, p, version),
    }
}

/// The disc's u, v, du and dv from x and z as the shape makes them before its point is reported:
/// x = t d_x + o_x and z = t d_z + o_z, each product and sum with a double's significand and a
/// wide exponent range, only the sum rounded to a double; s the square root  of x^2 + z^2
/// formed from the unrounded sums and rounded once to a double; the test x >= 0 and the
/// quotients read the rounded x and z.
/// (Wide numbers)
fn disc_texture_on(r: &Ray, t: f64, version: RenderAs) -> Texture {
    use wide::Wide;
    let along = |k: usize| Wide::from_f64(t).mul(Wide::from_f64(r.dir[k])).add(Wide::from_f64(r.origin[k]));
    let (wx, wz) = (along(0), along(2));
    let squares = wx.mul(wx).add(wz.mul(wz)).to_f64();
    let (x, z) = (wx.to_f64(), wz.to_f64());
    let s = sqrt_as(version, squares);
    let u = turn_fraction(acos_x87(div_x87(z, s)), x >= 0.0);
    let (du, dv) = if s > 0.0 {
        ([z / s, 0.0, -(x / s)], [x / s, 0.0, z / s])
    } else {
        ([z, 0.0, -x], [x, 0.0, z])
    };
    Texture { uv: [u, 1.0 - s], du, dv }
}

fn disc_texture(x: f64, z: f64, version: RenderAs) -> Texture {
    let s = sqrt_as(version, z * z + x * x);
    // At the centre z / s is 0/0, so u is the indefinite value .
    let u = turn_fraction(acos_x87(div_x87(z, s)), x >= 0.0);
    let (du, dv) = if s > 0.0 {
        ([z / s, 0.0, -(x / s)], [x / s, 0.0, z / s])
    } else {
        ([z, 0.0, -x], [x, 0.0, z])
    };
    Texture { uv: [u, 1.0 - s], du, dv }
}

/// : latitude from the arccosine of -y; longitude from x over FSIN of that angle,
/// the division done on the extended sine .
fn sphere_texture(x: f64, y: f64, z: f64, version: RenderAs) -> Texture {
    let a = acos_x87(-y);
    let v = a / PI;
    let u = if v == 0.0 || v == 1.0 || v.is_nan() {
        0.0
    } else {
        let c = div_f64_by(x, sin_ext(a));
        turn_fraction(acos_x87(c), z > 0.0)
    };
    let s = sqrt_as(version, x * x + z * z);
    // At a pole both quotients are 0/0: the indefinite value, the first negated (vectors).
    Texture { uv: [u, v], du: [-div_x87(z, s), 0.0, div_x87(x, s)], dv: [0.0, 1.0, 0.0] }
}

/// : u about the y axis, v about the tube, both from's arctangent carried
/// in the extended register.
fn torus_texture(x: f64, y: f64, z: f64, tube: f64, ring: f64, version: RenderAs) -> Texture {
    let rho = sqrt_as(version, x * x + z * z);
    let w = rho - ring;
    let u = if z > 0.0 {
        atan_div(-x / z, TAU)
    } else if z < 0.0 {
        atan_add(-x / z, PI) / TAU
    } else if x < 0.0 {
        0.0
    } else {
        TORUS_U_EDGE
    };
    let v = if w == 0.0 || w.is_nan() {
        if y <= 0.0 { -0.5 } else { 0.0 }
    } else {
        // (v, revised: q = (r - y) / w; A = FPATAN of q and 1 in the extended
        // register; c = A / pi rounded once; v = 1 - (c + 1))
        1.0 - (atan_div((tube - y) / w, PI) + 1.0)
    };
    Texture { uv: [u, v], du: [-z, w, 0.0], dv: [y / tube, w / tube, 0.0] }
}

/// The band of the torus's tests: a value strictly between -1e-10 and 1e-10 counts as zero.
/// , Conventions
const TORUS_ZERO_BAND: f64 = 1e-10; // , Conventions

/// The circle about the y axis that the starting point must lie within is widened from E by
/// this fraction: W = (E x 0.1 + E)^2., the starting distance t0
const TORUS_CIRCLE_WIDENING: f64 = 0.1;

fn torus_counts_as_zero(x: f64) -> bool {
    x > -TORUS_ZERO_BAND && x < TORUS_ZERO_BAND
}

/// The starting distance t0 on the ray, or `None` where its bounding tests reject the ray.
/// (rays nearly parallel to a pair of the box's faces; the starting
/// distance t0; the cylinder's quadratic as restated, with its 5.0 discriminant rule)
fn torus_start(r: &Ray, tube: f64, ring: f64, v: RenderAs) -> Option<f64> {
    let ([ox, oy, oz], [dx, dy, dz]) = (r.origin, r.dir);
    let e = ring + tube;
    // Rays nearly parallel to a pair of the box's faces, from outside the box on that axis.
    for (o, d, half) in [(ox, dx, e), (oy, dy, tube), (oz, dz, e)] {
        if torus_counts_as_zero(d) && (o < -half || o > half) {
            return None;
        }
    }
    let widened = e * TORUS_CIRCLE_WIDENING + e;
    let circle = widened * widened;
    let inside = |t: f64| {
        let (x, z) = (t * dx + ox, t * dz + oz);
        x * x + z * z < circle
    };
    if !torus_counts_as_zero(dy) {
        let (top, bottom) = ((tube - oy) / dy, (-tube - oy) / dy);
        let nearer_plane = if bottom < top { bottom } else { top };
        if inside(nearer_plane) {
            return Some(nearer_plane);
        }
    }
    // The endless cylinder x^2 + z^2 = W.
    let a = dz * dz + dx * dx;
    let b = 2.0 * (dz * oz + dx * ox);
    let c = (ox * ox + oz * oz) - circle;
    if torus_counts_as_zero(a) {
        return None;
    }
    let bb = b * b;
    let ca4 = (c * a) * 4.0;
    let disc = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => {
            if !(bb > ca4) {
                return None;
            }
            bb - ca4
        }
        RenderAs::Bryce50 => {
            let d = bb - ca4;
            if !(d > -TORUS_ZERO_BAND) {
                return None; // at or below -1e-10, or not a number
            }
            if d < 0.0 { 0.0 } else { d }
        }
    };
    let s = sqrt_as(v, disc);
    let k = 1.0 / (a + a);
    let (first, second) = ((-b - s) * k, (s - b) * k);
    let height = |t: f64| t * dy + oy;
    let (h1, h2) = (height(first), height(second));
    if (h1 < -tube && h2 < -tube) || (h1 > tube && h2 > tube) {
        return None;
    }
    Some(if second < first { second } else { first })
}

/// The quartic's coefficients, constant term first, in the distance s along the unit direction
/// u from the starting point p: m = Px^2 + Pz^2, n = Ux^2 + Uz^2, j = Ux Px + Pz Uz,
/// h = Py Uy + j, e = Uy^2 + n, g = ((R^2 - r^2) + Py^2) + m, K = R^2 x 4; c4 = e^2,
/// c3 = (e h) 4, c2 = ((h h) 4 + (e g) 2) - K n, c1 = (g h) 4 - 2 (K j), c0 = g^2 - K m.
/// , the quartic
fn torus_coefficients(p: Vec3, u: Vec3, tube: f64, ring: f64) -> [f64; 5] {
    let ([px, py, pz], [ux, uy, uz]) = (p, u);
    let m = px * px + pz * pz;
    let n = ux * ux + uz * uz;
    let j = ux * px + pz * uz;
    let h = py * uy + j;
    let e = uy * uy + n;
    let rr = ring * ring;
    let g = ((rr - tube * tube) + py * py) + m;
    let big_k = rr * 4.0;
    [
        g * g - big_k * m,
        (g * h) * 4.0 - 2.0 * (big_k * j),
        ((h * h) * 4.0 + (e * g) * 2.0) - big_k * n,
        (e * h) * 4.0,
        e * e,
    ]
}

/// . The crossings are the real roots s of the torus's quartic
/// in the distance along the unit direction U = d / L from the starting point P = o + t0 d
/// (`torus_start`), L = sqrt((dy^2 + dz^2) + dx^2) as 3.0 and 3.1 and sqrt((dy^2 + dx^2) + dz^2)
/// as 5.0 ('s square root), solved by Ferrari's method in Schwarze's arrangement
/// (`roots::quartic`); each root gives t = s / L + t0, and the hit is the smallest t with
/// lo < t < hi, its point o + t d.
fn torus(r: &Ray, range: Range, tube: f64, ring: f64, v: RenderAs) -> Option<LocalHit> {
    torus_roots(r, tube, ring, v).into_iter().find(|&t| range.holds(t)).map(|t| {
        let p = r.at(t);
        LocalHit::new(t, p, torus_normal(p, tube, ring, v), Part::Curved)
    })
}

/// Every t the torus's solver gives (see `torus`), sorted by `total_cmp`; empty where the
/// bounding tests reject the ray. Any r and R are taken as stored, a box whose low end lies
/// above its high end read as the tests read it., ,
/// Errors (revised: no case is made of r or R of 0 or less, or r >= R)
fn torus_roots(r: &Ray, tube: f64, ring: f64, v: RenderAs) -> Vec<f64> {
    let Some(t0) = torus_start(r, tube, ring, v) else { return Vec::new() };
    let [dx, dy, dz] = r.dir;
    let squares = match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (dy * dy + dz * dz) + dx * dx,
        RenderAs::Bryce50 => (dy * dy + dx * dx) + dz * dz,
    };
    let len = sqrt_as(v, squares);
    let u = [dx / len, dy / len, dz / len];
    let coefficients = torus_coefficients(r.at(t0), u, tube, ring);
    let mut ts: Vec<f64> = roots::quartic(v, coefficients).into_iter().map(|s| s / len + t0).collect();
    ts.sort_by(|x, y| x.total_cmp(y));
    ts
}

/// The torus's normal at a point, in its own space, before's carrying: with
/// rho = sqrt(x^2 + z^2), w = rho - R, r2 = r r and G = (w + w) / (rho r2), the gradient
/// N = (G x, (y + y) / r2, G z), times k1 = q((Ny^2 + Nz^2) + Nx^2).
/// , (the normal)
pub fn torus_normal(p: Vec3, tube: f64, ring: f64, v: RenderAs) -> Vec3 {
    let [x, y, z] = p;
    let r2 = tube * tube;
    let rho = sqrt_as(v, x * x + z * z);
    let w = rho - ring;
    let g = (w + w) / (rho * r2);
    let n = [g * x, (y + y) / r2, g * z];
    let k1 = rsqrt_as(v, (n[1] * n[1] + n[2] * n[2]) + n[0] * n[0]);
    [k1 * n[0], k1 * n[1], k1 * n[2]]
}

// ---------------------------------------------------------------------------------------
// Interior.,
// (Interior)
// ---------------------------------------------------------------------------------------

/// Where a point of a shape's own space lies against the shape's solid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interior {
    /// In the open interior: the solid holds the point.
    Inside,
    Outside,
    /// Exactly on the surface, as the test computes it (Open).
    OnSurface,
}

/// Each `(value, bound)`: inside where every value is below its bound, outside where any is
/// above, on the surface otherwise. (Interior)
fn classify(conditions: &[(f64, f64)]) -> Interior {
    if conditions.iter().any(|&(v, b)| v > b) {
        Interior::Outside
    } else if conditions.iter().all(|&(v, b)| v < b) {
        Interior::Inside
    } else {
        Interior::OnSurface
    }
}

/// Where the local point `p` lies against `shape`'s solid ('s forms). The plane, the
/// square and the disc hold nothing. `None`: not decidable (a torus with r or R not finite;
/// a point that is not finite).
/// (inside the sphere, cube, cylinder, cone, torus, between the slab's
/// faces; plane, square, disc never). (Interior)
pub fn interior(shape: Shape, p: Vec3) -> Option<Interior> {
    if !p.iter().all(|c| c.is_finite()) {
        return None;
    }
    let [x, y, z] = p;
    let xz = x * x + z * z;
    Some(match shape {
        Shape::Plane | Shape::Square | Shape::Disc => Interior::Outside,
        Shape::Slab => classify(&[(y.abs(), 1.0)]),
        Shape::Cube => classify(&[(x.abs(), 0.5), (y.abs(), 0.5), (z.abs(), 0.5)]),
        Shape::Sphere => classify(&[(xz + y * y, 1.0)]),
        Shape::Cylinder => classify(&[(xz, 1.0), (y.abs(), 1.0)]),
        Shape::Cone => {
            let r = (1.0 - y) / 2.0;
            classify(&[(y.abs(), 1.0), (xz, r * r)])
        }
        Shape::Pyramid => {
            let half = (0.5 - y) / 2.0;
            classify(&[(y.abs(), 0.5), (x.abs(), half), (z.abs(), half)])
        }
        // Errors (revised: a torus whose r or R is 0 or less, or r not below R, is
        // not refused; reads r through r^2). The solid of such a torus, for the eye:
        // the points nearer than |r| to the ring of radius |R| (our choice).
        // (The eye and a torus of any radii)
        Shape::Torus { tube, ring } => {
            if !(tube.is_finite() && ring.is_finite()) {
                return None;
            }
            let d = xz.sqrt() - ring.abs();
            classify(&[(d * d + y * y, tube * tube)])
        }
        // : a mesh never
        // holds the eye, closed or not, in 3.0, 3.1 and 5.0.
        Shape::Mesh(_) => Interior::Outside,
        Shape::StoredMesh(_) => Interior::Outside,
        // A terrain's test needs its grid: `Scene::interior_of` . Asked here, with
        // no grid, it holds nothing.
        Shape::Terrain(_) | Shape::Path(_) => Interior::Outside,
        // A group's test needs its members: `Scene::interior_of` (The
        // eye inside a group). Asked here, without them, it holds nothing.
        Shape::Group(_) => Interior::Outside,
        // The eye inside a tree's wood: Open (Inputs); our choice is that it holds none.
        // (The skins)
        Shape::Tree { .. } | Shape::TreeLeaves { .. } => Interior::Outside,
        // A set of metaballs: the depth is the sum of the fields , asked of the scene.
        Shape::Metaball(_) | Shape::MetaSet => Interior::Outside,
        // : a light's object is its plain shape's solid; the endless cone's wall
        // goes on below the base (the eye inside it: Open for lights, refused by the render).
        Shape::Light(l) if l.endless() && l.shape == light::LightShape::Cone => {
            let r = (1.0 - y) / 2.0;
            classify(&[(y, 1.0), (-y, ENDLESS_DEPTH), (xz, r * r)])
        }
        Shape::Light(l) => return interior(l.plain(), p),
    })
}

impl Scene {
    /// Where the world point `p` lies against object `i`'s solid: `interior` for the shapes,
    /// 's test for a terrain (inside or outside; it has no surface case).
    pub fn interior_of(&self, i: usize, p: Vec3) -> Option<Interior> {
        let o = &self.objects[i];
        let local = o.placement.point_to_local(p);
        // and: the point carried by a
        // singular map's inverse is not a number (or infinite); every test begins with a
        // comparison that takes a coordinate that is not a number as outside, except the
        // sphere's (and the sphere-shaped light object's), whose one comparison of the squared
        // distance from the centre with 1 takes a distance that is not a number as inside.
        // (singular maps)
        if o.placement.is_singular() {
            let sphere = matches!(o.shape, Shape::Sphere) || matches!(o.shape, Shape::Light(l) if l.plain() == Shape::Sphere);
            let d2 = (local[0] * local[0] + local[2] * local[2]) + local[1] * local[1];
            return Some(if sphere && !(d2 >= 1.0) { Interior::Inside } else { Interior::Outside });
        }
        match o.shape {
            Shape::Terrain(k) => {
                if !local.iter().all(|c| c.is_finite()) {
                    return None;
                }
                let t = self.terrains.get(k)?;
                Some(if t.holds_eye(local) { Interior::Inside } else { Interior::Outside })
            }
            // (The eye inside a group)
            Shape::Group(k) => self.group_interior(k, p),
            // : the tree asks each piece with the point
            // itself (not carried into the tree's space)
            Shape::Tree { model, .. } => {
                Some(if self.trees.get(model).is_some_and(|t| t.holds_point(p)) { Interior::Inside } else { Interior::Outside })
            }
            shape => interior(shape, local),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree finds the triangle the plain scan finds (same t, earliest on a tie), for rays
    /// from everywhere, including rays along the axes and through shared edges and corners.
    /// (Speed on mesh scenes)
    #[test]
    fn mesh_tree_matches_scan() {
        let mut seed = 12345u64;
        let mut rnd = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let mut tris = Vec::new();
        for _ in 0..600 {
            let c = [rnd() * 10.0, rnd() * 10.0, rnd() * 10.0];
            let mut p = |k: f64| [c[0] + (rnd() - 0.5) * k, c[1] + (rnd() - 0.5) * k, c[2] + (rnd() - 0.5) * k];
            tris.push(Triangle { corners: [p(1.0), p(1.0), p(1.0)], normal: None });
        }
        // copies (equal t on a tie) and a flat grid triangle pair on an axis plane
        tris.extend(tris.clone().into_iter().take(30));
        tris.push(Triangle { corners: [[0.0, 5.0, 0.0], [10.0, 5.0, 0.0], [0.0, 5.0, 10.0]], normal: None });
        let mesh = Mesh::new(tris);
        let ctx = ShapeContext { version: RenderAs::Bryce50, pyramid_normal: [0.0; 2], shadow: false };
        for k in 0..4000 {
            let origin = [rnd() * 14.0 - 2.0, rnd() * 14.0 - 2.0, rnd() * 14.0 - 2.0];
            let mut dir = [rnd() - 0.5, rnd() - 0.5, rnd() - 0.5];
            if k % 7 == 0 { dir[1] = 0.0; }
            if k % 11 == 0 { dir = [0.0, 0.0, -1.0]; }
            let r = Ray { origin, dir };
            let range = Range { near: 1e-6, far: if k % 3 == 0 { 6.0 } else { f64::INFINITY } };
            let fast = intersect_mesh(&mesh, &r, range, &ctx);
            let mut slow = None;
            let mut limit = range;
            for tri in &mesh.triangles {
                if let Some(t) = triangle_t(tri, &r, limit) {
                    limit.far = t;
                    slow = Some(t);
                }
            }
            assert_eq!(fast.map(|h| h.t.to_bits()), slow.map(f64::to_bits), "ray {k}");
        }
    }

    /// : a quotient that overflows a double is a finite
    /// wide number, so a direction x of 1e-300 gives a finite x at t = +infinity and a direction
    /// z of 1 an infinite one; where hi is finite the crossing is not met.
    #[test]
    fn plane_far_crossing_leaning() {
        let ctx = ShapeContext { version: RenderAs::Bryce50, pyramid_normal: [0.0; 2], shadow: false };
        let ray = Ray { origin: [0.0, 1e305, 0.0], dir: [1e-300, -1e-4, 1.0] };
        let far = Range { near: 0.0, far: f64::INFINITY };
        for shape in [Shape::Plane, Shape::Slab] {
            let h = intersect_shape(shape, &ray, far, &ctx).expect("met where hi is infinity");
            assert_eq!(h.t, f64::INFINITY);
            assert!((h.point[0] / 1e9 - 1.0).abs() < 1e-9, "{:?}", h.point);
            assert_eq!(h.point[2], f64::INFINITY);
            assert!(intersect_shape(shape, &ray, Range { near: 0.0, far: 1e308 }, &ctx).is_none());
        }
    }

    /// the solids hold points of their interior, the flat shapes never.
    #[test]
    fn interior_of_each_shape() {
        use Interior::*;
        let torus = Shape::Torus { tube: 0.25, ring: 1.0 };
        let cases: &[(Shape, Vec3, Interior)] = &[
            (Shape::Sphere, [0.0, 0.0, 0.0], Inside),
            (Shape::Sphere, [0.5, 0.5, 0.5], Inside),
            (Shape::Sphere, [1.0, 0.0, 0.0], OnSurface),
            (Shape::Sphere, [0.8, 0.8, 0.0], Outside),
            (Shape::Cube, [0.49, -0.49, 0.0], Inside),
            (Shape::Cube, [0.5, 0.0, 0.0], OnSurface),
            (Shape::Cube, [0.6, 0.0, 0.0], Outside),
            (Shape::Cube, [0.5, 0.0, 0.7], Outside),
            (Shape::Cylinder, [0.0, 0.99, 0.0], Inside),
            (Shape::Cylinder, [0.0, 1.0, 0.0], OnSurface),
            (Shape::Cylinder, [0.9, 0.0, 0.9], Outside),
            (Shape::Cone, [0.0, -0.99, 0.0], Inside),
            (Shape::Cone, [0.5, 0.0, 0.0], OnSurface),
            (Shape::Cone, [0.0, 0.9, 0.1], Outside),
            (Shape::Cone, [0.0, -1.0, 0.0], OnSurface),
            (torus, [1.0, 0.0, 0.0], Inside),
            (torus, [0.0, 0.1, -1.1], Inside),
            (torus, [0.0, 0.0, 0.0], Outside),
            (torus, [1.25, 0.0, 0.0], OnSurface),
            (Shape::Slab, [1e9, 0.5, -1e9], Inside),
            (Shape::Slab, [0.0, -1.0, 0.0], OnSurface),
            (Shape::Slab, [0.0, 1.5, 0.0], Outside),
            (Shape::Pyramid, [0.0, 0.0, 0.0], Inside),
            (Shape::Pyramid, [0.3, 0.0, 0.0], Outside),
            (Shape::Plane, [0.0, -5.0, 0.0], Outside),
            (Shape::Plane, [0.0, 0.0, 0.0], Outside),
            (Shape::Square, [0.0, 0.0, 0.0], Outside),
            (Shape::Disc, [0.0, 0.0, 0.0], Outside),
        ];
        for &(shape, p, want) in cases {
            assert_eq!(interior(shape, p), Some(want), "{shape:?} at {p:?}");
        }
        // Errors (revised): any finite radii are taken as stored; r through r^2 and our
        // choice of |R| for the eye.
        assert_eq!(interior(Shape::Torus { tube: 0.0, ring: 1.0 }, [1.0, 0.0, 0.0]), Some(OnSurface));
        assert_eq!(interior(Shape::Torus { tube: -0.35, ring: 1.0 }, [1.2, 0.0, 0.0]), Some(Inside));
        assert_eq!(interior(Shape::Torus { tube: 1.2, ring: 0.5 }, [0.0, 0.0, 0.0]), Some(Inside));
        assert_eq!(interior(Shape::Torus { tube: 0.25, ring: f64::NAN }, [1.0, 0.0, 0.0]), None);
        assert_eq!(interior(Shape::Sphere, [f64::NAN, 0.0, 0.0]), None);
    }

    /// The eye carried by the inverse of the object's matrix : picture 11's cube
    /// (3 times the identity at (0, 8, -59.5)) holds (0, 8, -60.0000177) and not
    /// (0, 8, -61.77).
    #[test]
    fn eye_carried_into_the_cube() {
        let rows = [[3.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 3.0]];
        let pl = Placement::new(rows, [0.0, 8.0, -59.5]).unwrap();
        assert_eq!(interior(Shape::Cube, pl.point_to_local([0.0, 8.0, -60.0000177])), Some(Interior::Inside));
        assert_eq!(interior(Shape::Cube, pl.point_to_local([0.0, 8.0, -61.77])), Some(Interior::Outside));
    }

    /// : a matrix with a determinant of 0 is inverted as any other (k an
    /// infinity); a sphere of size (2.6, 0, 2.6) holds the eye wherever it is (the distance is not
    /// a number: inside), a cube of that matrix does not, and no ray meets it.
    #[test]
    fn singular_map_holds_the_eye_as_a_sphere() {
        let rows = [[2.6, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 2.6]];
        let pl = Placement::new(rows, [1.0, 2.0, 3.0]).expect("a determinant of 0 is not tested");
        assert!(pl.is_singular());
        assert!(pl.inverse_rows().iter().flatten().all(|x| !x.is_finite()));
        let scene = Scene {
            objects: vec![Object { shape: Shape::Sphere, placement: pl }, Object { shape: Shape::Cube, placement: pl }],
            ..Scene::default()
        };
        assert_eq!(scene.interior_of(0, [0.5, 0.5, 0.5]), Some(Interior::Inside));
        assert_eq!(scene.interior_of(1, [0.5, 0.5, 0.5]), Some(Interior::Outside));
        let ray = Ray { origin: [0.0, 0.0, -10.0], dir: [0.0, 0.0, 1.0] };
        let ctx = ShapeContext { version: RenderAs::Bryce50, pyramid_normal: [0.0; 2], shadow: false };
        assert!(scene.intersect(&ray, Range { near: 0.0, far: f64::INFINITY }, &ctx).is_none());
    }

    /// The identity's inverse by cofactors, with the sign of every zero. geometry
    #[test]
    fn identity_inverse_is_the_cofactor_inverse() {
        let got = cofactor_inverse(&IDENTITY_ROWS).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(got[i][j].to_bits(), IDENTITY_INVERSE[i][j].to_bits(), "({i}, {j})");
            }
        }
    }
}
