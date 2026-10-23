// SPDX-License-Identifier: MIT
//! A terrain (height field) drawn as a relief of triangles: the ray's stretch through its
//! box and bound planes, the walk over the cells under the stretch, the crossing with each
//! cell's triangles (and their mirror images), the closing faces, the smoothed normal, the
//! test of a point under the surface and the terrain's boxes.
//!
//! (the list of
//! a terrain held by a group)
//! ; (the switches, the
//! bounds as each version reads them),
//! (Drawing a terrain),
//!
//! Every operation is one IEEE binary64 operation in the order written , with no fused
//! multiply-add: the order of each sum below is part of the result's bits (Accuracy:
//! the vectors decide the orders; the ones kept are recorded in).

use crate::geom::Vec3;
use crate::numerics::rsqrt_as;
use crate::scene_height_fields::TerrainPart;
use crate::version::RenderAs;

use super::{LocalHit, Part, Range, Ray};

/// Half the range of heights: an ordinary terrain's height h lies at y = h - 32767.5.
const HALF_RANGE: f64 = 32767.5;
/// The largest height.,
const TOP: f64 = 65535.0;
/// The box's half height in y: ordinary and mirrored.,
const BOX_Y: f64 = 32767.51;
const BOX_Y_MIRRORED: f64 = 65535.01;
/// The side faces' extent in y for a mirrored terrain (±32767.5 for an ordinary one).
const SIDE_Y_MIRRORED: f64 = 65535.0;
/// The closing faces' offset from the bound planes.
const FACE_OFFSET: f64 = 1e-7;
/// A face is met only where the direction's component across it is above this.
const FACE_LIMIT: f64 = 1e-6;
/// The solid word's top and bottom lie this far beyond the bounds as 3.0 and 3.1.
const SOLID_OFFSET_3X: f64 = 10.0;
/// The slack of the test of lying between the bounds.
const UNDER_SLACK: f64 = 1e-10;
/// The boxes' margin.
const BOX_MARGIN: f64 = 0.01;
/// The bounds as 3.1 and 5.0 hold them for the scene's box, and 5.0's group box step.
const BOX_BOUND_LOW: f64 = -0.00001;
const BOX_BOUND_HIGH: f64 = 1.00001;
/// The factors of n that bound the eye test as 3.1.
const EYE_BOUND_LOW: f64 = -0.00001;
const EYE_BOUND_HIGH: f64 = 1.00001;
const GROUP_TOP_SLACK: f64 = 0.00001;
/// The object's second flag word bit by which a terrain with no closing face holds points
/// as 3.1 and 5.0.
pub const HOLDS_WITHOUT_FACES: u32 = 0x40;

/// The six closing faces, in the order they are tried and listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefFace {
    Top,
    Bottom,
    LowX,
    HighX,
    LowZ,
    HighZ,
}

impl ReliefFace {
    pub const ALL: [ReliefFace; 6] =
        [ReliefFace::Top, ReliefFace::Bottom, ReliefFace::LowX, ReliefFace::HighX, ReliefFace::LowZ, ReliefFace::HighZ];

    /// The face's bit in the closing and the solid words.
    pub fn bit(self) -> u32 {
        match self {
            ReliefFace::Bottom => 0x1,
            ReliefFace::Top => 0x2,
            ReliefFace::HighX => 0x4,
            ReliefFace::LowX => 0x8,
            ReliefFace::LowZ => 0x10,
            ReliefFace::HighZ => 0x20,
        }
    }

    /// The axis across the face.
    fn axis(self) -> usize {
        match self {
            ReliefFace::Top | ReliefFace::Bottom => 1,
            ReliefFace::LowX | ReliefFace::HighX => 0,
            ReliefFace::LowZ | ReliefFace::HighZ => 2,
        }
    }

    /// The outward axis vector.
    pub fn normal(self) -> Vec3 {
        match self {
            ReliefFace::Top => [0.0, 1.0, 0.0],
            ReliefFace::Bottom => [0.0, -1.0, 0.0],
            ReliefFace::LowX => [-1.0, 0.0, 0.0],
            ReliefFace::HighX => [1.0, 0.0, 0.0],
            ReliefFace::LowZ => [0.0, 0.0, -1.0],
            ReliefFace::HighZ => [0.0, 0.0, 1.0],
        }
    }

    /// The word the vectors and the reports use.
    pub fn word(self) -> &'static str {
        match self {
            ReliefFace::Top => "top",
            ReliefFace::Bottom => "bottom",
            ReliefFace::LowX => "low_x",
            ReliefFace::HighX => "high_x",
            ReliefFace::LowZ => "low_z",
            ReliefFace::HighZ => "high_z",
        }
    }
}

/// What part of a terrain a hit is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefPart {
    /// A triangle of the relief: the cell (c, r), the second triangle (B) or the first (A),
    /// and whether it is the mirror image .
    Triangle { cell: [usize; 2], second: bool, image: bool },
    /// A closing face.
    Face(ReliefFace),
}

/// One face of the solid word met by a ray (the list made for the combined groups).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListedFace {
    pub face: ReliefFace,
    /// Whether its bit is also on in the closing word.
    pub visible: bool,
    pub t: f64,
    pub point: Vec3,
    pub normal: Vec3,
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
}

/// A terrain as a render as one version draws it: the grid, the switches and the bounds that
/// version reads (as 5.0 the bounds are held and the solid word is 0x3f).
#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    version: RenderAs,
    /// n, the grid's side.
    n: usize,
    /// The heights as doubles, row after row (row j along z, column i along x).
    h: Vec<f64>,
    mirrored: bool,
    smooth: bool,
    closing: u32,
    solid: u32,
    /// λ and μ as the version reads them.
    bounds: [f64; 2],
    /// Λ = 65535 λ, M = 65535 μ.
    lam: f64,
    mu: f64,
    /// Λ' and M': Λ and M held between 0 and 65535 .
    lam_held: f64,
    mu_held: f64,
    /// Bit 0x40 of the object's second flag word .
    holds_without_faces: bool,
    /// The own box's y numbers as the document holds them.
    own_y: [f64; 2],
    /// The lowest and highest height of each cell, for the walk's skip .
    cell_low: Vec<f64>,
    cell_high: Vec<f64>,
    /// The world box (low, high) a ray must meet before the terrain is asked:'s box
    /// carried by the object's matrix, set by `place`; `None` for a terrain asked in its own
    /// space alone (the function-level vectors).
    world_box: Option<(Vec3, Vec3)>,
    /// The world box a combined group tests a ray against for this terrain as its member
    /// , set by `place_member`.
    member_box: Option<(Vec3, Vec3)>,
    /// The world box by which the walk of lists a group holding this terrain ,
    /// set by `place_member`.
    walk_box: Option<(Vec3, Vec3)>,
}

