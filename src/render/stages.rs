// SPDX-License-Identifier: MIT
//! The staged loop: places, camera rays, hits, colour requests, per-pixel sums, levels.
//!
//! (order),,
//! points 3, 6 and 7 (the generator's start, the region, lights' own objects),
//! (Effects: the order of a Premium sample's draws; points 4 and 5: the cones' and the
//! ambient ray's draws, taken here just before the ray is traced; point 5: the ambient ray's
//! air that of any further ray), (Further rays: the air over each ray of the tree,
//! after the join and before the blended replacement), (Where marches
//! are made: each ray's march made once it is traced, laid over its colour last),
//! (the shadows of a light with a radius draw; the generator lent to the rules
//! around each surface, every such render sample by sample)
//! (Stages; Shading that draws; Premium samples that draw in their trees; The air
//! over blurred and ambient rays), DESIGN.md (a staged card version later), sky.md (The air
//! over a tree of rays)
//!
//! Each stage reads one plain batch and fills the next. Batches are cleared and refilled,
//! never shrunk. Nothing recurses: a camera ray's tree of further rays is walked depth first
//! on a stack of reused buffers (`shade_tree`), the order in which rays are traced ('s
//! angle law depends on it).

use crate::geom::Vec3;
use crate::geometry::{Hit, Range, Ray, ShapeContext};
use super::media::PathState;
use super::plan::GeometryBuild;
use crate::sampling::premium;
use crate::sky::air::AirPath;
use crate::numerics::CRand;
use crate::sampling::aa_pass::{EXTRA_RAYS, RefineRule, edge_mask_in, refine_points};
use crate::sampling::blocks::CornerBlocks;
use crate::sampling::lens::{Lens, LensOrders};
use crate::sampling::batch::{CameraRay, PointBatch, RayBatch};
use crate::sampling::output::{PostOptions, channel_level};
use crate::sampling::samples::to_binary32;
use crate::sampling::{PixelSum, PixelVisit};
use crate::scene::Scene;
use crate::shading::OpenItem;
use crate::shading::further::BroughtBack;
use crate::shading::volume::Laying;

use super::plan::{PictureWay, RenderError, RenderPlan, open};

/// Which camera sample a request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleTag {
    /// The first pass: the sample's number within its pixel, in the order the places are
    /// taken .
    First(u32),
    /// The pass over edges: which of the seven extra rays (0 to 6, the order of the
    /// substrate's `aa-offsets`). In Bryce 5 these carry no sample number: they are not
    /// Premium samples (question 5).
    Edge(u32),
}

/// Pixels whose samples go through the stages together. (batch size;
/// any size gives the same picture while no rule draws from the generator)
pub const PIXELS_PER_BATCH: usize = 256;

/// Per ray of a `RayBatch`: what it meets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitBatch {
    /// `None`: the ray meets nothing (the sky's).
    pub hit: Vec<Option<Hit>>,
}

impl HitBatch {
    pub fn clear(&mut self) {
        self.hit.clear();
    }
    pub fn len(&self) -> usize {
        self.hit.len()
    }
    pub fn is_empty(&self) -> bool {
        self.hit.is_empty()
    }
}

/// A camera ray or a further ray that met a surface, handed to the surface rules.
/// , (Inputs: the
/// ray's depth and total weight)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRequest {
    /// The ray: a camera ray, or a further ray (its origin, direction and start).
    pub ray: CameraRay,
    pub hit: Hit,
    /// The index in `Scene::objects` of the object met.
    pub object: usize,
    /// The pixel (row-order index) and which of its samples this is (a further ray carries
    /// its camera ray's). For's blocks: the place's index in the grid of places
    /// (`CornerBlocks::place_of`), sample 0.
    pub pixel: u32,
    pub sample: SampleTag,
    /// The ray's depth: 0 for a camera ray, its parent's + 1 for a further ray.
    pub depth: u32,
    /// The ray's total weight: (1, 1, 1) for a camera ray.
    pub total: [f32; 3],
    /// The ray's path: its media, its count of total internal reflections, its rays.
    pub state: PathState,
    /// The Premium sample's number the ray carries (every ray of the sample's tree carries the
    /// camera ray's), where the render draws in trees; `None`: the ray carries no number and
    /// is never turned and asks for no ambient ray.
    pub premium: Option<PremiumSample>,
}

/// A Premium sample's number s (0 to n² − 1) and the rays a side n.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PremiumSample {
    pub number: u32,
    pub n: u32,
}

/// What the stage does to a queued ray just before it is traced, taking the generator's two
/// draws for it. points 4 and 5, Effects ("the cone's draws come immediately
/// before the ray is traced"); (Premium samples that draw in their trees)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayTurn {
    /// Traced as queued; no draw.
    None,
    /// A blurred ray: its direction turned toward a point of the disc of this radius ρb.
    Cone { radius: f64 },
    /// The ambient ray: the queued direction is the surface's geometric normal g, replaced by
    /// the direction about it.
    Ambient,
}

/// The total weight of a camera ray.
pub const CAMERA_TOTAL: [f32; 3] = [1.0; 3];

/// A camera ray or a further ray that met nothing, handed to the sky rules.
/// ; (Effects: a further ray that meets nothing
/// takes the sky's colour)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyRequest {
    pub ray: CameraRay,
    /// Where the ray's path starts: the camera ray's origin, for it and for every further ray
    /// of its tree (As 5.0 meets the layers from there).
    pub path_origin: Vec3,
    pub pixel: u32,
    pub sample: SampleTag,
    /// The path the ray's air is measured along (a continued ray's starts before its origin).
    pub air: AirPath,
    /// The ray's depth (0 for a camera ray) and the media of its path: what the light in the
    /// air reads of a ray that meets nothing. (Inputs: the ray's depth m; whether
    /// the ray travels in the medium of a solid)
    pub depth: u32,
    pub media: super::media::PathMedia,
}

/// What a queued ray is for.;
/// (shadow rays are traced inline by the rules: one queued is refused)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryKind {
    Reflection,
    Transmission,
    /// The blended material's ray straight on (The blended material).
    Blended,
    /// A fuzzy surface's continuing ray straight on (step 1).
    Fuzzy,
    /// The ray that goes on through a volume material's surface (A volume material's
    /// surface): its colour replaces the surface's.
    VolumeThrough,
    Shadow,
    /// True ambience's one ambient ray (point 5): queued alone by `surface`, its colour
    /// handed to `SurfaceRules::resume` as it comes back (no weight).
    Ambient,
    /// The start ray of a Light-mode surface : straight on from the hit, queued alone;
    /// its colour goes to `SurfaceRules::resume` as the colour s0 the surface's sum starts from.
    Start,
}

/// A further ray a surface sends, as's step gives it.
/// ; (Further rays: queued, not traced recursively)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondaryRay {
    pub kind: SecondaryKind,
    /// Origin, direction (as sent, not brought to unit length) and the distance from which
    /// it counts surfaces.
    pub ray: CameraRay,
    /// The weight W its colour comes back with (the weight w as sent, for a ray
    /// outside every object).
    pub weight: [f32; 3],
    /// Its total weight T', and its depth (its parent's + 1).
    pub total: [f32; 3],
    pub depth: u32,
    pub pixel: u32,
    pub sample: SampleTag,
    /// The path's state the ray carries (its media, its count, its rays).
    pub state: PathState,
}

/// The rays the surface rules send while one generation of the ray tree is shaded, in the
/// order sent. (Further rays)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondaryQueue {
    rays: Vec<SecondaryRay>,
    /// Per ray, what the stage does to it before tracing it.
    turns: Vec<RayTurn>,
    /// The one ray queued is sent before the surface is lit: its colour is held for the join
    /// and the rules' `resume` lights the surface when it is back (steps 1 and 3).
    lit_after: bool,
}

impl SecondaryQueue {
    /// The ray queued goes alone and the surface is lit when it is back.
    pub fn defer_lighting(&mut self) {
        self.lit_after = true;
    }
    /// A ray traced as queued.
    pub fn push(&mut self, r: SecondaryRay) {
        self.push_turned(r, RayTurn::None);
    }
    /// A ray the stage turns (with two draws) just before tracing it.
    pub fn push_turned(&mut self, r: SecondaryRay, turn: RayTurn) {
        self.rays.push(r);
        self.turns.push(turn);
    }
    pub fn len(&self) -> usize {
        self.rays.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rays.is_empty()
    }
    pub fn clear(&mut self) {
        self.rays.clear();
        self.turns.clear();
        self.lit_after = false;
    }
}

