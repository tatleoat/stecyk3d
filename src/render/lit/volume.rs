// SPDX-License-Identifier: MIT
//! Volume materials in the render: which materials are volumes, the march of the medium a ray
//! travels in (laid over its colour by the stage), a volume material's surface and the ray
//! that goes on through it, and the marches of shadow rays.
//!
//! (the 5.0 rules: depth inside, Uniform Density, the weight, the join, the ray that
//! goes on through a surface, shadow rays' depth)
//! (revised at: Volume materials,
//! A sample's material, A sample's colour (the shaded kind's shadows, the lit kind's lights and
//! shadow rays), The order of the evaluations, Where marches are made, The weight a ray comes
//! back with, A volume material's surface, Shadow rays, Open),,
//! (a sample's
//! texture point)
//! (Volume materials : In the render)


use super::*;
use crate::geometry::Placement;
use crate::shading::lighting::{LightSource as Source, SurfacePoint};
use crate::shading::volume::{
    self as vol, After, DepthShape, Laying, MarchRay, MarchSite, SampleChannels, VolumeKind, VolumeNumbers,
    VolumeOrders, bits, march, stretch_ends,
};

/// Where a volume material's channels come from: each channel's A value, or's
/// evaluation at each sample.,
#[derive(Clone, Debug, PartialEq)]
pub(super) enum VolumeChannels {
    Plain(SampleChannels),
    Textured(Box<TexturedMaterial>),
}

/// A volume material on a shape object.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VolumeMaterial {
    /// The kinds the flag word has, marched in turn (flat, shaded, lit); the last one's colour
    /// and opacity are the march's.
    kinds: Vec<VolumeKind>,
    flags: u32,
    numbers: VolumeNumbers,
    depth: DepthShape,
    channels: VolumeChannels,
}

/// Uniform Density and Sky Integration, the flag word's bits 0x1000000 and 0x2000000 [format].
const UNIFORM_DENSITY: u32 = 0x100_0000;
const SKY_INTEGRATION: u32 = 0x200_0000;
/// Jittered sample places and the flat kind lit by the lights.
const JITTERED: u32 = 0x80_0000;
const FLAT_LIT: u32 = 0x40_0000;

/// The depth inside of a document shape. (The depth inside: the sphere; the
/// cylinder, the torus, the cube, the cone and the pyramid 1; the slab's own), (as 5.0
/// the cylinder's and the torus's own)
fn depth_of_shape(shape: &crate::scene::Shape, version: RenderAs) -> Result<DepthShape, OpenItem> {
    use crate::scene::Shape as S;
    let five = version == RenderAs::Bryce50;
    match shape {
        S::Sphere => Ok(DepthShape::Sphere),
        // ; as 5.0 their own depth is's, not
        // implemented here (refused)
        S::Cube | S::Cone | S::Pyramid if !five => Ok(DepthShape::Whole),
        S::Cube => Ok(DepthShape::Cube),
        S::Cone => Ok(DepthShape::Cone),
        S::Pyramid => Ok(DepthShape::Pyramid),
        S::Slab => Ok(DepthShape::Slab),
        S::Cylinder if five => Ok(DepthShape::Cylinder),
        S::Torus { tube_radius, ring_radius } if five => Ok(DepthShape::Torus { tube: *tube_radius, ring: *ring_radius }),
        S::Cylinder | S::Torus { .. } => Ok(DepthShape::Whole),
        _ => Err(vol::open::SHAPE),
    }
}

/// The same for a geometry shape.
fn depth_of(shape: &Shape, version: RenderAs) -> Result<DepthShape, OpenItem> {
    let five = version == RenderAs::Bryce50;
    match shape {
        Shape::Sphere => Ok(DepthShape::Sphere),
        Shape::Cube | Shape::Cone | Shape::Pyramid if !five => Ok(DepthShape::Whole),
        Shape::Cube => Ok(DepthShape::Cube),
        Shape::Cone => Ok(DepthShape::Cone),
        Shape::Pyramid => Ok(DepthShape::Pyramid),
        Shape::Slab => Ok(DepthShape::Slab),
        // : the radial light's own object is the sphere's; the
        // own objects of the other lights have 1 everywhere (as 3.0 and 3.1); as 5.0 theirs is Open
        Shape::Light(l) if l.shape == crate::geometry::light::LightShape::Sphere => Ok(DepthShape::Sphere),
        // revised (beam-light-pictures): the other four kinds' objects have the depth 1
        // everywhere as 3.0, 3.1 and 5.0 (as 5.0 unlike the plain cone, cube, cylinder, pyramid)
        Shape::Light(_) => Ok(DepthShape::Whole),
        Shape::Cylinder if five => Ok(DepthShape::Cylinder),
        Shape::Torus { tube, ring } if five => Ok(DepthShape::Torus { tube: *tube, ring: *ring }),
        Shape::Cylinder | Shape::Torus { .. } => Ok(DepthShape::Whole),
        // : a terrain's and a mesh's depth inside is 1 everywhere, in every version
        Shape::Terrain(_) | Shape::StoredMesh(_) | Shape::Mesh(_) => Ok(DepthShape::Whole),
        // : the object standing for every set and a tree's wood
        Shape::MetaSet | Shape::Tree { .. } => Ok(DepthShape::Field),
        // : a leaf's shape object is a mesh, depth 1
        Shape::TreeLeaves { .. } => Ok(DepthShape::Whole),
        _ => Err(vol::open::SHAPE),
    }
}

