// SPDX-License-Identifier: MIT
//! Bryce 5's soft shadows: a light with a radius sends its shadow rays toward points of a disc
//! about the light, which rays a shadow takes and how many draws of the generator they cost,
//! and the mean of the rays' shadows.
//!
//! ("Which rays", "The disc",
//! "The point of the disc" for the n_s² rays and for a numbered ray, "A ray given up", "The
//! shadow", "The draws"), points 1 to 3 (the cell, the disc point of a numbered ray,
//! the turn T),,
//! (Bryce 5's soft shadows: `shading::soft`)
//!
//! Pure functions of numbers: the draws are arguments (the first drawn first); when they are
//! drawn and which rays are traced is the render's (`render::lit`).

use crate::geom::Vec3;
use crate::numerics::{rsqrt_as, sqrt_as, x87};
use crate::sampling::premium::{self, CellPoint};
use crate::shading::lighting::{Shadow, SumOrder};
use crate::shading::media::{BlurOrders, frame_turn};
use crate::version::RenderAs;

/// The rendering version these rules exist in.
const FIVE: RenderAs = RenderAs::Bryce50;

/// n_s, the shadow grid of the render of a frame (64 rays a light).
pub const FRAME_GRID: u32 = 8;

/// The scene's switch bit that turns a numbered ray toward its disc point (soft shadows in the
/// Premium quality)., point 2 (0x40 [format])
pub const SWITCH_SOFT_SHADOWS: u32 = 0x40;

/// The distance of the sun's disc.
const SUN_DISC_DISTANCE: f64 = 10_000.0;
/// The factor of the sun's disc radius to its rho.
const SUN_DISC_FACTOR: f64 = 400.0;

/// E: the binary64 number nearest 1/32767.
const E: f64 = 1.0 / 32767.0;
/// 2π: the binary64 number nearest it.
const TWO_PI: f64 = std::f64::consts::TAU;

/// The arrangement of the disc's turn: the order of a² + b² + D² under Q (x = a², y = b²,
/// z = D²) and of T's products and sums. The one ` ` selects
/// (tests/shading_lights_five.rs tries every candidate). (Accuracy: left to the
/// vectors, "the cone's arrangement, one of the two that describes")
pub const SOFT_ORDERS: BlurOrders = premium::CONE_ORDERS;

/// The order of the three-term sum w·n of the test for a ray given up, the one
/// `lights-five.jsonl` selects (its 23 normals found by search tell the three apart).
pub const GIVE_UP_ORDER: SumOrder = SumOrder::YzX;

/// What the light is, as the disc needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiscLight {
    /// A radial light or a light with a beam: ρ, and ℓ, the shadow ray's reach.
    Placed { radius: f64, reach: f64 },
    /// The sun: its radius rho (the disc: 10,000 away, radius 400*rho).
    Sun { radius: f64 },
}

/// The disc the rays are aimed at: square to L, centred on the line along L at the distance
/// D from the ray's origin, of radius ρ_d.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    pub distance: f64,
    pub radius: f64,
}

/// What a shadow of a light takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowRays {
    /// One ray,'s, no draw.
    One,
    /// Case 1: n_s² rays aimed at jittered points of the disc, two draws each.
    Grid { grid: u32, disc: Disc },
    /// Case 2: one ray turned toward the disc point the sample's number gives, two draws.
    Numbered { sample: u32, rays_a_side: u32, disc: Disc },
}

impl ShadowRays {
    /// The rays sent and the draws taken (where no ray is given up, every ray is sent).
    pub fn counts(&self) -> (u32, u32) {
        match self {
            ShadowRays::One => (1, 0),
            ShadowRays::Grid { grid, .. } => (grid * grid, 2 * grid * grid),
            ShadowRays::Numbered { .. } => (1, 2),
        }
    }
}

