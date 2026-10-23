// SPDX-License-Identifier: MIT
//! The turn of an object or a camera toward its target: the axis its first flag word
//! chooses, the rows of the rotation that points it, and a camera's second transform made
//! from the two angles of its forward direction.
//!
//! , (D·L·Q as for the
//! parent's chain), (the first flag
//! word at offset 4)

use super::AnimError;
use super::link::LinkArithmetic;
use crate::geom::Vec3;
use crate::scene_records::Transform;
use crate::scene_view::rows_from_angles;

/// First flag word: the tracker's z axis points at the target.
pub const AIM_Z: u32 = 0x0800_0000;
/// First flag word: its x axis (where 0x8000000 is not set).
pub const AIM_X: u32 = 0x0200_0000;
/// First flag word: its y axis (where neither of the two above is set).
pub const AIM_Y: u32 = 0x0400_0000;
/// First flag word: the chosen axis points away from the target.
pub const AIM_AWAY: u32 = 0x1000_0000;
/// Second flag word bit for which a camera's turn is given only where the translations of
/// its first transform and of its target's are 0.
pub const CAMERA_BIT: u32 = 0x0010_0000;

/// Where ||d_k| − 1| is below this, the level row is made from the other pair of
/// coordinates.
pub const LEVEL_BOUND: f64 = 1e-4;
/// A tracker nearer its target's place than this is not turned: objects and cameras as 3.0.
pub const LEAST_LENGTH: f64 = 1e-4;
/// A camera as 3.1 and 5.0 is not turned nearer than this.
pub const LEAST_LENGTH_CAMERA_LATER: f64 = 1e-7;

/// One of the tracker's own axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// What the first flag word chooses: an axis (or none: the rotation is kept), and whether
/// it points away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aim {
    pub axis: Option<Axis>,
    pub away: bool,
}

impl Aim {
    /// (0x8000000 z; else 0x2000000 x; else 0x4000000 y; none: no axis;
    /// 0x10000000 beside one of them: away)
    pub fn of_flags(first_flags: u32) -> Aim {
        let axis = if first_flags & AIM_Z != 0 {
            Some(Axis::Z)
        } else if first_flags & AIM_X != 0 {
            Some(Axis::X)
        } else if first_flags & AIM_Y != 0 {
            Some(Axis::Y)
        } else {
            None
        };
        Aim { axis, away: axis.is_some() && first_flags & AIM_AWAY != 0 }
    }
}

fn open(what: &'static str) -> AnimError {
    AnimError::Unsupported(what)
}

fn length(v: Vec3) -> f64 {
    ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt()
}

fn unit(w: Vec3) -> Vec3 {
    let n = length(w);
    [w[0] / n, w[1] / n, w[2] / n]
}

fn neg(v: Vec3) -> Vec3 {
    [-v[0], -v[1], -v[2]]
}

/// a × b.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The rows of R for the unit direction `d`, or `None` for no axis (R = Q).
pub fn aim_rows(d: Vec3, aim: Aim) -> Option<[Vec3; 3]> {
    let near_one = |x: f64| (x.abs() - 1.0).abs() < LEVEL_BOUND;
    let sign = |v: Vec3| if aim.away { neg(v) } else { v };
    Some(match aim.axis? {
        Axis::Z => {
            let level = if near_one(d[1]) { unit([-d[1], d[0], 0.0]) } else { unit([d[2], 0.0, -d[0]]) };
            let (r2, r0) = (sign(d), sign(level));
            [r0, unit(cross(r2, r0)), r2]
        }
        Axis::X => {
            let level = if near_one(d[1]) { unit([0.0, -d[2], d[1]]) } else { unit([-d[2], 0.0, d[0]]) };
            let (r0, r2) = (sign(d), sign(level));
            [r0, unit(cross(r2, r0)), r2]
        }
        Axis::Y => {
            let level = if near_one(d[2]) { unit([d[2], 0.0, -d[0]]) } else { unit([d[1], -d[0], 0.0]) };
            let (r1, r0) = (sign(d), sign(level));
            [r0, r1, unit(cross(r0, r1))]
        }
    })
}

fn det(m: &[Vec3; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The place `q` carried into the space of a parent whose chain is `chain`: (q − t)·M⁻¹,
/// M⁻¹ by the adjugate. `None` where M has no inverse.
pub fn into_parent_space(q: Vec3, chain: &Transform) -> Option<Vec3> {
    let m = &chain.rows;
    let dt = det(m);
    if !(dt.is_finite() && dt != 0.0) {
        return None;
    }
    // The inverse's element (i, j) is the cofactor (j, i) over the determinant.
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let inv: [Vec3; 3] = std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / dt));
    let y: Vec3 = std::array::from_fn(|k| q[k] - chain.translation[k]);
    Some(std::array::from_fn(|j| (y[0] * inv[0][j] + y[1] * inv[1][j]) + y[2] * inv[2][j]))
}

