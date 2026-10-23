// SPDX-License-Identifier: MIT
//! A track's time map: the curve m through the map's points, and the time the track is at.
//!
//! (revised, log entries 2868 and 3076: straight and curved stretches, the
//! Bézier handles, the cubic solved by's solver, m(u) a binary32 number held to 0…1,
//! first + L · m(u) in binary64; as 5.0 a neighbour within 1e-6 counts as none)
//! (time maps)

use super::track::{TimeMap, TimeMapPoint};
use crate::geometry::cubic_roots;
use crate::version::RenderAs;

/// Within this distance of 0 or 1, u reads the first or last point's value; a point within it
/// of u counts as at u.
const END_BAND: f64 = 1e-10;

/// Each handle's length, as a part of its stretch's width.
const HANDLE: f64 = 0.4;

/// As 5.0, a neighbouring point within this distance of a stretch's end counts as no
/// neighbour.
pub const NEAR_POINTS: f64 = 1e-6;

/// How far outside 0..1 a root may fall and count as the end it is near: as 3.0 the binary32
/// nearest 10^-6 on the root rounded to binary32, as 3.1 and 5.0 the binary32 nearest 10^-5 on
/// the root in binary64.
const ROOT_SLACK_30: f32 = 1e-6;
const ROOT_SLACK_LATER: f32 = 1e-5;

/// The flag bit of a point whose stretch (the one that starts there) is curved.
pub const CURVED: u32 = 2;

/// Whether gives m for this map: two or more points, the first (in the order the
/// document holds them) at 0 and the last at 1. Errors (Open): otherwise a stretch may hold
/// no u, the refusal being acceptable. Points out of order of x, and points that coincide,
/// are kept as the document holds them (Outputs; Variants: a handle with no direction).
pub fn binds(map: &TimeMap) -> bool {
    let p = &map.points;
    p.len() >= 2 && p[0].part == 0.0 && p[p.len() - 1].part == 1.0
}

/// The arithmetic leaves to the formats inside the handles (Open, the
/// paragraph after the entry's fields; checked against time-map-steps.jsonl) and the orders of
/// the coefficients' sums of three and four terms (decided by time-map-steps.jsonl). Each
/// handle switch rounds the named intermediate results to binary32 (otherwise binary64).
/// animation (the arithmetic searched against the vectors)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapArith {
    /// The formats of the handles from a weighted direction or a plain difference (the
    /// handle at a where a point comes before a, the handle at b where one comes after b).
    pub weighted: HandleArith,
    /// The formats of the handles toward the other handle's end (the end stretches').
    pub toward: HandleArith,
    /// The order of k2's three terms.
    pub k2: SumOrder,
    /// The order of k3's four terms.
    pub k3: SumOrder,
}

/// Which intermediate results of a handle are rounded to binary32 (each bit one kind of
/// result; the others stay binary64), and how the step along the unit direction is formed.
/// animation (the formats inside the handles, Open)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandleArith {
    pub bits: u16,
    pub form: StepForm,
}

/// The step from a point to its handle's end along d, of length len, with n = |d|.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepForm {
    /// (d / n) · len
    UnitTimesLength,
    /// d · (len / n)
    TimesRatio,
    /// (d · len) / n
    ProductOverLength,
}

impl HandleArith {
    /// The differences of points (a − p, b − a, C − a, ...).
    pub const DIFF: u16 = 1;
    /// The products of a difference by a width.
    pub const PRODUCT: u16 = 2;
    /// The weighted direction's sum.
    pub const SUM: u16 = 4;
    /// The widths w, xa − xp, xn − xb.
    pub const WIDTH: u16 = 8;
    /// The squares of the direction's coordinates.
    pub const SQUARE: u16 = 16;
    /// Their sum.
    pub const SQUARES: u16 = 32;
    /// The square root n.
    pub const ROOT: u16 = 64;
    /// The quotient (d / n or len / n).
    pub const QUOTIENT: u16 = 128;
    /// The step (the last product or quotient) before it is added to the point.
    pub const STEP: u16 = 256;
    /// The handle's length 0.4 · w.
    pub const LENGTH: u16 = 512;

    fn r(self, bit: u16, x: f64) -> f64 {
        if self.bits & bit != 0 { x as f32 as f64 } else { x }
    }

