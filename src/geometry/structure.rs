// SPDX-License-Identifier: MIT
//! Which object a ray reports where objects meet it at the same distance: the walk of a
//! render through its tree (code `bspT`) or its grid (code `grid`), with each object's kept
//! distance and missed mark for the ray.
//!
//! , (the
//! grid),;
//! (a group
//! member's box, not widened, and its box test: `shape_box`, `box_met`; a combined group is
//! asked before the walk:, Combined groups).
//! (The structure: the beam lights' entries)

use std::f64::consts::PI;

use super::light::LightShape;
use super::metaball::MetaWalk;
use super::{LocalHit, Part, Range, Ray, Scene, Shape, ShapeContext, sphere_hit_at, sphere_roots};
use crate::geom::Vec3;
use crate::version::RenderAs;

/// The two walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Tree,
    Grid,
}

/// One item of the root list as the builder receives it, in list order.
/// (which objects go where)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Listed {
    /// A geometry object: index into `Scene::objects`.
    Object(usize),
    /// A primitive shape's object listed by the box of a place other than its own: `rows` and
    /// `translation` carry its points (the box of the place the document stored, where a held
    /// object was placed by a path without the box being made again). (A held
    /// object with no key, as 5.0). (Held objects)
    Stale { object: usize, rows: [Vec3; 3], translation: Vec3 },
    /// A radial light without its own object: listed by its sphere's box, never hit.
    InertSphere { rows: [Vec3; 3], translation: Vec3 },
    /// A spot or parallel light without its own object: listed, never hit, by the box of
    /// 's points of `shape` (the cone for the spot light, the cube for the parallel
    /// light). leaves these lights' boxes Open: our choice.
    /// (The structure)
    InertShape { shape: Shape, rows: [Vec3; 3], translation: Vec3 },
    /// A parallel light without its own object: listed, never hit, by the box of's
    /// points (the cube's eight corners and, as 3.0 and 3.1, (0, -1, 0)).
    InertParallel { rows: [Vec3; 3], translation: Vec3 },
    /// An object listed by the box of its points already carried to the world (`min`,
    /// `max`), widened as every box; never hit: a path kept out of renders, by the box of
    /// .. (Paths)
    InertBox { min: Vec3, max: Vec3 },
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

/// An entry of the structure: the object it answers for (`None`: the sun or a light, which
/// answers no hit) and its widened box.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub object: Option<usize>,
    pub bounds: Bounds,
}

/// The tree.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Leaf(Vec<u32>),
    Cut { axis: usize, plane: f64, first: Box<Node>, second: Box<Node> },
}

/// The grid.
#[derive(Clone, Debug, PartialEq)]
struct Grid {
    n: usize,
    bounds: Bounds,
    cells: Vec<Vec<u32>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Walker {
    Tree { root: Node, bounds: Bounds },
    Grid(Grid),
}

/// What a render's geometry scene carries to decide which object a ray reports.
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    version: RenderAs,
    /// Objects asked before the structure, in list order: planes and slabs , and
    /// the objects gives no box, meshes and terrains among them (our choice:
    unbounded: Vec<usize>,
    entries: Vec<Entry>,
    walker: Walker,
}

/// Tree leaf size N and depth limit D by option.
fn tree_limits(option: u32) -> Option<(usize, usize)> {
    match option {
        1 => Some((3, 8)),
        2 => Some((5, 10)),
        0 => None,           // o = 0: no split
        // : the tree is made with
        // N = 5 and D = 5 and the render overwrites the pair for 0, 1 and 2 only.
        _ => Some((5, 5)),
    }
}

/// Grid cells a side by option.
fn grid_side(option: u32) -> usize {
    match option {
        1 => 20,
        2 => 40,
        // : the window's grid of a first render has 10 cells a
        // side; a render stores the option's number for 0, 1 and 2 only.
        3.. => 10,
        0 => 1,
    }
}

const STRETCH_OVERLAP: f64 = 0.0001;
const CELL_WIDENING: f64 = 0.001;
const THIN_LEAF: f64 = 0.001;
const THIN_ROOT: f64 = 0.01;
const ROOT_MOVE: f64 = 0.25;
const WIDEN_PART: f64 = 0.01;
const FLAT_EXTENT: f64 = 1e-10;
const FLAT_WIDENING: f64 = 1e-7;

/// `p · M + t`, in the order `Placement::to_world` carries a point.
fn carry(p: Vec3, rows: &[Vec3; 3], t: Vec3) -> Vec3 {
    let c = |j: usize| p[0] * rows[0][j] + p[1] * rows[1][j] + p[2] * rows[2][j] + t[j];
    [c(0), c(1), c(2)]
}

