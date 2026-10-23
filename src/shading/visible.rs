// SPDX-License-Identifier: MIT
//! The Light shading mode of a light's own surface, and the glow of a visible light.
//!
//! Pure functions: the quantities a Light-mode material gives, and
//! g, the glow. How the render queues the start ray and the blended ray is `render::lit`.

use crate::numerics::{exp2_as, pow, rsqrt_as, sqrt_as};
use crate::version::RenderAs;

/// The Light mode's bit in a material's flag word.
pub const LIGHT_MODE: u32 = 0x4;
/// The volume bits: a material with one is a volume material, whose path decides.
pub const VOLUME_BITS: u32 = 0x100 | 0x200 | 0x400;

/// The cone, cube, cylinder or pyramid of a light's own object: g whatever the ray.
pub const CONE_GLOW: f64 = 0.7;

/// The constants of the glow's law.
const GLOW_SCALE: f64 = 6.0;
/// log2 e as the entry gives it.
const LOG2_E: f64 = 1.4426950408889634;

/// The channels of a material in the Light mode that the entry reads, as gives them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightModeInput {
    /// The flag word .
    pub flags: u32,
    /// Transparency t (a double).
    pub transparency: f64,
    /// The glow g (3.1 and 5.0 only; ignored as 3.0).
    pub glow: f64,
    /// The Diffuse colour D, which is also the Ambient colour of the surface.
    pub diffuse: [f32; 3],
}

/// What makes of the surface. The Diffuse colour is unchanged and not repeated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightModeSurface {
    /// Diffusion, Specularity, Reflection and Metallicity are 0 in every version.
    pub diffusion: f64,
    pub specularity: f64,
    pub reflection: f64,
    pub metallicity: f64,
    pub ambience: f64,
    /// The Ambient colour: the Diffuse colour D (the material's own is not read).
    pub ambient_colour: [f32; 3],
    /// The flag word the surface is given whole.
    pub flags: u32,
    /// The surface takes no shadow (bit 0x20 of the replaced word).
    pub no_shadow: bool,
    /// Transparency t': the blended share V as 3.1 and 5.0.
    pub transparency: f64,
    /// 3.1 and 5.0: the surface is a blended one; as 3.0 it is not (the material's own bit 0x8
    /// at 3.0 is Open and not read here).
    pub blended: bool,
    /// The sum of starts from the colour of the start ray (3.1 and 5.0).
    pub adds_to_behind: bool,
    /// The angle law is off, and the refraction value counts as 0 for the tint as 3.0.
    pub angle_law_off: bool,
    /// Shadow rays pass the surface (bit 0x80 of the replaced word, every version).
    pub shadow_rays_pass: bool,
}

/// Whether a flag word is a Light-mode surface that this entry shades (bit 0x4, no volume
/// bit).
pub fn is_light_mode(flags: u32) -> bool {
    flags & LIGHT_MODE != 0 && flags & VOLUME_BITS == 0
}

/// The surface of a Light-mode material for `version`; `None` where the entry does not apply
/// (no bit 0x4, or a volume bit).
pub fn light_mode_surface(version: RenderAs, input: &LightModeInput) -> Option<LightModeSurface> {
    if !is_light_mode(input.flags) {
        return None;
    }
    let t = input.transparency;
    let (ambience, transparency, blended) = match version {
        // : 0.3 - t*0.3 and 0.7 - t*(-0.3), each in binary64
        RenderAs::Bryce30 => (0.3 - t * 0.3, 0.7 - t * (-0.3), false),
        RenderAs::Bryce31 | RenderAs::Bryce50 => {
            // : p = t^2 by, V = 1 - (1 - p)*g,
            // Ambience 0.4 - V*0.4
            let p = pow(t, 2.0);
            let v = 1.0 - (1.0 - p) * input.glow;
            (0.4 - v * 0.4, v, true)
        }
    };
    let five_or_31 = version != RenderAs::Bryce30;
    Some(LightModeSurface {
        diffusion: 0.0,
        specularity: 0.0,
        reflection: 0.0,
        metallicity: 0.0,
        ambience,
        ambient_colour: input.diffuse,
        // : 0x00a0 as 3.0, 0x20a8 as 3.1 and 5.0
        flags: if five_or_31 { 0x20a8 } else { 0x00a0 },
        no_shadow: true,
        transparency,
        blended,
        adds_to_behind: five_or_31,
        angle_law_off: true,
        shadow_rays_pass: true,
    })
}

/// g for a radial light's sphere, from the ray (origin `o`, direction `d` of any length) in the
/// sphere's own space. Not defined as 3.0 (no glow there).
pub fn sphere_glow(version: RenderAs, o: [f64; 3], d: [f64; 3]) -> f64 {
    // |d|^2 = (dy^2 + dz^2) + dx^2; u = d times the reciprocal square root.
    let len2 = (d[1] * d[1] + d[2] * d[2]) + d[0] * d[0];
    let q = rsqrt_as(version, len2);
    let u = [d[0] * q, d[1] * q, d[2] * q];
    let n = [-o[0], -o[1], -o[2]];
    let a = (u[0] * n[0] + u[1] * n[1]) + u[2] * n[2];
    let w = 1.0 - (((n[0] * n[0] + n[1] * n[1]) + n[2] * n[2]) - a * a);
    if w >= 0.0 {
        exp2_as(version, (sqrt_as(version, w) * GLOW_SCALE - GLOW_SCALE) * LOG2_E)
    } else {
        1.0
    }
}
