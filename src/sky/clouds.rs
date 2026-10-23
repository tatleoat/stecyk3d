// SPDX-License-Identifier: MIT
//! The two cloud layers as 3.0, 3.1 and 5.0, over one ray: the colour after the clouds for a ray
//! that meets nothing, and the amount a shadow ray that meets nothing keeps after them.
//!
//! (When the clouds act, Numbers fixed for the
//! render, Where a ray meets a layer, The readings, The upper layer, The lower layer, Cloud
//! shadows); Notation of (⊕ ⊖ ⊗ as `f32` operations, ⟨x⟩ as `x as f32`, every other
//! operation one `f64` operation);;
//! (each reading from a fresh texture: see
//! the design note), (step 5: the covers a star
//! field's visibility takes, `colour_covers`)
//!
//! As 5.0 (the edge noise on each cover; a layer met from the start of the ray's
//! path), (Freedom: no table is read
//! where its numbers reach no result)
//!
//! : as 5.0 the layers of a Bryce 5 sky on a dome
//! (bit 0x40000000), the edge noise's two numbers from the sky part, clouds that follow the
//! view (bit 0x80000).
//!
//! Not here: the colour under the clouds  and which words ask for them as 5.0 (the
//! renderer's wiring).

use crate::geom::Vec3;
use crate::textures::lattice::Lattice;
use crate::textures::orders::{Order3, TextureOrders};
use crate::textures::{SurfaceContext, TextureError, TextureSettings, TextureTables, evaluate};
use crate::version::RenderAs;

use super::colours::{SunInputs, cloud_step, main_light};

/// The sky with atmosphere.
pub const ATMOSPHERE: u32 = 0x1;
/// The horizon colour as stored.
pub const HORIZON_AS_STORED: u32 = 0x8;
/// The clouds.
pub const CLOUDS: u32 = 0x10;
/// The upper layer.
pub const UPPER_LAYER: u32 = 0x20;
/// The lower layer.
pub const LOWER_LAYER: u32 = 0x40;
/// No clouds whatever the other bits.
pub const NO_SKY_EFFECTS: u32 = 0x800;
/// The cloud shadows.
pub const CLOUD_SHADOWS: u32 = 0x1000;
/// Fixed third coordinates.
pub const FIXED_THIRD: u32 = 0x40_0000;

/// The layers lie on a dome as 5.0.
pub const DOME_LAYERS: u32 = 0x4000_0000;
/// The clouds follow the view as 5.0.
pub const FOLLOW_VIEW: u32 = 0x8_0000;

/// The dome's centre lies this far below the origin.
const DOME_DEPTH: f64 = 5000.0;
/// The start counts this much in each axis of the centre's frame.
const DOME_START_SHARE: f64 = 0.01;
/// The dome's radius is this share of the height plus the depth.
const DOME_HEIGHT_SHARE: f64 = 0.75;
/// The edge noise's second coordinate per unit of ζ, upper and lower.
const EDGE_ZETA_UPPER: f64 = 0.008;
const EDGE_ZETA_LOWER: f64 = 0.005;

/// The scale's factor, the double nearest 0.00256.
const SCALE_FACTOR: f64 = 0.00256;
/// The upper layer's height above the cloud height.
const UPPER_RISE: f64 = 24.4140625;
/// The third coordinates with bit 0x400000: lower, upper.
const FIXED_W_LOWER: f64 = 9.765625;
const FIXED_W_UPPER: f64 = 34.1796875;
/// The upper layer's share of the offsets.
const UPPER_OFFSET_SHARE: f64 = 0.5;
/// E's weight on ω.
const AMBIENT_WEIGHT: f64 = 0.75;
/// The sun's depth below which the upper layer takes Λ: −y ≥ 0.25.
const SUNSET_DEPTH: f64 = 0.25;
/// The factor on −y for S.
const SUNSET_FACTOR: f64 = 4.0;
/// B, the double nearest 1/255.
const B: f64 = 1.0 / 255.0;
/// The first reading's shift of the lower layer, the double nearest 360.3.
const LOWER_SHIFT: f64 = 360.3;
/// The core's lift, the double nearest 0.4.
const CORE_LIFT: f64 = 0.4;
/// The bump's hold below and above.
const BUMP_LOW: f64 = 0.125;
const BUMP_HIGH: f64 = 1.5;
/// κ' is held to this from above.
const COVER_HOLD: f64 = 4.0;
/// The shade's factor on κ', the double nearest 0.15.
const SHADE_COVER: f64 = 0.15;

