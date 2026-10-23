// SPDX-License-Identifier: MIT
//! The render modes that show no shading: mask, distance and altitude. A mode is an option of
//! the render (no document holds one); its render is the ordinary one with a pair of rules in
//! place of the shading and the sky: a camera ray's first hit gives a grey, a miss the mode's
//! value, and no other ray is traced.
//!
//! , (the greys, the scene's boxes,
//! which objects count and the points of their boxes, Variants), (the render mode: no
//! document holds one), (first flag word bit 0x1: selected; second flag word bits 0x1
//! and 0x400000),,
//! (the sines and cosines, extended values),,
//! (Render modes: mask, distance and altitude);

use std::fmt;

use crate::geom::Vec3;
use crate::geometry::{Hit, Placement, Ray, Relief};
use crate::numerics::x87::{cos_ext, sin_ext};
use crate::sampling::batch::CameraRay;
use crate::scene::{Body, Light, LightKind, Scene, Shape};
use crate::scene_records::Transform;
use crate::shading::OpenItem;
use crate::version::RenderAs;

use super::plan::{self, OpenChoices, RenderError, RenderPlan};
use super::stages::{self, Picture, RenderCounts, SecondaryQueue, SkyRequest, SkyRules, SurfaceRequest, SurfaceRules};

/// What the modes leave Open. (Render modes)
pub mod open {
    use crate::shading::OpenItem;

    const fn item(entry: &'static str, subject: &'static str, question: &'static str) -> OpenItem {
        OpenItem { entry, subject, question }
    }

    /// (a camera counts in neither box; a camera inside a group's box is not
    /// stated);
    pub const MODE_OBJECT: OpenItem = item(
        "SO-2.9",
        "a camera inside a group, or an object of an unread kind, whose second flag word does not leave it out of the scene's boxes (SO-2.9a gives every other kind its box)",
        "",
    );
    /// a scene with no object counted has no box at all (does not state it).
    /// A box of a single value is no longer Open: the division by 0 is stated (Errors).
    pub const MODE_FLAT_BOX: OpenItem = item(
        "SO-2.9",
        "a distance or altitude render of a scene in which no object counts (the scene has no box)",
        "",
    );
}

/// A render mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderMode {
    Mask,
    Distance,
    Altitude,
}

impl RenderMode {
    /// The mode named by the vectors' and the command line's words.
    pub fn parse(word: &str) -> Option<RenderMode> {
        match word {
            "mask" => Some(RenderMode::Mask),
            "distance" => Some(RenderMode::Distance),
            "altitude" => Some(RenderMode::Altitude),
            _ => None,
        }
    }

    /// The mode drawn when several are switched on in the same render: the mask, else the
    /// altitude, else the distance, in all three versions. `None` when none is on.
    pub fn choose(on: &[RenderMode]) -> Option<RenderMode> {
        [RenderMode::Mask, RenderMode::Altitude, RenderMode::Distance].into_iter().find(|m| on.contains(m))
    }

    /// The mode the bits of a scene's switch word carry (0x400 mask, 0x800 distance, 0x1000
    /// altitude [format]), several resolved by `choose`.
    pub fn from_switch_word(word: u32) -> Option<RenderMode> {
        let bits = [(0x400, RenderMode::Mask), (0x800, RenderMode::Distance), (0x1000, RenderMode::Altitude)]; // [format]
        let on: Vec<RenderMode> = bits.iter().filter(|(b, _)| word & b != 0).map(|&(_, m)| m).collect();
        RenderMode::choose(&on)
    }

    /// The word `parse` takes.
    pub fn word(self) -> &'static str {
        match self {
            RenderMode::Mask => "mask",
            RenderMode::Distance => "distance",
            RenderMode::Altitude => "altitude",
        }
    }
}

impl fmt::Display for RenderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// First flag word: the object is selected.,
const SELECTED: u32 = 0x1;
/// Second flag word: left out of the boxes in every version.,
const LEFT_OUT: u32 = 0x40_0000;
/// Second flag word: left out of the boxes as 3.0 and 3.1.,
const LEFT_OUT_3D: u32 = 0x1;
/// Degrees to radians, the decimal the entry gives.
#[allow(clippy::approx_constant)]
const DEGREES: f64 = 0.01745329251994;
/// The steps of the curved shapes' points.
const RIM_STEP_DEGREES: f64 = 15.0;
const RIM_POINTS: u32 = 24;
const SPHERE_ROWS: u32 = 24;
/// The widening of the pyramid's cube in the test for its table. (A pyramid
/// without the `pyramid` table)
const CUBE_MARGIN: f64 = 1e-9;

