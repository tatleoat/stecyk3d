// SPDX-License-Identifier: MIT
//! The wash of a Bryce 5 sky with bit 0x1000000: the sky of a planet seen from inside its air,
//! laid over the colour of every ray of the tree, and the light of the sun at a place.
//!
//!
//! Notation as the entry's: every operation is binary64, rounded on its own, unless a rounding to
//! binary32 is written; the binary32 operations are those of `f32` (each result of two singles
//! rounded once, which is the exact sum or product of the widened singles rounded to binary32).
//! No fused multiply-add.

use crate::geom::Vec3;
use crate::numerics::{pow, rsqrt_as, sqrt_as};
use crate::textures::filters::exp_x87;
use crate::version::RenderAs;

/// The centre of the planet, in the frame the rays are traced in.
const K: Vec3 = [0.0, -500.0, 0.0];
/// The outer radius of the shell (its square is formed as 510 times 510).
const OUTER: f64 = 510.0;
/// The height of the shell's floor: the densities fall off with h - 500.
const FLOOR: f64 = 500.0;
/// The three rates of the first density, singles. (The wash: kappa 1 to 3; the third
/// is the single nearest 0.1961, not that nearest 0.196)
const KAPPA: [f32; 3] = [0.0196, 0.0686, 0.1961];
/// The single nearest 0.55.
const C: f32 = 0.55;
/// E, the double nearest 1/32767.
const E: f64 = 1.0 / 32767.0;

/// What the wash needs of the render: the generator's draws and the sun's shadow ray.
pub trait WashSite {
    /// The next number d of the generator , a whole number from 0 to 32767.
    fn draw(&mut self) -> f64;
    /// The amount of the shadow ray from the place `q` toward the sun, made, traced and averaged
    /// as make the sun's (its draws taken as a part of this call).
    /// (point 7a)
    fn sun_amount(&mut self, q: Vec3) -> Result<f64, crate::shading::OpenItem>;
}

/// The sun as the wash reads it. (Inputs: S; the colour L and intensity of the sun's
/// light part)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WashSun {
    /// S: the sun's own direction, of unit length, toward the sun, whatever its height.
    pub direction: Vec3,
    /// L: the colour of the sun's light part .
    pub light_colour: [f32; 3],
    /// The intensity of the sun's light part .
    pub light_intensity: f64,
}

/// X(x), the exponential as the x87 unit forms it.
fn x(v: f64) -> f64 {
    exp_x87(v)
}

/// rho(h).
fn rho(h: f64) -> f64 {
    x(-((h - FLOOR) * 0.833)) * 1.638
}

/// nu(h).
fn nu(h: f64) -> f64 {
    x(-((h - FLOOR) * 1.67)) * 1.638
}

/// G(p, q, s), the step from a density p to a density q over a step s.
fn step_g(p: f64, q: f64, s: f64) -> f64 {
    let v = 0.2 / ((((q - p) / s).abs() + 1.0) * q + 0.05);
    if v < 0.2 {
        0.2
    } else if v > 100.0 {
        100.0
    } else {
        v
    }
}

/// h(P) = R(a), a the sum of the squares of P - K ((y*y + z*z) + x*x: Accuracy).
fn height(p: Vec3) -> f64 {
    let (px, py, pz) = (p[0] - K[0], p[1] - K[1], p[2] - K[2]);
    sqrt_as(RenderAs::Bryce50, (py * py + pz * pz) + px * px)
}

/// B + V*t, component by component: Vk*t + Bk.
fn along(b: Vec3, v: Vec3, t: f64) -> Vec3 {
    [v[0] * t + b[0], v[1] * t + b[1], v[2] * t + b[2]]
}

/// The next step by the densities of the two last samples.
fn follow(r_before: f64, n_before: f64, r: f64, n: f64, s: f64) -> f64 {
    if (r - r_before).abs() <= (n - n_before).abs() {
        step_g(n_before, n, s)
    } else {
        step_g(r_before, r, s)
    }
}

/// The tint at a place q with a height h, added to the light just made.
fn tinted(light: [f32; 3], q: Vec3, h: f64, s: Vec3) -> [f32; 3] {
    let eps = (h - 500.0) - 1.5 * 0.3;
    if !(eps >= 0.0) {
        return light;
    }
    let eps2 = eps * 0.5;
    let (px, py, pz) = (q[0] - K[0], q[1] - K[1], q[2] - K[2]);
    let omega = rsqrt_as(RenderAs::Bryce50, (px * px + py * py) + pz * pz);
    let n = [omega * px, omega * py, omega * pz];
    let m = (n[2] * s[2] + n[1] * s[1]) + n[0] * s[0];
    let f = if m > 0.0 { m + 1.0 } else { pow(m + 1.0, 16.0) };
    let f = (f * eps2) * 1.5;
    [
        (f * 0.1 + f64::from(light[0])) as f32,
        (f * 0.2 + f64::from(light[1])) as f32,
        (f64::from(light[2]) + f) as f32,
    ]
}