/// The edge noise as 5.0: its octaves (j = 0 to 9), the factor on its sum, the scales on X and
/// Z and the second coordinates, upper and lower layer.
const EDGE_OCTAVES: i32 = 10;
const EDGE_GAIN: f64 = 0.8;
const EDGE_SCALE_UPPER: f64 = 40.0;
const EDGE_SCALE_LOWER: f64 = 20.0;
/// The edge noise is's type 5 with the 3-D setting.
const EDGE_TYPE: i64 = 5;
const EDGE_SETTING: u8 = 3;
/// Where the edge noise's factor (0.5 to 1) can change a held cover: above 0 and below the
/// cover at which it is still held whatever the factor (2 for a cover held to 1, 8 for κ'
/// held to 4). (As 5.0: "a cover κ not above 0 stays so, an upper cover of 2 or
/// more is still held to 1, and a lower cover of 8 or more is still held to 4")
const EDGE_REACH_UNIT: f64 = 1.0; // : the hold at 1, divided by 1 − α (2 at α = 0.5)
const EDGE_REACH_SHADE: f64 = 4.0; // : the hold at 4, divided by 1 − α (8 at α = 0.5)

/// The sky part's numbers the clouds read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudSky {
    pub switches: u32,
    /// c.
    pub cloud_height: f64,
    /// γ.
    pub cover: f64,
    /// a.
    pub amplitude: f64,
    /// f.
    pub frequency: f64,
    /// (ox, oz).
    pub offset: [f64; 2],
    /// K.
    pub lower_colour: [f32; 3],
    /// M.
    pub ambient_colour: [f32; 3],
    /// W.
    pub horizon_colour: [f32; 3],
    /// What 5.0 adds.
    pub five: CloudFive,
}

/// The numbers reads from the sky part and the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudFive {
    /// α, the edge noise's amount ('s cloud_detail).
    pub edge_amount: f64,
    /// ζ, the edge noise's second coordinate ('s cloud_detail_z).
    pub edge_second: f64,
    /// The view's position (x, z) for bit 0x80000 (the translation of the view's transform).
    pub view: Option<[f64; 2]>,
}

impl Default for CloudFive {
    /// No edge noise (α = ζ = 0: a sky whose tracks are not applied, as 3.0 and 3.1; a render as
    /// 5.0 gives its own numbers), no view.,
    fn default() -> Self {
        CloudFive { edge_amount: 0.0, edge_second: 0.0, view: None }
    }
}

/// The sun as the clouds read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudSun {
    /// Toward the sun, of unit length.
    pub direction: Vec3,
    /// C, the sun's colour.
    pub colour: [f32; 3],
}

/// The two layers' textures (the first reference names the upper layer's). A layer
/// with no texture lays nothing and casts no shadow (Errors: the implementer's choice).
#[derive(Clone, Copy, Debug)]
pub struct CloudTextures<'a> {
    pub upper: Option<&'a TextureSettings>,
    pub lower: Option<&'a TextureSettings>,
    /// The substrate's tables the noises may read (none for the vectors' textures).
    pub tables: TextureTables<'a>,
}

/// The order of each three-term sum in X and Z: the terms are (D·t, O, p) in that order, as
/// `Order3` names them.
pub type PointOrder = Order3;

/// The arrangement of the lower layer's cover κ = (2v − 1)·a + γ. (the vectors
/// decide between two arrangements)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LowerCover {
    /// ((2v − 1)·a) + γ, as written.
    Written,
    /// ((2v)·a − a) + γ: the product distributed.
    Distributed,
}

impl LowerCover {
    pub const ALL: [LowerCover; 2] = [LowerCover::Written, LowerCover::Distributed];

    fn eval(self, v: f64, a: f64, gamma: f64) -> f64 {
        match self {
            LowerCover::Written => (2.0 * v - 1.0) * a + gamma,
            LowerCover::Distributed => (2.0 * v * a - a) + gamma,
        }
    }
}

/// The arrangements leaves to the vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CloudOrders {
    pub point: PointOrder,
    pub lower_cover: LowerCover,
}