/// Holds `v` between `lo` and `hi`; a value that is not a number counts as the lowest.
fn held(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() || v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

impl Relief {
    /// The terrain of a document's part, as `version` draws it; `second_flags` is its
    /// object's second flag word .,
    pub fn from_part(part: &TerrainPart, version: RenderAs, second_flags: u32) -> Relief {
        let [lower, upper] = part.bounds(version);
        let mut relief = Relief::new(
            version,
            part.grid.side(),
            part.grid.heights(),
            ReliefSwitches {
                mirrored: part.mirrored,
                smooth: part.smooth,
                closing: part.closing_faces,
                solid: part.solid_faces(version),
                holds_without_faces: second_flags & HOLDS_WITHOUT_FACES != 0,
            },
            [lower, upper],
        );
        relief.own_y = part.stored_box_y;
        relief
    }

    /// A terrain from its grid (n by n heights, row after row), switches and bounds λ, μ as
    /// the version reads them.
    pub fn new(version: RenderAs, n: usize, heights: &[u16], sw: ReliefSwitches, bounds: [f64; 2]) -> Relief {
        assert!(n >= 2 && heights.len() == n * n, "a terrain's grid is n by n with n at least 2 (TE-1)");
        let h: Vec<f64> = heights.iter().map(|&v| f64::from(v)).collect();
        let lam = TOP * bounds[0];
        let mu = TOP * bounds[1];
        let cells = n - 1;
        let mut cell_low = Vec::with_capacity(cells * cells);
        let mut cell_high = Vec::with_capacity(cells * cells);
        for r in 0..cells {
            for c in 0..cells {
                let k = [h[r * n + c], h[r * n + c + 1], h[(r + 1) * n + c], h[(r + 1) * n + c + 1]];
                cell_low.push(k.iter().copied().fold(f64::INFINITY, f64::min));
                cell_high.push(k.iter().copied().fold(f64::NEG_INFINITY, f64::max));
            }
        }
        Relief {
            version,
            n,
            h,
            mirrored: sw.mirrored,
            smooth: sw.smooth,
            closing: sw.closing,
            solid: sw.solid,
            bounds,
            lam,
            mu,
            lam_held: held(lam, 0.0, TOP),
            mu_held: held(mu, 0.0, TOP),
            holds_without_faces: sw.holds_without_faces,
            own_y: if sw.mirrored { [-BOX_Y_MIRRORED, BOX_Y_MIRRORED] } else { [-BOX_Y, BOX_Y] },
            cell_low,
            cell_high,
            world_box: None,
            member_box: None,
            walk_box: None,
        }
    }

    /// Places the terrain in a scene: the box the scene's tests use (as 3.0 the own box, as
    /// 3.1 and 5.0 the bounds' box), its eight corners carried by the object's
    /// placement, the least and greatest of each coordinate.
    pub fn place(&mut self, placement: &super::Placement) {
        let (low, high) = self.scene_box();
        let mut b = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for k in 0..8 {
            let p = [
                if k & 4 == 0 { low[0] } else { high[0] },
                if k & 2 == 0 { low[1] } else { high[1] },
                if k & 1 == 0 { low[2] } else { high[2] },
            ];
            let w = placement.to_world(p);
            for a in 0..3 {
                b.0[a] = b.0[a].min(w[a]);
                b.1[a] = b.1[a].max(w[a]);
            }
        }
        self.world_box = Some(b);
    }

    /// Places the terrain as a member of a combined group: the box the group tests is, as 5.0,
    /// 's group box; as 3.0 the own box and as 3.1 the box its bounds give ('s
    /// fallback), its eight corners carried by the object's placement.
    /// , -group-members (Held back)
    pub fn place_member(&mut self, placement: &super::Placement) {
        let carry = |(low, high): (Vec3, Vec3)| {
            let mut b = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for k in 0..8 {
                let p = [
                    if k & 4 == 0 { low[0] } else { high[0] },
                    if k & 2 == 0 { low[1] } else { high[1] },
                    if k & 1 == 0 { low[2] } else { high[2] },
                ];
                let w = placement.to_world(p);
                for a in 0..3 {
                    b.0[a] = b.0[a].min(w[a]);
                    b.1[a] = b.1[a].max(w[a]);
                }
            }
            b
        };
        self.member_box = Some(carry(if self.version == RenderAs::Bryce50 { self.group_box() } else { self.scene_box() }));
        // : the
        // box the walk takes for a terrain member is's scene box in every version (as
        // 5.0 not the smaller group box).
        self.walk_box = Some(carry(self.scene_box()));
    }

    /// The box by which the walk of lists the terrain object itself, before the widening, in
    /// the world: as 3.0 its own box carried, as 3.1 the box its bounds give, as 5.0 the smaller
    /// box its heights give, whether or not a group holds it. (a
    /// terrain's box),
    pub fn listing_world_box(&self, placement: &super::Placement) -> (Vec3, Vec3) {
        let (low, high) = if self.version == RenderAs::Bryce50 { self.group_box() } else { self.scene_box() };
        let mut b = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for k in 0..8 {
            let p = [
                if k & 4 == 0 { low[0] } else { high[0] },
                if k & 2 == 0 { low[1] } else { high[1] },
                if k & 1 == 0 { low[2] } else { high[2] },
            ];
            let w = placement.to_world(p);
            for a in 0..3 {
                b.0[a] = b.0[a].min(w[a]);
                b.1[a] = b.1[a].max(w[a]);
            }
        }
        b
    }

    /// The box by which the walk of lists a group holding this terrain, in the world
    /// (after `place_member`).
    pub fn walk_world_box(&self) -> Option<(Vec3, Vec3)> {
        self.walk_box
    }

    /// The box a combined group tests for this member, in the world (after `place_member`).
    pub fn member_world_box(&self) -> Option<(Vec3, Vec3)> {
        self.member_box
    }

    /// Whether a world ray meets the world box within the range (the slab method; a zero
    /// component needs its origin between the planes, ends included). A terrain not placed
    /// is met by every ray. (the scene's test of the box)
    pub fn world_box_met(&self, r: &Ray, range: Range) -> bool {
        let Some((low, high)) = self.world_box else { return true };
        let (mut near, mut far) = (range.near, range.far);
        for a in 0..3 {
            let (o, d) = (r.origin[a], r.dir[a]);
            if d == 0.0 {
                if !(o >= low[a] && o <= high[a]) {
                    return false;
                }
                continue;
            }
            let (t1, t2) = ((low[a] - o) / d, (high[a] - o) / d);
            let (enter, leave) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            near = near.max(enter);
            far = far.min(leave);
        }
        near <= far
    }

    pub fn side(&self) -> usize {
        self.n
    }

    pub fn mirrored(&self) -> bool {
        self.mirrored
    }

    /// Whether the closing word has any of the six faces.
    pub fn closed(&self) -> bool {
        self.closing & 0x3f != 0
    }

    fn at(&self, i: usize, j: usize) -> f64 {
        self.h[j * self.n + i]
    }

    /// A height with indices held to the grid (a height outside it is that of the nearest
    /// grid point).
    fn at_held(&self, i: isize, j: isize) -> f64 {
        let last = self.n as isize - 1;
        self.at(i.clamp(0, last) as usize, j.clamp(0, last) as usize)
    }

    /// The amount by which y is carried to the grid's corner: 32767.5, or 0 for a mirrored
    /// terrain.
    fn y_shift(&self) -> f64 {
        if self.mirrored { 0.0 } else { HALF_RANGE }
    }

    fn half(&self) -> f64 {
        (self.n / 2) as f64 // n is even for every side draws; n/2 as writes it
    }
}

/// The switches of a terrain.,
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReliefSwitches {
    pub mirrored: bool,
    pub smooth: bool,
    pub closing: u32,
    pub solid: u32,
    pub holds_without_faces: bool,
}

// ---------------------------------------------------------------------------------------
// The stretch.
// ---------------------------------------------------------------------------------------

/// The ray's stretch: crossings count where `start < t <= end` (and lo < t < hi).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stretch {
    start: f64,
    end: f64,
}

