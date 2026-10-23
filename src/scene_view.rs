// SPDX-License-Identifier: MIT
//! The view's transform: the roll made into the first transform, and the perspective
//! camera's rows made from its angles as 3.0.
//!
//! (its transform: the roll; the perspective camera's angles; the animation
//! switch and the version),,,
//! point 1,
//! (the view's transform; the arithmetic fitted to
//! view-turned.jsonl)
//!
//! gives the two functions and leaves their last bits to the vectors (Accuracy: "Open:
//! the arithmetic that gives the vectors' last bits"). The arithmetic below is this reader's
//! own choice among evaluations of those functions, kept because it gives all 72 results of
//! ` ` to the bit (design note, "fitted arithmetic").

use crate::animation::{OpeningInputs, OpeningRule, tracks_applied_on_opening};
use crate::geom::{Vec3, dot};
use crate::transform_parts::{product_h, roll_first, turn_second};
use crate::scene_records::Transform;
use crate::version::RenderAs;

/// How close a rolled camera's first transform must be to its scales times the turn by the
/// roll to count as the form states ("to within 1e-9"); farther, the result is this
/// reader's choice.
const FORM_TOLERANCE: f64 = 1e-9;

/// The view's transform: given by the entries, by this reader's choice, or Open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewTransform {
    /// states it: the first transform (made with the roll where the roll is not 0)
    /// followed by the second (its rows made from the angles for "perspective" as 3.0).
    Stated(Transform),
    /// A first transform leaves to the implementer (with roll 0 not diagonal; with a
    /// roll, not its scales times the turn by the roll): computed by the same rules.
    Chosen(Transform),
    /// The entries leave it Open; the document is read all the same.
    Open(OpenView),
}

/// Why a view's transform is Open, with the stand-in this reader keeps.,
/// point 1 (Freedom: what a reader gives for these rows until they are specified)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenView {
    /// The transform with the stored rows (as 3.1 and 5.0 with the switch 0): a stand-in
    /// only, which no entry gives for this case.
    pub stand_in: Transform,
}

impl ViewTransform {
    /// The transform, where the entries or this reader's choice give one.
    pub fn given(&self) -> Option<Transform> {
        match *self {
            ViewTransform::Stated(t) | ViewTransform::Chosen(t) => Some(t),
            ViewTransform::Open(_) => None,
        }
    }
}

/// The three versions in the order `ViewTransforms` holds them.
pub const VERSIONS: [RenderAs; 3] = [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50];

/// The view's transform as each version opens the document. (Inputs: the
/// version the document is opened as)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransforms(pub [ViewTransform; 3]);

impl ViewTransforms {
    pub fn get(&self, version: RenderAs) -> ViewTransform {
        self.0[slot(version)]
    }

    /// The same transform for every version (for callers that build a view of their own).
    pub fn all(t: ViewTransform) -> ViewTransforms {
        ViewTransforms([t; 3])
    }
}

fn slot(version: RenderAs) -> usize {
    match version {
        RenderAs::Bryce30 => 0,
        RenderAs::Bryce31 => 1,
        RenderAs::Bryce50 => 2,
    }
}

/// The view's transform for each version.,
///
/// `perspective`: the camera is the first named "perspective" (the only one whose rows are
/// made from its angles); `opening`: what opening reads of the document ('s saved time
/// at offset 68 and switch byte at offset 95, the form of its parts).
///
/// A view other than "perspective" is the same whatever the switch (point
/// 2: a "director" without tracks as with the switch 0); its tracks, where it has keys, are
/// the caller's to decide (`Opening::view_camera_tracks_applied`).
pub fn view_transforms(
    first: &Transform,
    second: &Transform,
    angles: [f64; 3],
    roll: f64,
    perspective: bool,
    opening: OpeningInputs,
) -> ViewTransforms {
    // The roll, every camera, every version, whatever the switch. (the
    // roll: the first transform taken apart and put together around the roll's quaternion).
    // The older `rolled_first` stays for whether states the form and for a first
    // transform that cannot be taken apart .
    let (first_old, first_stated) = rolled_first(first, roll);
    let first_of = |version: RenderAs| -> Transform {
        if roll == 0.0 {
            first_old
        } else {
            roll_first(version, first, roll).unwrap_or(first_old)
        }
    };
    let make = |version: RenderAs, second: &Transform| -> Transform {
        // (H of the first and the second; with the identity first the
        // product still turns a stored -0 into 0: view-switch.jsonl)
        product_h(&first_of(version), second)
    };
    let tag = |t: Transform| if first_stated { ViewTransform::Stated(t) } else { ViewTransform::Chosen(t) };
    if !perspective {
        return ViewTransforms(VERSIONS.map(|version| tag(make(version, second))));
    }
    // Where opening applies the tracks ('s one rule, shared with `Opening`): the rows
    // made from the angles, the translation kept (3.0, whatever the switch). Where it
    // applies none: the stored rows (3.1 and 5.0 with the switch 0). Where what it applies
    // is Open: Open (3.1 with the switch not 0; 5.0 with it not 0 in Bryce 3D's form or at
    // another saved time). point 1
    // Where 5.0 applies the other owners' tracks (a marked document of Bryce 5's form saved
    // at 0), whether these rows are made from the angles is Open: Open. point
    // 1, point 1
    ViewTransforms(VERSIONS.map(|version| match tracks_applied_on_opening(version, opening) {
        // The switch not 0 in 3.1 and 5.0: the rows are made from the angles as 3.0 does
        // (: observed), whatever else the opening applies.
        Ok(OpeningRule::EveryTrack | OpeningRule::Marked { .. } | OpeningRule::EveryTrackButViewCameras) | Err(_) => {
            let made = turn_second(version, angles[0], angles[1], second.translation);
            tag(make(version, &made))
        }
        Ok(OpeningRule::AsStored) => tag(make(version, second)),
    }))
}

