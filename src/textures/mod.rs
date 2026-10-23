// SPDX-License-Identifier: MIT
//! Procedural textures as functions of position (component `textures`).
//!
//! The evaluation follows's order of stages: the global phase on the texture's point
//! ; per component the noise space , the phase , the noise type summed
//! by its octave method , the filter  and the colour map ; the
//! components blended in order ; then the global filter and colour map. The entries now
//! state the noise space, the phase (without a bump), every octave method, every filter and the
//! noise types that read no table. Still Open, and refused with the
//! points that block (`open_points`): the lattice types and every type built on them, type 17,
//! Bryce 5's types, method 11 over a noise, the colour maps, the blends and the bump.
//!
//! , (not
//! needed by the case is evaluated from a fresh texture), (the surface
//! context, given),;

pub mod blend;
pub mod catalogue;
pub mod colours;
pub mod filters;
pub mod five;
pub mod lattice;
pub mod mapping;
pub mod noise;
pub mod octaves;
pub mod orders;
pub mod picture;
pub mod settings;
pub mod space;
pub mod tables;

pub use catalogue::{NoiseFamily, OpenPoint, noise_family};
pub use orders::TextureOrders;
pub use settings::{
    ColourMap, ColourSpace, Component, Dimension, Filter, NoiseSettings, NoiseSpace, OutputSet, Phase, SurfaceContext,
    TextureSettings, MAX_COMPONENTS,
};
pub use tables::TextureTables;

use crate::numerics::{Reg53, sqrt_as};
use crate::version::RenderAs;

use blend::{ComponentResult, LAST_BLEND};
use catalogue::PERM_METHOD;
use five::{FiveSets, five_sample};
use lattice::{Lattice, NotComputed, moved_point, reads_a_table};
use colours::PermNoise;
use filters::{LAST_FILTER, filtered};
use noise::{TYPE0_VALUE, is_table_free, table_free};
use octaves::{MAX_OCTAVES, NoiseSample, Octaves, octave_value};
use space::SpaceMap;

/// What a texture holds from earlier evaluations that an evaluation may leave standing: its
/// colour and, as 3.0, its bump gradient. Before any evaluation a document's texture holds the
/// result as saved , and no evaluation of one texture in a render changes which of the
/// outputs it writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kept {
    pub colour: [u8; 3],
    pub gradient: [f64; 3],
}

thread_local! {
    /// The three points beside the point that a global phase last moved, shared by all textures
    /// of a render (the program's start: (0, 0, 0) each).
    static SHARED_POINTS: std::cell::Cell<[[f64; 3]; 3]> = const { std::cell::Cell::new([[0.0; 3]; 3]) };
}

