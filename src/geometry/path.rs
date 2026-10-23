// SPDX-License-Identifier: MIT
//! A path that rays meet: the path's curve cut into chords, and a ray met where it passes
//! within the band's half width of a chord.
//!
//! (the chords, the band, the first chord met, t not held to the range, the
//! normal −d, the surface coordinates 0; The box),,
//! (Paths)

use crate::animation::Path;
use crate::geom::Vec3;
use crate::numerics::sqrt_as;
use crate::version::RenderAs;

/// Half the width of the line a drawn path is: a ray within this of a chord meets it.
pub const BAND: f64 = 0.075;
/// A chord shorter than this, or a ray this near parallel to it, is passed over.
const LEAST: f64 = 1e-10;
/// The step between a curved stretch's chord ends, accumulated in binary64.
const STEP: f64 = 0.05;
/// A curved stretch's chords.
const CHORDS_A_STRETCH: usize = 20;

/// C as writes its components: (A_z·d_y − A_y·d_z, A_x·d_z − A_z·d_x,
/// A_y·d_x − A_x·d_y).
fn c_of(a: Vec3, d: Vec3) -> Vec3 {
    [a[2] * d[1] - a[1] * d[2], a[0] * d[2] - a[2] * d[0], a[1] * d[0] - a[0] * d[1]]
}

/// The cross product W × d and W × A of s and t: (a_y·b_z − a_z·b_y, a_z·b_x − a_x·b_z,
/// a_x·b_y − a_y·b_x). names them without components; with C's components as
/// written, this is the one that gives the entry's example (s within the chord, t = 3.995).
/// (Paths: the signs of the cross products)
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The sum of the three products a_j b_j taken in the order of the axes `order`, added
/// (first + second) + third.
fn dot_in(order: [usize; 3], a: Vec3, b: Vec3) -> f64 {
    let [i, j, k] = order;
    (a[i] * b[i] + a[j] * b[j]) + a[k] * b[k]
}

const XYZ: [usize; 3] = [0, 1, 2];
const YZX: [usize; 3] = [1, 2, 0];
const ZYX: [usize; 3] = [2, 1, 0];
const XZY: [usize; 3] = [0, 2, 1];

/// A sum of three squares in the order (y·y + z·z) + x·x: n in every version.
fn squares_yz_x(a: Vec3) -> f64 {
    dot_in(YZX, a, a)
}

/// The squared lengths under L and under the distance: (y² + z²) + x² as 3.0 and 3.1,
/// (x² + y²) + z² as 5.0.
fn length_squares(v: RenderAs, a: Vec3) -> f64 {
    match v {
        RenderAs::Bryce30 | RenderAs::Bryce31 => dot_in(YZX, a, a),
        RenderAs::Bryce50 => dot_in(XYZ, a, a),
    }
}

/// The orders of the sums of three products in s and in t: x, y, z as 3.0 and 3.1; as 5.0
/// z, y, x (both) for a chord of a straight stretch, and x, z, y for s and z, y, x for t for
/// a chord of a curved one.
fn product_orders(v: RenderAs, curved: bool) -> ([usize; 3], [usize; 3]) {
    match (v, curved) {
        (RenderAs::Bryce30 | RenderAs::Bryce31, _) => (XYZ, XYZ),
        (RenderAs::Bryce50, false) => (ZYX, ZYX),
        (RenderAs::Bryce50, true) => (XZY, ZYX),
    }
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Whether a number is far enough from 0: at most −1e-10 or at least 1e-10 (a number that is
/// not a number is not).
fn away_from_zero(x: f64) -> bool {
    x <= -LEAST || x >= LEAST
}

/// A path's chords in its own space, in the order they are tried, as a version draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PathChords {
    pub version: RenderAs,
    /// Each chord's first and second end.
    pub chords: Vec<[Vec3; 2]>,
    /// Whether each chord is one of a curved stretch's (the orders of the sums as 5.0).
    pub curved: Vec<bool>,
    /// The points whose box is the path's box in the walk (The box), in its own space.
    pub box_points: Vec<Vec3>,
}

/// Where a ray meets a path, in the path's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathHit {
    /// The distance along the ray (in units of d); not held to the range.
    pub t: f64,
    /// P = o + t·d.
    pub point: Vec3,
    /// Which chord, in the order tried.
    pub chord: usize,
}