/// The surface points boxes a shape by, in its own space; `None` for a shape it
/// lists no points for.
pub(crate) fn shape_points(shape: Shape, version: RenderAs) -> Option<Vec<Vec3>> {
    let step = |k: usize| k as f64 * PI / 12.0; // 15°·k, in binary64
    Some(match shape {
        Shape::Cube => {
            let mut v = Vec::new();
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    for z in [-0.5, 0.5] {
                        v.push([x, y, z]);
                    }
                }
            }
            v
        }
        Shape::Square => vec![[-1.0, 0.0, -1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0]],
        Shape::Pyramid => vec![
            [-0.5, -0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [0.0, 0.5, 0.0],
        ],
        Shape::Sphere | Shape::Metaball(_) => sphere_points(),
        Shape::Cylinder => {
            let mut v = Vec::new();
            for k in 0..24 {
                let a = step(k);
                for y in [-1.0, 1.0] {
                    v.push([a.sin(), y, a.cos()]);
                }
            }
            v
        }
        Shape::Cone => {
            let mut v: Vec<Vec3> = (0..24).map(|k| [step(k).sin(), -1.0, step(k).cos()]).collect();
            v.push([0.0, 1.0, 0.0]);
            v
        }
        Shape::Torus { tube, ring } => {
            let mut v = Vec::new();
            for i in 0..24 {
                let a = step(i);
                for j in 0..24 {
                    let b = step(j);
                    let w = ring + tube * b.sin();
                    v.push([w * a.cos(), tube * b.cos(), w * a.sin()]);
                }
            }
            v
        }
        // A terrain: gives it no box (Open); it is asked before the structure, as a
        // mesh is .
        // A combined group: leave its place in the walk Open; it is asked
        // before the structure, as a mesh is (Combined groups).
        // A light's own object:'s points; the endless spot light has none (
        // kept out of the leaves and cells, asked apart; how: Open, our choice as a plane's).
        Shape::Light(l) => {
            if l.endless() {
                return None;
            }
            return Some(l.box_points(version));
        }
        Shape::Plane
        | Shape::Slab
        | Shape::Disc
        | Shape::Mesh(_)
        // A document's mesh is listed by `mesh_corners` (`Structure::build`); one without
        // triangles or with a box that is no box stays unbounded.
        | Shape::StoredMesh(_)
        | Shape::Terrain(_)
        | Shape::Path(_)
        | Shape::Group(_)
        // A tree is listed by its own box (`Structure::build`), not by points.
        | Shape::Tree { .. }
        | Shape::TreeLeaves { .. }
        | Shape::MetaSet => {
            return None;
        }
    })
}

/// The eight corners of a mesh's stored box; `None` for a mesh without triangles or with a
/// stored box that is not finite (such a mesh is asked before the structure, our choice).
fn mesh_corners(mesh: Option<&super::StoredMesh>) -> Option<Vec<Vec3>> {
    let m = mesh?;
    if m.faces.is_empty() || !m.bounds.iter().all(|v| v.is_finite()) {
        return None;
    }
    let b = m.bounds;
    let mut v = Vec::with_capacity(8);
    for x in [b[0], b[3]] {
        for y in [b[1], b[4]] {
            for z in [b[2], b[5]] {
                v.push([x, y, z]);
            }
        }
    }
    Some(v)
}

/// A group member's box:'s points of `shape` carried to the world, not widened;
/// `None` for a shape with no points (the plane, the slab; the disc: Open, our choice).
pub(crate) fn shape_box(shape: Shape, version: RenderAs, rows: &[Vec3; 3], t: Vec3) -> Option<Bounds> {
    // The sphere's 600 points cost 1200 sines and cosines to make and a group asks for a member's
    // box on every ray, so they are made once (same points, same carrying, same min/max order).
    if matches!(shape, Shape::Sphere | Shape::Metaball(_)) {
        return Some(points_box(sphere_points_shared(), rows, t));
    }
    if matches!(shape, Shape::Cylinder | Shape::Cone | Shape::Cube) {
        // (version does not enter the points of these three)
        static LISTS: [std::sync::OnceLock<Vec<Vec3>>; 3] =
            [std::sync::OnceLock::new(), std::sync::OnceLock::new(), std::sync::OnceLock::new()];
        let slot = match shape { Shape::Cylinder => 0, Shape::Cone => 1, _ => 2 };
        let pts = LISTS[slot].get_or_init(|| shape_points(shape, version).expect("listed shape"));
        return Some(points_box(pts, rows, t));
    }
    let points = shape_points(shape, version)?;
    let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
    for p in points {
        let w = carry(p, rows, t);
        for a in 0..3 {
            b.min[a] = b.min[a].min(w[a]);
            b.max[a] = b.max[a].max(w[a]);
        }
    }
    Some(b)
}

/// The box of `points` carried by `rows` and `t`, not widened. (a
/// member's box)
pub(crate) fn points_box(points: &[Vec3], rows: &[Vec3; 3], t: Vec3) -> Bounds {
    let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
    for &p in points {
        let w = carry(p, rows, t);
        for a in 0..3 {
            b.min[a] = b.min[a].min(w[a]);
            b.max[a] = b.max[a].max(w[a]);
        }
    }
    b
}

/// A combined group's box as the walk lists it: its union widened by the group's own rule and
/// then as every box of.
pub(crate) fn group_box_widened(b: Bounds, version: RenderAs) -> Bounds {
    widen(widen_group(b, version), version)
}

/// The box a light's own object is listed by, before the widening of: the carried
/// points of.
pub fn light_box(shape: Shape, version: RenderAs, rows: &[Vec3; 3], t: Vec3) -> Option<Bounds> {
    shape_box(shape, version, rows, t)
}

/// The box a parallel light is listed by, before the widening of: the carried points
/// of.
pub fn parallel_light_box(version: RenderAs, rows: &[Vec3; 3], t: Vec3) -> Bounds {
    let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
    for p in super::light::parallel_points(version) {
        let w = carry(p, rows, t);
        for a in 0..3 {
            b.min[a] = b.min[a].min(w[a]);
            b.max[a] = b.max[a].max(w[a]);
        }
    }
    b
}

/// Whether the ray meets box `b` within lo and hi:'s box test with loL = lo and
/// F = hi.,
pub(crate) fn box_met(b: &Bounds, ray: &Ray, lo: f64, hi: f64) -> bool {
    meets(b, ray, lo, hi).is_some()
}

/// The sphere's 25 × 24 points (sin φ sin θ, cos θ, cos φ sin θ), θ = a·π/24, φ = b·2π/24.
/// : the angles are made by adding, θ_0 = φ_0 = 0,
/// θ_(a+1) = θ_a + π/24 and φ_(b+1) = φ_b + 2π/24, each sum rounded to binary64, each step the
/// binary64 π (or that π doubled) divided by 24 and rounded; not the index times the step.
/// This gives all 76 metaball boxes of metaballs-scenes.jsonl.
pub(crate) fn sphere_points() -> Vec<Vec3> {
    sphere_points_shared().to_vec()
}

