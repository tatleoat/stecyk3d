// SPDX-License-Identifier: MIT
//! Light in the air: the light the lights scatter toward the eye along a camera ray.
//!
//! (the shadow ray counting
//! surfaces from 0.0001 (the tracer's, render/lit.rs), and "As 5.0": every
//! ray of the tree, the step by depth, the path so far, the hold at 1, a sun that falls off);
//! (Light in the air; Light in the air revised; Light in the air along
//! every ray)
//!
//! Every binary64 operation is one Rust `f64` operation in the order the entry writes it.

use crate::geom::Vec3;
use crate::shading::lighting::{
    Bryce5Light, LightPlace, LightSource, SMALL, Shadow, SumOrder, falloff, light_direction_ordered, light_switch,
};
use crate::version::RenderAs;

/// The sky switch bit for 500 places.
pub const LIT_AIR_COARSE: u32 = 0x80_0000;
/// The sky switch bit for 1000 places.
pub const LIT_AIR_FINE: u32 = 0x100_0000;
/// As 5.0, the plain sky's bit keeps the light in the air off.
pub const LIT_AIR_PLAIN_SKY: u32 = 0x800;
const PLAIN_SKY: u32 = LIT_AIR_PLAIN_SKY;

const PLACES_COARSE: u32 = 500;
const PLACES_FINE: u32 = 1000;
/// How far from the eye the light is gathered.
const REACH: f64 = 64.0;
/// The factor on lights other than the sun.
const LAMP_FACTOR: f64 = 2.4;
/// The factor in the weight.
const WEIGHT_FACTOR: f64 = 1.2;

/// One ray of the tree, as the gathering reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRay {
    /// E: the point the ray's path started from (a camera ray's eye; a further ray's origin; a
    /// continued ray's path is its parent's).
    pub start: Vec3,
    /// D as the ray was sent.
    pub direction: Vec3,
    /// m: 0 for a camera ray.
    pub depth: u32,
    /// l: the length of the path already behind the ray (0 except for a continued ray).
    pub before: f64,
    /// d: where the ray meets a surface, counted from its own origin.
    pub hit: Option<f64>,
    /// Whether the ray travels in the medium of a solid.
    pub in_solid: bool,
}

impl PathRay {
    /// A camera ray in the open from `eye` along `direction`.
    pub fn camera(eye: Vec3, direction: Vec3, hit: Option<f64>) -> PathRay {
        PathRay { start: eye, direction, depth: 0, before: 0.0, hit, in_solid: false }
    }
}

/// What a render as 5.0 reads beside the sky: the scene's shadow bit ('s bit 0x8) and
/// the sky part's shadow setting s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitAirScene {
    pub shadows: bool,
    pub shadow_setting: f64,
    /// The place of the sun's object (the translation of its transform), from which a sun that
    /// falls off is measured.
    pub sun_position: Vec3,
}

/// The numbers of the light in the air fixed for a render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitAir {
    version: RenderAs,
    /// n.
    places: u32,
    /// δ = 64/n.
    step: f64,
    /// 2A·1.2.
    weight: f64,
    scene: LitAirScene,
}

impl LitAir {
    /// `None` where the light in the air does not act: as 3.0 and 3.1 neither bit 0x800000
    /// nor 0x1000000; as 5.0 also with bit 0x800.
    pub fn new(version: RenderAs, sky_switches: u32, amount: f64, scene: LitAirScene) -> Option<LitAir> {
        if version == RenderAs::Bryce50 && sky_switches & PLAIN_SKY != 0 {
            return None;
        }
        let places = if sky_switches & LIT_AIR_FINE != 0 {
            PLACES_FINE
        } else if sky_switches & LIT_AIR_COARSE != 0 {
            PLACES_COARSE
        } else {
            return None;
        };
        Some(Self::with_places(version, places, amount, scene))
    }

    fn with_places(version: RenderAs, places: u32, amount: f64, scene: LitAirScene) -> LitAir {
        LitAir { version, places, step: REACH / f64::from(places), weight: 2.0 * amount * WEIGHT_FACTOR, scene }
    }