impl PathChords {
    /// The chords, stretch by stretch: a straight stretch (its first knot without the curved
    /// flag) one chord between its knots; a curved one twenty, through the curve at the
    /// multiples of 0.05 accumulated in binary64 and taken as binary32, the twentieth at 1.
    /// The number of stretches and the curve are the version's .
    pub fn new(path: &Path, version: RenderAs) -> PathChords {
        let n = path.knots.len();
        let mut chords = Vec::new();
        let mut curved = Vec::new();
        for k in 0..path.stretch_count(version) {
            let first = path.knots[k].place;
            if !path.knots[k].curved {
                chords.push([first, path.knots[(k + 1) % n].place]);
                curved.push(false);
                continue;
            }
            let mut a = first;
            let mut acc = 0.0f64;
            for i in 1..=CHORDS_A_STRETCH {
                acc += STEP;
                let f = if i == CHORDS_A_STRETCH { 1.0 } else { acc as f32 as f64 };
                let b = path.place_on_stretch(version, k, f);
                chords.push([a, b]);
                curved.push(true);
                a = b;
            }
        }
        PathChords { version, chords, curved, box_points: path.box_points(version) }
    }

    /// The first chord the ray (origin o, direction d, both in the path's own space) passes
    /// within the band of; `None` where it passes none. The range is not asked: the answer's
    /// t may lie before the origin or beyond any range.
    pub fn hit(&self, o: Vec3, d: Vec3) -> Option<PathHit> {
        let v = self.version;
        for (index, &[a, b]) in self.chords.iter().enumerate() {
            let ab = sub(b, a);
            let l = sqrt_as(v, length_squares(v, ab));
            let (s_order, t_order) = product_orders(v, self.curved[index]);
            if !away_from_zero(l) {
                continue;
            }
            let axis = [ab[0] / l, ab[1] / l, ab[2] / l];
            let w = sub(o, a);
            let c = c_of(axis, d);
            let n = squares_yz_x(c);
            if !away_from_zero(n) {
                continue;
            }
            let s = -dot_in(s_order, c, cross(w, d)) / n;
            if !(s >= 0.0 && s <= l) {
                continue;
            }
            let t = -dot_in(t_order, cross(w, axis), c) / n;
            let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
            let q = [a[0] + s * axis[0], a[1] + s * axis[1], a[2] + s * axis[2]];
            let pq = sub(p, q);
            if sqrt_as(v, length_squares(v, pq)) <= BAND {
                return Some(PathHit { t, point: p, chord: index });
            }
        }
        None
    }
}

/// The path's box points carried by the object's matrix (rows) and translation, as a point
/// is carried to the world (p·rows + translation), and the box of them before
/// widens it: the lowest corner, then the highest.,
pub fn carried_box(path: &Path, version: RenderAs, rows: &[Vec3; 3], translation: Vec3) -> (Vec3, Vec3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in path.box_points(version) {
        let w: Vec3 = std::array::from_fn(|j| p[0] * rows[0][j] + p[1] * rows[1][j] + p[2] * rows[2][j] + translation[j]);
        for a in 0..3 {
            lo[a] = lo[a].min(w[a]);
            hi[a] = hi[a].max(w[a]);
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Knot;

    fn knot(place: Vec3) -> Knot {
        Knot { place, tension: 0.0, continuity: 0.0, bias: 0.0, roll: 0.0, curved: false, handle_out: None, handle_in: None }
    }

    /// 's example: two straight knots (0, 0, 0) and (6, 2, −3); a ray from
    /// (2, 0.7, −5) along z meets at t = 3.995; from (2, 0.8, −5) it misses.
    #[test]
    fn the_entry_example() {
        let path = Path { knots: vec![knot([0.0; 3]), knot([6.0, 2.0, -3.0])], closed: false };
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let c = PathChords::new(&path, v);
            assert_eq!(c.chords.len(), 1);
            let h = c.hit([2.0, 0.7, -5.0], [0.0, 0.0, 1.0]).expect("a hit");
            assert!((h.t - 3.995).abs() < 1e-12, "{}", h.t);
            assert!((h.point[2] + 1.005).abs() < 1e-12);
            assert!(c.hit([2.0, 0.8, -5.0], [0.0, 0.0, 1.0]).is_none());
        }
    }
}