thread_local! {
    /// Set when this thread READ shared render state that the evaluations before it in picture
    /// order would have written (the shared points of a 3.0 bump under a global phase of amount
    /// 0, or the shared context a cloud reading takes as 3.0 and 3.1 in a document with
    /// textured materials): a multithreaded render that finds it set throws its result away and
    /// renders serially, because that pixel would then depend on the order the pixels were drawn
    /// in. Writing alone does no harm: every read of this state is flagged, and nothing else
    /// reads it.
    static SHARED_TOUCHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

thread_local! {
    /// Set by a multithreaded worker whose document has textured materials or gels: its shared
    /// context is then written by evaluations, so a reading of it (as 3.0 and 3.1) is order-dependent.
    static TEXTURED_DOCUMENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The worker's document has textured materials (or gels): see `TEXTURED_DOCUMENT`.
pub fn note_textured_document(on: bool) {
    TEXTURED_DOCUMENT.with(|c| c.set(on));
}

/// Whether this thread has touched the shared render state since the last call (clears it).
pub fn take_shared_touched() -> bool {
    SHARED_TOUCHED.with(|c| c.replace(false))
}

/// A render starts with the shared points beside the point as a program just started has
/// them, (0, 0, 0) each.
pub fn reset_shared_points() {
    SHARED_POINTS.with(|c| c.set([[0.0; 3]; 3]));
    SHARED_CONTEXT.with(|c| c.set(SurfaceContext { altitude: 0.0, slope: 0.0, orientation: 0.0 }));
}

thread_local! {
    /// The surface context the last evaluation of a material's texture was given, shared by all
    /// textures; 0, 0, 0 at a render's start.
    static SHARED_CONTEXT: std::cell::Cell<SurfaceContext> =
        const { std::cell::Cell::new(SurfaceContext { altitude: 0.0, slope: 0.0, orientation: 0.0 }) };
}

/// The context a sky's cloud reading takes: as 3.0 and 3.1 the one the last material texture's
/// evaluation was given; as 5.0 altitude 10, slope 1, orientation 0, made the shared context
/// first.,
pub fn cloud_reading_context(version: RenderAs) -> SurfaceContext {
    if version == RenderAs::Bryce50 {
        let c = SurfaceContext { altitude: 10.0, slope: 1.0, orientation: 0.0 };
        SHARED_CONTEXT.with(|s| s.set(c));
        c
    } else {
        // A reading of the last texture evaluation's context: which evaluation was last depends
        // on the order the pixels were drawn in .
        if TEXTURED_DOCUMENT.with(|c| c.get()) {
            SHARED_TOUCHED.with(|c| c.set(true));
        }
        SHARED_CONTEXT.with(|s| s.get())
    }
}

/// A texture's outputs, each present only when asked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureResult {
    pub colour: Option<[u8; 3]>,
    pub value: Option<f64>,
    pub bump: Option<[f64; 3]>,
}

/// Why an evaluation gives no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    /// No component, or more than three (Limits).
    ComponentCount(usize),
    /// The stages the evaluation needs that the released entries leave Open (or to the
    /// implementer, who refuses), in the order the evaluation reaches them, each once.
    Open(Vec<OpenPoint>),
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextureError::ComponentCount(n) => write!(f, "{n} components (TX-1.1: one to three)"),
            TextureError::Open(points) => {
                write!(f, "Open in the released entries:")?;
                for (i, p) in points.iter().enumerate() {
                    write!(f, "{}{p}", if i == 0 { " " } else { "; " })?;
                }
                Ok(())
            }
        }
    }
}

/// Collects Open points in first-reached order, each once.
#[derive(Default)]
struct Needs(Vec<OpenPoint>);

impl Needs {
    fn add(&mut self, p: OpenPoint) {
        if !self.0.contains(&p) {
            self.0.push(p);
        }
    }

    /// A noise summed by its octave method .
    fn noise(&mut self, noise: &NoiseSettings, version: RenderAs) {
        if let Dimension::Other(n) = noise.dimension {
            self.add(OpenPoint::Dimension(n));
        }
        let t = noise.type_number;
        if t == 0 {
            return;
        }
        match noise_family(t, version) {
            // , : every type 0 to 26 is
            // computed; a table the substrate lacks is refused when reached .
            NoiseFamily::Three => debug_assert!(is_table_free(t) || reads_a_table(t)),
            // , 27 to 48 read `noise-bits-five` (refused when the substrate
            // lacks it, or where the type's result is Open, when reached); type 49 reads none.
            NoiseFamily::Five => {}
            NoiseFamily::Outside => self.add(OpenPoint::NoiseTypeOutside(t)),
        }
        // : method 11 is computed (its q0 reads `perm`, refused when lacking).
        // : every method number is a method (0 where not 1 to
        // 12) and a count from -9 to 9 is computed; only a count above 9 is refused.
        if noise.octaves > MAX_OCTAVES {
            self.add(OpenPoint::OctaveCount(noise.octaves));
        }
    }

    /// A colour map whose mode lies outside 0 to 16 (0xFFFFFF80 among them: Open; others the
    /// implementer's choice). The modes that read `perm` are computed (a table the substrate
    /// lacks is refused when reached).
    fn colour_map(&mut self, _m: &ColourMap) {
        // : a mode outside 0 to 16 is settled, no longer Open.
    }

    fn filter(&mut self, f: &Filter) {
        if !(0..=LAST_FILTER).contains(&f.number) {
            self.add(OpenPoint::Filter(f.number));
        }
    }
}

/// Whether a component's phase moves anything: not when its amount is 0 or its noise type 0.
fn component_phase_acts(p: &Phase) -> bool {
    p.amount != 0.0 && p.noise.type_number != 0
}

/// Whether the global phase moves anything: not when its noise type is 0; as 3.1 and 5.0 not
/// when its amount is not above 0.
fn global_phase_acts(p: &Phase, version: RenderAs) -> bool {
    if p.noise.type_number == 0 {
        return false;
    }
    match version {
        RenderAs::Bryce30 => true,
        RenderAs::Bryce31 | RenderAs::Bryce50 => p.amount > 0.0,
    }
}