    /// Every arrangement the search tries.
    pub fn all() -> Vec<HandleArith> {
        let mut v = Vec::new();
        for form in [StepForm::UnitTimesLength, StepForm::TimesRatio, StepForm::ProductOverLength] {
            for bits in 0..1024u16 {
                v.push(HandleArith { bits, form });
            }
        }
        v
    }
}

impl MapArith {
    /// Every intermediate result of the handles in binary32, the sums in the order the steps
    /// decide.
    pub const PLAIN: MapArith = MapArith {
        weighted: HandleArith { bits: 1023, form: StepForm::UnitTimesLength },
        toward: HandleArith { bits: 1023, form: StepForm::UnitTimesLength },
        k2: SumOrder::Turn([0, 1, 2, 3]),
        k3: SumOrder::Turn([1, 3, 2, 0]),
    };

    /// The arrangement for a version's handles where leaves their formats Open (a
    /// neighbouring stretch straight; as 5.0 a neighbour that counts as none): the formats that
    /// were found closest to the stated ones before they were stated. animation
    pub fn of(version: RenderAs) -> MapArith {
        let f = StepForm::UnitTimesLength;
        let (w, t) = match version {
            RenderAs::Bryce30 => (395, 936),
            RenderAs::Bryce31 => (271, 409),
            RenderAs::Bryce50 => (299, 937),
        };
        MapArith { weighted: HandleArith { bits: w, form: f }, toward: HandleArith { bits: t, form: f }, ..MapArith::PLAIN }
    }
}

/// The stages of one curved stretch (time-map-steps.jsonl's fields): the stretch's first
/// point, the handles' ends, the coefficients of x(s) and y(s), s and y(s) before it is held.
/// animation
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stages {
    pub stretch: usize,
    pub c1: [f32; 2],
    pub c2: [f32; 2],
    pub x: [f32; 4],
    pub y: [f32; 4],
    pub s: Option<f32>,
    pub y_s: Option<f32>,
}

type P2 = [f64; 2];

fn point(p: &TimeMapPoint) -> P2 {
    [p.part as f64, p.value as f64]
}

/// The end of a handle from `at` of length `len` along `dir` brought to unit length, rounded
/// to binary32.
fn handle_end(h: HandleArith, at: P2, dir: P2, len: f64) -> P2 {
    let n = h.r(
        HandleArith::ROOT,
        h.r(HandleArith::SQUARES, h.r(HandleArith::SQUARE, dir[0] * dir[0]) + h.r(HandleArith::SQUARE, dir[1] * dir[1])).sqrt(),
    );
    let ratio = h.r(HandleArith::QUOTIENT, len / n);
    let e = |i: usize| {
        let step = match h.form {
            StepForm::UnitTimesLength => h.r(HandleArith::QUOTIENT, dir[i] / n) * len,
            StepForm::TimesRatio => dir[i] * ratio,
            StepForm::ProductOverLength => dir[i] * len / n,
        };
        (at[i] + h.r(HandleArith::STEP, step)) as f32 as f64
    };
    [e(0), e(1)]
}

/// The weighted direction d0 · wa + d1 · wb.
fn weighted(h: HandleArith, d0: P2, wa: f64, d1: P2, wb: f64) -> P2 {
    let p = |x: f64| h.r(HandleArith::PRODUCT, x);
    [h.r(HandleArith::SUM, p(d0[0] * wa) + p(d1[0] * wb)), h.r(HandleArith::SUM, p(d0[1] * wa) + p(d1[1] * wb))]
}

fn sub(h: HandleArith, a: P2, b: P2) -> P2 {
    [h.r(HandleArith::DIFF, a[0] - b[0]), h.r(HandleArith::DIFF, a[1] - b[1])]
}

/// How a sum of three or four binary32 terms is added up, each partial sum rounded to binary32:
/// in turn from the left over the terms in the order named, or as two pairs.
/// animation (the orders the steps decide)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SumOrder {
    /// ((t[i] + t[j]) + t[k]) + t[l] (for three terms the last index is unused).
    Turn([usize; 4]),
    /// (t[i] + t[j]) + (t[k] + t[l]).
    Pairs([usize; 4]),
}

