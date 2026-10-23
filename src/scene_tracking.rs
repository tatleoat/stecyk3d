// SPDX-License-Identifier: MIT
//! Objects and cameras that point at their targets, as a version opens the document: the
//! cameras turned as the list is read, the tracks applied, and as 3.0 every tracker turned
//! again in 3.0's order.
//!
//! (Which object tracks which; The turn; A camera; When this is done; Order
//! against linking and tracks; as revised at), (the
//! chains; A camera held to a path; An object held to a path), (the tracks applied
//! between), (a radial light's
//! place: its world's translation)
//! (3.0's order of turns after the tracks)

use crate::animation::{
    AIM_Z, AnimError, Aim, CAMERA_BIT, LEAST_LENGTH, LEAST_LENGTH_CAMERA_LATER, LinkArithmetic, OpeningInputs, OpeningRule, PARTIAL_BITS, PartsForm, camera_turn_angles, direction,
    into_parent_space, tracks_applied_on_opening, turned_in_world, turned_second,
};
use crate::scene::linked_view;
use crate::scene_opening::held_place;
use crate::scene_paths::{FACES_ALONG_BIT, HELD_BIT};
use crate::geom::Vec3;
use crate::scene::{Body, Light, Scene, linked_worlds};
use crate::scene_records::Transform;
use crate::scene_view::{OpenView, ViewTransform, ViewTransforms, view_transforms};
use crate::version::RenderAs;

/// A row of the track list resolved to objects: indices into `Scene::objects`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackRow {
    pub tracker: usize,
    pub target: usize,
}

/// How near a camera's stored rows must be to length 1 to count as unit rows (the entry
/// leaves other rows Open). (this crate's bound)
const UNIT_ROWS: f64 = 1e-9;

fn open(what: &'static str) -> AnimError {
    AnimError::Unsupported(what)
}

/// The state of one opening: the version, its arithmetic, and the trackers (by object) whose
/// result is Open.
struct Turning {
    version: RenderAs,
    arith: LinkArithmetic,
    inputs: OpeningInputs,
    open: Vec<Option<AnimError>>,
}

/// The document as `version` opens it, for a document with a track list that has rows.
pub(crate) fn opened_with_tracking(scene: &Scene, version: RenderAs) -> Result<Scene, AnimError> {
    let inputs = scene.opening_inputs();
    let mut t = Turning { version, arith: LinkArithmetic::of(version), inputs, open: vec![None; scene.objects.len()] };
    let mut s = scene.clone();
    let rows = t.rows(&s);
    // Every version, as the list is read: a camera named as tracker in a row turned at once
    // toward that row's target (a row naming the camera itself included).
    let mut turned: Vec<usize> = Vec::new();
    for row in s.tracking.clone() {
        if matches!(s.objects[row.tracker].body, Body::Camera(_)) {
            t.turn_camera(&mut s, row, Phase::AsRead);
            turned.push(row.tracker);
        }
    }
    // Then the tracks .
    let mut s = s.opened_tracks(version)?.into_owned();
    // (Outputs: a turned tracker differs from the transform its own tracks leave
    // it,, only in its rotation): the stored parts no track replaces are kept by the
    // general placement rule (as 3.1 and 5.0), no special case here.
    // : as 3.1 and 5.0 with the animation
    // switch not 0 the trackers were turned after the tracks in 3.0's order (observed on
    // three documents; beyond them Open, the fallback is 3.0's turns).
    let as_3_0 = version == RenderAs::Bryce30 || t.inputs.animation_switch != 0;
    if as_3_0 {
        turned.extend(t.after_tracks(&mut s, &rows));
    }
    t.later_versions(&s, &rows, as_3_0);
    t.finish(&mut s, &turned);
    Ok(s)
}

/// Whether an object (or, for a camera, its camera) has a track with a key.
fn own_tracks(s: &Scene, i: usize) -> bool {
    let a = &s.animation;
    let mut slots: Vec<Option<usize>> = a.object(i).placement.to_vec();
    if let Body::Camera(c) = s.objects[i].body {
        slots.extend(a.camera(c));
    }
    slots.iter().any(|&k| a.keyed(k).is_some())
}