/// The two transforms whose product is the view's (`view_transforms`), per version: the first
/// with the roll, and the second as the version makes it (its rows made from the angles where
/// `view_transforms` makes them). A camera linked below an object follows its second transform
/// alone by the chain's part , so the pieces are needed apart.
pub fn view_pieces(
    first: &Transform,
    second: &Transform,
    angles: [f64; 3],
    roll: f64,
    perspective: bool,
    opening: OpeningInputs,
) -> [(Transform, Transform); 3] {
    let (first_old, _) = rolled_first(first, roll);
    VERSIONS.map(|version| {
        let f = if roll == 0.0 { first_old } else { roll_first(version, first, roll).unwrap_or(first_old) };
        let s = match tracks_applied_on_opening(version, opening) {
            Ok(OpeningRule::AsStored) => *second,
            _ if !perspective => *second,
            _ => turn_second(version, angles[0], angles[1], second.translation),
        };
        (f, s)
    })
}

/// The rows Rx(p)·Ry(y) of as `version` forms them: the turn from two angles.
pub fn rows_from_angles(version: RenderAs, p: f64, y: f64) -> [Vec3; 3] {
    turn_second(version, p, y, [0.0; 3]).rows
}

/// The turn by the roll about the camera's own z axis, rows (cos r, −sin r, 0),
/// (sin r, cos r, 0), (0, 0, 1).. (fitted
/// arithmetic): the cosine and sine are taken from the half angle, cos r = 1 − 2·sin²(r/2)
/// and sin r = 2·cos(r/2)·sin(r/2).
pub fn roll_turn(r: f64) -> [Vec3; 3] {
    let (h_s, h_c) = (r / 2.0).sin_cos();
    let c = 1.0 - 2.0 * (h_s * h_s);
    let s = 2.0 * (h_c * h_s);
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// The first transform made with the roll, and whether states the result (true) or
/// leaves it to the implementer (false).
///
/// Roll 0: a diagonal matrix is used as it is stored (stated); any other is used as stored
/// too (the implementer's choice). Roll not 0: the matrix is written D·L·Q by Gram–Schmidt
/// of its rows in order, Q taken with determinant +1 (D's last sign following), and Q
/// replaced by the turn by the roll; the translation is kept. Stated when the matrix is its
/// scales times that turn to within 1e-9 (no shear, Q the turn); otherwise the same
/// computation is this reader's choice. A row of length 0 (no decomposition): the stored
/// matrix, the implementer's choice.
pub fn rolled_first(first: &Transform, roll: f64) -> (Transform, bool) {
    if roll == 0.0 {
        return (*first, is_diagonal(first));
    }
    let m = &first.rows;
    let d0 = dot(m[0], m[0]).sqrt();
    let q0 = [m[0][0] / d0, m[0][1] / d0, m[0][2] / d0];
    let a10 = dot(m[1], q0);
    let r1 = [m[1][0] - a10 * q0[0], m[1][1] - a10 * q0[1], m[1][2] - a10 * q0[2]];
    let d1 = dot(r1, r1).sqrt();
    let q1 = [r1[0] / d1, r1[1] / d1, r1[2] / d1];
    let a20 = dot(m[2], q0);
    let a21 = dot(m[2], q1);
    let r2: Vec3 = std::array::from_fn(|k| m[2][k] - a20 * q0[k] - a21 * q1[k]);
    let mut d2 = dot(r2, r2).sqrt();
    let usable = [d0, d1, d2].iter().all(|d| d.is_finite() && *d > 0.0);
    if !usable {
        return (*first, false); // (no decomposition: as stored)
    }
    let q2 = [r2[0] / d2, r2[1] / d2, r2[2] / d2];
    if det(&[q0, q1, q2]) < 0.0 {
        d2 = -d2; // Q with determinant +1, D's sign following
    }
    let q2 = if d2 < 0.0 { [-q2[0], -q2[1], -q2[2]] } else { q2 };
    let (l10, l20, l21) = (a10 / d1, a20 / d2, a21 / d2);
    let z = roll_turn(roll);
    let rows = [
        std::array::from_fn(|k| d0 * z[0][k]),
        std::array::from_fn(|k| d1 * (l10 * z[0][k] + z[1][k])),
        std::array::from_fn(|k| d2 * ((l20 * z[0][k] + l21 * z[1][k]) + z[2][k])),
    ];
    let near = |a: Vec3, b: Vec3| (0..3).all(|k| (a[k] - b[k]).abs() <= FORM_TOLERANCE);
    let stated = d2 > 0.0
        && [l10, l20, l21].iter().all(|l| l.abs() <= FORM_TOLERANCE)
        && near(q0, z[0])
        && near(q1, z[1])
        && near(q2, z[2]);
    (Transform { rows, translation: first.translation }, stated)
}

fn det(m: &[Vec3; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// A transform whose matrix has zeros off its diagonal (its translation free).
pub fn is_diagonal(t: &Transform) -> bool {
    (0..3).all(|i| (0..3).all(|j| i == j || t.rows[i][j] == 0.0))
}