/// A point carried by a placement p·M + t, row vectors.
fn carry(t: &Transform, p: Vec3) -> Vec3 {
    let m = &t.rows;
    std::array::from_fn(|j| ((p[0] * m[0][j] + p[1] * m[1][j]) + p[2] * m[2][j]) + t.translation[j])
}

fn sin(x: f64) -> f64 {
    sin_ext(x).to_f64()
}

fn cos(x: f64) -> f64 {
    cos_ext(x).to_f64()
}

/// The eight corners of the box from `low` to `high`.
fn corners(low: Vec3, high: Vec3) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(8);
    for &x in &[low[0], high[0]] {
        for &y in &[low[1], high[1]] {
            for &z in &[low[2], high[2]] {
                out.push([x, y, z]);
            }
        }
    }
    out
}

/// The angle a = 15 k degrees of the rims, in radians.
fn rim_angle(k: u32) -> f64 {
    (RIM_STEP_DEGREES * f64::from(k)) * DEGREES
}

/// What an object is to the boxes: the kinds that count.
#[derive(Clone, Copy, Debug, PartialEq)]
enum BoxKind {
    Shape(Shape),
    RadialLight,
    /// A spot or parallel light: the points lists it by (they depend on the version).
    SpotLight(RenderAs),
    ParallelLight(RenderAs),
    Sun,
    /// A cylinder or square light's own object:'s points (point 2).
    LightObject(crate::geometry::light::LightShape, RenderAs),
    /// A metaball: the eight corners of the box of the sphere's points (point 2).
    Metaball,
}

/// The points of an object's own space whose box is its box.
fn own_points(kind: BoxKind) -> Vec<Vec3> {
    match kind {
        BoxKind::Shape(Shape::Cube) => corners([-0.5; 3], [0.5; 3]),
        BoxKind::Shape(Shape::Plane) => corners([-10.0, 0.0, -10.0], [10.0, 0.0, 10.0]),
        BoxKind::Shape(Shape::Slab) => corners([-10.0, -1.0, -10.0], [10.0, 1.0, 10.0]),
        BoxKind::Shape(Shape::Square) => corners([-1.0, 0.0, -1.0], [1.0, 0.0, 1.0]),
        BoxKind::SpotLight(v) => crate::geometry::light::spot_points(v),
        BoxKind::ParallelLight(v) => crate::geometry::light::parallel_points(v),
        BoxKind::Sun => corners([-1.0; 3], [1.0; 3]),
        BoxKind::Shape(Shape::Pyramid) => vec![
            [-0.5, -0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [0.0, 0.5, 0.0],
        ],
        // (sphere, radial light: (sin f sin q, cos q, cos f sin q), q = k pi / 24,
        // f = j 2 pi / 24)
        BoxKind::Shape(Shape::Sphere) | BoxKind::RadialLight => {
            let pi = std::f64::consts::PI;
            let mut out = Vec::with_capacity(25 * 24);
            for k in 0..=SPHERE_ROWS {
                let q = (f64::from(k) * pi) / f64::from(SPHERE_ROWS);
                let (sq, cq) = (sin(q), cos(q));
                for j in 0..RIM_POINTS {
                    let f = (f64::from(j) * (2.0 * pi)) / f64::from(RIM_POINTS);
                    out.push([sin(f) * sq, cq, cos(f) * sq]);
                }
            }
            out
        }
        BoxKind::Shape(Shape::Cylinder) => (0..RIM_POINTS)
            .flat_map(|k| {
                let a = rim_angle(k);
                [[sin(a), 1.0, cos(a)], [sin(a), -1.0, cos(a)]]
            })
            .collect(),
        BoxKind::Shape(Shape::Cone) => {
            let mut out: Vec<Vec3> = (0..RIM_POINTS)
                .map(|k| {
                    let a = rim_angle(k);
                    [sin(a), -1.0, cos(a)]
                })
                .collect();
            out.push([0.0, 1.0, 0.0]);
            out
        }
        BoxKind::Shape(Shape::Torus { tube_radius: r, ring_radius: big }) => {
            let mut out = Vec::with_capacity(24 * 24);
            for i in 0..RIM_POINTS {
                let a = rim_angle(i);
                for j in 0..RIM_POINTS {
                    let b = rim_angle(j);
                    let reach = big + r * sin(b);
                    out.push([cos(a) * reach, r * cos(b), sin(a) * reach]);
                }
            }
            out
        }
        // point 2 (disc: (sin a, 0, cos a), a = 15 degrees * k, 24 points)
        BoxKind::Shape(Shape::Disc) => (0..RIM_POINTS)
            .map(|k| {
                let a = rim_angle(k);
                [sin(a), 0.0, cos(a)]
            })
            .collect(),
        // point 2 (the cylinder and pyramid lights by their own points)
        BoxKind::LightObject(shape, v) => crate::geometry::light::LightObject { shape, flags: 0 }.box_points(v),
        // point 2 (a metaball: the eight corners of its own box,'s sphere box)
        BoxKind::Metaball => {
            let (low, high) = world_box(&|p| p, own_points(BoxKind::Shape(Shape::Sphere)));
            corners(low, high)
        }
    }
}

/// A running least and greatest.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    least: f64,
    greatest: f64,
}

