// SPDX-License-Identifier: MIT
//! The point a material's texture is evaluated at, and the frame its bump gradient reaches
//! the surface through:'s texture spaces and mappings, modes 0 to 16.
//!
//! (Terms, the object's
//! own box, the modes, Variants, Accuracy),
//! (FPATAN, FSIN extended),,;
//! revised
//! (Texture spaces and mappings; Mode 8, Random)
//!
//! Every operation is one binary64 operation, rounded once, in the order written, unless a
//! comment says the value is extended.

use crate::geom::Vec3;
use crate::numerics::{ExtValue, acos, atan2_ext, rsqrt_as, sqrt_as, to_i32};
use crate::numerics::x87::sin_ext;
use crate::shading::lighting::SumOrder;
use crate::version::RenderAs;

use super::lattice::{Lattice, NotComputed};

/// The switch bit that tiles and mirrors, as 5.0.
pub const SWITCH_TILE: u32 = 0x20;
/// Symmetrical Tiling, a picture's, as 3.0 and 3.1.
pub const SWITCH_SYMMETRIC: u32 = 0x1;
/// Scale Pict Size, a picture's, as 3.0 and 3.1.
/// The switch bit 0x4: modes 2 and 3 centre the surface coordinates, as 3.1 and 5.0.
pub const SWITCH_CENTRED: u32 = 0x4;
/// The switch bit 0x40: a picture's colour and value from one pixel, as 5.0.
pub const SWITCH_ONE_PIXEL: u32 = 0x40;
/// The switch bits a material's channel reads as 3.1 and 5.0: the texture's colour (0x8) and
/// value (0x10) taken as says. Not the mapping's.
pub const SWITCH_COLOUR_TAKEN: u32 = 0x8;
pub const SWITCH_VALUE_TAKEN: u32 = 0x10;
pub const SWITCH_SCALE_PICT: u32 = 0x2;
/// The largest mode number.
pub const LAST_MODE: u32 = 16;
/// The first mode that is 5.0's alone.
pub const FIRST_FIVE_MODE: u32 = 11;
/// The Random mode, whose angles are noise type 9's (it reads `perm`).
pub const RANDOM_MODE: u32 = 8;
/// Mode 8's noise: type 9 with the 3-D setting, at the point times 10.
const RANDOM_NOISE_TYPE: i64 = 9;
const RANDOM_NOISE_SETTING: u8 = 3;
const RANDOM_SCALE: f64 = 10.0;

/// π and 2π, the doubles nearest them.
const PI: f64 = std::f64::consts::PI;
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
/// π', the double nearest the decimal 3.14159265359.
#[allow(clippy::approx_constant)]
const PI_PRIME: f64 = 3.14159265359;
/// The quarter turn on the axis, the double nearest 1.570796326795.
#[allow(clippy::approx_constant)]
const QUARTER_TURN: f64 = 1.570796326795;
/// Below this extent ι is 0 as 5.0.
const LEAST_EXTENT: f64 = 1e-10;
/// A picture's scale: 17·s + 1.
const PICTURE_SCALE: f64 = 17.0;
/// The half added in the cubic modes.
const CUBIC_HALF: f64 = 0.5;

const ZERO: Vec3 = [0.0; 3];
const X_AXIS: Vec3 = [1.0, 0.0, 0.0];
const Y_AXIS: Vec3 = [0.0, 1.0, 0.0];
const Z_AXIS: Vec3 = [0.0, 0.0, 1.0];

/// The hit as a mapping reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapHit {
    /// P, in world space.
    pub world: Vec3,
    /// p, in the object's own space.
    pub object: Vec3,
    /// n, the shading normal before any facing test or bump.
    pub normal: Vec3,
    pub u: f64,
    pub v: f64,
    /// The directions of u and v, in the shape's own space.
    pub du: Vec3,
    pub dv: Vec3,
}

/// The object as a mapping reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapObject {
    /// w1, w2, w3: the rows of its 3 x 3 matrix .
    pub rows: [Vec3; 3],
    /// L, the own box's low corner.
    pub low: Vec3,
    /// H, its high corner.
    pub high: Vec3,
    /// The primitive is a sphere (modes 9 to 11 treat it apart).
    pub sphere: bool,
}

/// The mapping part of a texture, and whether the texture is a picture. (Inputs:
/// the mapping;'s brtx and pctx)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapSettings {
    pub mode: u32,
    pub switches: u32,
    pub scale: Vec3,
    pub offset: Vec3,
    /// The quaternion x, y, z, w.
    pub turn: [f64; 4],
    pub picture: bool,
}

/// The texture point t and the frame a, b.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexturePoint {
    pub point: Vec3,
    pub a: Vec3,
    pub b: Vec3,
}

/// Why a mapping gives no point. (Texture spaces and mappings: refusals)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingRefusal {
    /// A mode number outside 0 to 16 (Open), or 11 to 16 below 5.0 (Variants).
    Mode(u32),
    /// Mode 8 (Random) asked of `map_point`, which takes no tables (`map_point_tabled` does).
    Random,
    /// Mode 8 with tables that lack `perm` (no default table).
    RandomNeedsPerm,
}