impl CloudOrders {
    /// The arrangements ` ` decides (the same in both versions):
    /// X = ((Dx·t + Ox) + px)·k and Z likewise; κ = ((2v − 1)·a) + γ. The other point orders
    /// give 610 and 607 rows of 634, the distributed cover 629 (as the README counts).
    /// ; tests/sky_cloud_vectors.rs
    pub const DECIDED: CloudOrders = CloudOrders { point: Order3::AbC, lower_cover: LowerCover::Written };
}

/// Why the clouds give no result.
#[derive(Clone, Debug, PartialEq)]
pub enum CloudError {
    /// As 5.0 the edge noise reads `perm` where a cover lies within its reach, and the
    /// substrate lacks it .
    NoPerm,
    /// A reading of a layer's texture was refused.
    Texture(TextureError),
    /// A layer's texture does not give its value (outputs without `v`).
    NoValue,
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudError::NoPerm => write!(f, "the clouds' edge noise needs the table perm (SUB-8)"),
            CloudError::Texture(e) => write!(f, "cloud texture: {e}"),
            CloudError::NoValue => write!(f, "a cloud texture gives no value"),
        }
    }
}

/// One layer's plane and point.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LayerPlane {
    height: f64,
    /// The third coordinate w.
    third: f64,
    /// (px, pz).
    offset: [f64; 2],
    /// The edge noise's second coordinate.,
    edge_w: f64,
}

/// The numbers a layer's texture is asked at: X, Z and the third coordinate W.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LayerPoint {
    x: f64,
    z: f64,
    w: f64,
}

/// The numbers fixed for the render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudRender {
    version: RenderAs,
    switches: u32,
    cover: f64,
    amplitude: f64,
    /// k.
    scale: f64,
    /// The layers lie on a dome (as 5.0, bit 0x40000000).
    dome: bool,
    edge_amount: f64,
    upper: LayerPlane,
    lower: LayerPlane,
    /// A = Ω.
    core_colour: [f32; 3],
    /// E.
    lit_base: [f32; 3],
    /// S.
    upper_colour: [f32; 3],
    /// H.
    horizon: [f32; 3],
    /// ℓ, the main light's direction.
    light_direction: Vec3,
    /// Λ.
    light_colour: [f32; 3],
    /// The sun's direction (for s; the sun's even when the main light is the moon).
    sun_direction: Vec3,
    orders: CloudOrders,
}

/// The covers of the layers that acted over one ray: the upper layer's κ held to [0, 1], the
/// lower layer's κ as read (only where above 0).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerCovers {
    pub upper: Option<f64>,
    pub lower: Option<f64>,
}

/// The sky's own colours as 5.0 (any of them asks for the layers there). (As 5.0:
/// bits 0x2, 0x4 and 0x8 [format])
const FIVE_SKY_BITS: u32 = 0x2 | 0x4 | 0x8;

/// Whether the switch word asks for the layers (each then with its own bit) and for the cloud
/// shadows (with bit 0x1000): as 3.0 and 3.1 bits 0x1 and 0x10 set and 0x800 clear; as 5.0 any
/// of bits 0x2, 0x4 and 0x8 set and 0x800 clear, whatever bits 0x1 and 0x10 (the words "As 5.0"
/// names first, and "On any other word"). (When the clouds act, Cloud shadows,
/// Which words ask for them as 5.0)
pub fn clouds_asked(version: RenderAs, s: u32) -> bool {
    match version {
        RenderAs::Bryce50 => s & FIVE_SKY_BITS != 0 && s & NO_SKY_EFFECTS == 0,
        _ => s & ATMOSPHERE != 0 && s & CLOUDS != 0 && s & NO_SKY_EFFECTS == 0,
    }
}

/// H, the horizon colour.
fn horizon_colour(switches: u32, y: f64, stored: [f32; 3]) -> [f32; 3] {
    if switches & HORIZON_AS_STORED != 0 {
        return stored;
    }
    let yc = y.clamp(-1.0, 1.0);
    if yc > 0.5 {
        [1.0, 1.0, ((128.0 + 127.0 * (2.0 * yc - 1.0)) * B) as f32]
    } else if yc >= 0.0 {
        [1.0, (512.0 * yc * B) as f32, (256.0 * yc * B) as f32]
    } else {
        [(255.0 * (yc + 1.0) * B) as f32, 0.0, 0.0]
    }
}

/// a ⊕ ((b ⊖ a) ⊗ w): a drawn toward b by the single w.
fn toward(a: f32, b: f32, w: f32) -> f32 {
    a + (b - a) * w
}