impl Relief {
    /// The box that cuts the ray (The stretch searched): the own box of with its
    /// lower x and z raised by 0.01 and its upper x and z lowered by 1.01, each sum rounded to a
    /// double ((-(h + 0.01)) + 0.01 to (h + 0.01) - 1.01, which is -n/2 to n/2 - 1 to within the
    /// last bit), its y numbers kept as the document holds them. (The stretch
    /// searched),
    fn search_box(&self) -> (Vec3, Vec3) {
        let (low, high) = self.own_box();
        let (lx, hx) = (low[0] + BOX_MARGIN, high[0] - 1.01);
        ([lx, low[1], lx], [hx, high[1], hx])
    }

    /// The slab method on the box : `None` where the ray misses it (a zero component
    /// needs its origin strictly inside).
    fn box_stretch(&self, r: &Ray) -> Option<Stretch> {
        let (low, high) = self.search_box();
        let mut s = Stretch { start: f64::NEG_INFINITY, end: f64::INFINITY };
        for a in 0..3 {
            let (o, d) = (r.origin[a], r.dir[a]);
            if d == 0.0 {
                if !(o > low[a] && o < high[a]) {
                    return None;
                }
                continue;
            }
            let t1 = (low[a] - o) / d;
            let t2 = (high[a] - o) / d;
            let (enter, leave) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            s.start = s.start.max(enter);
            s.end = s.end.min(leave);
        }
        (s.start <= s.end).then_some(s)
    }

    /// The bound planes' y in the terrain's own space: lower, upper.
    fn bound_planes(&self) -> (f64, f64) {
        if self.mirrored {
            ((2.0 * self.lam - self.mu), self.mu)
        } else {
            (self.lam - HALF_RANGE, self.mu - HALF_RANGE)
        }
    }

    /// The box's stretch cut again by the bound planes, where d_y is not 0 (a level ray is
    /// not cut by them).,
    fn cut_by_bounds(&self, mut s: Stretch, r: &Ray) -> Stretch {
        let d = r.dir[1];
        if d != 0.0 {
            let (lo, hi) = self.bound_planes();
            let t1 = (lo - r.origin[1]) / d;
            let t2 = (hi - r.origin[1]) / d;
            let (enter, leave) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            s.start = s.start.max(enter);
            s.end = s.end.min(leave);
        }
        s
    }
}

// ---------------------------------------------------------------------------------------
// The triangles.,
// ---------------------------------------------------------------------------------------

/// A crossing found in a cell, before the closing faces.
#[derive(Clone, Copy, Debug)]
struct Crossing {
    t: f64,
    /// o' + t d: the point measured from the grid's lowest corner.
    at: Vec3,
    normal: Vec3,
    part: ReliefPart,
}

/// The orders of one triangle's arithmetic of t.
#[derive(Clone, Copy, Debug)]
struct TOrders {
    p_sum: [usize; 3],
    joins: [usize; 3],
    five_mirrored: bool,
}

