// SPDX-License-Identifier: MIT
//! The lens of Bryce 5's Premium setting (depth of field): a camera ray of a Premium sample
//! moved to a point of the lens and aimed at the point where it meets the plane of focus.
//!
//! point 3 (to)
//! (The lens)

use crate::geom::Vec3;
use crate::numerics::{rsqrt_as, sqrt_as, x87};
use crate::shading::lighting::SumOrder;
use crate::version::RenderAs;

use super::batch::CameraRay;
use super::samples::{RAND_MAX, premium_offset};

/// The camera ray runs along the plane of focus when |F · d| is below this. point 3
const ALONG_PLANE: f64 = 1e-10; // point 3

/// The binary64 number nearest 2π, by which x' is multiplied for the angle. point 3
const TWO_PI: f64 = std::f64::consts::TAU; // point 3 (the double nearest 2π)

/// The lens's two numbers from the document (Bryce 5's part): the focal distance f and the
/// lens radius ρ, binary32 values taken as they are. point 2
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lens {
    pub focal_distance: f32,
    pub radius: f32,
}

/// The camera's frame the lens works in:'s unit right, up and forward axes and the
/// image plane's centre C, to the last bit those of the camera rays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensFrame {
    pub right: Vec3,
    pub up: Vec3,
    pub forward: Vec3,
    pub centre: Vec3,
}

/// How the origin is moved by a along R and b along U. point 3 (the order of
/// the two moves is the vectors')
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoveOrder {
    /// (O + a R) + b U
    RightThenUp,
    /// (O + b U) + a R
    UpThenRight,
    /// O + (a R + b U)
    Together,
}

impl MoveOrder {
    pub const ALL: [MoveOrder; 3] = [MoveOrder::RightThenUp, MoveOrder::UpThenRight, MoveOrder::Together];
}

/// The orders point 3 leaves to the two products with F, the sum of
/// squares and the two moves of the origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LensOrders {
    /// d · F
    pub ray_dot: SumOrder,
    /// (C + f F − O) · F
    pub plane_dot: SumOrder,
    /// The direction's sum of squares.
    pub squares: SumOrder,
    pub moves: MoveOrder,
}

impl LensOrders {
    /// The one order of each that gives every case of lens.jsonl: both products with F
    /// (y + z) + x, the sum of squares (x + y) + z, the move along R first. Of the 81
    /// combinations tests/sampling_vectors.rs tries, no other gives all 300.
    /// point 3 ("the vectors decide them")
    pub const BRYCE50: LensOrders = LensOrders {
        ray_dot: SumOrder::YzX,
        plane_dot: SumOrder::YzX,
        squares: SumOrder::XyZ,
        moves: MoveOrder::RightThenUp,
    };
}

/// What the lens made of a camera ray: the ray traced, and where it moved it, the point of
/// the lens (a, b) and the point of focus Q.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensRay {
    pub ray: CameraRay,
    /// a along R and b along U, each a binary32 value.
    pub disc: [f32; 2],
    pub focus: Vec3,
}

impl Lens {
    /// Whether the lens moves a ray at all: f and ρ both other than 0. point 3
    pub fn acts(&self) -> bool {
        self.focal_distance != 0.0 && self.radius != 0.0
    }

    /// The lens on the camera ray of a Premium sample with number `number` of an `n` by `n`
    /// grid, rendered as 5.0 with the depth-of-field switch set. `None` where it leaves the
    /// ray unchanged and takes no draw: f or ρ is 0, or the ray runs along the plane of focus.
    /// Otherwise `draw` is called twice, the first for x', then for y'.
    /// point 3, (the draws)
    pub fn ray(
        &self,
        frame: &LensFrame,
        camera: &CameraRay,
        number: u32,
        n: u32,
        orders: LensOrders,
        mut draw: impl FnMut() -> u16,
    ) -> Option<LensRay> {
        if !self.acts() {
            return None;
        }
        let (o, d, fw) = (camera.origin, camera.dir, frame.forward);
        let d_f = orders.ray_dot.dot(d, fw);
        if !(d_f.abs() >= ALONG_PLANE) {
            return None;
        }
        // The point of focus: Q = O + t d, t = ((C + f F − O) · F) / (d · F), binary64.
        let f = f64::from(self.focal_distance);
        let to_plane: Vec3 = std::array::from_fn(|k| (frame.centre[k] + f * fw[k]) - o[k]);
        let t = orders.plane_dot.dot(to_plane, fw) / d_f;
        let focus: Vec3 = std::array::from_fn(|k| o[k] + t * d[k]);
        // The point of the lens: the cell's centre moved by the two draws as 's places
        // each coordinate rounded to binary32; then the polar map.
        let (col, row) = (number % n, number / n);
        let x1 = premium_offset(col, f64::from(draw()) * (1.0 / RAND_MAX), n);
        let y1 = premium_offset(row, f64::from(draw()) * (1.0 / RAND_MAX), n);
        let theta = x1 * TWO_PI;
        let r = f64::from(self.radius) * sqrt_as(RenderAs::Bryce50, y1);
        let a = (r * x87::cos_f64(theta)) as f32;
        let b = (r * x87::sin_f64(theta)) as f32;
        let (af, bf) = (f64::from(a), f64::from(b));
        let (rt, up) = (frame.right, frame.up);
        let origin: Vec3 = std::array::from_fn(|k| match orders.moves {
            MoveOrder::RightThenUp => (o[k] + af * rt[k]) + bf * up[k],
            MoveOrder::UpThenRight => (o[k] + bf * up[k]) + af * rt[k],
            MoveOrder::Together => o[k] + (af * rt[k] + bf * up[k]),
        });
        let towards: Vec3 = std::array::from_fn(|k| focus[k] - origin[k]);
        let q = rsqrt_as(RenderAs::Bryce50, orders.squares.dot(towards, towards));
        let dir = towards.map(|c| q * c);
        Some(LensRay { ray: CameraRay { origin, dir, start: camera.start }, disc: [a, b], focus })
    }
}