/// Held to [0, 1]: below 0 or not a number to 0, above 1 to 1.
fn unit(x: f64) -> f64 {
    if x > 1.0 {
        1.0
    } else if x >= 0.0 {
        x
    } else {
        0.0
    }
}

/// The clouds' s = D·S and the bump's ℓ·g, each (y·y′ + z·z′) + x·x′, the sky's d·S.
fn dot(a: Vec3, b: Vec3) -> f64 {
    super::sky_colours::sky_dot(a, b)
}

impl CloudRender {
    /// Prepare the clouds for a render as `version` with the decided arrangements.
    pub fn new(version: RenderAs, sky: &CloudSky, sun: &CloudSun) -> Result<CloudRender, CloudError> {
        CloudRender::with_orders(version, sky, sun, CloudOrders::DECIDED)
    }

    /// `new` with the arrangements given (for the vector test that tries each).
    pub fn with_orders(
        version: RenderAs,
        sky: &CloudSky,
        sun: &CloudSun,
        orders: CloudOrders,
    ) -> Result<CloudRender, CloudError> {
        let sun_in = SunInputs { direction: sun.direction, colour: sun.colour, glow_colour: [0.0; 3], intensity: 0.0 };
        let main = main_light(&sun_in);
        let y = sun.direction[1];
        let c = sky.cloud_height;
        let h_upper = c + UPPER_RISE;
        let (w_lower, w_upper) =
            if sky.switches & FIXED_THIRD != 0 { (FIXED_W_LOWER, FIXED_W_UPPER) } else { (c, h_upper) };
        let [ox, oz] = sky.offset;
        let five = version == RenderAs::Bryce50;
        // : the offsets are made from the view
        let (lower_offset, upper_offset) = match sky.five.view {
            // sky 02 ("Clouds that follow the view, as 3.0 and 3.1": the same offsets)
            Some([vx, vz]) if sky.switches & FOLLOW_VIEW != 0 => {
                ([ox - vx, oz - vz], [UPPER_OFFSET_SHARE * ox - vx, UPPER_OFFSET_SHARE * oz - vz])
            }
            _ => ([ox, oz], [UPPER_OFFSET_SHARE * ox, UPPER_OFFSET_SHARE * oz]),
        };
        let zeta = sky.five.edge_second;
        let step = cloud_step(version, y, sky.lower_colour); // step 3, whatever bit 0x8
        let w_e = (AMBIENT_WEIGHT * step.weight) as f32;
        let lit_base = std::array::from_fn(|k| toward(sky.ambient_colour[k], step.hue_colour[k], w_e));
        let upper_colour = if !(y < 0.0) || -y >= SUNSET_DEPTH {
            main.colour
        } else {
            let w = (SUNSET_FACTOR * -y) as f32;
            std::array::from_fn(|k| toward(sun.colour[k], main.colour[k], w))
        };
        Ok(CloudRender {
            version,
            switches: sky.switches,
            cover: sky.cover,
            amplitude: sky.amplitude,
            scale: sky.frequency * SCALE_FACTOR,
            dome: five && sky.switches & DOME_LAYERS != 0,
            edge_amount: sky.five.edge_amount,
            upper: LayerPlane {
                height: h_upper,
                third: w_upper,
                offset: upper_offset,
                edge_w: zeta * EDGE_ZETA_UPPER,
            },
            lower: LayerPlane { height: c, third: w_lower, offset: lower_offset, edge_w: zeta * EDGE_ZETA_LOWER },
            core_colour: step.colour,
            lit_base,
            upper_colour,
            horizon: horizon_colour(sky.switches, y, sky.horizon_colour),
            light_direction: main.direction,
            light_colour: main.colour,
            sun_direction: sun.direction,
            orders,
        })
    }

    fn clouds_on(&self) -> bool {
        clouds_asked(self.version, self.switches)
    }

    /// The numbers the layer's texture is asked at for a ray from `origin` along `dir`: on its
    /// plane, X and Z at the distance to it (signed as 3.0, from either side where `fold`) and
    /// the layer's third coordinate; on a dome,'s far crossing.
    fn locate(&self, plane: &LayerPlane, origin: Vec3, dir: Vec3, fold: bool) -> LayerPoint {
        if self.dome {
            return self.dome_point(plane, origin, dir);
        }
        let d = plane.height - origin[1];
        let t = if fold { d.abs() / dir[1] } else { d / dir[1] };
        let o = self.orders.point;
        let x = o.sum([dir[0] * t, origin[0], plane.offset[0]]) * self.scale;
        let z = o.sum([dir[2] * t, origin[2], plane.offset[1]]) * self.scale;
        LayerPoint { x, z, w: plane.third }
    }