/// The Open points an evaluation of `settings` as `version` passes through, in's order.
pub fn open_points(settings: &TextureSettings, version: RenderAs) -> Vec<OpenPoint> {
    let mut needs = Needs::default();
    let asked = settings.outputs;
    if global_phase_acts(&settings.phase, version) {
        needs.noise(&settings.phase.noise, version);
    }
    for (i, c) in settings.components.iter().enumerate() {
        if component_phase_acts(&c.phase) {
            needs.noise(&c.phase.noise, version);
        }
        needs.noise(&c.noise, version);
        needs.filter(&c.filter);
        if c.outputs.colour {
            needs.colour_map(&c.colours);
        }
        // : stated, not Open.
        if i > 0 {
            let m = settings.blend[i - 1];
            if !(0..=LAST_BLEND).contains(&m) {
                needs.add(OpenPoint::Blend(m));
            }
        }
    }
    needs.filter(&settings.filter);
    if asked.colour {
        needs.colour_map(&settings.colours);
    }
    needs.0
}

/// The tables the noises read, and Bryce 5's two sets, made from `noise-bits-five` before the
/// first sample that needs them.
struct Noises<'a> {
    lat: Lattice<'a>,
    bits: Option<&'a [u32; 64]>,
    sets: std::cell::OnceCell<FiveSets>,
}

/// Bryce 5's types that read `noise-bits-five`.
const FIVE_TABLE_TYPES: std::ops::RangeInclusive<i64> = 27..=48;

/// One noise's octave value at q in its own dimension, and the point its method leaves.
/// Only called once `open_points` is empty.
fn octave(noise: &NoiseSettings, q: [f64; 3], orders: &TextureOrders, noises: &Noises<'_>) -> Result<Octaves, OpenPoint> {
    let lat = &noises.lat;
    let t = noise.type_number;
    if t == 0 {
        // : with type 0 every method gives 0.5 and leaves the point as it was.
        return Ok(Octaves { value: TYPE0_VALUE, left: q, exact: None });
    }
    let d: u8 = match noise.dimension {
        Dimension::One => 1,
        Dimension::Two => 2,
        Dimension::Three => 3,
        Dimension::Other(n) => return Err(OpenPoint::Dimension(n)),
    };
    // A sample the noise cannot give (a table the substrate lacks; a fold that never ends; a
    // particular not built) is noted and the evaluation refused.
    let failed = std::cell::Cell::new(None::<OpenPoint>);
    let note = |p: OpenPoint| {
        if failed.get().is_none() {
            failed.set(Some(p));
        }
    };
    let n = |x: [f64; 3]| -> NoiseSample {
        let nan = |p: OpenPoint| {
            note(p);
            NoiseSample::Double(f64::NAN)
        };
        if FIVE_TABLE_TYPES.contains(&t) {
            let Some(bits) = noises.bits else {
                return nan(OpenPoint::NoTable("noise-bits-five"));
            };
            let sets = noises.sets.get_or_init(|| FiveSets::new(bits));
            match five_sample(t, d, x, bits, sets) {
                Some(s) => s,
                None => nan(OpenPoint::NoiseTypeFive(t)),
            }
        } else if t == 49 {
            // : the squares and sums are formed in the
            // register at 53 bits and the value is handed on unrounded to binary64; where
            // every coordinate is finite (the others behave alike in either format).
            let wide = (|| {
                let sq = |v: f64| Reg53::from_f64(v).map(|r| r.mul(r));
                let squares = match d {
                    1 => vec![sq(x[0])?],
                    2 => vec![sq(x[0])?, sq(x[1])?],
                    _ => vec![sq(x[0])?, sq(x[1])?, sq(x[2])?],
                };
                if d == 1 { Some(squares[0]) } else { orders.type49.eval_wide(&squares) }
            })();
            match wide {
                Some(w) => NoiseSample::Extended(w.to_ext()),
                None => match table_free(t, noise.dimension, x, orders) {
                    Some(v) => NoiseSample::Double(v),
                    None => nan(OpenPoint::NoiseTypeFive(t)),
                },
            }
        } else if reads_a_table(t) {
            match lat.sample(t, d, x) {
                Ok(s) => s.exact.map_or(NoiseSample::Double(s.value), NoiseSample::Extended),
                Err(NotComputed::NoTable) => nan(OpenPoint::NoTable(if t == 17 { "waves" } else { "perm" })),
                Err(NotComputed::NotHere) => nan(OpenPoint::NoiseType(t)),
            }
        } else {
            match table_free(t, noise.dimension, x, orders) {
                Some(v) => NoiseSample::Double(v),
                None => nan(OpenPoint::NoiseTypeFive(t)),
            }
        }
    };
    let mover = |x: [f64; 3]| moved_point(t, d, x).unwrap_or(x);
    let moves: Option<&dyn Fn([f64; 3]) -> [f64; 3]> = moved_point(t, d, q).is_some().then_some(&mover);
    let start = match noise.method {
        PERM_METHOD => Some(lat.method11_start(q).ok_or(OpenPoint::NoTable("perm"))?),
        _ => None,
    };
    let one_d = d == 1;
    let o = octave_value(noise.method, noise.octaves, one_d, q, orders, &n, moves, start).ok_or(match moves {
        Some(_) => OpenPoint::NotBuilt("TX-2.2", "octave methods 9 and 10 over type 8 with the 3-D setting"),
        None => OpenPoint::OctaveMethod(noise.method),
    })?;
    if let Some(p) = failed.get() {
        return Err(p);
    }
    Ok(o)
}


