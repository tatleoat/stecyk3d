// SPDX-License-Identifier: MIT
//! Linked objects: the chain of second transforms an object inherits from its parents, and
//! the part of its parent's chain an object takes when its first flag word holds bits of
//! 0xf80000.
//!
//! , (each
//! product of two transforms, `Transform::followed_by`), (the arithmetic of taking
//! a chain apart, the quaternion, and putting it together: `crate::transform_parts`)
//! (linked objects),

use crate::geom::Vec3;
use crate::scene_records::Transform;
use crate::transform_parts::{self as parts, Parts};
use crate::version::RenderAs;

/// The bits of the first flag word that say what a linked object takes from its parent's
/// chain.
pub const PARTIAL_BITS: u32 = 0x00f8_0000;
/// : as 3.0 every scale 1 and the translation 0 (with
/// another bit of these); as 3.1 and 5.0 the place S's translation plus P's.
pub const BIT_PLACE_ADDED: u32 = 0x0008_0000;
/// : every scale 1 (as 3.1 and 5.0 every shear 0 too).
pub const BIT_NO_SCALE: u32 = 0x0010_0000;
/// : Q the identity.
pub const BIT_NO_TURN: u32 = 0x0020_0000;
/// : as 3.0 the translation 0; as 3.1 and 5.0 the place
/// S's translation.
pub const BIT_NO_PLACE: u32 = 0x0040_0000;
/// : every shear 0.
pub const BIT_NO_SHEAR: u32 = 0x0080_0000;

/// A 3 x 3 matrix written as D·L·Q : D the scales, L lower triangular with ones on
/// its diagonal (the shears, `shear[0]` = l10, `shear[1]` = l20, `shear[2]` = l21), Q a
/// rotation with determinant +1, D's last sign following.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decomposed {
    pub scales: Vec3,
    pub shear: Vec3,
    pub turn: [Vec3; 3],
}

/// The arithmetic of's chain for one version:'s, which is the version's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkArithmetic {
    pub version: RenderAs,
}

/// The point to which `t` carries `p`: ((p0·m_c + p1·m_(3+c)) + p2·m_(6+c)) + m_(9+c).
fn carry(p: Vec3, t: &Transform) -> Vec3 {
    let m = &t.rows;
    std::array::from_fn(|j| ((p[0] * m[0][j] + p[1] * m[1][j]) + p[2] * m[2][j]) + t.translation[j])
}

/// As 3.1 and 5.0, a scale nearer 0 than this becomes this with its own sign.
const LEAST_SCALE: f64 = 1e-6;
/// As 3.1 and 5.0, every scale is held to -this ... this.
const MOST_SCALE: f64 = 10000.0;

/// A scale as 3.1 and 5.0 hold it once the parts are replaced: from -1e-6 to 1e-6 it
/// becomes 1e-6 with its own sign (0, of either sign, +1e-6), then it is held to
/// -10000 ... 10000.. (-0 counts as 0)
fn held_scale(d: f64) -> f64 {
    let d = if d == 0.0 {
        LEAST_SCALE
    } else if d.abs() < LEAST_SCALE {
        LEAST_SCALE.copysign(d)
    } else {
        d
    };
    d.clamp(-MOST_SCALE, MOST_SCALE)
}

/// What a linked object takes of its parent's chain P: P's matrix as it is, or put together
/// from its parts with some replaced; the translation; and the place that replaces the
/// product's translation where 3.1 and 5.0 do.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Taken {
    rows: [Vec3; 3],
    translation: Vec3,
    place: Option<Vec3>,
}

impl LinkArithmetic {
    /// The arithmetic of a version.
    pub fn of(version: RenderAs) -> LinkArithmetic {
        LinkArithmetic { version }
    }

    /// A matrix taken apart ("A transform taken apart"); `None` where a row has no
    /// length left (no decomposition).
    pub fn decompose(&self, m: &[Vec3; 3]) -> Option<Decomposed> {
        let p = parts::decompose(self.version, &Transform { rows: *m, translation: [0.0; 3] })?;
        Some(Decomposed { scales: p.scales, shear: p.shears, turn: p.rotation })
    }

