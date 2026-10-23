// SPDX-License-Identifier: MIT
//! Growing the wood: the trunk, its branches with their whorls, the outline test, the pieces
//! and the pieces they grow out of, and the roots, from one run of Bryce 5's generator.
//!
//! (A branch's curve, and gravity; Where the heights lie; A branch; A whorl; The
//! outline test; A piece; The trunk and the five roots; The draws, in order);
//! (orders and forms: lengths; a branch's lean; a branch's curve; carrying a branch's
//! curve; directions a frame carries; a child's vector; the turn around the parent; the heights;
//! the lean after a child grows; the roots; the step after a child grows; a placement's numbers)
//! (Orders and forms; Open behaviour read literally)

use super::arith::{
    E, FIVE_SEVENTHS, Frame, INV_E, PI, SinCos, TEN_THIRTEENTHS, UP, Vec3, add, curve_at, curve_at_2d, differs,
    greater, lean_matrix, lean_point, length, scale, smaller, sub, turn_x_about, turn_y, turn_z, unit_zyx,
};
use super::{GrowthInputs, GrowthRefusal, GrownTree, Leaf, Piece, Place, Placement, bounds, kind};
use crate::numerics::{Bryce5Rand, acos, pow};

/// The deepest level growth takes, whatever the settings hold.
pub const DEEPEST_LEVEL: u32 = 25;

/// A drawn signed number of one draw w: with m the low 23 bits, 1 − m / 2^23 where the top bit
/// is set, m / 2^23 − 1 where it is not (exact).
pub fn signed_of(w: u32) -> f64 {
    let m = f64::from(w & 0x7f_ffff) / 8_388_608.0;
    if w & 0x8000_0000 != 0 { 1.0 - m } else { m - 1.0 }
}

/// One draw as a signed number.
pub fn draw_signed(g: &mut Bryce5Rand) -> f64 {
    signed_of(g.next_u32())
}

/// One draw as a single from 0 up to 1, widened to a double (exact).,
pub fn draw_single(g: &mut Bryce5Rand) -> f64 {
    f64::from(g.next_unit())
}

/// The tree as it is being made: its pieces with their boxes, places and leaves, within our
/// bounds. (Data structures)
pub(super) struct Builder {
    pub tree: GrownTree,
    /// Each piece's box: least and greatest corner.
    boxes: Vec<[Vec3; 2]>,
    /// Work done (branch steps and children turned), against our bound.
    work: u64,
}

impl Builder {
    fn new() -> Builder {
        Builder { tree: GrownTree::default(), boxes: Vec::new(), work: 0 }
    }

    pub fn tick(&mut self) -> Result<(), GrowthRefusal> {
        self.work += 1;
        if self.work > bounds::STEPS {
            return Err(GrowthRefusal::BeyondBound("branch steps"));
        }
        Ok(())
    }

    /// The number of the last piece made so far, or none.
    pub fn last_piece(&self) -> Option<u32> {
        self.tree.pieces.len().checked_sub(1).map(|i| i as u32)
    }

    /// A piece from a to b with radius ρ', handed the piece `handed`; its number.
    pub fn add_piece(&mut self, a: Vec3, b: Vec3, radius: f64, handed: Option<u32>) -> Result<u32, GrowthRefusal> {
        if self.tree.pieces.len() >= bounds::PIECES {
            return Err(GrowthRefusal::BeyondBound("pieces"));
        }
        let v = sub(b, a);
        let frame = Frame::of(v).axes();
        let half = [v[0] / 2.0, v[1] / 2.0, v[2] / 2.0];
        // (A piece: 1.1·|b − a|; orders and forms: a placement's numbers and the Zeros
        // bullet): the product of the matrix of rows (ρ', 0, 0), (0, |b − a|·1.1, 0), (0, 0, ρ')
        // and F_{b − a}, then the middle; lengths: |b − a| as (x² + y²) + z²
        let [pa, pb, pc] = diagonal_product([radius, length(v) * 1.1, radius], frame);
        let placement: Placement = [pa, pb, pc, plus_zero(add(a, half))];
        let bx = box_of(&placement);
        let from = handed.filter(|&h| overlaps(&self.boxes[h as usize], &bx));
        self.tree.pieces.push(Piece { from, placement });
        self.boxes.push(bx);
        Ok((self.tree.pieces.len() - 1) as u32)
    }