impl SumOrder {
    fn add(self, t: &[f64]) -> f64 {
        let f = |x: f64| x as f32 as f64;
        match self {
            SumOrder::Turn(o) => {
                let mut v = f(t[o[0]] + t[o[1]]);
                for &i in &o[2..t.len()] {
                    v = f(v + t[i]);
                }
                v
            }
            SumOrder::Pairs(o) => f(f(t[o[0]] + t[o[1]]) + f(t[o[2]] + t[o[3]])),
        }
    }

    /// Every distinct order of three terms.
    pub const THREE: [SumOrder; 3] = [SumOrder::Turn([0, 1, 2, 3]), SumOrder::Turn([0, 2, 1, 3]), SumOrder::Turn([1, 2, 0, 3])];

    /// Every distinct order of four terms.
    pub fn four() -> Vec<SumOrder> {
        let mut v = Vec::new();
        for i in 0..4 {
            for j in i + 1..4 {
                let rest: Vec<usize> = (0..4).filter(|&k| k != i && k != j).collect();
                v.push(SumOrder::Turn([i, j, rest[0], rest[1]]));
                v.push(SumOrder::Turn([i, j, rest[1], rest[0]]));
            }
        }
        v.extend([SumOrder::Pairs([0, 1, 2, 3]), SumOrder::Pairs([0, 2, 1, 3]), SumOrder::Pairs([0, 3, 1, 2])]);
        v
    }
}

/// The coefficients in powers of s of one coordinate of the control points P0…P3, each a
/// binary32 number: k0 = P0, k1 = ⟨3·P1⟩ − ⟨3·P0⟩, k2 = ⟨3·P0⟩ − ⟨6·P1⟩ + ⟨3·P2⟩ and
/// k3 = P3 − P0 + ⟨3·P1⟩ − ⟨3·P2⟩, each partial sum rounded to binary32, the sums of three
/// and of four terms in the orders given (terms in the order written).
pub fn coefficients(k2: SumOrder, k3: SumOrder, p: [f64; 4]) -> [f64; 4] {
    let f = |x: f64| x as f32 as f64;
    let (p0, p1, p2, p3) = (p[0], p[1], p[2], p[3]);
    let (t0, t1, s1, t2) = (f(3.0 * p0), f(3.0 * p1), f(6.0 * p1), f(3.0 * p2));
    [p0, f(t1 - t0), k2.add(&[t0, -s1, t2]), k3.add(&[p3, -p0, t1, -t2])]
}

/// m(u): the map's value at the part u (already held to 0…1), a binary32 number.
pub fn map_value(version: RenderAs, map: &TimeMap, u: f64) -> f32 {
    map_value_with(version, &MapArith::of(version), map, u)
}

/// `map_value` with a given arithmetic. animation
pub fn map_value_with(version: RenderAs, k: &MapArith, map: &TimeMap, u: f64) -> f32 {
    match evaluate(version, k, map, u) {
        Reading::Value(m) => m,
        Reading::Curved(st) => match st.y_s {
            Some(m) if !m.is_nan() => m.clamp(0.0, 1.0),
            Some(_) => 0.0,
            // Errors (Open): no root in or near 0…1. animation (the stretch's start)
            None => map.points[st.stretch].value,
        },
    }
}

/// The stages of the curved stretch u lies on, or `None` where none is evaluated (u at an
/// end or a straight stretch). animation
pub fn stages(version: RenderAs, k: &MapArith, map: &TimeMap, u: f64) -> Option<Stages> {
    match evaluate(version, k, map, u) {
        Reading::Value(_) => None,
        Reading::Curved(st) => Some(st),
    }
}

enum Reading {
    Value(f32),
    Curved(Stages),
}

