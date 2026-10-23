// SPDX-License-Identifier: MIT
//! Every crossing of a ray's line with a primitive shape, nearest first.
//!
//! , with the shapes' own arithmetic from
//! 's carrying to the world.
//! (Crossings); `carry` is also's (Combined groups)

use super::{
    ENDLESS_DEPTH, FLAT_PIECE_LIMIT, LocalHit, PYRAMID_SIDES, Part, Placement, Ray, SOLID_FACE_LIMIT, Shape, ShapeContext, cone_wall,
    cylinder_wall, cylinder_wall_normal, flat_at, flat_t, plane_limit, pyramid_base, pyramid_side, pyramid_side_from,
    mul_x87, round_cap_at, sphere_roots, texture, torus_roots, wall_roots,
};
use crate::geom::Vec3;
use crate::numerics::{X87_INDEFINITE, atan, quieted, rsqrt_as, sqrt_as};
use crate::version::RenderAs;

/// The plane below the plane, and the slab's ends along it: "the plane y = -1,000,000".
const PADDING_DEPTH: f64 = 1_000_000.0;

/// The thickness the square and the disc are given as solids.
const FLAT_THICKNESS: f64 = 1e-7;

/// The grouping of a squared length's three squares: which two are added first.'s last
/// step on a normal multiplies by q of the squared length; leaves the last bits of the
/// lists' normals to the vectors, which fix the grouping per shape and version
/// geometry.md, Crossings: last bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SumOrder {
    /// x² + (y² + z²):'s hit order (`unit_as`).
    YzFirst,
    /// (x² + y²) + z².
    XyFirst,
    /// (x² + z²) + y².
    XzFirst,
}

impl SumOrder {
    fn squared_length(self, v: Vec3) -> f64 {
        let (x, y, z) = (v[0] * v[0], v[1] * v[1], v[2] * v[2]);
        match self {
            SumOrder::YzFirst => x + (y + z),
            SumOrder::XyFirst => (x + y) + z,
            SumOrder::XzFirst => (x + z) + y,
        }
    }

    /// `v` times q of its squared length in this grouping.
    pub(crate) fn unit(self, version: RenderAs, v: Vec3) -> Vec3 {
        let q = rsqrt_as(version, self.squared_length(v));
        [v[0] * q, v[1] * q, v[2] * q]
    }
}

/// One crossing in the shape's own space. `hit.uv`, `du`, `dv` are filled for a crossing that
/// shows; a crossing that never shows has only `hit.t` meaningful. `unit_order` is the
/// grouping of's last step on its normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalCrossing {
    pub hit: LocalHit,
    pub shows: bool,
    pub unit_order: SumOrder,
}

/// One crossing in the world. For a crossing that never shows only `t` is meaningful (the
/// rest are zeros).,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    pub t: f64,
    pub shows: bool,
    /// The world point.
    pub point: Vec3,
    /// The unit world normal .
    pub normal: Vec3,
    /// u, v, du, dv in the shape's own space .
    pub uv: [f64; 2],
    pub du: Vec3,
    pub dv: Vec3,
}

impl LocalCrossing {
    /// A crossing that shows, its normal brought to unit length in the default order (for the
    /// members whose normals are carried by their own rules: a mesh, a terrain).
    pub(crate) fn shown(hit: LocalHit) -> LocalCrossing {
        LocalCrossing::showing(hit)
    }

    fn showing(hit: LocalHit) -> LocalCrossing {
        LocalCrossing { hit, shows: true, unit_order: SumOrder::YzFirst }
    }

    fn showing_in(hit: LocalHit, unit_order: SumOrder) -> LocalCrossing {
        LocalCrossing { hit, shows: true, unit_order }
    }

    /// A crossing that never shows, at `t`.
    pub(crate) fn hidden(t: f64) -> LocalCrossing {
        LocalCrossing {
            hit: LocalHit::new(t, [0.0; 3], [0.0; 3], Part::Face { axis: 1 }),
            shows: false,
            unit_order: SumOrder::YzFirst,
        }
    }
}

/// Order A of two candidates: by t; where equal or either is NaN, the candidates' order.
fn ordered_a(p: LocalCrossing, q: LocalCrossing) -> [LocalCrossing; 2] {
    if q.hit.t < p.hit.t { [q, p] } else { [p, q] }
}