impl Span {
    fn take(span: &mut Option<Span>, v: f64) {
        match span {
            None => *span = Some(Span { least: v, greatest: v }),
            Some(s) => {
                if v < s.least {
                    s.least = v;
                }
                if v > s.greatest {
                    s.greatest = v;
                }
            }
        }
    }
}

/// The scene's two ranges: heights (y0, y1) and third coordinates in the camera's space
/// (z0, z1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneBoxes {
    pub height: (f64, f64),
    pub depth: (f64, f64),
}

/// Whether an object with this second flag word is left out of the boxes.
fn left_out(second_flags: u32, version: RenderAs) -> bool {
    let bits = match version {
        RenderAs::Bryce30 | RenderAs::Bryce31 => LEFT_OUT | LEFT_OUT_3D, // Variants
        RenderAs::Bryce50 => LEFT_OUT,                                    // Variants
    };
    second_flags & bits != 0
}

/// The camera's transform as `version` opens the document , as a placement whose
/// inverse carries a world point to the camera's space.
fn camera_space(scene: &Scene, version: RenderAs) -> Result<Placement, RenderError> {
    let t = scene.view.transform(version).given().ok_or(plan::open::VIEW_TURN)?;
    Placement::new(t.rows, t.translation).ok_or_else(|| RenderError::Refused {
        entry: "SO-2.9",
        reason: "the camera's transform has no inverse (its space is not defined)".to_owned(),
    })
}

/// The world box (least corner, greatest corner) of a set of own-space points carried by `world`.
fn world_box(carried: &dyn Fn(Vec3) -> Vec3, points: impl IntoIterator<Item = Vec3>) -> (Vec3, Vec3) {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in points {
        let w = carried(p);
        for j in 0..3 {
            low[j] = low[j].min(w[j]);
            high[j] = high[j].max(w[j]);
        }
    }
    (low, high)
}

/// The bit of a member's second flag word that leaves it out of its group's box.
/// point 2 (group: members marked with bit 0x4 left out), [format]
const MEMBER_LEFT_OUT: u32 = 0x4;
/// The bit of the group's first flag word (offset 4) that takes the origin of its space
/// into its box. point 2 (the first flag word; not the second, not's)
const GROUP_ORIGIN: u32 = 0x1000;
/// The widening of a group's own box on each side as 3.1 and 5.0. point 2
const GROUP_WIDEN_FIXED: f64 = 0.00001;
/// The widening of a group's own box as 3.0: a hundredth of the extent. point 2
const GROUP_WIDEN_FRACTION: f64 = 0.01;

fn open_object(scene: &Scene, i: usize) -> RenderError {
    RenderError::OpenObject { object: i, name: name_of(scene, i), item: open::MODE_OBJECT }
}