    /// The light in the air of a Bryce 5 sky (a sky part of version 7) as 5.0: it acts when W
    /// has bit 0x800000 (not bit 0x1000000 alone) and not bit 0x800, with the sky part's own `air_steps` as
    /// the number of places n (500 in every observed sky). `None` where it does not act or
    /// `air_steps` is 0 (nothing is added there).
    pub fn of_bryce5_sky(sky_switches: u32, amount: f64, air_steps: u32, scene: LitAirScene) -> Option<LitAir> {
        // Bit 0x1000000 alone gathers no lit air, and air_steps 0 adds nothing (a stated
        // freedom, V = 0).
        if sky_switches & PLAIN_SKY != 0 || sky_switches & LIT_AIR_COARSE == 0 || air_steps == 0 {
            return None;
        }
        Some(Self::with_places(RenderAs::Bryce50, air_steps, amount, scene))
    }

    /// V for a camera ray from `eye` along the unit `direction`, meeting a surface at
    /// `surface` (its t) if it does. `shadow(light, p, toward the light, reach)` is's
    /// (sigma, alpha) for a shadow ray leaving p itself and counting surfaces from 0.0001.
    /// (The places, What each light gives at a place, The colour added, As 5.0)
    pub fn gather(
        &self,
        eye: Vec3,
        direction: Vec3,
        surface: Option<f64>,
        lights: &[LightSource],
        shadow: &mut dyn FnMut(usize, Vec3, Vec3, f64) -> Shadow,
    ) -> [f32; 3] {
        self.gather_with(eye, direction, surface, lights, &mut |_, _| Some(1.0), shadow)
    }

    /// As `gather`, with the beam's factor of a spot or parallel light taken at every place:
    /// `beam(light, p)` is the factor gives at p (1 for a light with no beam), `None`
    /// outside the beam, where the light adds nothing. The light's intensity at p is its own
    /// times the factor; the rest of acts on that intensity.
    pub fn gather_with(
        &self,
        eye: Vec3,
        direction: Vec3,
        surface: Option<f64>,
        lights: &[LightSource],
        beam: &mut dyn FnMut(usize, Vec3) -> Option<f64>,
        shadow: &mut dyn FnMut(usize, Vec3, Vec3, f64) -> Shadow,
    ) -> [f32; 3] {
        self.gather_path(&PathRay::camera(eye, direction, surface), lights, &[], beam, shadow)
            .unwrap_or([0.0; 3])
    }

    /// Whether a ray gathers at all: not in a solid's medium as 3.0 and 3.1.
    pub fn gathers(&self, ray: &PathRay) -> bool {
        self.version == RenderAs::Bryce50 || !ray.in_solid
    }

    /// V along any ray of the tree, or `None` where the ray gathers nothing (as 3.0 and 3.1 in
    /// a solid's medium).
    pub fn gather_path(
        &self,
        ray: &PathRay,
        lights: &[LightSource],
        bryce5: &[Bryce5Light],
        beam: &mut dyn FnMut(usize, Vec3) -> Option<f64>,
        shadow: &mut dyn FnMut(usize, Vec3, Vec3, f64) -> Shadow,
    ) -> Option<[f32; 3]> {
        if !self.gathers(ray) {
            return None;
        }
        let five = self.version == RenderAs::Bryce50;
        let step = if five {
            self.step * f64::from(ray.depth + 1)
        } else if ray.depth > 1 {
            f64::from(ray.depth) * self.step
        } else {
            self.step
        };
        let before = if five { 0.0 } else { ray.before };
        let last = ray.hit.map(|d| before + d);
        let mut sum = [0.0f64; 3];
        let mut t = (before - step) + step;
        while !(t > REACH) && last.is_none_or(|d| !(t > d)) {
            let p: Vec3 = std::array::from_fn(|k| ray.direction[k] * t + ray.start[k]);
            let w = (1.0 - t / REACH) * self.weight;
            for (index, light) in lights.iter().enumerate() {
                if light.switches & light_switch::ON == 0 {
                    continue;
                }
                let Some(phi) = beam(index, p) else { continue };
                let intensity = if phi == 1.0 { light.intensity } else { light.intensity * phi };
                let b5 = bryce5.get(index).copied().unwrap_or(Bryce5Light::NONE);
                let given = match self.version {
                    RenderAs::Bryce50 => self.light_five(index, light, &b5, intensity, p, &mut sum, shadow),
                    _ => self.light_three(index, light, intensity, p, shadow),
                };
                if let Some((c, i)) = given {
                    for k in 0..3 {
                        sum[k] += (f64::from(c[k]) * i) * w;
                    }
                }
            }
            t += step;
        }
        let two_n = 2.0 * f64::from(self.places);
        Some(sum.map(|s| {
            let q = s / two_n;
            (if q > 1.0 { 1.0 } else { q }) as f32
        }))
    }