/// When a camera is turned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// As the list is read, every version.
    AsRead,
    /// As 3.0, after the tracks (the first transform is not moved).
    Again,
}

/// The objects in the order of the lists: the root list's objects in its order, then the
/// linked lists' as the reader keeps them (each root object's lists depth first:,
/// ), which is `Scene::objects`' own order. Stated by (revised,
///, our): the root list first, then each linked list depth first.
/// (3.0's order of turns after the tracks)
fn list_order(s: &Scene) -> Vec<usize> {
    (0..s.objects.len()).collect()
}

/// Whether the object is a camera held to a path (a row of a path's list with bit 0x800):
/// the path's object and its index in `Scene::paths`.
fn held_camera(s: &Scene, i: usize) -> Option<(usize, usize)> {
    let o = &s.objects[i];
    let Body::Camera(_) = o.body else { return None };
    let parent = o.linked_to?;
    match s.objects[parent].body {
        Body::Path(k) if o.placement.is_some_and(|p| p.first_flags & HELD_BIT != 0) => Some((parent, k)),
        _ => None,
    }
}

/// Whether the object (not a camera) is held to a path (a row of a path's list with bit
/// 0x800): the path's object.
fn held_object(s: &Scene, i: usize) -> Option<usize> {
    let o = &s.objects[i];
    if matches!(o.body, Body::Camera(_)) {
        return None;
    }
    let parent = o.linked_to?;
    match s.objects[parent].body {
        Body::Path(_) if o.placement.is_some_and(|p| p.first_flags & HELD_BIT != 0) => Some(parent),
        _ => None,
    }
}

/// The object's parents, nearest first.
fn ancestors(s: &Scene, i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = s.objects[i].linked_to;
    while let Some(p) = at {
        out.push(p);
        at = s.objects[p].linked_to;
    }
    out
}

impl Turning {
    fn mark(&mut self, i: usize, e: AnimError) {
        if self.open[i].is_none() {
            self.open[i] = Some(e);
        }
    }

    /// Whether the object, or one of its parents, is Open here.
    fn open_on_chain(&self, s: &Scene, i: usize) -> bool {
        self.open[i].is_some() || ancestors(s, i).iter().any(|&p| self.open[p].is_some())
    }

    /// Whether the row is the tracker settles as 5.0 in Bryce 5's form with the switch
    /// set (the documents `tr-`): the only row of the
    /// list, its tracker an object of the root list that is not a camera, its target with no
    /// track of its own. Its tracks are applied at s*k and it is then turned as 3.0 turns a
    /// tracker after the tracks.
    fn settled_tracker(&self, s: &Scene, rows: usize, r: TrackRow) -> bool {
        self.version == RenderAs::Bryce50
            && self.inputs.form == PartsForm::Bryce5
            && rows == 1
            && !matches!(s.objects[r.tracker].body, Body::Camera(_))
            && s.objects[r.tracker].linked_to.is_none()
            && !own_tracks(s, r.target)
    }

    /// The rows with their positions in the list, taken in order: a second row for a tracker
    /// replaces its target; a row (T, G) where G's target is already T, or a row (T, T),
    /// leaves T with no target (and without an earlier row's); rings of three or more are
    /// rows like any.
    fn rows(&mut self, s: &Scene) -> Vec<(usize, TrackRow)> {
        let mut rows: Vec<(usize, TrackRow)> = Vec::new();
        for (k, &r) in s.tracking.iter().enumerate() {
            let target_of = |rows: &Vec<(usize, TrackRow)>, x: usize| rows.iter().find(|(_, q)| q.tracker == x).map(|(_, q)| q.target);
            rows.retain(|(_, q)| q.tracker != r.tracker);
            if r.target == r.tracker || target_of(&rows, r.target) == Some(r.tracker) {
                continue;
            }
            rows.push((k, r));
        }
        rows
    }