/// The box of object `i`'s box points carried by `t` (its world transform for the scene's boxes,
/// its own placement for a member in its group's space); `None` for an object with no points
/// (an empty mesh, a group with no member). Refuses a camera and an object of an unread kind.
/// point 2 (the box of each kind),
fn box_through(scene: &Scene, version: RenderAs, i: usize, t: &dyn Fn(Vec3) -> Vec3) -> Result<Option<(Vec3, Vec3)>, RenderError> {
    let o = &scene.objects[i];
    let Some(placement) = &o.placement else { return Err(open_object(scene, i)) };
    let points = |kind: BoxKind| Some(world_box(t, own_points(kind)));
    let own_box = |low: Vec3, high: Vec3| Some(world_box(t, corners(low, high)));
    Ok(match o.body {
        Body::Shape(s) => points(BoxKind::Shape(s)),
        // (whatever 0x200; a spot and a parallel light count, on or off, with or
        // without the endless option), point 2 (the other light objects)
        Body::RadialLight(l) => match &scene.lights[l] {
            Light::Radial(r) => match r.kind() {
                LightKind::Radial => points(BoxKind::RadialLight),
                LightKind::Spot => points(BoxKind::SpotLight(version)),
                LightKind::Parallel => points(BoxKind::ParallelLight(version)),
                LightKind::Cylinder => points(BoxKind::LightObject(crate::geometry::light::LightShape::Cylinder, version)),
                LightKind::Square => points(BoxKind::LightObject(crate::geometry::light::LightShape::Pyramid, version)),
            },
            _ => None,
        },
        Body::Terrain(k) => {
            let (low, high) = Relief::from_part(&scene.terrains[k].part, version, placement.second_flags).scene_box();
            own_box(low, high)
        }
        Body::Metaball { .. } => points(BoxKind::Metaball),
        // point 2 (a mesh's own box)
        Body::MeshResource(k) => {
            let b = &scene.mesh_resources[k].bounds;
            own_box([b[0], b[1], b[2]], [b[3], b[4], b[5]])
        }
        // render-modes-so-2-9a (a description mesh, which no document holds: the exact
        // extent of its triangles' corners)
        Body::Mesh(k) => {
            let pts: Vec<Vec3> = scene.meshes[k].triangles.iter().flat_map(|t| t.corners).collect();
            if pts.is_empty() {
                None
            } else {
                let (low, high) = world_box(&|p| p, pts);
                own_box(low, high)
            }
        }
        // point 2 (a tree: its own box, the tree's bounds)
        Body::Tree(k) => {
            let stored = &scene.stored_trees[scene.trees[k].stored_tree];
            let model = crate::geometry::tree::TreeModel::new(&stored.shapes, &stored.pieces, &stored.leaves)
                .map_err(|e| RenderError::Refused { entry: "GEO-22", reason: format!("tree object {i}: {e}") })?;
            let bd = model.bounds();
            own_box(bd.min, bd.max)
        }
        // point 2 (path: points of its curve,'s box points by version)
        Body::Path(k) => Some(world_box(t, scene.paths[k].part.path_as(version).box_points(version))),
        // point 2 (group: the corners of its own box carried)
        Body::Group => group_own_box(scene, version, i)?.and_then(|(low, high)| own_box(low, high)),
        Body::Camera(_) | Body::Unread | Body::TreeLeaves(_) => return Err(open_object(scene, i)),
    })
}

/// The world box of object `i` as the scene's boxes take it (the least corner, then the greatest),
/// whether or not the object's second flag word leaves it out of them; `None` for an object with
/// no points. point 2 (the box of each kind)
pub fn object_box(scene: &Scene, version: RenderAs, i: usize) -> Result<Option<(Vec3, Vec3)>, RenderError> {
    let world = scene.objects[i].world_as(version).ok_or_else(|| open_object(scene, i))?;
    box_through(scene, version, i, &|p| carry(&world, p))
}

/// Whether the mask shows a hit on object `i` (point 3: its own selection, and as 3.1
/// and 5.0 the upward walk through groups). point 3
pub fn mask_shows(scene: &Scene, version: RenderAs, i: usize) -> bool {
    shown_in_mask(scene, i, version)
}