/// The disc of a light and the light's own radius: D = ℓ and ρ for a light with a place, D = 10,000
/// and 400·ρ for the sun. The disc is `None` for a radius that is not a number (case 2 counts it as
/// 0); a radius below 0 is kept as it stands (case 2 uses it so). (The disc; Which rays,
/// case 3)
fn disc_of(light: DiscLight) -> (f64, Option<Disc>) {
    match light {
        DiscLight::Placed { radius, reach } => (radius, (!radius.is_nan()).then_some(Disc { distance: reach, radius })),
        DiscLight::Sun { radius } => {
            let disc = SUN_DISC_FACTOR * radius;
            (radius, (!disc.is_nan()).then_some(Disc { distance: SUN_DISC_DISTANCE, radius: disc }))
        }
    }
}

/// Which rays the shadow of `light` takes as `version`, for a shadow ray that carries the
/// Premium sample number `sample` (of `rays_a_side` a side) or none, with the scene's switch
/// word `switches` and the shadow grid `grid`. As 3.0 and 3.1 a light always sends one ray.
pub fn shadow_rays(
    version: RenderAs,
    light: DiscLight,
    sample: Option<(u32, u32)>,
    switches: u32,
    grid: u32,
) -> ShadowRays {
    if version != FIVE {
        return ShadowRays::One;
    }
    let (rho, disc) = disc_of(light);
    match sample {
        // Case 1: n_s² rays only where ρ is above 0 (a ρ below 0 or not a number: one ray).
        None => match disc {
            Some(disc) if rho > 0.0 => ShadowRays::Grid { grid, disc },
            _ => ShadowRays::One,
        },
        // Case 2: a disc radius not a number counts as 0; one below 0 is used as it stands.
        Some((s, n)) => match disc {
            Some(disc) if switches & SWITCH_SOFT_SHADOWS != 0 && disc.radius != 0.0 && disc.distance != 0.0 => {
                ShadowRays::Numbered { sample: s, rays_a_side: n, disc }
            }
            _ => ShadowRays::One,
        },
    }
}

/// The point (a, b) of the disc of radius `radius` for ray `k` of the n_s² rays (n_s = `grid`),
/// from its two draws (d1 drawn first). Ray k takes the sector k mod n_s and the ring k div n_s,
/// jittered within its cell; neither coordinate is rounded to binary32.
pub fn grid_point(grid: u32, k: u32, draws: [u16; 2], radius: f64) -> [f64; 2] {
    let w = 1.0 / f64::from(grid);
    let j1 = f64::from(draws[0]) * E - 0.5;
    let j2 = f64::from(draws[1]) * E - 0.5;
    let theta = ((f64::from(k % grid) + 0.5) * w + j1 * w) * TWO_PI;
    let r = sqrt_as(FIVE, (f64::from(k / grid) + 0.5) * w + j2 * w);
    let a = x87::cos_ext(theta).mul_to_f64(r) * radius;
    let b = x87::sin_ext(theta).mul_to_f64(r) * radius;
    [a, b]
}

/// The point of the disc of a numbered ray: points 1 and 2 (the cell of the sample's
/// number among the n² Premium samples, moved by the two draws; a and b rounded to binary32).
/// , points 1 and 2
pub fn numbered_point(sample: u32, rays_a_side: u32, draws: [u16; 2], radius: f64) -> [f32; 2] {
    let cell: CellPoint = premium::cell_point(sample, rays_a_side, draws);
    premium::disc_point(radius, cell)
}

/// L turned toward the point (a, b) of the disc at the distance D: T(L; s·a, s·b, s·D),
/// s = Q(a² + b² + D²); not brought to unit length.
pub fn turn(l: Vec3, point: [f64; 2], distance: f64, orders: BlurOrders) -> Vec3 {
    let [a, b] = point;
    let s = rsqrt_as(FIVE, orders.norm.dot([a, b, distance], [a, b, distance]));
    frame_turn(l, [s * a, s * b, s * distance], orders)
}

/// Whether a ray of case 1 cast from a surface is given up: its turned direction w has w·n
/// below 0 (n the shading normal as leaves it).
pub fn given_up(w: Vec3, n: Vec3, order: SumOrder) -> bool {
    order.dot(w, n) < 0.0
}