/// A plain volume material's channels: each channel's A value (Volume density, channel 12,
/// included).,
fn plain_channels(part: &MaterialPart) -> SampleChannels {
    let [diffuse, ambient, specular, halo, _] = part.colours.map(|c| c.a);
    let v = part.values.map(|c| f64::from(c.a));
    SampleChannels {
        light: PlainMaterial {
            diffuse,
            diffusion: v[0],
            ambient,
            ambience: v[1],
            specular,
            specularity: v[2],
            halo,
            transparency: v[3],
            metallicity: v[6],
        },
        density_value: v[7],
    }
}

/// The volume material of document object `object`, `None` where its material has no volume
/// bit; refused (Open) as 5.0, with two or three kinds, on a shape whose depth inside is Open
/// or an object that is not a shape, with numbers outside the Limits. (Volume
/// materials; Applies to; The depth inside; Limits; Open)
pub(super) fn volume_material(scene: &Scene, version: RenderAs, object: usize) -> Option<Result<VolumeMaterial, OpenItem>> {
    let m = scene.objects[object].material?;
    let part = &scene.materials[m].part;
    let kinds = VolumeKind::kinds(part.flags);
    if kinds.is_empty() {
        return None;
    }
    let make = || -> Result<VolumeMaterial, OpenItem> {
        // : for a terrain or a mesh as the medium the depth
        // inside is 1 everywhere, in all three versions; a terrain that encloses no volume
        // (point 1) is no medium (no march is made in it, the solid rules see to that).
        let depth = match scene.objects[object].body {
            Body::Terrain(_) | Body::MeshResource(_) | Body::Mesh(_) => DepthShape::Whole,
            // : a metaball's material (it leads a set)
            // and a tree's wood are marched in the object standing for every set, whose depth
            // inside is the sum of the fields of the set last found or kept
            Body::Metaball { .. } | Body::Tree(_) => DepthShape::Field,
            // : the shape object is a mesh ,
            // whose depth inside is 1 everywhere
            Body::TreeLeaves(_) => DepthShape::Whole,
            Body::Shape(shape) => depth_of_shape(&shape, version)?,
            // : a visible light's own object
            Body::RadialLight(l) => match &scene.lights[l] {
                crate::scene::Light::Radial(r) => match r.kind() {
                    crate::scene::LightKind::Radial => DepthShape::Sphere,
                    // : depth 1 in every version
                    _ => DepthShape::Whole,
                },
                _ => return Err(vol::open::SHAPE),
            },
            _ => return Err(vol::open::SHAPE),
        };
        let numbers = VolumeNumbers {
            sigma: f64::from(part.edge_softness()),
            phi: f64::from(part.fuzzy_factor()),
            quality: f64::from(part.quality()),
        };
        // : outside the Lab's ranges the formulas hold as
        // written; no clamp is made (the original's pictures: volume-range-pictures).
        let channels = if channels_evaluated(part.channel_words(), scene.settings.switches) {
            VolumeChannels::Textured(Box::new(TexturedMaterial::new(scene, m, version)?))
        } else {
            VolumeChannels::Plain(plain_channels(part))
        };
        Ok(VolumeMaterial { kinds, flags: part.flags, numbers, depth, channels })
    };
    Some(make())
}