impl std::fmt::Display for MappingRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappingRefusal::Mode(m) => write!(f, "mapping mode {m} (TX-4.2: modes 0 to 16, 11 to 16 as 5.0 only)"),
            MappingRefusal::Random => write!(f, "mapping mode 8, Random, needs the tables (noise type 9 over `perm`)"),
            MappingRefusal::RandomNeedsPerm => {
                write!(f, "mapping mode 8, Random: the substrate lacks `perm` (SUB-8)")
            }
        }
    }
}

/// How the turn's diagonal entries 1 − 2(a² + b²) are formed. (the
/// arrangements searched against mapping.jsonl)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Diagonal {
    /// 1 − 2·(a² + b²), as written.
    Written,
    /// (1 − 2a²) − 2b².
    Split,
    /// (1 − 2b²) − 2a².
    SplitOther,
    /// ((w² + c²) − a²) − b², c the third coordinate.
    Squares,
}

/// The arrangement of the turn's entries from the quaternion. (Accuracy: "the
/// arrangement of the turn's entries from the quaternion")
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TurnForm {
    /// The matrix as written (false) or its transpose (true).
    pub transposed: bool,
    pub diagonal: Diagonal,
}

impl TurnForm {
    pub const ALL: [TurnForm; 8] = {
        let d = [Diagonal::Written, Diagonal::Split, Diagonal::SplitOther, Diagonal::Squares];
        [
            TurnForm { transposed: false, diagonal: d[0] },
            TurnForm { transposed: false, diagonal: d[1] },
            TurnForm { transposed: false, diagonal: d[2] },
            TurnForm { transposed: false, diagonal: d[3] },
            TurnForm { transposed: true, diagonal: d[0] },
            TurnForm { transposed: true, diagonal: d[1] },
            TurnForm { transposed: true, diagonal: d[2] },
            TurnForm { transposed: true, diagonal: d[3] },
        ]
    };
}

/// The grouping of a term of T: the point's coordinate xi, the turn's entry Rij and the factor
/// kj.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grouping {
    /// (xi·Rij)·kj
    TurnedFirst,
    /// xi·(Rij·kj)
    FactorOnTurn,
    /// (xi·kj)·Rij
    FactorOnPoint,
    /// (Σ xi·Rij)·kj: the factor after the turned sum.
    AfterSum,
}

impl Grouping {
    pub const ALL: [Grouping; 4] =
        [Grouping::TurnedFirst, Grouping::FactorOnTurn, Grouping::FactorOnPoint, Grouping::AfterSum];
}

/// How the arctangent of a quotient y/x is taken. (Terms: "the arctangent of the
/// quotient",'s extended value)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quotient {
    /// The quotient rounded to a double, then FPATAN of it over 1.
    Rounded,
    /// FPATAN of y over x directly (x made positive, both negated together).
    Unrounded,
}

/// The uses of unit( ), each with its own order of the sum of squares (as 5.0 not the same in
/// every use).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnitOrders {
    /// unit(p): the angles of modes 5 and 13.
    pub point: SumOrder,
    /// unit(n): mode 7.
    pub normal: SumOrder,
    /// unit(W·d): modes 2, 3 (du, dv), 5, 6 and 13 (the first direction).
    pub carried: SumOrder,
    /// unit(c): modes 5, 6 and 13.
    pub column: SumOrder,
    /// unit(wj) as the frame's a and b, for modes 9, 10 and 11 in turn.
    pub rows: [[SumOrder; 2]; 3],
    /// unit(wi) and unit(wi') in mode 12, by the side the surface faces (along x, y, z).
    pub cubic: [[SumOrder; 2]; 3],
    /// unit(m): modes 12 and 16.
    pub facing: SumOrder,
}

impl UnitOrders {
    pub const fn all(o: SumOrder) -> UnitOrders {
        UnitOrders { point: o, normal: o, carried: o, column: o, rows: [[o; 2]; 3], cubic: [[o; 2]; 3], facing: o }
    }
}

/// The orders and forms leaves to ` `.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MappingOrders {
    pub turn: TurnForm,
    pub grouping: Grouping,
    /// The order of the three terms of each coordinate of T.
    pub terms: SumOrder,
    pub unit: UnitOrders,
    /// The order of the three terms of each coordinate of W·d.
    pub carried: [SumOrder; 3],
    /// The order of each row's sum in mode 3, row by row.
    pub row_sum: [SumOrder; 3],
    pub quotient: Quotient,
}

