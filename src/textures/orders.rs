// SPDX-License-Identifier: MIT
//! The orders of operations the texture entries leave to the vectors, and the ones the vectors
//! decide, per version.
//!
//! (Accuracy: the three-term sums of methods 3 and 5 to 7),
//! (Orders the vectors decide)

use crate::numerics::Reg53;
use crate::version::RenderAs;

/// How a three-term sum a + b + c is grouped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order3 {
    /// (a + b) + c
    AbC,
    /// (a + c) + b
    AcB,
    /// (b + c) + a
    BcA,
}

impl Order3 {
    pub const ALL: [Order3; 3] = [Order3::AbC, Order3::AcB, Order3::BcA];

    pub fn sum(self, [a, b, c]: [f64; 3]) -> f64 {
        match self {
            Order3::AbC => (a + b) + c,
            Order3::AcB => (a + c) + b,
            Order3::BcA => (b + c) + a,
        }
    }

    /// The three terms in the order this sum takes them: `(p0 + p1) + p2`.
    pub fn arrange(self, [a, b, c]: [f64; 3]) -> [f64; 3] {
        match self {
            Order3::AbC => [a, b, c],
            Order3::AcB => [a, c, b],
            Order3::BcA => [b, c, a],
        }
    }
}

/// The order of a four-term binary64 sum, orders equal by commutativity counted as one: a
/// chain ((p0 + p1) + p2) + p3 (12 of them) or two pairs (p0 + p1) + (p2 + p3) (3), the terms
/// named by their places 0 to 3. (Arrangements)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order4 {
    Chain([u8; 4]),
    Pairs([u8; 4]),
}

impl Order4 {
    /// Every order of four terms but for commutativity (15).
    pub fn all() -> Vec<Order4> {
        let mut all = Vec::new();
        for a in 0..4u8 {
            for b in a + 1..4 {
                let rest: Vec<u8> = (0..4).filter(|k| *k != a && *k != b).collect();
                all.push(Order4::Chain([a, b, rest[0], rest[1]]));
                all.push(Order4::Chain([a, b, rest[1], rest[0]]));
            }
        }
        for b in 1..4u8 {
            let rest: Vec<u8> = (1..4).filter(|k| *k != b).collect();
            all.push(Order4::Pairs([0, b, rest[0], rest[1]]));
        }
        all
    }

    pub fn sum(self, t: [f64; 4]) -> f64 {
        match self {
            Order4::Chain(p) => ((t[p[0] as usize] + t[p[1] as usize]) + t[p[2] as usize]) + t[p[3] as usize],
            Order4::Pairs(p) => (t[p[0] as usize] + t[p[1] as usize]) + (t[p[2] as usize] + t[p[3] as usize]),
        }
    }
}

/// Which two of three factors x·y·z are multiplied first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreeFactor {
    /// (x·y)·z
    FirstPair,
    /// x·(y·z)
    LastPair,
    /// (x·z)·y
    OuterPair,
}

impl ThreeFactor {
    pub const ALL: [ThreeFactor; 3] = [ThreeFactor::FirstPair, ThreeFactor::LastPair, ThreeFactor::OuterPair];

    pub fn product(self, [x, y, z]: [f64; 3]) -> f64 {
        match self {
            ThreeFactor::FirstPair => (x * y) * z,
            ThreeFactor::LastPair => x * (y * z),
            ThreeFactor::OuterPair => (x * z) * y,
        }
    }
}

/// The forms of the smoothstep 3t² − 2t³ tried against filters.jsonl.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Smoothstep {
    /// t·t·(3 − 2t)
    Factored,
    /// 3t² − 2t³ as (3·(t·t)) − (2·((t·t)·t))
    Expanded,
    /// (t·t)·3 − (t·t)·(t·2)
    Shared,
    /// t·(t·(3 − 2t))
    Nested,
}

impl Smoothstep {
    pub const ALL: [Smoothstep; 4] = [Smoothstep::Factored, Smoothstep::Expanded, Smoothstep::Shared, Smoothstep::Nested];