    pub fn add_place(&mut self, point: Vec3, direction: Vec3) -> Result<(), GrowthRefusal> {
        if self.tree.places.len() >= bounds::PLACES {
            return Err(GrowthRefusal::BeyondBound("places"));
        }
        let piece = self.last_piece();
        let (point, direction) = (plus_zero(point), plus_zero(direction));
        self.tree.places.push(Place { point, direction, piece });
        Ok(())
    }

    /// A leaf at X along v: X + 1.1·|v|·(x·X_v + y·Y_v + z·Z_v).
    pub fn add_leaf(&mut self, at: Vec3, v: Vec3) -> Result<(), GrowthRefusal> {
        if self.tree.leaves.len() >= bounds::LEAVES {
            return Err(GrowthRefusal::BeyondBound("leaves"));
        }
        let frame = Frame::of(v).axes();
        // ; (orders and forms: a placement's numbers and the
        // Zeros bullet: the product of the matrix of rows (s, 0, 0), (0, s, 0), (0, 0, s), s =
        // |v|·1.1, and F_v; lengths: |v| as (x² + y²) + z²)
        let s = length(v) * 1.1;
        let [pa, pb, pc] = diagonal_product([s, s, s], frame);
        let placement = [pa, pb, pc, plus_zero(at)];
        self.tree.leaves.push(Leaf { placement });
        Ok(())
    }

    pub fn finish(self) -> GrownTree {
        self.tree
    }
}

/// The matrix of rows (d0, 0, 0), (0, d1, 0), (0, 0, d2) times a frame's rows X, Y, Z: the number
/// at row r and column c is the sum over k of row r's k-th number times the c-th coordinate of the
/// frame's k-th row, added in the order of k 3, 2, 1 for the first coordinate, 3, 1, 2 for the
/// second and 2, 1, 3 for the third. A zero has the sign the sum gives it: +0 unless all three
/// products are -0 (so -0 only in a level piece or leaf pointing toward negative x).
fn diagonal_product(d: [f64; 3], f: [Vec3; 3]) -> [Vec3; 3] {
    const ORDER: [[usize; 3]; 3] = [[2, 1, 0], [2, 0, 1], [1, 0, 2]];
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let [k0, k1, k2] = ORDER[c];
            let term = |k: usize| if k == r { d[r] * f[k][c] } else { 0.0 * f[k][c] };
            (term(k0) + term(k1)) + term(k2)
        })
    })
}

/// A zero among a place's point and direction and the place of a placement is +0: every point of
/// the growth is finished by adding a point or place whose zeros are +0, and every direction is an
/// image, a multiple of one or a difference of two points of unit length.
fn plus_zero(v: Vec3) -> Vec3 {
    v.map(|x| if x == 0.0 { 0.0 } else { x })
}

/// The smallest box along the axes holding the cube from −1 to 1 carried by the placement:
/// its eight corners, each carried as ((x·A + y·B) + z·C) + T with x, y, z = ±1.
fn box_of(p: &Placement) -> [Vec3; 2] {
    let [a, b, c, t] = *p;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for sa in [1.0, -1.0] {
        for sb in [1.0, -1.0] {
            for sc in [1.0, -1.0] {
                for i in 0..3 {
                    let x = ((sa * a[i] + sb * b[i]) + sc * c[i]) + t[i];
                    lo[i] = smaller(lo[i], x);
                    hi[i] = greater(hi[i], x);
                }
            }
        }
    }
    [lo, hi]
}

/// Overlap along every axis, each box's least below the other's greatest.
fn overlaps(p: &[Vec3; 2], q: &[Vec3; 2]) -> bool {
    (0..3).all(|i| p[0][i] < q[1][i] && q[0][i] < p[1][i])
}