impl MappingOrders {
    /// The orders tests/texture_mapping.rs finds give every case of mapping.jsonl (exactly
    /// one candidate of each, the others held at these). (Accuracy: "mapping.jsonl
    /// decides each")
    pub fn of(version: RenderAs) -> MappingOrders {
        use SumOrder::*;
        let three = MappingOrders {
            turn: TurnForm { transposed: false, diagonal: Diagonal::Split },
            grouping: Grouping::FactorOnTurn,
            terms: XyZ,
            unit: UnitOrders::all(YzX),
            carried: [YzX; 3],
            row_sum: [YzX; 3],
            quotient: Quotient::Rounded,
        };
        match version {
            RenderAs::Bryce30 => three,
            RenderAs::Bryce31 => MappingOrders { row_sum: [YzX, XyZ, YzX], ..three },
            RenderAs::Bryce50 => {
                // Most uses (x + y) + z; mode 9's a and mode 12's b on the y side (y + z) + x.
                let mut unit = UnitOrders::all(XyZ);
                unit.rows[0][0] = YzX;
                unit.cubic[1][1] = YzX;
                MappingOrders { unit, carried: [YzX, XyZ, XyZ], ..three }
            }
        }
    }
}

/// The turn R, as rows, or `None` where it does not apply (w = 0).
pub fn turn_matrix(q: [f64; 4], form: TurnForm) -> Option<[Vec3; 3]> {
    let [x, y, z, w] = q;
    if w == 0.0 {
        return None;
    }
    // A diagonal entry 1 − 2(a² + b²); c is the third coordinate.
    let diag = |a: f64, b: f64, c: f64| match form.diagonal {
        Diagonal::Written => 1.0 - 2.0 * (a * a + b * b),
        Diagonal::Split => (1.0 - 2.0 * (a * a)) - 2.0 * (b * b),
        Diagonal::SplitOther => (1.0 - 2.0 * (b * b)) - 2.0 * (a * a),
        Diagonal::Squares => ((w * w + c * c) - a * a) - b * b,
    };
    let r = [
        [diag(y, z, x), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)],
        [2.0 * (x * y + w * z), diag(x, z, y), 2.0 * (y * z - w * x)],
        [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), diag(x, y, z)],
    ];
    Some(if form.transposed { std::array::from_fn(|i| std::array::from_fn(|j| r[j][i])) } else { r })
}

/// The three terms of a sum in the order given.
fn sum3(order: SumOrder, t: Vec3) -> f64 {
    match order {
        SumOrder::XyZ => (t[0] + t[1]) + t[2],
        SumOrder::XzY => (t[0] + t[2]) + t[1],
        SumOrder::YzX => (t[1] + t[2]) + t[0],
    }
}

/// The mapping's working context: the version, its orders and the turn.
struct Ctx {
    version: RenderAs,
    o: MappingOrders,
    turn: Option<[Vec3; 3]>,
}

impl Ctx {
    /// (x·R)j·kj for each j: the point turned and scaled, with no offset; with no turn xj·kj.
    fn turned_scaled(&self, x: Vec3, k: Vec3) -> Vec3 {
        let Some(r) = self.turn else {
            return std::array::from_fn(|j| x[j] * k[j]);
        };
        std::array::from_fn(|j| {
            let term = |i: usize| match self.o.grouping {
                Grouping::TurnedFirst => (x[i] * r[i][j]) * k[j],
                Grouping::FactorOnTurn => x[i] * (r[i][j] * k[j]),
                Grouping::FactorOnPoint => (x[i] * k[j]) * r[i][j],
                Grouping::AfterSum => x[i] * r[i][j],
            };
            let s = sum3(self.o.terms, [term(0), term(1), term(2)]);
            if self.o.grouping == Grouping::AfterSum { s * k[j] } else { s }
        })
    }

    /// T(x; k).
    fn t(&self, x: Vec3, k: Vec3, offset: Vec3) -> Vec3 {
        let m = self.turned_scaled(x, k);
        std::array::from_fn(|j| m[j] + offset[j])
    }

    /// x·R, the point turned alone (modes 12 and 16), the terms in T's order.
    fn turned(&self, x: Vec3) -> Vec3 {
        match self.turn {
            None => x,
            Some(r) => std::array::from_fn(|j| sum3(self.o.terms, [x[0] * r[0][j], x[1] * r[1][j], x[2] * r[2][j]])),
        }
    }

    /// The inverse of R times m: coordinate j the dot product of row j of R's inverse with m.
    /// (modes 12 and 16, the inverse stated 2026-10-22: nine two-by-two
    /// differences, one rounded reciprocal of the determinant, and the dot products' orders)
    fn turned_back(&self, m: Vec3) -> Vec3 {
        let Some(r) = self.turn else { return m };
        let [[r11, r12, r13], [r21, r22, r23], [r31, r32, r33]] = r;
        let a11 = r33 * r22 - r32 * r23;
        let a12 = -(r12 * r33 - r13 * r32);
        let a13 = r12 * r23 - r13 * r22;
        let a21 = -(r21 * r33 - r23 * r31);
        let a22 = r11 * r33 - r13 * r31;
        let a23 = -(r11 * r23 - r13 * r21);
        let a31 = r21 * r32 - r22 * r31;
        let a32 = -(r11 * r32 - r12 * r31);
        let a33 = r11 * r22 - r12 * r21;
        let det = ((r33 * r22 - r32 * r23) * r11 - (r21 * r33 - r31 * r23) * r12) + (r21 * r32 - r31 * r22) * r13;
        let k = 1.0 / det;
        let [p11, p12, p13, p21, p22, p23, p31, p32, p33] =
            [a11, a12, a13, a21, a22, a23, a31, a32, a33].map(|a| a * k);
        [
            (p12 * m[1] + p13 * m[2]) + p11 * m[0],
            (p22 * m[1] + p21 * m[0]) + p23 * m[2],
            (p32 * m[1] + p31 * m[0]) + p33 * m[2],
        ]
    }