    /// The target's place: the translation of its own transform. `None` where it has none.
    fn place(&self, s: &Scene, g: usize) -> Option<Vec3> {
        if self.open_on_chain(s, g) {
            return None;
        }
        s.objects[g].world_as(self.version).map(|w| w.translation)
    }

    /// The chain of an object : its second transform followed by its parent's chain
    /// (a path's as any object's).
    fn chain(&self, s: &Scene, i: usize) -> Option<Transform> {
        let o = &s.objects[i];
        let p = o.placement?;
        match o.linked_to {
            None => Some(p.second),
            Some(parent) => {
                let pc = self.chain(s, parent)?;
                Some(self.arith.chain(&p.second, p.first_flags, &pc, self.version))
            }
        }
    }

    /// The target's place carried into the space of the tracker's parent (step 1): the
    /// place itself for a tracker of the root list.
    fn place_for(&self, s: &Scene, x: usize, g: usize) -> Result<Vec3, AnimError> {
        let q = self.place(s, g).ok_or_else(|| open("a tracker whose target has no place (AN-1.9a)"))?;
        let Some(parent) = s.objects[x].linked_to else { return Ok(q) };
        let first_flags = s.objects[x].placement.map_or(0, |p| p.first_flags);
        if first_flags & PARTIAL_BITS != 0 {
            return Err(open("a linked tracker with bits of 0xf80000 in its first flag word (AN-1.9a: open)"));
        }
        let c = self.chain(s, parent).ok_or_else(|| open("a tracker whose parent's chain is not given (AN-1.9)"))?;
        into_parent_space(q, &c).ok_or_else(|| open("a tracker whose parent's chain has no inverse (AN-1.9a: open)"))
    }

    /// A camera turned toward its row's target (A camera): as the list is read, or
    /// as 3.0 after the tracks (`Phase::Again`: the first transform is not moved).
    fn turn_camera(&mut self, s: &mut Scene, row: TrackRow, phase: Phase) {
        if let Err(e) = self.try_turn_camera(s, row, phase) {
            self.mark(row.tracker, e);
        }
    }