/// The colour of a surface sample.
pub trait SurfaceRules {
    /// A further ray has been traced and its own hit (or miss) found, before it is shaded: the
    /// rules may note in its state what they hold at this moment. The default changes
    /// nothing.
    fn ray_leaves(&self, _ray: &mut SecondaryRay) {}

    /// The surface's local colour, red, green, blue (clamps the sample's final
    /// colour). Further rays are pushed to `secondary`, in the order they are sent; their
    /// colours are joined afterwards by `join`. `&mut self`:'s carried state makes a
    /// result depend on the order of requests (Further rays).
    fn surface(&mut self, scene: &Scene, request: &SurfaceRequest, secondary: &mut SecondaryQueue)
    -> Result<[f64; 3], OpenItem>;

    /// The surface's colour: its local colour with what each further ray it sent brought
    /// back, in the order sent. Called only for a surface that sent at least one ray. The
    /// default refuses: rules that send rays say how their colours join.
    fn join(&mut self, _local: [f64; 3], _brought_back: &[BroughtBack]) -> Result<[f64; 3], OpenItem> {
        Err(open::SECONDARY_RAYS)
    }

    /// The colour of the ray that met the surface, from the surface's colour joined with what
    /// its reflected and transmitted rays brought back (the local colour where it sent none),
    /// before a blended ray's replacement:'s air of the ray's path over the sum.
    /// Called once for every surface met. The default changes nothing (rules with no air).
    /// ; (The air over a tree
    /// of rays)
    fn over_joined(&mut self, _request: &SurfaceRequest, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        Ok(colour)
    }

    /// Called once for every surface met, after's blended replacement (or at once
    /// after `over_joined` where there is none): where the light in the air is added. The
    /// default changes nothing.
    fn after_replacement(&mut self, _request: &SurfaceRequest, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        Ok(colour)
    }

    /// A camera sample's final colour from the colour its ray was given (the surface's,
    /// joined, or the sky's): where the camera's eye lies inside a solid,'s product
    /// with that solid's volume colour. Called for every camera ray, after the join and
    /// before the sum. The default changes nothing (rules that know no media).
    fn camera_sample(&mut self, _ray: &CameraRay, _hit: Option<&Hit>, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        Ok(colour)
    }

    /// A ray of the tree (a camera ray or a further ray, of depth `depth`, in the path's state
    /// `state`) has been traced and met `hit` (or nothing): the march of the medium it travels
    /// in, made now, before what it meets is coloured (its surface shaded or the sky asked),
    /// and laid over that colour last. The default: no march (rules that know no volumes).
    fn march(
        &mut self,
        _ray: &CameraRay,
        _depth: u32,
        _state: &PathState,
        _hit: Option<&Hit>,
        _premium: Option<PremiumSample>,
    ) -> Result<Option<Laying>, OpenItem> {
        Ok(None)
    }

    /// The wash of a Bryce 5 sky's bit 0x1000000, laid over the colour of a ray of the tree (a
    /// camera ray or a further ray; `hit` the parameter at which it meets a surface, if it
    /// does) right after the air and the light in the air, under a medium's
    /// march. It takes draws of the generator (lent to the rules by the stage as for a surface).
    /// The default changes nothing.;
    fn wash(
        &mut self,
        _ray: &CameraRay,
        _depth: u32,
        _state: &PathState,
        _hit: Option<f64>,
        _premium: Option<PremiumSample>,
        colour: [f32; 3],
    ) -> Result<[f32; 3], OpenItem> {
        Ok(colour)
    }

    /// The wash's march of a ray with its draws, apart from the colour it is laid over, for the
    /// ray whose draws come before the colour is known (a blended surface's: its wash comes
    /// before the ray straight on is sent). `None`: no wash, or the ray misses the sphere. The
    /// default: none.;
    fn wash_effect(
        &mut self,
        _ray: &CameraRay,
        _depth: u32,
        _state: &PathState,
        _hit: Option<f64>,
        _premium: Option<PremiumSample>,
    ) -> Result<Option<crate::sky::wash::WashEffect>, OpenItem> {
        Ok(None)
    }

    /// Whether the surface `request` meets is a volume material's: such a ray takes no wash of its
    /// own (its colour is what the ray that goes on brings back). The default: no.
    fn is_volume_surface(&self, _request: &SurfaceRequest) -> bool {
        false
    }

    /// The ambient rays the march just made asks for (the shaded kind's samples with true
    /// ambience), in the order asked: the stage traces each and hands the colours back with
    /// `march_gathered`, then marches again. The default asks none. point 5
    fn march_asks(&mut self) -> Vec<SecondaryRay> {
        Vec::new()
    }

    /// What the ambient rays of `march_asks` brought back, for the next march.
    fn march_gathered(&mut self, _gathered: Vec<[f32; 3]>) {}

    /// The state a camera ray's path starts in (the medium of the solid that holds the
    /// eye). The default: outside every object.
    fn camera_state(&self, _ray: &CameraRay) -> Result<PathState, OpenItem> {
        Ok(PathState::CAMERA)
    }

    /// What a further ray brings back to its parent's join: its colour and the weight W it
    /// comes back with (W as the medium it travelled in gives it, from its hit, or
    /// its miss). The default: the weight as sent.
    fn brought_back(&mut self, ray: &SecondaryRay, _hit: Option<&Hit>, colour: [f32; 3]) -> Result<BroughtBack, OpenItem> {
        Ok(BroughtBack::further(colour, ray.weight))
    }

    /// The second step of a surface that queued an ambient ray (`SecondaryKind::Ambient`,
    /// alone): `gathered` is what that ray brought back (its colour as binary32, no weight).
    /// Returns the surface's local colour and queues its further rays, as `surface` does for
    /// a surface that gathers nothing. The default refuses: rules that queue no ambient ray
    /// are never asked. point 5,
    fn resume(
        &mut self,
        _scene: &Scene,
        _request: &SurfaceRequest,
        _gathered: [f32; 3],
        _secondary: &mut SecondaryQueue,
    ) -> Result<[f64; 3], OpenItem> {
        Err(open::SECONDARY_RAYS)
    }

    /// Whether shading a surface draws from the generator (Bryce 5's soft shadows: a light
    /// with a radius above 0). Where it does, the stage shades every sample in the order of
    /// the render's draws and lends the generator around each surface (`swap_generator`).
    /// The default: no draw.; (Shading that draws)
    fn draws(&self) -> bool {
        false
    }

    /// Exchanges the rules' generator with the render's `generator`: the stage calls it just
    /// before `surface` (the rules then draw from the render's sequence) and just after (the
    /// render's generator goes on from the rules' last draw). The default does nothing.
    /// , (Shading that draws)
    fn swap_generator(&mut self, _generator: &mut CRand) {}
}

/// The deepest generation the stage traces before it refuses: a guard against rules whose
/// rays never stop.'s own limit (6) is the rules'. (Further rays)
pub const GENERATION_GUARD: u32 = 64;

/// The colour of a sample that meets nothing.
pub trait SkyRules {
    fn sky(&mut self, scene: &Scene, request: &SkyRequest) -> Result<[f64; 3], OpenItem>;

    /// Called once before the first pixel's places are drawn, with the render's generator.
    /// A sky with stars or comets starts the render's generator elsewhere than seed 1
    /// (point 3; how is component sky's, Open): rules that know it draw here. The
    /// default draws nothing, which is right for every sky without stars or comets.
    /// point 3
    fn before_first_pixel(&mut self, _scene: &Scene, _generator: &mut CRand) -> Result<(), OpenItem> {
        Ok(())
    }
}

/// The batches of stages 1 to 4, kept between batches of pixels.
#[derive(Clone, Debug, Default)]
pub struct Stages {
    /// Stage 4's depth-first walk of each camera ray's tree.
    tree: TreeBuffers,
    pub points: PointBatch,
    pub rays: RayBatch,
    pub hits: HitBatch,
    /// Which sample each point is, per point.
    pub sample: Vec<SampleTag>,
    /// Per point of the first pass with a lens : the ray the lens made of its
    /// camera ray, `None` where the lens left it. Empty when the plan has no lens.
    lens_rays: Vec<Option<CameraRay>>,
}