/// Order B of two candidates: by t; where equal the candidates' order; where either is NaN
/// the later candidate first.
fn ordered_b(p: LocalCrossing, q: LocalCrossing) -> [LocalCrossing; 2] {
    if p.hit.t.is_nan() || q.hit.t.is_nan() || q.hit.t < p.hit.t { [q, p] } else { [p, q] }
}

/// A flat end at height `h` of the endless object: the plane's crossing with the normal
/// (0, `ny`, 0), counted where x and z lie within `half` (square) or x^2 + z^2 <= half^2 (round).
fn end_at(r: &Ray, h: f64, ny: f64, half: f64, round: bool) -> Option<LocalHit> {
    let limit = if round { FLAT_PIECE_LIMIT } else { SOLID_FACE_LIMIT };
    if !(r.dir[1].abs() > limit) {
        return None;
    }
    let mut hit = flat_at(r, 1, h)?;
    let [x, _, z] = hit.point;
    hit.normal = [0.0, ny, 0.0];
    hit.part = Part::Cap;
    let inside = if round { x * x + z * z <= half * half } else { x.abs() <= half && z.abs() <= half };
    inside.then_some(hit)
}

/// 's list of the endless object of a light kind : the plain shape's list with its
/// lower end at y = E (-ENDLESS_DEPTH) and its wall or sides counting down to E.
pub fn endless_local_crossings(shape: Shape, r: &Ray, ctx: &ShapeContext) -> Vec<LocalCrossing> {
    let v = ctx.version;
    let e = -ENDLESS_DEPTH;
    let mut list = match shape {
        Shape::Cone => {
            let base = end_at(r, e, -1.0, (1.0 - e) / 2.0, true).into_iter().map(LocalCrossing::showing);
            let wall = wall_candidates(r, v, cone_wall(r), cone_list_normal, list_unit_order(shape, v, false), e);
            first_two(base.chain(wall), ordered_a)
        }
        Shape::Cylinder => {
            let caps = [round_cap_at(r, 1.0), end_at(r, e, -1.0, 1.0, true)].into_iter().flatten().map(LocalCrossing::showing);
            let wall = wall_candidates(r, v, cylinder_wall(r), cylinder_wall_normal, SumOrder::YzFirst, e);
            first_two(caps.chain(wall), ordered_b)
        }
        Shape::Cube => {
            let r = *r;
            let candidates = (0..3).flat_map(move |axis| {
                [0.5, -0.5].into_iter().filter_map(move |side| {
                    if !(r.dir[axis].abs() > SOLID_FACE_LIMIT) {
                        return None;
                    }
                    let height = if axis == 1 && side == -0.5 { e } else { side };
                    let h = flat_at(&r, axis, height)?;
                    (0..3)
                        .filter(|&k| k != axis)
                        .all(|k| if k == 1 { (e..=0.5).contains(&h.point[k]) } else { h.point[k].abs() <= 0.5 })
                        .then_some(LocalCrossing::showing(h))
                })
            });
            first_two_or_none(candidates)
        }
        Shape::Pyramid => {
            let r = *r;
            let table = ctx.pyramid_normal;
            let candidates = end_at(&r, e, -1.0, (0.5 - e) / 2.0, false)
                .into_iter()
                .chain(PYRAMID_SIDES.into_iter().filter_map(move |side| pyramid_side_from(&r, side, table, e)))
                .map(LocalCrossing::showing);
            first_two_or_none(candidates)
        }
        other => return local_crossings(other, r, ctx),
    };
    for c in list.iter_mut().filter(|c| c.shows) {
        let tex = texture(shape, c.hit.part, c.hit.point, v);
        (c.hit.uv, c.hit.du, c.hit.dv) = (tex.uv, tex.du, tex.dv);
    }
    list
}

/// The first two of `candidates` in order A, or nothing with fewer than two (the cube, the
/// pyramid).
fn first_two_or_none(candidates: impl Iterator<Item = LocalCrossing>) -> Vec<LocalCrossing> {
    let firsts: Vec<LocalCrossing> = candidates.take(2).collect();
    match firsts[..] {
        [p, q] => ordered_a(p, q).to_vec(),
        _ => Vec::new(),
    }
}