/// The sphere's points, made once for the process.
pub(crate) fn sphere_points_shared() -> &'static [Vec3] {
    static POINTS: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
    POINTS.get_or_init(sphere_points_made)
}

fn sphere_points_made() -> Vec<Vec3> {
    let mut v = Vec::with_capacity(600);
    let (th_step, ph_step) = (PI / 24.0, (2.0 * PI) / 24.0);
    let mut th = 0.0f64;
    for _ in 0..=24 {
        let mut ph = 0.0f64;
        for _ in 0..24 {
            v.push([ph.sin() * th.sin(), th.cos(), ph.cos() * th.sin()]);
            ph += ph_step;
        }
        th += th_step;
    }
    v
}

/// The box of the points carried to the world, widened.
fn widened_box(points: &[Vec3], rows: &[Vec3; 3], t: Vec3, version: RenderAs) -> Bounds {
    let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
    for &p in points {
        let w = carry(p, rows, t);
        for a in 0..3 {
            b.min[a] = b.min[a].min(w[a]);
            b.max[a] = b.max[a].max(w[a]);
        }
    }
    widen(b, version)
}

/// (min − d, max + d, d = (max − min)·0.01; 1e-7 where the extent is within
/// 1e-10 of 0, as 5.0; 3.0 and 3.1 widen by d alone)
fn widen(mut b: Bounds, version: RenderAs) -> Bounds {
    for a in 0..3 {
        let extent = b.max[a] - b.min[a];
        let d = if version == RenderAs::Bryce50 && extent.abs() < FLAT_EXTENT { FLAT_WIDENING } else { extent * WIDEN_PART };
        b.min[a] -= d;
        b.max[a] += d;
    }
    b
}

/// A group's own widening of the union of its members' boxes: 1 % of the extent at each end of
/// each axis; as 5.0 0.1 %, or 1e-7 where the extent is within 1e-10 of 0. The widening of
/// every box of follows (`widen`). (How a combined group is asked in the
/// walk of)
fn widen_group(mut b: Bounds, version: RenderAs) -> Bounds {
    for a in 0..3 {
        let extent = b.max[a] - b.min[a];
        let d = if version != RenderAs::Bryce50 {
            extent * 0.01
        } else if extent.abs() < 1e-10 {
            1e-7
        } else {
            extent * 0.001
        };
        b.min[a] -= d;
        b.max[a] += d;
    }
    b
}

fn union(boxes: impl Iterator<Item = Bounds>) -> Option<Bounds> {
    boxes.reduce(|u, b| {
        let mut r = u;
        for a in 0..3 {
            r.min[a] = r.min[a].min(b.min[a]);
            r.max[a] = r.max[a].max(b.max[a]);
        }
        r
    })
}

/// The ray's stretch [near, far] in a box (the slab method), tested against the box test's
/// four conditions as it grows: far > 0, near ≤ far, near ≤ f and far ≥ lo. Over the axes near
/// only rises and far only falls (f64 max and min never make them NaN), so a condition that
/// already fails part way through fails at the end too, and the test leaves early with `None`,
/// as the full test would.
/// The origin on the plane of an axis with direction 0 counts as between (our choice: leaves it
/// Open).
fn stretch_met(b: &Bounds, ray: &Ray, lo: f64, f: f64) -> Option<(f64, f64)> {
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for a in 0..3 {
        let (o, d) = (ray.origin[a], ray.dir[a]);
        // : an axis with direction 0 gives ±(distance · 16776704.001)
        // in place of ±(distance / direction)
        let (t1, t2) = if d == 0.0 {
            ((b.min[a] - o) * ZERO_DIRECTION_FACTOR, (b.max[a] - o) * ZERO_DIRECTION_FACTOR)
        } else {
            ((b.min[a] - o) / d, (b.max[a] - o) / d)
        };
        let (enter, leave) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        near = near.max(enter);
        far = far.min(leave);
        if !(far > 0.0 && near <= far && near <= f && far >= lo) {
            return None;
        }
    }
    Some((near, far))
}

/// The multiplier that stands for dividing by a direction of 0 in the box test.
const ZERO_DIRECTION_FACTOR: f64 = 16776704.001;
/// The nearly-zero direction and the widening of the root's entry point.
const ROOT_DIRECTION_LIMIT: f64 = 1e-7;
/// The stand-in for an unbounded distance in the root's entry.
const ROOT_UNBOUNDED: f64 = 1.7e308;

/// The ray's entry and exit distances at the root's box, where the walk starts.
/// (the walk)
fn root_entry(b: &Bounds, ray: &Ray) -> Option<(f64, f64)> {
    let (mut e, mut e_axis, mut x) = (f64::NEG_INFINITY, 0usize, f64::INFINITY);
    for a in 0..3 {
        let (o, d, lo, hi) = (ray.origin[a], ray.dir[a], b.min[a], b.max[a]);
        let (enter, leave) = if o <= lo {
            if !(d >= ROOT_DIRECTION_LIMIT) {
                return None;
            }
            ((lo - o) / d, (hi - o) / d)
        } else if o >= hi {
            if !(d <= -ROOT_DIRECTION_LIMIT) {
                return None;
            }
            ((hi - o) / d, (lo - o) / d)
        } else if d > ROOT_DIRECTION_LIMIT {
            ((lo - o) / d, (hi - o) / d)
        } else if d < -ROOT_DIRECTION_LIMIT {
            ((hi - o) / d, (lo - o) / d)
        } else {
            (-ROOT_UNBOUNDED, ROOT_UNBOUNDED)
        };
        if enter > e {
            (e, e_axis) = (enter, a);
        }
        if leave < x {
            x = leave;
        }
    }
    // the point at e lies within the box widened by 1e-7 on the other two axes
    for a in (0..3).filter(|&a| a != e_axis) {
        let p = ray.origin[a] + e * ray.dir[a];
        if !(p >= b.min[a] - ROOT_DIRECTION_LIMIT && p <= b.max[a] + ROOT_DIRECTION_LIMIT) {
            return None;
        }
    }
    Some((e, x))
}

