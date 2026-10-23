// SPDX-License-Identifier: MIT
//! The Premium samples' own rays as 5.0: where a blurred reflected or transmitted ray is
//! turned inside its cone, and which way the true ambience's one ambient ray goes.
//!
//! points 1 to 5 (
//! , ambience-directions.jsonl), (The blurred rays: the
//! cone's radius),
//! (Premium rays: cones and the ambient ray)
//!
//! Pure functions of numbers. The generator's two draws are arguments (the first drawn
//! first): when they are drawn is the render's (Premium samples that
//! draw in their trees). Whether a ray is turned at all is the caller's test.

use crate::geom::Vec3;
use crate::numerics::{rsqrt_as, sqrt_as, x87};
use crate::shading::lighting::SumOrder;
use crate::shading::further::{RayKind, switch};
use crate::shading::media::{BLUR_DISTANCE, BlurOrders, FrameSum, FrameTerm, HaloTerm, blur_radius, frame_turn};
use crate::version::RenderAs;

use super::samples::RAND_MAX;

/// The rendering version these rules exist in.
const FIVE: RenderAs = RenderAs::Bryce50;

/// E: the binary64 number nearest 1/32767.
const E: f64 = 1.0 / RAND_MAX;
/// The binary64 number nearest 2π, by which X is multiplied for the disc's angle.
const TWO_PI: f64 = std::f64::consts::TAU; // point 2 (the double nearest 2π)
/// ε: the binary64 number nearest 1.745329251994e-2 (not π/180). point 5
const DEGREE: f64 = 1.745329251994e-2; // point 5 (ε)
/// The tilt's scale: τ = Y·180. point 5
const TILT_SCALE: f64 = 180.0; // point 5
/// The fold of the tilt: φ = |τ − 90|·ε. point 5
const TILT_FOLD: f64 = 90.0; // point 5
/// The turn's scale: ψ = (X·360)·ε. point 5
const TURN_SCALE: f64 = 360.0; // point 5

/// The arrangement of the cone's turn: the order of a² + b² + 100² and of the frame's terms.
/// The one blur.jsonl selects, which blur-cones.jsonl gives too (tests/sampling_premium_vectors.rs).
/// Accuracy ("the cone's arrangement also gives every row of's blur.jsonl")
pub const CONE_ORDERS: BlurOrders = BlurOrders::BRYCE50;

/// The arrangement of the ambient ray's turn: the order of a0² + b0² + c0² and of the frame's
/// terms, the one ambience-directions.jsonl selects (tests/sampling_premium_vectors.rs tries
/// every candidate). Accuracy (left to the vectors)
pub const AMBIENT_ORDERS: BlurOrders = BlurOrders {
    norm: SumOrder::XyZ,
    term: FrameTerm::OwnAxisFirst,
    halo: HaloTerm::ProductFirst,
    sum: FrameSum::FrameFirst,
};

/// The point of a sample's cell: X binary32, Y binary64 (not rounded). point 1
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellPoint {
    pub x: f32,
    pub y: f64,
}

/// The point of the cell of sample `number` of an `n` by `n` grid, moved by the two draws
/// (the first for X, the second for Y). point 1
pub fn cell_point(number: u32, n: u32, draws: [u16; 2]) -> CellPoint {
    let w = 1.0 / f64::from(n);
    let (col, row) = (number % n, number / n);
    let cx = ((f64::from(col) + 0.5) * w) as f32;
    let cy = ((f64::from(row) + 0.5) * w) as f32;
    let p1 = f64::from(draws[0]) * E - 0.5;
    let p2 = f64::from(draws[1]) * E - 0.5;
    CellPoint { x: (w * p1 + f64::from(cx)) as f32, y: w * p2 + f64::from(cy) }
}

/// The point (a, b) of a disc of radius `rho` that the cell point maps to: X gives the angle,
/// ⟨Y⟩ the radius by its root. point 2
pub fn disc_point(rho: f64, cell: CellPoint) -> [f32; 2] {
    let theta = f64::from(cell.x) * TWO_PI;
    let r = rho * sqrt_as(FIVE, f64::from(cell.y as f32));
    [(r * x87::cos_f64(theta)) as f32, (r * x87::sin_f64(theta)) as f32]
}