/// The mean of the n_s² rays' shadows, summed in the order of k: each colour channel
/// ⟨σ + s_σ⟩, the amount α + s_α; a ray given up counts as ((1, 1, 1), 1). Then, with
/// m = 1/n_s², (⟨s_σ·m⟩, m·s_α).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowMean {
    colour: [f32; 3],
    amount: f64,
}

/// What a ray given up counts as: a share (1, 1, 1) of amount 1.
pub const GIVEN_UP: Shadow = Shadow { colour: [1.0; 3], amount: 1.0 };

impl Default for ShadowMean {
    fn default() -> Self {
        ShadowMean::new()
    }
}

impl ShadowMean {
    /// The sums at 0.
    pub fn new() -> ShadowMean {
        ShadowMean { colour: [0.0; 3], amount: 0.0 }
    }

    /// Ray k's shadow added (`GIVEN_UP` for a ray given up).
    pub fn add(&mut self, s: Shadow) {
        for k in 0..3 {
            self.colour[k] = (f64::from(s.colour[k]) + f64::from(self.colour[k])) as f32;
        }
        self.amount += s.amount; // α + s_α, one binary64 sum
    }

    /// The mean over the n_s² rays of a grid of `grid` a side.
    pub fn mean(&self, grid: u32) -> Shadow {
        let m = 1.0 / f64::from(grid * grid);
        Shadow { colour: self.colour.map(|c| (f64::from(c) * m) as f32), amount: m * self.amount }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : the cells' middles (draws at 16383.5 are not whole; 16383 and 16384 straddle).
    #[test]
    fn grid_points_lie_in_their_cells() {
        let n = FRAME_GRID;
        for k in 0..n * n {
            for d in [[0u16, 0u16], [16384, 16384], [32767, 32767]] {
                let [a, b] = grid_point(n, k, d, 2.0);
                let r = (a * a + b * b).sqrt() / 2.0;
                let ring = f64::from(k / n);
                let w = 1.0 / f64::from(n);
                assert!(r * r >= ring * w - 1e-12 && r * r <= (ring + 1.0) * w + 1e-12, "k {k}: r² {}", r * r);
            }
        }
    }

    /// : the plan of `lights-five.jsonl`'s rows, in words.
    #[test]
    fn which_rays() {
        let placed = |radius, reach| DiscLight::Placed { radius, reach };
        assert_eq!(shadow_rays(RenderAs::Bryce31, placed(2.0, 12.0), None, 0, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, placed(0.0, 12.0), None, 0, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, placed(0.5, 0.0), None, 0, 8).counts(), (64, 128));
        assert_eq!(shadow_rays(FIVE, placed(0.5, 12.0), Some((0, 3)), 0, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, placed(0.5, 12.0), Some((0, 3)), 0x40, 8).counts(), (1, 2));
        assert_eq!(shadow_rays(FIVE, placed(0.5, 0.0), Some((0, 3)), 0x40, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, DiscLight::Sun { radius: 0.0 }, None, 0x40, 8), ShadowRays::One);
        let sun = shadow_rays(FIVE, DiscLight::Sun { radius: 1.5 }, None, 0, 8);
        assert_eq!(sun, ShadowRays::Grid { grid: 8, disc: Disc { distance: 10_000.0, radius: 600.0 } });
        // Case 3 : a radius below 0 or not a number gives one ray in case 1; in case 2
        // not a number counts as 0 and below 0 is used as it stands.
        assert_eq!(shadow_rays(FIVE, placed(f64::NAN, 1.0), None, 0, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, placed(-2.0, 1.0), None, 0, 8), ShadowRays::One);
        assert_eq!(shadow_rays(FIVE, placed(f64::NAN, 1.0), Some((0, 3)), 0x40, 8), ShadowRays::One);
        assert_eq!(
            shadow_rays(FIVE, placed(-2.0, 1.0), Some((0, 3)), 0x40, 8),
            ShadowRays::Numbered { sample: 0, rays_a_side: 3, disc: Disc { distance: 1.0, radius: -2.0 } }
        );
    }
}