    /// The far crossing of the ray with the layer's dome (a sphere 5,000 below the origin, of
    /// radius h·0.75 + 5000, the start counting a hundredth).
    fn dome_point(&self, plane: &LayerPlane, origin: Vec3, dir: Vec3) -> LayerPoint {
        let [ox, oy, oz] = origin;
        let [dx, dy, dz] = dir;
        let [px, pz] = plane.offset;
        let q = [-(DOME_START_SHARE * ox), -(DOME_START_SHARE * oy + DOME_DEPTH), -(DOME_START_SHARE * oz)];
        let b = (q[2] * dz + q[1] * dy) + q[0] * dx;
        let r = plane.height * DOME_HEIGHT_SHARE + DOME_DEPTH;
        let delta = r * r - (((q[2] * q[2] + q[1] * q[1]) + q[0] * q[0]) - b * b);
        // Not δ ≥ 0 (below 0, or not a number): met nowhere; the numbers are not scaled.
        if !(delta >= 0.0) {
            return LayerPoint { x: px, z: pz, w: 0.0 };
        }
        let t = delta.sqrt() + b; // 's square root (correctly rounded)
        LayerPoint {
            x: ((t * dx + ox) + px) * self.scale,
            z: ((t * dz + oz) + pz) * self.scale,
            w: ((t * dy + oy) + 0.0) * self.scale,
        }
    }

    /// The upper layer's cover at (X, Z), unheld (as 5.0 with its edge noise where it can
    /// change the cover held to [0, 1]).
    fn upper_cover(&self, tex: &TextureSettings, tables: TextureTables<'_>, p: LayerPoint, surface: SurfaceContext) -> Result<f64, CloudError> {
        let LayerPoint { x, z, w } = p;
        let v = read(tex, tables, [x + 0.5 * z, z, w], surface, self.version)?.0;
        let kappa = self.cover + ((2.0 * v - 1.0) * self.amplitude) * 0.5;
        self.edged(kappa, tables, [EDGE_SCALE_UPPER * x, self.upper.edge_w, EDGE_SCALE_UPPER * z], EDGE_REACH_UNIT)
    }

    /// The lower layer's cover at (X, Z), unheld (as 5.0 with its edge noise where it can change
    /// a result: `reach` 8 where κ' is used, 2 where the cover is held to [0, 1]).
    fn lower_cover(
        &self,
        tex: &TextureSettings,
        tables: TextureTables<'_>,
        p: LayerPoint,
        surface: SurfaceContext,
        reach: f64,
    ) -> Result<f64, CloudError> {
        let LayerPoint { x, z, w } = p;
        let v = read(tex, tables, [x + LOWER_SHIFT, z + LOWER_SHIFT, w], surface, self.version)?.0;
        let kappa = self.orders.lower_cover.eval(v, self.amplitude, self.cover);
        self.edged(kappa, tables, [EDGE_SCALE_LOWER * x, self.lower.edge_w, EDGE_SCALE_LOWER * z], reach)
    }

    /// As 5.0, κ after the edge noise at `at` = (x, w, z): (1 − (1 − τ)·0.5)·κ, τ = 0.8·N held
    /// to [0, 1], N = Σ 2^(−j)·n(2^j·x, 2^j·w, 2^j·z) over j = 0 to 9 in that order, n type 5
    /// with the 3-D setting. The factor lies in 0.5 to 1, so a κ not above 0, or of `reach` or
    /// more, gives the same result without it: there it is not computed and no table is read
    /// ('s Freedom). As 3.0 and 3.1 the same, where the sky's tracks make α ('s wind),
    /// with the version's own type 5 and its own `perm` (read 2026-10-22: each
    /// program's cloud code calls the noise function its own type table gives for type 5, on the
    /// three numbers and mode 2).
    fn edged(&self, kappa: f64, tables: TextureTables<'_>, at: [f64; 3], reach: f64) -> Result<f64, CloudError> {
        // : α = 0 exactly forms no edge noise. The
        // factor 1 − (1 − τ)·α lies in [1 − α, 1] for α in [0, 1]: the reach scales with it.
        let alpha = self.edge_amount;
        if alpha == 0.0 {
            return Ok(kappa);
        }
        let reach = if alpha > 0.0 && alpha < 1.0 {
            reach / (1.0 - alpha)
        } else {
            f64::INFINITY
        };
        let in_reach = if alpha >= 0.0 && alpha <= 1.0 { kappa > 0.0 && kappa < reach } else { true };
        if !in_reach {
            return Ok(kappa);
        }
        let perm = tables.perm.ok_or(CloudError::NoPerm)?;
        let lattice = Lattice {
            perm: Some(perm),
            waves: tables.waves,
            version: self.version,
            smooth: TextureOrders::of(self.version).smoothstep,
        };
        let mut n = 0.0;
        for j in 0..EDGE_OCTAVES {
            let up = 2f64.powi(j);
            let s = lattice
                .sample(EDGE_TYPE, EDGE_SETTING, at.map(|c| up * c))
                .map_err(|_| CloudError::NoPerm)?;
            n += 2f64.powi(-j) * s.value;
        }
        let tau = unit(EDGE_GAIN * n);
        Ok((1.0 - (1.0 - tau) * alpha) * kappa)
    }