impl Stages {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage 1: the sample places of `visits`, in render order, drawing from the plan's
    /// generator. Each point is tagged with its pixel's row-order index.
    pub fn places(&mut self, plan: &mut RenderPlan, visits: &[PixelVisit]) {
        self.points.clear();
        self.sample.clear();
        self.lens_rays.clear();
        if let Some(lens) = plan.lens.filter(Lens::acts) {
            return self.lens_places(plan, lens, visits);
        }
        for v in visits {
            let pixel = v.y * plan.width + v.x;
            let before = self.points.len();
            plan.placer.place_pixel(v.x, v.y, pixel, &mut plan.generator, &mut self.points);
            self.sample.extend((0..(self.points.len() - before) as u32).map(SampleTag::First));
        }
    }

    /// Stage 1 with the lens of the Premium setting: each sample is placed (its draws, and
    /// after sample 0 the shuffle), then its camera ray is made and the lens takes its two
    /// draws where it moves the ray, before the next sample's places (Effects).
    /// The ray the lens makes is kept for stage 2. point 3
    fn lens_places(&mut self, plan: &mut RenderPlan, lens: Lens, visits: &[PixelVisit]) {
        let RenderPlan { placer, generator, camera, .. } = plan;
        let frame = camera.lens_frame();
        let n = placer.rays_a_side;
        for v in visits {
            let pixel = v.y * plan.width + v.x;
            let (fx, fy) = (v.x as f64, v.y as f64);
            placer.pixel_samples(generator, |rng, s| {
                let (x, y) = (fx + s.dx, fy + s.dy);
                self.points.push(x, y, pixel);
                self.sample.push(SampleTag::First(s.index));
                let camera_ray = camera.ray(x, y);
                let moved = lens.ray(&frame, &camera_ray, s.number, n, LensOrders::BRYCE50, || rng.next_u15());
                self.lens_rays.push(moved.map(|l| l.ray));
            });
        }
    }