    fn try_turn_camera(&mut self, s: &mut Scene, row: TrackRow, phase: Phase) -> Result<(), AnimError> {
        let obj = row.tracker;
        if self.open[obj].is_some() {
            return Ok(());
        }
        let Body::Camera(c) = s.objects[obj].body else { unreachable!("a camera row") };
        let p = s.cameras[c].placement;
        // : as 3.0 the axis its first flag word
        // chooses, as any tracker's; as 3.1 and 5.0 the reading sets bit 0x8000000, so z (the
        // bit 0x10000000 as stored)
        let aim = match self.version {
            RenderAs::Bryce30 => Aim::of_flags(p.first_flags),
            RenderAs::Bryce31 | RenderAs::Bryce50 => Aim::of_flags(p.first_flags | AIM_Z),
        };
        let unit = |r: Vec3| ((((r[0] * r[0] + r[1] * r[1]) + r[2] * r[2]).sqrt()) - 1.0).abs() <= UNIT_ROWS;
        if !p.second.rows.iter().all(|&r| unit(r)) {
            return Err(open("a tracking camera whose second transform's rows are not unit (AN-1.9a: open)"));
        }
        // (step 1: the target's place in the parent's space, a camera linked
        // below an object or a path included)
        let q = self.place_for(s, obj, row.target)?;
        if p.second_flags & CAMERA_BIT != 0 {
            let target_first = s.objects[row.target].placement.map(|t| t.first.translation);
            if p.first.translation != [0.0; 3] || target_first != Some([0.0; 3]) {
                return Err(open(
                    "a tracking camera with bit 0x100000 of its second flag word and an offset first transform (AN-1.9a: open)",
                ));
            }
        }
        // (A camera held to a path that has a target: as 3.1 with the switch 0,
        // and as 5.0 by the fallback, turned as the list is read with v measured from the
        // place where the path holds it at its saved distance; as 3.0 from its stored place)
        let mut held_saved_place = None;
        let from = match held_camera(s, obj) {
            Some((path_obj, k)) if phase == Phase::AsRead && self.version != RenderAs::Bryce30 => {
                let path_first = s.objects[path_obj].placement.map(|q| q.first);
                let at = held_place(self.version, s.paths[k].part.path_as(self.version), path_first, p.saved_distance)?.0;
                // : as 3.1 and 5.0 with the switch not 0,
                // a camera whose distance track has a key is moved after the reading
                let a = &s.animation;
                if self.inputs.animation_switch != 0
                    && a.keyed(a.object(obj).placement[crate::scene_records::placement_slot::PATH]).is_some()
                {
                    held_saved_place = Some(at);
                }
                at
            }
            _ => p.second.translation,
        };
        let least = if self.version == RenderAs::Bryce30 { LEAST_LENGTH } else { LEAST_LENGTH_CAMERA_LATER };
        let d = direction(q, from, least)?;
        let (second, moved, angles) = camera_turn_angles(&self.arith, &p.first, &p.second, d, aim)?;
        let cam = &mut s.cameras[c];
        cam.placement.second = second;
        // The camera's two angles go with its turned rows: the view of the perspective camera
        // is made from its angles where the tracks are applied .
        // (a held camera turned again)
        if d.is_some() {
            (cam.part.angles[0], cam.part.angles[1]) = (angles[0], angles[1]);
        }
        // (A camera (c): as 3.0, a camera turned again after the tracks does not
        // move its first transform)
        if phase == Phase::AsRead {
            cam.placement.first.translation = moved;
            // : the
            // second transform takes the translation q's of the place where the path holds the
            // camera at its saved distance, and the first transform's translation t_F keeps the
            // origin at its stored place: t_F * M + q's = the origin of the stored transforms.
            if let Some(q_saved) = held_saved_place
                && d.is_some()
            {
                let m = cam.placement.second.rows;
                let o: Vec3 = std::array::from_fn(|j| {
                    let r = p.first.translation;
                    ((r[0] * p.second.rows[0][j] + r[1] * p.second.rows[1][j]) + r[2] * p.second.rows[2][j]) + p.second.translation[j]
                });
                let w: Vec3 = std::array::from_fn(|j| o[j] - q_saved[j]);
                // (o - q's) * M^T: element j is w . (row j of M), M a rotation
                cam.placement.first.translation = std::array::from_fn(|j| (w[0] * m[j][0] + w[1] * m[j][1]) + w[2] * m[j][2]);
                cam.placement.second.translation = q_saved;
            }
        }
        s.objects[obj].placement = Some(cam.placement);
        self.refresh(s);
        Ok(())
    }

    /// An object (not a camera) turned toward its target, as 3.0 after the tracks.
    fn turn_object(&mut self, s: &mut Scene, row: TrackRow) {
        if let Err(e) = self.try_turn_object(s, row) {
            self.mark(row.tracker, e);
        }
    }

    fn try_turn_object(&mut self, s: &mut Scene, row: TrackRow) -> Result<(), AnimError> {
        let x = row.tracker;
        if self.open_on_chain(s, x) {
            return Ok(());
        }
        let o = &s.objects[x];
        // : as 3.1 and 5.0 with the
        // switch not 0 a held object with the facing bit 0x400 is placed and not turned.
        if self.version != RenderAs::Bryce30
            && held_object(s, x).is_some()
            && o.placement.is_some_and(|p| p.first_flags & FACES_ALONG_BIT != 0)
        {
            return Ok(());
        }
        let Some(p) = o.placement else {
            return Err(open("a tracker whose parts are Open (SCN-5; AN-1.9a needs its second transform)"));
        };
        if o.world_as(self.version).is_none() {
            return Ok(()); // already without a world (its own `world_open` says why)
        }
        // (step 1; a tracker linked to a path that does not hold it, or held to
        // one and placed by it, as one linked to any object)
        let q = self.place_for(s, x, row.target)?;
        let second = if self.version == RenderAs::Bryce30 || o.linked_to.is_none() {
            let d = direction(q, p.second.translation, LEAST_LENGTH)?;
            turned_second(&self.arith, &p.second, d, Aim::of_flags(p.first_flags))?
        } else {
            // (as 3.1 and 5.0 with the switch not 0: the turn in the world's axes
            // on the tracker's chain). (revised, 14753)
            let parent = o.linked_to.expect("linked");
            let pc = self.chain(s, parent).ok_or_else(|| open("a tracker whose parent's chain is not given (AN-1.9)"))?;
            let c = self.chain(s, x).ok_or_else(|| open("a tracker whose chain is not given (AN-1.9)"))?;
            let q = self.place(s, row.target).ok_or_else(|| open("a tracker whose target has no place (AN-1.9a)"))?;
            turned_in_world(&self.arith, &p.second, &c, Some(&pc), q, Aim::of_flags(p.first_flags), LEAST_LENGTH)?
        };
        if let Some(pl) = s.objects[x].placement.as_mut() {
            pl.second = second;
        }
        self.refresh(s);
        Ok(())
    }