    /// The colour after the clouds for a ray that meets nothing along the unit `dir`, over the
    /// colour so far: `origin` is where the ray starts (the point a further ray leaves),
    /// `path_start` where its path starts (the eye); as 3.0 and 3.1 a layer is met from the
    /// first, as 5.0 from the second. (When the clouds act, Where a ray meets a
    /// layer, The upper layer, The lower layer, As 5.0)
    pub fn colour(
        &self,
        textures: CloudTextures<'_>,
        origin: Vec3,
        path_start: Vec3,
        dir: Vec3,
        colour: [f32; 3],
        surface: SurfaceContext,
    ) -> Result<[f32; 3], CloudError> {
        Ok(self.colour_covers(textures, origin, path_start, dir, colour, surface)?.0)
    }

    /// `colour`, with the covers of the layers that acted, upper then lower, as a star field's
    /// visibility takes them: the upper layer's κ held to [0, 1], the lower layer's κ where it
    /// is above 0 (a lower layer whose κ is not above 0 changes nothing).
    pub fn colour_covers(
        &self,
        textures: CloudTextures<'_>,
        origin: Vec3,
        path_start: Vec3,
        dir: Vec3,
        colour: [f32; 3],
        surface: SurfaceContext,
    ) -> Result<([f32; 3], LayerCovers), CloudError> {
        let mut c = colour;
        let mut covers = LayerCovers::default();
        if !(self.clouds_on() && dir[1] > 0.0) {
            return Ok((c, covers));
        }
        // (Where a further ray meets a layer as 5.0):
        // as 5.0 a layer is met from the start of the ray's PATH, which is the ray's own origin for a
        // camera, reflected, transmitted or ambient ray and the parent's start only for a continued ray
        // (original's RayState.start; answered 2026-10-22). The caller hands that point.
        let origin = if self.version == RenderAs::Bryce50 { path_start } else { origin };
        let fold = self.version != RenderAs::Bryce30;
        if let (true, Some(tex)) = (self.switches & UPPER_LAYER != 0, textures.upper) {
            let p = self.locate(&self.upper, origin, dir, fold);
            let held = unit(self.upper_cover(tex, textures.tables, p, surface)?);
            covers.upper = Some(held);
            let kappa = held as f32;
            let r = 1.0 - dir[1].abs();
            let q = r * r;
            let wq = (q * q) as f32;
            for (k, ck) in c.iter_mut().enumerate() {
                let p = toward(self.upper_colour[k], self.horizon[k], wq);
                *ck = toward(*ck, p, kappa);
            }
        }
        if let (true, Some(tex)) = (self.switches & LOWER_LAYER != 0, textures.lower) {
            let p = self.locate(&self.lower, origin, dir, fold);
            let kappa = self.lower_cover(tex, textures.tables, p, surface, EDGE_REACH_SHADE)?;
            if kappa > 0.0 {
                covers.lower = Some(kappa);
                c = self.lower_layer(tex, textures.tables, p, kappa, dir, c, surface)?;
            }
        }
        Ok((c, covers))
    }