    /// Stages 1 to 4 for Premium samples whose trees draw : each pixel of `visits`, in
    /// order, one sample at a time: its places (and at its first sample the shuffle), the
    /// lens's two draws if it acts, its camera ray and hit, its tree with every draw of its
    /// cones and ambient rays, the camera sample's colour to `each`; then the next sample's
    /// places. Also every render whose shading draws , whatever the setting: its
    /// samples carry a number only in the Premium setting. (Effects: the order of
    /// everything a numbered sample draws), point 3, (the
    /// jittered places, two draws a sample, then its shading),
    /// (Premium samples that draw in their trees: the per-sample path;
    /// Shading that draws)
    #[allow(clippy::too_many_arguments)]
    pub fn premium_samples(
        &mut self,
        scene: &Scene,
        plan: &mut RenderPlan,
        surface: &mut dyn SurfaceRules,
        sky: &mut dyn SkyRules,
        visits: &[PixelVisit],
        counts: &mut RenderCounts,
        mut each: impl FnMut(u32, [f32; 3]),
    ) -> Result<(), RenderError> {
        let RenderPlan { placer, generator, camera, geometry, shape_context, lens, width, .. } = plan;
        let tracing = Tracing { geometry, ctx: shape_context };
        let lens = lens.filter(Lens::acts);
        let frame = camera.lens_frame();
        let eye = camera.eye();
        let n = placer.rays_a_side;
        let numbered = placer.setting == crate::sampling::AaSetting::Premium;
        let tree = &mut self.tree;
        let mut failed: Option<RenderError> = None;
        for v in visits {
            let pixel = v.y * *width + v.x;
            let (fx, fy) = (v.x as f64, v.y as f64);
            placer.pixel_samples(generator, |rng, s| {
                if failed.is_some() {
                    return;
                }
                let mut one = || -> Result<(), RenderError> {
                    let camera_ray = camera.ray(fx + s.dx, fy + s.dy);
                    let ray = match lens {
                        Some(l) => l
                            .ray(&frame, &camera_ray, s.number, n, LensOrders::BRYCE50, || rng.next_u15())
                            .map_or(camera_ray, |m| m.ray),
                        None => camera_ray,
                    };
                    let sample = SampleTag::First(s.index);
                    let range = Range { near: ray.start, far: f64::INFINITY };
                    if let Some(ms) = &tracing.geometry.scene.metaballs {
                        ms.new_sample();
                    }
                    let hit = tracing.geometry.scene.intersect(&Ray { origin: ray.origin, dir: ray.dir }, range, tracing.ctx);
                    let rgb = match hit {
                        Some(hit) => {
                            counts.surface += 1;
                            let req = SurfaceRequest {
                                ray,
                                hit,
                                object: tracing.geometry.scene_object[hit.object],
                                pixel,
                                sample,
                                depth: 0,
                                total: CAMERA_TOTAL,
                                state: camera_state_of(surface, &ray, eye, lens.is_some())?,
                                // : a Premium sample's rays carry its
                                // number; the other settings' carry none (case 1).
                                premium: numbered.then_some(PremiumSample { number: s.number, n }),
                            };
                            shade_tree(scene, &tracing, surface, sky, &req, tree, counts, Some(&mut *rng), ray.origin)?
                        }
                        None => {
                            counts.sky += 1;
                            {
                                let st = camera_state_of(surface, &ray, eye, lens.is_some())?;
                                camera_miss(scene, &tracing, surface, sky, ray, pixel, sample, st, numbered.then_some(PremiumSample { number: s.number, n }), counts, Some(&mut *rng))?
                            }
                        }
                    };
                    counts.samples += 1;
                    each(pixel, surface.camera_sample(&ray, hit.as_ref(), rgb)?);
                    Ok(())
                };
                if let Err(e) = one() {
                    failed = Some(e);
                }
            });
            if let Some(e) = failed {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Stage 1 of's blocks: one point a place (column, row) of `places` (indices in
    /// `way`'s grid of places, which reaches 4 beyond the picture's last column and row), at
    /// the place plus `offset` each way; each point is tagged with its grid index. No draw
    /// of the generator.
    pub fn corner_places(&mut self, stride: u32, offset: f64, places: &[u32]) {
        self.points.clear();
        self.sample.clear();
        for &i in places {
            let (x, y) = (i % stride, i / stride);
            self.points.push(x as f64 + offset, y as f64 + offset, i);
            self.sample.push(SampleTag::First(0));
        }
    }

    /// Stage 1 of the pass over edges: the seven fixed places of each pixel of `pixels`
    /// (row-order indices, in the order the pass visits them), from `aa-offsets`. These
    /// places take no draws of the generator.
    pub fn edge_places(&mut self, width: u32, offsets: &[f64; 14], pixels: &[u32]) {
        self.points.clear();
        self.sample.clear();
        for &p in pixels {
            refine_points(p % width, p / width, p, offsets, &mut self.points);
            self.sample.extend((0..EXTRA_RAYS as u32).map(SampleTag::Edge));
        }
    }

    /// Stage 4: a colour per camera ray, in the order the points were made, each as the
    /// binary32 channels the shading hands back; `each(pixel, colour)` receives them in that
    /// order. Each camera ray's tree is run depth first, on a stack (`shade_tree`; no
    /// recursion, buffers reused): the ray's surface is shaded (its
    /// shadow rays traced inline by the rules), then each further ray it sends, in the order
    /// sent, with every ray it leads to before the next; then the join. This is the order in
    /// which rays are traced, which decides which surface of a material is met first.
    /// `draws`: the render's generator, lent to the rules around each surface where their
    /// shading draws (the pass over edges,'s places; `None` in the batched first pass,
    /// which runs only where nothing draws).; (the angle
    /// law: within a sample depth first),
    #[allow(clippy::too_many_arguments)]
    pub fn colours(
        &mut self,
        scene: &Scene,
        plan: &RenderPlan,
        surface: &mut dyn SurfaceRules,
        sky: &mut dyn SkyRules,
        counts: &mut RenderCounts,
        mut draws: Option<&mut CRand>,
        mut each: impl FnMut(u32, [f32; 3]),
    ) -> Result<(), RenderError> {
        for i in 0..self.rays.len() {
            // Asked at every ray, not each batch: a batch of 256 pixels takes seconds on a scene of many triangles, and
            // a view change must not wait for it (the serve mode's stop; never set by a one-shot run). The old
            // engine's render asks at every pixel the same way. `hits` leaves its list short when stopped: this
            // ends the batch before any of it is read.
            super::parallel::stop_if_cancelled()?;
            let ray = self.rays.get(i);
            let point = self.rays.source[i] as usize;
            let pixel = self.points.pixel[point];
            let sample = self.sample[point];
            let found = if plan.walks_change_shading() {
                if let Some(ms) = &plan.geometry.scene.metaballs {
                    ms.new_sample();
                }
                let range = Range { near: ray.start, far: f64::INFINITY };
                plan.geometry.scene.intersect(&Ray { origin: ray.origin, dir: ray.dir }, range, &plan.shape_context)
            } else {
                self.hits.hit[i]
            };
            let rgb = match found {
                Some(hit) => {
                    counts.surface += 1;
                    let object = plan.geometry.scene_object[hit.object];
                    let state = camera_state_of(surface, &ray, plan.camera.eye(), plan.lens.is_some_and(|l| l.acts()))?;
                    // No number: the batched stages run only where no tree draws (render.md).
                    let req = SurfaceRequest {
                        ray,
                        hit,
                        object,
                        pixel,
                        sample,
                        depth: 0,
                        total: CAMERA_TOTAL,
                        state,
                        premium: None,
                    };
                    let tracing = Tracing { geometry: &plan.geometry, ctx: &plan.shape_context };
                    shade_tree(scene, &tracing, surface, sky, &req, &mut self.tree, counts, draws.as_deref_mut(), ray.origin)?
                }
                None => {
                    counts.sky += 1;
                    {
                        let st = camera_state_of(surface, &ray, plan.camera.eye(), plan.lens.is_some_and(|l| l.acts()))?;
                        let tracing = Tracing { geometry: &plan.geometry, ctx: &plan.shape_context };
                        camera_miss(scene, &tracing, surface, sky, ray, pixel, sample, st, None, counts, draws.as_deref_mut())?
                    }
                }
            };
            counts.samples += 1;
            // The camera sample's colour (the eye inside a solid), then stage 5.
            let colour = surface.camera_sample(&ray, found.as_ref(), rgb)?;
            each(pixel, colour);
        }
        Ok(())
    }

    /// Stage 2: camera rays, and where the lens moved a sample's ray in stage 1, that ray.
    /// point 3
    pub fn rays(&mut self, plan: &RenderPlan) {
        plan.camera.trace_points(&self.points, &mut self.rays);
        for (i, moved) in self.lens_rays.iter().enumerate() {
            if let Some(r) = moved {
                self.rays.set(i, r);
            }
        }
    }

    /// Stage 2 of the pass over edges: each of the seven rays is's camera ray for
    /// its picture point with depth of field off (from the eye through the point's place on
    /// the image plane, hits counting only beyond it), never a ray from a point of the lens,
    /// with no sample number and no draws for a lens. Kept apart from `rays` so that the
    /// first pass's depth of field (the lens of stage 1, `lens_places`) cannot reach
    /// these rays. As 5.0 their starting length takes (y² + z²) + x², not the first pass's
    /// grouping. point 3,
    pub fn edge_rays(&mut self, plan: &RenderPlan) {
        plan.camera.trace_edge_points(&self.points, &mut self.rays);
    }

    /// Stage 3: each ray against the scene, hits counting from the ray's starting length.
    pub fn hits(&mut self, plan: &RenderPlan) {
        self.hits.clear();
        if plan.walks_change_shading() {
            return;
        }
        for i in 0..self.rays.len() {
            if super::parallel::cancelled() {
                return; // the colours stage sees the stop first and ends the batch
            }
            let r = self.rays.get(i);
            let ray = Ray { origin: r.origin, dir: r.dir };
            let range = Range { near: r.start, far: f64::INFINITY };
            self.hits.hit.push(plan.geometry.scene.intersect(&ray, range, &plan.shape_context));
        }
    }
}

/// One surface on the path from a camera ray down the ray tree, while its further rays are
/// traced: its local colour, its children (a range of `TreeBuffers::rays`), the next child to
/// trace, where its children's brought-back values start, and the hit of the ray that led to
/// it (for its parent's join). (Further rays: depth first, a stack)
/// A node that queued an ambient ray (`gathering`) keeps its request for `resume`.
#[derive(Clone, Copy, Debug)]
struct TreeNode {
    local: [f32; 3],
    first: usize,
    count: usize,
    next: usize,
    backs: usize,
    generation: u32,
    hit: Option<Hit>,
    /// The request that shaded it: for `resume`, and for the air over its joined colour.
    /// point 5,
    request: SurfaceRequest,
    /// Its one child is the ambient ray, whose colour goes to `resume`, not to the join.
    gathering: bool,
    /// The march of the medium the ray that met it travelled in, laid over its colour last.
    laying: Option<Laying>,
    /// The wash's march of the ray that met it, taken when it was opened (`Some`) because the
    /// surface sends a blended ray straight on: the wash comes before that ray is sent. `None`:
    /// the wash is made when the node is finished.
    wash_early: Option<Option<crate::sky::wash::WashEffect>>,
    /// Its one child (a fuzzy surface's continuing ray) is sent before the surface is lit: when
    /// it comes back the rules light the surface (`resume`), and what it brought back is held
    /// to be joined last.
    lit_after: bool,
    held: Option<BroughtBack>,
}

/// The buffers of stage 4's depth-first walk, reused from ray to ray (no allocation per ray
/// once they have grown). (Further rays)
#[derive(Clone, Debug, Default)]
pub struct TreeBuffers {
    /// What one surface sends while it is shaded.
    sent: SecondaryQueue,
    /// The children of every node on the stack, each node's consecutive.
    rays: Vec<SecondaryRay>,
    /// Per child, its turn , beside `rays`.
    turns: Vec<RayTurn>,
    /// What the children traced so far brought back, each node's consecutive.
    backs: Vec<BroughtBack>,
    stack: Vec<TreeNode>,
}

/// What the walk traces against. (the per-sample path borrows the plan's
/// generator meanwhile)
pub(crate) struct Tracing<'a> {
    pub geometry: &'a GeometryBuild,
    pub ctx: &'a ShapeContext,
}

/// The queue `surface` or `resume` filled is either one ambient ray alone or rays of other
/// kinds. point 5 (the ambient ray before anything else of this surface)
fn gathers(sent: &SecondaryQueue) -> Result<bool, RenderError> {
    let ambient = sent.rays.iter().filter(|r| matches!(r.kind, SecondaryKind::Ambient | SecondaryKind::Start)).count();
    match ambient {
        0 => Ok(false),
        1 if sent.rays.len() == 1 => Ok(true),
        _ => Err(RenderError::Refused {
            entry: "SO-2.5b",
            reason: "an ambient ray queued with other rays (design: render.md, rules error)".into(),
        }),
    }
}

impl TreeBuffers {
    /// Shade `req`'s surface and put it on the stack with the rays it sends.
    #[allow(clippy::too_many_arguments)]
    fn open(
        &mut self,
        scene: &Scene,
        tracing: &Tracing<'_>,
        surface: &mut dyn SurfaceRules,
        sky: &mut dyn SkyRules,
        req: &SurfaceRequest,
        generation: u32,
        hit: Option<Hit>,
        path_origin: Vec3,
        counts: &mut RenderCounts,
        draws: Option<&mut CRand>,
    ) -> Result<(), RenderError> {
        // : as soon as the ray is traced, before its
        // surface is shaded.
        let mut draws = draws;
        let laying = march_gathering(scene, tracing, surface, sky, &req.ray, req.depth, &req.state, Some(&req.hit), req.premium, req.pixel, req.sample, path_origin, counts, draws.as_deref_mut())?;
        self.sent.clear();
        // : the surface's shadow rays draw from the render's sequence,
        // lent for the surface. (Shading that draws)
        let shaded = match draws.as_deref_mut() {
            Some(g) => {
                surface.swap_generator(g);
                let shaded = surface.surface(scene, req, &mut self.sent);
                surface.swap_generator(g);
                shaded
            }
            None => surface.surface(scene, req, &mut self.sent),
        };
        let local = to_binary32(shaded?);
        let gathering = gathers(&self.sent)?;
        let lit_after = self.sent.lit_after;
        let first = self.rays.len();
        self.rays.extend_from_slice(&self.sent.rays);
        self.turns.extend_from_slice(&self.sent.turns);
        let count = self.rays.len() - first;
        let backs = self.backs.len();
        // : the wash of a ray that meets a blended surface, with its
        // draws, comes before that surface's ray straight on is sent and before every draw of
        // that ray's tree; the surface has been lit (its draws made) by now.
        let wash_early = if self.sent.rays.iter().any(|r| r.kind == SecondaryKind::Blended) {
            Some(wash_effect_lent(surface, draws, req)?)
        } else {
            None
        };
        self.stack.push(TreeNode { local, first, count, next: 0, backs, generation, hit, request: *req, gathering, laying, wash_early, lit_after, held: None });
        Ok(())
    }

    /// The top node gathered `colour` with its ambient ray: the rules finish its surface, and
    /// its children become the further rays they now send. point 5
    fn resume(
        &mut self,
        scene: &Scene,
        surface: &mut dyn SurfaceRules,
        colour: [f32; 3],
        draws: Option<&mut CRand>,
    ) -> Result<(), RenderError> {
        let node = *self.stack.last().expect("a node is open");
        self.sent.clear();
        // the lighting draws from the render's generator, lent as for `surface`
        let lit = match draws {
            Some(g) => {
                surface.swap_generator(g);
                let lit = surface.resume(scene, &node.request, colour, &mut self.sent);
                surface.swap_generator(g);
                lit
            }
            None => surface.resume(scene, &node.request, colour, &mut self.sent),
        };
        let local = to_binary32(lit?);
        if gathers(&self.sent)? {
            return Err(RenderError::Refused {
                entry: "SO-2.5b",
                reason: "a second ambient ray from one surface (design: render.md, rules error)".into(),
            });
        }
        self.rays.truncate(node.first);
        self.turns.truncate(node.first);
        self.rays.extend_from_slice(&self.sent.rays);
        self.turns.extend_from_slice(&self.sent.turns);
        let top = self.stack.last_mut().expect("a node is open");
        top.local = local;
        top.count = self.rays.len() - node.first;
        top.next = 0;
        top.gathering = false;
        Ok(())
    }

    /// A child's colour comes back to the top node: to `resume` where it is the ambient ray,
    /// otherwise as its brought-back value for the join.
    fn child_back(
        &mut self,
        scene: &Scene,
        surface: &mut dyn SurfaceRules,
        ray: &SecondaryRay,
        hit: Option<&Hit>,
        colour: [f32; 3],
        draws: Option<&mut CRand>,
    ) -> Result<(), RenderError> {
        if self.stack.last().expect("a node is open").gathering {
            return self.resume(scene, surface, colour, draws);
        }
        let back = surface.brought_back(ray, hit, colour)?;
        // : the continuing ray of a fuzzy surface lit after it is back:
        // its colour is held for the join, which lays it last; the surface is lit now.
        let top = self.stack.last_mut().expect("a node is open");
        if top.lit_after {
            top.lit_after = false;
            top.held = Some(back);
            return self.resume(scene, surface, [0.0; 3], draws);
        }
        self.backs.push(back);
        Ok(())
    }
}

/// The most further rays one camera ray's tree sends as 5.0.
const FURTHER_RAY_CAP: u32 = 9_999;

/// The state a camera ray's path starts in. With the lens moving the ray, its path starts where
/// the ray began before the lens moved it: the eye.,
fn camera_state_of(surface: &dyn SurfaceRules, ray: &CameraRay, eye: Vec3, lens_acts: bool) -> Result<PathState, OpenItem> {
    let mut state = surface.camera_state(ray)?;
    if lens_acts && ray.origin != eye {
        state.air = AirPath::Continued { start: eye, before: 0.0 };
    }
    Ok(state)
}

/// The march of a ray's medium with the render's generator lent to the rules for its jittered
/// samples' draws, as for a surface.,;
/// (Shading that draws)
fn march_lent(
    surface: &mut dyn SurfaceRules,
    draws: Option<&mut CRand>,
    ray: &CameraRay,
    depth: u32,
    state: &PathState,
    hit: Option<&Hit>,
    premium: Option<PremiumSample>,
) -> Result<Option<Laying>, OpenItem> {
    match draws {
        Some(g) => {
            surface.swap_generator(g);
            let laying = surface.march(ray, depth, state, hit, premium);
            surface.swap_generator(g);
            laying
        }
        None => surface.march(ray, depth, state, hit, premium),
    }
}

/// The wash over a ray's colour with the render's generator lent to the rules for its draws, as
/// for a surface.,
#[allow(clippy::too_many_arguments)]
fn wash_lent(
    surface: &mut dyn SurfaceRules,
    draws: Option<&mut CRand>,
    ray: &CameraRay,
    depth: u32,
    state: &PathState,
    hit: Option<f64>,
    premium: Option<PremiumSample>,
    colour: [f32; 3],
) -> Result<[f32; 3], OpenItem> {
    match draws {
        Some(g) => {
            surface.swap_generator(g);
            let washed = surface.wash(ray, depth, state, hit, premium, colour);
            surface.swap_generator(g);
            washed
        }
        None => surface.wash(ray, depth, state, hit, premium, colour),
    }
}

/// The wash's march of the ray `req` met a surface with, its draws taken from the render's
/// generator, apart from the colour it is laid over.
fn wash_effect_lent(
    surface: &mut dyn SurfaceRules,
    draws: Option<&mut CRand>,
    req: &SurfaceRequest,
) -> Result<Option<crate::sky::wash::WashEffect>, OpenItem> {
    let (ray, depth, state, hit, premium) = (&req.ray, req.depth, &req.state, Some(req.hit.t), req.premium);
    match draws {
        Some(g) => {
            surface.swap_generator(g);
            let effect = surface.wash_effect(ray, depth, state, hit, premium);
            surface.swap_generator(g);
            effect
        }
        None => surface.wash_effect(ray, depth, state, hit, premium),
    }
}

/// The march of the medium a ray travels in, where it is a volume material's, with the render's
/// generator lent for its draws. Where the march's shaded samples ask for the ambient ray of true
/// ambience (point 5), each ray is turned, traced to its colour and handed back,
/// in the order asked, and the march is made again with those colours.
/// , point 5
/// (Revision of)
#[allow(clippy::too_many_arguments)]
fn march_gathering(
    scene: &Scene,
    tracing: &Tracing<'_>,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    ray: &CameraRay,
    depth: u32,
    state: &PathState,
    hit: Option<&Hit>,
    premium: Option<PremiumSample>,
    pixel: u32,
    sample: SampleTag,
    path_origin: Vec3,
    counts: &mut RenderCounts,
    mut draws: Option<&mut CRand>,
) -> Result<Option<Laying>, RenderError> {
    let laying = march_lent(surface, draws.as_deref_mut(), ray, depth, state, hit, premium)?;
    let asks = surface.march_asks();
    if asks.is_empty() {
        return Ok(laying);
    }
    let mut gathered = Vec::with_capacity(asks.len());
    for mut ask in asks {
        ask.pixel = pixel;
        ask.sample = sample;
        gathered.push(trace_ambient(scene, tracing, surface, sky, ask, premium, path_origin, counts, draws.as_deref_mut())?);
    }
    surface.march_gathered(gathered);
    Ok(march_lent(surface, draws, ray, depth, state, hit, premium)?)
}

/// An ambient ray a march's sample sent: turned about its direction (two draws), traced, and
/// its colour as it comes back (no weight). point 5,
#[allow(clippy::too_many_arguments)]
fn trace_ambient(
    scene: &Scene,
    tracing: &Tracing<'_>,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    mut r: SecondaryRay,
    premium: Option<PremiumSample>,
    path_origin: Vec3,
    counts: &mut RenderCounts,
    mut draws: Option<&mut CRand>,
) -> Result<[f32; 3], RenderError> {
    turned(&mut r.ray, RayTurn::Ambient, premium, draws.as_deref_mut())?;
    counts.further += 1;
    let ray = Ray { origin: r.ray.origin, dir: r.ray.dir };
    let range = Range { near: r.ray.start, far: f64::INFINITY };
    let found = tracing.geometry.scene.intersect(&ray, range, tracing.ctx);
    surface.ray_leaves(&mut r);
    match found {
        Some(hit) => {
            let child = SurfaceRequest {
                ray: r.ray,
                hit,
                object: tracing.geometry.scene_object[hit.object],
                pixel: r.pixel,
                sample: r.sample,
                depth: r.depth,
                total: r.total,
                state: r.state,
                premium,
            };
            let mut buffers = TreeBuffers::default();
            shade_tree(scene, tracing, surface, sky, &child, &mut buffers, counts, draws, path_origin)
        }
        None => ray_miss(scene, tracing, surface, sky, &r, premium, path_origin, counts, draws),
    }
}

/// The colour of a further ray that meets nothing: the march before the sky, the sky's colour
/// with the air of the ray's path, the wash, the march laid over it last.
/// , points 4 and 5,
#[allow(clippy::too_many_arguments)]
fn ray_miss(
    scene: &Scene,
    tracing: &Tracing<'_>,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    r: &SecondaryRay,
    premium: Option<PremiumSample>,
    path_origin: Vec3,
    counts: &mut RenderCounts,
    mut draws: Option<&mut CRand>,
) -> Result<[f32; 3], RenderError> {
    let laying = march_gathering(scene, tracing, surface, sky, &r.ray, r.depth, &r.state, None, premium, r.pixel, r.sample, path_origin, counts, draws.as_deref_mut())?;
    let mut colour = to_binary32(sky.sky(scene, &SkyRequest { ray: r.ray, path_origin, pixel: r.pixel, sample: r.sample, air: r.state.air, depth: r.depth, media: r.state.media })?);
    colour = wash_lent(surface, draws, &r.ray, r.depth, &r.state, None, premium, colour)?;
    if let Some(l) = laying {
        colour = l.over(colour);
    }
    Ok(colour)
}

/// The colour of a camera ray that meets nothing: the march of the medium it starts in (made
/// before the sky is asked), the sky's colour (a camera ray starts its own path), and
/// the march laid over it.,
#[allow(clippy::too_many_arguments)]
fn camera_miss(
    scene: &Scene,
    tracing: &Tracing<'_>,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    ray: CameraRay,
    pixel: u32,
    sample: SampleTag,
    state: PathState,
    premium: Option<PremiumSample>,
    counts: &mut RenderCounts,
    mut draws: Option<&mut CRand>,
) -> Result<[f32; 3], RenderError> {
    let laying = march_gathering(scene, tracing, surface, sky, &ray, 0, &state, None, premium, pixel, sample, ray.origin, counts, draws.as_deref_mut())?;
    let colour = to_binary32(sky.sky(scene, &SkyRequest { ray, path_origin: ray.origin, pixel, sample, air: state.air, depth: 0, media: state.media })?);
    // : the wash right after the air, the march over it
    let colour = wash_lent(surface, draws, &ray, 0, &state, None, premium, colour)?;
    Ok(laying.map_or(colour, |l| l.over(colour)))
}

/// The two draws a turn takes and the ray's direction after it. points 1, 2, 4, 5
fn turned(ray: &mut CameraRay, turn: RayTurn, premium: Option<PremiumSample>, draws: Option<&mut CRand>) -> Result<(), RenderError> {
    if turn == RayTurn::None {
        return Ok(());
    }
    let (Some(p), Some(rng)) = (premium, draws) else {
        return Err(RenderError::Refused {
            entry: "SO-2.5b",
            reason: "a ray turned in a sample without a number (design: render.md, rules error)".into(),
        });
    };
    let first = rng.next_u15();
    let cell = premium::cell_point(p.number, p.n, [first, rng.next_u15()]);
    ray.dir = match turn {
        RayTurn::None => ray.dir,
        RayTurn::Cone { radius } => {
            premium::cone_turn(ray.dir, premium::disc_point(radius, cell), premium::CONE_ORDERS)
        }
        RayTurn::Ambient => premium::ambient_direction(ray.dir, cell, premium::AMBIENT_ORDERS),
    };
    Ok(())
}

/// The colour of the surface a camera ray meets: its local colour, then each further ray it
/// sends, in the order sent, traced and shaded with every ray it leads to before the next
/// (depth first, on an explicit stack), then the join. A node's place in the tree is bounded
/// by `GENERATION_GUARD`. A child with a turn  takes its two draws from `draws`
/// immediately before it is traced; an ambient child's colour goes to `resume`, which
/// replaces the node's children by its further rays. (the join; the angle law's
/// order of rays),
/// (Further rays; Premium samples that draw in their trees)
#[allow(clippy::too_many_arguments)]
fn shade_tree(
    scene: &Scene,
    tracing: &Tracing<'_>,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    req: &SurfaceRequest,
    buffers: &mut TreeBuffers,
    counts: &mut RenderCounts,
    mut draws: Option<&mut CRand>,
    path_origin: Vec3,
) -> Result<[f32; 3], RenderError> {
    buffers.rays.clear();
    buffers.turns.clear();
    buffers.backs.clear();
    buffers.stack.clear();
    buffers.open(scene, tracing, surface, sky, req, 0, None, path_origin, counts, draws.as_deref_mut())?;
    // The further rays this camera ray's tree has sent so far.
    let mut sent_so_far = 0u32;
    loop {
        let top = *buffers.stack.last().expect("a node is open");
        if top.next < top.count {
            buffers.stack.last_mut().expect("a node is open").next += 1;
            let i = top.first + top.next;
            // : a cone's or the ambient ray's two draws, immediately
            // before the ray is traced.
            turned(&mut buffers.rays[i].ray, buffers.turns[i], req.premium, draws.as_deref_mut())?;
            let r = buffers.rays[i];
            let next = top.generation + 1;
            // A ray's depth is at most its generation (as 5.0 the blended ray keeps its
            // parent's depth).
            if r.kind == SecondaryKind::Shadow || r.depth > next {
                return Err(open::SECONDARY_RAYS.into());
            }
            if next > GENERATION_GUARD {
                return Err(RenderError::Refused {
                    entry: "SH-2.2",
                    reason: format!("a ray tree deeper than {GENERATION_GUARD} generations (design guard)"),
                });
            }
            // : as 5.0 a tree sends at most 9,999 further rays after the
            // camera ray (every kind counts); a ray past that returns black, untraced, its two
            // draws (taken above) already made.
            if tracing.ctx.version == crate::version::RenderAs::Bryce50 && sent_so_far >= FURTHER_RAY_CAP {
                buffers.child_back(scene, surface, &r, None, [0.0; 3], draws.as_deref_mut())?;
                continue;
            }
            sent_so_far += 1;
            counts.further += 1;
            let ray = Ray { origin: r.ray.origin, dir: r.ray.dir };
            let range = Range { near: r.ray.start, far: f64::INFINITY };
            let found = tracing.geometry.scene.intersect(&ray, range, tracing.ctx);
            // : the ray's own hit has been found, and counts.
            surface.ray_leaves(&mut buffers.rays[i]);
            let r = buffers.rays[i];
            match found {
                Some(hit) => {
                    let object = tracing.geometry.scene_object[hit.object];
                    let child = SurfaceRequest {
                        ray: r.ray,
                        hit,
                        object,
                        pixel: r.pixel,
                        sample: r.sample,
                        depth: r.depth,
                        total: r.total,
                        state: r.state,
                        premium: req.premium, // every ray of the sample carries s
                    };
                    buffers.open(scene, tracing, surface, sky, &child, next, Some(hit), path_origin, counts, draws.as_deref_mut())?;
                }
                None => {
                    let colour = ray_miss(scene, tracing, surface, sky, &r, req.premium, path_origin, counts, draws.as_deref_mut())?;
                    buffers.child_back(scene, surface, &r, None, colour, draws.as_deref_mut())?;
                }
            }
            continue;
        }
        // Every child traced: the join of the rays that are not blended, the air of the ray
        // that met the surface over it, then the blended ray's replacement (Further
        // rays: Order at a surface;, The blended material: after the haze), or a fuzzy
        // surface's continuing ray's (steps 2 to 4: o with the haze, then the mix). Then hand
        // the colour to the parent.
        let node = buffers.stack.pop().expect("a node is open");
        // : a continuing ray sent before the surface was lit is joined
        // after the rays the lighting sent (the mix with it comes last)
        if let Some(held) = node.held {
            buffers.backs.push(held);
        }
        let backs = &buffers.backs[node.backs..];
        let split = backs.iter().position(|b| b.replace.is_some()).unwrap_or(backs.len());
        let (joined, blended) = backs.split_at(split);
        let mut colour = if joined.is_empty() {
            node.local
        } else {
            to_binary32(surface.join(node.local.map(f64::from), joined)?)
        };
        colour = surface.over_joined(&node.request, colour)?;
        // : the wash right after the air; a ray
        // that meets a volume material's surface takes none (the ray that goes on was washed as any
        // ray), nor does a ray that meets a fuzzy solid's surface from inside (its colour is what
        // its continuing ray brings back, `Replace::Through`). The two mixes
        // (the blended ray's share, the fuzzy surface's) are laid over the washed colour; then
        // : the march is laid over the colour last.
        let through = blended.iter().any(|b| matches!(b.replace, Some(crate::shading::further::Replace::Through)));
        if !through && !surface.is_volume_surface(&node.request) {
            colour = match node.wash_early {
                Some(effect) => effect.map_or(colour, |e| e.over(colour)),
                None => wash_lent(
                    surface,
                    draws.as_deref_mut(),
                    &node.request.ray,
                    node.request.depth,
                    &node.request.state,
                    Some(node.request.hit.t),
                    req.premium,
                    colour,
                )?,
            };
        }
        if !blended.is_empty() {
            colour = to_binary32(surface.join(colour.map(f64::from), blended)?);
        }
        // : a surface that lays nothing (a
        // fuzzy one met from inside) has no light of its ray laid at it; a volume material's
        // surface lays its ray's light over what the continuing ray brought back
        if !through || surface.is_volume_surface(&node.request) {
            colour = surface.after_replacement(&node.request, colour)?;
        }
        if let Some(l) = node.laying {
            colour = l.over(colour);
        }
        buffers.backs.truncate(node.backs);
        buffers.rays.truncate(node.first);
        buffers.turns.truncate(node.first);
        let Some(parent) = buffers.stack.last() else { return Ok(colour) };
        let r = buffers.rays[parent.first + parent.next - 1];
        buffers.child_back(scene, surface, &r, node.hit.as_ref(), colour, draws.as_deref_mut())?;
    }
}

/// A rendered picture: levels in row order, each pixel red, green, blue.,
/// README ("The expected result")
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub levels: Vec<u8>,
}

impl Picture {
    pub fn png(&self) -> Vec<u8> {
        crate::sampling::png::encode_rgb8(self.width, self.height, &self.levels)
    }

