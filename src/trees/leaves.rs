// SPDX-License-Identifier: MIT
//! Leaves: for each place in turn, its stalk's pieces and its leaves, by the leaves' kind, from
//! the leaves' own run of the generator.
//!
//! (The leaves' run; Terms: a leaf at X along v, a stalk, a pair's vectors, a lean and
//! scale, the end leaf; Each kind)
//! (Orders and forms)

use super::arith::{
    Frame, PI, PI_FIFTHS, PI_SIXTHS, PI_THIRDS, SEVENTEEN_TWENTY_FIRSTS, UP, Vec3, add, curve_at, scale, sub, turn_x,
    turn_y, turn_z, unit,
};
use super::wood::{Builder, draw_signed, draw_single, own_curve};
use super::{GrowthInputs, GrowthRefusal, LeafRun, Place, kind};
use crate::numerics::Bryce5Rand;

/// A stalk's curve S, its points already carried and scaled.
struct Stalk {
    q: [Vec3; 4],
}

impl Stalk {
    /// The curve for the vector d with gravity 0.9, not as the trunk, carried by the frame of
    /// unit(d) and multiplied by (σ'·0.5)·10.
    fn new(d: Vec3, leaf_size2: f64) -> Stalk {
        let (own, _) = own_curve(d, 0.9, false);
        let f = Frame::of(unit(d));
        let k = (leaf_size2 * 0.5) * 10.0;
        Stalk { q: own.map(|o| scale(k, f.apply(o))) }
    }

    fn at(&self, u: f64) -> Vec3 {
        curve_at(&self.q, u)
    }

    /// 0.1·j as the stalk's parameter.
    fn u_of(j: u32) -> f64 {
        0.1 * f64::from(j)
    }

    /// Fj: the frame of S(0.1·(j + 1)) − S(0.1·j).
    fn frame(&self, j: u32) -> Frame {
        Frame::of(sub(self.at(Stalk::u_of(j + 1)), self.at(Stalk::u_of(j))))
    }
}

/// The leaves' grower: the settings' numbers reads, its run, the spiral's turn.
struct LeafGrower<'a> {
    s: &'a GrowthInputs,
    rng: Bryce5Rand,
    /// σ' = 2σ.
    size: f64,
    /// h = 3E/2.
    h: f64,
    /// ψ, through all the places.
    psi: f64,
    psi_step: f64,
}