/// A group's own box, in the group's space: the join of its members' boxes carried into it
/// (members with bit 0x4 of the second flag word left out), the origin where the group's first
/// flag word has bit 0x1000, widened. point 2 (group)
fn group_own_box(scene: &Scene, version: RenderAs, group: usize) -> Result<Option<(Vec3, Vec3)>, RenderError> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    let mut any = false;
    let group_world = scene.objects[group].world_as(version).ok_or_else(|| open_object(scene, group))?;
    let space = Placement::new(group_world.rows, group_world.translation).ok_or_else(|| RenderError::Refused {
        entry: "SO-2.9a",
        reason: format!("the group {group}'s transform has no inverse (its space is not defined)"),
    })?;
    let mut take = |b: (Vec3, Vec3)| {
        any = true;
        for j in 0..3 {
            low[j] = low[j].min(b.0[j]);
            high[j] = high[j].max(b.1[j]);
        }
    };
    for (m, member) in scene.objects.iter().enumerate() {
        if member.linked_to != Some(group) || matches!(member.body, Body::TreeLeaves(_)) {
            continue;
        }
        let Some(placement) = &member.placement else { return Err(open_object(scene, m)) };
        if placement.second_flags & MEMBER_LEFT_OUT != 0 {
            continue;
        }
        // point 2 (a member's box is made from its own points carried into
        // the group's space; the members do not take the group's first transform,, so a
        // member's points are carried out by its own transform and in by the group's inverse)
        // point 2, as 3.0 : the member's own first and second
        // transforms alone (its parents' second transforms not taken), no way in by the group's
        // inverse; the box goes out by the group's own transform in `box_through`
        let found = if version == RenderAs::Bryce30 {
            let own = placement.world();
            box_through(scene, version, m, &|p| carry(&own, p))?
        } else {
            let Some(member_world) = member.world_as(version) else { return Err(open_object(scene, m)) };
            box_through(scene, version, m, &|p| space.point_to_local(carry(&member_world, p)))?
        };
        if let Some(b) = found {
            take(b);
        }
    }
    if scene.objects[group].placement.as_ref().is_some_and(|p| p.first_flags & GROUP_ORIGIN != 0) {
        take(([0.0; 3], [0.0; 3]));
    }
    if !any {
        return Ok(None);
    }
    for j in 0..3 {
        let widen = match version {
            RenderAs::Bryce30 => (high[j] - low[j]) * GROUP_WIDEN_FRACTION,
            RenderAs::Bryce31 | RenderAs::Bryce50 => GROUP_WIDEN_FIXED,
        };
        low[j] -= widen;
        high[j] += widen;
    }
    Ok(Some((low, high)))
}

/// Every counted object's world box: every object but one whose second flag word leaves it out
/// (`left_out`). Refuses a camera and an object of an unread kind that would count.
/// , point 1
fn counted(scene: &Scene, version: RenderAs) -> Result<Vec<(Vec3, Vec3)>, RenderError> {
    let mut out = Vec::new();
    for (i, o) in scene.objects.iter().enumerate() {
        if matches!(o.body, Body::TreeLeaves(_)) {
            continue;
        }
        // : a camera counts in neither box, whatever
        // its flag words.
        if matches!(o.body, Body::Camera(_)) {
            continue;
        }
        let Some(placement) = &o.placement else {
            // No flag word to read: whether it counts is not decidable. Open
            return Err(open_object(scene, i));
        };
        // point 1 (revised: code): a program clears bit 0x400000 of a
        // group's second flag word when it reads the group, so a group counts whatever that bit
        // holds in the document (as 3.0 and 3.1 bit 0x1 still leaves it out); no other bit, and
        // not its members' bits.
        let second_flags = if o.body == Body::Group { placement.second_flags & !LEFT_OUT } else { placement.second_flags };
        if left_out(second_flags, version) {
            continue;
        }
        if let Some(e) = &o.world_open {
            return Err(RenderError::Refused { entry: "AN-1.9", reason: format!("object {i}: {e}") });
        }
        let Some(world) = o.world_as(version) else { continue };
        if let Some(b) = box_through(scene, version, i, &|p| carry(&world, p))? {
            out.push(b);
        }
    }
    let sun = scene.sun();
    if !left_out(sun.placement.second_flags, version) {
        let w = sun.placement.world();
        out.push(world_box(&|p| carry(&w, p), own_points(BoxKind::Sun)));
    }
    Ok(out)
}