/// The box test of an entry or a cell: the stretch, where far > 0, near ≤ far, near ≤ F and
/// far ≥ loL.
pub(crate) fn meets(b: &Bounds, ray: &Ray, lo: f64, f: f64) -> Option<(f64, f64)> {
    stretch_met(b, ray, lo, f)
}

fn build_tree(entries: &[Entry], order: Vec<u32>, bounds: Bounds, depth: usize, limits: Option<(usize, usize)>) -> Node {
    let axis = depth % 3;
    let Some((n, d)) = limits else { return Node::Leaf(order) };
    // : a leaf if its extent on a is not above 0.001, or it lists N entries
    // or fewer, or q ≥ D.
    if !(bounds.max[axis] - bounds.min[axis] > THIN_LEAF) || order.len() <= n || depth >= d {
        return Node::Leaf(order);
    }
    let m = bounds.min[axis] + 0.5 * (bounds.max[axis] - bounds.min[axis]);
    let mut first = Vec::new();
    let mut second = Vec::new();
    for &e in &order {
        let b = &entries[e as usize].bounds;
        if b.max[axis] < m {
            first.push(e);
        } else if b.min[axis] > m {
            second.push(e);
        } else {
            first.push(e);
            second.push(e);
        }
    }
    // : each child lists its entries in the reverse of the node's order.
    first.reverse();
    second.reverse();
    let mut fb = bounds;
    fb.max[axis] = m;
    let mut sb = bounds;
    sb.min[axis] = m;
    Node::Cut {
        axis,
        plane: m,
        first: Box::new(build_tree(entries, first, fb, depth + 1, limits)),
        second: Box::new(build_tree(entries, second, sb, depth + 1, limits)),
    }
}

/// The cell number of coordinate `c` on an axis. (k = ⌊(c − min)/(max − min)
/// · n⌋, cut towards zero, held to 0 … n − 1)
fn cell_of(c: f64, min: f64, max: f64, n: usize) -> usize {
    let k = ((c - min) / (max - min) * n as f64).trunc();
    if k >= (n - 1) as f64 {
        n - 1
    } else if k > 0.0 {
        k as usize
    } else {
        0
    }
}