    /// The lower layer where κ > 0.
    #[allow(clippy::too_many_arguments)]
    fn lower_layer(
        &self,
        tex: &TextureSettings,
        tables: TextureTables<'_>,
        p: LayerPoint,
        kappa: f64,
        dir: Vec3,
        colour: [f32; 3],
        surface: SurfaceContext,
    ) -> Result<[f32; 3], CloudError> {
        let LayerPoint { x, z, w } = p;
        let lam = self.light_colour;
        let s = dot(dir, self.sun_direction);
        let e = (s + 1.0) * 0.5;
        let e2 = e * e;
        // The lit side.
        let v2 = read(tex, tables, [0.5 * x, 0.5 * z, w], surface, self.version)?.0;
        let lambda = unit(e2 + (2.0 * v2 - 1.0) * 0.5) as f32;
        let mut t: [f32; 3] = std::array::from_fn(|k| toward(self.lit_base[k], lam[k], lambda));
        // The core.
        let (v3, g) = read(tex, tables, [x, z, 0.5 * w], surface, self.version)?;
        let u = unit((2.0 * v3 - 1.0) + CORE_LIFT) as f32;
        let a = self.core_colour;
        for k in 0..3 {
            t[k] = a[k] + u * (t[k] - a[k]);
        }
        // The bump: red and green rounded, blue kept as a double.
        let mut blue = f64::from(t[2]);
        // : a texture whose outputs include bump gives its gradient with the value.
        if let (true, Some(g)) = (tex.outputs.bump, g) {
            // Held to 0.125 below and 1.5 above (a NaN stays NaN: the text does not say).
            let beta = (1.0 + 2.0 * dot(self.light_direction, g)).clamp(BUMP_LOW, BUMP_HIGH);
            t[0] = (beta * f64::from(t[0])) as f32;
            t[1] = (beta * f64::from(t[1])) as f32;
            blue = beta * f64::from(t[2]);
        }
        // The shade.
        let kp = if kappa > COVER_HOLD { COVER_HOLD } else { kappa };
        let sigma = (1.0 - SHADE_COVER * kp) - 0.5 * e;
        t[0] = (sigma * f64::from(t[0])) as f32;
        t[1] = (sigma * f64::from(t[1])) as f32;
        t[2] = (sigma * blue) as f32;
        // Thin cloud near the sun.
        let nu = unit((1.0 - kp) * e2) as f32;
        for k in 0..3 {
            t[k] = t[k] + nu * (lam[k] - t[k]);
        }
        // Laid over.
        let rho = if kp < 1.0 { kp as f32 } else { 1.0 };
        Ok(std::array::from_fn(|k| colour[k] + rho * (t[k] - colour[k])))
    }

    /// The amount a shadow ray that meets nothing keeps after the clouds, from `alpha`.
    /// `toward_sun`: the ray goes toward the sun (as 3.1 only those pass the layers).
    pub fn shadow(
        &self,
        textures: CloudTextures<'_>,
        origin: Vec3,
        dir: Vec3,
        toward_sun: bool,
        alpha: f64,
        surface: SurfaceContext,
    ) -> Result<f64, CloudError> {
        let mut alpha = alpha;
        let passes = match self.version {
            RenderAs::Bryce30 => true,
            _ => toward_sun,
        };
        if !(passes && self.switches & CLOUD_SHADOWS != 0 && self.clouds_on() && dir[1] > 0.0) {
            return Ok(alpha);
        }
        if let (true, Some(tex)) = (self.switches & UPPER_LAYER != 0, textures.upper) {
            let p = self.locate(&self.upper, origin, dir, false);
            let kappa = unit(self.upper_cover(tex, textures.tables, p, surface)?);
            alpha *= (1.0 - kappa) * dir[1];
        }
        if let (true, Some(tex)) = (self.switches & LOWER_LAYER != 0, textures.lower) {
            let p = self.locate(&self.lower, origin, dir, false);
            let kappa = unit(self.lower_cover(tex, textures.tables, p, surface, EDGE_REACH_UNIT)?);
            alpha *= (1.0 - kappa) * dir[1];
        }
        Ok(alpha)
    }
}

/// One reading: the texture's value and, where its outputs include bump, the bump gradient.
/// The point is already in texture space; each reading is from a fresh texture (design note).
fn read(
    tex: &TextureSettings,
    tables: TextureTables<'_>,
    point: [f64; 3],
    surface: SurfaceContext,
    version: RenderAs,
) -> Result<(f64, Option<[f64; 3]>), CloudError> {
    let r = evaluate(tex, point, surface, tables, version).map_err(CloudError::Texture)?;
    let v = r.value.ok_or(CloudError::NoValue)?;
    Ok((v, r.bump))
}
