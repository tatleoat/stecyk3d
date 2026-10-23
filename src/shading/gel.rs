// SPDX-License-Identifier: MIT
//! A gel's hit: where a light's textured material is evaluated for a point it lights.
//!
//! (the inverse an object's rays are carried by);
//! (A gel's hit)

use crate::geom::Vec3;
use crate::geometry::Placement;
use crate::numerics::acos;
use crate::numerics::{rsqrt_as, sqrt_as};
use crate::numerics::x87::{div_f64_by, sin_ext};
use crate::shading::lighting::SumOrder;
use crate::version::RenderAs;

/// π and 2π, the doubles nearest them.
const PI: f64 = std::f64::consts::PI;
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// The orders of the three-term sums, which `gel-points.jsonl` decides by version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GelOrders {
    /// q·q.
    pub length: SumOrder,
    /// Each coordinate of the world place (its three products; the translation added last).
    pub world: [SumOrder; 3],
    /// Each coordinate of the normal.
    pub normal: [SumOrder; 3],
}

impl GelOrders {
    /// The orders tests/gel_points.rs finds give every case (138 of 138 in each version; as
    /// 3.1 and 5.0 the world place's x is given by (x + y) + z and (x + z) + y alike, and the
    /// first is taken).
    pub fn of(version: RenderAs) -> GelOrders {
        use SumOrder::*;
        match version {
            RenderAs::Bryce30 | RenderAs::Bryce31 => GelOrders { length: YzX, world: [XyZ; 3], normal: [YzX; 3] },
            RenderAs::Bryce50 => GelOrders { length: YzX, world: [XyZ; 3], normal: [YzX, XyZ, XyZ] },
        }
    }
}

/// The hit, every quantity names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GelHit {
    /// p in the light's own space.
    pub q: Vec3,
    /// d, the hit's place in the object's space.
    pub place: Vec3,
    /// d carried to the world.
    pub world: Vec3,
    pub u: f64,
    pub v: f64,
    /// The geometric and shading normal, not of unit length.
    pub normal: Vec3,
}

/// q: p carried by the inverse of the light's object-to-world transform.
pub fn light_space(placement: &Placement, p: Vec3) -> Vec3 {
    placement.point_to_local(p)
}

/// q of a gel's hit as `version` forms it: p carried as a ray's origin is, by's inverse
/// map i for the version, coordinate j being ((px·i(j) + py·i(3 + j)) + pz·i(6 + j)) + i(9 + j).
pub fn light_space_as(version: RenderAs, placement: &Placement, p: Vec3) -> Vec3 {
    placement.to_local_as(version, &crate::geometry::Ray { origin: p, dir: [0.0; 3] }).origin
}

/// d = q · rsqrt(q·q).
pub fn place(version: RenderAs, q: Vec3, order: SumOrder) -> Vec3 {
    let r = rsqrt_as(version, order.dot(q, q));
    q.map(|x| x * r)
}

/// d carried by the object-to-world transform (rows, then the translation added).
pub fn world(d: Vec3, rows: &[Vec3; 3], translation: Vec3, order: [SumOrder; 3]) -> Vec3 {
    std::array::from_fn(|k| order[k].dot(d, [rows[0][k], rows[1][k], rows[2][k]]) + translation[k])
}

/// (u, v): a = arccos(−dy), v = a/π, u = arccos(dx / sin a)/2π; u is 0 where v is exactly 0
/// or 1, and otherwise 1 − u unless dz > 0.
pub fn coordinates(d: Vec3) -> (f64, f64) {
    let a = acos(-d[1]);
    let v = a / PI;
    let u = acos(div_f64_by(d[0], sin_ext(a))) / TWO_PI;
    let u = if v == 0.0 || v == 1.0 {
        0.0
    } else if d[2] > 0.0 {
        u
    } else {
        1.0 - u
    };
    (u, v)
}

