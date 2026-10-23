// SPDX-License-Identifier: MIT
//! The multithreaded render of the first pass and the pass over edges.
//!
//! The picture is the same bytes as the serial render's: this runs only where no pixel's
//! result depends on the pixels drawn before it (the checks in `try_render`), and where the
//! one thing that does depend on order, the sample places drawn from the generator, is
//! reproduced exactly (the generator is stepped serially over the whole picture's places
//! first, and each chunk of pixels starts from the state it would have had).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::lit::{PlainSky, PlainSurface, plain_rules_from};
use super::plan::{OpenChoices, PictureWay, RenderError, RenderPlan};
use super::stages::{PIXELS_PER_BATCH, Picture, RenderCounts, Stages};
use crate::geometry::metaball::{MetaCarry, MetaMarks};
use crate::numerics::CRand;
use crate::sampling::aa_pass::{EXTRA_RAYS, RefineRule, edge_mask_in};
use crate::sampling::output::{PostOptions, channel_level};
use crate::sampling::{AaSetting, PixelSum, PixelVisit};
use crate::scene::Scene;
use crate::version::RenderAs;

/// The least pixels a chunk of work holds: small pictures are chunked finely too, so that the
/// tests exercise the hand-over between chunks (a chunk costs a generator state, not a thread).
const MIN_CHUNK: usize = 16;

/// The most pixels a chunk holds: a finished chunk is what the live picture shows and where a
/// closed pipe is noticed, so chunks stay small enough to finish within moments.
const MAX_CHUNK: usize = 1024;

/// The numbers of the generator's sequence between the places two pixels start from when every
/// pixel has a place of its own (`SharedDraws::Independent`): an odd number far above what a
/// pixel draws. The same stride as the old recomp's.
const RANDOM_STRIDE: u64 = 1_000_003;

/// `STECYK3D_TRACE_THREADS=1` says on standard error which way a render went (for the tests and for
/// finding out why a document stays serial).
fn trace(what: &str) {
    if std::env::var_os("STECYK3D_TRACE_THREADS").is_some() {
        eprintln!("threads: {what}");
    }
}

/// The number of worker threads for a `threads` choice (0: every core).
pub fn worker_count(threads: usize) -> usize {
    if threads == 0 { std::thread::available_parallelism().map_or(1, |n| n.get()) } else { threads }
}

/// What the workers share, read-only: the frame's plan and the sky as the serial render starts
/// them (the star list already made). A worker takes its own copies (a plan and a sky are plain
/// data, copied in a moment; the geometry build and the star list are paid once, not per
/// thread). No `unsafe`: every cell in a plan or a sky is a lock or an atomic (the metaballs'
/// record, the angle law's met objects, a texture's held colour), so `Shared` is `Sync` by
/// construction, and `assert_shared_is_sync` below fails to compile if someone puts a
/// `Cell`, `RefCell` or `Rc` into them. What the workers do not share are the rules (the
/// surface and the sky as made from the scene), which each thread makes for itself: the sky's
/// light-in-the-air gather would share one met-objects record between workers, so a scene that
/// has one is never shared (`PlainSky::shareable`).
struct Shared<'a> {
    plan: &'a RenderPlan,
    sky: &'a PlainSky,
    /// The caller's surface limits (the scattered preview's are the quick preview's).
    depth_limit: u32,
    soft_grid: u32,
}

const _: fn() = || {
    fn assert_shared_is_sync<T: Sync>() {}
    assert_shared_is_sync::<Shared<'static>>();
};

/// One worker's own plan, rules and batches (the rules hold cells that a thread owns alone, so each thread
/// makes its own from the shared, read-only document).
struct Worker {
    plan: RenderPlan,
    surface: PlainSurface,
    sky: PlainSky,
    stages: Stages,
}