impl Structure {
    /// The entries in list order, the sun last (for tests of the boxes).
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Builds the structure over the root list (`listed`, in list order) of the geometry
    /// scene `scene`.
    pub fn build(
        scene: &Scene,
        listed: &[Listed],
        sun: ([Vec3; 3], Vec3),
        version: RenderAs,
        layout: Layout,
        option: u32,
    ) -> Structure {
        let mut unbounded = Vec::new();
        let mut entries = Vec::new();
        for item in listed {
            match *item {
                Listed::Object(i) => {
                    let o = &scene.objects[i];
                    // (a tree's box is the eight corners of the tree's
                    // bounds,, carried by the object's matrix, widened as every box); the
                    // box the tree tests a ray by (step 2) stays unwidened.
                    if let Shape::Tree { model, .. } = o.shape {
                        entries.push(Entry { object: Some(i), bounds: widen(scene.trees[model].world_box(&o.placement), version) });
                        continue;
                    }
                    // : a mesh is listed in its place in the list by
                    // the eight corners of its own box (the stored box), carried by the matrix,
                    // widened like every other box.
                    if let Shape::StoredMesh(m) = o.shape
                        && let Some(points) = mesh_corners(scene.stored_meshes.get(m))
                    {
                        entries.push(Entry {
                            object: Some(i),
                            bounds: widened_box(&points, &o.placement.rows(), o.placement.translation(), version),
                        });
                        continue;
                    }
                    // : a terrain is listed in its place by its box by
                    // version , a disc by the 24 points (sin a, 0, cos a), a = 15 degrees
                    // k, carried by the object's matrix, each widened as every box.
                    if let Shape::Terrain(k) = o.shape
                        && let Some(t) = scene.terrains.get(k)
                    {
                        let (min, max) = t.listing_world_box(&o.placement);
                        if min.iter().chain(max.iter()).all(|v| v.is_finite()) {
                            entries.push(Entry { object: Some(i), bounds: widen(Bounds { min, max }, version) });
                            continue;
                        }
                    }
                    if o.shape == Shape::Disc {
                        let points: Vec<Vec3> =
                            (0..24).map(|k| [(k as f64 * PI / 12.0).sin(), 0.0, (k as f64 * PI / 12.0).cos()]).collect();
                        entries.push(Entry {
                            object: Some(i),
                            bounds: widened_box(&points, &o.placement.rows(), o.placement.translation(), version),
                        });
                        continue;
                    }
                    // : a group that
                    // combines is listed in its place by the union of its members' boxes, widened
                    // twice; one with a member that has no box is asked first, as before.
                    if let Shape::Group(k) = o.shape
                        && let Some(b) = scene.group_walk_box(k, version)
                    {
                        entries.push(Entry { object: Some(i), bounds: group_box_widened(b, version) });
                        continue;
                    }
                    // : a drawn path is listed in its place by the box of its
                    // points carried by the object's matrix, widened as every box, nothing added
                    // for the band.
                    if let Shape::Path(k) = o.shape {
                        entries.push(Entry {
                            object: Some(i),
                            bounds: widened_box(&scene.paths[k].box_points, &o.placement.rows(), o.placement.translation(), version),
                        });
                        continue;
                    }
                    match shape_points(o.shape, version) {
                        Some(points) => entries.push(Entry {
                            object: Some(i),
                            bounds: widened_box(&points, &o.placement.rows(), o.placement.translation(), version),
                        }),
                        None => unbounded.push(i),
                    }
                }
                Listed::Stale { object: i, rows, translation } => match shape_points(scene.objects[i].shape, version) {
                    Some(points) => {
                        entries.push(Entry { object: Some(i), bounds: widened_box(&points, &rows, translation, version) })
                    }
                    None => unbounded.push(i),
                },
                Listed::InertSphere { rows, translation } => entries
                    .push(Entry { object: None, bounds: widened_box(&sphere_points(), &rows, translation, version) }),
                Listed::InertShape { shape, rows, translation } => {
                    let points = shape_points(shape, version).unwrap_or_else(sphere_points);
                    entries.push(Entry { object: None, bounds: widened_box(&points, &rows, translation, version) });
                }
                Listed::InertParallel { rows, translation } => entries.push(Entry {
                    object: None,
                    bounds: widened_box(&super::light::parallel_points(version), &rows, translation, version),
                }),
                Listed::InertBox { min, max } => entries.push(Entry { object: None, bounds: widen(Bounds { min, max }, version) }),
            }
        }
        // : the sun's entry last, the box of the eight corners
        // of the cube (-1, -1, -1) to (1, 1, 1) each carried through the sun's placement
        // (p . M + t), widened; the placement is the identity in every document observed.
        let mut corners = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    corners.push([x, y, z]);
                }
            }
        }
        entries.push(Entry { object: None, bounds: widened_box(&corners, &sun.0, sun.1, version) });
        let order: Vec<u32> = (0..entries.len() as u32).collect();
        let union = union(entries.iter().map(|e| e.bounds)).unwrap_or(Bounds { min: [0.0; 3], max: [0.0; 3] });
        let layout = if version == RenderAs::Bryce50 { layout } else { Layout::Tree };
        let walker = match layout {
            Layout::Tree => {
                let mut root = union;
                for a in 0..3 {
                    let d = root.max[a] - root.min[a];
                    if -THIN_ROOT < d && d < THIN_ROOT {
                        root.min[a] -= ROOT_MOVE;
                        root.max[a] += ROOT_MOVE;
                    }
                }
                Walker::Tree { root: build_tree(&entries, order, root, 0, tree_limits(option)), bounds: root }
            }
            Layout::Grid => {
                let n = grid_side(option);
                let mut cells = vec![Vec::new(); n * n * n];
                for (k, e) in entries.iter().enumerate() {
                    let lo: Vec<usize> = (0..3).map(|a| cell_of(e.bounds.min[a], union.min[a], union.max[a], n)).collect();
                    let hi: Vec<usize> = (0..3).map(|a| cell_of(e.bounds.max[a], union.min[a], union.max[a], n)).collect();
                    for x in lo[0]..=hi[0] {
                        for y in lo[1]..=hi[1] {
                            for z in lo[2]..=hi[2] {
                                cells[(x * n + y) * n + z].push(k as u32);
                            }
                        }
                    }
                }
                Walker::Grid(Grid { n, bounds: union, cells })
            }
        };
        Structure { version, unbounded, entries, walker }
    }

    /// The walk of one ray: the reported object and its local hit.
    pub(crate) fn walk(&self, scene: &Scene, ray: &Ray, range: Range, ctx: &ShapeContext) -> Option<(usize, LocalHit)> {
        let mut w = Walk {
            s: self,
            scene,
            ray,
            ctx,
            lo: range.near,
            b: range.far,
            f: range.far,
            mails: take_mails(self.entries.len()),
            walk_id: next_walk_id(),
            marks: vec![super::tree::RayMarks::new(); scene.trees.len()],
            shared: Default::default(),
            crossing: Default::default(),
            best: None,
            hit_here: false,
            meta: None,
        };
        // : the planes and slabs (and the unboxed objects, our choice), in list
        // order, each by rule P with lo and B; a hit makes B and F equal to t.
        for &i in &self.unbounded {
            if let Some(h) = scene.ask(i, ray, Range { near: w.lo, far: w.b }, ctx) {
                // : a later object at exactly the far end does not replace
                // the earlier (a mesh's closed range, a crossing at +infinity).
                if w.best.is_some() && h.t == w.b {
                    continue;
                }
                w.b = h.t;
                w.f = h.t;
                w.best = Some((i, h));
            }
        }
        let unbounded_hit = w.best.is_some();
        match &self.walker {
            Walker::Tree { root, bounds } => {
                // : a ray that fails the root's entry meets nothing in
                // the structure.
                if let Some((near, far)) = root_entry(bounds, ray) {
                    w.node(root, near - STRETCH_OVERLAP, far + STRETCH_OVERLAP);
                }
            }
            Walker::Grid(g) => w.grid(g, unbounded_hit),
        }
        w.best
    }
}

/// An entry's mailbox for one ray.
#[derive(Clone, Copy, Debug, Default)]
struct Mail {
    kept: Option<LocalHit>,
    missed: bool,
    /// The walk this mailbox content belongs to; any other number means it is empty.
    stamp: u64,
}

thread_local! {
    /// Mailbox buffers of this thread, reused from walk to walk (nested walks take their own).
    static MAIL_POOL: std::cell::RefCell<Vec<Vec<Mail>>> = const { std::cell::RefCell::new(Vec::new()) };
    static WALK_COUNTER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A fresh walk number for this thread (never 0, which marks an untouched mailbox; a u64 does not wrap).
fn next_walk_id() -> u64 {
    WALK_COUNTER.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
}

fn take_mails(n: usize) -> Vec<Mail> {
    let mut v = MAIL_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
    if v.len() < n {
        v.resize(n, Mail::default());
    }
    v
}

impl Drop for Walk<'_> {
    fn drop(&mut self) {
        let v = std::mem::take(&mut self.mails);
        MAIL_POOL.with(|p| p.borrow_mut().push(v));
    }
}