fn name_of(scene: &Scene, i: usize) -> String {
    String::from_utf8_lossy(&scene.objects[i].name).into_owned()
}

/// The scene's boxes.
pub fn scene_boxes(scene: &Scene, version: RenderAs) -> Result<SceneBoxes, RenderError> {
    let camera = camera_space(scene, version)?;
    let mut height = None;
    let mut depth = None;
    for (low, high) in counted(scene, version)? {
        Span::take(&mut height, low[1]);
        Span::take(&mut height, high[1]);
        for c in corners(low, high) {
            Span::take(&mut depth, camera.point_to_local(c)[2]);
        }
    }
    // Variants (3.0 and 3.1: the origin of the world and the camera's own place)
    if matches!(version, RenderAs::Bryce30 | RenderAs::Bryce31) {
        Span::take(&mut height, 0.0);
        Span::take(&mut depth, 0.0);
    }
    let pair = |s: Option<Span>| s.map_or((f64::NAN, f64::NAN), |s| (s.least, s.greatest));
    Ok(SceneBoxes { height: pair(height), depth: pair(depth) })
}

/// Whether the stretch (near, far) of a ray meets the cube from -0.5 to 0.5 (widened by
/// `CUBE_MARGIN`) in the ray's own space. Not-a-number counts as meeting.
fn meets_cube(r: &Ray, near: f64, far: f64) -> bool {
    let h = 0.5 + CUBE_MARGIN;
    let (mut lo, mut hi) = (near, far);
    for j in 0..3 {
        let (o, d) = (r.origin[j], r.dir[j]);
        if o.is_nan() || d.is_nan() {
            return true;
        }
        if d == 0.0 {
            if !(o >= -h && o <= h) {
                return false;
            }
            continue;
        }
        let (a, b) = ((-h - o) / d, (h - o) / d);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        if a > lo {
            lo = a;
        }
        if b < hi {
            hi = b;
        }
        if lo > hi {
            return false;
        }
    }
    true
}

/// Whether the mask shows a hit on object `i`: its own bit 0x1 of the first flag word is set; as
/// 3.1 and 5.0 also when a group above it is selected, by the upward walk, which differs by
/// version: 3.1 tests each group's bit 0x4 before its selection, 5.0 after it.
/// point 3 (the mask). render-modes-so-2-9a
fn shown_in_mask(scene: &Scene, i: usize, version: RenderAs) -> bool {
    let bits = |x: usize| scene.objects.get(x).and_then(|o| o.placement.as_ref());
    let own = |x: usize| bits(x).is_some_and(|p| p.first_flags & SELECTED != 0);
    if own(i) {
        return true;
    }
    if version == RenderAs::Bryce30 {
        return false; // point 3 (as 3.0 only the object's own bit)
    }
    // The object must have no bit 0x4; the walk starts at its parent.
    if bits(i).is_none_or(|p| p.second_flags & MEMBER_LEFT_OUT != 0) {
        return false;
    }
    let mut x = i;
    // A chain of parents is finite: a parent is always an earlier object .
    while let Some(parent) = scene.objects.get(x).and_then(|o| o.linked_to) {
        if scene.objects[parent].body != Body::Group {
            return false;
        }
        let marked = bits(parent).is_none_or(|p| p.second_flags & MEMBER_LEFT_OUT != 0);
        if version == RenderAs::Bryce31 {
            // As 3.1: bit 0x4 is tested first and ends the walk even at a selected group.
            if marked {
                return false;
            }
            if own(parent) {
                return true;
            }
        } else {
            // As 5.0: a selected group shows the hit whatever its bit 0x4; an unselected
            // group with the bit ends the walk.
            if own(parent) {
                return true;
            }
            if marked {
                return false;
            }
        }
        x = parent;
    }
    false
}

/// What both rules read. (Render modes)
#[derive(Clone, Debug)]
pub struct ModeRules {
    mode: RenderMode,
    boxes: Option<SceneBoxes>,
    camera: Placement,
    /// Per document object: its first flag word has bit 0x1.
    selected: Vec<bool>,
    /// The pyramids' placements, where the plan has no `pyramid` table.
    pyramids_without_table: Vec<Placement>,
}

