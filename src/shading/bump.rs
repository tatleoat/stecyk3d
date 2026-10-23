// SPDX-License-Identifier: MIT
//! Bump: the shading normal tilted by a texture's gradient.
//! (bump.rs)
//!
//! Fixed by: the gradient comes from the texture  or from three extra
//! samples a small step along each axis (Perlin 1985); the normal is perturbed by it
//! (Blinn 1978) scaled by the bump amount; an option fades bump with distance.
//! Open (parameters here): the scale, the distance fade, the step.

use crate::geom::{self, Vec3};

use super::arith::Arith;

/// Gradient by forward differences: `g_i = (f(p + h e_i) - f0) / h`, where `f0` is the
/// texture's value at `p`, already evaluated. The three extra samples are taken along x,
/// then y, then z (the order matters for carried texture state).
/// The step `h` is Open.
pub fn forward_difference_gradient(
    p: Vec3,
    f0: f64,
    step: f64,
    mut sample: impl FnMut(Vec3) -> f64,
) -> Vec3 {
    let mut g = [0.0; 3];
    for (axis, gi) in g.iter_mut().enumerate() {
        let mut q = p;
        q[axis] += step;
        let f = sample(q);
        *gi = (f - f0) / step;
    }
    g
}

/// Blinn's perturbation with a solid-texture gradient: the gradient's component along the
/// unit normal `n` is removed, `g_t = g - (g.n) n`, and the normal tilts away from it:
/// `normalize(n - factor * g_t)`. `factor` comes from `BumpScale`.
pub fn perturb_normal(n: Vec3, gradient: Vec3, factor: f64, arith: &impl Arith) -> Vec3 {
    let along = geom::dot(gradient, n);
    let tangent = geom::sub(gradient, geom::scale(n, along));
    let tilted = geom::sub(n, geom::scale(tangent, factor));
    let len = arith.sqrt(geom::dot(tilted, tilted));
    if len == 0.0 { n } else { geom::scale(tilted, 1.0 / len) }
}

/// The factor applied to the gradient, from the bump amount and, when the fade option is
/// on, the distance (`None` when it is off). Open.
pub trait BumpScale {
    fn factor(&self, amount: f64, distance: Option<f64>) -> f64;
}

/// The bump settings of a material at a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpInput {
    pub amount: f64,
    pub fade_with_distance: bool,
    pub distance: f64,
}

/// The shading normal , from a gradient obtained either way.
pub fn bumped_normal(
    n: Vec3,
    gradient: Vec3,
    input: &BumpInput,
    scale: &impl BumpScale,
    arith: &impl Arith,
) -> Vec3 {
    let distance = if input.fade_with_distance { Some(input.distance) } else { None };
    let factor = scale.factor(input.amount, distance);
    perturb_normal(n, gradient, factor, arith)
}
