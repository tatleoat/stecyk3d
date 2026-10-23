// SPDX-License-Identifier: MIT
//! Fuzzy solids: the weight F with which a fuzzy surface counts against what lies behind it,
//! the mix of the two colours, and a shadow ray's amount through such a surface.
//!
//! (The weight of a solid; the fuzzy surface of a ray that is not a shadow ray,
//! step 4; Shadow rays), released
//! (Fuzzy solids: The weight)

use crate::geom::Vec3;
use crate::numerics::sqrt_as;
use crate::version::RenderAs;

/// The order of the sum of the three squares e_x² + e_y² + e_z², which leaves to
/// ` `. (Fuzzy solids)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SquareSum {
    /// (x² + y²) + z²
    XyZ,
    /// x² + (y² + z²)
    XYz,
    /// (x² + z²) + y²
    XzY,
}

impl SquareSum {
    pub const ALL: [SquareSum; 3] = [SquareSum::XyZ, SquareSum::XYz, SquareSum::XzY];

    pub fn sum(self, e: Vec3) -> f64 {
        let [x, y, z] = e.map(|v| v * v);
        match self {
            SquareSum::XyZ => (x + y) + z,
            SquareSum::XYz => x + (y + z),
            SquareSum::XzY => (x + z) + y,
        }
    }
}

/// The arrangements leaves to the vectors. (Fuzzy solids)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuzzyOrders {
    pub squares: SquareSum,
}

impl FuzzyOrders {
    /// The arrangement `fuzzy.jsonl` decides, the same in every version: x² + (y² + z²)
    /// (tests/shading_fuzzy.rs; the other two fail 22 to 29 rows a version).
    pub fn of(_version: RenderAs) -> FuzzyOrders {
        FuzzyOrders { squares: SquareSum::XYz }
    }
}

/// A world point carried into O's own space by the inverse of its object-to-world map (
/// `inverse` the cofactor inverse's rows, `translation` the map's): p·M⁻¹ − T·M⁻¹, each a row
/// vector times the inverse, then the difference. The arrangement is left to the vectors
/// ; `fuzzy.jsonl` gives every row with this one and fails 3 / 4 / 4 rows with
/// (p − T)·M⁻¹. (Fuzzy solids)
pub fn to_own_space(inverse: &[Vec3; 3], translation: Vec3, p: Vec3) -> Vec3 {
    let times = |v: Vec3| -> Vec3 {
        std::array::from_fn(|j| v[0] * inverse[0][j] + v[1] * inverse[1][j] + v[2] * inverse[2][j])
    };
    let (x, y) = (times(p), times(translation));
    [x[0] - y[0], x[1] - y[1], x[2] - y[2]]
}

/// The width's constants: w = 2 − (0.5 − a)·1.8.
const WIDTH_BASE: f64 = 2.0;
const WIDTH_MID: f64 = 0.5;
const WIDTH_SLOPE: f64 = 1.8;
/// u is held to at most 2.
const U_HOLD: f64 = 2.0;
/// The width is held to at least 1 and at most 3.
const WIDTH_LEAST: f64 = 1.0;
const WIDTH_MOST: f64 = 3.0;

/// The scale on one axis that makes an object's own box measure 2: 2/(hi - lo), 1 where
/// hi = lo.
pub fn box_scale(lo: f64, hi: f64) -> f64 {
    if hi == lo { 1.0 } else { 2.0 / (hi - lo) }
}

/// F for a sphere or a cone, from the points `p` and `q` already carried into O's own space,
/// the Transparency's A value `a` and the Transparency `t` at the hit. (The
/// weight of a solid)
pub fn solid_weight(version: RenderAs, p: Vec3, q: Vec3, a: f64, t: f64, orders: FuzzyOrders) -> f64 {
    solid_weight_scaled(version, p, q, [1.0; 3], a, t, orders)
}

/// F for any solid: `solid_weight` with each coordinate of `p` and `q` first multiplied by the
/// scale of the object's own box (`box_scale`), so that the box measures 2 on every axis.
pub fn solid_weight_scaled(version: RenderAs, p: Vec3, q: Vec3, scale: Vec3, a: f64, t: f64, orders: FuzzyOrders) -> f64 {
    let e: Vec3 = std::array::from_fn(|k| q[k] * scale[k] - p[k] * scale[k]);
    let d = sqrt_as(version, orders.squares.sum(e));
    let mut w = WIDTH_BASE - (WIDTH_MID - a) * WIDTH_SLOPE;
    if w < WIDTH_LEAST {
        w = WIDTH_LEAST;
    } else if w > WIDTH_MOST {
        w = WIDTH_MOST;
    }
    let ratio = d / w;
    let mut u = ratio * ratio;
    // : u = 2 where it is above 2 or is not a number
    if u > U_HOLD || u.is_nan() {
        u = U_HOLD;
    }
    // : where t is 0, f = 1 - u whatever a (the term (t - a)(a + 1) is not added)
    let f = if t == 0.0 { 1.0 - u } else { (t - a) * (a + 1.0) + (1.0 - u) };
    if f < 0.0 {
        1.0
    } else if f > 1.0 {
        0.0
    } else {
        1.0 - f
    }
}

/// F for a plane: 1 − t.
pub fn plane_weight(t: f64) -> f64 {
    1.0 - t
}

/// A shadow ray's amount where it leaves O (or meets a plane): 1 − F·(1 − a).
pub fn shadow_amount(f: f64, a: f64) -> f64 {
    1.0 - f * (1.0 - a)
}

/// The ray's colour b + F·(o − b), `o` the surface's colour as lit (with the air), `b` what the
/// continuing ray brings back, F a double: red ⟨⟨F·(o − b)⟩ + b⟩, green ⟨⟨F·⟨o − b⟩⟩ + b⟩ and
/// blue ⟨F·⟨o − b⟩ + b⟩ (⟨⟩ a rounding to binary32).
pub fn mix(o: [f32; 3], b: [f32; 3], f: f64) -> [f32; 3] {
    let r32 = |x: f64| f64::from(x as f32);
    let (o, b) = (o.map(f64::from), b.map(f64::from));
    let red = r32(f * (o[0] - b[0])) + b[0];
    let green = r32(f * r32(o[1] - b[1])) + b[1];
    let blue = f * r32(o[2] - b[2]) + b[2];
    [red as f32, green as f32, blue as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : for a plain material F = u below the width and 1 at it or beyond.
    #[test]
    fn plain_weight_is_u() {
        let o = FuzzyOrders::of(RenderAs::Bryce31);
        let a = 0.5; // w = 2
        let f = solid_weight(RenderAs::Bryce31, [0.0; 3], [1.0, 0.0, 0.0], a, a, o);
        assert_eq!(f, 0.25);
        assert_eq!(solid_weight(RenderAs::Bryce31, [0.0; 3], [3.0, 0.0, 0.0], a, a, o), 1.0);
        assert_eq!(solid_weight(RenderAs::Bryce31, [0.0; 3], [0.0; 3], a, a, o), 0.0);
    }

    #[test]
    fn mix_ends() {
        assert_eq!(mix([0.5, 0.25, 1.0], [0.0, 1.0, 0.5], 1.0), [0.5, 0.25, 1.0]);
        assert_eq!(mix([0.5, 0.25, 1.0], [0.0, 1.0, 0.5], 0.0), [0.0, 1.0, 0.5]);
        assert_eq!(shadow_amount(1.0, 0.25), 0.25);
        assert_eq!(plane_weight(0.4), 0.6);
    }
}