    pub fn eval(self, t: f64) -> f64 {
        match self {
            Smoothstep::Factored => t * t * (3.0 - 2.0 * t),
            Smoothstep::Expanded => 3.0 * (t * t) - 2.0 * ((t * t) * t),
            Smoothstep::Shared => (t * t) * 3.0 - (t * t) * (t * 2.0),
            Smoothstep::Nested => t * (t * (3.0 - 2.0 * t)),
        }
    }
}

/// How method 4's product of three factors is grouped: ((P + 0.1)·N)·w as stated, and the two
/// groupings `arrangements.jsonl` excludes (kept so the test can try them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method4Grouping {
    /// ((P + 0.1)·N)·w
    SumSampleFirst,
    /// (P + 0.1)·(N·w)
    SampleWeightFirst,
    /// ((P + 0.1)·w)·N
    SumWeightFirst,
}

/// Where's factor 0.002777777778 enters a component's phase space map: on the
/// frequencies before the map is formed, or on each entry of the formed map (as stated, log
/// 17004: each entry formed and rounded, then times the factor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhaseFactor {
    OnFrequencies,
    OnEntries,
}

/// Where the term 0.5/(n + 1) enters the bump gradient's sum of the three squares (the squares
/// themselves in the order `bump_squares` gives). (How a component forms its bump
/// gradient: "bump-gradients.jsonl decides it")
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BumpSum {
    /// ((x² + y²) + z²) + t
    Last,
    /// ((t + x²) + y²) + z²
    First,
    /// ((x² + t) + y²) + z²
    Second,
    /// ((x² + y²) + t) + z²
    Third,
    /// (x² + y²) + (z² + t)
    Paired,
}

impl BumpSum {
    pub const ALL: [BumpSum; 5] = [BumpSum::Last, BumpSum::First, BumpSum::Second, BumpSum::Third, BumpSum::Paired];

    pub fn eval(self, [x, y, z]: [f64; 3], t: f64) -> f64 {
        match self {
            BumpSum::Last => ((x + y) + z) + t,
            BumpSum::First => ((t + x) + y) + z,
            BumpSum::Second => ((x + t) + y) + z,
            BumpSum::Third => ((x + y) + t) + z,
            BumpSum::Paired => (x + y) + (z + t),
        }
    }
}

/// The forms of type 49's 2-D and 3-D sums ("the order and the form of its sums of
/// squares ... type-49-sums.jsonl decides them").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type49Form {
    /// 1 − (the sum of the squares in the order given).
    OneLessSum(Order3),
    /// ((1 − x²) − y²) − z².
    ChainXyz,
    /// ((1 − z²) − y²) − x².
    ChainZyx,
}

impl Type49Form {
    pub const ALL: [Type49Form; 5] = [
        Type49Form::OneLessSum(Order3::AbC),
        Type49Form::OneLessSum(Order3::AcB),
        Type49Form::OneLessSum(Order3::BcA),
        Type49Form::ChainXyz,
        Type49Form::ChainZyx,
    ];

    /// 1 less the squares of `sq` (two or three of them).
    pub fn eval(self, sq: &[f64]) -> f64 {
        match (self, sq) {
            (Type49Form::OneLessSum(_), [a, b]) => 1.0 - (a + b),
            (Type49Form::OneLessSum(o), [a, b, c]) => 1.0 - o.sum([*a, *b, *c]),
            (Type49Form::ChainXyz, _) => sq.iter().fold(1.0, |acc, s| acc - s),
            (Type49Form::ChainZyx, _) => sq.iter().rev().fold(1.0, |acc, s| acc - s),
            _ => f64::NAN,
        }
    }

    /// The same in the extended register at 53 bits (finite squares), the value kept unrounded
    /// to binary64.
    pub fn eval_wide(self, sq: &[Reg53]) -> Option<Reg53> {
        let one = Reg53::from_f64(1.0)?;
        Some(match (self, sq) {
            (Type49Form::OneLessSum(_), [a, b]) => one.sub(a.add(*b)),
            (Type49Form::OneLessSum(o), [a, b, c]) => {
                let [p, q, r] = o.arrange([0.0, 1.0, 2.0]);
                let t = [*a, *b, *c];
                one.sub(t[p as usize].add(t[q as usize]).add(t[r as usize]))
            }
            (Type49Form::ChainXyz, _) => sq.iter().fold(one, |acc, s| acc.sub(*s)),
            (Type49Form::ChainZyx, _) => sq.iter().rev().fold(one, |acc, s| acc.sub(*s)),
            _ => return None,
        })
    }
}

