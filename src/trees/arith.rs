// SPDX-License-Identifier: MIT
//! The arithmetic the growth rules share: vectors, lengths and unit vectors, the three turns,
//! the frame of a vector, the cubic curve, and the constants of's Terms.
//!
//! (Terms; Accuracy:'s sines and cosines, each multiplied by a double and the
//! product rounded once; square roots correctly rounded and unit(v) = v times 1 / |v|, as
//! 5.0; every other operation one binary64 operation)
//! (orders and forms: lengths; a branch's curve; carrying a branch's curve; directions
//! a frame carries; the lean after a child grows; curves)
//! (Orders and forms)

use crate::numerics::ExtValue;
use crate::numerics::x87::{cos_ext, sin_ext};

/// A point or vector: x, y, z.
pub type Vec3 = [f64; 3];

/// π, the double nearest it.
pub const PI: f64 = std::f64::consts::PI;
/// e, the double nearest it.
pub const E: f64 = std::f64::consts::E;
/// 1/e, the double nearest it (not e's reciprocal computed).
pub const INV_E: f64 = 0.367_879_441_171_442_321_595_523_770_161_460_867_445_811_131_031_767_834_507_836_801_7;
/// 5/7, the double nearest it.
pub const FIVE_SEVENTHS: f64 = 0.714_285_714_285_714_285_714_285_714_285_714_285_714;
/// Ten thirteenths: the double 0.7692307692307692 (one unit in the last place below the double
/// nearest 10/13).
#[allow(clippy::excessive_precision)]
pub const TEN_THIRTEENTHS: f64 = 0.7692307692307692;
/// π/3: π times the double nearest 1/3.
pub const PI_THIRDS: f64 = PI * (1.0 / 3.0);
/// π/5: π times the double nearest 1/5.
pub const PI_FIFTHS: f64 = PI * (1.0 / 5.0);
/// π/6: π times the double nearest 1/6.
pub const PI_SIXTHS: f64 = PI * (1.0 / 6.0);
/// 17/21, the double nearest it.
pub const SEVENTEEN_TWENTY_FIRSTS: f64 = 0.809_523_809_523_809_523_809_523_809_523_809_523_809;

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// k times each coordinate.
pub fn scale(k: f64, a: Vec3) -> Vec3 {
    [k * a[0], k * a[1], k * a[2]]
}

/// Left to right, (a_x·b_x + a_y·b_y) + a_z·b_z. (Orders and forms)
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// The vector product.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// |v| with its square (x² + y²) + z², correctly rounded root: every length but the two below.
pub fn length(a: Vec3) -> f64 {
    ((a[0] * a[0] + a[1] * a[1]) + a[2] * a[2]).sqrt()
}

/// |v| with its square (z² + y²) + x²: for d in a frame F_d and for a place's direction.
pub fn length_zyx(a: Vec3) -> f64 {
    ((a[2] * a[2] + a[1] * a[1]) + a[0] * a[0]).sqrt()
}

/// unit(v) = v times 1 / |v|, |v| as `length`.
pub fn unit(a: Vec3) -> Vec3 {
    scale(1.0 / length(a), a)
}

/// unit(v) with |v| as `length_zyx` (F_d's Y; a place's direction).
pub fn unit_zyx(a: Vec3) -> Vec3 {
    scale(1.0 / length_zyx(a), a)
}

/// Strictly different in some coordinate (a not-a-number differs).
pub fn differs(a: Vec3, b: Vec3) -> bool {
    a[0] != b[0] || a[1] != b[1] || a[2] != b[2]
}