/// The first two of `candidates` (or the one), ordered by `order` (the cylinder, the cone).
fn first_two(
    candidates: impl Iterator<Item = LocalCrossing>,
    order: fn(LocalCrossing, LocalCrossing) -> [LocalCrossing; 2],
) -> Vec<LocalCrossing> {
    let firsts: Vec<LocalCrossing> = candidates.take(2).collect();
    match firsts[..] {
        [p, q] => order(p, q).to_vec(),
        _ => firsts,
    }
}

/// The wall's counted roots, the smaller first: each where -1 <= y <= 1.
fn wall_candidates(
    r: &Ray,
    v: RenderAs,
    (a, b, c): (f64, f64, f64),
    normal: fn(Vec3, RenderAs) -> Vec3,
    unit_order: SumOrder,
    floor: f64,
) -> impl Iterator<Item = LocalCrossing> {
    let roots = wall_roots(v, a, b, c).map(|ts| ts.to_vec()).unwrap_or_default();
    let r = *r;
    roots.into_iter().filter_map(move |t| {
        let p = r.at(t);
        (floor..=1.0)
            .contains(&p[1])
            .then(|| LocalCrossing::showing_in(LocalHit::new(t, p, normal(p, v), Part::Curved), unit_order))
    })
}

/// The grouping of's last step on a list's curved normal: the hit's (y² + z² first) in
/// 3.0 and 3.1; in 5.0 the one the vectors give for the shape. `far` is the sphere's second
/// crossing. (Crossings: last bits; vector-fixed, Accuracy)
fn list_unit_order(shape: Shape, v: RenderAs, far: bool) -> SumOrder {
    match (v, shape) {
        (RenderAs::Bryce30 | RenderAs::Bryce31, _) => SumOrder::YzFirst,
        (RenderAs::Bryce50, Shape::Sphere) if far => SumOrder::XzFirst,
        (RenderAs::Bryce50, Shape::Sphere | Shape::Cone) => SumOrder::XyFirst,
        (RenderAs::Bryce50, Shape::Torus { .. }) => SumOrder::XzFirst,
        (RenderAs::Bryce50, _) => SumOrder::YzFirst,
    }
}

/// The cone wall's normal in a list:'s form (`cone_wall_normal`), its second unit
/// step's squared length grouped as `list_unit_order` (y² + z² first in 3.0 and 3.1, x² + y²
/// first in 5.0). (Crossings: last bits; vector-fixed)
fn cone_list_normal(p: Vec3, v: RenderAs) -> Vec3 {
    let q = rsqrt_as(v, p[0] * p[0] + p[2] * p[2]);
    let h = [mul_x87(p[0], q), 0.5, mul_x87(p[2], q)];
    list_unit_order(Shape::Cone, v, false).unit(v, h)
}

/// The torus's normal in a list:'s gradient ('s `G`, `(y + y) / r²`) without the
/// unit step in the shape's own space;'s step alone brings it to unit length.
/// (Crossings: last bits; vector-fixed)
fn torus_list_normal(p: Vec3, tube: f64, ring: f64, v: RenderAs) -> Vec3 {
    let [x, y, z] = p;
    let r2 = tube * tube;
    let rho = sqrt_as(v, x * x + z * z);
    let w = rho - ring;
    let g = (w + w) / (rho * r2);
    [g * x, (y + y) / r2, g * z]
}

/// The torus's u in a list where z > 0: the arctangent of -x / z rounded to a double, then
/// divided by 2π (rounded again); elsewhere's u as the hit has it.
/// (Crossings: last bits; vector-fixed,"the torus's u")
fn torus_list_u(p: Vec3, hit_u: f64) -> f64 {
    let [x, _, z] = p;
    if z > 0.0 { atan(-x / z) / (2.0 * std::f64::consts::PI) } else { hit_u }
}

/// 's choice between two operands where at least one is not a number: of two NaNs the
/// one with the larger significand, the positive one on equal significands; one NaN, that NaN
/// (quiet).
fn num3_nan(a: f64, b: f64) -> f64 {
    const FRACTION: u64 = (1 << 52) - 1;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => {
            let (fa, fb) = (a.to_bits() & FRACTION, b.to_bits() & FRACTION);
            let pick = if fa != fb {
                if fa > fb { a } else { b }
            } else if a.is_sign_positive() {
                a
            } else {
                b
            };
            quieted(pick)
        }
        (true, false) => quieted(a),
        (false, true) => quieted(b),
        (false, false) => unreachable!("num3_nan needs a NaN operand"),
    }
}