impl Relief {
    /// The unit normal of triangle A (`second` false) or B of cell (c, r), upward.
    fn facet_normal(&self, c: usize, r: usize, second: bool) -> Vec3 {
        let raw = if second {
            [self.at(c, r + 1) - self.at(c + 1, r + 1), 1.0, self.at(c + 1, r) - self.at(c + 1, r + 1)]
        } else {
            [self.at(c, r) - self.at(c + 1, r), 1.0, self.at(c, r) - self.at(c, r + 1)]
        };
        // : the squares are whole numbers; their sum does not depend on the order.
        let q = rsqrt_as(self.version, raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]);
        [raw[0] * q, raw[1] * q, raw[2] * q]
    }

    /// The corner V the plane is taken through: (c, H(c, r), r) for A, (c + 1, H(c + 1, r), r)
    /// for B, the height as it stands.
    fn corner(&self, c: usize, r: usize, second: bool) -> Vec3 {
        let i = if second { c + 1 } else { c };
        [i as f64, self.at(i, r), r as f64]
    }

    /// The orders of's "arithmetic of t" for one triangle: the order of the three
    /// products of P (the first two added, then the third), the order in which the origin's
    /// products join the running sum, and whether S is formed as 5.0 forms it for a mirrored
    /// terrain.
    fn t_orders(&self, second: bool) -> TOrders {
        let (x, y, z) = (0usize, 1usize, 2usize);
        let v = self.version;
        if self.mirrored {
            let p_sum = match (v, second) {
                (RenderAs::Bryce30, _) => [x, y, z],
                (RenderAs::Bryce31, _) => [x, z, y],
                (RenderAs::Bryce50, false) => [y, z, x],
                (RenderAs::Bryce50, true) => [y, x, z],
            };
            return TOrders { p_sum, joins: [y, z, x], five_mirrored: v == RenderAs::Bryce50 };
        }
        let (p_sum, joins) = match (v, second) {
            (RenderAs::Bryce30, false) => ([x, z, y], [y, z, x]),
            (RenderAs::Bryce31, false) => ([y, z, x], [z, y, x]),
            (RenderAs::Bryce50, false) => ([y, z, x], [y, x, z]),
            (RenderAs::Bryce30, true) => ([z, y, x], [y, z, x]),
            (RenderAs::Bryce31, true) => ([x, y, z], [z, y, x]),
            (RenderAs::Bryce50, true) => ([y, x, z], [y, x, z]),
        };
        TOrders { p_sum, joins, five_mirrored: false }
    }

    /// The crossing of the ray (o', d) with the plane through `v` of unit normal `nn`;
    /// `None` where the ray is parallel to it (in the plane included). (the bullet
    /// "The arithmetic of t": P the products of N with V added two and then the third,
    /// D = ((-P + p1) + p2) + p3 with the origin's products added one at a time, t = -(D / S))
    fn plane_t(o: Vec3, d: Vec3, nn: Vec3, v: Vec3, ord: TOrders) -> Option<f64> {
        let den = if ord.five_mirrored {
            (nn[0] * d[0] + nn[2] * d[2]) + nn[1] * d[1]
        } else {
            (nn[1] * d[1] + nn[2] * d[2]) + d[0] * nn[0]
        };
        if den == 0.0 || den.is_nan() {
            return None;
        }
        let [a, b, c] = ord.p_sum;
        let p = (nn[a] * v[a] + nn[b] * v[b]) + nn[c] * v[c];
        let [j, k, l] = ord.joins;
        let dd = ((-p + nn[j] * o[j]) + nn[k] * o[k]) + nn[l] * o[l];
        Some(-(dd / den))
    }

    /// Whether (x, z), measured from the grid's corner, lies in the closed triangle A
    /// (`second` false) or B of cell (c, r).
    fn in_triangle(x: f64, z: f64, c: f64, r: f64, second: bool) -> bool {
        let diagonal = c + r + 1.0;
        if second {
            x <= c + 1.0 && z <= r + 1.0 && x + z >= diagonal
        } else {
            x >= c && z >= r && x + z <= diagonal
        }
    }

    /// The triangles of cell (c, r) against the ray: the nearest crossing in `s`.
    fn cell_crossing(&self, c: usize, r: usize, o: Vec3, d: Vec3, s: Stretch, range: Range) -> Option<Crossing> {
        let mut best: Option<Crossing> = None;
        let images: &[bool] = if self.mirrored { &[false, true] } else { &[false] };
        for &image in images {
            for second in [false, true] {
                let mut nn = self.facet_normal(c, r, second);
                let mut v = self.corner(c, r, second);
                if image {
                    // : every height h replaced by 2Λ - h; the normal's y negated.
                    nn[1] = -nn[1];
                    v[1] = self.lam - (v[1] - self.lam);
                }
                let Some(t) = Relief::plane_t(o, d, nn, v, self.t_orders(second)) else { continue };
                if !(t > s.start && t <= s.end && t > range.near && t < range.far) {
                    continue;
                }
                let at = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                if !Relief::in_triangle(at[0], at[2], c as f64, r as f64, second) {
                    continue;
                }
                if self.mirrored {
                    // : the relief counts within Λ to M, the image within 2Λ - M to Λ.
                    let ok = if image {
                        at[1] >= 2.0 * self.lam - self.mu && at[1] <= self.lam
                    } else {
                        at[1] >= self.lam && at[1] <= self.mu
                    };
                    if !ok {
                        continue;
                    }
                }
                if best.is_none_or(|b| t < b.t) {
                    best = Some(Crossing { t, at, normal: nn, part: ReliefPart::Triangle { cell: [c, r], second, image } });
                }
            }
        }
        best
    }

    /// The cell index of a coordinate measured from the grid's corner: a coordinate on a grid
    /// line takes the cell with the higher number; below 0 the cell 0, from n - 1 on n - 2.
    fn cell_of(&self, x: f64) -> usize {
        let last = self.n - 2;
        if !(x >= 0.0) {
            return 0;
        }
        if x >= (self.n - 1) as f64 {
            return last;
        }
        (x.floor() as usize).min(last)
    }

    /// The nearest crossing of the relief in the stretch, by a walk over the cells under it
    /// in the ray's order. (the walk)
    fn relief_crossing(&self, o: Vec3, d: Vec3, s: Stretch, range: Range) -> Option<Crossing> {
        let start = s.start.max(range.near);
        let end = s.end.min(range.far);
        if !(start <= end) {
            return None;
        }
        let along = |a: usize, t: f64| if d[a] == 0.0 { o[a] } else { o[a] + t * d[a] };
        // The first and last cells on each axis (a ray with no x component stays in its column).
        let first = [self.cell_of(along(0, start)), self.cell_of(along(2, start))];
        let last = [self.cell_of(along(0, end)), self.cell_of(along(2, end))];
        let step = |a: usize, k: usize| -> isize {
            let (f, l) = (first[k], last[k]);
            if d[a] == 0.0 || f == l {
                0
            } else if l > f {
                1
            } else {
                -1
            }
        };
        let steps = [step(0, 0), step(2, 1)];
        let mut cell = first;
        let mut best: Option<Crossing> = None;
        // The t at which the ray enters the present cell's footprint (for the skip only).
        let mut entry = start;
        let total = first[0].abs_diff(last[0]) + first[1].abs_diff(last[1]);
        // The t at which the ray leaves the footprint of cell `at` along axis k (the line ahead).
        let exit = |k: usize, at: usize| -> f64 {
            let a = if k == 0 { 0 } else { 2 };
            if steps[k] == 0 || at == last[k] {
                return f64::INFINITY;
            }
            let line = if steps[k] > 0 { at + 1 } else { at } as f64;
            (line - o[a]) / d[a]
        };
        // The same for the cell one step further along axis k (none past the last cell).
        let ahead = |k: usize, at: usize| -> f64 {
            if steps[k] == 0 || at == last[k] {
                return f64::INFINITY;
            }
            exit(k, (at as isize + steps[k]) as usize)
        };
        // Each axis's exit is made one cell before it is needed (the same division, so the same
        // bits), so the step from cell to cell waits for no division: the recomp's walk steps by
        // multiplying with 1/d for that reason (bryce-rt terrain.rs: `inv`), which rounds
        // otherwise. (the walk)
        let (mut ex, mut ez) = (exit(0, cell[0]), exit(1, cell[1]));
        let (mut ex_next, mut ez_next) = (ahead(0, cell[0]), ahead(1, cell[1]));
        for _ in 0..=total {
            let leave = ex.min(ez);
            if self.cell_may_hold(cell[0], cell[1], o, d, entry.max(start), leave.min(end), false)
                && let Some(x) = self.cell_crossing(cell[0], cell[1], o, d, s, range)
                && best.is_none_or(|b| x.t < b.t)
            {
                best = Some(x);
            }
            // A later cell's crossing lies at or beyond the line the ray leaves through.
            if best.is_some_and(|b| b.t < leave) || cell == last {
                break;
            }
            entry = leave;
            if ex <= ez && steps[0] != 0 && cell[0] != last[0] {
                cell[0] = (cell[0] as isize + steps[0]) as usize;
                (ex, ex_next) = (ex_next, ahead(0, cell[0]));
            } else if steps[1] != 0 && cell[1] != last[1] {
                cell[1] = (cell[1] as isize + steps[1]) as usize;
                (ez, ez_next) = (ez_next, ahead(1, cell[1]));
            } else {
                // (x cannot be at its last cell here, or the walk would have ended above)
                cell[0] = (cell[0] as isize + steps[0]) as usize;
                (ex, ex_next) = (ex_next, ahead(0, cell[0]));
            }
        }
        best
    }

    /// Whether the ray's height over the cell can reach the cell's heights (or their image):
    /// a skip that only drops cells whose triangles the ray cannot meet; it widens the ray's
    /// heights generously, so rounding never drops a crossing. (the walk)
    fn cell_may_hold(&self, c: usize, r: usize, o: Vec3, d: Vec3, t0: f64, t1: f64, exact: bool) -> bool {
        let k = r * (self.n - 1) + c;
        let (mut low, mut high) = (self.cell_low[k], self.cell_high[k]);
        if self.mirrored {
            low = low.min(2.0 * self.lam - high);
            high = high.max(2.0 * self.lam - self.cell_low[k]);
        }
        let y0 = o[1] + t0 * d[1];
        let y1 = o[1] + t1 * d[1];
        let (ylo, yhi) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        // A list's vertical line never leaves its column: its heights run on without end in the
        // direction of d_y (member-lists-pixels.jsonl: t01-054 leaves a cell out that t01-059
        // keeps); only a number that is none keeps the cell. step 2
        if exact && !(y0.is_nan() || y1.is_nan()) {
            // infinite ends are compared as they are
        } else if !(ylo.is_finite() && yhi.is_finite()) {
            return true;
        }
        // step 2 (a list leaves a cell out exactly when the line's heights are
        // all above the highest corner or all below the lowest; equal is kept: no slack)
        let slack = if exact { 0.0 } else { 1e-6 * (1.0 + ylo.abs().max(yhi.abs())) };
        !(yhi + slack < low || ylo - slack > high)
    }
}

