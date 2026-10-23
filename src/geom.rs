// SPDX-License-Identifier: MIT
//! Shared colour and vector types used by `shading` and `sky`.
//! (shared vocabulary)
//!
//! Every operation is a single IEEE operation per component, in the order written, with no
//! fused multiply-add. Bit-exact results depend on that order.

use std::ops::{Add, Mul, Sub};

/// A colour as three linear channels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Rgb = Rgb { r: 1.0, g: 1.0, b: 1.0 };

    pub const fn new(r: f64, g: f64, b: f64) -> Rgb {
        Rgb { r, g, b }
    }

    pub fn splat(v: f64) -> Rgb {
        Rgb { r: v, g: v, b: v }
    }

    pub fn scale(self, k: f64) -> Rgb {
        Rgb { r: self.r * k, g: self.g * k, b: self.b * k }
    }

    pub fn map(self, f: impl Fn(f64) -> f64) -> Rgb {
        Rgb { r: f(self.r), g: f(self.g), b: f(self.b) }
    }

    /// `self + (to - self) * t`, per channel, with `t` per channel.
    pub fn lerp_each(self, to: Rgb, t: Rgb) -> Rgb {
        Rgb {
            r: self.r + (to.r - self.r) * t.r,
            g: self.g + (to.g - self.g) * t.g,
            b: self.b + (to.b - self.b) * t.b,
        }
    }

    /// `self + (to - self) * t`.
    pub fn lerp(self, to: Rgb, t: f64) -> Rgb {
        self.lerp_each(to, Rgb::splat(t))
    }
}

impl Add for Rgb {
    type Output = Rgb;
    fn add(self, o: Rgb) -> Rgb {
        Rgb { r: self.r + o.r, g: self.g + o.g, b: self.b + o.b }
    }
}

impl Sub for Rgb {
    type Output = Rgb;
    fn sub(self, o: Rgb) -> Rgb {
        Rgb { r: self.r - o.r, g: self.g - o.g, b: self.b - o.b }
    }
}

/// Channel-by-channel product.
impl Mul for Rgb {
    type Output = Rgb;
    fn mul(self, o: Rgb) -> Rgb {
        Rgb { r: self.r * o.r, g: self.g * o.g, b: self.b * o.b }
    }
}

/// A point or direction in scene space.
pub type Vec3 = [f64; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// `a[0]*b[0] + a[1]*b[1] + a[2]*b[2]`, summed left to right.
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// `a / |a|`. A zero vector is returned unchanged.
pub fn normalize(a: Vec3) -> Vec3 {
    let len = length(a);
    if len == 0.0 { a } else { scale(a, 1.0 / len) }
}