/// `a op b` with's NaNs: a NaN operand handed on by's rule, an invalid operation
/// on numbers giving the x87 indefinite value.
fn num3(a: f64, b: f64, op: fn(f64, f64) -> f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        num3_nan(a, b)
    } else {
        let x = op(a, b);
        if x.is_nan() { X87_INDEFINITE } else { x }
    }
}

/// The point of a flat shape's crossing at `t` on the plane y = `h` : the
/// point `o + t·d` with's arithmetic; where a coordinate of it is not a number,'s
/// choice among its NaNs is every coordinate ("carried by the object's matrix into
/// every coordinate", the identity included); otherwise y is `h`.
/// , (Crossings)
fn flat_point(r: &Ray, t: f64, h: f64) -> Vec3 {
    let q: Vec3 = std::array::from_fn(|k| num3(r.origin[k], num3(t, r.dir[k], |a, b| a * b), |a, b| a + b));
    let spread = spread_nan(q);
    if spread[0].is_nan() { spread } else { [q[0], h, q[2]] }
}

/// A point whose coordinates include a NaN becomes's choice among its NaNs in every
/// coordinate, as a product with a matrix (the identity's zeros included) hands it on; a point
/// without a NaN is unchanged.,
fn spread_nan(p: Vec3) -> Vec3 {
    match p.iter().copied().filter(|c| c.is_nan()).reduce(num3_nan) {
        Some(nan) => [nan; 3],
        None => p,
    }
}

/// The plane's or a slab face's crossing at y = `h`, which shows. Where t is infinite, o_y is
/// finite and d_x and d_z are both zero, the point is (o_x, h, o_z) with a zero as +0 (
/// Far crossings); elsewhere `flat_point` (the leaning case at infinite t is Open,
fn plane_face(r: &Ray, h: f64) -> LocalCrossing {
    let mut hit = flat_at(r, 1, h).expect("|d_y| at or above the limit is not zero");
    if hit.t.is_infinite() && r.origin[1].is_finite() {
        // : x = o_x + (d_x T rounded to a
        // double), z likewise, T with the wide exponent range; a zero as +0.
        hit.point = crate::geometry::plane_face(r, h, crate::geometry::FaceForm::Rounded).point;
    } else {
        hit.point = flat_point(r, hit.t, h);
    }
    LocalCrossing::showing(hit)
}