    /// D·L·Q put together again through the quaternion of Q. ("Parts put
    /// together").
    pub fn assemble(&self, p: &Decomposed) -> [Vec3; 3] {
        let t = p.turn;
        let o = parts::twelve(&parts::product_p(self.version, &parts::IDENTITY, &Transform { rows: t, translation: [0.0; 3] }));
        parts::compose_from(o, [0.0; 3], p.scales, p.shear).rows
    }

    /// P with the parts replaced as the bits say. `s` is the object's second transform.
    /// , (taken apart, the
    /// quaternion, put together). (other combinations, other P)
    fn partial(&self, bits: u32, p: &Transform, s: &Transform) -> Taken {
        let version = self.version;
        let as_30 = version == RenderAs::Bryce30;
        // 3.0: with none of the four upper bits, P as it is (0x80000 alone changes nothing).
        if as_30 && bits & (BIT_NO_SCALE | BIT_NO_TURN | BIT_NO_PLACE | BIT_NO_SHEAR) == 0 {
            return Taken { rows: p.rows, translation: p.translation, place: None };
        }
        let (no_scale, no_shear, no_turn, no_place) = if as_30 {
            (
                bits & (BIT_NO_SCALE | BIT_PLACE_ADDED) != 0,
                bits & BIT_NO_SHEAR != 0,
                bits & BIT_NO_TURN != 0,
                bits & (BIT_NO_PLACE | BIT_PLACE_ADDED) != 0,
            )
        } else {
            (bits & BIT_NO_SCALE != 0, bits & (BIT_NO_SCALE | BIT_NO_SHEAR) != 0, bits & BIT_NO_TURN != 0, false)
        };
        let translation = if no_place { [0.0, 0.0, 0.0] } else { p.translation };
        // The chain's rotation always goes through the quaternion. (a rotation
        // taken from the parent's matrix without the quaternion fails the vectors)
        let rows = match parts::decompose(version, p) {
            // No decomposition (a row of length 0, or not finite): P's matrix as it is.
            // (the implementer's choice)
            None => p.rows,
            Some(Parts { mut scales, mut shears, rotation, .. }) => {
                let mut q = parts::rotation_to_quaternion(version, &rotation);
                if no_scale {
                    scales = [1.0, 1.0, 1.0];
                }
                if no_shear {
                    shears = [0.0, 0.0, 0.0];
                }
                if no_turn {
                    q = [0.0, 0.0, 0.0, 1.0];
                }
                if !as_30 {
                    scales = scales.map(held_scale);
                }
                parts::compose(version, translation, q, scales, shears).rows
            }
        };
        let place = if as_30 {
            None
        } else if bits & BIT_NO_PLACE != 0 {
            Some(s.translation)
        } else if bits & BIT_PLACE_ADDED != 0 {
            Some(std::array::from_fn(|k| s.translation[k] + p.translation[k]))
        } else if bits & (BIT_NO_SCALE | BIT_NO_TURN) != 0 {
            Some(carry(s.translation, p))
        } else {
            None
        };
        Taken { rows, translation, place }
    }

    /// `a` followed by what is taken: the product H, the translation then replaced where
    /// 3.1 and 5.0 say.
    fn followed_by_taken(&self, a: &Transform, taken: &Taken) -> Transform {
        let mut t = parts::product_h(a, &Transform { rows: taken.rows, translation: taken.translation });
        if let Some(place) = taken.place {
            t.translation = place;
        }
        t
    }

    /// The chain of a linked object: its second transform `s` followed by what it takes of
    /// its parent's chain `parent` (all of it, or part as `flags`' bits of 0xf80000 say).
    pub fn chain(&self, s: &Transform, flags: u32, parent: &Transform, version: RenderAs) -> Transform {
        debug_assert_eq!(version, self.version);
        let bits = flags & PARTIAL_BITS;
        if bits == 0 {
            return s.followed_by(parent);
        }
        let taken = self.partial(bits, parent, s);
        self.followed_by_taken(s, &taken)
    }