impl ModeRules {
    /// The rules of `mode` for a planned render of `scene`.
    pub fn new(scene: &Scene, plan: &RenderPlan, mode: RenderMode) -> Result<ModeRules, RenderError> {
        let version = plan.version;
        let boxes = match mode {
            RenderMode::Mask => None,
            RenderMode::Altitude | RenderMode::Distance => {
                let b = scene_boxes(scene, version)?;
                let (lo, hi) = if mode == RenderMode::Altitude { b.height } else { b.depth };
                // Errors (a box of one value divides by 0: 0/0 is not a number and
                // the clamp gives grey 0; positive/0 grey 1; negative/0 grey 0: the IEEE
                // division, then's clamp). no object counted, no box: refused.
                if lo.is_nan() || hi.is_nan() {
                    return Err(open::MODE_FLAT_BOX.into());
                }
                Some(b)
            }
        };
        // point 3 (a hit on a tree's leaves is a hit on the tree)
        let selected = (0..scene.objects.len())
            .map(|i| match scene.objects[i].body {
                Body::TreeLeaves(k) => shown_in_mask(scene, scene.trees[k].object, version),
                _ => shown_in_mask(scene, i, version),
            })
            .collect();
        let no_table = plan.shape_context.pyramid_normal.iter().any(|c| c.is_nan());
        let pyramids_without_table = if no_table {
            plan.geometry
                .scene
                .objects
                .iter()
                .filter(|o| o.shape == crate::geometry::Shape::Pyramid)
                .map(|o| o.placement)
                .collect()
        } else {
            Vec::new()
        };
        Ok(ModeRules { mode, boxes, camera: camera_space(scene, version)?, selected, pyramids_without_table })
    }

    /// Refuse where the ray's stretch up to `far` could have met a pyramid's sides, whose
    /// table the render does not hold. (A pyramid without the table)
    fn check_pyramids(&self, ray: &CameraRay, far: f64) -> Result<(), OpenItem> {
        let r = Ray { origin: ray.origin, dir: ray.dir };
        for p in &self.pyramids_without_table {
            if meets_cube(&p.to_local(&r), ray.start, far) {
                return Err(plan::open::PYRAMID_TABLE);
            }
        }
        Ok(())
    }

    /// The grey of a hit on document object `object`.
    fn grey(&self, object: usize, hit: &Hit) -> f64 {
        match (self.mode, self.boxes) {
            (RenderMode::Mask, _) => {
                if self.selected.get(object).copied().unwrap_or(false) { 1.0 } else { 0.0 }
            }
            (RenderMode::Altitude, Some(b)) => (hit.point[1] - b.height.0) / (b.height.1 - b.height.0),
            (RenderMode::Distance, Some(b)) => {
                let z = self.camera.point_to_local(hit.point)[2];
                (z - b.depth.0) / (b.depth.1 - b.depth.0)
            }
            _ => unreachable!("the boxes are made for the modes that divide"),
        }
    }

    /// The value of a ray that meets nothing.
    fn miss(&self) -> f64 {
        match self.mode {
            RenderMode::Mask | RenderMode::Altitude => 0.0,
            RenderMode::Distance => 1.0,
        }
    }
}

/// The surface rule of a mode. The grey is handed on in binary64; the stage rounds it once to
/// binary32 , as's Accuracy says.
pub struct ModeSurface<'a>(pub &'a ModeRules);

impl SurfaceRules for ModeSurface<'_> {
    fn surface(&mut self, _scene: &Scene, request: &SurfaceRequest, _secondary: &mut SecondaryQueue) -> Result<[f64; 3], OpenItem> {
        self.0.check_pyramids(&request.ray, request.hit.t)?;
        let v = self.0.grey(request.object, &request.hit);
        Ok([v; 3])
    }
}

/// The sky rule of a mode: the miss value; draws nothing before the first pixel.
pub struct ModeSky<'a>(pub &'a ModeRules);

impl SkyRules for ModeSky<'_> {
    fn sky(&mut self, _scene: &Scene, request: &SkyRequest) -> Result<[f64; 3], OpenItem> {
        self.0.check_pyramids(&request.ray, f64::INFINITY)?;
        Ok([self.0.miss(); 3])
    }
}

