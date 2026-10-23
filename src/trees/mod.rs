// SPDX-License-Identifier: MIT
//! Trees: a Bryce 5 tree grown from its settings: its pieces of wood, its places for leaves,
//! its leaves, and the built-in leaf's shape and normals.
//!
//! testing-interface.md (kind `tree`: the tree's text)

pub mod arith;
mod leaves;
pub mod shape;
pub mod text;
mod wood;

pub use arith::Vec3;
pub use shape::{LeafMesh, ShapeTriangle};
pub use wood::DEEPEST_LEVEL;

/// The settings growth reads, by this crate's names; each names its setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthInputs {
    /// The deepest level of branching (read, not used: growth takes 25).
    pub deepest_level: u32,
    /// Bt: the most branches of the trunk at one height.
    pub trunk_branches: u32,
    /// Bb: branches at one height of a branch.
    pub branch_branches: u32,
    /// H1, H2: the range of the trunk's count of heights, in the document's order.
    pub trunk_heights: [u32; 2],
    /// n: leaves at each place.
    pub leaves_per_place: u32,
    /// A1 (thickest branch), A2 (thinnest), radians.
    pub angles: [f64; 2],
    /// T: the trunk's thickness.
    pub trunk_thickness: f64,
    /// E: the least thickness.
    pub least_thickness: f64,
    /// S1, S2: the range of a branch's share of its parent's radius.
    pub shares: [f64; 2],
    /// ρ: how far drawn numbers move turns and places.
    pub randomness: f64,
    /// p: the power that spreads the branching heights.
    pub power: f64,
    /// σ: the leaves' size.
    pub leaf_size: f64,
    /// The kind of the leaves' shape (3 pairs, 4 alternating, 5 spiral, 6 bunches, 7 conifer
    /// twigs; any other number gives no leaves).
    pub leaf_kind: u32,
    /// The turn between heights (read, not used: growth takes π/2, or 0 with conifer twigs).
    pub turn: f64,
    /// γ: gravity.
    pub gravity: f64,
    /// O0 to O3: (across, up) each.
    pub outline: [[f64; 2]; 4],
    /// s: the seed of the wood's run.
    pub seed: u32,
}

/// A starting state of the leaves' run: its sum v and its positions a, b, c, d.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafRun {
    pub sum: u32,
    pub positions: [u32; 4],
}

impl LeafRun {
    /// The run a seed starts: v = s, a = s mod 47, b = s mod 53, c = s mod 59, d = s mod 61.
    pub fn of_seed(s: u32) -> LeafRun {
        LeafRun { sum: s, positions: crate::numerics::Bryce5Rand::PERIODS.map(|p| s % p) }
    }
}

/// The leaf kinds' numbers.
pub mod kind {
    pub const PAIRS: u32 = 3;
    pub const ALTERNATING: u32 = 4;
    pub const SPIRAL: u32 = 5;
    pub const BUNCHES: u32 = 6;
    pub const CONIFER: u32 = 7;
}

/// A placement: x·A + y·B + z·C + T, kept as [A, B, C, T].
/// (A placement)
pub type Placement = [Vec3; 4];

/// A piece of wood (or of a stalk).,
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// The number of the piece it grows out of, or none.
    pub from: Option<u32>,
    pub placement: Placement,
}

/// A place for leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub point: Vec3,
    pub direction: Vec3,
    pub piece: Option<u32>,
}

/// A leaf: its placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    pub placement: Placement,
}

/// A grown tree: the wood's pieces then the stalks', the places, the leaves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GrownTree {
    pub pieces: Vec<Piece>,
    pub places: Vec<Place>,
    pub leaves: Vec<Leaf>,
}

/// Our own bounds (Limits: "an implementation may refuse a tree beyond a bound of its own,
/// saying so"). (Refusals and bounds)
pub mod bounds {
    pub const PIECES: usize = 1_000_000; // (bounds)
    pub const PLACES: usize = 1_000_000; // (bounds)
    pub const LEAVES: usize = 4_000_000; // (bounds)
    pub const STEPS: u64 = 200_000_000; // (bounds)
}

/// Why a tree is not grown.
#[derive(Clone, Debug, PartialEq)]
pub enum GrowthRefusal {
    /// Beyond one of our bounds (Limits).
    BeyondBound(&'static str),
    /// A branch whose step no longer moves t on: growth would not end.
    Unending,
}

impl std::fmt::Display for GrowthRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrowthRefusal::BeyondBound(what) => {
                write!(f, "the tree is beyond this program's own bound on {what} (TR-3 Limits)")
            }
            GrowthRefusal::Unending => {
                write!(f, "a branch's step no longer moves its parameter on below 1: growth would not end")
            }
        }
    }
}

impl std::error::Error for GrowthRefusal {}

/// Grow a tree: the wood from the seed's run , then the leaves from `leaf_run` .
/// `noise_bits_five` is the substrate's table.
pub fn grow(s: &GrowthInputs, leaf_run: LeafRun, noise_bits_five: &[u32; 64]) -> Result<GrownTree, GrowthRefusal> {
    let mut b = wood::grow_wood(s, noise_bits_five)?;
    leaves::grow_leaves(s, leaf_run, noise_bits_five, &mut b)?;
    Ok(b.finish())
}