/// d carried as a surface normal is (by the transpose of the inverse), not made unit.
pub fn normal(d: Vec3, inverse: &[Vec3; 3], order: [SumOrder; 3]) -> Vec3 {
    std::array::from_fn(|k| order[k].dot(inverse[k], d))
}

/// The whole hit for a point p the light lights.
pub fn gel_hit(version: RenderAs, placement: &Placement, p: Vec3) -> GelHit {
    let o = GelOrders::of(version);
    let q = light_space_as(version, placement, p);
    let d = place(version, q, o.length);
    let (u, v) = coordinates(d);
    GelHit {
        q,
        place: d,
        world: world(d, &placement.rows(), placement.translation(), o.world),
        u,
        v,
        normal: normal(d, &placement.inverse_rows(), o.normal),
    }
}

/// The kinds of light a gel point is made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GelKind {
    Radial,
    Cone,
    Cylinder,
    Pyramid,
    Box,
}

/// The gel point in the light's own space: q, the surface coordinates (U, V), and the
/// direction the normals are carried from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GelPoint {
    pub q: Vec3,
    pub u: f64,
    pub v: f64,
    pub direction: Vec3,
}

/// The order of the sum under Q for each kind (u·u, w·w, and h·h, uz·uz, ux·ux in that
/// order of terms), which ` ` decides, the same in every
/// version: (y + z) + x for u·u and w·w, (h·h + uz·uz) + ux·ux for the pyramid
/// (tests/shading_gel_points.rs).
pub fn gel_point_order(version: RenderAs, kind: GelKind) -> SumOrder {
    match kind {
        GelKind::Radial => GelOrders::of(version).length,
        GelKind::Cone => SumOrder::YzX,
        GelKind::Pyramid => SumOrder::XyZ,
        GelKind::Cylinder | GelKind::Box => SumOrder::XyZ, // no sum under Q
    }
}

/// The gel point for u, the lit point in the light's own space.
pub fn gel_point(version: RenderAs, kind: GelKind, u: Vec3) -> GelPoint {
    gel_point_ordered(version, kind, u, gel_point_order(version, kind))
}

/// `gel_point` with the order of the sum under Q given (for the vector test).
pub fn gel_point_ordered(version: RenderAs, kind: GelKind, u: Vec3, order: SumOrder) -> GelPoint {
    const DOWN: Vec3 = [0.0, -1.0, 0.0];
    let round = |q: Vec3| {
        let rho = sqrt_as(version, q[0] * q[0] + q[2] * q[2]);
        let a = acos(q[0] / rho) / TWO_PI;
        let u = if q[2] < 0.0 { 1.0 - a } else { a };
        GelPoint { q, u, v: rho, direction: DOWN }
    };
    let square = |q: Vec3| GelPoint { q, u: q[0] + 0.5, v: q[2] + 0.5, direction: DOWN };
    match kind {
        GelKind::Radial => {
            let d = place(version, u, order);
            let (gu, gv) = coordinates(d);
            GelPoint { q: d, u: gu, v: gv, direction: d }
        }
        GelKind::Cone => {
            let w = [u[0], u[1] - 1.0, u[2]];
            let kappa = rsqrt_as(version, order.dot(w, w));
            let k = -2.0 / (kappa * w[1]);
            round([k * (kappa * w[0]), -1.0, k * (kappa * w[2])])
        }
        GelKind::Cylinder => round([u[0], -1.0, u[2]]),
        GelKind::Pyramid => {
            let h = u[1] - 0.5;
            // The terms h·h, uz·uz, ux·ux in that order, summed as `order` groups three terms.
            let t = [h, u[2], u[0]];
            let kappa = rsqrt_as(version, order.dot(t, t));
            let k = -1.0 / (kappa * h);
            square([k * (kappa * u[0]), -0.5, k * (kappa * u[2])])
        }
        GelKind::Box => square([u[0], -0.5, u[2]]),
    }
}
