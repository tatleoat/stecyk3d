// SPDX-License-Identifier: MIT
//! The arithmetic of a transform's parts: the version's product, the turn from two angles,
//! a rotation to a quaternion and back, a transform taken apart and put together, the roll.
//!
//! , (3.0's
//! square root)
//!
//! Every operation is one binary64 operation, in the order writes, with no fused
//! multiply-add.

use crate::geom::Vec3;
use crate::numerics::sqrt_as;
use crate::numerics::x87::{cos_ext, sin_ext};
use crate::scene_records::Transform;
use crate::version::RenderAs;

/// The order of a sum of three terms (i j k): term i plus term j, then term k added.
type Order = [usize; 3];

fn sum3(o: Order, t: [f64; 3]) -> f64 {
    (t[o[0]] + t[o[1]]) + t[o[2]]
}

/// The orders of the twelve numbers of P, per version.
fn product_orders(version: RenderAs) -> [Order; 12] {
    match version {
        RenderAs::Bryce30 => [[0, 1, 2]; 12],
        RenderAs::Bryce31 => [
            [2, 1, 0], [0, 1, 2], [0, 2, 1], [0, 2, 1], [0, 2, 1], [2, 1, 0],
            [0, 1, 2], [2, 1, 0], [1, 2, 0], [0, 1, 2], [0, 1, 2], [0, 1, 2],
        ],
        RenderAs::Bryce50 => [
            [0, 2, 1], [2, 1, 0], [1, 0, 2], [2, 1, 0], [2, 1, 0], [0, 1, 2],
            [2, 1, 0], [2, 1, 0], [0, 1, 2], [2, 1, 0], [2, 0, 1], [1, 0, 2],
        ],
    }
}

/// A transform as its twelve numbers: the matrix by rows, then the translation .
pub fn twelve(t: &Transform) -> [f64; 12] {
    let r = &t.rows;
    [
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2], t.translation[0],
        t.translation[1], t.translation[2],
    ]
}

/// The transform of twelve numbers.
pub fn from_twelve(n: &[f64; 12]) -> Transform {
    Transform { rows: [[n[0], n[1], n[2]], [n[3], n[4], n[5]], [n[6], n[7], n[8]]], translation: [n[9], n[10], n[11]] }
}

/// The product with the three products of each number added in `orders`; B's translation
/// is added last.
fn product_with(orders: &[Order; 12], a: &[f64; 12], b: &[f64; 12]) -> [f64; 12] {
    std::array::from_fn(|e| {
        let (row, col) = if e < 9 { (e / 3, e % 3) } else { (3, e - 9) };
        let terms: [f64; 3] = std::array::from_fn(|k| a[row * 3 + k] * b[k * 3 + col]);
        let s = sum3(orders[e], terms);
        if e < 9 { s } else { s + b[e] }
    })
}

/// H(A, B):'s product, each sum in the order (0 1 2).
pub fn product_h(a: &Transform, b: &Transform) -> Transform {
    from_twelve(&product_with(&[[0, 1, 2]; 12], &twelve(a), &twelve(b)))
}

/// P(A, B): the version's product.
pub fn product_p(version: RenderAs, a: &Transform, b: &Transform) -> Transform {
    from_twelve(&product_with(&product_orders(version), &twelve(a), &twelve(b)))
}

/// The identity transform.
pub const IDENTITY: Transform =
    Transform { rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], translation: [0.0, 0.0, 0.0] };

/// The second transform of a camera made from its two angles, `pitch` the first and `yaw`
/// the second : the rows of P(Ry, T) and the stored translation.
pub fn turn_second(version: RenderAs, pitch: f64, yaw: f64, translation: Vec3) -> Transform {
    let c = cos_ext(yaw).to_f64();
    let s = sin_ext(yaw).to_f64();
    let ry = Transform { rows: [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]], translation: [0.0; 3] };
    let (x, z) = (c, -s);
    let sigma = sin_ext(pitch).to_f64();
    let cp = cos_ext(pitch);
    let k = cp.negated().add_to_f64(1.0); // ⟨1 − Cp⟩
    let xk = x * k;
    let cross = if version == RenderAs::Bryce50 { (x * z) * k } else { xk * z };
    let t = Transform {
        rows: [
            [cp.add_to_f64((x * x) * k), sigma * z, cross],
            [-(sigma * z), cp.to_f64(), x * sigma],
            [cross, -(x * sigma), cp.add_to_f64((z * z) * k)],
        ],
        translation: [0.0; 3],
    };
    let mut second = product_p(version, &ry, &t);
    second.translation = translation;
    second
}