/// The phase's displacement of p. `factor` multiplies the phase space (0.002777777778
/// for a component's phase, none for the global phase) and `amount_factor` the amount.
fn displaced(
    phase: &Phase,
    p: [f64; 3],
    factor: Option<f64>,
    amount_factor: f64,
    orders: &TextureOrders,
    noises: &Noises<'_>,
) -> Result<[f64; 3], OpenPoint> {
    let mu = phase.amount * amount_factor;
    let angles = [phase.space.angles[0] + PHASE_TURN, phase.space.angles[1]];
    // : each entry of M' is's entry rounded, then times the factor
    // and rounded again (OnEntries, the default); OnFrequencies is kept for the vector search.
    let map = match (factor, orders.phase_factor) {
        (Some(f), orders::PhaseFactor::OnFrequencies) => {
            SpaceMap::new(angles, phase.space.frequencies.map(|s| s * f), orders.space)
        }
        (Some(f), orders::PhaseFactor::OnEntries) => SpaceMap::new(angles, phase.space.frequencies, orders.space).scaled(f),
        (None, _) => SpaceMap::new(angles, phase.space.frequencies, orders.space),
    };
    let mut r = map.apply(p);
    let axes = match phase.noise.dimension {
        Dimension::One => 1,
        Dimension::Two => 2,
        _ => 3,
    };
    let mut out = p;
    for slot in out.iter_mut().take(axes) {
        let o = octave(&phase.noise, r, orders, noises)?;
        // pk + v·μ, the one rounding of v·μ + pk; an extended sample multiplied unrounded
        // (type 12).
        *slot += o.exact.map_or(o.value * mu, |e| e.mul_to_f64(mu));
        let [l1, l2, l3] = o.left;
        r = [l2, l3, l1];
    }
    Ok(out)
}

/// The three points beside the point as a global phase moves them, for a bump sampled there.
/// `p` is the texture's point before the phase. (the four places r0 and
/// rj = r0 + 0.001·(column j of M'), the four points p and p + 0.001 on axis j, each point's
/// axis k moved by the phase noise's value at its own place; the places are turned to (second,
/// third, first) from what they were first computed, not from what the octave method left)
fn displaced_beside(
    phase: &Phase,
    p: [f64; 3],
    orders: &TextureOrders,
    noises: &Noises<'_>,
) -> Result<([f64; 3], [[f64; 3]; 3]), OpenPoint> {
    let mu = phase.amount * GLOBAL_PHASE_AMOUNT;
    let angles = [phase.space.angles[0] + PHASE_TURN, phase.space.angles[1]];
    let map = SpaceMap::new(angles, phase.space.frequencies, orders.space);
    let r0 = map.apply(p);
    // Index 3 is P0 itself: with a bump, P0 is moved by's turns too.
    let mut places: [[f64; 3]; 4] = std::array::from_fn(|j| {
        if j == 3 {
            return r0;
        }
        let col = map.column(j);
        std::array::from_fn(|i| r0[i] + BUMP_STEP * col[i])
    });
    let mut points: [[f64; 3]; 4] = std::array::from_fn(|j| {
        let mut q = p;
        if j < 3 {
            q[j] += BUMP_STEP;
        }
        q
    });
    let axes = match phase.noise.dimension {
        Dimension::One => 1,
        Dimension::Two => 2,
        _ => 3,
    };
    for k in 0..axes {
        for j in 0..4 {
            let o = octave(&phase.noise, places[j], orders, noises)?;
            points[j][k] += o.exact.map_or(o.value * mu, |e| e.mul_to_f64(mu));
        }
        for place in places.iter_mut() {
            let [a, b, c] = *place;
            *place = [b, c, a];
        }
    }
    Ok((points[3], [points[0], points[1], points[2]]))
}