/// The unit direction from `s` (the second transform's translation) to `q` (the target's
/// place in the parent's space): `None` where they are less than `least` apart (not turned).
pub fn direction(q: Vec3, s: Vec3, least: f64) -> Result<Option<Vec3>, AnimError> {
    let v: Vec3 = std::array::from_fn(|k| q[k] - s[k]);
    let n = length(v);
    if !(n >= least) {
        return Ok(None);
    }
    Ok(Some([v[0] / n, v[1] / n, v[2] / n]))
}

/// The tracker's second transform turned: its matrix written D·L·Q (the version's
/// arithmetic), Q replaced by R (or kept for no axis), put together with the same
/// translation. `d` `None`: not turned, the transform as it is. Open where the
/// determinant is not positive or a scale is 0 (no decomposition).
pub fn turned_second(arith: &LinkArithmetic, s: &Transform, d: Option<Vec3>, aim: Aim) -> Result<Transform, AnimError> {
    let Some(d) = d else { return Ok(*s) };
    let bad = || open("a tracker whose second transform has a determinant that is not positive, or a scale of 0 (AN-1.9a: open)");
    if !(det(&s.rows) > 0.0) {
        return Err(bad());
    }
    let mut parts = arith.decompose(&s.rows).ok_or_else(bad)?;
    if let Some(r) = aim_rows(d, aim) {
        parts.turn = r;
    }
    Ok(Transform { rows: arith.assemble(&parts), translation: s.translation })
}

/// The inverse of a 3 x 3 matrix by the adjugate; `None` where it has none.
/// . (revised, 14753)
fn inverse_rows(m: &[Vec3; 3]) -> Option<[Vec3; 3]> {
    let dt = det(m);
    if !(dt.is_finite() && dt != 0.0) {
        return None;
    }
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    Some(std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / dt)))
}

/// A tracker turned in the world's axes on its chain, as 3.1 and 5.0 do with the animation
/// switch not 0: `chain` is the tracker's chain C, `parent` its parent's chain (`None` in the
/// root list), `q` the target's place in the world. Returns the new second transform, or the
/// stored `second` where |q - o| < `least`.
pub fn turned_in_world(
    arith: &LinkArithmetic,
    second: &Transform,
    chain: &Transform,
    parent: Option<&Transform>,
    q: Vec3,
    aim: Aim,
    least: f64,
) -> Result<Transform, AnimError> {
    let Some(d) = direction(q, chain.translation, least)? else { return Ok(*second) };
    let turned = turned_second(arith, chain, Some(d), aim)?;
    let Some(p) = parent else { return Ok(turned) };
    let inv = inverse_rows(&p.rows).ok_or_else(|| open("a tracker whose parent's chain has no inverse (AN-1.9a: open)"))?;
    let times = |v: Vec3| -> Vec3 { std::array::from_fn(|j| (v[0] * inv[0][j] + v[1] * inv[1][j]) + v[2] * inv[2][j]) };
    let o: Vec3 = std::array::from_fn(|k| turned.translation[k] - p.translation[k]);
    Ok(Transform { rows: [times(turned.rows[0]), times(turned.rows[1]), times(turned.rows[2])], translation: times(o) })
}

/// A camera turned: its second transform made from the two angles of its forward row f
/// (the turned second transform's third row, or the stored one's where `d` is `None`),
/// p = arcsin(−f_y) and y = atan2(f_x, f_z), with the same translation; and its first
/// transform's translation t_F·M·M'⁻¹ that keeps the place of its origin. Open where f
/// is straight up or down (y undefined). Returns (second transform, first translation).
/// (M' inverted as its transpose: a rotation by construction)
pub fn camera_turn(arith: &LinkArithmetic, first: &Transform, second: &Transform, d: Option<Vec3>) -> Result<(Transform, Vec3), AnimError> {
    let (turned, moved, _) = camera_turn_angles(arith, first, second, d, Aim { axis: Some(Axis::Z), away: false })?;
    Ok((turned, moved))
}