    /// The SHA-256 of the levels, as `pictures.json` gives it.
    pub fn levels_sha256_hex(&self) -> String {
        crate::sha256::to_hex(&crate::sha256::sha256(&self.levels))
    }
}

/// Counts from a render, for reports and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderCounts {
    /// Camera samples of both passes; `surface + sky == samples`.
    pub samples: u64,
    pub surface: u64,
    pub sky: u64,
    /// Pixels the pass over edges refined (seven samples each).
    pub refined: u64,
    /// Further rays traced , not counted in `samples`.
    pub further: u64,
}

/// Pixels as they finish: `emit(x, y, levels)` gives one run of finished pixels of one row
/// (as many as `levels` holds thirds of), the runs of a batch in row order. Only pixels of
/// the region, and only once; a call with no pixels at all means "a part is complete" (flush
/// now) and comes from the multithreaded render only. (the live picture)
pub type PixelObserver<'a> = &'a mut dyn FnMut(u32, u32, &[u8]);

/// A serial render of a frame that draws gives up once it has made more draws than its budget
/// (`SharedDraws::Auto`; a count, not a time, so the outcome is the same on every machine): the caller
/// starts over with a place in the generator's sequence for every pixel.
fn too_slow(plan: &RenderPlan) -> Result<(), RenderError> {
    match plan.give_up_at {
        Some(limit) if crate::numerics::draws_made() > limit => {
            Err(RenderError::Refused { entry: "slow-draws", reason: "the exact order asks for too many draws".to_owned() })
        }
        _ => Ok(()),
    }
}