    /// unit(d) = d·Q(d·d), the sum of squares in the order given.
    fn unit(&self, d: Vec3, order: SumOrder) -> Vec3 {
        let q = rsqrt_as(self.version, sum3(order, [d[0] * d[0], d[1] * d[1], d[2] * d[2]]));
        d.map(|c| c * q)
    }

    /// ι(d).
    fn inverse(&self, d: f64) -> f64 {
        match self.version {
            RenderAs::Bryce30 | RenderAs::Bryce31 if d == 0.0 => 1.0,
            RenderAs::Bryce50 if -LEAST_EXTENT < d && d < LEAST_EXTENT => 0.0,
            _ => 1.0 / d,
        }
    }

    /// W·d: coordinate j is wj·d.
    fn carried(&self, rows: &[Vec3; 3], d: Vec3) -> Vec3 {
        std::array::from_fn(|j| sum3(self.o.carried[j], [rows[j][0] * d[0], rows[j][1] * d[1], rows[j][2] * d[2]]))
    }

    /// The extended arctangent of y/x.,
    fn atan_of(&self, y: f64, x: f64) -> Result<ExtValue, f64> {
        let (y, x) = match self.o.quotient {
            Quotient::Rounded => (y / x, 1.0),
            Quotient::Unrounded => if x < 0.0 { (-y, -x) } else { (y, x) },
        };
        if y.is_nan() || x.is_nan() {
            return Err(y + x);
        }
        Ok(atan2_ext(y, x))
    }

    /// The angle about the y axis of (e1, e3).
    fn about_y(&self, e1: f64, e3: f64) -> f64 {
        if e3 == 0.0 {
            return if -e1 <= 0.0 { QUARTER_TURN } else { 0.0 };
        }
        match self.atan_of(-e1, e3) {
            Err(nan) => nan,
            Ok(a) if e3 > 0.0 => a.to_f64(),
            Ok(a) => a.add_to_f64(PI_PRIME),
        }
    }

    /// The angle down from the y axis of e2.
    fn down_from_y(&self, e2: f64) -> f64 {
        if e2 == 0.0 {
            return PI;
        }
        let r = sqrt_as(self.version, 1.0 - e2 * e2);
        match self.atan_of(r, e2) {
            Err(nan) => PI - nan,
            Ok(b) if e2 > 0.0 => b.negated().add_to_f64(PI),
            Ok(b) => PI - b.add_to_f64(PI_PRIME),
        }
    }

    /// The two numbers of a direction: the angle about y over 2π, down from y over π, and e.
    fn angles(&self, d: Vec3, order: SumOrder) -> (f64, f64, Vec3) {
        let e = self.unit(d, order);
        (self.about_y(e[0], e[2]) / TWO_PI, self.down_from_y(e[1]) / PI, e)
    }

    /// Tiling: as 5.0 with the switch 0x20, t1 and t2 each brought to 0 up to 1; as 3.0 and 3.1
    /// with the switch 0x1 (Symmetrical Tiling), each folded by a mirror of period 2 and the
    /// direction turned where the fold went backward. The point is returned with its directions.
    fn tile(&self, switches: u32, t: Vec3, a: Vec3, b: Vec3) -> TexturePoint {
        if switches & SWITCH_SYMMETRIC != 0 {
            let (t1, back1) = mirror_fold(t[0]);
            let (t2, back2) = mirror_fold(t[1]);
            let a = if back1 { [-a[0], a[1], a[2]] } else { a };
            let b = if back2 { [b[0], -b[1], b[2]] } else { b };
            return TexturePoint { point: [t1, t2, t[2]], a, b };
        }
        if !(self.version == RenderAs::Bryce50 && switches & SWITCH_TILE != 0) {
            return TexturePoint { point: t, a, b };
        }
        let f = |x: f64| {
            let r = x - f64::from(to_i32(x));
            if r < 0.0 { r + 1.0 } else { r }
        };
        TexturePoint { point: [f(t[0]), f(t[1]), t[2]], a, b }
    }
}

/// The mirror of period 2 held at most 0.99, and whether x fell in the backward half.
fn mirror_fold(x: f64) -> (f64, bool) {
    let n = |v: f64| f64::from(to_i32(v));
    let r = if x > 0.0 { x - 2.0 * n(x / 2.0) } else { 2.0 - ((-x) - 2.0 * n(-x / 2.0)) };
    let (m, back) = if r <= 1.0 { (r, false) } else { (2.0 - r, true) };
    (if m > 0.99 { 0.99 } else { m }, back)
}

/// s' for the texture's kind.
fn kind_scale(s: Vec3, picture: bool, version: RenderAs, switches: u32) -> Vec3 {
    if picture && version != RenderAs::Bryce50 && switches & SWITCH_SCALE_PICT != 0 {
        s.map(|c| 1.0 - c)
    } else if picture {
        s.map(|c| PICTURE_SCALE * c + 1.0)
    } else {
        s
    }
}