    /// As 3.0, after the tracks: the objects in the order of the lists; each object X that
    /// has a target is turned, then at once each object whose target is X, once, toward X's
    /// new place, and no further. The camera named director held to a path is not turned
    /// again . Returns the objects turned.
    fn after_tracks(&mut self, s: &mut Scene, rows: &[(usize, TrackRow)]) -> Vec<usize> {
        let order = list_order(s);
        let mut target_of: Vec<Option<usize>> = vec![None; s.objects.len()];
        for &(_, r) in rows {
            target_of[r.tracker] = Some(r.target);
        }
        let excluded = |s: &Scene, x: usize| held_camera(s, x).is_some() && matches!(s.objects[x].body, Body::Camera(c) if s.cameras[c].name.as_slice() == b"director");
        let mut turned = Vec::new();
        for &x in &order {
            let Some(g) = target_of[x] else { continue };
            if excluded(s, x) {
                continue;
            }
            self.turn_any(s, TrackRow { tracker: x, target: g });
            turned.push(x);
            for &z in &order {
                if target_of[z] == Some(x) && !excluded(s, z) {
                    self.turn_any(s, TrackRow { tracker: z, target: x });
                    turned.push(z);
                }
            }
        }
        turned
    }

    fn turn_any(&mut self, s: &mut Scene, row: TrackRow) {
        match s.objects[row.tracker].body {
            Body::Camera(_) => self.turn_camera(s, row, Phase::Again),
            _ => self.turn_object(s, row),
        }
    }

    /// As 3.1 and 5.0 no object but a camera is turned; the cases the entry leaves Open are
    /// marked.
    fn later_versions(&mut self, s: &Scene, rows: &[(usize, TrackRow)], turned_as_3_0: bool) {
        for &(i, r) in rows.iter().filter(|_| !turned_as_3_0) {
            if matches!(s.objects[r.tracker].body, Body::Camera(_)) {
                continue;
            }
            let anc = ancestors(s, r.tracker);
            let camera_later = rows.iter().any(|&(j, c)| {
                j > i && matches!(s.objects[c.tracker].body, Body::Camera(_)) && (r.target == c.tracker || anc.contains(&c.tracker))
            });
            if camera_later {
                self.mark(
                    r.tracker,
                    open("an object that tracks a camera, or is linked below one, whose row comes before the camera's (AN-1.9a: open)"),
                );
            }
        }
        // 5.0 applying tracks: a tracker with a track of its own. Settled (
        // , the documents `tr-`) for Bryce 5's form with one
        // row, whose tracker is an object of the root list (not a camera) and whose target has
        // no track of its own: the tracker's tracks at s·k, then the turn as 3.0 turns after
        // the tracks. Open beyond (more rows, a target with a track, a linked tracker, a
        // camera, Bryce 3D's form).
        if self.version == RenderAs::Bryce50
            && matches!(tracks_applied_on_opening(self.version, self.inputs), Ok(OpeningRule::Marked { .. } | OpeningRule::EveryTrackButViewCameras))
        {
            for &(_, r) in rows {
                // : a held object that
                // has a target, in Bryce 3D's form with the switch set, is observed as 5.0
                // (tracking-switch1.jsonl: tracker-held-to-path-sw1, tracker-held-facing-sw1).
                let settled = self.settled_tracker(s, rows.len(), r)
                    || (self.inputs.form == PartsForm::Bryce3d
                        && self.inputs.animation_switch != 0
                        && (held_object(s, r.tracker).is_some() || held_camera(s, r.tracker).is_some()));
                // (a held camera with a target:, A camera held to a path, observed as 5.0
                // with the switch 1 on camera-held-target-far-sw1)
                if !settled && own_tracks(s, r.tracker) {
                    self.mark(
                        r.tracker,
                        open("a tracker with a track of its own in a marked document of Bryce 5's form opened as 5.0 (AN-1.9a: open)"),
                    );
                }
            }
        }
    }