/// A branch's curve in its own space: Q0 to Q3 bent by β, multiplied by
/// 1 / ((((c + ℓ3) + ℓ1) + ℓ2)·0.5); and L = |V|.
pub(super) fn own_curve(v: Vec3, gravity: f64, trunk: bool) -> ([Vec3; 4], f64) {
    let l = length(v);
    let lean = acos(v[1] * (1.0 / l));
    let g = if gravity == 0.0 { 0.001 } else { gravity };
    let limit = (PI / 2.0) * (1.0 - g);
    let bend = if trunk {
        lean / 2.0
    } else if lean < limit {
        smaller(0.3 * lean, (limit - lean) / 2.0)
    } else {
        greater(0.3 * (lean - PI), (limit - lean) / 2.0)
    };
    // : the four points on the y axis, turned by
    // Tx(β) with ĉ and ŝ, each about another as it then stands: Q3 about Q2; Q3 about Q1; Q2
    // about Q1; Q3 about Q0; Q2 about Q0; Q1 about Q0.
    let t = SinCos::of(bend);
    let (c, s) = (t.cos_double(), t.sin_double());
    let mut q: [Vec3; 4] = [
        [0.0, 0.0, 0.0],
        [0.0, 0.3333, 0.0],
        [0.0, 0.6666, 0.0],
        [0.0, 1.0, 0.0],
    ];
    for (v, p) in [(3, 2), (3, 1), (2, 1), (3, 0), (2, 0), (1, 0)] {
        q[v] = turn_x_about(q[v], q[p], c, s);
    }
    let [l1, l2, l3, chord] =
        [length(sub(q[1], q[0])), length(sub(q[2], q[1])), length(sub(q[3], q[2])), length(sub(q[3], q[0]))];
    let k = 1.0 / ((((chord + l3) + l1) + l2) * 0.5);
    (q.map(|p| scale(k, p)), l)
}

/// The wood's grower: the settings, the run, the tree so far.
struct Grower<'a> {
    s: &'a GrowthInputs,
    rng: Bryce5Rand,
    b: Builder,
    /// m = 3E, the least radius.
    m: f64,
    /// The up of O3.
    top: f64,
    /// τ: π/2, or 0 with conifer twigs.
    turn: f64,
}

/// A branch's own numbers while it grows.
struct Branch {
    trunk: bool,
    /// The level at its start.
    start_level: u32,
    /// The level its next children grow from (raised as's whorl says).
    level: u32,
    length: f64,
    n: i64,
    h0: f64,
    last: Option<u32>,
}

/// The step's lean, its turn, and whether a child grew at it.
#[derive(Default)]
struct StepLean {
    lean: f64,
    turn: f64,
    grew: bool,
}