/// The hold of modes 9 to 11: 0 where x ≥ 0 does not hold, 1 where x ≤ 1 does not hold.
fn hold_strict(x: f64) -> f64 {
    if !(x >= 0.0) {
        0.0
    } else if !(x <= 1.0) {
        1.0
    } else {
        x
    }
}

/// The hold of ν as 3.0 and 3.1: 0 below 0, 1 above 1, a number that is not one passed on.
fn hold_passing(x: f64) -> f64 {
    x.clamp(0.0, 1.0) // a number that is not one passes on, as `clamp` passes it
}

/// The texture point and frame for a hit, with the orders the vectors decide.
pub fn map_point(
    version: RenderAs,
    settings: &MapSettings,
    object: &MapObject,
    hit: &MapHit,
) -> Result<TexturePoint, MappingRefusal> {
    map_point_with(version, settings, object, hit, MappingOrders::of(version))
}

/// `map_point` with the user's tables for mode 8 (Random), whose angles are noise type 9's
/// values over `perm`; every other mode as `map_point`.,
/// (Mode 8, Random)
pub fn map_point_tabled(
    version: RenderAs,
    settings: &MapSettings,
    object: &MapObject,
    hit: &MapHit,
    lattice: &Lattice<'_>,
) -> Result<TexturePoint, MappingRefusal> {
    map_point_full(version, settings, object, hit, MappingOrders::of(version), Some(lattice))
}

/// `map_point` with the orders given (for the vector test that searches them).
pub fn map_point_with(
    version: RenderAs,
    settings: &MapSettings,
    object: &MapObject,
    hit: &MapHit,
    orders: MappingOrders,
) -> Result<TexturePoint, MappingRefusal> {
    map_point_full(version, settings, object, hit, orders, None)
}

/// Noise type 9 with the 3-D setting at q, as mode 8 reads it.,
fn random_angle(lattice: &Lattice<'_>, q: Vec3) -> Result<f64, MappingRefusal> {
    match lattice.sample(RANDOM_NOISE_TYPE, RANDOM_NOISE_SETTING, q) {
        Ok(s) => Ok(s.value),
        Err(NotComputed::NoTable) => Err(MappingRefusal::RandomNeedsPerm),
        Err(NotComputed::NotHere) => unreachable!("type 9 is a lattice type"),
    }
}