fn evaluate(version: RenderAs, k: &MapArith, map: &TimeMap, u: f64) -> Reading {
    let pts = &map.points;
    let last = pts.len() - 1;
    if u.abs() < END_BAND {
        return Reading::Value(pts[0].value);
    }
    if (u - 1.0).abs() < END_BAND {
        return Reading::Value(pts[last].value);
    }
    // The points in the order held: the first point whose x is within 1e-10 of u starts the
    // stretch; failing that, the first point whose x is above u (u rounded to binary32 for
    // this comparison) ends it and the point before it starts it.
    let uf = u as f32 as f64;
    let i = match pts.iter().position(|p| ((p.part as f64) - u).abs() < END_BAND) {
        Some(i) => i.min(last - 1),
        None => pts.iter().position(|p| (p.part as f64) > uf).map_or(last - 1, |j| j.saturating_sub(1)),
    };
    let a = point(&pts[i]);
    let b = point(&pts[i + 1]);
    // The neighbours: as 5.0 one within 1e-6 of the end it neighbours counts as none.
    let counts = |q: P2, end: P2| {
        version != RenderAs::Bryce50 || ((q[0] - end[0]) * (q[0] - end[0]) + (q[1] - end[1]) * (q[1] - end[1])).sqrt() >= NEAR_POINTS
    };
    let before = (i > 0).then(|| point(&pts[i - 1])).filter(|&p| counts(p, a));
    let after = (i + 1 < last).then(|| point(&pts[i + 2])).filter(|&n| counts(n, b));
    let w64 = b[0] - a[0];
    // A straight stretch: a's flag without the bit of 2, or no neighbour on either side.
    if pts[i].flags & CURVED == 0 || (before.is_none() && after.is_none()) {
        return Reading::Value((a[1] + (b[1] - a[1]) * (u - a[0]) / w64) as f32);
    }
    // The maps of the documents' form (every neighbouring stretch curved): the formats the
    // entry states.
    let before_curved = pts[i - usize::from(i > 0)].flags & CURVED != 0;
    let after_curved = after.is_some() && pts[i + 1].flags & CURVED != 0;
    if (before.is_none() || before_curved) && (after.is_none() || after_curved) {
        let (c1, c2) = stated_handles(version, a, b, before, after);
        return curve_stages(version, k, i, [a, c1, c2, b], u);
    }
    // A neighbouring stretch straight: the formats inside the handles are Open there; those
    // of `MapArith`. animation
    let (h, t) = (k.weighted, k.toward);
    let w = h.r(HandleArith::WIDTH, w64);
    let len = h.r(HandleArith::LENGTH, HANDLE * w);
    let from_before = before.map(|p| {
        let dir = if before_curved {
            let wp = h.r(HandleArith::WIDTH, a[0] - p[0]);
            weighted(h, sub(h, a, p), w, sub(h, b, a), wp)
        } else {
            sub(h, a, p)
        };
        handle_end(h, a, dir, len)
    });
    let to_after = after.map(|n| {
        let dir = if before.is_none() && !after_curved {
            sub(h, b, n)
        } else {
            let wn = h.r(HandleArith::WIDTH, n[0] - b[0]);
            weighted(h, sub(h, a, b), wn, sub(h, b, n), w)
        };
        handle_end(h, b, dir, len)
    });
    let tw = t.r(HandleArith::WIDTH, w64);
    let tlen = t.r(HandleArith::LENGTH, HANDLE * tw);
    let (c1, c2) = match (from_before, to_after) {
        (Some(c1), Some(c2)) => (c1, c2),
        (Some(c1), None) => (c1, handle_end(t, b, sub(t, c1, b), tlen)),
        (None, Some(c2)) => (handle_end(t, a, sub(t, c2, a), if after_curved { tlen } else { tw }), c2),
        (None, None) => unreachable!("a stretch with no neighbour is straight"),
    };
    curve_stages(version, k, i, [a, c1, c2, b], u)
}

/// ⟨x⟩: x rounded to binary32.
fn f(x: f64) -> f64 {
    x as f32 as f64
}

/// The reciprocal length q of a version's handles, of a squared length.
fn q(version: RenderAs, x: f64) -> f64 {
    match version {
        RenderAs::Bryce30 => crate::numerics::rsqrt_single_30(x as f32),
        RenderAs::Bryce31 | RenderAs::Bryce50 => 1.0 / x.sqrt(),
    }
}