/// The smaller of two numbers (the first where neither is smaller).
pub fn smaller(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// The greater of two numbers (the first where neither is greater).
pub fn greater(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// An angle's sine and cosine as's extended values.
#[derive(Clone, Copy, Debug)]
pub struct SinCos {
    sin: ExtValue,
    cos: ExtValue,
}

impl SinCos {
    pub fn of(a: f64) -> SinCos {
        SinCos { sin: sin_ext(a), cos: cos_ext(a) }
    }

    /// d · sin a, rounded once.,
    pub fn sin_times(self, d: f64) -> f64 {
        self.sin.mul_to_f64(d)
    }

    /// d · cos a, rounded once.,
    pub fn cos_times(self, d: f64) -> f64 {
        self.cos.mul_to_f64(d)
    }

    /// ĉ: the cosine rounded to a double ('s value multiplied by 1).
    pub fn cos_double(self) -> f64 {
        self.cos.mul_to_f64(1.0)
    }

    /// ŝ: the sine rounded to a double ('s value multiplied by 1).
    pub fn sin_double(self) -> f64 {
        self.sin.mul_to_f64(1.0)
    }
}

/// v turned by Tx about p with the doubles ĉ and ŝ: p + w, where v − p = (x, y, z) gives
/// w = (x, y·ĉ + z·ŝ, z·ĉ − y·ŝ).
pub fn turn_x_about(v: Vec3, p: Vec3, c: f64, s: f64) -> Vec3 {
    let [x, y, z] = sub(v, p);
    add(p, [x, y * c + z * s, z * c - y * s])
}

/// Tx(a): (x, y·cos a + z·sin a, z·cos a − y·sin a).
pub fn turn_x(a: f64, p: Vec3) -> Vec3 {
    let t = SinCos::of(a);
    let [x, y, z] = p;
    [x, t.cos_times(y) + t.sin_times(z), t.cos_times(z) - t.sin_times(y)]
}

/// Ty(a): (x·cos a + z·sin a, y, z·cos a − x·sin a).
pub fn turn_y(a: f64, p: Vec3) -> Vec3 {
    let t = SinCos::of(a);
    let [x, y, z] = p;
    [t.cos_times(x) + t.sin_times(z), y, t.cos_times(z) - t.sin_times(x)]
}

/// Tz(a): (x·cos a + y·sin a, y·cos a − x·sin a, z).
pub fn turn_z(a: f64, p: Vec3) -> Vec3 {
    let t = SinCos::of(a);
    let [x, y, z] = p;
    [t.cos_times(x) + t.sin_times(y), t.cos_times(y) - t.sin_times(x), z]
}

/// The y axis, which the turns and frames carry.
pub const UP: Vec3 = [0.0, 1.0, 0.0];

/// The frame F_d of a vector d.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    /// Where d's first and third coordinates are both 0.
    Identity,
    /// (x, y, z) to x·X + y·Y + z·Z.
    Axes { x: Vec3, y: Vec3, z: Vec3 },
}

impl Frame {
    /// (orders and forms: lengths: d's square (z² + y²) + x²; Y × X's and X's
    /// (x² + y²) + z²)
    pub fn of(d: Vec3) -> Frame {
        if d[0] == 0.0 && d[2] == 0.0 {
            return Frame::Identity;
        }
        let y = unit_zyx(d);
        // X' = (0 - Y_z, 0, Y_x - 0): a subtraction from 0 and one of 0 (orders and
        // forms: Zeros)
        let x = unit([0.0 - y[2], 0.0, y[0] - 0.0]);
        let z = unit(cross(y, x));
        Frame::Axes { x, y, z }
    }

    /// The three vectors the frame carries the axes to (the identity's are the axes).
    pub fn axes(self) -> [Vec3; 3] {
        match self {
            Frame::Identity => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Frame::Axes { x, y, z } => [x, y, z],
        }
    }

    /// F(p) for a stalk's points: per coordinate (x·X + y·Y) + z·Z; the identity leaves p as it
    /// is. The order is Open and changes no case (orders and forms: last paragraph).
    /// (Orders and forms)
    pub fn apply(self, p: Vec3) -> Vec3 {
        match self {
            Frame::Identity => p,
            Frame::Axes { x, y, z } => {
                std::array::from_fn(|i| (p[0] * x[i] + p[1] * y[i]) + p[2] * z[i])
            }
        }
    }

    /// F(w) for a direction w (a child's D; the vectors of pairs, alternating leaves, spirals
    /// and bunches): ((w_z·Z_x + w_y·Y_x) + w_x·X_x), ((w_z·Z_y + w_y·Y_y) + w_x·X_y),
    /// ((w_x·X_z + w_y·Y_z) + w_z·Z_z); the identity leaves w as it is.
    pub fn carry_direction(self, w: Vec3) -> Vec3 {
        match self {
            Frame::Identity => w,
            Frame::Axes { x, y, z } => [
                (w[2] * z[0] + w[1] * y[0]) + w[0] * x[0],
                (w[2] * z[1] + w[1] * y[1]) + w[0] * x[1],
                (w[0] * x[2] + w[1] * y[2]) + w[2] * z[2],
            ],
        }
    }

    /// P + L·F(q) for a point q of a branch's curve: per coordinate
    /// ((q_x·(L·X) + q_y·(L·Y)) + q_z·(L·Z)) + P; with the identity, (q·L) + P.
    pub fn carry_curve_point(self, q: Vec3, l: f64, p: Vec3) -> Vec3 {
        match self {
            Frame::Identity => std::array::from_fn(|i| q[i] * l + p[i]),
            Frame::Axes { x, y, z } => std::array::from_fn(|i| {
                ((q[0] * (l * x[i]) + q[1] * (l * y[i])) + q[2] * (l * z[i])) + p[i]
            }),
        }
    }
}

/// The matrix M of the lean after a child grows, as its three rows M1, M2, M3 (the images of
/// the axes): F's inverse (its adjugate times 1/det F), its rows each turned by Tz(lean) and
/// then Ty(turn) with's products, then the product with F entry by entry in the orders
/// stated. F's rows are X, Y, Z (the axes for the identity).
pub fn lean_matrix(f: Frame, lean: f64, turn: f64) -> [Vec3; 3] {
    let a = f.axes(); // a[k][c]: F's row k (X, Y, Z), coordinate c
    // det F = ((Z_z·Y_y − Z_y·Y_z)·X_x − (Z_z·Y_x − Y_z·Z_x)·X_y) + (Z_y·Y_x − Z_x·Y_y)·X_z
    let [x, y, z] = a;
    let det = ((z[2] * y[1] - z[1] * y[2]) * x[0] - (z[2] * y[0] - y[2] * z[0]) * x[1])
        + (z[1] * y[0] - z[0] * y[1]) * x[2];
    let inv_det = 1.0 / det;
    // The adjugate: entry (r, c) is the cofactor of F's entry (c, r), each the difference of two
    // products: a[c+1][r+1]·a[c+2][r+2] − a[c+1][r+2]·a[c+2][r+1] (indices mod 3).
    let cof = |i: usize, j: usize| {
        let (i1, i2, j1, j2) = ((i + 1) % 3, (i + 2) % 3, (j + 1) % 3, (j + 2) % 3);
        a[i1][j1] * a[i2][j2] - a[i1][j2] * a[i2][j1]
    };
    let inverse: [Vec3; 3] = std::array::from_fn(|r| std::array::from_fn(|c| cof(c, r) * inv_det));
    let turned: [Vec3; 3] = inverse.map(|row| turn_y(turn, turn_z(lean, row)));
    // The entry (r, c): the sum over k of turned[r][k]·a[k][c], k in the stated order.
    let entry = |r: usize, c: usize| {
        let t = |k: usize| turned[r][k] * a[k][c];
        let [k1, k2, k3] = match (r, c) {
            (0, 0) => [0, 2, 1],
            (0, 1) => [2, 1, 0],
            (0, 2) => [1, 0, 2],
            (_, 2) => [0, 1, 2],
            _ => [2, 1, 0],
        };
        (t(k1) + t(k2)) + t(k3)
    };
    std::array::from_fn(|r| std::array::from_fn(|c| entry(r, c)))
}

/// A point q leaned about the point c by M: c + ((u_x·M1 + u_y·M2) + u_z·M3), u = q − c, in
/// each coordinate.
pub fn lean_point(m: &[Vec3; 3], c: Vec3, q: Vec3) -> Vec3 {
    let u = sub(q, c);
    std::array::from_fn(|i| c[i] + ((u[0] * m[0][i] + u[1] * m[1][i]) + u[2] * m[2][i]))
}

/// a + (b − a)·t: every interpolation of de Casteljau's construction (a branch's curve, the
/// outline, a stalk, a root).
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

/// The cubic curve of four points at t, by de Casteljau's construction (t may lie outside 0
/// to 1).
pub fn curve_at(q: &[Vec3; 4], t: f64) -> Vec3 {
    let a = lerp3(q[0], q[1], t);
    let b = lerp3(q[1], q[2], t);
    let c = lerp3(q[2], q[3], t);
    let d = lerp3(a, b, t);
    let e = lerp3(b, c, t);
    lerp3(d, e, t)
}

/// The same in the plane (across, up), for the outline.
pub fn curve_at_2d(q: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let l = |a: [f64; 2], b: [f64; 2]| [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
    let (a, b, c) = (l(q[0], q[1]), l(q[1], q[2]), l(q[2], q[3]));
    let (d, e) = (l(a, b), l(b, c));
    l(d, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_of_the_terms() {
        // Terms: ten thirteenths is one unit in the last place below the double nearest 10/13.
        let nearest = 10.0f64 / 13.0;
        assert_eq!(TEN_THIRTEENTHS.to_bits() + 1, nearest.to_bits());
        assert_eq!(FIVE_SEVENTHS, 5.0f64 / 7.0);
        assert_eq!(SEVENTEEN_TWENTY_FIRSTS, 17.0f64 / 21.0);
        // 1/e as the double nearest it: within half a unit of the true value 0.36787944117144233...
        assert_eq!(INV_E.to_bits(), 0x3fd7_8b56_362c_ef38);
    }

    #[test]
    fn turns_at_zero_keep_points() {
        let p = [1.5, -2.0, 0.25];
        assert_eq!(turn_x(0.0, p), p);
        assert_eq!(turn_y(0.0, p), p);
        assert_eq!(turn_z(0.0, p), p);
    }

    #[test]
    fn frames() {
        assert_eq!(Frame::of([0.0, 2.0, 0.0]), Frame::Identity);
        assert_eq!(Frame::of([0.0, -3.0, -0.0]), Frame::Identity);
        let f = Frame::of([1.0, 1.0, 0.0]);
        let y = f.apply(UP);
        assert!((y[0] - y[1]).abs() < 1e-15 && y[2] == 0.0);
        // With no lean and no turn, M is F's inverse then F: the identity, to rounding.
        let m = lean_matrix(f, 0.0, 0.0);
        let c = [1.0, 2.0, 3.0];
        let q = [1.3, 1.8, 3.9];
        let back = lean_point(&m, c, q);
        assert!(back.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-15));
        assert_eq!(f.carry_direction(UP), f.apply(UP));
        assert_eq!(Frame::Identity.carry_curve_point([0.0, 0.5, 0.0], 2.0, [0.0, -1.0, 0.0]), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn turning_about_a_point() {
        // : v about p goes to p + (x, y·ĉ + z·ŝ, z·ĉ − y·ŝ).
        assert_eq!(turn_x_about([0.0, 1.0, 0.0], [0.0, 0.5, 0.0], 0.0, 1.0), [0.0, 0.5, -0.5]);
        assert_eq!(turn_x_about([0.0, 1.0, 0.0], [0.0, 0.5, 0.0], 1.0, 0.0), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn length_orders() {
        let v = [3.0, 4.0, 12.0];
        assert_eq!(length(v), 13.0);
        assert_eq!(length_zyx(v), 13.0);
    }

    #[test]
    fn curve_ends() {
        let q = [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        assert_eq!(curve_at(&q, 0.0), q[0]);
        assert_eq!(curve_at(&q, 1.0), q[3]);
    }
}