/// What a shadow ray reads of a volume material's object: no share; its solid's medium (F = 1,
/// Φ'); `None` where the material has no volume bit. (Shadow rays; The weight a
/// ray comes back with; The medium colour)
pub(super) fn shadow_surface(
    part: &MaterialPart,
    shape: &Shape,
    shape_object: bool,
    encloses: bool,
    version: RenderAs,
) -> Option<Result<ShadowSurface, OpenItem>> {
    if VolumeKind::kinds(part.flags).is_empty() {
        return None;
    }
    let make = || -> Result<ShadowSurface, OpenItem> {
        // : a terrain or a mesh has the depth inside 1 in all
        // three versions; a terrain that encloses no volume is no medium (no march is made in it)
        let field = matches!(shape, Shape::Terrain(_) | Shape::StoredMesh(_) | Shape::Mesh(_) | Shape::MetaSet | Shape::Tree { .. } | Shape::TreeLeaves { .. });
        if !shape_object && !field && !matches!(shape, Shape::Light(_)) {
            return Err(vol::open::SHAPE);
        }
        depth_of(shape, version)?;
        let (law, tint) = super::super::media::solid_medium(part, shape);
        Ok(ShadowSurface {
            transparency: 0.0,
            colour: [0.0; 3],
            textured: false,
            refraction: 1.0,
            blended: false,
            medium: encloses.then_some(law),
            fuzzy: None,
            volume: Some(part.flags),
            tint,
            word: part.flags,
            own_word: part.flags,
        })
    };
    Some(make())
}

/// Bits 0x2000 and 0x80: a shadow ray's stretch in the solid is as one outside every object.
pub(super) fn no_shadow_medium(flags: u32) -> bool {
    flags & (bits::ADDS | bits::NO_SHADOW_MEDIUM) != 0
}

/// What the shaded and the lit kinds' samples light with: the surface rules, and where their
/// shadow rays start.
struct SiteLighting<'a> {
    surface: &'a PlainSurface,
    /// The shaded kind: as for a hit of the ray marched at X.
    shaded: ShadowStart,
    /// Whether the shaded kind's shadow rays leave X moved along N (the ray met a surface) or
    /// X itself, counting from 0.0001 (it met nothing).
    shaded_offset: bool,
    /// The lit kind: X itself, in the medium of the ray marched.
    lit: ShadowStart,
    /// What a shaded sample does where asks for the gathered light of true ambience.
    ambient: Ambient,
}

/// What a shaded sample does where asks for the gathered light (point 5).
/// , point 5
#[derive(Clone, Copy, Debug)]
enum Ambient {
    /// The ray marched carries no number: nothing is gathered.
    Unnumbered,
    /// The rays the path has gone through before the sample are not below the maximum ray depth:
    /// no ray, the colour gathered is black.
    TooDeep,
    /// The ambient ray is sent from the sample's place; the state is the ray marched's.
    Asks(PathState),
}

/// What a Sky Integration march lays over each sample: the air of the sky, then the lit air of
/// the ray marched (the same V for every sample)., (As
/// 5.0),
struct SkyOver<'a> {
    surface: &'a PlainSurface,
    /// The ray marched, its depth, its path state and its own hit.
    ray: CameraRay,
    depth: u32,
    state: PathState,
    hit: Option<f64>,
    /// V, gathered when the first sample asks.
    lit: Option<Option<[f32; 3]>>,
}

/// The scene as one march sees it.
struct RenderSite<'a> {
    /// Sky Integration: `Some` where the material has bit 0x2000000 (as 5.0), the ray is not a
    /// shadow ray.
    sky: Option<SkyOver<'a>>,
    version: RenderAs,
    placement: &'a Placement,
    map_object: MapObject,
    material: &'a VolumeMaterial,
    tables: TextureTables<'a>,
    switches: u32,
    direction: Vec3,
    /// The ray's origin (the distance the textures are given as 5.0).
    origin: Vec3,
    /// The depth of the ray marched (its samples' shadow rays' marches take it).
    ray_depth: u32,
    lighting: Option<SiteLighting<'a>>,
    /// The render's generator for jittered sample places.
    generator: Option<&'a Cell<CRand>>,
    sets: Option<&'a crate::geometry::metaball::MetaScene>,
}