/// The same give-up, early: once `done` of `total` pixels of the first pass are made, a render whose draws so far
/// (`since` the pass began) project to more than three budgets for the whole frame gives up at once rather than
/// after the budget (a count, so the outcome is the same on every machine). The frames that finish inside the
/// budget keep their exact order: the wash's draws per pixel differ little across the picture.
fn too_slow_projected(plan: &RenderPlan, started: u64, done: u64, total: u64) -> Result<(), RenderError> {
    if let Some(limit) = plan.give_up_at {
        let budget = limit.saturating_sub(started);
        // a twentieth of the frame at least, so one dense corner does not decide
        if done * 20 >= total && done < total {
            let since = crate::numerics::draws_made().saturating_sub(started) as u128;
            if since * total as u128 > 3 * budget as u128 * done as u128 {
                return Err(RenderError::Refused {
                    entry: "slow-draws",
                    reason: "the exact order asks for too many draws".to_owned(),
                });
            }
        }
    }
    too_slow(plan)
}

/// The whole loop: stages 1 to 5, then the pass over edges when it is on.
pub fn render_picture(
    scene: &Scene,
    plan: &mut RenderPlan,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
) -> Result<(Picture, RenderCounts), RenderError> {
    render_picture_progress(scene, plan, surface, sky, None, &mut |_, _, _| {})
}

