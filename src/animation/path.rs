// SPDX-License-Identifier: MIT
//! Paths of motion: the place a number of the way along a path, and the orientation of an
//! object that faces along it.
//!
//! (the rules path.jsonl fixes; the facing)

use super::Quat;
use super::place::bezier;
use crate::geom::Vec3;
use crate::numerics::{rsqrt_as, sqrt_as, x87};
use crate::version::RenderAs;

/// The weight of 3.0's rules: a quarter.
pub const QUARTER: f64 = 0.25;

/// The weight of 3.1 and later: 0.19, as the binary32 nearest it.
/// ; animation (path.jsonl:
/// the binary32 0.19 meets every place, the binary64 0.19 does not)
pub const NINETEEN_HUNDREDTHS: f64 = 0.19_f32 as f64;

/// A path's knot, in the path's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knot {
    pub place: Vec3,
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
    /// Roll in radians. animation (path.jsonl: radians fit)
    pub roll: f32,
    /// Whether the stretch that starts at this knot is curved.
    pub curved: bool,
    /// 3.1 and 5.0: the knot's own vector for the first inner point of the stretch that starts here (its flag word's
    /// bit 4, the vector at offset 72 of the knot), added to its place.,
    pub handle_out: Option<Vec3>,
    /// The knot's own vector for the second inner point of the stretch that ends here (bit 2, offset 48).
    pub handle_in: Option<Vec3>,
}

/// A path object.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub knots: Vec<Knot>,
    pub closed: bool,
}

/// The place at a distance along a path, and the facing orientation there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSample {
    pub place: Vec3,
    pub facing: Quat,
}

/// One stretch's four Bézier points, or a straight stretch's two ends.
enum Stretch {
    Straight(Vec3, Vec3),
    Curved([Vec3; 4]),
}