/// One ray's walk.
struct Walk<'a> {
    s: &'a Structure,
    scene: &'a Scene,
    ray: &'a Ray,
    ctx: &'a ShapeContext,
    lo: f64,
    b: f64,
    f: f64,
    mails: Vec<Mail>,
    walk_id: u64,
    /// What each tree of the scene holds for this ray.
    marks: Vec<super::tree::RayMarks>,
    /// The marks of the items of each stored tree and the pieces' one kept crossing, shared by
    /// the trees as says.
    shared: std::collections::HashMap<usize, super::tree::ItemMarks>,
    crossing: super::tree::PieceCrossing,
    best: Option<(usize, LocalHit)>,
    /// Whether the leaf or cell being asked answered a hit.
    hit_here: bool,
    /// The metaballs' marks and kept crossing for this ray.
    meta: Option<MetaWalk>,
}

/// What an object answers.
enum Answer {
    Hit(LocalHit),
    None,
}

impl Walk<'_> {
    /// Entry k's mailbox for this walk: empty unless stamped with this walk's number.
    #[inline]
    fn mail(&mut self, k: usize) -> &mut Mail {
        let m = &mut self.mails[k];
        if m.stamp != self.walk_id {
            *m = Mail { kept: None, missed: false, stamp: self.walk_id };
        }
        m
    }

    fn five(&self) -> bool {
        self.s.version == RenderAs::Bryce50
    }

    fn node(&mut self, node: &Node, e: f64, x: f64) {
        match node {
            Node::Leaf(list) => self.leaf(list, e, x, false),
            Node::Cut { axis, plane, first, second } => {
                let (o, d) = (self.ray.origin[*axis], self.ray.dir[*axis]);
                let one = |w: &mut Self, child: &Node| {
                    if !(e > w.b || x < w.lo) {
                        w.node(child, e, x);
                    }
                };
                // : the near child is the second if
                // m < o_a (or either is not a number), else the first; t = (m − o_a)/d_a.
                let (near, far) = if *plane < o || plane.is_nan() || o.is_nan() { (second, first) } else { (first, second) };
                let t = (*plane - o) / d;
                if t == 0.0 || t.is_nan() {
                    // the far child if d_a > 0, else the near child, with the same stretch
                    self.node(if d > 0.0 { far } else { near }, e, x);
                } else if t > x || t < 0.0 {
                    one(self, near);
                } else if t < e {
                    one(self, far);
                } else {
                    if !(e > self.b || t < self.lo) {
                        self.node(near, e, t + STRETCH_OVERLAP);
                    }
                    if self.b > t {
                        self.node(far, t - STRETCH_OVERLAP, x);
                    }
                }
            }
        }
    }

    /// A leaf or a cell over [e, x]: its entries in list order.,
    fn leaf(&mut self, list: &[u32], e: f64, x: f64, cell: bool) {
        let lo_l = self.lo.max(e);
        for &k in list {
            let k = k as usize;
            if self.mail(k).missed {
                continue;
            }
            match self.mail(k).kept {
                Some(kept) if cell => {
                    if kept.t < lo_l || kept.t > self.f {
                        self.mail(k).missed = true;
                        continue;
                    }
                }
                _ => {
                    if meets(&self.s.entries[k].bounds, self.ray, lo_l, self.f).is_none() {
                        // Variants: as 3.0 and 3.1 no object is marked missed.
                        if self.five() {
                            self.mail(k).missed = true;
                        }
                        continue;
                    }
                }
            }
            let bl = self.b.min(x);
            if let Answer::Hit(h) = self.ask(k, lo_l, bl) {
                self.b = h.t;
                self.f = h.t;
                self.best = Some((self.s.entries[k].object.expect("an inert entry answers no hit"), h));
                self.hit_here = true;
            }
        }
    }

    fn ask(&mut self, k: usize, lo_l: f64, bl: f64) -> Answer {
        let Some(i) = self.s.entries[k].object else { return Answer::None };
        let five = self.five();
        let scene = self.scene;
        let obj = &scene.objects[i];
        // : a tree keeps its own marks and kept hit for the ray.
        if let Shape::Tree { model, .. } = obj.shape {
            // : trees that show one stored tree share the items' marks and
            // the leaves' kept hits; the pieces' one kept crossing is the scene's.
            let key = scene.trees[model].stored().unwrap_or(usize::MAX - model);
            let mut items = self.shared.remove(&key).unwrap_or_default();
            self.marks[model].exchange(&mut items, &mut self.crossing);
            let (hit, far) = scene.ask_tree(i, model, &mut self.marks[model], self.ray, lo_l, bl, self.f);
            self.marks[model].exchange(&mut items, &mut self.crossing);
            self.shared.insert(key, items);
            self.f = far;
            return hit.map_or(Answer::None, Answer::Hit);
        }
        // : a shadow ray does not meet a light's own object, nor does any ray where
        // its word lacks 0x200 or holds a hidden bit.
        if matches!(obj.shape, Shape::Light(l) if !l.met(self.ctx.shadow)) {
            return Answer::None;
        }
        // : a metaball is asked through the metaballs' own walk (5.0 only).
        if let Shape::Metaball(m0) = obj.shape {
            return if five { self.meta_ask(k, m0, lo_l, bl) } else { Answer::None };
        }
        // rule S for the sphere, and for a radial light's object (a tie
        // with it may report either object; our choice).
        // : as 5.0 rule S; as 3.0 and 3.1
        // rule P with the leaf bound, below (revised).
        if obj.shape == Shape::Sphere || (five && matches!(obj.shape, Shape::Light(l) if l.shape == LightShape::Sphere)) {
            return self.rule_s(k, i, lo_l, bl);
        }
        // rule M (a document's mesh, in every version): as rule K, with the range
        // closed at both ends and a missed mark in every version.
        if matches!(obj.shape, Shape::StoredMesh(_)) {
            return self.rule_m(k, i, lo_l, bl);
        }
        // : in every version a
        // kept distance, else the first crossing that shows with loL < t < F; beyond BL it is
        // kept and F becomes it (the answer is no hit); none gives the missed mark.
        if matches!(obj.shape, Shape::Group(_)) {
            return self.rule_m(k, i, lo_l, bl);
        }
        // : a path's answer is taken as a hit like any other, whatever its t: the
        // range is not asked of it, no distance is kept and no missed mark is made.
        if matches!(obj.shape, Shape::Path(_)) {
            return match self.scene.ask(i, self.ray, Range { near: lo_l, far: bl }, self.ctx) {
                Some(h) => Answer::Hit(h),
                None => Answer::None,
            };
        }
        if !five {
            // Variants: rule P with BL.
            return match self.scene.ask(i, self.ray, Range { near: lo_l, far: bl }, self.ctx) {
                Some(h) => Answer::Hit(h),
                None => Answer::None,
            };
        }
        // Rule K.
        if let Some(kept) = self.mail(k).kept {
            return if lo_l < kept.t && kept.t < bl { Answer::Hit(kept) } else { Answer::None };
        }
        match self.scene.ask(i, self.ray, Range { near: lo_l, far: self.f }, self.ctx) {
            None => {
                self.mail(k).missed = true;
                Answer::None
            }
            Some(h) if bl < h.t => {
                self.mail(k).kept = Some(h);
                self.f = h.t;
                Answer::None
            }
            Some(h) => Answer::Hit(h),
        }
    }

    /// rule M. With a kept distance t_k: the hit when loL < t_k < BL (both strict),
    /// else no hit and nothing changes. Otherwise the nearest crossing in loL <= t <= F (closed;
    /// F the far end): none gives the missed mark; one beyond BL is kept and F becomes it; one
    /// at or before BL is the hit.
    fn rule_m(&mut self, k: usize, i: usize, lo_l: f64, bl: f64) -> Answer {
        if let Some(kept) = self.mail(k).kept {
            return if lo_l < kept.t && kept.t < bl { Answer::Hit(kept) } else { Answer::None };
        }
        match self.scene.ask(i, self.ray, Range { near: lo_l, far: self.f }, self.ctx) {
            None => {
                self.mail(k).missed = true;
                Answer::None
            }
            Some(h) if bl < h.t => {
                self.mail(k).kept = Some(h);
                self.f = h.t;
                Answer::None
            }
            Some(h) => Answer::Hit(h),
        }
    }

    /// A metaball asked about the stretch (lo_l, bl). The marks of the set's members
    /// live in the metaballs' walk; the missed ones are copied to the entries' mailboxes, which
    /// 's leaf reads.
    fn meta_ask(&mut self, k: usize, m0: usize, lo_l: f64, bl: f64) -> Answer {
        let _ = k;
        let Some(ms) = self.scene.metaballs.as_ref() else { return Answer::None };
        let mut walk = self.meta.take().unwrap_or_else(|| MetaWalk::new(ms.set.balls.len()));
        let kept_before = walk.kept.as_ref().map(|c| c.t.to_bits());
        let got = walk.ask(&ms.set, m0, self.ray, lo_l, bl, &mut self.f);
        for &m in &walk.newly_missed {
            let object = Some(ms.object_of[m]);
            if let Some(e) = self.s.entries.iter().position(|e| e.object == object) {
                self.mail(e).missed = true;
            }
        }
        if let Some(c) = walk.kept.as_ref().filter(|c| Some(c.t.to_bits()) != kept_before) {
            ms.record_kept(c);
        }
        self.meta = Some(walk);
        match got {
            Some(c) => {
                let id = ms.record_hit(&c);
                let mut h = LocalHit::new(c.t, c.point, c.normal, Part::Curved);
                h.meta = Some(id);
                Answer::Hit(h)
            }
            None => Answer::None,
        }
    }

    /// rule S (the sphere; as 3.0 and 3.1 without missed marks)
    fn rule_s(&mut self, k: usize, i: usize, lo_l: f64, bl: f64) -> Answer {
        let five = self.five();
        if let Some(kept) = self.mail(k).kept {
            return if lo_l <= kept.t && kept.t <= bl { Answer::Hit(kept) } else { Answer::None };
        }
        let local = self.scene.objects[i].placement.to_local_as(self.ctx.version, self.ray);
        let v = self.ctx.version;
        let mark = |w: &mut Self| {
            if five {
                w.mail(k).missed = true;
            }
        };
        let Some([t1, t2]) = sphere_roots(&local, v) else {
            mark(self);
            return Answer::None;
        };
        let t = if t1 > lo_l || t1 >= bl {
            t1
        } else if t2 > lo_l {
            t2
        } else {
            mark(self);
            return Answer::None;
        };
        if t < bl {
            return Answer::Hit(sphere_hit_at(&local, t, v));
        }
        if t > self.f {
            mark(self);
        } else {
            self.mail(k).kept = Some(sphere_hit_at(&local, t, v));
            self.f = t;
        }
        Answer::None
    }

    fn grid(&mut self, g: &Grid, unbounded_hit: bool) {
        let Some((near, _)) = meets(&g.bounds, self.ray, self.lo, self.f) else { return };
        let (o, d) = (self.ray.origin, self.ray.dir);
        let n = g.n;
        let mut cell = [0usize; 3];
        let w: Vec<f64> = (0..3).map(|a| (g.bounds.max[a] - g.bounds.min[a]) / n as f64).collect();
        for a in 0..3 {
            let s = if near <= 0.0 { o[a] } else { o[a] + near * d[a] };
            let s = s.max(g.bounds.min[a]).min(g.bounds.max[a]);
            cell[a] = cell_of(s, g.bounds.min[a], g.bounds.max[a], n);
        }
        let line = |a: usize, k: usize| g.bounds.min[a] + k as f64 * w[a];
        loop {
            let mut b = Bounds { min: [0.0; 3], max: [0.0; 3] };
            for a in 0..3 {
                b.min[a] = line(a, cell[a]) - CELL_WIDENING;
                b.max[a] = line(a, cell[a] + 1) + CELL_WIDENING;
            }
            if let Some((e, x)) = meets(&b, self.ray, self.lo, self.f) {
                self.hit_here = false;
                let list = &g.cells[(cell[0] * n + cell[1]) * n + cell[2]];
                self.leaf(list, e, x, true);
                if self.hit_here {
                    return;
                }
                if unbounded_hit {
                    if e < self.f && self.f < x {
                        return;
                    }
                } else if self.b < x {
                    return;
                }
            }
            // The next cell: the order of the cells ("The order of the cells").
            match next_cell(&cell, n, o, d, &line) {
                Some(c) => cell = c,
                None => return,
            }
        }
    }
}