/// The bump gradient's step along each column of the noise space map.
const BUMP_STEP: f64 = 0.001;

/// 's turn of the phase space's first angle.
const PHASE_TURN: f64 = 90.0;
/// 's factor on a component's phase space.
const PHASE_SPACE_FACTOR: f64 = 0.002777777778;
/// 's amount factors: a component's, the global phase's.
const PHASE_AMOUNT: f64 = 0.1;
const GLOBAL_PHASE_AMOUNT: f64 = 0.0002777777778;

/// Evaluate a texture at a point already in texture space, from a texture not evaluated before,
/// with the orders the vectors decide for the version.
pub fn evaluate(
    settings: &TextureSettings,
    point: [f64; 3],
    surface: SurfaceContext,
    tables: TextureTables<'_>,
    version: RenderAs,
) -> Result<TextureResult, TextureError> {
    evaluate_with(settings, point, surface, tables, version, &TextureOrders::of(version))
}

/// `evaluate` with the orders given (for the vector tests that search them).
pub fn evaluate_with(
    settings: &TextureSettings,
    point: [f64; 3],
    surface: SurfaceContext,
    tables: TextureTables<'_>,
    version: RenderAs,
    orders: &TextureOrders,
) -> Result<TextureResult, TextureError> {
    evaluate_inner(settings, point, surface, tables, version, orders, None, true, None)
}

/// `evaluate` for a texture a material names in a render, with `bump_on` false where no Bump
/// Height channel names it: each component that produces a bump then gives the gradient of a
/// component whose bump is off ((0, 0, 0) as 3.1 and 5.0; none as 3.0, and a colour or value
/// made from it is refused, `OpenPoint::BumpOffKept`), and the rules that go by what the
/// components produce take it.,
pub fn evaluate_in_render(
    settings: &TextureSettings,
    point: [f64; 3],
    surface: SurfaceContext,
    tables: TextureTables<'_>,
    version: RenderAs,
    bump_on: bool,
    kept: Option<Kept>,
) -> Result<TextureResult, TextureError> {
    SHARED_CONTEXT.with(|s| s.set(surface));
    evaluate_inner(settings, point, surface, tables, version, &TextureOrders::of(version), None, bump_on, kept)
}

/// `evaluate_with` where each component's bump gradient is given (`gradients[i]` for a
/// component that produces a bump), since leaves how a component forms it Open. For the
/// vector tests only, whose companion cases give the gradients (
/// "Companions"); a render never calls it.
pub fn evaluate_with_gradients(
    settings: &TextureSettings,
    point: [f64; 3],
    surface: SurfaceContext,
    version: RenderAs,
    orders: &TextureOrders,
    gradients: &[Option<[f64; 3]>],
) -> Result<TextureResult, TextureError> {
    evaluate_inner(settings, point, surface, TextureTables::none(), version, orders, Some(gradients), true, None)
}