fn r32(x: f64) -> f64 {
    x as f32 as f64
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

impl Path {
    /// The number of stretches. (Variants: 3.0 n - 1 always; 3.1 and later n
    /// for a closed path)
    pub fn stretch_count(&self, version: RenderAs) -> usize {
        let n = self.knots.len();
        if self.closed && version != RenderAs::Bryce30 { n } else { n.saturating_sub(1) }
    }

    /// The four points of stretch k.
    ///
    /// animation (path.jsonl: 3.0's closed seam; 0.19 in binary32; the rounding
    /// of 1 - t in the first inner point)
    fn stretch(&self, version: RenderAs, k: usize) -> Stretch {
        let n = self.knots.len();
        let i1 = (k + 1) % n;
        let kn0 = &self.knots[k];
        let kn1 = &self.knots[i1];
        let p0 = kn0.place;
        let p3 = kn1.place;
        if !kn0.curved {
            return Stretch::Straight(p0, p3);
        }
        let v30 = version == RenderAs::Bryce30;
        let q = if v30 { QUARTER } else { NINETEEN_HUNDREDTHS };
        if n == 2 {
            let h = if v30 { 0.5 } else { q };
            let p1 = [0, 1, 2].map(|j| p0[j] + (p3[j] - p0[j]) * h);
            let p2 = [0, 1, 2].map(|j| p3[j] + (p0[j] - p3[j]) * h);
            let p1 = match kn0.handle_out { Some(v) if !v30 => [0, 1, 2].map(|j| p0[j] + v[j]), _ => p1 };
            let p2 = match kn1.handle_in { Some(v) if !v30 => [0, 1, 2].map(|j| p3[j] + v[j]), _ => p2 };
            return Stretch::Curved([p0, p1, p2, p3]);
        }
        let wraps = self.closed && !v30;
        let seam30 = self.closed && v30;
        let (before, seam_before) = if k > 0 {
            (Some(self.knots[k - 1].place), false)
        } else if wraps {
            (Some(self.knots[n - 1].place), true)
        } else if seam30 {
            (Some(self.knots[n - 2].place), true)
        } else {
            (None, false)
        };
        let after = if k + 2 < n {
            if seam30 && k + 2 == n - 1 {
                Some(self.knots[0].place)
            } else {
                Some(self.knots[k + 2].place)
            }
        } else if wraps {
            Some(self.knots[(k + 2) % n].place)
        } else {
            None
        };
        let (t0, c0, b0) = (kn0.tension as f64, kn0.continuity as f64, kn0.bias as f64);
        let (t1, c1, b1) = (kn1.tension as f64, kn1.continuity as f64, kn1.bias as f64);

        let p1 = before.map(|before| {
            let round = match version {
                RenderAs::Bryce30 => !seam_before,
                RenderAs::Bryce31 => true,
                RenderAs::Bryce50 => false,
            };
            let om = if round { r32(1.0 - t0) } else { 1.0 - t0 };
            let wa = (1.0 + c0) * (1.0 + b0) * om * q;
            let wb = (1.0 - c0) * (1.0 - b0) * om * q;
            [0, 1, 2].map(|j| p0[j] + wa * (p0[j] - before[j]) + wb * (p3[j] - p0[j]))
        });
        let p2 = after.map(|after| {
            let om = 1.0 - t1;
            let wa = (1.0 + b1) * (1.0 - c1) * om * q;
            let wb = (1.0 - b1) * (1.0 + c1) * om * q;
            [0, 1, 2].map(|j| p3[j] + wa * (p0[j] - p3[j]) + wb * (p3[j] - after[j]))
        });
        let (p1, p2) = match (p1, p2) {
            (Some(p1), Some(p2)) => (p1, p2),
            (None, Some(p2)) => {
                let x = [0, 1, 2].map(|j| p3[j] + (c0 + 1.0) * (p2[j] - p3[j]));
                (std::array::from_fn(|j| p0[j] + (x[j] - p0[j]) * (t0 + q)), p2)
            }
            (Some(p1), None) => {
                let x = [0, 1, 2].map(|j| p0[j] + (c1 + 1.0) * (p1[j] - p0[j]));
                (p1, std::array::from_fn(|j| p3[j] + (x[j] - p3[j]) * (t1 + q)))
            }
            (None, None) => unreachable!("three or more knots have a neighbour on one side"),
        };
        // : 3.1 and 5.0 take a knot's own vector where the knot has one; the other inner point is
        // the one the stretch has without handles (3.0 does not read them)
        let p1 = match kn0.handle_out { Some(v) if !v30 => [0, 1, 2].map(|j| p0[j] + v[j]), _ => p1 };
        let p2 = match kn1.handle_in { Some(v) if !v30 => [0, 1, 2].map(|j| p3[j] + v[j]), _ => p2 };
        Stretch::Curved([p0, p1, p2, p3])
    }

    /// The stretch and the part u for a distance: held to the path's range, or wrapped
    /// for a closed path of 3.1 and later.
    ///
    fn locate(&self, version: RenderAs, distance: f32) -> (usize, f64) {
        let m = self.stretch_count(version);
        let mut s = distance as f64;
        if self.closed && version != RenderAs::Bryce30 {
            s %= m as f64;
            if s < 0.0 {
                s += m as f64;
            }
        }
        if s.is_nan() || s < 0.0 {
            return (0, 0.0);
        }
        if s >= m as f64 {
            return (m - 1, 1.0);
        }
        let k = s.floor();
        (k as usize, s - k)
    }

    /// The place on stretch `k` (below `stretch_count`) at the part `u` of the way (0 to 1),
    /// as `sample` makes it, without the distance's whole part being added to `u` first.
    /// ; (the curve at a distance along a
    /// stretch)
    pub fn place_on_stretch(&self, version: RenderAs, k: usize, u: f64) -> Vec3 {
        self.on_stretch(version, k, u).0
    }

    /// The place and the direction of travel on stretch `k` at `u`.
    fn on_stretch(&self, version: RenderAs, k: usize, u: f64) -> (Vec3, Vec3) {
        match self.stretch(version, k) {
            Stretch::Straight(p0, p3) => {
                // animation (path.jsonl: a straight stretch is the line itself and
                // its direction is P3 - P0)
                let place = [0, 1, 2].map(|j| (p3[j] - p0[j]) * u + p0[j]);
                let place = if u == 1.0 { p3 } else { place };
                (place, sub(p3, p0))
            }
            Stretch::Curved([p0, p1, p2, p3]) => {
                let (place, d, e) = bezier(p0, p1, p2, p3, u);
                let place = if u == 1.0 { p3 } else { place };
                (place, sub(e, d))
            }
        }
    }

    /// The points whose box is the path's box, in the path's own space: for every stretch
    /// in order its first knot's place, then the curve at the stretch's sample distances.
    /// As 3.0 twenty distances, the multiples of the binary32 0.05 accumulated in binary64
    /// and rounded to binary32, the twentieth 1; as 3.1 and 5.0 five, the multiples of the
    /// binary32 0.2 accumulated the same way, the fifth 1 (3.1's curve and stretches).
    pub fn box_points(&self, version: RenderAs) -> Vec<Vec3> {
        let (step, count) = match version {
            RenderAs::Bryce30 => (0.05_f32, 20),
            RenderAs::Bryce31 | RenderAs::Bryce50 => (0.2_f32, 5),
        };
        let mut out = Vec::new();
        for k in 0..self.stretch_count(version) {
            out.push(self.knots[k].place);
            let mut acc = 0.0f64;
            for i in 1..=count {
                let u = if i == count {
                    1.0
                } else {
                    acc += step as f64;
                    acc as f32 as f64
                };
                out.push(self.place_on_stretch(version, k, u));
            }
        }
        out
    }

    /// The box of `box_points`: the lowest x, y, z, then the highest.
    pub fn bounding_box(&self, version: RenderAs) -> [f64; 6] {
        let mut b = [f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for p in self.box_points(version) {
            for a in 0..3 {
                if p[a] < b[a] {
                    b[a] = p[a];
                }
                if p[a] > b[3 + a] {
                    b[3 + a] = p[a];
                }
            }
        }
        b
    }

    /// The place and the facing orientation at a distance along the path (at least two
    /// knots).
    pub fn sample(&self, version: RenderAs, distance: f32) -> PathSample {
        let (k, u) = self.locate(version, distance);
        let n = self.knots.len();
        let (place, direction) = self.on_stretch(version, k, u);
        let r0 = self.knots[k].roll as f64;
        let r1 = self.knots[(k + 1) % n].roll as f64;
        let roll = r0 + (r1 - r0) * u;
        PathSample { place, facing: facing(version, direction, roll) }
    }
}

/// A row brought to unit length: k * v with k the version's reciprocal square root of the
/// squared length, summed as (y*y + z*z) + x*x in 3.0 and 3.1, (x*x + y*y) + z*z in 5.0.
fn unit(version: RenderAs, v: Vec3) -> Vec3 {
    let [x, y, z] = v;
    let sq = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (y * y + z * z) + x * x,
        RenderAs::Bryce50 => (x * x + y * y) + z * z,
    };
    let k = rsqrt_as(version, sq);
    [x * k, y * k, z * k]
}

/// a x b.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The orientation of an object facing along direction t, turned by `roll` radians about
/// it.
///
/// (T brought to unit length first; the usual rows
/// (W x T, W = (0, T.z, -T.y), T); the exceptional rows V = (T.z, 0, -T.x), T x V when
/// |T.x - 1| is below 1e-5 or not comparable; the roll as a quaternion product; a zero
/// direction)
/// (the rows made from the unit T before any is scaled, then all
/// three scaled, T twice; the conversion's sums and square roots by version; the roll
/// product in its stated order, with's sine and cosine rounded to binary64)
fn facing(version: RenderAs, t: Vec3, roll: f64) -> Quat {
    let t = unit(version, t);
    let along_x = !((t[0] - 1.0).abs() >= ALONG_X); // NaN counts as below
    let (row0, row1) = if along_x {
        let v = [t[2], 0.0, -t[0]];
        (v, cross(t, v))
    } else {
        let w = [0.0, t[2], -t[1]];
        (cross(w, t), w)
    };
    let m = [unit(version, row0), unit(version, row1), unit(version, t)];
    let b = matrix_to_quat(version, &m);
    let half = roll * 0.5;
    let a = [0.0, 0.0, x87::sin_f64(half), x87::cos_f64(half)];
    let dot = (a[1] * b[1] + a[2] * b[2]) + a[0] * b[0];
    let q = [
        (a[0] * b[3] + b[0] * a[3]) + (b[2] * a[1] - b[1] * a[2]),
        (b[1] * a[3] + a[1] * b[3]) + (b[0] * a[2] - a[0] * b[2]),
        (a[2] * b[3] + b[2] * a[3]) + (a[0] * b[1] - b[0] * a[1]),
        a[3] * b[3] - dot,
    ];
    if q.iter().any(|v| v.is_nan()) {
        return [SIGN_SET_QUIET_NAN; 4];
    }
    q
}

/// T within this of +x takes the exceptional frame.
const ALONG_X: f64 = 1e-5;

/// The not-a-number a facing of no direction gives in 3.1 and 5.0.
const SIGN_SET_QUIET_NAN: f64 = f64::from_bits(0xfff8_0000_0000_0000);

/// A rotation matrix (rows) as a unit quaternion, by Shoemake's method.
/// (steps 2 to 4: the trace's order by version, the trace branch at
/// t >= 0, otherwise the largest diagonal; Q17)
fn matrix_to_quat(version: RenderAs, m: &[Vec3; 3]) -> Quat {
    let trace = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => (m[0][0] + m[2][2]) + m[1][1],
        RenderAs::Bryce50 => (m[1][1] + m[0][0]) + m[2][2],
    };
    if trace >= 0.0 {
        let s = sqrt_as(version, trace + 1.0);
        let w = s * 0.5;
        let r = 0.5 / s;
        return [(m[2][1] - m[1][2]) * r, (m[0][2] - m[2][0]) * r, (m[1][0] - m[0][1]) * r, w];
    }
    let mut i = 0;
    if m[1][1] > m[0][0] {
        i = 1;
    }
    if m[2][2] > m[i][i] {
        i = 2;
    }
    let j = (i + 1) % 3;
    let k = (j + 1) % 3;
    let s = sqrt_as(version, (m[i][i] - (m[j][j] + m[k][k])) + 1.0);
    let r = 0.5 / s;
    let mut q = [0.0; 4];
    q[i] = s * 0.5;
    q[3] = (m[k][j] - m[j][k]) * r;
    q[j] = (m[j][i] + m[i][j]) * r;
    q[k] = (m[k][i] + m[i][k]) * r;
    q
}