/// The whole mapping; `random` holds the tables mode 8 reads (`None`: mode 8 is refused).
fn map_point_full(
    version: RenderAs,
    settings: &MapSettings,
    object: &MapObject,
    hit: &MapHit,
    orders: MappingOrders,
    random: Option<&Lattice<'_>>,
) -> Result<TexturePoint, MappingRefusal> {
    let five = version == RenderAs::Bryce50;
    // : as 5.0 a turn whose w is exactly 1 is not applied.
    let turn = if five && settings.turn[3] == 1.0 { None } else { turn_matrix(settings.turn, orders.turn) };
    let cx = Ctx { version, o: orders, turn };
    let mode = settings.mode;
    // : as 3.0 and 3.1 every number but 0 to 10, as 5.0 every
    // number but 0 to 16: no mode's step; t = (0, 0, 0), a = b = (0, 0, 0), then the tiling step.
    if mode > LAST_MODE || (mode >= FIRST_FIVE_MODE && !five) {
        return Ok(cx.tile(settings.switches, ZERO, ZERO, ZERO));
    }
    let s = kind_scale(settings.scale, settings.picture, version, settings.switches);
    let half = s.map(|c| c * 0.5);
    let o = settings.offset;
    let (l, h) = (object.low, object.high);
    let rows = &object.rows;
    let (p, world, n) = (hit.object, hit.world, hit.normal);
    let sw = settings.switches;
    let pt = |point: Vec3, a: Vec3, b: Vec3| TexturePoint { point, a, b };
    let flat = |t: Vec3| [t[0], t[1], 1.0];
    let column = [rows[0][1], rows[1][1], rows[2][1]];
    let u = &orders.unit;
    Ok(match mode {
        0 => {
            let k: Vec3 = std::array::from_fn(|j| cx.inverse(h[j] - l[j]) * s[j]);
            let t = cx.t(p, k, o);
            if settings.picture { pt([t[0], t[2], t[2]], X_AXIS, Z_AXIS) } else { pt(t, ZERO, ZERO) }
        }
        1 => {
            let t = cx.t(world, half, o);
            if settings.picture {
                cx.tile(sw, [t[0], -t[1], t[2]], X_AXIS, Y_AXIS)
            } else {
                pt(t, ZERO, ZERO)
            }
        }
        2 | 3 => {
            let k = if mode == 2 {
                s
            } else {
                std::array::from_fn(|j| (sum3(orders.row_sum[j], rows[j]) * s[j]) / 2.0)
            };
            // : the point (u - 0.5,
            // v - 0.5, 1), then t1 + 0.5 and t2 + 0.5 take the place of t1 and t2.
            let t = if sw & SWITCH_CENTRED != 0 {
                let t = cx.t([hit.u - 0.5, hit.v - 0.5, 1.0], k, o);
                [t[0] + 0.5, t[1] + 0.5, 1.0]
            } else {
                flat(cx.t([hit.u, hit.v, 1.0], k, o))
            };
            let a = cx.unit(cx.carried(rows, hit.du), u.carried);
            let b = cx.unit(cx.carried(rows, hit.dv), u.carried);
            cx.tile(sw, t, a, b)
        }
        4 | 14 | 15 => {
            let t = cx.t(world, half, o);
            let (t, a) = match mode {
                4 => ([t[0], t[2], 1.0], X_AXIS),
                14 => ([t[0], t[1], 1.0], X_AXIS),
                _ => ([t[2], t[1], 1.0], Z_AXIS),
            };
            if settings.picture {
                let b = if mode == 4 { Z_AXIS } else { Y_AXIS };
                cx.tile(sw, t, a, b)
            } else {
                pt(t, ZERO, ZERO)
            }
        }
        5 => {
            let (alpha, beta, e) = cx.angles(p, u.point);
            let t = flat(cx.t([alpha, beta, 1.0], s, o));
            let a = cx.unit(cx.carried(rows, [-e[2], 0.0, e[0]]), u.carried);
            cx.tile(sw, t, a, cx.unit(column, u.column))
        }
        6 => {
            let alpha = cx.about_y(p[0], p[2]) / TWO_PI;
            let beta = (p[1] - l[1]) * cx.inverse(h[1] - l[1]);
            let t = flat(cx.t([alpha, beta, 1.0], s, o));
            let a = cx.unit(cx.carried(rows, [-p[2], p[0], 0.0]), u.carried);
            cx.tile(sw, t, a, cx.unit(column, u.column))
        }
        7 => {
            let (alpha, beta, e) = cx.angles(n, u.normal);
            let t = flat(cx.t([alpha, beta, 1.0], s, o));
            cx.tile(sw, t, [-e[2], 0.0, e[0]], [-e[0], -1.0, -e[2]])
        }
        // : α and β noise type 9's values (3-D setting) at
        // (10·p1, 10·p2, 10·p3) and (10·p2, 10·p1, −10·p3), each coordinate one product.
        RANDOM_MODE => {
            let lattice = random.ok_or(MappingRefusal::Random)?;
            let alpha = random_angle(lattice, [RANDOM_SCALE * p[0], RANDOM_SCALE * p[1], RANDOM_SCALE * p[2]])?;
            let beta = random_angle(lattice, [RANDOM_SCALE * p[1], RANDOM_SCALE * p[0], -RANDOM_SCALE * p[2]])?;
            let t = flat(cx.t([alpha, beta, 1.0], s, o));
            cx.tile(sw, t, X_AXIS, Z_AXIS)
        }
        9 | 10 => {
            let top = mode == 9;
            let beyond = if top {
                let lambda = if object.sphere { (h[1] + l[1]) * 0.5 } else { l[1] };
                p[1] < lambda
            } else {
                let lambda = if object.sphere { (h[2] + l[2]) * 0.5 } else { h[2] };
                lambda < p[2]
            };
            if beyond {
                return Ok(pt([o[0], o[1], 1.0], ZERO, ZERO));
            }
            let i = if top { 2 } else { 1 };
            let [oa, ob] = u.rows[(mode - 9) as usize];
            let a = cx.unit(rows[0], oa);
            let b = cx.unit(rows[i], ob);
            let x = [p[0], p[i], 1.0];
            let t = if five {
                let t = cx.turned_scaled(x, s);
                let mu = (t[0] - l[0]) * cx.inverse(h[0] - l[0]) + o[0];
                let nu = (t[1] - l[i]) * cx.inverse(h[i] - l[i]) + o[i];
                [hold_strict(mu), hold_strict(nu), 1.0]
            } else {
                let t = cx.t(x, s, o);
                let mu = (t[0] - l[0]) / (h[0] - l[0]);
                let nu = (t[1] - l[i]) / (h[i] - l[i]);
                [hold_strict(mu), hold_passing(nu), 1.0]
            };
            pt(t, a, b)
        }
        11 => {
            let lambda = if object.sphere { (h[0] + l[0]) * 0.5 } else { h[0] };
            if !(p[0] <= lambda) {
                return Ok(pt([o[0], o[1], 1.0], ZERO, ZERO));
            }
            let [oa, ob] = u.rows[2];
            let a = cx.unit(rows[2], oa);
            let b = cx.unit(rows[1], ob);
            let t = cx.turned_scaled([p[2], p[1], 1.0], s);
            let mu = (t[0] - l[2]) * cx.inverse(h[2] - l[2]) + o[2];
            let nu = (t[1] - l[1]) * cx.inverse(h[1] - l[1]) + o[1];
            pt([hold_strict(mu), hold_strict(nu), 1.0], a, b)
        }
        12 | 16 => {
            let object_cubic = mode == 12;
            let (x, m) = if object_cubic { (p, cx.carried(rows, n)) } else { (world, n) };
            let (x, m) = (cx.turned(x), cx.turned_back(m));
            let d = cx.unit(m, u.facing);
            let a = d.map(f64::abs);
            let sides = if a[0] >= a[1] && a[0] >= a[2] {
                Some((0, 2, 1))
            } else if a[1] >= a[0] && a[1] >= a[2] {
                Some((1, 0, 2))
            } else if a[2] >= a[0] && a[2] >= a[1] {
                Some((2, 0, 1))
            } else {
                None
            };
            let Some((side, i, i2)) = sides else {
                return Ok(cx.tile(sw, ZERO, ZERO, ZERO));
            };
            let xd: Vec3 = std::array::from_fn(|j| x[j] - o[j]);
            let f = |j: usize| if object_cubic { cx.inverse(h[j] - l[j]) * s[j] } else { s[j] };
            let t = [f(i) * xd[i] + CUBIC_HALF, f(i2) * xd[i2] + CUBIC_HALF, 1.0];
            let axis = |j: usize| std::array::from_fn(|k| if k == j { 1.0 } else { 0.0 });
            let (fa, fb) = if object_cubic {
                (cx.unit(rows[i], u.cubic[side][0]), cx.unit(rows[i2], u.cubic[side][1]))
            } else {
                (axis(i), axis(i2))
            };
            cx.tile(sw, t, fa, fb)
        }
        13 => {
            let e = cx.unit(p, u.point);
            let q = rsqrt_as(version, e[0] * e[0] + e[2] * e[2]);
            let (f, hh) = (e[0] * q, e[2] * q);
            let theta = if f > 0.0 { acos(-hh) + PI } else { acos(hh) };
            let delta = PI - acos(e[1]);
            let st = sin_ext(delta).mul_to_f64(theta - PI);
            let alpha = (st + PI) / TWO_PI;
            let beta = delta / PI;
            let t = flat(cx.t([alpha, beta, 1.0], s, o));
            let a = cx.unit(cx.carried(rows, [-e[2], 0.0, e[0]]), u.carried);
            cx.tile(sw, t, a, cx.unit(column, u.column))
        }
        _ => unreachable!("mode checked above"),
    })
}