/// The cell the walk goes to after `cell`; `None` where the cell number leaves 0 ..= n - 1.
/// One axis leads (the largest component in magnitude, the lowest axis on equal magnitudes).
/// For each other axis the walk asks how far the ray, where it leaves the current cell along
/// the leading axis, lies beyond the far side of the cell on that axis; above 1e-10 on either,
/// the axis whose edge the ray reaches at the smaller distance steps (equal: the higher axis;
/// only one over by more than 1e-10: that axis, unless the other has the smaller distance);
/// otherwise the leading axis steps. An axis whose component over the leading one is below
/// 1e-10 in magnitude never steps.
pub(super) fn next_cell(
    cell: &[usize; 3],
    n: usize,
    o: Vec3,
    d: Vec3,
    line: &dyn Fn(usize, usize) -> f64,
) -> Option<[usize; 3]> {
    const BAND: f64 = 1e-10;
    let mut lead = 0;
    for a in 1..3 {
        if d[a].abs() > d[lead].abs() {
            lead = a;
        }
    }
    if d[lead] == 0.0 {
        return None;
    }
    let far = |a: usize| if d[a] >= 0.0 { line(a, cell[a] + 1) } else { line(a, cell[a]) };
    let t_exit = (far(lead) - o[lead]) / d[lead];
    // (axis, distance to its edge, over by more than the band)
    let mut cand: Vec<(usize, f64, bool)> = Vec::new();
    for a in 0..3 {
        if a == lead || (d[a] / d[lead]).abs() < BAND {
            continue;
        }
        let edge = far(a);
        let at_exit = o[a] + t_exit * d[a];
        let gap = if d[a] >= 0.0 { at_exit - edge } else { edge - at_exit };
        cand.push((a, (edge - o[a]) / d[a], gap > BAND));
    }
    let mut axis = lead;
    if cand.iter().any(|c| c.2) {
        let mut best: Option<(usize, f64, bool)> = None;
        for &c in &cand {
            best = match best {
                None => Some(c),
                // upwards: a later (higher) axis wins an equal distance, except that an over
                // axis beats a not-over one at equal distance
                Some(b) if c.1 < b.1 || (c.1 == b.1 && (c.2 || !b.2)) => Some(c),
                keep => keep,
            };
        }
        if let Some(b) = best {
            axis = b.0;
        }
    }
    let mut next = *cell;
    if d[axis] >= 0.0 {
        next[axis] += 1;
        if next[axis] >= n {
            return None;
        }
    } else {
        if next[axis] == 0 {
            return None;
        }
        next[axis] -= 1;
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_numbers_are_held_inside() {
        assert_eq!(cell_of(-1.0, 0.0, 1.0, 20), 0);
        assert_eq!(cell_of(1.0, 0.0, 1.0, 20), 19);
        assert_eq!(cell_of(0.5, 0.0, 1.0, 20), 10);
    }

    // : cells of side 1, lines at the integers
    fn step_from(cell: [usize; 3], o: Vec3, d: Vec3) -> Option<[usize; 3]> {
        next_cell(&cell, 20, o, d, &|_, k| k as f64)
    }

    #[test]
    fn a_ray_through_an_edge_steps_the_leading_axis_first() {
        // leads x (equal x and y magnitudes: the lowest axis); it leaves cell (2, 2, 0) at the
        // corner (3, 3): the leading axis steps, the other axis in the next step
        assert_eq!(step_from([2, 2, 0], [2.5, 2.5, 0.5], [1.0, 1.0, 0.0]), Some([3, 2, 0]));
    }

    #[test]
    fn a_ray_beyond_the_edge_steps_that_axis_first() {
        // leaves cell (2, 2, 0) along x at x = 3, where y = 3.5 lies beyond the edge y = 3
        assert_eq!(step_from([2, 2, 0], [2.0, 3.0, 0.5], [1.0, 1.0, 0.0]), Some([2, 3, 0]));
        // within 1e-10 beyond the edge: not over
        assert_eq!(step_from([2, 2, 0], [2.0, 2.0 + 1e-12, 0.5], [1.0, 1.0, 0.0]), Some([3, 2, 0]));
    }

    #[test]
    fn a_nearly_parallel_axis_never_steps_and_the_walk_ends_at_the_grid() {
        assert_eq!(step_from([2, 2, 0], [2.5, 2.5, 0.5], [1.0, 1e-11, 0.0]), Some([3, 2, 0]));
        assert_eq!(step_from([19, 2, 0], [19.5, 2.5, 0.5], [1.0, 0.1, 0.0]), None);
        assert_eq!(step_from([0, 2, 0], [0.5, 2.5, 0.5], [-1.0, 0.1, 0.0]), None);
    }

    #[test]
    fn widening_of_a_flat_box() {
        let b = Bounds { min: [0.0, 1.0, 0.0], max: [2.0, 1.0, 2.0] };
        let w5 = widen(b, RenderAs::Bryce50);
        assert_eq!((w5.min[1], w5.max[1], w5.min[0]), (1.0 - 1e-7, 1.0 + 1e-7, -0.02));
        let w3 = widen(b, RenderAs::Bryce31);
        assert_eq!((w3.min[1], w3.max[1]), (1.0, 1.0));
    }
}