/// Whether a further ray of kind `kind` is turned in its cone, and the cone's radius ρb if so:
/// the ray carries a sample number (`numbered`), the switch word has its own bit (0x800000
/// for the reflected ray, 0x1000000 for the transmitted one, each independent of the other
/// effects) and ρb (from the Specular Halo colour) is not 0. point 4
pub fn cone_radius(switches: u32, kind: RayKind, numbered: bool, halo: [f32; 3]) -> Option<f64> {
    let bit = match kind {
        RayKind::Reflected => switch::BLUR_REFLECTED,     // point 4 (0x800000)
        RayKind::Transmitted => switch::BLUR_TRANSMITTED, // point 4 (0x1000000 [format])
    };
    if !numbered || switches & bit == 0 {
        return None;
    }
    let radius = blur_radius(halo);
    (radius != 0.0).then_some(radius)
}

fn sum_of_squares(order: SumOrder, v: [f64; 3]) -> f64 {
    order.dot(v, v)
}

/// A blurred ray's direction δ turned toward the disc point of radius ρb standing 100 along
/// it: T(δ; k·a, k·b, k·100), k = Q(a² + b² + 100²). Not brought to unit length.
/// point 4
pub fn cone_turn(direction: Vec3, disc: [f32; 2], orders: BlurOrders) -> Vec3 {
    let (a, b) = (f64::from(disc[0]), f64::from(disc[1]));
    let k = rsqrt_as(FIVE, sum_of_squares(orders.norm, [a, b, BLUR_DISTANCE]));
    frame_turn(direction, [k * a, k * b, k * BLUR_DISTANCE], orders)
}

/// The point (a0, b0, c0) of the unit sphere the cell point gives, before the frame: τ = Y·180,
/// φ = |τ − 90|·ε, ψ = (X·360)·ε, s = FSIN(φ) rounded, (FSIN(ψ)·s, FCOS(ψ)·s, FCOS(φ)), each
/// rounded once to binary64. point 5
pub fn ambient_local(cell: CellPoint) -> [f64; 3] {
    let tau = cell.y * TILT_SCALE;
    let phi = (tau - TILT_FOLD).abs() * DEGREE;
    let psi = (f64::from(cell.x) * TURN_SCALE) * DEGREE;
    let s = x87::sin_f64(phi);
    [x87::sin_ext(psi).mul_to_f64(s), x87::cos_ext(psi).mul_to_f64(s), x87::cos_f64(phi)]
}

/// The ambient ray's direction about the geometric normal g, from the cell point (X and the
/// unrounded Y): even in the angle φ from the normal (folded at 90 degrees) and in the turn ψ
/// about it. point 5
pub fn ambient_direction(normal: Vec3, cell: CellPoint, orders: BlurOrders) -> Vec3 {
    let [a0, b0, c0] = ambient_local(cell);
    let k = rsqrt_as(FIVE, sum_of_squares(orders.norm, [a0, b0, c0]));
    frame_turn(normal, [k * a0, k * b0, k * c0], orders)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// point 1: X is 's place in the cell (`premium_offset`) to the bit; Y is
    /// the same sum unrounded.
    #[test]
    fn cell_point_is_the_place_in_the_cell() {
        for n in 1..=16u32 {
            for s in [0, n * n / 2, n * n - 1] {
                for d in [0u16, 1, 16384, 32767] {
                    let c = cell_point(s, n, [d, d]);
                    let x = super::super::samples::premium_offset(s % n, f64::from(d) * E, n);
                    assert_eq!(f64::from(c.x).to_bits(), x.to_bits());
                    let y = super::super::samples::premium_offset(s / n, f64::from(d) * E, n);
                    assert_eq!(f64::from(c.y as f32).to_bits(), y.to_bits());
                }
            }
        }
    }

    /// point 5: the direction is of unit length where g is, and never below the side g
    /// points to.
    #[test]
    fn ambient_direction_is_above_the_surface() {
        let g = [0.0, 1.0, 0.0];
        for s in 0..16 {
            for d in [0u16, 9000, 32767] {
                let v = ambient_direction(g, cell_point(s, 4, [d, 32767 - d]), AMBIENT_ORDERS);
                let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-12 && v[1] >= 0.0, "{v:?}");
            }
        }
    }
}