/// 's list in the shape's own space, with texture coordinates for the crossings that
/// show. `shape` must not be a mesh (Limits: Open; an empty list is returned).
pub fn local_crossings(shape: Shape, r: &Ray, ctx: &ShapeContext) -> Vec<LocalCrossing> {
    let v = ctx.version;
    // : a light's list is the plain shape's with's marks.
    if let Shape::Light(l) = shape {
        return l.list(r, ctx);
    }
    let mut list = match shape {
        Shape::Mesh(_) | Shape::StoredMesh(_) => Vec::new(), // Limits (meshes: Open)
        // Limits (terrains: Open, a later revision gives their lists;
        // gives only the solid word's faces, `Relief::solid_faces`)
        Shape::Terrain(_) => Vec::new(),
        Shape::Path(_) => Vec::new(),
        // A group's list is's (`Scene::group_list`), which needs the scene.
        Shape::Group(_) => Vec::new(),
        Shape::Tree { .. } | Shape::TreeLeaves { .. } => Vec::new(),
        Shape::Metaball(_) | Shape::MetaSet => Vec::new(),
        Shape::Light(_) => unreachable!("a light's list is answered above"),
        Shape::Sphere => match sphere_roots(r, v) {
            Some([near, far]) => {
                let at = |t: f64, far: bool| {
                    let p = r.at(t);
                    LocalCrossing::showing_in(LocalHit::new(t, p, p, Part::Curved), list_unit_order(shape, v, far))
                };
                let first = at(near, false);
                // : a tangent ray's crossings are identical records as
                // 3.0 and 3.1; as 5.0 each carries the normal of its own place in the list.
                let second = if far == near && v != crate::version::RenderAs::Bryce50 { first } else { at(far, true) };
                vec![first, second]
            }
            None => Vec::new(),
        },
        Shape::Cube => {
            let r = *r;
            let candidates = (0..3).flat_map(move |axis| {
                [0.5, -0.5].into_iter().filter_map(move |side| {
                    if !(r.dir[axis].abs() > SOLID_FACE_LIMIT) {
                        return None;
                    }
                    let h = flat_at(&r, axis, side)?;
                    (0..3)
                        .filter(|&k| k != axis)
                        .all(|k| h.point[k].abs() <= 0.5)
                        .then_some(LocalCrossing::showing(h))
                })
            });
            first_two_or_none(candidates)
        }
        Shape::Pyramid => {
            let r = *r;
            let table = ctx.pyramid_normal;
            let candidates = pyramid_base(&r)
                .into_iter()
                .chain(PYRAMID_SIDES.into_iter().filter_map(move |side| pyramid_side(&r, side, table)))
                .map(LocalCrossing::showing);
            first_two_or_none(candidates)
        }
        Shape::Cylinder => {
            let caps = [round_cap_at(r, 1.0), round_cap_at(r, -1.0)].into_iter().flatten().map(LocalCrossing::showing);
            let wall = wall_candidates(r, v, cylinder_wall(r), cylinder_wall_normal, SumOrder::YzFirst, -1.0);
            first_two(caps.chain(wall), ordered_b)
        }
        Shape::Cone => {
            let base = round_cap_at(r, -1.0).into_iter().map(LocalCrossing::showing);
            let wall = wall_candidates(r, v, cone_wall(r), cone_list_normal, list_unit_order(shape, v, false), -1.0);
            first_two(base.chain(wall), ordered_a)
        }
        Shape::Torus { tube, ring } => {
            let list: Vec<LocalCrossing> = torus_roots(r, tube, ring, v)
                .into_iter()
                .filter(|t| !t.is_nan())
                .map(|t| {
                    let p = r.at(t);
                    let n = torus_list_normal(p, tube, ring, v);
                    LocalCrossing::showing_in(LocalHit::new(t, p, n, Part::Curved), list_unit_order(shape, v, false))
                })
                .collect();
            // : fewer than two roots give the empty list
            if list.len() < 2 { Vec::new() } else { list }
        }
        Shape::Plane => {
            if !(r.dir[1].abs() >= plane_limit(v)) {
                Vec::new()
            } else {
                let pad = LocalCrossing::hidden(flat_t(r, 1, -PADDING_DEPTH));
                ordered_b(plane_face(r, 0.0), pad).to_vec()
            }
        }
        Shape::Slab => {
            if r.dir[1].abs() >= plane_limit(v) {
                ordered_b(plane_face(r, 1.0), plane_face(r, -1.0)).to_vec()
            } else if (-1.0..=1.0).contains(&r.origin[1]) {
                vec![LocalCrossing::hidden(-PADDING_DEPTH), LocalCrossing::hidden(PADDING_DEPTH)]
            } else {
                Vec::new()
            }
        }
        Shape::Square | Shape::Disc => {
            if !(r.dir[1].abs() > FLAT_PIECE_LIMIT) {
                return Vec::new();
            }
            let Some(mut hit) = flat_at(r, 1, 0.0) else { return Vec::new() };
            // : a zero coordinate of the point is +0.
            hit.point = hit.point.map(|c| c + 0.0);
            let [x, _, z] = hit.point;
            // : a NaN coordinate fails the square's test and passes the disc's.
            let inside = if shape == Shape::Square {
                x.abs() <= 1.0 && z.abs() <= 1.0
            } else {
                !(x * x + z * z > 1.0)
            };
            if inside {
                let t = hit.t;
                vec![LocalCrossing::showing(hit), LocalCrossing::hidden(t + FLAT_THICKNESS)]
            } else {
                Vec::new()
            }
        }
    };
    for c in list.iter_mut().filter(|c| c.shows) {
        let mut tex = super::texture_on(shape, c.hit.part, c.hit.point, r, c.hit.t, v);
        if let Shape::Torus { .. } = shape {
            tex.uv[0] = torus_list_u(c.hit.point, tex.uv[0]);
        }
        (c.hit.uv, c.hit.du, c.hit.dv) = (tex.uv, tex.du, tex.dv);
    }
    list
}

/// 's list for an object: the world ray carried into the shape's space , each
/// crossing that shows carried back as carries a hit. The square's and the disc's point
/// is carried by the matrix even for the identity, so that a NaN coordinate reaches every
/// coordinate (Far crossings).
pub fn crossings(shape: Shape, ray: &Ray, placement: &Placement, ctx: &ShapeContext) -> Vec<Crossing> {
    let local = placement.to_local_as(ctx.version, ray);
    local_crossings(shape, &local, ctx).iter().map(|c| carry(shape, placement, c, ctx.version)).collect()
}