/// `camera_turn`, and the two angles (p, y) the turned rows are made from: a camera whose
/// view is made from its angles (the perspective camera) keeps them beside the rows.
pub fn camera_turn_angles(
    arith: &LinkArithmetic,
    first: &Transform,
    second: &Transform,
    d: Option<Vec3>,
    aim: Aim,
) -> Result<(Transform, Vec3, [f64; 2]), AnimError> {
    // : the axis as the caller found it (as 3.0
    // the first flag word's, x, y, none and away alike; as 3.1 and 5.0 z), the rows then made
    // from the third row of the transform so turned
    let turned = turned_second(arith, second, d, aim)?;
    let f = turned.rows[2];
    if f[0] == 0.0 && f[2] == 0.0 {
        return Err(open("a camera whose target is straight above or below it (AN-1.9a: open)"));
    }
    let p = crate::numerics::asin((-f[1]).clamp(-1.0, 1.0));
    let y = crate::numerics::atan2(f[0], f[2]);
    let rows = rows_from_angles(arith.version, p, y);
    let m = &second.rows;
    let t = first.translation;
    let tm: Vec3 = std::array::from_fn(|j| (t[0] * m[0][j] + t[1] * m[1][j]) + t[2] * m[2][j]);
    // tm·M'ᵀ: element j is tm · (row j of M').
    let moved: Vec3 = std::array::from_fn(|j| (tm[0] * rows[j][0] + tm[1] * rows[j][1]) + tm[2] * rows[j][2]);
    Ok((Transform { rows, translation: second.translation }, moved, [p, y]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::RenderAs;

    fn near(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < 1e-12)
    }

    fn rotation_ok(r: &[Vec3; 3]) {
        assert!((det(r) - 1.0).abs() < 1e-12, "determinant {}", det(r));
        for i in 0..3 {
            assert!((length(r[i]) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn axes() {
        // : the row of the axis is d (−d away); R right-handed.
        let d = unit([0.3, -0.4, 0.8]);
        for (flags, row) in [(AIM_Z, 2), (AIM_X, 0), (AIM_Y, 1)] {
            for away in [false, true] {
                let aim = Aim::of_flags(flags | if away { AIM_AWAY } else { 0 });
                let r = aim_rows(d, aim).unwrap();
                rotation_ok(&r);
                assert!(near(r[row], if away { neg(d) } else { d }));
            }
        }
        // z first, then x, then y; none: no axis.
        assert_eq!(Aim::of_flags(AIM_Z | AIM_X).axis, Some(Axis::Z));
        assert_eq!(Aim::of_flags(AIM_X | AIM_Y).axis, Some(Axis::X));
        assert_eq!(Aim::of_flags(AIM_AWAY), Aim { axis: None, away: false });
        // Straight along z: the identity.
        let r = aim_rows([0.0, 0.0, 1.0], Aim::of_flags(AIM_Z)).unwrap();
        assert_eq!(r, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        // Straight up with axis z: the level row's other form.
        let r = aim_rows([0.0, 1.0, 0.0], Aim::of_flags(AIM_Z)).unwrap();
        rotation_ok(&r);
        assert_eq!(r[0], [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn steps() {
        let chain = Transform { rows: [[0.0, 2.0, 0.0], [-2.0, 0.0, 0.0], [0.0, 0.0, 2.0]], translation: [1.0, 2.0, 3.0] };
        let p = [0.5, -1.0, 4.0];
        let q: Vec3 = std::array::from_fn(|j| (p[0] * chain.rows[0][j] + p[1] * chain.rows[1][j]) + p[2] * chain.rows[2][j] + chain.translation[j]);
        assert!(near(into_parent_space(q, &chain).unwrap(), p));
        assert_eq!(direction([1.0, 2.0, 3.0], [1.0, 2.0, 3.0], LEAST_LENGTH), Ok(None));
        assert_eq!(direction([1.0, 2.0, 3.0 + 5e-5], [1.0, 2.0, 3.0], LEAST_LENGTH), Ok(None));
        assert!(direction([1.0, 2.0, 3.0 + 5e-5], [1.0, 2.0, 3.0], LEAST_LENGTH_CAMERA_LATER).unwrap().is_some());
        assert_eq!(direction([1.0, 2.0, 5.0], [1.0, 2.0, 3.0], LEAST_LENGTH),Ok(Some([0.0, 0.0, 1.0])));
    }

    #[test]
    fn scales_kept() {
        let a = LinkArithmetic::of(RenderAs::Bryce30);
        let s = Transform { rows: [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]], translation: [1.0, 1.0, 1.0] };
        let t = turned_second(&a, &s, Some([1.0, 0.0, 0.0]), Aim::of_flags(AIM_Z)).unwrap();
        assert!(near(t.rows[2], [4.0, 0.0, 0.0]));
        assert!((length(t.rows[0]) - 2.0).abs() < 1e-12);
        assert_eq!(t.translation, s.translation);
        let mirrored = Transform { rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], ..s };
        assert!(turned_second(&a, &mirrored, Some([1.0, 0.0, 0.0]), Aim::of_flags(AIM_Z)).is_err());
    }

    #[test]
    fn camera() {
        let a = LinkArithmetic::of(RenderAs::Bryce30);
        let first = Transform { rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], translation: [0.5, 0.25, -1.0] };
        let second = Transform { rows: first.rows, translation: [3.0, 1.0, 2.0] };
        let d = unit([1.0, -0.5, 2.0]);
        let (s2, tf) = camera_turn(&a, &first, &second, Some(d)).unwrap();
        assert!(near(s2.rows[2], d));
        rotation_ok(&s2.rows);
        let place = |t: Vec3, m: &[Vec3; 3]| -> Vec3 { std::array::from_fn(|j| t[0] * m[0][j] + t[1] * m[1][j] + t[2] * m[2][j]) };
        assert!(near(place(tf, &s2.rows), place(first.translation, &second.rows)));
        assert!(camera_turn(&a, &first, &second, Some([0.0, 1.0, 0.0])).is_err());
    }
}