/// The whole loop, with the pixels handed to `emit` as each batch of them finishes, and an
/// optional order: `Some(visits)` renders those visits (a scattered order
/// blender-addon.md) in place of `plan.order()`. The finished picture does not depend on
/// either: levels are written per pixel from that pixel's own sum, so the picture is the
/// same bytes however the pixels are ordered and whenever they are handed out — as long as
/// no pixel's samples draw from the generator, which only the jittered and Premium places
/// and the lens do (the caller gives a scattered order at its risk
/// otherwise, and `lit::render_lit_streamed` does not: it scatters the exact pixels only
/// when the setting takes no draws).
pub fn render_picture_progress(
    scene: &Scene,
    plan: &mut RenderPlan,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    order: Option<Vec<PixelVisit>>,
    mut emit: PixelObserver<'_>,
) -> Result<(Picture, RenderCounts), RenderError> {
    let (w, h) = (plan.width, plan.height);
    crate::textures::reset_shared_points();
    sky.before_first_pixel(scene, &mut plan.generator)?; // point 3
    let mut sums = vec![PixelSum::default(); (w as usize) * (h as usize)];
    // The levels row by row: the pictures the render started from (the earlier work, or white),
    // each pixel a visit renders written at its visit, and with it the b by b square a visit of
    // a pass of block size b > 1 writes over the rest of its block, held to the region (later
    // passes write their own places over it). A pixel the render has not written keeps the
    // earlier work's level. point 6,
    let mut levels = plan.background.clone();
    // The pixels the render has written (a visit, or a square written over): the pass over
    // edges of the render on opening, as 3.0, passes by the others.
    let mut written = vec![false; sums.len()];
    // The pixels this render has visited: a pixel visited again (passes from the block size
    // 2^32 - 1 can ask for a place twice) starts its sum afresh.
    let mut visited = vec![false; sums.len()];
    let mut stages = Stages::new();
    let mut counts = RenderCounts::default();
    let post = plan.post.unwrap_or(PostOptions { gamma: false, dither: false });
    let dither = plan.dither_table.clone(); // held beside the plan's mutable uses in the loop
    let table = dither.as_deref();
    let region = plan.region;
    let mut normal = plan.order().peekable();
    let mut given = order.map(|v| v.into_iter().peekable());
    let mut visits: Vec<PixelVisit> = Vec::with_capacity(PIXELS_PER_BATCH);
    let mut run = [0u8; 3];
    let started = crate::numerics::draws_made();
    let total_pixels = (region.right.saturating_sub(region.left) as u64) * (region.bottom.saturating_sub(region.top) as u64);
    let mut done_pixels: u64 = 0;
    loop {
        super::parallel::stop_if_cancelled()?;
        too_slow_projected(plan, started, done_pixels, total_pixels)?;
        visits.clear();
        // A batch holds no pixel twice: a visit to a pixel the batch holds or an earlier batch
        // visited ends the batch, and (alone in the next) restarts that pixel's sum.
        let mut fresh = false;
        while visits.len() < PIXELS_PER_BATCH {
            let next = match &mut given {
                Some(it) => it.peek().copied(),
                None => normal.peek().copied(),
            };
            let Some(v) = next else { break };
            let i = (v.y * w + v.x) as usize;
            if visited[i] {
                if !visits.is_empty() {
                    break;
                }
                sums[i] = PixelSum::default();
                fresh = true;
            }
            visited[i] = true;
            visits.push(v);
            match &mut given {
                Some(it) => it.next(),
                None => normal.next(),
            };
            if fresh {
                break;
            }
        }
        if visits.is_empty() {
            break;
        }
        done_pixels += visits.len() as u64;
        if plan.draws_in_trees() || surface.draws() {
            // Premium samples whose trees draw, or any render whose shading draws : one
            // sample at a time (Effects); stage 5's sum as below.
            // (the per-sample path; Shading that draws)
            stages.premium_samples(scene, plan, surface, sky, &visits, &mut counts, |pixel, rgb| {
                sums[pixel as usize].add(rgb)
            })?;
        } else {
            stages.places(plan, &visits);
            stages.rays(plan);
            stages.hits(plan);
            // Stage 4: colour, in the order stage 1 made the samples; stage 5: the sum, in the
            // order the samples are taken.
            stages.colours(scene, plan, surface, sky, &mut counts, None, |pixel, rgb| {
                sums[pixel as usize].add(rgb)
            })?;
        }
        // Every visited pixel is finished by its own visit (each pass takes the places the
        // passes before left out): write its levels now, and hand them out, one pixel a
        // run. (the live picture)
        for v in &visits {
            let i = (v.y * w + v.x) as usize;
            let c = sums[i].colour(plan.scale);
            for (k, ch) in c.into_iter().enumerate() {
                run[k] = channel_level(plan.version, ch, v.x, v.y, post, table);
            }
            let (x1, y1) = if v.block > 1 {
                (v.x.saturating_add(v.block).min(region.right), v.y.saturating_add(v.block).min(region.bottom))
            } else {
                (v.x + 1, v.y + 1)
            };
            for y in v.y..y1 {
                for x in v.x..x1 {
                    let j = (y * w + x) as usize;
                    levels[j * 3..j * 3 + 3].copy_from_slice(&run);
                    written[j] = true;
                }
            }
            emit(v.x, v.y, &run);
        }
    }
    // The passes after the first run in batches; their shading draws from the plan's generator,
    // lent and written back . (Shading that draws)
    let mut generator = plan.generator;
    if let PictureWay::CornerBlocks { offset } = plan.way {
        corner_blocks(scene, plan, offset, post, surface, sky, &mut stages, &mut counts, &mut levels, &mut generator)?;
    }
    if let Some(pass) = plan.edge_pass {
        let only_written = plan.skip_unwritten.then_some(&written[..]);
        edge_pass_progress(
            scene, plan, pass, post, surface, sky, &mut stages, &mut counts, &mut levels, only_written, &mut emit,
            &mut generator,
        )?;
    }
    plan.generator = generator;
    Ok((Picture { width: w, height: h, levels }, counts))
}