// ---------------------------------------------------------------------------------------
// The smoothed normal.
// ---------------------------------------------------------------------------------------

impl Relief {
    /// The unit corner vector at grid point (i, j).
    fn corner_vector(&self, i: usize, j: usize, image: bool) -> Vec3 {
        let (i, j) = (i as isize, j as isize);
        let gx = -2.0 * (self.at_held(i + 1, j) - self.at_held(i - 1, j));
        let gz = -2.0 * (self.at_held(i, j + 1) - self.at_held(i, j - 1));
        let v = [gx, 4.0, gz];
        let q = rsqrt_as(self.version, v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
        let mut u = [v[0] * q, v[1] * q, v[2] * q];
        if image {
            u[1] = -u[1];
        }
        u
    }

    /// The smoothed normal of cell (c, r) at the fractions (fx, fz), of unit length: the four
    /// corner vectors blended along x (`a + (b - a) fx`), then along z, then brought to unit
    /// length by q of the squared length, summed `x² + (y² + z²)` as 3.0 and 3.1 and
    /// `(x² + y²) + z²` as 5.0 (the vectors decide the orders, Accuracy).
    /// . (last bits)
    fn smoothed(&self, c: usize, r: usize, fx: f64, fz: f64, image: bool) -> Vec3 {
        let a = self.corner_vector(c, r, image);
        let b = self.corner_vector(c + 1, r, image);
        let g = self.corner_vector(c, r + 1, image);
        let e = self.corner_vector(c + 1, r + 1, image);
        let lerp = |p: f64, q: f64, f: f64| p + (q - p) * f;
        let m: Vec3 = std::array::from_fn(|k| lerp(lerp(a[k], b[k], fx), lerp(g[k], e[k], fx), fz));
        let len2 = match self.version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => m[0] * m[0] + (m[1] * m[1] + m[2] * m[2]),
            RenderAs::Bryce50 => (m[0] * m[0] + m[1] * m[1]) + m[2] * m[2],
        };
        let q = rsqrt_as(self.version, len2);
        [m[0] * q, m[1] * q, m[2] * q]
    }
}

// ---------------------------------------------------------------------------------------
// The closing faces.
// ---------------------------------------------------------------------------------------

/// Where a face lies and what part of it counts.
#[derive(Clone, Copy, Debug)]
struct FacePlane {
    face: ReliefFace,
    /// The plane's coordinate across the face, in the terrain's own space.
    at: f64,
    /// For top and bottom faces, the height (from the grid's corner) the point is tested at
    /// for lying under the surface; `None`: the point's own.
    test_y: Option<f64>,
}

impl Relief {
    /// The closing face's plane (the closing word's faces).
    fn closing_plane(&self, face: ReliefFace) -> FacePlane {
        let half = self.half();
        let at = match face {
            ReliefFace::Top => {
                if self.mirrored {
                    self.mu_held - FACE_OFFSET
                } else {
                    self.mu_held - HALF_RANGE - FACE_OFFSET
                }
            }
            ReliefFace::Bottom => {
                if self.mirrored {
                    2.0 * self.lam_held - self.mu_held + FACE_OFFSET
                } else {
                    self.lam_held - HALF_RANGE + FACE_OFFSET
                }
            }
            ReliefFace::LowX | ReliefFace::LowZ => -half,
            ReliefFace::HighX | ReliefFace::HighZ => half - 1.0,
        };
        FacePlane { face, at, test_y: None }
    }

    /// The solid word's face's plane.
    fn solid_plane(&self, face: ReliefFace) -> FacePlane {
        let off = if self.version == RenderAs::Bryce50 { 0.0 } else { SOLID_OFFSET_3X };
        let (lam, mu) = (self.lam_held, self.mu_held);
        let shift = self.y_shift();
        match face {
            ReliefFace::Top => FacePlane { face, at: (mu - shift) + off, test_y: Some(mu) },
            ReliefFace::Bottom => {
                let floor = if self.mirrored { 2.0 * lam - mu } else { lam };
                FacePlane { face, at: (floor - shift) - off, test_y: Some(floor) }
            }
            _ => self.closing_plane(face),
        }
    }

    /// A ray's crossing with a face: t, the point in the terrain's own space; `None` where
    /// the face is not met.
    fn face_crossing(&self, p: FacePlane, r: &Ray) -> Option<(f64, Vec3)> {
        let a = p.face.axis();
        let d = r.dir[a];
        if !(d.abs() > FACE_LIMIT) {
            return None;
        }
        let t = (p.at - r.origin[a]) / d;
        let mut point = r.at(t);
        point[a] = p.at;
        let half = self.half();
        let within = |v: f64| v >= -half && v <= half - 1.0;
        let ok = match a {
            1 => within(point[0]) && within(point[2]),
            _ => {
                let other = if a == 0 { point[2] } else { point[0] };
                let y = if self.mirrored { SIDE_Y_MIRRORED } else { HALF_RANGE };
                within(other) && point[1] >= -y && point[1] <= y
            }
        };
        if !ok {
            return None;
        }
        let ty = match p.test_y {
            Some(y) => y,
            None => point[1] + self.y_shift(),
        };
        self.under([point[0] + half, ty, point[2] + half]).then_some((t, point))
    }

    /// Whether a point measured from the grid's corner (x, z in cells, y a height from 0)
    /// lies under the surface.
    pub fn under(&self, p: Vec3) -> bool {
        self.under_bounded(p, self.lam, self.mu)
    }

    /// 's test with the bounds Λ and M given (puts other numbers in their
    /// place for the eye).
    fn under_bounded(&self, p: Vec3, lam: f64, mu: f64) -> bool {
        let [mut x, y, mut z] = p;
        let floor = if self.mirrored { 2.0 * lam - mu } else { lam };
        if !(y <= mu + UNDER_SLACK && y >= floor - UNDER_SLACK) {
            return false;
        }
        let nf = self.n as f64;
        if !(x >= 0.0 && x <= nf && z >= 0.0 && z <= nf) {
            return false;
        }
        let last = (self.n - 1) as f64;
        let cell = |v: &mut f64| -> usize {
            if *v > last {
                *v -= 1.0;
            }
            if *v >= last { self.n - 2 } else { (v.floor() as usize).min(self.n - 2) }
        };
        let c = cell(&mut x);
        let r = cell(&mut z);
        let (a, b, e, g) = (self.at(c, r), self.at(c + 1, r), self.at(c + 1, r + 1), self.at(c, r + 1));
        let highest = a.max(b).max(e).max(g);
        let lowest = a.min(b).min(e).min(g);
        if y > highest {
            return false;
        }
        if self.mirrored && y < 2.0 * lam - highest {
            return false;
        }
        if y < lowest && (!self.mirrored || y > 2.0 * lam - lowest) {
            return true;
        }
        let (fx, fz) = (x - c as f64, z - r as f64);
        let h = if (c + r + 1) as f64 <= x + z {
            (g - e) * (1.0 - fx) + (e - b) * fz + b
        } else {
            (g - a) * fz + a + (b - a) * fx
        };
        if y > h {
            return false;
        }
        !(self.mirrored && y < 2.0 * lam - h)
    }
}