    /// I after the falloff (I/r, or I/r² with bit 0x80000000), unless bit 0x400.
    fn fallen_off(&self, light: &LightSource, b5: &Bryce5Light, p: Vec3, i: f64) -> (f64, Vec3, f64) {
        let order = if self.version == RenderAs::Bryce50 { SumOrder::XzY } else { SumOrder::YzX };
        let (toward, r, reach) = match light.place {
            LightPlace::Sun { direction } => {
                let position = self.scene.sun_position;
                let (_, r, _) = light_direction_ordered(self.version, LightPlace::Radial { position }, p, order);
                (direction, r, f64::INFINITY)
            }
            _ => light_direction_ordered(self.version, light.place, p, order),
        };
        let mut i = i;
        if light.switches & light_switch::NO_FALL_OFF == 0 {
            if self.version == RenderAs::Bryce50 && light.switches & light_switch::RANGE_FALL_OFF != 0 {
                // (As 5.0, Bryce 5's lights: the falloff over a range, a times I with
                // the light's range, in place of 1/r and of its square, which is then not read)
                i *= falloff(self.version, light.switches, b5.range, r);
            } else {
                let r = if light.switches & light_switch::SQUARE_FALL_OFF != 0 { r * r } else { r };
                i *= 1.0 / r;
            }
        }
        (i, toward, reach)
    }

    /// What one light gives at p as 3.0 and 3.1: (c, I), or `None`.
    fn light_three(
        &self,
        index: usize,
        light: &LightSource,
        intensity: f64,
        p: Vec3,
        shadow: &mut dyn FnMut(usize, Vec3, Vec3, f64) -> Shadow,
    ) -> Option<([f32; 3], f64)> {
        let mut c = light.colour;
        if intensity.abs() < SMALL || intensity.is_nan() {
            return None;
        }
        let (mut i, toward, reach) = self.fallen_off(light, &Bryce5Light::NONE, p, intensity);
        if let LightPlace::Radial { .. } = light.place {
            i *= LAMP_FACTOR;
        }
        if light.switches & light_switch::NO_SHADOWS == 0 {
            let sh = shadow(index, p, toward, reach);
            c = std::array::from_fn(|k| (f64::from(sh.colour[k]) * f64::from(c[k])) as f32);
            i *= sh.amount;
        }
        Some((c, i))
    }

    /// What one light gives at p as 5.0: (c, I), or `None`.
    fn light_five(
        &self,
        index: usize,
        light: &LightSource,
        b5: &Bryce5Light,
        intensity: f64,
        p: Vec3,
        sum: &mut [f64; 3],
        shadow: &mut dyn FnMut(usize, Vec3, Vec3, f64) -> Shadow,
    ) -> Option<([f32; 3], f64)> {
        let c = light.colour;
        let above = |c: [f32; 3]| c.iter().any(|&ck| f64::from(ck).abs() > SMALL);
        if intensity.abs() < SMALL || intensity.is_nan() || !above(c) {
            return None;
        }
        let (mut i, toward, reach) = self.fallen_off(light, b5, p, intensity);
        if i.abs() < SMALL {
            return None;
        }
        if let LightPlace::Radial { .. } = light.place {
            i *= LAMP_FACTOR;
        }
        if light.switches & light_switch::NO_SHADOWS == 0 && self.scene.shadows {
            let sh = shadow(index, p, toward, reach);
            // : after the shadow ray and
            // before the lifting, the share eta with the colour psi goes straight into the sum,
            // f = ((1 - s)(1 - alpha)) eta, each channel psi_k f + sum_k
            let psi = b5.shadow_colour;
            if !(b5.shadow_share.abs() < SMALL) && psi.iter().any(|&x| f64::from(x).abs() > SMALL) {
                let f = ((1.0 - self.scene.shadow_setting) * (1.0 - sh.amount)) * b5.shadow_share;
                for k in 0..3 {
                    sum[k] = f64::from(psi[k]) * f + sum[k];
                }
            }
            return lift_by_shadow(c, i, sh, self.scene.shadow_setting);
        }
        Some((c, i))
    }