fn make_worker(scene: &Scene, shared: &Shared<'_>) -> Result<Worker, RenderError> {
    let t0 = std::time::Instant::now();
    let mut plan = shared.plan.clone();
    // The record of what the hits on sets of metaballs showed is this worker's own (the clones of a plan
    // otherwise share one): the worker follows it from the state each job is handed.
    if let Some(ms) = plan.geometry.scene.metaballs.as_mut() {
        *ms = ms.forked();
    }
    let (mut surface, sky) = plain_rules_from(scene, &plan, shared.sky.clone())?;
    surface.depth_limit = shared.depth_limit;
    surface.soft_grid = shared.soft_grid;
    trace(&format!("worker made in {:.3}s", t0.elapsed().as_secs_f64()));
    Ok(Worker { plan, surface, sky, stages: Stages::new() })
}

/// What a worker brings back from one job: its part, its counts, and (where the angle law's
/// state is followed) the objects its rays asked about.
struct Part<T> {
    part: T,
    counts: RenderCounts,
    asked: Vec<bool>,
    /// The metaballs' record: what the job read and wrote of it, and how it stood at the job's end.
    meta: Option<(MetaMarks, MetaCarry)>,
}

/// The state a job starts from where state is followed: the angle law's met objects, and the metaballs' record.
#[derive(Clone)]
struct Guess {
    met: Option<Vec<bool>>,
    meta: Option<MetaCarry>,
}

fn add_counts(a: &mut RenderCounts, b: &RenderCounts) {
    a.samples += b.samples;
    a.surface += b.surface;
    a.sky += b.sky;
    a.refined += b.refined;
    a.further += b.further;
}

/// The serve mode's stop flag (`stecyk3d-render --serve`): set when a newer request comes, it ends the render going
/// on at the next batch of every worker. A render that sees it gives `Refused { entry: "serve", .. }`, no picture.
/// Never set by a one-shot run. (The long-lived renderer)
static CANCEL: AtomicBool = AtomicBool::new(false);

/// Set once by `stecyk3d-render --serve`: chunks of work are as small as they go (`chunk_limit`).
static SERVING: AtomicBool = AtomicBool::new(false);

/// The server's renders are many and unfinished: the viewport shows a chunk the moment it is done and the next
/// view must not wait for a chunk of hundreds of pixels (seconds, on a scene of many triangles) to give its
/// first rows. A chunk costs a generator state and a message, not a thread, so small ones lose nothing worth
/// naming. The pictures are the same bytes: a pixel's value is its own, whatever the chunks.
pub fn set_serving(on: bool) {
    SERVING.store(on, Ordering::Relaxed);
}

/// The most pixels a chunk holds in this process.
fn chunk_limit() -> usize {
    if SERVING.load(Ordering::Relaxed) { MIN_CHUNK } else { MAX_CHUNK }
}

/// Ask (or stop asking) the renders going on to stop.
pub fn set_cancel(on: bool) {
    CANCEL.store(on, Ordering::SeqCst);
}

/// True while a stop is asked for.
pub fn cancelled() -> bool {
    CANCEL.load(Ordering::Relaxed)
}

/// The serial render's check between batches: the same refusal the threaded render gives, so a stop reaches a render
/// that could not be threaded (a lens, a texture that keeps a result) as soon as the threaded one.
pub fn stop_if_cancelled() -> Result<(), RenderError> {
    if cancelled() {
        return Err(RenderError::Refused { entry: "serve", reason: "cancelled by a newer request".to_owned() });
    }
    Ok(())
}