    /// A linked object's own transform: its first transform `f`, then its second `s`, then
    /// its parent's chain (or the part of it the bits say); with the object's own chain.
    /// Returns (own transform, chain).
    pub fn linked(&self, f: &Transform, s: &Transform, flags: u32, parent: &Transform, version: RenderAs) -> (Transform, Transform) {
        let chain = self.chain(s, flags, parent, version);
        (f.followed_by(&chain), chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows equal to rounding: the quaternion route  leaves a last bit.
    fn near(a: &[Vec3; 3], b: &[Vec3; 3]) -> bool {
        (0..3).all(|i| (0..3).all(|j| (a[i][j] - b[i][j]).abs() < 1e-12))
    }

    fn near3(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|j| (a[j] - b[j]).abs() < 1e-12)
    }

    fn t(rows: [Vec3; 3], translation: Vec3) -> Transform {
        Transform { rows, translation }
    }

    #[test]
    fn decompose_and_assemble() {
        let a = LinkArithmetic::of(RenderAs::Bryce30);
        let m = [[2.0, 0.0, 0.0], [0.0, 0.0, 3.0], [0.0, -4.0, 0.0]];
        let p = a.decompose(&m).unwrap();
        assert_eq!(p.scales, [2.0, 3.0, 4.0]);
        assert_eq!(p.shear, [0.0, 0.0, 0.0]);
        assert_eq!(a.assemble(&p), m);
        // A mirror: step 4 changes the sign of all three scales and of the rows of
        // the rotation, which keeps determinant +1.
        let p = a.decompose(&[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -2.0]]).unwrap();
        assert_eq!(p.scales, [-1.0, -1.0, -2.0]);
        assert_eq!(p.turn[2], [0.0, 0.0, 1.0]);
        assert!(a.decompose(&[[0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]).is_none());
    }

    #[test]
    fn bits() {
        // : P = scale 2 with a quarter turn about z, at (1, 2, 3); S = place (5, 0, 0).
        let p = t([[0.0, 2.0, 0.0], [-2.0, 0.0, 0.0], [0.0, 0.0, 2.0]], [1.0, 2.0, 3.0]);
        let s = t([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [5.0, 0.0, 0.0]);
        let v30 = RenderAs::Bryce30;
        let v31 = RenderAs::Bryce31;
        let a = LinkArithmetic::of(v30);
        let b = LinkArithmetic::of(v31);
        // No bits: S·P.
        assert_eq!(a.chain(&s, 0, &p, v30), s.followed_by(&p));
        // 3.0: 0x80000 alone changes nothing; with 0x400000 the translation and every scale go.
        assert_eq!(a.chain(&s, BIT_PLACE_ADDED, &p, v30), s.followed_by(&p));
        let c = a.chain(&s, BIT_PLACE_ADDED | BIT_NO_PLACE, &p, v30);
        assert!(near(&c.rows, &[[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]));
        assert!(near3(c.translation, [0.0, 5.0, 0.0]));
        // 3.0: 0x200000: Q the identity, translation kept.
        let c = a.chain(&s, BIT_NO_TURN, &p, v30);
        assert!(near(&c.rows, &[[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]]));
        assert!(near3(c.translation, [11.0, 2.0, 3.0]));
        // 3.1: 0x200000: the place is where P carries S's translation.
        let c = b.chain(&s, BIT_NO_TURN, &p, v31);
        assert!(near3(c.translation, [1.0, 12.0, 3.0]));
        // 3.1: 0x400000: S's translation; 0x80000: S's plus P's.
        assert!(near3(b.chain(&s, BIT_NO_PLACE, &p, v31).translation, [5.0, 0.0, 0.0]));
        assert!(near3(b.chain(&s, BIT_PLACE_ADDED, &p, v31).translation, [6.0, 2.0, 3.0]));
        // 3.1: 0x100000: scales 1, the turn kept.
        assert!(near(&b.chain(&s, BIT_NO_SCALE, &p, v31).rows, &[[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]));
    }
}