impl LeafGrower<'_> {
    /// A lean and scale for piece j of q, by r1 then r2.
    fn lean_and_scale(&mut self, j: u32, q: u32) -> (f64, f64) {
        let rho = self.s.randomness;
        let r1 = draw_signed(&mut self.rng);
        let r2 = draw_signed(&mut self.rng);
        let first = ((r1 / 4.0) * (PI / 2.0)) * rho;
        if q == 0 {
            (first + PI_THIRDS, (rho * r2) / 4.0 + 1.0)
        } else {
            let (jf, qf) = (f64::from(j), f64::from(q));
            (
                first + (((qf - jf) / qf) * PI) * (1.0 / 3.0),
                (rho * r2) / 4.0 + ((jf / qf) + 1.0) / 2.0,
            )
        }
    }

    /// The pair's vectors' first side: Fj(κ·Ty(π/6)(Tz(−(π/5 + ζ))(0, 1, 0))), κ multiplying the
    /// turned vector before the frame carries it.; (orders and
    /// forms: leaves; directions a frame carries)
    fn first_side(f: Frame, lean: f64, k: f64) -> Vec3 {
        f.carry_direction(scale(k, turn_y(PI_SIXTHS, turn_z(-(PI_FIFTHS + lean), UP))))
    }

    /// The other side's: Fj(κ·Ty(−π/6)(Tz(π/5 + ζ)(0, 1, 0))).;
    /// (orders and forms: leaves; directions a frame carries)
    fn other_side(f: Frame, lean: f64, k: f64) -> Vec3 {
        f.carry_direction(scale(k, turn_y(-PI_SIXTHS, turn_z(PI_FIFTHS + lean, UP))))
    }

    /// A leaf by a vector v at the stalk's point S: at (S + Pp) + h·v, along σ'·v (`point` is
    /// S + Pp).;
    fn leaf_by(&self, b: &mut Builder, point: Vec3, v: Vec3) -> Result<(), GrowthRefusal> {
        b.add_leaf(add(point, scale(self.h, v)), scale(self.size, v))
    }

    /// A leaf along σ'·e at point + h·e, e = unit(S(ua) − S(ub)) (the end leaf, and the pairs'
    /// last leaf where n is odd), standing at (S + Pp) + h·e. (Terms: the end leaf;
    /// Pairs);
    fn leaf_at_end(&self, b: &mut Builder, pp: Vec3, st: &Stalk, ua: f64, ub: f64) -> Result<(), GrowthRefusal> {
        let sa = st.at(ua);
        let e = unit(sub(sa, st.at(ub)));
        b.add_leaf(add(add(pp, sa), scale(self.h, e)), scale(self.size, e))
    }

    /// Stalk piece j, handed `last`; its number.
    fn stalk_piece(&self, b: &mut Builder, pp: Vec3, st: &Stalk, j: u32, last: &mut Option<u32>) -> Result<(), GrowthRefusal> {
        let a = add(pp, st.at(Stalk::u_of(j)));
        let c = add(pp, st.at(Stalk::u_of(j + 1)));
        *last = Some(b.add_piece(a, c, self.h, *last)?);
        Ok(())
    }

    /// The spiral's vector for a leaf of piece j: Fj(Ty(ψ)(Tx(ρ·u·(π/2)/2 + π/3)(0, 1, 0))),
    /// then ψ grows by π × 17/21.
    fn spiral_vector(&mut self, f: Frame) -> Vec3 {
        let u = draw_single(&mut self.rng);
        let a = ((u / 2.0) * (PI / 2.0)) * self.s.randomness + PI_THIRDS;
        let v = f.carry_direction(turn_y(self.psi, turn_x(a, UP)));
        self.psi += self.psi_step;
        v
    }

    fn place(&mut self, b: &mut Builder, place: Place) -> Result<(), GrowthRefusal> {
        let n = self.s.leaves_per_place;
        let pp = place.point;
        let d = place.direction;
        match self.s.leaf_kind {
            kind::PAIRS => {
                let st = Stalk::new(d, self.size);
                let q = (n / 2).saturating_sub(1);
                let mut last = place.piece;
                for j in 0..=q {
                    b.tick()?;
                    let f = st.frame(j);
                    let point = add(pp, st.at(Stalk::u_of(j)));
                    let (lean, k) = self.lean_and_scale(j, q);
                    self.leaf_by(b, point, Self::first_side(f, lean, k))?;
                    let (lean, k) = self.lean_and_scale(j, q);
                    self.leaf_by(b, point, Self::other_side(f, lean, k))?;
                    if j < q {
                        self.stalk_piece(b, pp, &st, j, &mut last)?;
                    }
                }
                if n % 2 == 1 {
                    self.stalk_piece(b, pp, &st, q, &mut last)?;
                    self.leaf_at_end(b, pp, &st, Stalk::u_of(q + 1), Stalk::u_of(q))?;
                }
            }
            kind::ALTERNATING | kind::SPIRAL => {
                let spiral = self.s.leaf_kind == kind::SPIRAL;
                let st = Stalk::new(d, self.size);
                let p = n / 2;
                let before = Stalk::u_of(p.wrapping_sub(1));
                self.leaf_at_end(b, pp, &st, Stalk::u_of(p), before)?;
                let mut last = place.piece;
                for j in 0..p {
                    b.tick()?;
                    self.stalk_piece(b, pp, &st, j, &mut last)?;
                    let f = st.frame(j);
                    let point1 = add(pp, st.at(Stalk::u_of(j)));
                    let point2 = add(pp, st.at(Stalk::u_of(j) + 0.05));
                    if spiral {
                        if n % 2 == 1 || j > 0 {
                            let v = self.spiral_vector(f);
                            self.leaf_by(b, point1, v)?;
                        }
                        let v = self.spiral_vector(f);
                        self.leaf_by(b, point2, v)?;
                    } else {
                        let (lean, k) = self.lean_and_scale(j, p);
                        if n % 2 == 1 || j > 0 {
                            self.leaf_by(b, point1, Self::first_side(f, lean, k))?;
                        }
                        self.leaf_by(b, point2, Self::other_side(f, lean, k))?;
                    }
                }
            }
            kind::BUNCHES => {
                let f = Frame::of(unit(d));
                let w = (0.05 * f64::from(n)) * PI;
                for _ in 0..n {
                    b.tick()?;
                    let u1 = draw_single(&mut self.rng);
                    let u2 = draw_single(&mut self.rng);
                    let u3 = draw_single(&mut self.rng);
                    let turned = turn_z((u2 - 0.5) * w, turn_x((u1 - 0.5) * w, UP));
                    let k = 1.0 - (self.s.randomness * u3) / 2.0;
                    // (orders and forms: leaves: 1 − ρ·u3/2 multiplies the turned
                    // vector before the frame carries it; directions a frame carries)
                    let v = f.carry_direction(scale(k, turned));
                    b.add_leaf(pp, scale(self.size, v))?;
                }
            }
            kind::CONIFER => {
                let e = unit(d);
                let along = scale(self.size, e);
                let mut last = place.piece;
                for j in 0..n {
                    b.tick()?;
                    let x = add(pp, scale(self.size, scale(f64::from(j), e)));
                    last = Some(b.add_piece(x, add(x, along), self.h, last)?);
                    b.add_leaf(x, along)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// The stalks and leaves of every place, in the places' order.
pub(super) fn grow_leaves(
    s: &GrowthInputs,
    run: LeafRun,
    noise_bits_five: &[u32; 64],
    b: &mut Builder,
) -> Result<(), GrowthRefusal> {
    if s.leaves_per_place == 0 {
        return Ok(());
    }
    let mut g = LeafGrower {
        s,
        rng: Bryce5Rand::from_state(run.sum, run.positions, noise_bits_five),
        size: 2.0 * s.leaf_size,
        h: (3.0 * s.least_thickness) / 2.0,
        psi: 0.0,
        psi_step: SEVENTEEN_TWENTY_FIRSTS * PI,
    };
    for i in 0..b.tree.places.len() {
        let place = b.tree.places[i];
        g.place(b, place)?;
    }
    Ok(())
}