/// Render `scene` as `version` in `mode`.
pub fn render_mode(
    scene: &Scene,
    version: RenderAs,
    choices: &OpenChoices,
    mode: RenderMode,
) -> Result<(Picture, RenderCounts), RenderError> {
    let opened = plan::opened_with_haze_five(scene, version, choices.haze_five)?;
    let scene: &Scene = &opened;
    let mut plan = RenderPlan::new_for_mode(scene, version, choices)?;
    let rules = ModeRules::new(scene, &plan, mode)?;
    stages::render_picture(scene, &mut plan, &mut ModeSurface(&rules), &mut ModeSky(&rules))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_counts() {
        assert_eq!(own_points(BoxKind::Shape(Shape::Sphere)).len(), 600);
        assert_eq!(own_points(BoxKind::Shape(Shape::Cylinder)).len(), 48);
        assert_eq!(own_points(BoxKind::Shape(Shape::Cone)).len(), 25);
        assert_eq!(own_points(BoxKind::Shape(Shape::Torus { tube_radius: 0.25, ring_radius: 1.0 })).len(), 576);
        assert_eq!(own_points(BoxKind::Shape(Shape::Pyramid)).len(), 5);
        assert_eq!(own_points(BoxKind::Sun).len(), 8);
    }

    /// point 2 (the cylinder and square lights by's points: as 3.0 the
    /// extra point below, as 3.1 and 5.0 not: revised), (disc: 24 rim points)
    #[test]
    fn light_object_and_disc_points() {
        use crate::geometry::light::LightShape;
        let low = |p: &[Vec3]| p.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min);
        let cyl50 = own_points(BoxKind::LightObject(LightShape::Cylinder, RenderAs::Bryce50));
        let cyl30 = own_points(BoxKind::LightObject(LightShape::Cylinder, RenderAs::Bryce30));
        assert_eq!((cyl50.len(), cyl30.len()), (48, 49));
        assert_eq!((low(&cyl50), low(&cyl30)), (-1.0, -2.0));
        let pyr50 = own_points(BoxKind::LightObject(LightShape::Pyramid, RenderAs::Bryce50));
        let pyr31 = own_points(BoxKind::LightObject(LightShape::Pyramid, RenderAs::Bryce31));
        let pyr30 = own_points(BoxKind::LightObject(LightShape::Pyramid, RenderAs::Bryce30));
        // : the point below the object is 3.0's only; 3.1's points are 5.0's
        assert_eq!(pyr30.len(), pyr50.len() + 1);
        assert_eq!(low(&pyr30), -1.0);
        assert_eq!(pyr31.len(), pyr50.len());
        assert_eq!(low(&pyr31), low(&pyr50));
        assert_eq!(own_points(BoxKind::Shape(Shape::Disc)).len(), 24);
    }

    #[test]
    fn sphere_points_reach_the_poles() {
        let pts = own_points(BoxKind::RadialLight);
        let ys: Vec<f64> = pts.iter().map(|p| p[1]).collect();
        assert_eq!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 1.0);
        assert!(ys.iter().cloned().fold(f64::INFINITY, f64::min) <= -0.999_999_999);
    }

    #[test]
    fn left_out_by_version() {
        assert!(left_out(0x40_0000, RenderAs::Bryce50));
        assert!(!left_out(0x1, RenderAs::Bryce50));
        assert!(left_out(0x1, RenderAs::Bryce30) && left_out(0x1, RenderAs::Bryce31));
        assert!(!left_out(0x2, RenderAs::Bryce31));
    }

    #[test]
    fn cube_test() {
        let r = Ray { origin: [0.0, 0.0, -5.0], dir: [0.0, 0.0, 1.0] };
        assert!(meets_cube(&r, 0.0, f64::INFINITY));
        assert!(!meets_cube(&r, 0.0, 4.0), "the stretch stops before the cube");
        let off = Ray { origin: [0.0, 2.0, -5.0], dir: [0.0, 0.0, 1.0] };
        assert!(!meets_cube(&off, 0.0, f64::INFINITY));
    }

    #[test]
    fn mode_words() {
        for m in [RenderMode::Mask, RenderMode::Distance, RenderMode::Altitude] {
            assert_eq!(RenderMode::parse(m.word()), Some(m));
        }
        assert_eq!(RenderMode::parse("shaded"), None);
    }
}