impl Grower<'_> {
    /// H(i) = h0 + (top − h0)·(1 − (e^(x^p)·(1/e))·x), x = i / n. (Where the heights
    /// lie),
    fn height(&self, br: &Branch, i: i64) -> f64 {
        let x = i as f64 / br.n as f64;
        br.h0 + (self.top - br.h0) * (1.0 - (pow(E, pow(x, self.s.power)) * INV_E) * x)
    }

    /// The reach for a height h and an angle a.
    fn reach(&self, h: f64, a: f64) -> f64 {
        let o = &self.s.outline;
        if h < o[0][1] || h > o[3][1] {
            return 0.0;
        }
        let t = SinCos::of(a);
        let mut u = 0.0;
        let mut pts = [[0.0f64; 2]; 6]; // (along, off) at u = 0, 0.2, … 1
        for (i, p) in pts.iter_mut().enumerate() {
            if i > 0 {
                u += 0.2;
            }
            let [x, y] = curve_at_2d(o, u);
            *p = [t.sin_times(x) + t.cos_times(y - h), t.cos_times(x) - t.sin_times(y - h)];
        }
        for w in pts.windows(2) {
            let ([aa, ao], [ba, bo]) = (w[0], w[1]);
            let r = if ao == 0.0 {
                aa
            } else if bo == 0.0 {
                ba
            } else if (ao < 0.0) != (bo < 0.0) {
                aa + (ba - aa) * (ao / (ao - bo))
            } else {
                0.0
            };
            if r > 0.0 {
                return r;
            }
        }
        0.0
    }

    /// A branch of level `level` from p with vector v and radius `radius`, n heights, the first
    /// at h0, handed the piece `handed`.
    #[allow(clippy::too_many_arguments)]
    fn grow_branch(
        &mut self,
        level: u32,
        p: Vec3,
        v: Vec3,
        radius: f64,
        n: i64,
        h0: f64,
        handed: Option<u32>,
        trunk: bool,
    ) -> Result<(), GrowthRefusal> {
        if level > DEEPEST_LEVEL {
            return Ok(());
        }
        let (own, l) = own_curve(v, self.s.gravity, trunk);
        let f = Frame::of(v);
        let mut q: [Vec3; 4] = own.map(|o| f.carry_curve_point(o, l, p));
        let m = self.m;
        let count = smaller((l / radius).trunc(), (4.0 * l + 4.0).trunc());
        let k_steps = greater(1.0, count);
        let delta = 1.0 / k_steps;
        let r_at = |t: f64| (1.0 - t) * (radius - m) + m;
        let mut br = Branch { trunk, start_level: level, level, length: l, n, h0, last: handed };
        let mut t_star = 0.0;
        if n != 0 {
            let mut t = delta;
            let mut step = delta;
            let mut prev = curve_at(&q, 0.0);
            let mut k: i64 = 0;
            while t <= 1.0 {
                self.b.tick()?;
                let c = curve_at(&q, t);
                let rt = r_at(t);
                let before = prev;
                if differs(c, prev) {
                    br.last = Some(self.b.add_piece(prev, c, rt, br.last)?);
                    t_star = t;
                }
                prev = c;
                let frame = Frame::of(sub(c, before));
                let mut lean = StepLean::default();
                // Step 2: whorls at every height below top that t is above, k = n included.
                while k <= n {
                    let h = self.height(&br, n - k);
                    if !(h < self.top && t > h) {
                        break;
                    }
                    self.whorl(&mut br, k, h, c, rt, frame, &mut lean)?;
                    k += 1;
                }
                // Step 3.
                if lean.grew {
                    if !(lean.lean == 0.0 && lean.turn == 0.0) {
                        let mm = lean_matrix(frame, lean.lean, lean.turn);
                        q = q.map(|x| lean_point(&mm, c, x));
                    }
                    step = (delta / ((radius - m) + m)) * rt;
                }
                // Step 4: no more steps once n heights have been set. A step that set one more
                // (k = n + 1) leaves the branch going on to t > 1, setting no more heights (
                // Open; chosen by the tree cases' feedback,.)
                if k == n {
                    break;
                }
                let next = t + step;
                if !(next > t) {
                    return Err(GrowthRefusal::Unending);
                }
                t = next;
            }
        }
        let at = curve_at(&q, t_star);
        let dir = unit_zyx(sub(at, curve_at(&q, t_star - 0.5)));
        self.b.add_place(at, dir)
    }

    /// The k-th whorl, at height h, at the step's point c with radius r and frame f.
    #[allow(clippy::too_many_arguments)]
    fn whorl(
        &mut self,
        br: &mut Branch,
        k: i64,
        h: f64,
        c: Vec3,
        r: f64,
        f: Frame,
        step: &mut StepLean,
    ) -> Result<(), GrowthRefusal> {
        let s = self.s;
        let (bt, bb) = (i64::from(s.trunk_branches), i64::from(s.branch_branches));
        let count: i64 = if br.trunk {
            bt.min(bb + br.n - 1 - k)
        } else if br.start_level > 3 && bb > 2 {
            2
        } else {
            bb
        };
        let rho = s.randomness;
        let kf = k as f64;
        let r0 = draw_signed(&mut self.rng);
        let base = if br.trunk && count == 1 {
            (rho * r0) / 2.0 - (PI / 2.0) * (k % 2) as f64
        } else if br.trunk {
            (2.0 * PI) * r0
        } else {
            (rho * r0) / 2.0 - self.turn * kf
        };
        let m = self.m;
        let [s1, s2] = s.shares;
        let [a1, a2] = s.angles;
        for j in 0..count.max(0) {
            self.b.tick()?;
            let rj = draw_signed(&mut self.rng);
            let phi = if br.trunk && count == 1 {
                let r1 = draw_signed(&mut self.rng);
                ((rho * r1) / 2.0 + (PI / 2.0) * kf) + base
            } else {
                let w = (2.0 * PI) / count as f64;
                ((j as f64 * w) + ((rj / 2.0) * rho) * w) + base
            };
            let sigma = s1 + (s2 - s1) * draw_single(&mut self.rng);
            let (keep, child_r) = if sigma < 0.5 {
                (greater((1.0 - sigma * sigma).sqrt() * r, m), greater(sigma * r, m))
            } else {
                let sigma2 = s1 + (s2 - s1) * draw_single(&mut self.rng);
                (greater(sigma * r, m), greater(sigma2 * r, m))
            };
            let part = child_r / (child_r + keep);
            let theta = smaller(1.0, (2.0 * child_r) / (3.0 * s.trunk_thickness - m));
            let alpha = a1 + (a2 - a1) * (1.0 - theta);
            let child_lean = alpha - alpha * part;
            let parent_lean = if part < 0.2 { 0.0 } else { alpha * part };
            let d = f.carry_direction(turn_y(phi, turn_z(-child_lean, UP)));
            let reach = self.reach(h, child_lean);
            if reach > 0.0001 {
                let _ = draw_signed(&mut self.rng);
                let n_child = (1..=br.n).find(|&i| 1.0 - self.height(br, i) > reach).map_or(0, |i| i - 1);
                let h0_child = 1.0 / (n_child + 1) as f64;
                let v = scale(reach * br.length, d);
                self.grow_branch(br.level + 1, c, v, child_r, n_child, h0_child, br.last, false)?;
                step.grew = true;
                if !br.trunk && count > 1 {
                    br.level += 1;
                }
            }
            if count == 1 && (step.lean == 0.0 || step.lean > parent_lean) {
                step.lean = parent_lean;
                step.turn = phi;
            }
        }
        Ok(())
    }

    /// The three pieces under the foot and the five roots.
    fn roots(&mut self) -> Result<(), GrowthRefusal> {
        let foot: Vec3 = [0.0, -1.0, 0.0];
        let rr = 0.625 * (3.0 * self.s.trunk_thickness);
        let w = smaller(rr, 0.1);
        let y1 = -1.0 + 0.3;
        let y2 = y1 - 0.15;
        let y3 = y2 - 0.1;
        let end: Vec3 = [0.0, (-1.0 + 0.3) - 0.6, 0.0];
        let first = if self.b.tree.pieces.is_empty() { None } else { Some(0) };
        let p1 = self.b.add_piece([0.0, y1, 0.0], end, rr * FIVE_SEVENTHS, first)?;
        let p2 = self.b.add_piece([0.0, y2, 0.0], end, rr * FIVE_SEVENTHS, Some(p1))?;
        let p3 = self.b.add_piece([0.0, y3, 0.0], end, rr * TEN_THIRTEENTHS, Some(p2))?;
        for k in 0..5 {
            let u = draw_single(&mut self.rng);
            let angle = f64::from(k) * (((u - 0.5) * 0.5) + (2.0 * PI) * 0.2);
            let own: [Vec3; 4] = [
                [0.0, 0.0, 0.0],
                [-3.5 * w, -2.6, 0.0],
                [-7.0 * w, -1.2, 0.0],
                [-15.5 * w, -2.2, 0.0],
            ];
            // : the rows of Ty(angle) with ĉ and ŝ, each
            // number one product by 0.3; each coordinate ((x·M1 + y·M2) + z·M3) + P0.
            let ts = SinCos::of(angle);
            let (c, s) = (ts.cos_double(), ts.sin_double());
            let rows: [Vec3; 3] = [
                [0.3 * c, 0.3 * 0.0, 0.3 * -s],
                [0.3 * 0.0, 0.3 * 1.0, 0.3 * 0.0],
                [0.3 * s, 0.3 * 0.0, 0.3 * c],
            ];
            let q = own.map(|o| -> Vec3 {
                std::array::from_fn(|i| ((o[0] * rows[0][i] + o[1] * rows[1][i]) + o[2] * rows[2][i]) + foot[i])
            });
            let mut v0 = 0.0;
            let mut last = p3;
            let tenth = 0.1 * rr;
            let three_tenths = 3.0 * tenth;
            for i in 0..5 {
                let v1 = v0 + 0.2;
                let radius = smaller(rr, ((1.2 - v1) * (0.5 * rr - tenth)) + three_tenths);
                let handed = match i {
                    0 => p2,
                    1 => p3,
                    _ => last,
                };
                last = self.b.add_piece(curve_at(&q, v0), curve_at(&q, v1), radius, Some(handed))?;
                v0 = v1;
            }
        }
        Ok(())
    }
}