/// The point a procedural texture is evaluated at, and which coordinates were negated: as 5.0
/// with the switch 0x20, each coordinate below 0 negated.
pub fn mirrored(version: RenderAs, settings: &MapSettings, t: Vec3) -> (Vec3, [bool; 3]) {
    let five = version == RenderAs::Bryce50;
    if settings.picture || !(settings.switches & SWITCH_SYMMETRIC != 0 || (five && settings.switches & SWITCH_TILE != 0)) {
        return (t, [false; 3]);
    }
    let flips = t.map(|c| c < 0.0);
    (std::array::from_fn(|j| if flips[j] { -t[j] } else { t[j] }), flips)
}

/// The gradient the material's channels read: in modes 0 and 1 the texture's own g (each
/// component along a mirrored coordinate negated), in every other mode a·g1 + b·g2 (each
/// coordinate the sum of two products).
pub fn frame_gradient(mode: u32, frame: &TexturePoint, g: Vec3, flips: [bool; 3]) -> Vec3 {
    let g: Vec3 = std::array::from_fn(|j| if flips[j] { -g[j] } else { g[j] });
    match mode {
        0 | 1 => g,
        _ => std::array::from_fn(|k| frame.a[k] * g[0] + frame.b[k] * g[1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_turn_turns_nothing() {
        assert_eq!(turn_matrix([1.0, 0.0, 0.0, 0.0], MappingOrders::of(RenderAs::Bryce31).turn), None);
    }

    #[test]
    fn identity_turn() {
        let r = turn_matrix([0.0, 0.0, 0.0, 1.0], MappingOrders::of(RenderAs::Bryce31).turn).unwrap();
        assert_eq!(r, [X_AXIS, Y_AXIS, Z_AXIS]);
    }

    /// The inverse of a unit turn, formed as states it, is the turn's transpose to within
    /// rounding (the original's last bits are the stated arithmetic's, which no case reaches).
    #[test]
    fn stated_inverse_undoes_a_turn() {
        let v = RenderAs::Bryce50;
        let q = [0.18257418583505536, 0.3651483716701107, 0.5477225575051661, 0.7302967433402214];
        let r = turn_matrix(q, MappingOrders::of(v).turn).unwrap();
        let cx = Ctx { version: v, o: MappingOrders::of(v), turn: Some(r) };
        let m = [0.3, -1.7, 2.25];
        // R times the column m, then the inverse of R brings m back.
        let back = cx.turned_back(std::array::from_fn(|j| (0..3).map(|i| r[j][i] * m[i]).sum::<f64>()));
        for k in 0..3 {
            assert!((back[k] - m[k]).abs() < 1e-12, "{back:?} {m:?}");
        }
    }

    /// Mode 8: refused without tables, and without `perm`; with a permutation (arbitrary: no
    /// result is claimed) the frame is (1, 0, 0), (0, 0, 1) and t3 is 1.
    #[test]
    fn random_mode_needs_perm() {
        let v = RenderAs::Bryce31;
        let settings =
            MapSettings { mode: 8, switches: 0, scale: [1.0; 3], offset: [0.0; 3], turn: [0.0, 0.0, 0.0, 1.0], picture: false };
        let object = MapObject { rows: [X_AXIS, Y_AXIS, Z_AXIS], low: [-1.0; 3], high: [1.0; 3], sphere: true };
        let hit = MapHit {
            world: [0.3, 0.4, 0.5],
            object: [0.3, 0.4, 0.5],
            normal: Y_AXIS,
            u: 0.0,
            v: 0.0,
            du: ZERO,
            dv: ZERO,
        };
        assert_eq!(map_point(v, &settings, &object, &hit), Err(MappingRefusal::Random));
        let smooth = crate::textures::orders::TextureOrders::of(v).smoothstep;
        let none = Lattice { perm: None, waves: None, version: v, smooth };
        assert_eq!(map_point_tabled(v, &settings, &object, &hit, &none), Err(MappingRefusal::RandomNeedsPerm));
        let perm: [i32; 256] = std::array::from_fn(|i| ((i * 37 + 11) % 256) as i32);
        let some = Lattice { perm: Some(&perm), ..none };
        let t = map_point_tabled(v, &settings, &object, &hit, &some).unwrap();
        assert_eq!((t.a, t.b, t.point[2]), (X_AXIS, Z_AXIS, 1.0));
        assert!(t.point[0].is_finite() && t.point[1].is_finite());
    }

    fn plain(mode: u32, switches: u32, turn: [f64; 4]) -> (MapSettings, MapObject, MapHit) {
        let settings = MapSettings { mode, switches, scale: [1.0; 3], offset: [0.0; 3], turn, picture: false };
        let object = MapObject { rows: [X_AXIS, Y_AXIS, Z_AXIS], low: [-1.0; 3], high: [1.0; 3], sphere: true };
        let hit = MapHit {
            world: [0.3, 0.4, 0.5],
            object: [0.3, 0.4, 0.5],
            normal: Y_AXIS,
            u: 0.25,
            v: 0.75,
            du: X_AXIS,
            dv: Z_AXIS,
        };
        (settings, object, hit)
    }

    /// : t = (0, 0, 0) and a = b = (0, 0, 0), then the tiling step,
    /// so that only the switch bit 0x1 (and as 5.0 the switch 0x20) can change t.
    #[test]
    fn other_mode_numbers() {
        for (v, mode) in [(RenderAs::Bryce30, 11), (RenderAs::Bryce31, 17), (RenderAs::Bryce50, 17), (RenderAs::Bryce50, 255)] {
            let (s, o, h) = plain(mode, 0, [0.0, 0.0, 0.0, 1.0]);
            let t = map_point(v, &s, &o, &h).unwrap();
            assert_eq!((t.point, t.a, t.b), (ZERO, ZERO, ZERO), "{v:?} {mode}");
            let (s, o, h) = plain(mode, SWITCH_SYMMETRIC, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(map_point(v, &s, &o, &h).unwrap().point, ZERO);
        }
    }

    /// : as 5.0 a turn whose w is exactly 1 is not applied; as 3.0 and
    /// 3.1 R of the numbers as they stand.
    #[test]
    fn turn_with_w_one() {
        let (s, o, h) = plain(0, 0, [0.5, 0.0, 0.0, 1.0]);
        let free = plain(0, 0, [0.0, 0.0, 0.0, 0.0]);
        let five = map_point(RenderAs::Bryce50, &s, &o, &h).unwrap();
        assert_eq!(five.point, map_point(RenderAs::Bryce50, &free.0, &free.1, &free.2).unwrap().point);
        let three = map_point(RenderAs::Bryce30, &s, &o, &h).unwrap();
        assert_ne!(three.point, five.point);
    }

    /// : with no turn it changes nothing in mode 2.
    #[test]
    fn centred_bit_without_a_turn() {
        let (s, o, h) = plain(2, SWITCH_CENTRED, [0.0, 0.0, 0.0, 0.0]);
        let (s0, ..) = plain(2, 0, [0.0, 0.0, 0.0, 0.0]);
        let a = map_point(RenderAs::Bryce31, &s, &o, &h).unwrap().point;
        let b = map_point(RenderAs::Bryce31, &s0, &o, &h).unwrap().point;
        assert_eq!(a, b);
    }

    #[test]
    fn tiling_keeps_zero_to_one() {
        let cx = Ctx { version: RenderAs::Bryce50, o: MappingOrders::of(RenderAs::Bryce50), turn: None };
        assert_eq!(cx.tile(SWITCH_TILE, [-0.25, 2.5, 7.0], ZERO, ZERO).point, [0.75, 0.5, 7.0]);
        let cx3 = Ctx { version: RenderAs::Bryce31, o: MappingOrders::of(RenderAs::Bryce31), turn: None };
        assert_eq!(cx3.tile(SWITCH_TILE, [-0.25, 2.5, 7.0], ZERO, ZERO).point, [-0.25, 2.5, 7.0]);
    }
}