fn evaluate_inner(
    settings: &TextureSettings,
    point: [f64; 3],
    surface: SurfaceContext,
    tables: TextureTables<'_>,
    version: RenderAs,
    orders: &TextureOrders,
    gradients: Option<&[Option<[f64; 3]>]>,
    bump_on: bool,
    kept_state: Option<Kept>,
) -> Result<TextureResult, TextureError> {
    let n = settings.components.len();
    if n == 0 || n > MAX_COMPONENTS {
        return Err(TextureError::ComponentCount(n));
    }
    let mut needs = open_points(settings, version);
    // A bump turned off samples no points beside the point.
    if gradients.is_some() || !bump_on {
        needs.retain(|p| *p != OpenPoint::BumpGradient);
    }
    if !needs.is_empty() {
        return Err(TextureError::Open(needs));
    }
    let open = |p: OpenPoint| TextureError::Open(vec![p]);
    let lat = Lattice { perm: tables.perm, waves: tables.waves, version, smooth: orders.smoothstep };
    let noises = Noises { lat, bits: tables.noise_bits_five, sets: std::cell::OnceCell::new() };
    // : the global phase moves the texture's point before any component.
    let mut p = point;
    let mut beside: Option<[[f64; 3]; 3]> = None;
    if global_phase_acts(&settings.phase, version) {
        // : where a component makes a bump (its bump on, its gradient formed here)
        // and the amount is not exactly 0, the three points beside the point are moved too.
        if bump_on
            && gradients.is_none()
            && settings.phase.amount != 0.0
            && settings.components.iter().any(|c| c.outputs.bump)
        {
            // , with a bump P0 is moved as moves it.
            let (p0, three) = displaced_beside(&settings.phase, p, orders, &noises).map_err(open)?;
            beside = Some(three);
            p = p0;
            if kept_state.is_some() {
                SHARED_POINTS.with(|c| c.set(three));
            }
        } else {
            p = displaced(&settings.phase, p, None, GLOBAL_PHASE_AMOUNT, orders, &noises).map_err(open)?;
            // ,: as 3.0 a global phase whose
            // amount is exactly 0, under a bump, leaves the bump the points an earlier
            // evaluation of any texture left.
            if bump_on
                && gradients.is_none()
                && kept_state.is_some()
                && settings.components.iter().any(|c| c.outputs.bump)
            {
                beside = Some(SHARED_POINTS.with(|c| c.get()));
                // The points an evaluation before this one left: order-dependent (multithread.md).
                SHARED_TOUCHED.with(|c| c.set(true));
            }
        }
    }
    let mut results = Vec::with_capacity(n);
    // (The point q of modes 7, 8, 10, 13, 14 and blend mode 14: the first
    // component's point in its noise space as its evaluation leaves it)
    let mut pn = PermNoise { lattice: lat, q: [f64::NAN; 3], version };
    for (i, c) in settings.components.iter().enumerate() {
        // A sample of the component at a point of its noise space: its phase, its octave
        // value, its filter.
        let sample = |q: [f64; 3]| -> Result<(Octaves, f64), TextureError> {
            let mut q = q;
            if component_phase_acts(&c.phase) {
                q = displaced(&c.phase, q, Some(PHASE_SPACE_FACTOR), PHASE_AMOUNT, orders, &noises).map_err(open)?;
            }
            let o = octave(&c.noise, q, orders, &noises).map_err(open)?;
            let v = filtered(&c.filter, o.value, surface, version, orders.smoothstep)
                .ok_or(open(OpenPoint::Filter(c.filter.number)))?;
            Ok((o, v))
        };
        let map = SpaceMap::of(&c.space, orders.space);
        let q = map.apply(p);
        let (o, v) = sample(q)?;
        if i == 0 {
            pn.q = o.left;
        }
        // : a component that produces a colour has its own map at its own value.
        let colour = match c.outputs.colour {
            true => Some(colours::map_colour(&c.colours, v, surface, orders.ease, &pn).map_err(open)?),
            false => None,
        };
        let kept = c.outputs.bump && !bump_on && version == RenderAs::Bryce30;
        let gradient = match (c.outputs.bump, gradients) {
            (true, _) if !bump_on => (!kept).then_some([0.0; 3]),
            (true, Some(g)) => Some(g.get(i).copied().flatten().ok_or(open(OpenPoint::BumpGradient))?),
            (true, None) => {
                // : qj = q + 0.001·(column
                // j of M), each sampled as q is; dj = sj − v; the gradient d/ℓ with
                // ℓ = r(dx² + dy² + dz² + 0.5/(n + 1)).
                let mut d = [0.0; 3];
                for (j, dj) in d.iter_mut().enumerate() {
                    let qj = match &beside {
                        // : the moved point beside the point, in this noise space
                        Some(moved) => map.apply(moved[j]),
                        None => {
                            let col = map.column(j);
                            std::array::from_fn(|r| q[r] + BUMP_STEP * col[r])
                        }
                    };
                    *dj = sample(qj)?.1 - v;
                }
                let term = 0.5 / (c.noise.octaves as f64 + 1.0);
                let l = sqrt_as(version, orders.bump.eval(orders.bump_squares.arrange(d.map(|e| e * e)), term));
                Some(d.map(|e| e / l))
            }
            _ => None,
        };
        results.push(ComponentResult { produces: c.outputs, value: v, colour, gradient, kept, map: c.colours });
    }
    blend::finish(settings, &results, surface, version, orders, &pn, kept_state.as_ref()).map_err(open)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space() -> NoiseSpace {
        NoiseSpace { angles: [0.0; 2], frequencies: [1.0; 3] }
    }

    fn noise(t: i64) -> NoiseSettings {
        NoiseSettings { type_number: t, method: 0, octaves: 1, dimension: Dimension::Three }
    }

    fn no_phase() -> Phase {
        Phase { amount: 0.0, noise: NoiseSettings { octaves: 0, ..noise(0) }, space: space() }
    }

    fn filter() -> Filter {
        Filter { number: 0, n: 0, a: 1.0, b: 0.0 }
    }

    fn colours() -> ColourMap {
        ColourMap { mode: 0, space: ColourSpace::Rgb, a: [128; 3], b: [0; 3], c: [255; 3] }
    }

    fn component(t: i64) -> Component {
        Component {
            outputs: OutputSet { value: true, ..Default::default() },
            noise: noise(t),
            space: space(),
            phase: no_phase(),
            filter: filter(),
            colours: colours(),
        }
    }

    fn texture(components: Vec<Component>) -> TextureSettings {
        TextureSettings {
            outputs: OutputSet { value: true, ..Default::default() },
            components,
            blend: [3, 4],
            phase: no_phase(),
            filter: filter(),
            colours: colours(),
        }
    }

    #[test]
    fn families_by_version() {
        assert_eq!(noise_family(26, RenderAs::Bryce30), NoiseFamily::Three);
        assert_eq!(noise_family(27, RenderAs::Bryce31), NoiseFamily::Outside);
        assert_eq!(noise_family(27, RenderAs::Bryce50), NoiseFamily::Five);
        assert_eq!(noise_family(49, RenderAs::Bryce50), NoiseFamily::Five);
        assert_eq!(noise_family(50, RenderAs::Bryce50), NoiseFamily::Outside);
        assert_eq!(noise_family(-1, RenderAs::Bryce50), NoiseFamily::Outside);
    }

    /// : type 0 is 0.5 under every method, and needs nothing Open.
    #[test]
    fn type_0_everywhere() {
        let mut c = component(0);
        c.noise.method = 11;
        c.noise.octaves = 5;
        let t = texture(vec![c]);
        assert!(open_points(&t, RenderAs::Bryce31).is_empty());
        let r = evaluate(&t, [3.0, -7.0, 1e5], SurfaceContext::default(), TextureTables::none(), RenderAs::Bryce31);
        assert_eq!(r.unwrap().value, Some(0.5));
    }

    #[test]
    fn what_stays_open() {
        let mut a = component(30);
        a.phase = Phase { amount: 2.0, noise: NoiseSettings { octaves: 10, ..noise(5) }, space: space() };
        a.outputs = OutputSet { bump: true, ..Default::default() };
        let mut b = component(2);
        b.noise.method = 11;
        let mut t = texture(vec![a, b, component(17)]);
        t.outputs = OutputSet { colour: true, value: false, bump: true };
        t.blend = [14, 3];
        t.colours.mode = 7;
        // A global phase that acts (as 3.0, whatever its amount): the bump's points are Open.
        t.phase = Phase { amount: 0.0, noise: noise(1), space: space() };
        let got = open_points(&t, RenderAs::Bryce30);
        for p in [OpenPoint::OctaveCount(10), OpenPoint::NoiseTypeOutside(30)] {
            assert!(got.contains(&p), "{p:?} in {got:?}");
        }
        // Stated since's revisions and : the lattice types
        // type 17, method 11, blend mode 14 and the colour modes that read `perm`.
        for p in [
            OpenPoint::NoiseType(5),
            OpenPoint::NoiseType(17),
            OpenPoint::OctaveMethod(11),
            OpenPoint::Blend(14),
            OpenPoint::ColourMode(7),
            OpenPoint::Blend(3),
            OpenPoint::ColourMode(0),
        ] {
            assert!(!got.contains(&p), "{p:?} in {got:?}");
        }
        assert_eq!(OpenPoint::NoiseType(5).entry(), "TX-2.1");
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), got.len(), "each point once");
    }

    #[test]
    fn component_count_limits() {
        let none = texture(vec![]);
        let four = texture(vec![component(1); 4]);
        let s = SurfaceContext::default();
        assert_eq!(
            evaluate(&none, [0.0; 3], s, TextureTables::none(), RenderAs::Bryce30),
            Err(TextureError::ComponentCount(0))
        );
        assert_eq!(
            evaluate(&four, [0.0; 3], s, TextureTables::none(), RenderAs::Bryce30),
            Err(TextureError::ComponentCount(4))
        );
    }

    /// : one component that
    /// produces only a bump, its bump turned off, the texture asked for everything: as 3.1 and
    /// 5.0's grey (128, 128, 128) and the value 0.5; as 3.0 the gradient rests on what the
    /// texture kept, refused.
    #[test]
    fn a_bump_turned_off() {
        let mut c = component(1);
        c.outputs = OutputSet { bump: true, ..Default::default() };
        c.space.frequencies = [8.0; 3];
        let mut t = texture(vec![c]);
        t.outputs = OutputSet { colour: true, value: true, bump: true };
        let s = SurfaceContext::default();
        let p = [0.3, -0.2, 0.7];
        for v in [RenderAs::Bryce31, RenderAs::Bryce50] {
            let r = evaluate_in_render(&t, p, s, TextureTables::none(), v, false, None).unwrap();
            assert_eq!((r.colour, r.value, r.bump), (Some([128; 3]), Some(0.5), Some([0.0; 3])), "{v:?}");
            let on = evaluate_in_render(&t, p, s, TextureTables::none(), v, true, None).unwrap();
            assert_eq!(on, evaluate(&t, p, s, TextureTables::none(), v).unwrap(), "{v:?}: on as outside a render");
        }
        assert_eq!(
            evaluate_in_render(&t, p, s, TextureTables::none(), RenderAs::Bryce30, false, None),
            Err(TextureError::Open(vec![OpenPoint::BumpOffKept]))
        );
        // : in a render the texture holds its saved gradient; as 3.0 the
        // colour is made from it (a saved (0, 0, 0) gives's grey).
        let held = Kept { colour: [0; 3], gradient: [0.0; 3] };
        let r = evaluate_in_render(&t, p, s, TextureTables::none(), RenderAs::Bryce30, false, Some(held)).unwrap();
        assert_eq!((r.colour, r.bump), (Some([128; 3]), Some([0.0; 3])));
    }
}