/// Grow the wood from the settings: the trunk with every branch, then the roots.
pub(super) fn grow_wood(s: &GrowthInputs, noise_bits_five: &[u32; 64]) -> Result<Builder, GrowthRefusal> {
    let mut g = Grower {
        s,
        rng: Bryce5Rand::new(s.seed, noise_bits_five),
        b: Builder::new(),
        m: 3.0 * s.least_thickness,
        top: s.outline[3][1],
        turn: if s.leaf_kind == kind::CONIFER { 0.0 } else { PI / 2.0 },
    };
    let u0 = draw_single(&mut g.rng);
    // The range's words read as signed 32-bit numbers: grows a trunk with N0 below 0 "from a
    // range that holds a negative number" (our reading).
    let [h1, h2] = s.trunk_heights.map(|h| f64::from(h as i32));
    let n0 = (((h1 - h2) * u0 + 0.5) + h2).trunc();
    // : N0 = 0, the trunk makes no piece and the
    // first piece under the foot grows out of none; N0 below 0, no whorl is set, the count of
    // heights set never equals N0 and the trunk steps on to t > 1.
    let n0 = n0 as i64;
    let trunk_radius = 3.0 * s.trunk_thickness;
    g.grow_branch(1, [0.0, -1.0, 0.0], [0.0, 2.0, 0.0], trunk_radius, n0, s.outline[0][1], None, true)?;
    g.roots()?;
    Ok(g.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_numbers() {
        // Terms: top bit set gives 1 − m / 2^23, clear gives m / 2^23 − 1.
        assert_eq!(signed_of(0x8000_0000), 1.0);
        assert_eq!(signed_of(0x0000_0000), -1.0);
        assert_eq!(signed_of(0x807f_ffff), 1.0 - 8_388_607.0 / 8_388_608.0);
        assert_eq!(signed_of(0x007f_ffff), 8_388_607.0 / 8_388_608.0 - 1.0);
        assert_eq!(signed_of(0x0040_0000), -0.5);
        assert_eq!(signed_of(0xff40_0000), 0.5);
    }

    #[test]
    fn trunk_curve_is_straight() {
        // The trunk's vector (0, 2, 0) leans 0, so β = 0 and its points lie on the y axis.
        let (q, l) = own_curve([0.0, 2.0, 0.0], 0.5, true);
        assert_eq!(l, 2.0);
        assert!(q.iter().all(|p| p[0] == 0.0 && p[2] == 0.0));
        assert_eq!(q[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn boxes_overlap_strictly() {
        let a = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let b = [[1.0, 0.0, 0.0], [2.0, 1.0, 1.0]];
        let c = [[0.5, 0.5, 0.5], [2.0, 2.0, 2.0]];
        assert!(!overlaps(&a, &b));
        assert!(overlaps(&a, &c));
    }
}
