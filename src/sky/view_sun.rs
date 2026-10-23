// SPDX-License-Identifier: MIT
//! The sun's direction in the view's frame: the direction S' every rule of the sky means by "the
//! sun's direction" when the switch word has bit 0x200.
//!
//!
//! Every operation is one binary64 operation in the order the entry writes (no fused
//! multiply-add, no reordering: the grouping is the one the vectors need).

use crate::geom::Vec3;

/// Bit 0x200 of the sky's switch word: the sun is set in the camera's frame.
pub const SUN_IN_VIEW: u32 = 0x200;

/// S', the inverse of the camera's matrix (rows right, up, forward, as the document gives them,
/// none made unit) applied to S as a column. `five` selects the order of the three-term sums
/// for the second and third components, which differs as 5.0. The camera's position plays no
/// part. A singular matrix gives the inverse of its determinant as the arithmetic gives it (an
/// infinity or a number that is not a number; no document holds one).
pub fn sun_in_world(five: bool, rows: [Vec3; 3], s: Vec3) -> Vec3 {
    apply_inverse(five, &inverse_of(rows), s)
}

/// The inverse of a matrix given by rows, made as makes the camera's: the nine entries of
/// the adjugate, δ in the grouping written there, k = 1/δ and vj,i = a(3j+i)·k.'s field
/// turn makes R⁻¹ the same way.,
/// (The field's turn)
pub fn inverse_of(rows: [Vec3; 3]) -> [Vec3; 3] {
    let [r0, r1, r2] = rows[0];
    let [r3, r4, r5] = rows[1];
    let [r6, r7, r8] = rows[2];
    // The nine entries of the adjugate, each the difference of two products.
    let a = [
        r8 * r4 - r5 * r7,
        -(r8 * r1 - r2 * r7),
        r5 * r1 - r4 * r2,
        -(r8 * r3 - r5 * r6),
        r8 * r0 - r6 * r2,
        -(r5 * r0 - r2 * r3),
        r7 * r3 - r6 * r4,
        -(r7 * r0 - r1 * r6),
        r4 * r0 - r3 * r1,
    ];
    // The determinant and its inverse.
    let delta = ((r8 * r4 - r7 * r5) * r0 - (r8 * r3 - r5 * r6) * r1) + (r7 * r3 - r6 * r4) * r2;
    let k = 1.0 / delta;
    // The inverse's entry (row j, column i).
    std::array::from_fn(|j| std::array::from_fn(|i| a[3 * j + i] * k))
}

/// The inverse `v` applied to s as a column: S′j = (vj,1·Sy + vj,2·Sz) + vj,0·Sx, and as 5.0
/// for j = 1 and 2 (vj,1·Sy + vj,0·Sx) + vj,2·Sz., (D′ = R⁻¹·D summed as
/// 5.0 sums S′)
pub fn apply_inverse(five: bool, v: &[Vec3; 3], s: Vec3) -> Vec3 {
    std::array::from_fn(|j| {
        let r = v[j];
        if five && j != 0 { (r[1] * s[1] + r[0] * s[0]) + r[2] * s[2] } else { (r[1] * s[1] + r[2] * s[2]) + r[0] * s[0] }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // (for rows unit and at right angles the inverse is the transpose: S' =
    // Sx·r(0..2) + Sy·r(3..5) + Sz·r(6..8))
    #[test]
    fn a_turn_is_carried_by_its_transpose() {
        let (c, s) = (0.6, 0.8);
        let rows = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
        let sun = [0.0, 0.0, 1.0];
        for five in [false, true] {
            let got = sun_in_world(five, rows, sun);
            let want = [-s, 0.0, c]; // Sz times the forward row
            for k in 0..3 {
                assert!((got[k] - want[k]).abs() < 1e-15, "{got:?} {want:?}");
            }
        }
    }

    #[test]
    fn the_identity_changes_nothing() {
        let rows = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let sun = [0.3, 0.5, -0.2];
        assert_eq!(sun_in_world(false, rows, sun), sun);
        assert_eq!(sun_in_world(true, rows, sun), sun);
    }
}