impl<'a> RenderSite<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        version: RenderAs,
        geometry: &'a GeometryBuild,
        solid: usize,
        material: &'a VolumeMaterial,
        tables: TextureTables<'a>,
        switches: u32,
        direction: Vec3,
        origin: Vec3,
        ray_depth: u32,
        lighting: Option<SiteLighting<'a>>,
        generator: Option<&'a Cell<CRand>>,
    ) -> RenderSite<'a> {
        let o = &geometry.scene.objects[solid];
        let (low, high) = own_box(&o.shape);
        let map_object = MapObject { rows: o.placement.rows(), low, high, sphere: matches!(o.shape, Shape::Sphere) };
        RenderSite { sky: None, version, placement: &o.placement, map_object, material, tables, switches, direction, origin, ray_depth, lighting, generator, sets: geometry.scene.metaballs.as_ref() }
    }

    fn lighting(&self) -> &SiteLighting<'a> {
        self.lighting.as_ref().expect("a shadow ray's march lights nothing (SH-2.4)")
    }
}

/// 's shadow queries with the offset gives the shaded kind's samples.
struct Offset<'b, 'a> {
    inner: &'b mut ShadowTracer<'a>,
    offset: bool,
}

impl ShadowSource for Offset<'_, '_> {
    fn shadow(&mut self, q: &ShadowQuery) -> Shadow {
        self.inner.shadow(&ShadowQuery { offset: self.offset, ..*q })
    }
}