/// 's blocks traced at their corners, in rounds of one side each (4, 2, 1): each
/// round's places not yet traced go through stages 1 to 4 in batches; each ray's colour is
/// made into levels as a pixel of one sample (as 5.0 with gamma or
/// dithering, the implementer's choice: as that pixel would be, at the place's column and
/// row); then the round's blocks are tested and divided. Last, the
/// undivided blocks are filled and the traced places inside the picture written over the
/// earlier levels.
#[allow(clippy::too_many_arguments)]
fn corner_blocks(
    scene: &Scene,
    plan: &RenderPlan,
    offset: f64,
    post: PostOptions,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    stages: &mut Stages,
    counts: &mut RenderCounts,
    levels: &mut [u8],
    generator: &mut CRand,
) -> Result<(), RenderError> {
    let mut way = CornerBlocks::new(plan.width, plan.height, plan.region);
    let stride = way.stride();
    let mut traced: Vec<(u32, [f32; 3])> = Vec::with_capacity(1);
    let table = plan.dither_table.as_deref();
    // The blocks are taken depth first, a block's corners in the order it asks for them, each
    // ray traced alone with every ray it leads to before the next is asked for (the order
    // decides which surface of a material is met first,, and as 5.0 which draws each
    // ray's shading takes).
    way.walk::<RenderError>(&mut |places, out| {
        out.clear();
        for &place in places {
            super::parallel::stop_if_cancelled()?;
            stages.corner_places(stride, offset, &[place]);
            stages.rays(plan);
            stages.hits(plan);
            traced.clear();
            stages.colours(scene, plan, surface, sky, counts, Some(&mut *generator), |p, rgb| traced.push((p, rgb)))?;
            let mut sum = PixelSum::default();
            for &(p, rgb) in &traced {
                debug_assert_eq!(p, place);
                sum.add(rgb);
            }
            let c = sum.colour(plan.scale);
            let (x, y) = (place % stride, place / stride);
            out.push(c.map(|ch| channel_level(plan.version, ch, x, y, post, table)));
        }
        Ok(())
    })?;
    way.finish(levels, None);
    Ok(())
}

/// The pass over edges on the finished first-pass levels: every pixel is tested against
/// the first-pass levels (in a render that goes on, the whole picture's, the earlier work's
/// included: point 4); the pixels on an edge are visited row by
/// row from the top, each row from the left, and each gets its seven rays (in the order of
/// `aa-offsets`) and its refined level, which changes only that pixel. Nothing here draws
/// from the generator: the places are fixed and only the shading of the rays could draw
/// (none as 3.0 and 3.1; none in the rules built as 5.0).
/// (questions 3 to 6), question 2. With `emit`, each refined pixel
/// goes to it as it is written (the live picture).
#[allow(clippy::too_many_arguments)]
fn edge_pass_progress(
    scene: &Scene,
    plan: &RenderPlan,
    pass: super::plan::EdgePass,
    post: PostOptions,
    surface: &mut dyn SurfaceRules,
    sky: &mut dyn SkyRules,
    stages: &mut Stages,
    counts: &mut RenderCounts,
    levels: &mut [u8],
    only_written: Option<&[bool]>,
    emit: PixelObserver<'_>,
    generator: &mut CRand,
) -> Result<(), RenderError> {
    let w = plan.width;
    let mut mask = edge_mask_in(levels, w, plan.height, plan.region, pass.limit);
    // (the render on opening, as 3.0: a pixel the render has not written is not
    // tested and not refined; its levels were still read above for its neighbours' tests).
    // Variants (as 3.0 a tested pixel whose right, lower or lower-right neighbour
    // the render has not written is on an edge, whatever the levels)
    if let Some(written) = only_written {
        let r = plan.region;
        let (wu, bottom, right) = (w as usize, r.bottom.min(plan.height), r.right.min(w));
        for y in r.top..bottom.saturating_sub(1) {
            for x in r.left..right.saturating_sub(1) {
                let i = y as usize * wu + x as usize;
                if !written[i] {
                    mask[i] = false;
                } else if !written[i + 1] || !written[i + wu] || !written[i + wu + 1] {
                    mask[i] = true;
                }
            }
        }
    }
    let on_edge: Vec<u32> = (0..mask.len() as u32).filter(|&p| mask[p as usize]).collect();
    let rule = RefineRule { version: plan.version, post, dither_table: plan.dither_table.as_deref() };
    let mut extra: Vec<[f32; 3]> = Vec::with_capacity(PIXELS_PER_BATCH * EXTRA_RAYS);
    for batch in on_edge.chunks(PIXELS_PER_BATCH) {
        super::parallel::stop_if_cancelled()?;
        too_slow(plan)?;
        stages.edge_places(w, &pass.offsets, batch);
        stages.edge_rays(plan); // point 3
        stages.hits(plan);
        extra.clear();
        stages.colours(scene, plan, surface, sky, counts, Some(&mut *generator), |_, rgb| extra.push(rgb))?;
        for (k, &p) in batch.iter().enumerate() {
            let seven: [[f32; 3]; EXTRA_RAYS] =
                extra[k * EXTRA_RAYS..(k + 1) * EXTRA_RAYS].try_into().expect("seven rays a pixel");
            let i = p as usize * 3;
            let first = [levels[i], levels[i + 1], levels[i + 2]];
            levels[i..i + 3].copy_from_slice(&rule.pixel(first, &seven, p % w, p / w));
            emit(p % w, p / w, &levels[i..i + 3]); // (the live picture)
            counts.refined += 1;
        }
    }
    Ok(())
}
