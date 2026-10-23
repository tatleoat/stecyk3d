// SPDX-License-Identifier: MIT
//! The wash of a Bryce 5 sky with bit 0x1000000, in the render: which skies have it, the site the
//! arithmetic of `crate::sky::wash` draws and shadows through, the wash over a ray's colour and
//! the sun's light at a lit point.
//!

use super::*;
use crate::scene_sky::SKY_VERSION_SEVEN;
use crate::sky::wash::{self, WashSite, WashSun};

/// The bits of a sky's word one of which the wash over the rays needs.
pub(super) const RAY_BITS: u32 = 0x2 | 0x4 | 0x8;
/// Bit 0x800: the sky is the plain colour and no wash is laid.
pub(super) const SKY_EFFECTS_OFF: u32 = 0x800;
/// The wash bit of a Bryce 5 sky's word.
const WASH_BIT: u32 = 0x100_0000;

/// Whether the render as `version` of `scene` has the wash's bit: a sky part of version 7 whose
/// word has bit 0x1000000, as 5.0 (a Bryce 3D sky's stored bit is the lit air's, taken over).
pub(super) fn bit_of(scene: &Scene, version: RenderAs) -> bool {
    version == RenderAs::Bryce50 && scene.sky.form() == SKY_VERSION_SEVEN && scene.sky.flags & WASH_BIT != 0
}

/// The sun as the wash reads it: S is the sun's own direction as the document holds it, with bit
/// 0x200 too (only the sky's colours take S′). (Inputs: S, L, the light part's
/// intensity; "With bit 0x200")
pub(super) fn sun_of(scene: &Scene) -> WashSun {
    let sun = scene.sun();
    WashSun {
        direction: sun.direction,
        light_colour: sun.light.colour,
        light_intensity: sun.light.intensity,
    }
}

/// What the wash draws and shadows through for one ray of the tree.
struct Site<'a> {
    surface: &'a PlainSurface,
    sun: WashSun,
    depth: u32,
    media: PathMedia,
    /// The ray's sample number and the rays a side, if it carries one.
    sample: Option<(u32, u32)>,
}

impl WashSite for Site<'_> {
    /// : the render's generator, lent to the surface rules by the stage
    fn draw(&mut self) -> f64 {
        let mut g = self.surface.generator.get();
        let d = g.next_u15();
        self.surface.generator.set(g);
        f64::from(d)
    }

    /// : the sun's shadow ray from q itself (the origin is not moved and no
    /// ray is given up), in the medium of the ray, with's soft rays where the sun has a
    /// radius
    fn sun_amount(&mut self, q: Vec3) -> Result<f64, OpenItem> {
        let surface = self.surface;
        let mut tracer = surface.tracer(
            ShadowStart { incoming: self.media, far: None, toward_eye: [0.0; 3], object: None },
            self.depth,
        );
        tracer.soft = Some(SoftAt { generator: &surface.generator, sample: self.sample, grid: surface.soft_grid });
        let light = surface.lit.len() - 1; // the sun is the last light
        let query = ShadowQuery {
            light,
            point: q,
            geometric_normal: [0.0; 3],
            normal: [0.0; 3],
            direction: self.sun.direction,
            reach: f64::INFINITY,
            offset: false,
        };
        let shadow = tracer.shadow(&query);
        match tracer.error {
            Some(e) => Err(e),
            None => Ok(shadow.amount),
        }
    }
}

impl PlainSurface {
    /// The colour of a ray after the wash, or as it is where the sky has none. (points 1
    /// to 6)
    pub(super) fn wash_ray(
        &self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<f64>,
        premium: Option<stages::PremiumSample>,
        colour: [f32; 3],
    ) -> Result<[f32; 3], OpenItem> {
        let (true, Some(sun)) = (self.wash_rays, self.wash) else { return Ok(colour) };
        let mut site = Site { surface: self, sun, depth, media: state.media, sample: premium.map(|p| (p.number, p.n)) };
        wash::wash_ray(&mut site, &sun, ray.origin, ray.dir, hit, colour)
    }

    /// The wash's march of a ray, with its draws, apart from the colour it is laid over (`None`
    /// where the sky has no wash or the ray misses the outer sphere).
    pub(super) fn wash_effect_of(
        &self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<f64>,
        premium: Option<stages::PremiumSample>,
    ) -> Result<Option<wash::WashEffect>, OpenItem> {
        let (true, Some(sun)) = (self.wash_rays, self.wash) else { return Ok(None) };
        let mut site = Site { surface: self, sun, depth, media: state.media, sample: premium.map(|p| (p.number, p.n)) };
        wash::wash_effect(&mut site, &sun, ray.origin, ray.dir, hit)
    }

    /// The sun's colour at a sample's place of a march that lights its samples (the lit kind and the
    /// flat kind lit by the scene's lights): point 9's light at the place, its draws taken now. `None`
    /// where the sky has no wash or light `i` is not the sun (the last light).
    pub(super) fn sun_colour_in_march(&self, i: usize, place: Vec3, depth: u32, media: PathMedia) -> Option<Result<[f32; 3], OpenItem>> {
        let sun = self.wash?;
        if i + 1 != self.lit.len() {
            return None;
        }
        let mut site = Site { surface: self, sun, depth, media, sample: None };
        Some(wash::sun_at_lit_point(&mut site, &sun, place))
    }

    /// The lights with the sun's colour and intensity at the lit point of `request`'s surface
    /// replaced as point 9 says: the light at the point without a shadow ray and without the tint,
    /// with the intensity of the sun's light part. The sun is the last light; one that is off
    /// asks nothing.
    pub(super) fn sun_lit_at(&self, lights: &[LitLight], request: &SurfaceRequest) -> Result<Vec<LitLight>, OpenItem> {
        let mut lights = lights.to_vec();
        let (Some(sun), Some(last)) = (self.wash, lights.last_mut()) else { return Ok(lights) };
        if last.source.switches & light_switch::ON == 0 {
            return Ok(lights);
        }
        let mut site = Site {
            surface: self,
            sun,
            depth: request.depth,
            media: request.state.media,
            sample: request.premium.map(|p| (p.number, p.n)),
        };
        last.source.colour = wash::sun_at_lit_point(&mut site, &sun, request.hit.point)?;
        last.source.intensity = sun.light_intensity;
        Ok(lights)
    }
}