impl MarchSite for RenderSite<'_> {
    fn to_own(&self, p: Vec3) -> Vec3 {
        self.placement.point_to_local(p)
    }

    fn to_world(&self, x: Vec3) -> Vec3 {
        self.placement.to_world(x)
    }

    fn normal_to_world(&self, n: Vec3) -> Vec3 {
        self.placement.carry_normal(n, self.version)
    }

    /// (A sample's material: as at a hit whose point is X and x, whose normal is
    /// −d, whose distance is s, whose u, v and directions are 0)
    fn channels(&mut self, world: Vec3, own: Vec3, s: f64) -> Result<SampleChannels, OpenItem> {
        match &self.material.channels {
            VolumeChannels::Plain(c) => Ok(*c),
            VolumeChannels::Textured(t) => {
                let d = self.direction;
                let n = [-d[0], -d[1], -d[2]];
                let hit = MapHit { world, object: own, normal: n, u: 0.0, v: 0.0, du: [0.0; 3], dv: [0.0; 3] };
                let ch = ChannelHit { height: world[1], top: f64::NAN, normal: n, distance: s };
                let at = t.at_hit(self.version, &self.map_object, &hit, &ch, self.switches, self.tables)?;
                Ok(SampleChannels {
                    light: PlainMaterial {
                        diffuse: at.diffuse,
                        diffusion: at.diffusion,
                        ambient: at.ambient,
                        ambience: at.ambience,
                        specular: at.specular,
                        specularity: at.specularity,
                        halo: at.specular_halo,
                        transparency: at.transparency,
                        metallicity: at.metallicity,
                    },
                    density_value: at.volume_density,
                })
            }
        }
    }

    /// (the shaded kind: at X, both normals N, Transparency 0,
    /// s0 black, the flag word as it is; its shadows's as for a hit of the ray marched)
    fn shade(&mut self, world: Vec3, normal: Vec3, channels: &SampleChannels) -> Result<[f32; 3], OpenItem> {
        let l = self.lighting();
        let surface = l.surface;
        let lights = surface.lights_at(world)?;
        let at = SurfacePoint { point: world, ray_direction: self.direction, geometric_normal: normal, shading_normal: normal };
        let m = PlainMaterial { transparency: 0.0, ..channels.light };
        let inputs = SurfaceInputs { flags: self.material.flags, start: [0.0; 3], gathered: None };
        let mut tracer = surface.tracer(l.shaded, self.ray_depth);
        let mut shadows = Offset { inner: &mut tracer, offset: l.shaded_offset };
        let orders = DotOrders::of(self.version);
        let mut terms = light_terms(self.version, orders, &at, &m, &inputs, &surface.scene_light, &lights, &mut shadows);
        // : as 3.1 a sample whose normal is not a number is lit by
        // the ambient term alone: no light reaches it (every light's lambda is not a number)
        if normal.iter().any(|c| c.is_nan()) {
            terms.sum = inputs.start;
        }
        if let Some(e) = tracer.error {
            return Err(e);
        }
        // , point 5: the ambient ray from the sample's place X
        // with the sample's normal N as the normal g, by the ray marched. The ray is traced by the
        // stage between two passes of the march : pass 1 notes the ask and goes
        // on with black, pass 2 is given what each ask brought back, in the order asked.
        let gathered = if !terms.gathered_asked {
            None
        } else {
            match l.ambient {
                Ambient::Unnumbered => None,
                Ambient::TooDeep => Some([0.0; 3]),
                Ambient::Asks(state) => {
                    let mut given = surface.march_given.borrow_mut();
                    Some(match given.as_mut() {
                        Some(queue) => queue.pop_front().expect("pass 2 meets the asks pass 1 made"),
                        None => {
                            // Open: the order of the draws of the samples' own places and shadow
                            // rays against the ambient rays' (the original's is depth first)
                            if self.material.flags & JITTERED != 0 || surface.radii.iter().any(|&(r, _)| r != 0.0) {
                                return Err(vol::open::SHADED_AMBIENT);
                            }
                            surface.march_asks.borrow_mut().push(SecondaryRay {
                                kind: SecondaryKind::Ambient,
                                ray: CameraRay { origin: world, dir: normal, start: further::RAY_START },
                                weight: [1.0; 3],
                                total: [1.0; 3],
                                depth: self.ray_depth,
                                pixel: 0,
                                sample: stages::SampleTag::First(0),
                                state: PathState { media: state.media, tir_count: state.tir_count, rays: state.rays + 1, air: AirPath::Own, set_showing: state.set_showing },
                            });
                            [0.0; 3]
                        }
                    })
                }
            }
        };
        Ok(terms.finish(self.version, &m, &surface.scene_light, gathered).colour)
    }

    fn sky_integrated(&self) -> bool {
        self.sky.is_some()
    }

    /// : the sample's colour replaced by what the air as 5.0 gives
    /// it (the fog, then the haze: L the world distance from the ray's origin to X plus the path
    /// before this ray, s the path's start height), then the lit air of the ray marched added.
    fn sky_over(&mut self, world: Vec3, colour: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        let sky = self.sky.as_mut().expect("asked only with Sky Integration");
        let mut colour = colour;
        if let Some(air) = &sky.surface.air {
            let d: Vec3 = std::array::from_fn(|k| world[k] - sky.ray.origin[k]);
            let l = crate::numerics::sqrt_as(self.version, (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1]);
            let along = sky.state.air.to_hit(sky.ray.origin[1], sky.ray.dir, l);
            colour = air.over(self.version, colour, &along);
        }
        if let Some(lit) = &sky.surface.lit_air {
            if sky.lit.is_none() {
                sky.lit = Some(lit.along_ray(&sky.ray, sky.hit, sky.depth, sky.state.air, sky.state.media)?);
            }
            if let Some(Some(v)) = sky.lit {
                colour = crate::sky::lit_air::LitAir::over(colour, v);
            }
        }
        Ok(colour)
    }

    /// : the next draw of the render's generator
    fn draw(&mut self) -> Result<u32, OpenItem> {
        let Some(cell) = self.generator else { return Err(vol::open::JITTER_WITHOUT_GENERATOR) };
        let mut g = cell.get();
        let r = g.next_u15();
        cell.set(g);
        Ok(u32::from(r))
    }

    fn light_count(&self) -> usize {
        self.lighting().surface.lit.len()
    }

    fn shadow_setting(&self) -> f64 {
        self.lighting().surface.scene_light.shadow_setting
    }

    fn light_range(&self, i: usize) -> f64 {
        self.lighting().surface.lit[i].bryce5.range
    }

    fn light_shadow_share(&self, i: usize) -> (f64, [f32; 3]) {
        let b = self.lighting().surface.lit[i].bryce5;
        (b.shadow_share, b.shadow_colour)
    }

    fn sun_place(&self) -> Vec3 {
        self.lighting().surface.sun_place()
    }

    /// (The distance the textures are given: the world distance from the ray's
    /// origin to the sample's place, R((Δx² + Δz²) + Δy²) for the flat and the lit kinds and
    /// R((Δz² + Δy²) + Δx²) for the shaded kind)
    fn texture_distance(&self, kind: VolumeKind, world: Vec3, s: f64) -> f64 {
        if self.version != RenderAs::Bryce50 {
            return s;
        }
        let d: Vec3 = std::array::from_fn(|k| world[k] - self.origin[k]);
        let sq: Vec3 = std::array::from_fn(|k| d[k] * d[k]);
        let sum = match kind {
            VolumeKind::Shaded => (sq[2] + sq[1]) + sq[0],
            VolumeKind::Flat | VolumeKind::Lit => (sq[0] + sq[2]) + sq[1],
        };
        crate::numerics::sqrt_as(self.version, sum)
    }

    /// : the sum of the fields of the set last found or kept, 0 where
    /// there is none (or no set in the scene)
    fn field_depth(&self, world: Vec3) -> f64 {
        self.sets.map_or(0.0, |ms| ms.depth_at(world))
    }

    fn light(&self, i: usize) -> Source {
        let surface = self.lighting().surface;
        let mut source = surface.lit[i].source;
        // : with the wash the sun's intensity is the sun's light part's
        if let (Some(sun), true) = (surface.wash, i + 1 == surface.lit.len()) {
            source.intensity = sun.light_intensity;
        }
        source
    }

    fn beam_factor_at(&self, i: usize, world: Vec3) -> Result<Option<f64>, OpenItem> {
        let surface = self.lighting().surface;
        let Some(b) = &surface.beams[i] else { return Ok(Some(1.0)) };
        let u = crate::shading::gel::light_space_as(self.version, &b.placement, world);
        crate::shading::lights::beam_factor(self.version, b.beam, u, ConeOrder::of(self.version))
    }

    fn light_colour_at(&mut self, i: usize, world: Vec3) -> Result<[f32; 3], OpenItem> {
        let l = self.lighting();
        let surface = l.surface;
        // : the sun's colour at the sample is the wash's light there
        if let Some(colour) = surface.sun_colour_in_march(i, world, self.ray_depth, l.lit.incoming) {
            return colour;
        }
        match &surface.gels[i] {
            Some(g) => g.colour_at(self.version, world, surface.scene_light.switches, surface.tables.view()),
            None => Ok(surface.lit[i].source.colour),
        }
    }

    /// (the lit kind, step 2: a shadow ray of toward the light, leaving X
    /// itself and counting surfaces from 0.0001, in the medium of the ray marched)
    fn shadow(&mut self, i: usize, world: Vec3, direction: Vec3, reach: f64) -> Result<Shadow, OpenItem> {
        let l = self.lighting();
        let mut tracer = l.surface.tracer(l.lit, self.ray_depth);
        let q = ShadowQuery { light: i, point: world, geometric_normal: [0.0; 3], normal: [0.0; 3], direction, reach, offset: false };
        let s = tracer.shadow(&q);
        match tracer.error {
            Some(e) => Err(e),
            None => Ok(s),
        }
    }
}