#[cfg(test)]
mod beside_tests {
    use super::*;

    /// : a global phase of a vanishing amount moves the four points
    /// by nothing, so the bump gradient agrees with the one of a texture with no phase to
    /// rounding of the points (p + 0.001 on an axis against q + 0.001 times a column).
    #[test]
    fn vanishing_phase_gives_the_plain_gradient() {
        let noise = |t| NoiseSettings { type_number: t, method: 0, octaves: 1, dimension: Dimension::Three };
        let space = NoiseSpace { angles: [0.0; 2], frequencies: [8.0; 3] };
        let none = Phase { amount: 0.0, noise: NoiseSettings { octaves: 0, ..noise(0) }, space };
        let filter = Filter { number: 0, n: 0, a: 1.0, b: 0.0 };
        let colours = ColourMap { mode: 0, space: ColourSpace::Rgb, a: [128; 3], b: [0; 3], c: [255; 3] };
        let comp = Component {
            outputs: OutputSet { bump: true, ..Default::default() },
            noise: noise(1),
            space,
            phase: none,
            filter,
            colours,
        };
        let mut t = TextureSettings {
            outputs: OutputSet { bump: true, ..Default::default() },
            components: vec![comp],
            blend: [3, 4],
            phase: none,
            filter,
            colours,
        };
        let s = SurfaceContext::default();
        let p = [0.3, -0.2, 0.7];
        let plain = evaluate(&t, p, s, TextureTables::none(), RenderAs::Bryce50).unwrap().bump.unwrap();
        t.phase = Phase { amount: 1e-300, noise: noise(1), space };
        let moved = evaluate(&t, p, s, TextureTables::none(), RenderAs::Bryce50).unwrap().bump.unwrap();
        for k in 0..3 {
            assert!((plain[k] - moved[k]).abs() < 1e-6, "{plain:?} {moved:?}");
        }
    }
}