// ---------------------------------------------------------------------------------------
// The hit.
// ---------------------------------------------------------------------------------------

/// u = (n/2 + x) / n, v = (n/2 + z) / n; du = (1, 0, 0), dv = (0, 0, 1).
fn top_texture(half: f64, n: f64, x: f64, z: f64) -> ([f64; 2], Vec3, Vec3) {
    ([(half + x) / n, (half + z) / n], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
}

impl Relief {
    /// The nearest hit of a ray in the terrain's own space: the relief, then the closing
    /// faces. The local hit's `shade` is the shading normal (unit length, own space).
    pub fn hit(&self, r: &Ray, range: Range) -> Option<LocalHit> {
        // : a ray that misses the box meets no face (nor the relief).
        let boxed = self.box_stretch(r)?;
        let shift = self.y_shift();
        let half = self.half();
        let o = [r.origin[0] + half, r.origin[1] + shift, r.origin[2] + half];
        let mut best: Option<(f64, Vec3, Vec3, Vec3, ReliefPart)> = None;
        {
            let s = self.cut_by_bounds(boxed, r);
            if let Some(x) = self.relief_crossing(o, r.dir, s, range) {
                let point = [x.at[0] - half, x.at[1] - shift, x.at[2] - half];
                let ReliefPart::Triangle { cell: [c, rr], image, .. } = x.part else { unreachable!() };
                let shade = if self.smooth {
                    self.smoothed(c, rr, x.at[0] - c as f64, x.at[2] - rr as f64, image)
                } else {
                    x.normal
                };
                best = Some((x.t, point, x.normal, shade, x.part));
            }
        }
        for face in ReliefFace::ALL {
            if self.closing & face.bit() == 0 {
                continue;
            }
            let Some((t, point)) = self.face_crossing(self.closing_plane(face), r) else { continue };
            let nearer = match best {
                Some(b) => t < b.0,
                None => t < range.far,
            };
            if t > range.near && nearer {
                best = Some((t, point, face.normal(), face.normal(), ReliefPart::Face(face)));
            }
        }
        let (t, point, normal, shade, part) = best?;
        let (uv, du, dv) = top_texture(half, self.n as f64, point[0], point[2]);
        Some(LocalHit { t, point, normal, part: Part::Relief(part), uv, du, dv, shade: Some(shade), meta: None })
    }

    /// Every face of the solid word the ray's line meets, in the order top, bottom, low x,
    /// high x, low z, high z, with no interval on t.
    pub fn solid_faces(&self, r: &Ray) -> Vec<ListedFace> {
        let mut out = Vec::new();
        if self.box_stretch(r).is_none() {
            return out;
        }
        let half = self.half();
        let nf = self.n as f64;
        for face in ReliefFace::ALL {
            if self.solid & face.bit() == 0 {
                continue;
            }
            let Some((t, p)) = self.face_crossing(self.solid_plane(face), r) else { continue };
            let (uv, du, dv) = match face.axis() {
                1 => top_texture(half, nf, p[0], p[2]),
                0 => ([(p[2] + half) / nf, (p[1] + half) / nf], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
                _ => ([(p[0] + half) / nf, (p[1] + half) / nf], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            };
            out.push(ListedFace {
                face,
                visible: self.closing & face.bit() != 0,
                t,
                point: p,
                normal: face.normal(),
                uv,
                du,
                dv,
            });
        }
        out
    }

    /// The record of a relief crossing, as `hit` makes it.
    fn relief_record(&self, x: &Crossing) -> LocalHit {
        let (half, shift) = (self.half(), self.y_shift());
        let point = [x.at[0] - half, x.at[1] - shift, x.at[2] - half];
        let ReliefPart::Triangle { cell: [c, rr], image, .. } = x.part else { unreachable!() };
        let shade =
            if self.smooth { self.smoothed(c, rr, x.at[0] - c as f64, x.at[2] - rr as f64, image) } else { x.normal };
        let (uv, du, dv) = top_texture(half, self.n as f64, point[0], point[2]);
        LocalHit { t: x.t, point, normal: x.normal, part: Part::Relief(x.part), uv, du, dv, shade: Some(shade), meta: None }
    }

    /// Every crossing of the line with the triangles of cell (c, r) (and their images), the
    /// first triangle then the second, wherever along the line.
    fn cell_all(&self, c: usize, r: usize, o: Vec3, d: Vec3) -> Vec<Crossing> {
        let mut out = Vec::new();
        let images: &[bool] = if self.mirrored { &[false, true] } else { &[false] };
        for &image in images {
            for second in [false, true] {
                let mut nn = self.facet_normal(c, r, second);
                let mut v = self.corner(c, r, second);
                if image {
                    nn[1] = -nn[1];
                    v[1] = self.lam - (v[1] - self.lam);
                }
                let Some(t) = Relief::plane_t(o, d, nn, v, self.t_orders(second)) else { continue };
                let at = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                if !Relief::in_triangle(at[0], at[2], c as f64, r as f64, second) {
                    continue;
                }
                if self.mirrored {
                    let ok = if image {
                        at[1] >= 2.0 * self.lam - self.mu && at[1] <= self.lam
                    } else {
                        at[1] >= self.lam && at[1] <= self.mu
                    };
                    if !ok {
                        continue;
                    }
                }
                out.push(Crossing { t, at, normal: nn, part: ReliefPart::Triangle { cell: [c, r], second, image } });
            }
        }
        out
    }

    /// : every crossing of the ray's line (in the terrain's own space) with the terrain
    /// as a member of a combined group: the relief's, cell by cell along the stretch, then the
    /// faces of the solid word; none unless there are two or more; entries of one t as 3.0 and
    /// 3.1 leave them out (an entry whose t equals the entry before it, or either of whose t
    /// is not a number), nearest first, stable; as 5.0 by insertion with no sort, and a lone
    /// entry gets a hidden partner.
    pub fn list(&self, r: &Ray) -> Vec<super::crossings::LocalCrossing> {
        use super::crossings::LocalCrossing;
        // Step 1: the box, then the bound planes.
        let Some(boxed) = self.box_stretch(r) else { return Vec::new() };
        let s = self.cut_by_bounds(boxed, r);
        let (half, shift) = (self.half(), self.y_shift());
        let o = [r.origin[0] + half, r.origin[1] + shift, r.origin[2] + half];
        let d = r.dir;
        let mut entries: Vec<LocalCrossing> = Vec::new();
        // Step 2: the cells the stretch passes over, in the order of the walk (cell to cell
        // along the line); a cell the line's heights cannot reach is left out.
        if s.start.is_finite() && s.end.is_finite() && s.start <= s.end {
            let along = |a: usize, t: f64| if d[a] == 0.0 { o[a] } else { o[a] + t * d[a] };
            let first = [self.cell_of(along(0, s.start)), self.cell_of(along(2, s.start))];
            let last = [self.cell_of(along(0, s.end)), self.cell_of(along(2, s.end))];
            let mut cell = [first[0] as isize, first[1] as isize];
            let mut entry = s.start;
            let cells = (self.n - 1) as isize;
            for _ in 0..(2 * self.n + 4) {
                if cell[0] < 0 || cell[1] < 0 || cell[0] >= cells || cell[1] >= cells {
                    break;
                }
                let cu = [cell[0] as usize, cell[1] as usize];
                let column = |k: usize| -> f64 {
                    let a = if k == 0 { 0 } else { 2 };
                    if d[a] == 0.0 {
                        return f64::INFINITY;
                    }
                    let line = if d[a] > 0.0 { cu[k] + 1 } else { cu[k] } as f64;
                    (line - o[a]) / d[a]
                };
                let (ex, ez) = (column(0), column(1));
                let column_exit = ex.min(ez);
                if self.cell_may_hold(cu[0], cu[1], o, d, entry, column_exit, true) {
                    for x in self.cell_all(cu[0], cu[1], o, d) {
                        entries.push(LocalCrossing::shown(self.relief_record(&x)));
                    }
                }
                if cu == last || column_exit >= 0.9999999 * s.end {
                    break;
                }
                entry = column_exit;
                if column_exit.is_infinite() {
                    break;
                }
                if ex <= ez {
                    cell[0] += if d[0] > 0.0 { 1 } else { -1 };
                } else {
                    cell[1] += if d[2] > 0.0 { 1 } else { -1 };
                }
            }
        }
        // Step 3: the faces of the solid word.
        for f in self.solid_faces(r) {
            let hit = LocalHit {
                t: f.t,
                point: f.point,
                normal: f.normal,
                part: Part::Relief(ReliefPart::Face(f.face)),
                uv: f.uv,
                du: f.du,
                dv: f.dv,
                shade: Some(f.normal),
                meta: None,
            };
            entries.push(if f.visible { LocalCrossing::shown(hit) } else { LocalCrossing::hidden(f.t) });
        }
        // Step 4.
        if entries.len() < 2 {
            return Vec::new();
        }
        if self.version == RenderAs::Bryce50 {
            // : as 5.0 each entry is inserted into the kept list at its
            // place by t, an entry whose t equals (or is no number) any kept one is dropped, no
            // sort; exactly one kept gets a second at t + 0.00001 that never shows.
            let mut kept: Vec<LocalCrossing> = Vec::with_capacity(entries.len());
            for e in &entries {
                let t = e.hit.t;
                if t.is_nan() || kept.iter().any(|k| k.hit.t == t) {
                    continue;
                }
                let at = kept.iter().position(|k| k.hit.t > t).unwrap_or(kept.len());
                kept.insert(at, *e);
            }
            if kept.len() == 1 {
                kept.push(LocalCrossing::hidden(kept[0].hit.t + 0.00001));
            }
            return kept;
        }
        // Step 5 (3.0 and 3.1): equal t to the entry just before, or a number that is none.
        let mut kept: Vec<LocalCrossing> = Vec::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            let t = e.hit.t;
            let drop = t.is_nan() || (i > 0 && (entries[i - 1].hit.t.is_nan() || entries[i - 1].hit.t == t));
            if !drop {
                kept.push(*e);
            }
        }
        kept.sort_by(|a, b| a.hit.t.total_cmp(&b.hit.t));
        kept
    }

    /// Whether the terrain holds the eye at a point of its own space, in a render on opening:
    /// as 5.0 as `holds`; as 3.1 with the bounds -0.00001 n and 1.00001 n in place of Λ and M;
    /// as 3.0 never (the scene gives nothing where the original's test passes and it holds the
    /// eye at no other point: our own choice, Freedom).
    pub fn holds_eye(&self, p: Vec3) -> bool {
        match self.version {
            RenderAs::Bryce30 => false,
            RenderAs::Bryce50 => self.holds(p),
            RenderAs::Bryce31 => {
                if !(self.closed() || self.holds_without_faces) {
                    return false;
                }
                let half = self.half();
                let n = self.n as f64;
                self.under_bounded(
                    [p[0] + half, p[1] + self.y_shift(), p[2] + half],
                    EYE_BOUND_LOW * n,
                    EYE_BOUND_HIGH * n,
                )
            }
        }
    }

    /// Whether the terrain holds a point of its own space.
    pub fn holds(&self, p: Vec3) -> bool {
        let holds_any = match self.version {
            RenderAs::Bryce30 => self.closed(),
            RenderAs::Bryce31 | RenderAs::Bryce50 => self.closed() || self.holds_without_faces,
        };
        if !holds_any {
            return false;
        }
        match self.version {
            // : as 3.0 the test is made on the point as it is.
            RenderAs::Bryce30 => self.under(p),
            RenderAs::Bryce31 | RenderAs::Bryce50 => {
                let half = self.half();
                self.under([p[0] + half, p[1] + self.y_shift(), p[2] + half])
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// The boxes.
// ---------------------------------------------------------------------------------------

impl Relief {
    /// The terrain's own box (low, high).
    pub fn own_box(&self) -> (Vec3, Vec3) {
        let e = self.half() + BOX_MARGIN;
        ([-e, self.own_y[0], -e], [e, self.own_y[1], e])
    }

    /// The box the terrain gives the scene's bounds, in its own space (carried by the matrix
    /// by the caller): as 3.0 the own box; as 3.1 and 5.0 the bounds' box.
    pub fn scene_box(&self) -> (Vec3, Vec3) {
        let (mut low, mut high) = self.own_box();
        if self.version == RenderAs::Bryce30 {
            return (low, high);
        }
        let [lower, upper] = self.box_bounds(self.bounds[1]);
        (low[1], high[1]) = self.box_heights(lower, upper);
        (low, high)
    }

    /// λ and μ as holds them for the boxes (between -0.00001 and 1.00001, a bound
    /// that is not a number counting as the lowest), with `upper` for μ.
    fn box_bounds(&self, upper: f64) -> [f64; 2] {
        [held(self.bounds[0], BOX_BOUND_LOW, BOX_BOUND_HIGH), held(upper, BOX_BOUND_LOW, BOX_BOUND_HIGH)]
    }

    /// The y extent of the bounds' box from the held bounds a and b: Λ' = 65535 a, and for a
    /// mirrored terrain d = 65535 (b - a).. (the boxes'
    /// last bits: the vectors give these orders)
    fn box_heights(&self, a: f64, b: f64) -> (f64, f64) {
        let lam = TOP * a;
        if self.mirrored {
            let d = TOP * (b - a);
            ((lam - d) - BOX_MARGIN, (lam + d) + BOX_MARGIN)
        } else {
            ((lam - HALF_RANGE) - BOX_MARGIN, (TOP * b - HALF_RANGE) + BOX_MARGIN)
        }
    }

    /// The box a 5.0 group asks of a terrain that is its member.
    pub fn group_box(&self) -> (Vec3, Vec3) {
        let (mut low, mut high) = self.scene_box();
        let highest = self.h.iter().copied().fold(0.0, f64::max);
        let [lower, mut upper] = self.bounds;
        let top = highest / TOP + GROUP_TOP_SLACK;
        if top < upper {
            upper = top;
        }
        let lo_cut = (lower * TOP).trunc();
        let hi_cut = (upper * TOP).trunc();
        let n = self.n;
        let (mut c0, mut c1, mut r0, mut r1) = (usize::MAX, 0, usize::MAX, 0);
        for j in 0..n {
            for i in 0..n {
                let v = self.at(i, j);
                if v >= lo_cut && v <= hi_cut {
                    c0 = c0.min(i);
                    c1 = c1.max(i);
                    r0 = r0.min(j);
                    r1 = r1.max(j);
                }
            }
        }
        if c0 == usize::MAX {
            (c0, c1, r0, r1) = (n / 2, n / 2, n / 2, n / 2);
        }
        let half = self.half();
        let x0 = (c0 as f64 - half - 1.0).max(-half);
        let x1 = (c1 as f64 - half + 1.0).min(half);
        let z0 = (r0 as f64 - half - 1.0).max(-half);
        let z1 = (r1 as f64 - half + 1.0).min(half);
        let [a, b] = self.box_bounds(upper);
        (low[1], high[1]) = self.box_heights(a, b);
        low[0] = x0 - BOX_MARGIN;
        high[0] = x1 + BOX_MARGIN;
        low[2] = z0 - BOX_MARGIN;
        high[2] = z1 + BOX_MARGIN;
        (low, high)
    }
}

/// A terrain's normal (or shading normal) in its own space carried to the world as:
/// by the transpose of the inverse of the object's matrix, then brought to unit length by q of
/// its squared length, summed `x² + (y² + z²)` as 3.0 and 3.1 and `(x² + z²) + y²` as 5.0,
/// for the facet's normal and the shading normal alike (the vectors decide the orders,
/// Accuracy).. (last bits)
/// A listed crossing's normal in the world (a terrain held by a group): carried as the hit's,
/// its squares added as the hit adds them as 3.0 and 3.1, and as (x^2 + y^2) + z^2 as 5.0.
pub fn world_normal_listed(placement: &super::Placement, n: Vec3, version: RenderAs) -> Vec3 {
    if version != RenderAs::Bryce50 {
        return world_normal(placement, n, version);
    }
    let v = placement.carry_normal(n, version);
    let q = rsqrt_as(version, (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]);
    [v[0] * q, v[1] * q, v[2] * q]
}

pub fn world_normal(placement: &super::Placement, n: Vec3, version: RenderAs) -> Vec3 {
    let v = placement.carry_normal(n, version);
    let len2 = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => v[0] * v[0] + (v[1] * v[1] + v[2] * v[2]),
        RenderAs::Bryce50 => (v[0] * v[0] + v[2] * v[2]) + v[1] * v[1],
    };
    let q = rsqrt_as(version, len2);
    [v[0] * q, v[1] * q, v[2] * q]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small generator for the test's grids and rays (tests only).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn rough(n: usize, spread: f64, g: &mut Lcg) -> Vec<u16> {
        (0..n * n).map(|_| (32768.0 + (g.next() - 0.5) * spread) as u16).collect()
    }

    /// Every cell under the stretch tested, no walk and no skip:'s hit as the entry
    /// states it (the README of the program that tests every cell under the ray's
    /// stretch, with no pyramid, gives every row").
    fn every_cell(t: &Relief, r: &Ray, range: Range) -> Option<(f64, ReliefPart)> {
        let s = t.cut_by_bounds(t.box_stretch(r)?, r);
        let half = t.half();
        let o = [r.origin[0] + half, r.origin[1] + t.y_shift(), r.origin[2] + half];
        let mut best: Option<Crossing> = None;
        for c in 0..t.n - 1 {
            for rr in 0..t.n - 1 {
                if let Some(x) = t.cell_crossing(c, rr, o, r.dir, s, range)
                    && best.is_none_or(|b| x.t < b.t)
                {
                    best = Some(x);
                }
            }
        }
        best.map(|b| (b.t, b.part))
    }

    /// The walk over the cells with its skip finds the crossing that testing every cell finds,
    /// for rays in every direction over rough, steep and mirrored terrains.
    /// (the walk)
    #[test]
    fn walk_finds_what_every_cell_finds() {
        let mut g = Lcg(7);
        for (n, spread, mirrored) in [(16, 200.0, false), (32, 60000.0, false), (16, 300.0, true)] {
            let sw = ReliefSwitches { mirrored, smooth: true, closing: 0, solid: 0, holds_without_faces: false };
            let bounds = if mirrored { [0.5, 0.5005] } else { [-0.00001, 1.00001] };
            let t = Relief::new(RenderAs::Bryce31, n, &rough(n, spread, &mut g), sw, bounds);
            let ybase = if mirrored { 32768.0 } else { 0.0 };
            for _ in 0..3000 {
                let half = (n / 2) as f64;
                let origin = [(g.next() - 0.5) * 3.0 * half, ybase + (g.next() - 0.5) * spread * 1.5, (g.next() - 0.5) * 3.0 * half];
                let target = [(g.next() - 0.5) * 2.0 * half, ybase + (g.next() - 0.5) * spread, (g.next() - 0.5) * 2.0 * half];
                let dir = [target[0] - origin[0], target[1] - origin[1], target[2] - origin[2]];
                let r = Ray { origin, dir };
                let range = Range { near: 0.0, far: f64::INFINITY };
                let walked = t.box_stretch(&r).and_then(|b| {
                    let s = t.cut_by_bounds(b, &r);
                    let o = [r.origin[0] + half, r.origin[1] + t.y_shift(), r.origin[2] + half];
                    t.relief_crossing(o, r.dir, s, range).map(|x| (x.t, x.part))
                });
                assert_eq!(walked, every_cell(&t, &r, range), "n {n}, ray {r:?}");
            }
        }
    }

    /// 's world box for a placed terrain, and the test of a world ray against it.
    #[test]
    fn world_box_of_a_placed_terrain() {
        let sw = ReliefSwitches { mirrored: false, smooth: true, closing: 0, solid: 0, holds_without_faces: false };
        let mut t = Relief::new(RenderAs::Bryce31, 16, &[32768; 256], sw, [0.5, 0.75]);
        let p = super::super::Placement::new([[2.0, 0.0, 0.0], [0.0, 0.001, 0.0], [0.0, 0.0, 2.0]], [0.0, 10.0, 0.0])
            .unwrap();
        t.place(&p);
        let (low, high) = t.world_box.unwrap();
        assert_eq!((low[0], high[0]), (-16.02, 16.02));
        // y from (Λ - 32767.5) - 0.01 to (M - 32767.5) + 0.01, times 0.001, plus 10.
        assert!((low[1] - (10.0 + 0.001 * (65535.0 * 0.5 - 32767.5 - 0.01))).abs() < 1e-12);
        assert!((high[1] - (10.0 + 0.001 * (65535.0 * 0.75 - 32767.5 + 0.01))).abs() < 1e-12);
        let range = Range { near: 0.0, far: f64::INFINITY };
        let level = |y: f64| Ray { origin: [-40.0, y, 0.0], dir: [1.0, 0.0, 0.0] };
        assert!(t.world_box_met(&level(10.0), range));
        assert!(!t.world_box_met(&level(30.0), range));
        // As 3.0 the own box: every height of the relief.
        let mut t30 = Relief::new(RenderAs::Bryce30, 16, &[32768; 256], sw, [0.5, 0.75]);
        t30.place(&p);
        assert!(t30.world_box_met(&level(30.0), range));
    }
}