/// One crossing of `shape`'s list carried to the world by its object's placement,
/// carries a hit. Used by `crossings` and by a combined group's entries and hit (the
/// member's record, unchanged by the group).
pub fn carry(shape: Shape, placement: &Placement, c: &LocalCrossing, version: RenderAs) -> Crossing {
    if !c.shows {
        return Crossing { t: c.hit.t, shows: false, point: [0.0; 3], normal: [0.0; 3], uv: [0.0; 2], du: [0.0; 3], dv: [0.0; 3] };
    }
    let h = c.hit;
    Crossing {
        t: h.t,
        shows: true,
        point: carry_point(shape, placement, h.point),
        normal: c.unit_order.unit(version, placement.carry_normal(h.normal, version)),
        uv: h.uv,
        du: h.du,
        dv: h.dv,
    }
}

/// A crossing's local point carried to the world.: the square's
/// and the disc's NaN coordinate is carried by the matrix into every coordinate, the
/// identity's too; their texture coordinates are those of the point before it (disc-015,
/// disc-111).
pub(crate) fn carry_point(shape: Shape, placement: &Placement, p: Vec3) -> Vec3 {
    let point = match shape {
        Shape::Square | Shape::Disc => spread_nan(p),
        _ => p,
    };
    placement.to_world(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: f64) -> LocalCrossing {
        LocalCrossing::hidden(t)
    }

    fn ts(list: [LocalCrossing; 2]) -> [u64; 2] {
        [list[0].hit.t.to_bits(), list[1].hit.t.to_bits()]
    }

    /// 's orders A and B: they differ only where a t is not a number.
    #[test]
    fn orders_a_and_b() {
        let nan = f64::NAN;
        assert_eq!(ts(ordered_a(at(2.0), at(1.0))), [1f64.to_bits(), 2f64.to_bits()]);
        assert_eq!(ts(ordered_b(at(2.0), at(1.0))), [1f64.to_bits(), 2f64.to_bits()]);
        assert_eq!(ts(ordered_a(at(nan), at(1.0))), [nan.to_bits(), 1f64.to_bits()]);
        assert_eq!(ts(ordered_b(at(nan), at(1.0))), [1f64.to_bits(), nan.to_bits()]);
        assert_eq!(ts(ordered_b(at(1.0), at(nan))), [nan.to_bits(), 1f64.to_bits()]);
        // equal distances keep the candidates' order in both
        let (p, q) = (at(1.0), LocalCrossing { shows: true, ..at(1.0) });
        assert!(!ordered_a(p, q)[0].shows && !ordered_b(p, q)[0].shows);
    }

    /// : of two NaNs the larger significand, the positive one on a tie.
    #[test]
    fn num3_choice() {
        let pos = f64::from_bits(0x7ff8_0000_0000_0000);
        assert_eq!(num3_nan(X87_INDEFINITE, pos).to_bits(), pos.to_bits());
        let big = f64::from_bits(0xfff8_0000_0000_0001);
        assert_eq!(num3_nan(pos, big).to_bits(), big.to_bits());
        assert_eq!(num3(0.0, f64::INFINITY, |a, b| a * b).to_bits(), X87_INDEFINITE.to_bits());
    }

    /// A zero direction: the slab's two crossings that never show, at -1e6 and +1e6, where the
    /// origin lies between its faces; every other shape here lists nothing (Errors; the
    /// sphere as 5.0, whose q(0) is not finite).
    #[test]
    fn zero_direction() {
        let ctx = ShapeContext { version: RenderAs::Bryce50, pyramid_normal: [0.0, 1.0], shadow: false };
        let ray = Ray { origin: [0.0, 0.5, 0.0], dir: [0.0; 3] };
        let list = local_crossings(Shape::Slab, &ray, &ctx);
        assert_eq!(list.iter().map(|c| (c.hit.t, c.shows)).collect::<Vec<_>>(), [(-1e6, false), (1e6, false)]);
        for shape in [Shape::Plane, Shape::Cube, Shape::Cylinder, Shape::Cone, Shape::Square, Shape::Disc, Shape::Sphere] {
            assert!(local_crossings(shape, &ray, &ctx).is_empty(), "{shape:?}");
        }
    }
}