/// The light at a place `q` for a ray (point 7), three singles: through the air from `q` to the
/// outer sphere along the sun's direction, cut by the shadow ray where `shadow` (point 7a; the
/// sun's light at a lit point has none). The tint is the caller's.
pub fn light_at<W: WashSite>(site: &mut W, sun: &WashSun, q: Vec3, shadow: bool) -> Result<[f32; 3], crate::shading::OpenItem> {
    if shadow && site.sun_amount(q)? == 0.0 {
        return Ok([0.0; 3]);
    }
    let s = sun.direction;
    // b.
    let v = [K[0] - q[0], K[1] - q[1], K[2] - q[2]];
    let b = (s[1] * v[1] + s[2] * v[2]) + v[0] * s[0];
    let g = (OUTER * OUTER - ((v[1] * v[1] + v[2] * v[2]) + v[0] * v[0])) + b * b;
    if !(g > 0.0) {
        return Ok([0.0; 3]);
    }
    let root = sqrt_as(RenderAs::Bryce50, g);
    let t1 = root + b;
    let mut t0 = b - root;
    if t0 < 0.0 {
        t0 = 0.0;
    }
    // c.
    let mut step = step_g(rho(height(along(q, s, t0))), rho(height(along(q, s, 0.2 + t0))), 0.2);
    if t1 - t0 < step {
        step = t1 - t0;
    }
    let mut t = (site.draw() * E) * step + t0;
    let at = along(q, s, t);
    let (mut r, mut n) = (rho(height(at)), nu(height(at)));
    let mut alpha = [1.0f32; 3];
    // d.
    while t < t1 {
        let (r_before, n_before) = (r, n);
        let u = t - (site.draw() * E) * step;
        let h = height(along(q, s, u));
        r = rho(h);
        n = nu(h);
        let tau = ((r_before + r) * step) * 0.5;
        let upsilon = ((n_before + n) * step) * 0.5;
        let beta = x(-(upsilon * 0.1)) as f32;
        for j in 0..3 {
            let a = x(-(f64::from(KAPPA[j]) * tau)) as f32;
            alpha[j] *= (1.0 - (1.0 - a)) * (1.0 - (1.0 - beta));
        }
        step = follow(r_before, n_before, r, n, step);
        if t1 - t < step {
            step = t1 - t;
        }
        t += step;
    }
    // e.
    Ok([0, 1, 2].map(|j| alpha[j] * sun.light_colour[j]))
}

/// The light at a place with the tint at a height h (points 7 and 8).
fn lit_tinted<W: WashSite>(site: &mut W, sun: &WashSun, q: Vec3, h: f64) -> Result<[f32; 3], crate::shading::OpenItem> {
    let light = light_at(site, sun, q, true)?;
    Ok(tinted(light, q, h, sun.direction))
}

/// The sun's colour at a lit point of a surface (point 9): the light at the point without the
/// shadow ray and without the tint.
pub fn sun_at_lit_point<W: WashSite>(site: &mut W, sun: &WashSun, p: Vec3) -> Result<[f32; 3], crate::shading::OpenItem> {
    light_at(site, sun, p, false)
}

/// What the wash adds to a ray and how much of the ray's own colour it lets through (point 6:
/// the colour becomes `added + through * colour`, per channel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WashEffect {
    pub added: [f32; 3],
    pub through: [f32; 3],
}

impl WashEffect {
    /// The colour after the wash.
    pub fn over(&self, colour: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|j| self.added[j] + (self.through[j] * colour[j]))
    }
}

/// The colour of a ray after the wash: the ray's origin `o`, its direction `d` as sent, the
/// parameter `hit` at which it meets a surface (if it does) and its colour (three singles).
pub fn wash_ray<W: WashSite>(
    site: &mut W,
    sun: &WashSun,
    o: Vec3,
    d: Vec3,
    hit: Option<f64>,
    colour: [f32; 3],
) -> Result<[f32; 3], crate::shading::OpenItem> {
    Ok(wash_effect(site, sun, o, d, hit)?.map_or(colour, |e| e.over(colour)))
}