    /// The worlds made again after a turn: the root list's products, the linked
    /// objects' chains, the trees' copies and the radial lights' places; every `world_open`
    /// set before is kept.
    fn refresh(&self, s: &mut Scene) {
        let before: Vec<Option<AnimError>> = s.objects.iter().map(|o| o.world_open.clone()).collect();
        for o in s.objects.iter_mut() {
            let Some(p) = o.placement else { continue };
            if o.world_open.is_some() {
                continue;
            }
            if o.linked_to.is_none() {
                o.world = Some(p.world());
            } else {
                o.world = None;
                o.linked_worlds = None;
            }
            if let Body::Tree(t) = o.body {
                s.trees[t].placement = p;
                s.trees[t].world = p.world();
            }
        }
        linked_worlds(&mut s.objects, &mut s.lights, &mut s.trees);
        for (o, e) in s.objects.iter_mut().zip(before) {
            if e.is_some() {
                o.world = None;
                o.linked_worlds = None;
                o.world_open = e;
            }
        }
        let version = self.version;
        for l in s.lights.iter_mut() {
            if let Light::Radial(r) = l
                && let Some(w) = s.objects[r.object].world_as(version)
            {
                r.position = w.translation;
            }
        }
    }

    /// The Open marks applied (the tracker and every object linked below it have no world; a
    /// view camera's view is Open), and the view made again from a turned camera (followed by
    /// its parent's chain for a linked camera).
    /// (the view)
    fn finish(&self, s: &mut Scene, turned: &[usize]) {
        for i in 0..s.objects.len() {
            let e = std::iter::once(i).chain(ancestors(s, i)).find_map(|j| self.open[j].clone());
            if let Some(e) = e {
                let o = &mut s.objects[i];
                o.world = None;
                o.linked_worlds = None;
                o.world_open = Some(e);
            }
        }
        let c = s.view.camera;
        let cam = &s.cameras[c];
        let obj = cam.object;
        let current = s.view.transform(self.version);
        let stand_in = match current {
            ViewTransform::Open(v) => v.stand_in,
            t => t.given().expect("a given transform"),
        };
        if self.open_on_chain(s, obj) {
            s.view.transforms = ViewTransforms::all(ViewTransform::Open(OpenView { stand_in }));
            return;
        }
        if !turned.contains(&obj) || matches!(current, ViewTransform::Open(_)) {
            return;
        }
        // As 3.0 the camera ends turned toward its target whatever its angles give; as 3.1
        // and 5.0 the stored (turned) rows stand where the view's rule keeps them.
        let perspective = cam.name.as_slice() == b"perspective" && self.version != RenderAs::Bryce30;
        let t = view_transforms(&cam.placement.first, &cam.placement.second, cam.part.angles, cam.part.roll, perspective, self.inputs)
            .get(self.version);
        s.view.transforms = ViewTransforms::all(t);
        if s.objects[obj].linked_to.is_some() {
            s.view.transforms = linked_view(&s.objects, &s.cameras, &s.view, self.inputs);
        }
    }
}