/// The unit quaternion (x, y, z, w) of a rotation. ("Rotation to a quaternion
/// and back": Shepperd 1978)
pub fn rotation_to_quaternion(version: RenderAs, r: &[Vec3; 3]) -> [f64; 4] {
    let m = |i: usize, j: usize| r[i][j];
    let (m0, m4, m8) = (r[0][0], r[1][1], r[2][2]);
    let trace = if version == RenderAs::Bryce50 { (m4 + m0) + m8 } else { (m0 + m8) + m4 };
    if trace >= 0.0 {
        let a = sqrt_as(version, trace + 1.0);
        let f = 0.5 / a;
        return [(m(2, 1) - m(1, 2)) * f, (m(0, 2) - m(2, 0)) * f, (m(1, 0) - m(0, 1)) * f, a * 0.5];
    }
    let mut i = if m4 > m0 { 1 } else { 0 };
    if m8 > r[i][i] {
        i = 2;
    }
    let j = (i + 1) % 3;
    let k = (j + 1) % 3;
    let a = sqrt_as(version, (m(i, i) - (m(k, k) + m(j, j))) + 1.0);
    let f = 0.5 / a;
    let mut q = [0.0; 4];
    q[i] = a * 0.5;
    q[j] = (m(i, j) + m(j, i)) * f;
    q[k] = (m(i, k) + m(k, i)) * f;
    q[3] = (m(k, j) - m(j, k)) * f;
    q
}

/// The rotation of a quaternion (x, y, z, w), all versions.
pub fn quaternion_to_rotation(q: [f64; 4]) -> [Vec3; 3] {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (wx, wy, wz) = (w * x2, y2 * w, z2 * w);
    let (xx, xy, xz) = (x2 * x, y2 * x, x * z2);
    let (yy, yz, zz) = (y2 * y, z2 * y, z2 * z);
    let k = 1.0 - xx;
    [
        [(1.0 - yy) - zz, xy - wz, xz + wy],
        [xy + wz, k - zz, yz - wx],
        [xz - wy, yz + wx, k - yy],
    ]
}

/// A transform taken apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    pub scales: Vec3,
    /// d7 (the second row along the first), d4 and d5 (the third along the first and second).
    pub shears: Vec3,
    pub rotation: [Vec3; 3],
    pub translation: Vec3,
}