/// The handles' ends C1 and C2 of a stretch from a to b whose neighbouring stretches are curved
/// (p the point before a, n the one after b, where they count), in the formats the paragraph
/// after's fields states for each version. A product of three factors is grouped from
/// the left (the paragraph: any grouping). (the formats inside the handles,
/// revised at log entry 3313)
fn stated_handles(version: RenderAs, a: P2, b: P2, p: Option<P2>, n: Option<P2>) -> (P2, P2) {
    let [xa, ya] = a;
    let [xb, yb] = b;
    match version {
        RenderAs::Bryce30 => {
            let w = f(xb - xa);
            let h = HANDLE * w;
            let hh = f(h);
            let c1 = p.map(|[xp, yp]| {
                let dp = xa - xp;
                let dx = f(f(f(dp) * w) + f(w * dp));
                let dy = f(f(f(ya - yp) * w) + f(yb - ya) * dp);
                let r = q(version, f(dx * dx + dy * dy));
                [f(xa + f(h * f(r * dx))), f(ya + h * f(r * dy))]
            });
            let c2 = n.map(|[xn, yn]| {
                let dn = xn - xb;
                if p.is_some() {
                    let ex = f(f(f(xa - xb) * dn) + f(f(xb - xn) * w));
                    let ey = f(f(f(ya - yb) * dn) + f(f(yb - yn) * w));
                    let r = q(version, f(ex * ex + ey * ey));
                    [f(xb + f(hh * f(r * ex))), f(yb + hh * r * ey)]
                } else {
                    let ex = f(f(f(xa - xb) * dn) + f(xb - xn) * w);
                    let ey = f(f(ya - yb) * dn + f(f(yb - yn) * w));
                    let r = q(version, f(ex * ex + ey * ey));
                    [f(xb + hh * f(r * ex)), f(yb + f(hh * f(r * ey)))]
                }
            });
            let toward = |e: P2, c: P2| {
                let g = [f(c[0] - e[0]), f(c[1] - e[1])];
                let r = q(version, f(g[0] * g[0] + g[1] * g[1]));
                [f(e[0] + f(hh * f(r * g[0]))), f(e[1] + f(hh * f(r * g[1])))]
            };
            ends(c1, c2, |c1| toward(b, c1), |c2| toward(a, c2))
        }
        RenderAs::Bryce31 => {
            let w = xb - xa;
            let h = HANDLE * w;
            let c1 = p.map(|[xp, yp]| {
                let dp = xa - xp;
                let dx = f(f(f(dp) * w) + f(f(w) * dp));
                let dy = f(f(ya - yp) * w) + f(yb - ya) * dp;
                let r = q(version, dx * dx + dy * dy);
                [f(xa + f(h * f(r * dx))), f(ya + h * f(r * dy))]
            });
            let c2 = n.map(|[xn, yn]| {
                let dn = xn - xb;
                if p.is_some() {
                    let ex = f(f(xa - xb) * dn) + f(f(xb - xn) * f(w));
                    let ey = f(f(f(ya - yb) * dn) + f(f(yb - yn) * f(w)));
                    let r = q(version, ex * ex + ey * ey);
                    [f(xb + f(h * r * ex)), f(yb + h * f(f(r) * ey))]
                } else {
                    let ex = f(f(f(xa - xb) * dn) + f(xb - xn) * w);
                    let ey = f(ya - yb) * dn + f(f(yb - yn) * w);
                    let r = q(version, ex * ex + ey * ey);
                    [f(xb + h * f(r * ex)), f(yb + f(h * f(r * ey)))]
                }
            });
            let toward = |e: P2, c: P2, l: f64| {
                let g = [f(c[0] - e[0]), f(c[1] - e[1])];
                let r = q(version, g[0] * g[0] + g[1] * g[1]);
                [f(e[0] + f(l * f(r * g[0]))), f(e[1] + f(l * f(r * g[1])))]
            };
            ends(c1, c2, |c1| toward(b, c1, f(h)), |c2| toward(a, c2, h))
        }
        RenderAs::Bryce50 => {
            let w = f(xb - xa);
            let h = HANDLE * w;
            let hh = f(h);
            let c1 = p.map(|[xp, yp]| {
                let dp = xa - xp;
                let dx = f(f(dp) * w + f(w * f(dp)));
                let dy = f(f(f(ya - yp) * w) + f(f(yb - ya) * f(dp)));
                let r = f(q(version, f(dx * dx + dy * dy)));
                [f(xa + f(h * r * dx)), f(ya + f(hh * f(r * dy)))]
            });
            let c2 = n.map(|[xn, yn]| {
                let dn = xn - xb;
                let ex = f(f(f(xa - xb) * f(dn)) + f(xb - xn) * w);
                let ey = f(f(f(ya - yb) * f(dn)) + f(f(yb - yn) * w));
                let r = q(version, f(ex * ex + ey * ey));
                [f(xb + f(hh * f(r * ex))), f(yb + f(hh * r * ey))]
            });
            let last = |c1: P2| {
                let g = [f(c1[0] - xb), f(c1[1] - yb)];
                let d = f(g[0] * g[0] + g[1] * g[1]).sqrt();
                [f(xb + f(hh * f(g[0] / d))), f(yb + f(hh * f(g[1] / d)))]
            };
            let first = |c2: P2| {
                let g = [f(c2[0] - xa), f(c2[1] - ya)];
                let r = q(version, f(g[0] * g[0] + g[1] * g[1]));
                [f(xa + f(hh * f(r * g[0]))), f(ya + f(hh * f(r * g[1])))]
            };
            ends(c1, c2, last, first)
        }
    }
}