/// Run the jobs `todo` on `n` threads, each worker making its own rules; `job(worker, i, counts)`
/// gives job i's part. Where `guess` is given, a job starts from the angle law's state
/// `guess[i]` (the objects met) and reports the objects it asked about. Returns `None` where a
/// worker found state that makes the picture depend on order (or an error), so that the caller
/// renders serially.
fn run_jobs<T: Send>(
    scene: &Scene,
    shared: &Shared<'_>,
    n: usize,
    todo: &[usize],
    guess: Option<&[Guess]>,
    job: &(dyn Fn(&mut Worker, usize, &mut RenderCounts) -> Result<Option<T>, RenderError> + Sync),
    on_part: &mut dyn FnMut(usize, &T),
    in_order: bool,
) -> Option<Vec<(usize, Part<T>)>> {
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let unusable = AtomicBool::new(false);
    let failed = AtomicBool::new(false);
    let mut all: Vec<(usize, Part<T>)> = Vec::new();
    let (tx, rx) = std::sync::mpsc::channel::<(usize, Part<T>)>();
    let (next, stop, unusable, failed) = (&next, &stop, &unusable, &failed);
    std::thread::scope(|scope| {
        for _ in 0..n.min(todo.len().max(1)) {
            let tx = tx.clone();
            scope.spawn(move || {
                match make_worker(scene, shared) {
                    Err(_) => {
                        stop.store(true, Ordering::Relaxed);
                        failed.store(true, Ordering::Relaxed);
                    }
                    Ok(mut w) => {
                        crate::textures::reset_shared_points();
                        crate::textures::note_textured_document(w.surface.has_textures());
                        while !stop.load(Ordering::Relaxed) && !cancelled() {
                            let k = next.fetch_add(1, Ordering::Relaxed);
                            if k >= todo.len() {
                                break;
                            }
                            let i = todo[k];
                            if let Some(g) = guess {
                                if let Some(m) = &g[i].met {
                                    w.surface.met().load(m);
                                }
                                if let (Some(c), Some(ms)) = (&g[i].meta, w.plan.geometry.scene.metaballs.as_ref()) {
                                    ms.load(c);
                                }
                            }
                            let mut counts = RenderCounts::default();
                            match job(&mut w, i, &mut counts) {
                                Ok(Some(part)) => {
                                    let asked = if guess.is_some_and(|g| g[i].met.is_some()) { w.surface.met().asked() } else { Vec::new() };
                                    let meta = match (guess.is_some_and(|g| g[i].meta.is_some()), w.plan.geometry.scene.metaballs.as_ref()) {
                                        (true, Some(ms)) => Some((ms.marks(), ms.carry())),
                                        _ => None,
                                    };
                                    // The calling thread owns the output: it gets each part as
                                    // it is done (a closed receiver only ends the loop).
                                    let _ = tx.send((i, Part { part, counts, asked, meta }));
                                }
                                Ok(None) => {
                                    unusable.store(true, Ordering::Relaxed);
                                    stop.store(true, Ordering::Relaxed);
                                }
                                Err(_) => {
                                    // An error may come from state the order changed (or be the
                                    // document's own): either way the serial render decides it.
                                    stop.store(true, Ordering::Relaxed);
                                    failed.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                        if crate::textures::take_shared_touched() {
                            unusable.store(true, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
        drop(tx);
        // The single writer: parts reach `on_part` here, in the order they finish.
        if in_order {
            // The jobs are taken in order, so the parts that wait for an earlier one are few (one a thread). A part
            // reaches `on_part` only when every job before it has: a picture that grows from the top without gaps.
            let mut waiting: std::collections::BTreeMap<usize, Part<T>> = std::collections::BTreeMap::new();
            let mut released = 0;
            for (i, p) in rx {
                waiting.insert(i, p);
                while released < todo.len() {
                    let Some(p) = waiting.remove(&todo[released]) else { break };
                    // Workers outrun the pipe, so a backlog of finished parts is queued here when the stop comes;
                    // writing it out would keep the reader busy with a picture nobody wants (serve mode).
                    if !cancelled() {
                        on_part(todo[released], &p.part);
                    }
                    all.push((todo[released], p));
                    released += 1;
                }
            }
            for (i, p) in waiting {
                all.push((i, p)); // jobs after a stopped one: finished, never shown
            }
        } else {
            for (i, p) in rx {
                if !cancelled() {
                    on_part(i, &p.part);
                }
                all.push((i, p));
            }
        }
    });
    if cancelled() || failed.load(Ordering::Relaxed) || unusable.load(Ordering::Relaxed) {
        return None;
    }
    Some(all)
}

/// The angle law's state: what a job's rays asked about agrees with the state they started
/// from and the state the serial order would have given them, for every object whose law can
/// be in force.
fn agrees(guess: &[bool], actual: &[bool], asked: &[bool], capable: &[bool]) -> bool {
    asked.iter().enumerate().all(|(o, &a)| {
        !a || !capable.get(o).copied().unwrap_or(true) || guess.get(o).copied().unwrap_or(false) == actual.get(o).copied().unwrap_or(false)
    })
}

/// The metaballs' record as it stands after a job, from the state before it: a part the job wrote is the job's
/// (last writer wins), a part it did not write stays as it was.
fn after_job(before: &MetaCarry, marks: MetaMarks, end: &MetaCarry) -> MetaCarry {
    MetaCarry {
        latest: if marks.wrote_latest { end.latest } else { before.latest },
        last_set: if marks.wrote_set { end.last_set.clone() } else { before.last_set.clone() },
    }
}

/// Whether a job that started from `guess` stands where the order gives it `actual`: it agrees on everything it asked.
fn meta_agrees(guess: &MetaCarry, actual: &MetaCarry, marks: MetaMarks) -> bool {
    (!marks.asked_latest || guess.latest == actual.latest) && (!marks.asked_set || guess.last_set == actual.last_set)
}

/// All `jobs` jobs, in the picture's order, as `(parts, counts, state after)`. Where the angle
/// law can be in force (`law`: the capable objects and the state before the first job), or where the scene has
/// metaballs (`meta`: the record's state before the first job), the jobs run on a guess of the state they start from
/// (the angle law: everything met; the metaballs: the record as the frame starts it; the first job's own is exact)
/// and each is checked against the state the order gives it, from what the jobs before it asked about and wrote: it
/// stands if the two agree on everything it asked about, and is run again from the exact state otherwise (the first
/// job not yet right is always run from an exact state, so this ends). The picture is then what the serial order
/// gives. This is the old recomp's rounds (`docs/rendering-on-several-cores.md`, "The state each stretch starts from
/// is found, not assumed"), with the same two marks (asked, written) on each kept thing.
#[allow(clippy::type_complexity)]
fn run_ordered<T: Send>(
    scene: &Scene,
    shared: &Shared<'_>,
    n: usize,
    jobs: usize,
    law: Option<(&[bool], &[bool])>,
    meta: Option<&MetaCarry>,
    job: &(dyn Fn(&mut Worker, usize, &mut RenderCounts) -> Result<Option<T>, RenderError> + Sync),
    on_part: &mut dyn FnMut(usize, &T),
    in_order: bool,
) -> Option<(Vec<T>, RenderCounts, Option<Vec<bool>>, Option<MetaCarry>)> {
    if law.is_none() && meta.is_none() {
        let todo: Vec<usize> = (0..jobs).collect();
        let done = run_jobs(scene, shared, n, &todo, None, job, on_part, in_order)?;
        let mut counts = RenderCounts::default();
        let mut parts: Vec<Option<T>> = (0..jobs).map(|_| None).collect();
        for (i, p) in done {
            add_counts(&mut counts, &p.counts);
            parts[i] = Some(p.part);
        }
        return Some((parts.into_iter().map(|p| p.expect("every job done")).collect(), counts, None, None));
    }
    let (capable, before): (&[bool], &[bool]) = law.unwrap_or((&[], &[]));
    let len = before.len().max(capable.len());
    let mut start = before.to_vec();
    start.resize(len, false);
    let meta_start = meta.cloned();
    let mut guesses: Vec<Guess> = (0..jobs)
        .map(|i| Guess {
            met: law.map(|_| if i == 0 { start.clone() } else { vec![true; len] }),
            meta: meta_start.clone(),
        })
        .collect();
    let mut results: Vec<Option<Part<T>>> = (0..jobs).map(|_| None).collect();
    let mut stale: Vec<Vec<bool>> = vec![Vec::new(); jobs];
    let mut stale_meta: Vec<Option<(MetaMarks, MetaCarry)>> = vec![None; jobs];
    let mut rounds = 0;
    loop {
        let todo: Vec<usize> = (0..jobs).filter(|&i| results[i].is_none()).collect();
        if todo.is_empty() {
            break;
        }
        rounds += 1;
        for (i, p) in run_jobs(scene, shared, n, &todo, Some(&guesses), job, on_part, in_order)? {
            results[i] = Some(p);
        }
        // The state the order gives each job, from what the jobs before it asked and wrote so far (exact up
        // to the first job that does not stand, an estimate after it).
        let mut actual = start.clone();
        let mut meta_actual = meta_start.clone();
        for i in 0..jobs {
            let asked = results[i].as_ref().map(|p| p.asked.clone()).unwrap_or_else(|| stale[i].clone());
            let mut marks_end = results[i].as_ref().and_then(|p| p.meta.clone()).or_else(|| stale_meta[i].clone());
            if let Some(p) = &results[i] {
                let met_ok = match &guesses[i].met {
                    Some(g) => agrees(g, &actual, &p.asked, capable),
                    None => true,
                };
                let meta_ok = match (&p.meta, &meta_actual, &guesses[i].meta) {
                    (Some((marks, _)), Some(actual), Some(g)) => meta_agrees(g, actual, *marks),
                    _ => true,
                };
                if !(met_ok && meta_ok) {
                    stale[i] = p.asked.clone();
                    stale_meta[i] = p.meta.clone();
                    results[i] = None;
                    if law.is_some() {
                        guesses[i].met = Some(actual.clone());
                    }
                    guesses[i].meta = meta_actual.clone();
                    marks_end = stale_meta[i].clone();
                }
            }
            if actual.len() < asked.len() {
                actual.resize(asked.len(), false);
            }
            for (o, &a) in asked.iter().enumerate() {
                if a {
                    actual[o] = true;
                }
            }
            if let (Some(a), Some((marks, end))) = (meta_actual.as_mut(), marks_end) {
                *a = after_job(a, marks, &end);
            }
        }
        if rounds > jobs + 2 {
            return None;
        }
        if results.iter().all(Option::is_some) {
            trace(&format!("state followed in {rounds} rounds"));
            let mut counts = RenderCounts::default();
            let mut parts = Vec::with_capacity(jobs);
            for r in results {
                let p = r.expect("every job stands");
                add_counts(&mut counts, &p.counts);
                parts.push(p.part);
            }
            return Some((parts, counts, law.map(|_| actual), meta_actual));
        }
    }
    None
}

/// The render on `choices.threads` threads, or `None` (the caller renders serially) where
/// the picture could depend on the order its pixels are drawn in, or the render is of a kind
/// this does not take. `plan` is the frame's plan as the serial render would start it, and
/// is changed (its generator) only where this returns `Some`.
pub fn try_render(
    scene: &Scene,
    version: RenderAs,
    choices: &OpenChoices,
    plan: &mut RenderPlan,
    surface: &PlainSurface,
    order: Option<Vec<PixelVisit>>,
    emit: &mut dyn FnMut(u32, u32, &[u8]),
) -> Option<Result<(Picture, RenderCounts), RenderError>> {
    let done = try_render_inner(scene, version, choices, plan, surface, order, emit);
    if done.is_none() && cancelled() {
        // Not the serial fallback: the render was stopped (serve mode).
        return Some(Err(RenderError::Refused { entry: "serve", reason: "cancelled by a newer request".to_owned() }));
    }
    done
}

fn try_render_inner(
    scene: &Scene,
    version: RenderAs,
    choices: &OpenChoices,
    plan: &mut RenderPlan,
    surface: &PlainSurface,
    order: Option<Vec<PixelVisit>>,
    emit: &mut dyn FnMut(u32, u32, &[u8]),
) -> Option<Result<(Picture, RenderCounts), RenderError>> {
    use super::stages::{SkyRules, SurfaceRules};
    let n = worker_count(choices.threads.unwrap_or(1));
    let refusal = if n <= 1 {
        Some("one thread asked for")
    } else if choices.go_on || plan.way != PictureWay::Passes || plan.saved.is_some() {
        Some("scattered, go-on or non-pass picture")
    } else if !plan.independent_draws && (plan.lens.is_some() || plan.draws_in_trees() || surface.draws()) {
        Some("a lens or shading draws from the generator")
    } else if surface.holds_texture_state() {
        Some("a texture keeps a result between evaluations")
    } else {
        None
    };
    if let Some(why) = refusal {
        trace(&format!("serial: {why}"));
        return None;
    }
    let (w, h) = (plan.width, plan.height);
    // The live picture in row order (`--rows`): one job a row, and the rows reach `emit` strictly from the top, the
    // threads working a few rows ahead. (Live order)
    let rows_order = choices.scattered_rows && order.is_some();
    let visits: Vec<PixelVisit> = order.unwrap_or_else(|| plan.order().collect());
    // Each pixel of the region visited once, and no visit's square (a visit of block
    // size b > 1 writes its levels over the b by b square, to be overwritten by the finer passes)
    // reaching a pixel visited before it: then every pixel's final level is its own sum's, and
    // the order the squares were laid in does not show.
    let r = plan.region;
    let mut seen = vec![false; w as usize * h as usize];
    let mut ok = visits.len() == ((r.right - r.left) as usize) * ((r.bottom - r.top) as usize);
    for v in &visits {
        if !ok {
            break;
        }
        if !plan.in_region(v.x, v.y) || v.x >= w || v.y >= h {
            ok = false;
            break;
        }
        let (x1, y1) = (v.x.saturating_add(v.block).min(r.right), v.y.saturating_add(v.block).min(r.bottom));
        'square: for y in v.y..y1 {
            for x in v.x..x1 {
                if seen[(y * w + x) as usize] {
                    ok = false;
                    break 'square;
                }
            }
        }
        seen[(v.y * w + v.x) as usize] = true;
    }
    if !ok {
        trace("serial: the visits do not cover each pixel once");
        return None;
    }
    drop(seen);
    if visits.len() < 2 * MIN_CHUNK {
        trace("serial: too few pixels");
        return None;
    }

    // The generator at the start of each chunk of visits: the places are the only draws, and
    // they are drawn in the picture's order. The sky's star list comes first, as in the
    // serial render (point 3).
    let mut chunk = (visits.len() / (n * 8)).clamp(MIN_CHUNK, chunk_limit());
    if rows_order {
        chunk = (r.right - r.left).max(1) as usize;
    }
    let chunks: Vec<&[PixelVisit]> = visits.chunks(chunk).collect();
    let original = plan.generator;
    let mut gstate = original;
    let mut start_sky = match PlainSky::new(scene, plan) {
        Ok(s) => s,
        Err(e) => return Some(Err(e.into())),
    };
    if let Err(e) = start_sky.before_first_pixel(scene, &mut gstate) {
        return Some(Err(e.into()));
    }
    if !start_sky.shareable() {
        trace("serial: the sky holds state that cannot be shared");
        return None;
    }
    // A cloud layer reads the shared context the last texture evaluation left (as 3.0 and 3.1):
    // which one was last depends on the order, so the workers would find it and give up. Not
    // started at all.
    if version != RenderAs::Bryce50 && start_sky.reads_texture_context() && surface.has_textures() {
        trace("serial: a cloud layer reads the texture context the last evaluation left");
        return None;
    }
    // The angle law : the objects whose law can act, and the state the render starts
    // from (the quick preview before it has met objects).
    let capable = surface.law_capable();
    let met_before = surface.met().snapshot();
    let law = capable.as_deref().map(|c| (c, met_before.as_slice()));
    let base = plan.clone();
    let shared = Shared { plan: &base, sky: &start_sky, depth_limit: surface.depth_limit, soft_grid: surface.soft_grid };
    // A place of its own in the generator's sequence for every pixel (`SharedDraws::Independent`): the
    // pixel draws everything it needs (its places, its shading) from there, so the chunks need no
    // stepping and the picture is the same on any number of threads (not the program's numbers).
    let independent = plan.independent_draws && (plan.lens.is_some() || plan.draws_in_trees() || surface.draws());
    let draws = !independent && !matches!(plan.placer.setting, AaSetting::Centre | AaSetting::Corner | AaSetting::CentreAgain);
    let mut starts: Vec<CRand> = Vec::with_capacity(chunks.len());
    let mut stepper = Stages::new();
    for c in &chunks {
        starts.push(gstate);
        if draws {
            plan.generator = gstate;
            for batch in c.chunks(PIXELS_PER_BATCH) {
                stepper.places(plan, batch);
            }
            gstate = plan.generator;
        }
    }
    drop(stepper);
    plan.generator = original;

    let post = plan.post.unwrap_or(PostOptions { gamma: false, dither: false });
    let meta_before = plan.geometry.scene.metaballs.as_ref().map(|m| m.carry());
    let first = run_ordered(scene, &shared, n, chunks.len(), law, meta_before.as_ref(), &|w, i, counts| {
        let part = chunks[i];
        w.plan.generator = starts[i];
        let table = w.plan.dither_table.clone();
        let mut out: Vec<[u8; 3]> = Vec::with_capacity(part.len());
        if independent {
            for v in part {
                if cancelled() {
                    return Ok(None);
                }
                w.plan.generator = starts[0].skip(u64::from(v.y * w.plan.width + v.x) * RANDOM_STRIDE);
                let mut s = PixelSum::default();
                w.stages.premium_samples(scene, &mut w.plan, &mut w.surface, &mut w.sky, std::slice::from_ref(v), counts, |_, rgb| s.add(rgb))?;
                let c = s.colour(w.plan.scale);
                out.push(c.map(|ch| channel_level(w.plan.version, ch, v.x, v.y, post, table.as_deref())));
            }
            return Ok(Some(out));
        }
        for batch in part.chunks(PIXELS_PER_BATCH) {
            if cancelled() {
                return Ok(None);
            }
            w.stages.places(&mut w.plan, batch);
            w.stages.rays(&w.plan);
            w.stages.hits(&w.plan);
            let mut sums: Vec<(u32, PixelSum)> = Vec::with_capacity(batch.len());
            w.stages.colours(scene, &w.plan, &mut w.surface, &mut w.sky, counts, None, |pixel, rgb| {
                match sums.last_mut() {
                    Some((p, s)) if *p == pixel => s.add(rgb),
                    _ => {
                        let mut s = PixelSum::default();
                        s.add(rgb);
                        sums.push((pixel, s));
                    }
                }
            })?;
            // One run of samples a pixel, in the batch's order (as stage 5 reads them).
            if sums.len() != batch.len() || sums.iter().zip(batch).any(|((p, _), v)| *p != v.y * w.plan.width + v.x) {
                return Ok(None);
            }
            for ((_, s), v) in sums.iter().zip(batch) {
                let c = s.colour(w.plan.scale);
                out.push(c.map(|ch| channel_level(w.plan.version, ch, v.x, v.y, post, table.as_deref())));
            }
        }
        Ok(Some(out))
    }, &mut |i, part: &Vec<[u8; 3]>| {
        emit_runs(emit, chunks[i].iter().zip(part).map(|(v, rgb)| (v.x, v.y, *rgb)));
    }, rows_order);
    if first.is_none() {
        trace("serial: a worker found order-dependent state or an error");
    }
    let (parts, mut counts, met_after_first, meta_after_first) = first?;
    let mut levels = plan.background.clone();
    for (c, part) in chunks.iter().zip(parts) {
        for (v, rgb) in c.iter().zip(part) {
            let j = (v.y * w + v.x) as usize;
            levels[j * 3..j * 3 + 3].copy_from_slice(&rgb);
        }
    }
    // Parts went to `emit` as they finished; a fallback to the serial render emits again over them.
    let mut met_final = met_after_first.clone();
    let mut meta_final = meta_after_first.clone();

    // The pass over edges, on the finished first-pass levels: each refined pixel changes only
    // itself and reads only its own first-pass levels and its seven rays.
    if let Some(pass) = plan.edge_pass {
        let mut mask = edge_mask_in(&levels, w, h, plan.region, pass.limit);
        if plan.skip_unwritten {
            // The pixels the first pass wrote are all of the region (every visit above).
            let r = plan.region;
            let mut written = vec![false; mask.len()];
            for v in &visits {
                written[(v.y * w + v.x) as usize] = true;
            }
            let (wu, bottom, right) = (w as usize, r.bottom.min(h), r.right.min(w));
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
        let jobsize = (on_edge.len() / (n * 8)).clamp(MIN_CHUNK, chunk_limit());
        let jobs: Vec<&[u32]> = on_edge.chunks(jobsize).collect();
        let first_levels = &levels;
        let law = capable.as_deref().map(|c| (c, met_after_first.as_deref().unwrap_or(&met_before)));
        let meta_in = meta_after_first.as_ref().or(meta_before.as_ref());
        let edge = run_ordered(scene, &shared, n, jobs.len(), law, meta_in, &|wk, i, counts| {
            let rule = RefineRule { version: wk.plan.version, post, dither_table: wk.plan.dither_table.as_deref() };
            let mut gstate = wk.plan.generator;
            let mut out: Vec<[u8; 3]> = Vec::with_capacity(jobs[i].len());
            let mut extra: Vec<[f32; 3]> = Vec::with_capacity(PIXELS_PER_BATCH * EXTRA_RAYS);
            // Independent draws: one pixel a batch, each from its own place (after the first pass's).
            let batch_size = if independent { 1 } else { PIXELS_PER_BATCH };
            for batch in jobs[i].chunks(batch_size) {
                if independent {
                    gstate = starts[0].skip((u64::from(w) * u64::from(h) + u64::from(batch[0])) * RANDOM_STRIDE);
                }
                if cancelled() {
                    return Ok(None);
                }
                wk.stages.edge_places(w, &pass.offsets, batch);
                wk.stages.edge_rays(&wk.plan);
                wk.stages.hits(&wk.plan);
                extra.clear();
                wk.stages.colours(scene, &wk.plan, &mut wk.surface, &mut wk.sky, counts, Some(&mut gstate), |_, rgb| extra.push(rgb))?;
                for (k, &p) in batch.iter().enumerate() {
                    let seven: [[f32; 3]; EXTRA_RAYS] =
                        extra[k * EXTRA_RAYS..(k + 1) * EXTRA_RAYS].try_into().expect("seven rays a pixel");
                    let at = p as usize * 3;
                    let first = [first_levels[at], first_levels[at + 1], first_levels[at + 2]];
                    out.push(rule.pixel(first, &seven, p % w, p / w));
                    counts.refined += 1;
                }
            }
            Ok(Some(out))
        }, &mut |i, part: &Vec<[u8; 3]>| {
            emit_runs(emit, jobs[i].iter().zip(part).map(|(&p, rgb)| (p % w, p / w, *rgb)));
        }, false);
        if edge.is_none() {
            trace("serial: a worker found order-dependent state or an error (edge pass)");
        }
        let (parts, edge_counts, met_after_edge, meta_after_edge) = edge?;
        if meta_after_edge.is_some() {
            meta_final = meta_after_edge;
        }
        if met_after_edge.is_some() {
            met_final = met_after_edge;
        }
        add_counts(&mut counts, &edge_counts);
        for (c, part) in jobs.iter().zip(parts) {
            for (&p, rgb) in c.iter().zip(part) {
                let at = p as usize * 3;
                levels[at..at + 3].copy_from_slice(&rgb);
            }
        }
    }
    plan.generator = gstate;
    // The objects met are what the serial order would have left (a frame series goes on from them).
    if let Some(m) = met_final {
        surface.met().load(&m);
    }
    // ... and the metaballs' record (a frame series goes on from it).
    if let (Some(c), Some(ms)) = (meta_final, plan.geometry.scene.metaballs.as_ref()) {
        ms.load(&c);
    }
    trace(&format!("parallel: {n} threads"));
    Some(Ok((Picture { width: w, height: h, levels }, counts)))
}

/// Hand finished pixels to `emit`, a run for each stretch of neighbours along a row, then an
/// empty call: "flush now" (the part is complete; the binary's stream sends it on). Called only
/// from the thread that owns the output.
fn emit_runs(emit: &mut dyn FnMut(u32, u32, &[u8]), pixels: impl Iterator<Item = (u32, u32, [u8; 3])>) {
    let mut run: Vec<u8> = Vec::new();
    let (mut sx, mut sy, mut next_x) = (0u32, 0u32, 0u32);
    for (x, y, rgb) in pixels {
        if !run.is_empty() && !(y == sy && x == next_x) {
            emit(sx, sy, &run);
            run.clear();
        }
        if run.is_empty() {
            (sx, sy) = (x, y);
        }
        run.extend_from_slice(&rgb);
        next_x = x + 1;
    }
    if !run.is_empty() {
        emit(sx, sy, &run);
    }
    emit(0, 0, &[]);
}