impl PlainSurface {
    /// Whether, as 5.0, some volume material has jittered sample places (bit 0x800000): the
    /// render's shading then draws.
    pub(super) fn jitters(&self) -> bool {
        self.version == RenderAs::Bryce50
            && self.materials.iter().any(|m| matches!(m, Ok(ObjectMaterial::Volume(vm)) if vm.flags & JITTERED != 0))
    }

    /// Whether geometry object `o`'s material is a volume material.
    pub(super) fn is_volume(&self, o: usize, meta: Option<u32>) -> bool {
        super::material_entry(&self.geometry, o, meta).is_some_and(|i| matches!(self.materials[i], Ok(ObjectMaterial::Volume(_))))
    }

    /// A shadow tracer for a point of these rules. (Shadow rays)
    pub(super) fn tracer(&self, start: ShadowStart, ray_depth: u32) -> ShadowTracer<'_> {
        ShadowTracer {
            version: self.version,
            ray_depth,
            start,
            geometry: &self.geometry,
            ctx: &self.ctx,
            surfaces: &self.shadow_surfaces,
            materials: &self.materials,
            solid_of: &self.solid_of,
            material_of: &self.material_of,
            met: &self.met,
            tables: self.tables.view(),
            switches: self.scene_light.switches,
            transmission: self.scene_light.switches & scene_switch::TRANSMISSION != 0,
            clouds: self.clouds.as_ref(),
            // : a light's shadow from a point of a march is asked by
            // 's rules, from the march's point, no ray given up, the sample number that of the
            // ray along which the march runs
            radii: &self.radii,
            generator: Some(&self.generator),
            soft: Some(SoftAt { generator: &self.generator, sample: self.march_sample.get(), grid: self.soft_grid }),
            error: None,
        }
    }

    /// A ray that meets a volume material's surface: not shaded (no light term, no textures
    /// evaluated, no ray); one ray goes on from the point met, the same direction, from
    /// 0.0001 without end, weight and total weight (1, 1, 1), the depth plus 1 (whatever the
    /// depth limit), the incoming ray's air continued, in the medium on the far side. Its
    /// colour replaces the surface's (`brought_back`). (A volume material's
    /// surface)
    pub(super) fn volume_surface(&mut self, request: &SurfaceRequest, secondary: &mut SecondaryQueue) -> Result<[f64; 3], OpenItem> {
        // : the continuing ray gathers as any ray
        let state = request.state;
        // : as 5.0 only
        // while the rays the path has gone through before this surface are below 100 (as the
        // blended ray), and with the depth of the ray that met the surface
        let five = self.version == RenderAs::Bryce50;
        if five && state.rays - 1 >= 100 {
            return Ok([0.0; 3]);
        }
        let (met, far) = self.surface_media(request);
        let media = match far {
            Some(f) => f?,
            None => met,
        };
        secondary.push(SecondaryRay {
            kind: SecondaryKind::VolumeThrough,
            ray: CameraRay { origin: request.hit.point, dir: request.ray.dir, start: further::RAY_START },
            weight: [1.0; 3],
            total: [1.0; 3],
            depth: if five { request.depth } else { request.depth + 1 },
            pixel: request.pixel,
            sample: request.sample,
            // ("keeps that ray's distance travelled and starting point for the
            // haze"),
            state: PathState {
                media,
                tir_count: state.tir_count,
                rays: state.rays + 1,
                air: state.air.continued(request.ray.origin, request.hit.t),
                set_showing: state.set_showing,
            },
        });
        Ok([0.0; 3])
    }

    /// The march of the medium a ray (not a shadow ray) travels in, where it is a volume
    /// material's solid, with how it is laid over the ray's colour. (Where marches
    /// are made; The stretch; the shaded kind's shadows; the lit kind's shadow rays)
    pub(super) fn march_ray(
        &self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<&Hit>,
        numbered: bool,
    ) -> Result<Option<Laying>, OpenItem> {
        let laid = self.march_pass(ray, depth, state, hit, numbered);
        // pass 2 (or any march) leaves nothing given behind
        self.march_given.borrow_mut().take();
        laid
    }

    /// The ambient rays the march just made asks for (pass 1), in the order asked.
    /// (Revision of)
    pub(super) fn take_march_asks(&self) -> Vec<SecondaryRay> {
        std::mem::take(&mut *self.march_asks.borrow_mut())
    }

    /// What the ambient rays brought back, for the next march (pass 2): the colour of each as it
    /// comes back, in the order asked.
    pub(super) fn give_march_gathered(&self, gathered: Vec<[f32; 3]>) {
        *self.march_given.borrow_mut() = Some(gathered.into());
    }

    fn march_pass(
        &self,
        ray: &CameraRay,
        depth: u32,
        state: &PathState,
        hit: Option<&Hit>,
        numbered: bool,
    ) -> Result<Option<Laying>, OpenItem> {
        // : the depth inside is that of the object's medium names (a group's
        // member, never the group's own)
        let Some(solid) = state.media.current_material() else { return Ok(None) };
        let Some(entry) = super::material_entry(&self.geometry, solid, None) else { return Ok(None) };
        let Ok(ObjectMaterial::Volume(vm)) = &self.materials[entry] else {
            return Ok(None);
        };
        let toward_eye = ray.dir.map(|x| -x);
        let (after, shaded, shaded_offset) = match hit {
            None => (After::Miss, ShadowStart { incoming: state.media, far: None, toward_eye, object: None }, false),
            Some(h) => {
                let after = if self.is_volume(h.object, h.meta) { After::VolumeSurface } else { After::Surface };
                // As for a hit of the ray marched: its media with the meeting, the far side of
                // that surface's object for's start rule (as 3.1).
                let (met, far) = if self.encloses[self.solid_of[h.object]] {
                    let solid = self.solid_of[h.object];
                    let met = state.media.met(solid);
                    (met, Some(met.through_as(solid, h.medium_object(self.version))))
                } else {
                    (state.media, None)
                };
                (after, ShadowStart { incoming: met, far, toward_eye, object: None }, true)
            }
        };
        let lit = ShadowStart { incoming: state.media, far: None, toward_eye, object: None };
        // (the shaded kind: the ambient ray of point 5, asked by a ray that
        // carries a number while the rays its path has gone through are below the depth limit)
        let ambient = if !numbered {
            Ambient::Unnumbered
        } else if state.rays - 1 < self.depth_limit {
            Ambient::Asks(*state)
        } else {
            Ambient::TooDeep
        };
        let lighting = SiteLighting { surface: self, shaded, shaded_offset, lit, ambient };
        let switches = self.scene_light.switches;
        let mut site =
            RenderSite::new(self.version, &self.geometry, solid, vm, self.tables.view(), switches, ray.dir, ray.origin, depth, Some(lighting), Some(&self.generator));
        // : the sky's air on every sample
        if self.version == RenderAs::Bryce50 && vm.flags & SKY_INTEGRATION != 0 {
            // Open : the wash over the samples; the original drew no such picture
            if self.wash.is_some() {
                return Err(vol::open::SKY_INTEGRATION);
            }
            site.sky = Some(SkyOver { surface: self, ray: *ray, depth, state: *state, hit: hit.map(|h| h.t), lit: None });
        }
        let (a, b) = stretch_ends(ray.origin, ray.dir, ray.start, None, hit.map(|h| h.point));
        // : W is the medium colour of the object's material for this ray
        let five = self.version == RenderAs::Bryce50;
        // : the light's shadow is taken
        // from the sample's place (observed, fl-s2; the fallback for what was Open), in the march
        let weight = match (five, self.medium_law(&state.media)?) {
            (true, Some(m)) => {
                let s = super::surface_of(&self.shadow_surfaces, &self.geometry, solid, None, switches, self.version)?;
                let placement = &self.geometry.scene.objects[solid].placement;
                super::super::media::tinted(self.version, m, s.tint, placement, hit.map(|h| (h.t, h.point)))?.volume_colour
            }
            _ => [1.0; 3],
        };
        let mray = MarchRay {
            a,
            b,
            direction: ray.dir,
            weight,
            uniform_density: vm.flags & UNIFORM_DENSITY != 0,
            jitter: five && vm.flags & JITTERED != 0,
            flat_lit: (five && vm.flags & FLAT_LIT != 0)
                .then_some((self.scene_light.ambient_colour, self.scene_light.ambience)),
            // : the scene's switch word has bit 0x8 and the material lacks 0x20
            light_shadows: switches & scene_switch::SHADOWS != 0 && vm.flags & material_flag::TAKES_NO_SHADOWS == 0,
            depth,
            shadow: false,
        };
        let orders = VolumeOrders::of(self.version);
        // : each kind in turn over the whole stretch, with its own
        // evaluations of the textures; the last one's colour and opacity are the march's.
        let mut marched = None;
        for &kind in &vm.kinds {
            marched = Some(march(self.version, orders, kind, vm.numbers, vm.depth, &mray, &mut site)?);
        }
        let marched = marched.expect("a volume material has a kind");
        Ok(Some(Laying { marched, adds: vm.flags & bits::ADDS != 0, after, five }))
    }
}