/// The march of the wash (points 1 to 5), with its draws, apart from the colour it is laid over
/// (point 6): `None` where the ray does not meet the outer sphere (no draw is taken then). The
/// effect can be taken before the colour is known and laid later (a blended surface's
/// wash takes its draws before the ray straight on is sent).
pub fn wash_effect<W: WashSite>(
    site: &mut W,
    sun: &WashSun,
    o: Vec3,
    d: Vec3,
    hit: Option<f64>,
) -> Result<Option<WashEffect>, crate::shading::OpenItem> {
    let s = sun.direction;
    // 1. The sphere.
    let v = [K[0] - o[0], K[1] - o[1], K[2] - o[2]];
    let b = (v[0] * d[0] + d[2] * v[2]) + d[1] * v[1];
    let g = (OUTER * OUTER - ((v[0] * v[0] + v[2] * v[2]) + v[1] * v[1])) + b * b;
    if !(g > 0.0) {
        return Ok(None);
    }
    let root = sqrt_as(RenderAs::Bryce50, g);
    let t1 = root + b;
    let mut t0 = b - root;
    if t0 < 0.0 {
        t0 = 0.0;
    }
    // 2. The phases.
    let mu = (s[0] * d[0] + s[2] * d[2]) + s[1] * d[1];
    let m = mu * mu;
    let phi = m * 0.5 + 0.75;
    let mut w = pow(m, 64.0) * 10.0;
    if mu < 0.0 {
        w *= 0.1;
    }
    let psi = ((phi + 1.0) + (phi + 1.0)) + w;
    let (phi, psi) = (phi as f32, psi as f32);
    // 3. The end.
    let e = match hit {
        Some(p) if p != 0.0 && p < t1 => p,
        _ => t1,
    };
    // 4. The first step.
    let mut t = t0;
    let mut step = step_g(rho(height(along(o, d, t))), rho(height(along(o, d, 0.2 + t))), 0.2);
    if e - t < step {
        step = e - t;
    }
    t = (site.draw() * E) * step + t;
    let mut p = along(o, d, t);
    let mut h = height(p);
    let mut r = rho(h);
    let mut n = nu(h);
    let mut lam = lit_tinted(site, sun, p, h)?;
    let mut lam_before = lam;
    let mut a = [0.0f32; 3];
    let mut tr = [1.0f32; 3];
    let mut k = 0u32;
    let mut last = p;
    // 5. The samples.
    while t < e {
        // a.
        let (r_before, n_before) = (r, n);
        let u = t - (site.draw() * E) * step;
        p = along(o, d, u);
        h = height(p);
        r = rho(h);
        n = nu(h);
        // b.
        if k % 5 == 0 {
            lam_before = lam;
            let vv = site.draw() * E;
            let q = [
                (p[0] - last[0]) * vv + last[0],
                (p[1] - last[1]) * vv + last[1],
                (p[2] - last[2]) * vv + last[2],
            ];
            lam = lit_tinted(site, sun, q, h)?;
            last = p;
        }
        k += 1;
        // c.
        let tau = ((r_before + r) * step) * 0.5;
        let upsilon = ((n_before + n) * step) * 0.5;
        let beta = x(-(upsilon * 0.1)) as f32;
        for j in 0..3 {
            let mj = (lam[j] + lam_before[j]) * 0.5;
            let aj = x(-(f64::from(KAPPA[j]) * tau)) as f32;
            let sj = (((phi * (1.0 - aj)) * mj) * C) + (((psi * (1.0 - beta)) * mj) * C);
            a[j] = (tr[j] * sj) + a[j];
            tr[j] = aj * tr[j];
        }
        // d.
        step = if t == 0.0 {
            0.2 * 0.001
        } else if t > 0.0 && t < 0.2 {
            t * 0.2
        } else {
            follow(r_before, n_before, r, n, step)
        };
        if e - t < step {
            step = e - t;
        }
        t += step;
    }
    // 6.
    Ok(Some(WashEffect { added: a, through: tr }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(u32);
    impl WashSite for Fixed {
        fn draw(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(1103515245).wrapping_add(12345);
            f64::from((self.0 >> 16) & 0x7fff)
        }
        fn sun_amount(&mut self, _q: Vec3) -> Result<f64, crate::shading::OpenItem> {
            Ok(1.0)
        }
    }

    fn sun() -> WashSun {
        WashSun { direction: [0.0, 1.0, 0.0], light_colour: [1.0; 3], light_intensity: 1.0 }
    }

    /// point 1: a ray that does not cross the outer sphere keeps its colour and draws
    /// nothing (the camera raised by 20 units).
    #[test]
    fn a_ray_outside_the_sphere_keeps_its_colour() {
        let mut site = Fixed(1);
        let c = [0.25, 0.5, 0.75];
        // a line that misses the sphere of radius 510 about (0, -500, 0): g is not above 0
        let out = wash_ray(&mut site, &sun(), [0.0, 20.0, -30.0], [1.0, 0.0, 0.0], None, c).unwrap();
        assert_eq!(out, c);
        assert_eq!(site.0, 1, "no draw");
    }

    /// point 6: the colour is dimmed and added to; black over black adds light only.
    #[test]
    fn a_ray_in_the_air_is_changed() {
        let mut site = Fixed(1);
        let out = wash_ray(&mut site, &sun(), [0.0, 0.0, -30.0], [0.0, 0.7, 0.714_142_8], None, [0.0; 3]).unwrap();
        assert!(out.iter().all(|&v| v > 0.0), "{out:?}");
    }
}