/// C1 and C2 from the handles found at a and at b, the missing one toward the other.
fn ends(c1: Option<P2>, c2: Option<P2>, at_b: impl Fn(P2) -> P2, at_a: impl Fn(P2) -> P2) -> (P2, P2) {
    match (c1, c2) {
        (Some(c1), Some(c2)) => (c1, c2),
        (Some(c1), None) => (c1, at_b(c1)),
        (None, Some(c2)) => (at_a(c2), c2),
        (None, None) => unreachable!("a stretch with no neighbour is straight"),
    }
}

/// The stages from the control points on: the coefficients, the root s (held to 0…1 and
/// rounded to binary32) of k3·s^3 + k2·s^2 + k1·s + (k0 − u) (the constant term in binary64),
/// and y(s) by Horner's rule, each step ⟨kj + s·v⟩. (Accuracy, revised at log
/// entry 3171)
fn curve_stages(version: RenderAs, k: &MapArith, stretch: usize, p: [P2; 4], u: f64) -> Reading {
    let x = coefficients(k.k2, k.k3, [p[0][0], p[1][0], p[2][0], p[3][0]]);
    let y = coefficients(k.k2, k.k3, [p[0][1], p[1][1], p[2][1], p[3][1]]);
    let roots = cubic_roots(version, [x[0] - u, x[1], x[2], x[3]]);
    // The root taken: of those that lie in 0..1 or just outside it (the tolerance per version),
    // the one the solver gives last, held to 0..1; where none is, s is the binary32 nearest the
    // cubic's constant term k0 - u. (Outputs: on a curved stretch m is found by
    // solving x(s) = u)
    let taken = |r: f64| match version {
        RenderAs::Bryce30 => {
            let r = r as f32 as f64;
            -(ROOT_SLACK_30 as f64) < r && r - 1.0 < ROOT_SLACK_30 as f64
        }
        RenderAs::Bryce31 | RenderAs::Bryce50 => -(ROOT_SLACK_LATER as f64) < r && r - 1.0 < ROOT_SLACK_LATER as f64,
    };
    let s = match roots.iter().copied().filter(|&r| taken(r)).last() {
        Some(r) => r.clamp(0.0, 1.0) as f32,
        None => (x[0] - u) as f32,
    };
    let y_s = {
        let s = s as f64;
        let mut v = y[3];
        for j in (0..3).rev() {
            v = (y[j] + s * v) as f32 as f64;
        }
        v as f32
    };
    let (s, y_s) = (Some(s), Some(y_s));
    let f4 = |c: [f64; 4]| c.map(|v| v as f32);
    let f2 = |c: P2| c.map(|v| v as f32);
    Reading::Curved(Stages { stretch, c1: f2(p[1]), c2: f2(p[2]), x: f4(x), y: f4(y), s, y_s })
}

/// The time the track is at for the part u = (R(t) − first) / L: first + L · m(u), with u held
/// to 0…1 (a u that is not a number counts as 0).
pub fn time_at_part(version: RenderAs, map: &TimeMap, first: f64, last: f64, u: f64) -> f64 {
    let u = if u.is_nan() { 0.0 } else { u.clamp(0.0, 1.0) };
    first + (last - first) * map_value(version, map, u) as f64
}

/// The time the track is at for the time r = R(t) its keys would be read at.
pub fn mapped_time(version: RenderAs, map: &TimeMap, first: f64, last: f64, r: f64) -> f64 {
    time_at_part(version, map, first, last, (r - first) / (last - first))
}