impl ShadowTracer<'_> {
    /// The march of a shadow ray's stretch from λ0 = `near` to `met` or its end, where it lies
    /// in a volume material's solid whose flag word has neither 0x2000 nor 0x80: its α, else
    /// `None`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn march_stretch(
        &self,
        media: &PathMedia,
        origin: Vec3,
        direction: Vec3,
        near: f64,
        end: Option<f64>,
        met: Option<Vec3>,
        depth: u32,
    ) -> Result<Option<f64>, OpenItem> {
        let Some(solid) = media.current_material() else { return Ok(None) };
        let Some(entry) = super::material_entry(&self.geometry, solid, None) else { return Ok(None) };
        let Ok(ObjectMaterial::Volume(vm)) = &self.materials[entry] else {
            return Ok(None);
        };
        if no_shadow_medium(vm.flags) {
            return Ok(None);
        }
        let mut site = RenderSite::new(self.version, self.geometry, solid, vm, self.tables, self.switches, direction, origin, 0, None, self.generator);
        let (a, b) = stretch_ends(origin, direction, near, end, met);
        // : as 5.0 every stretch takes the depth of the ray whose
        // point the shadow ray lights
        let depth = if self.version == RenderAs::Bryce50 { self.ray_depth } else { depth };
        let mray = MarchRay {
            a,
            b,
            direction,
            weight: [1.0; 3],
            uniform_density: vm.flags & UNIFORM_DENSITY != 0,
            jitter: self.version == RenderAs::Bryce50 && vm.flags & JITTERED != 0,
            flat_lit: (self.version == RenderAs::Bryce50 && vm.flags & FLAT_LIT != 0).then_some(([0.0; 3], 0.0)),
            light_shadows: true,
            depth,
            shadow: true,
        };
        let orders = VolumeOrders::of(self.version);
        let mut alpha = 0.0;
        for &kind in &vm.kinds {
            alpha = march(self.version, orders, kind, vm.numbers, vm.depth, &mray, &mut site)?.alpha;
        }
        Ok(Some(alpha))
    }
}