    /// Ck ⊕ Vk.
    pub fn over(colour: [f32; 3], v: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|k| colour[k] + v[k])
    }
}

/// A light's colour and intensity lifted by its shadow ('s colour and amount) under the
/// scene's shadow setting s, as 5.0; `None` where the light then counts for nothing. Used by the
/// light in the air and by the flat kind lit by the lights (observed).
pub fn lift_by_shadow(mut c: [f32; 3], mut i: f64, sh: Shadow, s: f64) -> Option<([f32; 3], f64)> {
    let above = |c: [f32; 3]| c.iter().any(|&ck| f64::from(ck).abs() > SMALL);
    let (sigma, alpha) = (sh.colour, sh.amount);
    if s > 0.0 {
        let tau = alpha * i;
        if tau != i {
            i = tau + s * (i - tau);
        }
        if i.abs() < SMALL {
            return None;
        }
        let s1 = f64::from(s as f32);
        let red_shaded = f64::from(c[0]) * f64::from(sigma[0]);
        let green_shaded = c[1] * sigma[1];
        let blue_shaded = c[2] * sigma[2];
        // where the three equal the light's own channels the colour is unchanged
        let unchanged = red_shaded == f64::from(c[0]) && green_shaded == c[1] && blue_shaded == c[2];
        if !unchanged {
            let red = (red_shaded + (f64::from(c[0]) - red_shaded) * s1) as f32;
            let lift = |ck: f32, shaded: f32| {
                let part = (f64::from(ck - shaded) * s1) as f32;
                part + shaded
            };
            c = [red, lift(c[1], green_shaded), lift(c[2], blue_shaded)];
        }
        if !above(c) {
            return None;
        }
    } else {
        if alpha.abs() < SMALL {
            return None;
        }
        i *= alpha;
        if !above(sigma) {
            return None;
        }
        c = std::array::from_fn(|k| sigma[k] * c[k]);
    }
    Some((c, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: LitAirScene = LitAirScene { shadows: true, shadow_setting: 0.0, sun_position: [0.0; 3] };

    #[test]
    fn which_bits_act() {
        let n = |v, s| LitAir::new(v, s, 1.0, SCENE).map(|a| a.places);
        assert_eq!(n(RenderAs::Bryce30, 0x100), None);
        assert_eq!(n(RenderAs::Bryce30, LIT_AIR_COARSE), Some(500));
        assert_eq!(n(RenderAs::Bryce31, LIT_AIR_FINE), Some(1000));
        assert_eq!(n(RenderAs::Bryce31, LIT_AIR_FINE | LIT_AIR_COARSE), Some(1000), "with both, n = 1000");
        assert_eq!(n(RenderAs::Bryce50, LIT_AIR_FINE), Some(1000));
        assert_eq!(n(RenderAs::Bryce50, LIT_AIR_FINE | PLAIN_SKY), None, "as 5.0 a plain sky has none");
        assert_eq!(n(RenderAs::Bryce30, LIT_AIR_FINE | PLAIN_SKY), Some(1000));
    }

    /// The places end at d for a ray that meets a surface: a ray meeting one at t = 0 has
    /// the single place t = 0, whose weight is 2A·1.2.
    #[test]
    fn a_surface_at_the_eye_has_one_place() {
        let air = LitAir::new(RenderAs::Bryce30, LIT_AIR_COARSE, 0.5, SCENE).unwrap();
        let sun = LightSource {
            place: LightPlace::Sun { direction: [0.0, 1.0, 0.0] },
            colour: [1.0, 0.5, 0.25],
            intensity: 1.0,
            switches: light_switch::ON | light_switch::NO_FALL_OFF | light_switch::NO_SHADOWS,
        };
        let mut asked = 0;
        let v = air.gather([0.0; 3], [0.0, 0.0, 1.0], Some(0.0), &[sun], &mut |_, _, _, _| {
            asked += 1;
            Shadow { colour: [1.0; 3], amount: 1.0 }
        });
        assert_eq!(asked, 0, "bit 0x800: no shadow asked");
        let w = 2.0 * 0.5 * 1.2;
        assert_eq!(v, [(w / 1000.0) as f32, (0.5 * w / 1000.0) as f32, (0.25 * w / 1000.0) as f32]);
    }

    /// As 5.0 a light with the range bit falls off as a = 1 - lambda over its range, not as 1/r.
    #[test]
    fn five_range_falloff() {
        let air = LitAir::new(RenderAs::Bryce50, LIT_AIR_COARSE, 0.5, SCENE).unwrap();
        let lamp = LightSource {
            place: LightPlace::Radial { position: [0.0, 4.0, 0.0] },
            colour: [1.0, 1.0, 1.0],
            intensity: 1.0,
            switches: light_switch::ON | light_switch::RANGE_FALL_OFF | light_switch::NO_SHADOWS,
        };
        let b5 = Bryce5Light { range: 8.0, shadow_share: 0.0, shadow_colour: [0.0; 3] };
        let ray = PathRay::camera([0.0; 3], [0.0, 0.0, 1.0], Some(0.0));
        let v = air.gather_path(&ray, &[lamp], &[b5], &mut |_, _| Some(1.0), &mut |_, _, _, _| unreachable!()).unwrap();
        // lambda = (r - 1)/8 = 0.5 (r the root plus 1, less 1), a = 0.5, then the lamp factor 2.4
        let w = 2.0 * 0.5 * 1.2;
        let expected = ((1.0 * ((1.0 * 0.5) * 2.4)) * w) / 1000.0;
        assert_eq!(v[0], expected as f32);
    }

    /// As 5.0 the light in shadow goes straight into the sum, before the lifting.
    /// (As 5.0, Bryce 5's lights: the light in shadow)
    #[test]
    fn five_shadow_share_goes_into_the_sum() {
        let scene = LitAirScene { shadows: true, shadow_setting: 0.25, sun_position: [0.0; 3] };
        let air = LitAir::new(RenderAs::Bryce50, LIT_AIR_COARSE, 0.5, scene).unwrap();
        let sun = LightSource {
            place: LightPlace::Sun { direction: [0.0, 1.0, 0.0] },
            colour: [1.0, 0.0, 0.0],
            intensity: 1.0,
            switches: light_switch::ON | light_switch::NO_FALL_OFF,
        };
        let b5 = Bryce5Light { range: 0.0, shadow_share: 0.5, shadow_colour: [1.0, 0.0, 0.5] };
        let ray = PathRay::camera([0.0; 3], [0.0, 0.0, 1.0], Some(0.0));
        let blocked = Shadow { colour: [0.0; 3], amount: 0.0 };
        let v = air.gather_path(&ray, &[sun], &[b5], &mut |_, _| Some(1.0), &mut |_, _, _, _| blocked).unwrap();
        // f = ((1 - s)(1 - alpha)) eta = 0.375 added to the sum before anything else: psi_k f
        let share = [0.375f64, 0.0, 0.1875];
        // the lifted light: tau = 0, I = 0.25 (s = 0.25 above 0); colour 1 shaded to 0 and lifted by s' to 0.25
        let w = 2.0 * 0.5 * 1.2;
        let own = (0.25f64 * 0.25) * w;
        assert_eq!(v[0], ((share[0] + own) / 1000.0) as f32);
        assert_eq!(v[2], (share[2] / 1000.0) as f32);
    }

    /// As 5.0 without the scene's shadow bit no shadow ray is cast.
    #[test]
    fn five_without_scene_shadows_casts_none() {
        let scene = LitAirScene { shadows: false, shadow_setting: 0.8, sun_position: [0.0; 3] };
        let air = LitAir::new(RenderAs::Bryce50, LIT_AIR_COARSE, 0.5, scene).unwrap();
        let sun = LightSource {
            place: LightPlace::Sun { direction: [0.0, 1.0, 0.0] },
            colour: [1.0, 0.5, 0.25],
            intensity: 1.0,
            switches: light_switch::ON | light_switch::NO_FALL_OFF,
        };
        let mut asked = 0;
        air.gather([0.0; 3], [0.0, 0.0, 1.0], Some(1.0), &[sun], &mut |_, _, _, _| {
            asked += 1;
            Shadow { colour: [0.0; 3], amount: 0.0 }
        });
        assert_eq!(asked, 0);
    }
}