/// Takes a transform apart. `None` where a scale comes out 0 or not finite (no
/// decomposition; the callers' fallback).
pub fn decompose(version: RenderAs, t: &Transform) -> Option<Parts> {
    let v5 = version == RenderAs::Bryce50;
    let sq_order: Order = if v5 { [2, 1, 0] } else { [0, 1, 2] };
    let length = |v: Vec3| (sum3(sq_order, [v[0] * v[0], v[1] * v[1], v[2] * v[2]])).sqrt();
    let div = |v: Vec3, s: f64| -> Vec3 { [v[0] / s, v[1] / s, v[2] / s] };
    let m = &t.rows;
    // 1.
    let s0 = length(m[0]);
    let x = div(m[0], s0);
    // 2.
    let d7 = (m[1][1] * x[1] + m[1][2] * x[2]) + m[1][0] * x[0];
    let r1 = [m[1][0] - d7 * x[0], m[1][1] - d7 * x[1], m[1][2] - x[2] * d7];
    let s1 = length(r1);
    let y = div(r1, s1);
    let d7 = d7 / s1;
    // 3.
    let o4: Order = match version {
        RenderAs::Bryce30 => [2, 0, 1],
        RenderAs::Bryce31 => [0, 1, 2],
        RenderAs::Bryce50 => [2, 1, 0],
    };
    let d4 = sum3(o4, [m[2][0] * x[0], m[2][1] * x[1], m[2][2] * x[2]]);
    let r2 = [m[2][0] - d4 * x[0], m[2][1] - d4 * x[1], m[2][2] - d4 * x[2]];
    let o5: Order = if v5 { [2, 1, 0] } else { [2, 0, 1] };
    let d5 = sum3(o5, [r2[0] * y[0], r2[1] * y[1], r2[2] * y[2]]);
    let r2b = [r2[0] - y[0] * d5, r2[1] - d5 * y[1], r2[2] - d5 * y[2]];
    let s2 = length(r2b);
    let mut z = div(r2b, s2);
    let d4 = d4 / s2;
    let d5 = d5 / s2;
    if ![s0, s1, s2].iter().all(|s| s.is_finite() && *s != 0.0) {
        return None;
    }
    // 4. The handedness: only the sign of the determinant is used.
    let h = x[0] * (y[1] * z[2] - y[2] * z[1]) - x[1] * (y[0] * z[2] - y[2] * z[0]) + x[2] * (y[0] * z[1] - y[1] * z[0]);
    let (mut x, mut y) = (x, y);
    let mut scales = [s0, s1, s2];
    if h < 0.0 {
        let neg = |v: Vec3| -> Vec3 { [-v[0], -v[1], -v[2]] };
        x = neg(x);
        y = neg(y);
        z = neg(z);
        scales = scales.map(|s| -s);
    }
    Some(Parts { scales, shears: [d7, d4, d5], rotation: [x, y, z], translation: t.translation })
}

/// The parts put together around a quaternion.
pub fn compose(version: RenderAs, translation: Vec3, q: [f64; 4], scales: Vec3, shears: Vec3) -> Transform {
    let rot = Transform { rows: quaternion_to_rotation(q), translation: [0.0; 3] };
    let o = twelve(&product_p(version, &IDENTITY, &rot));
    compose_from(o, translation, scales, shears)
}

/// The shears and scales applied to the nine numbers of a rotation (the steps of `compose`
/// after the rotation is formed).
pub fn compose_from(mut o: [f64; 12], translation: Vec3, scales: Vec3, shears: Vec3) -> Transform {
    let [d7, d4, d5] = shears;
    o[6] = (d5 * o[3] + o[0] * d4) + o[6];
    o[7] = (d4 * o[1] + d5 * o[4]) + o[7];
    o[8] = (d4 * o[2] + d5 * o[5]) + o[8];
    o[3] = o[0] * d7 + o[3];
    o[4] = d7 * o[1] + o[4];
    o[5] = d7 * o[2] + o[5];
    for r in 0..3 {
        for c in 0..3 {
            o[r * 3 + c] *= scales[r];
        }
    }
    o[9] = translation[0];
    o[10] = translation[1];
    o[11] = translation[2];
    from_twelve(&o)
}

/// The first transform with the roll: taken apart and put together around the quaternion
/// (0, 0, ⟨S(h)⟩, ⟨C(h)⟩), h = 0.5 · roll, with its own scales, shears and translation.
/// `None` where it cannot be taken apart.
pub fn roll_first(version: RenderAs, first: &Transform, roll: f64) -> Option<Transform> {
    let p = decompose(version, first)?;
    let h = 0.5 * roll;
    let q = [0.0, 0.0, sin_ext(h).to_f64(), cos_ext(h).to_f64()];
    Some(compose(version, p.translation, q, p.scales, p.shears))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip() {
        // : the identity has unit scales, zero shears, the identity rotation
        for v in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let p = decompose(v, &IDENTITY).unwrap();
            assert_eq!(p.scales, [1.0; 3]);
            let q = rotation_to_quaternion(v, &p.rotation);
            assert_eq!(q, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(compose(v, [0.0; 3], q, p.scales, p.shears), IDENTITY);
        }
    }
}