/// Every order the texture stages take, for one version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureOrders {
    /// : the three-factor entries and q1's, q2's terms.
    pub space: super::space::SpaceOrders,
    /// : the 3-D products of types 1 and 19.
    pub type1: ThreeFactor,
    pub type19: ThreeFactor,
    /// : type 25's sum of three squares (x², y², z²).
    pub type25: Order3,
    /// : the first two coordinates' three-term sums of method 3, and of methods 5 to 7.
    pub method3: [Order3; 2],
    pub method5: [Order3; 2],
    /// : method 4's grouping of its product (stated).
    pub method4: Method4Grouping,
    /// : the form of s.
    pub smoothstep: Smoothstep,
    /// : the form of ease (blend mode 13's weight separates them).
    pub ease: Smoothstep,
    /// : value mode 6's sum a + b − 0.5.
    pub value6: Order3,
    /// : type 49's sums.
    pub type49: Type49Form,
    /// : the bump gradient's sum.
    pub bump: BumpSum,
    /// : the order of the bump gradient's three squares among themselves.
    pub bump_squares: Order3,
    /// : the sum gx + gy + gz + 1.5 of the value from a bump alone (terms 0 to 3).
    pub bump_value: Order4,
    /// : where the phase space's factor enters.
    pub phase_factor: PhaseFactor,
}

impl TextureOrders {
    /// The orders the released table-free vectors decide for each version, found by
    /// tests/texture_vectors.rs (`orders_the_vectors_decide`), which sets each in turn over
    /// noise-types, octave-methods, noise-space, filters and phase (675 cases a version): with
    /// these every case is the same, and for each order but one, no other value of it gives
    /// every case. Type 25's sum is the exception the entry names ("the other two give
    /// every case (Open between them)"): (x² + y²) + z² is kept (vector-provisional).
    pub fn of(version: RenderAs) -> TextureOrders {
        use super::space::SpaceOrders;
        use Order3::*;
        use ThreeFactor::*;
        let shared = TextureOrders {
            // : s·(trig·d) as 3.0 and 3.1; q1's and q2's terms in their written order.
            space: SpaceOrders { three_factor: LastPair, terms: [AbC, AbC] },
            type1: FirstPair,
            type19: OuterPair,
            type25: AbC,
            // : x' = (425 + 0.656x) − 0.115y; y' = 733 + (0.656y + 0.115x), and so for 5 to 7.
            method3: [AbC, BcA],
            method5: [AbC, BcA],
            method4: Method4Grouping::SumSampleFirst,
            // : s(t) = t·(t·(3 − 2t)).
            smoothstep: Smoothstep::Nested,
            // : ease(u) = u·(u·(3 − 2u)), as s (blends.jsonl's mode 13 decides it).
            ease: Smoothstep::Nested,
            // : value mode 6's sum a + (b − 0.5) (blends.jsonl decides it).
            value6: BcA,
            type49: Type49Form::OneLessSum(AbC),
            // : decided by bump-gradients.jsonl (tests/texture_vectors.rs).
            bump: BumpSum::Last,
            // : (y² + z²) + x² as 3.0 and 3.1,
            // (x² + y²) + z² as 5.0 (set below).
            bump_squares: BcA,
            // : ((gy + gz) + gx) + 1.5, alone of the 15 orders in giving every case of
            // bump-value.jsonl, in each version (tests/texture_vectors.rs).
            bump_value: Order4::Chain([1, 2, 0, 3]),
            // : on each entry of the formed map (bump-gradients.jsonl's bg0095 as 5.0
            // separates it; every other vector gives the same either way).
            phase_factor: PhaseFactor::OnEntries,
        };
        match version {
            RenderAs::Bryce30 => shared,
            RenderAs::Bryce31 => TextureOrders { type1: OuterPair, type19: LastPair, ..shared },
            RenderAs::Bryce50 => TextureOrders {
                bump_squares: AbC,
                space: SpaceOrders { three_factor: OuterPair, ..shared.space },
                type1: LastPair,
                type19: LastPair,
                ..shared
            },
        }
    }
}
