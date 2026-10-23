// SPDX-License-Identifier: MIT
//! A read document as a version opens it: its owners' tracks applied where applies
//! them.
//!
//! (revised: material tracks, the blend between keys; the flag track changes
//! nothing), (linked objects made again from
//! the opened chains),,
//! , (a document
//! with a track list: `opened` hands it to `scene_tracking`, which calls `opened_tracks`
//! between the turns)
//! as revised at (An object held to a path; A camera held to a path; A
//! camera linked to an object), (the tracks at a time a frame asks,
//! through `Scene::opening_inputs`)
//! (Bryce 5's documents: Opening),
//! (Paths of motion in documents; Frames)

use std::borrow::Cow;

use crate::animation::{
    AnimError, ChannelAt, ChannelNumbers, Interpolate, Opening, OpeningRule, TimeMap, Track, TrackReader, channel_at_asked,
    orientation_at, place_at, texture_record_at,
};
use crate::animation::Path;
use crate::scene::{Body, Light, Material, Scene, SecondChannel, Shape, linked_view, linked_worlds};
use crate::scene_textures::{TextureBody, TextureClass};
use crate::scene_paths::{FACES_ALONG_BIT, HELD_BIT};
use crate::scene_beams::BeamRecord;
use crate::scene_parts::{Channel, Rgb, TextureRef, light_slot};
use crate::scene_records::{PlacementPart, Transform, placement_slot};
use crate::scene_tracks::{DocTrack, SceneTracks, TrackValues, slots};
use crate::scene_view::{OpenView, VERSIONS, ViewTransform, ViewTransforms, rows_from_angles, view_transforms};
use crate::version::RenderAs;

/// A texture of the sky's layers with a track that has keys: whether its tracks apply is Open.
fn refuse_sky_texture_tracks() -> AnimError {
    AnimError::Unsupported("a track of a texture of the sky's cloud layers applied on opening (AN-1.8a: open)")
}

/// A channel key that names a texture the document does not hold (the reader reads every
/// texture the keys name, so none is reachable from a read document).
/// (Opening: the textures the keys name are read with the others)
fn refuse_unread_texture() -> AnimError {
    AnimError::Unsupported("a material channel key that names a texture the scene does not hold (AN-1.8)")
}

/// A track of a class whose keys this reader does not lay out, with keys, where tracks are
/// applied.
fn refuse_unread_track() -> AnimError {
    AnimError::Unsupported(
        "a track whose keys' layout SCN-6d leaves Open (Obj_ePathT, Mat_eMatFlags, Cam_eCorpOrient, ...), applied on opening",
    )
}

/// A held object's placement, and whether the structure's box for it is made again (A
/// held object with no key, as 5.0: not, where no track has a key).
struct Held {
    placement: PlacementPart,
    box_made_again: bool,
}

/// An object with a distance track (slot 1) that is not held to the path it is linked to.
fn refuse_path_track() -> AnimError {
    AnimError::Unsupported(
        "an object with the held bit 0x800 that no path holds, with a key on a track that places a held object (AN-1.9: the implementer's choice)",
    )
}

impl Scene {
    /// The reading `version` makes of the document where it is not this one (Errors:
    /// parts of Bryce 5's versions as 3.0 and 3.1; Variants: a texture record of version
    /// 2 as 3.0), with this copy's frame time; `None` where this reading is the version's.
    /// An error where the version does not open the document. The reading `opened` makes
    /// reads every part by its own version (the vectors of compare it);
    /// a render starts from this one (`render::plan::opened`).
    /// . (Readings as 3.0 and 3.1)
    pub fn read_by(&self, version: RenderAs) -> Result<Option<Scene>, AnimError> {
        let Some(older) = &self.older else { return Ok(None) };
        let inner = match version {
            RenderAs::Bryce30 if older.three_one.is_some() => {
                return Err(AnimError::NotOpened(
                    "Bryce 3.0 stops with a fault as it opens a document with a placement part of version 2 or a light part of version 2 or 3 (SCN-2, Errors)".to_string(),
                ));
            }
            RenderAs::Bryce30 => match &older.three_zero {
                Some(Ok(s)) => Some(&**s),
                Some(Err(why)) => {
                    return Err(AnimError::NotOpened(format!(
                        "read as Bryce 3.0 reads a texture record of version 2 (as version 1), the document is not made (SCN-15, Variants): {why}"
                    )));
                }
                None => None,
            },
            RenderAs::Bryce31 => match &older.three_one {
                Some(Ok(s)) => Some(&**s),
                Some(Err(why)) => {
                    return Err(AnimError::NotOpened(format!(
                        "read as Bryce 3.1 reads its parts of Bryce 5's versions, the document is not made (SCN-2, Errors): {why}"
                    )));
                }
                None => None,
            },
            RenderAs::Bryce50 => None,
        };
        Ok(inner.map(|inner| Scene { frame_time: self.frame_time, ..inner.clone() }))
    }
    /// The document as `version` opens it : where tracks are applied, each owner's
    /// values replaced by its tracks' values at the opening time; otherwise the scene as
    /// read. An error where what opening applies is Open (point 1) and some track has
    /// keys, and where a track's value is Open (a material blend, a layout does not
    /// give). The copy is meant for `version` only: its view holds `version`'s transform.
    ///
    /// A document whose track list has rows has its trackers turned too : the
    /// cameras as the list is read, before the tracks, and as 3.0 every tracker after them
    /// (`scene_tracking`). A tracker whose turn is Open has no world (`world_open`), and the
    /// view through it is Open; the rest of the document opens.
    /// . (Bryce 5's documents:
    /// Opening), animation.md
    pub fn opened(&self, version: RenderAs) -> Result<Cow<'_, Scene>, AnimError> {
        if self.tracking.is_empty() {
            return self.opened_tracks(version);
        }
        crate::scene_tracking::opened_with_tracking(self, version).map(Cow::Owned)
    }

    /// The tracks applied as says, without the turns of.
    /// . (Bryce 5's documents: Opening)
    pub(crate) fn opened_tracks(&self, version: RenderAs) -> Result<Cow<'_, Scene>, AnimError> {
        // : a held object is placed whether or not
        // a track has a key, so as 5.0 a document with one has something to apply.
        // (The wind; read 2026-10-22: the routine that applies the sky's
        // tracks runs whenever the sky names any track, with or without a key, and then makes the
        // offsets and the edge noise's numbers anew),
        let sky_names_tracks = self.sky.tracks.iter().any(|&t| t != 0);
        if !self.animation.any_keys() && !sky_names_tracks && !(version == RenderAs::Bryce50 && self.holds_object_to_path()) {
            return Ok(Cow::Borrowed(self)); // nothing to apply (a track with no key)
        }
        let inputs = self.opening_inputs();
        let opening = Opening::new(version, inputs)?;
        if !opening.applies_tracks {
            return Ok(Cow::Borrowed(self));
        }
        if opening.rule.applies_sky_tracks() && self.sky_texture_tracks_keyed() {
            return Err(refuse_sky_texture_tracks());
        }
        let mut s = self.clone();
        let a = Apply { tracks: &self.animation, version, opening };
        a.materials(&mut s)?;
        a.textures(&mut s, self)?;
        a.objects(&mut s)?;
        a.terrains(&mut s)?;
        a.lights(&mut s)?;
        a.sky(&mut s)?;
        a.view(&mut s)?;
        Ok(Cow::Owned(s))
    }

    /// Whether an object is held to a path: a row of a path's list with bit 0x800 of its first
    /// flag word.
    fn holds_object_to_path(&self) -> bool {
        self.objects.iter().any(|o| {
            o.linked_to.is_some_and(|p| matches!(self.objects[p].body, Body::Path(_)))
                && o.placement.is_some_and(|p| p.first_flags & HELD_BIT != 0)
                && !matches!(o.body, Body::Camera(_))
        })
    }

    /// Whether a texture of the sky's cloud layers has a track with a key (Open where its
    /// tracks apply).
    fn sky_texture_tracks_keyed(&self) -> bool {
        [self.clouds.stratus, self.clouds.cumulus]
            .into_iter()
            .flatten()
            .any(|t| self.animation.texture(t).iter().any(|&slot| self.animation.keyed(slot).is_some()))
    }
}

/// The textures of a scene by class and identity, to turn a key's references into indices.
/// (Texture tracks on opening)
struct TextureIndex(Vec<(TextureClass, u32)>);

impl TextureIndex {
    /// A channel's references A, B and alpha as indices into `Scene::textures`.
    fn refs(&self, refs: &[Option<TextureRef>; 3]) -> Result<[Option<usize>; 3], AnimError> {
        let mut out = [None; 3];
        for (k, r) in refs.iter().enumerate() {
            let Some(r) = r else { continue };
            let class = TextureClass::of_code(r.code).ok_or_else(refuse_unread_texture)?;
            out[k] = Some(self.0.iter().position(|&(c, id)| c == class && id == r.identity).ok_or_else(refuse_unread_texture)?);
        }
        Ok(out)
    }
}

/// What applying needs: the bound tracks, the version and the opening.
struct Apply<'a> {
    tracks: &'a SceneTracks,
    version: RenderAs,
    opening: Opening,
}

impl Apply<'_> {
    /// The tracks of the textures that materials with a track name: the mapping's mode and
    /// switches, scale, offset and turn, and a procedural texture's record, at the opening
    /// time. A texture is named by a material as the document holds it or as its tracks make
    /// it (either channel of a mix). A texture that no material with a track names keeps what
    /// its record holds.,
    fn textures(&self, s: &mut Scene, read: &Scene) -> Result<(), AnimError> {
        let mut named = vec![false; s.textures.len()];
        let has_track = |m: &crate::scene::Material| m.part.tracks.iter().any(|&id| id != 0);
        for (m, before) in s.materials.iter().zip(&read.materials).filter(|(m, _)| has_track(m)) {
            let own = [m.textures, before.textures];
            let mut all: Vec<usize> = own.iter().flat_map(|t| t.colours.iter().chain(&t.values)).flatten().flatten().copied().collect();
            let seconds = m.second.colours.iter().flatten().map(|c| c.textures).chain(m.second.values.iter().flatten().map(|c| c.textures));
            all.extend(seconds.flatten().flatten());
            for t in all {
                named[t] = true;
            }
        }
        for (t, texture) in s.textures.iter_mut().enumerate().filter(|(t, _)| named[*t]) {
            let sl = self.tracks.texture(t);
            let map = &mut texture.mapping;
            if let Some(d) = self.keyed(sl[0]) {
                let TrackValues::Words(tr) = &d.values else { return Err(refuse_unread_track()) };
                if let Some(w) = self.plain(d, tr)? {
                    (map.mode, map.switches) = (w.mode, w.switches);
                }
            }
            if let Some(v) = self.triple(sl[1])? {
                map.scale = v;
            }
            if let Some(v) = self.triple(sl[2])? {
                map.offset = v;
            }
            if let Some(q) = self.orientation(sl[3])? {
                map.turn = q;
            }
            if let Some(d) = self.keyed(sl[4]) {
                let TrackValues::Record(records) = &d.values else { return Err(refuse_unread_track()) };
                // (Variants: a key's record part in version 1's form as 3.0 and 3.1,
                // in version 2's as 5.0)
                let own_form = match &texture.body {
                    TextureBody::Procedural(p) => p.form,
                    _ => crate::scene_textures::RecordForm::Whole,
                };
                let tr = records.as_read_by(self.version, own_form).map_err(|_| {
                    AnimError::Unsupported("a key of a texture's own track that does not read in the form this version takes (AN-1.8a)")
                })?;
                let record = texture_record_at(&self.with_map(d, tr), self.opening.time)?;
                if let (Some((settings, stand)), TextureBody::Procedural(p)) = (record, &mut texture.body) {
                    p.settings = settings;
                    // : at a stand the key's whole record is the texture's, its
                    // result as saved included; between keys the texture keeps what it held
                    if let Some(key) = stand {
                        p.saved_result = records.saved[key];
                    }
                }
            }
        }
        Ok(())
    }

    /// The track a slot names, with keys.
    fn keyed(&self, slot: Option<usize>) -> Option<&DocTrack> {
        self.tracks.keyed(slot)
    }

    /// The track with its time map read by this version's arithmetic.
    fn with_map<V: Clone>(&self, t: &DocTrack, track: &Track<V>) -> Track<V> {
        let mut track = track.clone();
        track.time_map = t.map.as_ref().map(|points| TimeMap { points: points.clone(), version: self.version });
        track
    }

    /// A plain track's value at the opening time , read once.
    fn plain<V: Interpolate>(&self, t: &DocTrack, track: &Track<V>) -> Result<Option<V>, AnimError> {
        let track = self.with_map(t, track);
        TrackReader::new(&track).value_at(self.opening.time)
    }

    fn double(&self, slot: Option<usize>) -> Result<Option<f64>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Double(tr) => self.plain(t, tr),
            _ => Err(refuse_unread_track()),
        }
    }

    fn single(&self, slot: Option<usize>) -> Result<Option<f32>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Single(tr) => Ok(self.plain(t, tr)?.map(|v| v[0])),
            _ => Err(refuse_unread_track()),
        }
    }

    fn colour(&self, slot: Option<usize>) -> Result<Option<[f32; 3]>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Colour(tr) => self.plain(t, tr),
            _ => Err(refuse_unread_track()),
        }
    }

    fn triple(&self, slot: Option<usize>) -> Result<Option<[f64; 3]>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Triple(tr) => self.plain(t, tr),
            _ => Err(refuse_unread_track()),
        }
    }

    /// A place track's value (the version's rules).
    fn place(&self, slot: Option<usize>) -> Result<Option<[f64; 3]>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Place(tr) => place_at(self.version, &self.with_map(t, tr), self.opening.time),
            _ => Err(refuse_unread_track()),
        }
    }

    /// An orientation track's value (the version's rules).
    fn orientation(&self, slot: Option<usize>) -> Result<Option<[f64; 4]>, AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(None) };
        match &t.values {
            TrackValues::Orientation(tr) => orientation_at(self.version, &self.with_map(t, tr), self.opening.time),
            _ => Err(refuse_unread_track()),
        }
    }

    /// A material channel track's value: a key's whole channel, or the blend between two keys
    /// with the second channel where the keys name different textures; at the time a frame
    /// asks,'s amount., (the read time, the key
    /// search),
    fn channel<T: ChannelNumbers>(&self, t: &DocTrack, track: &Track<Channel<T>>) -> Result<Option<ChannelAt<T>>, AnimError> {
        let track = self.with_map(t, track);
        // : the same
        // when a document opens as when a frame asks
        channel_at_asked(&track, self.opening.time, self.version)
    }

    /// A material's channel replaced by its track: the channel at the time, the textures its
    /// references name, and the second channel where there is one.
    fn replace_channel<T: ChannelNumbers>(
        &self,
        slot: Option<usize>,
        found: &TextureIndex,
        into: (&mut Channel<T>, &mut [Option<usize>; 3], &mut Option<SecondChannel<T>>),
        unpack: impl Fn(&TrackValues) -> Option<&Track<Channel<T>>>,
    ) -> Result<(), AnimError> {
        let Some(t) = self.keyed(slot) else { return Ok(()) };
        let track = unpack(&t.values).ok_or_else(refuse_unread_track)?;
        let (ch, textures, second) = into;
        if let Some(at) = self.channel(t, track)? {
            *ch = at.own;
            *textures = found.refs(&at.own.refs)?;
            *second = match at.second {
                Some(sc) => Some(SecondChannel {
                    channel: sc.channel,
                    textures: found.refs(&sc.channel.refs)?,
                    amount: sc.amount,
                }),
                None => None,
            };
        }
        Ok(())
    }

    /// Every material's tracks.,
    fn materials(&self, s: &mut Scene) -> Result<(), AnimError> {
        let found = TextureIndex(s.textures.iter().map(|t| (t.class, t.identity)).collect());
        for (i, m) in s.materials.iter_mut().enumerate() {
            let sl = self.tracks.material(i);
            let Material { part, textures, second, .. } = m;
            for (k, ch) in part.colours.iter_mut().enumerate() {
                let into = (ch, &mut textures.colours[k], &mut second.colours[k]);
                self.replace_channel(sl[k], &found, into, |v| match v {
                    TrackValues::ColourChannel(t) => Some(t),
                    _ => None,
                })?;
            }
            for (k, ch) in part.values.iter_mut().enumerate() {
                let into = (ch, &mut textures.values[k], &mut second.values[k]);
                self.replace_channel(sl[5 + k], &found, into, |v| match v {
                    TrackValues::ValueChannel(t) => Some(t),
                    _ => None,
                })?;
            }
            if let Some(c) = self.colour(sl[slots::MAT_VOLUME_COLOUR])? {
                part.volume_colour = c;
            }
            if let Some(r) = self.single(sl[slots::MAT_REFRACTION])? {
                part.refraction = r;
            }
            for k in 0..3 {
                if let Some(v) = self.single(sl[slots::MAT_VOLUME_NUMBERS + k])? {
                    part.volume_numbers[k] = v;
                }
            }
        }
        Ok(())
    }

    /// An object's placement with its tracks applied, or why it is Open.
    fn placement(&self, p: &PlacementPart, sl: &[Option<usize>; 7]) -> Result<Result<PlacementPart, AnimError>, AnimError> {
        let mut p = *p;
        // : an object is given its
        // track of place only where its bit 0x800 is clear and its distance track only where
        // the bit is set; a track it is not given is passed over. An object with the bit that
        // no path holds, with a key on a track that places a held object: no result defined,
        // the implementer's choice (refused).
        let held_bit = p.first_flags & HELD_BIT != 0;
        if held_bit {
            let places = [placement_slot::PATH, placement_slot::ORIENT, placement_slot::SCALE, placement_slot::SHEAR];
            if places.iter().any(|&k| self.keyed(sl[k]).is_some()) {
                return Ok(Err(refuse_path_track()));
            }
        }
        let position = if held_bit { None } else { self.place(sl[placement_slot::POSITION])? };
        let orient = self.orientation(sl[placement_slot::ORIENT])?;
        let scale = self.triple(sl[placement_slot::SCALE])?;
        let shear = self.triple(sl[placement_slot::SHEAR])?;
        let origin = self.place(sl[placement_slot::ORIGIN])?;
        if position.is_some() || orient.is_some() || scale.is_some() || shear.is_some() {
            // (any of the four tracks of the second transform: it is rebuilt; as 3.0
            // an absent track gives the identity for its factor, the position 0; as 3.1 and 5.0
            // it leaves its part as stored)
            let stored = if self.version == RenderAs::Bryce30 { None } else { Some(p.second) };
            let turn = orient.map(quaternion_rows);
            let plain = |sc: Option<[f64; 3]>, sh: Option<[f64; 3]>| {
                product_rows(sc.unwrap_or([1.0; 3]), sh.unwrap_or([0.0; 3]), turn.unwrap_or(IDENTITY_ROWS))
            };
            let rows = match stored {
                Some(st) if orient.is_none() && scale.is_none() && shear.is_none() => st.rows,
                Some(st) => match stored_factors(&st.rows) {
                    Some((d, l, q)) => product_rows(scale.unwrap_or(d), shear.unwrap_or(l), turn.unwrap_or(q)),
                    None => plain(scale, shear),
                },
                None => plain(scale, shear),
            };
            let translation = position.unwrap_or(stored.map_or([0.0; 3], |st| st.translation));
            p.second = Transform { rows, translation };
        }
        if let Some(o) = origin {
            p.first.translation = o;
        }
        Ok(Ok(p))
    }

    /// The distance along the path a held object or camera stands at: its Obj_ePathT track's
    /// value at the opening time, `None` where the track has no key. (one single
    /// a key, read as reads any single, the time map included),
    fn distance(&self, sl: &[Option<usize>; 7]) -> Result<Option<f32>, AnimError> {
        self.single(sl[placement_slot::PATH])
    }

    /// The place q' a held object takes at distance `s` (`held_place`).
    fn path_place(&self, path: &Path, path_first: Option<Transform>, s: f32) -> Result<([f64; 3], [f64; 4]), AnimError> {
        held_place(self.version, path, path_first, s)
    }

    /// The placement of an object held to a path (not a camera), where tracks are applied: its
    /// second transform made from the path's place at its distance, the turn (its orientation
    /// track, or as it faces along the path with bit 0x400), its scales and shears; its stored
    /// one where none of its distance, orientation, scale and shear tracks has a key. The
    /// cases leaves Open are refused, saying which.
    fn held_placement(
        &self,
        p: &PlacementPart,
        sl: &[Option<usize>; 7],
        path: &Path,
        path_first: Option<Transform>,
    ) -> Result<Result<Held, AnimError>, AnimError> {
        let mut p = *p;
        let keyed = |k: usize| self.keyed(sl[k]).is_some();
        let acts = [placement_slot::PATH, placement_slot::ORIENT, placement_slot::SCALE, placement_slot::SHEAR];
        let origin_keyed = keyed(placement_slot::ORIGIN);
        let faces = p.first_flags & FACES_ALONG_BIT != 0;
        let as_30 = self.version == RenderAs::Bryce30;
        // (the stored second transform's turn, scales and shears are kept where a
        // part has no track, as 3.1 and 5.0; as 3.0 the identity, 1 and 0). A stored matrix
        // that cannot be taken apart keeps nothing (the implementer's choice).
        let stored = if as_30 { None } else { stored_factors(&p.second.rows) };
        if !acts.iter().any(|&k| keyed(k)) {
            // ; the
            // origin track as for any object
            if let Some(o) = self.place(sl[placement_slot::ORIGIN])? {
                p.first.translation = o;
            }
            if self.version != RenderAs::Bryce50 {
                // (a held object takes no track of place; it is
                // passed over)
                return Ok(Ok(Held { placement: p, box_made_again: true }));
            }
            // : placed by the path at its saved
            // distance, keeping the turn, scales and shears of the stored second transform (with
            // bit 0x400 the facing orientation in place of the turn); its box is made again only
            // where a track has a key (the origin track's too).
            let (place, facing) = match self.path_place(path, path_first, p.saved_distance) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            let rows = match (&stored, faces) {
                (Some((d, l, _)), true) => product_rows(*d, *l, quaternion_rows(facing)),
                (None, true) => quaternion_rows(facing),
                (_, false) => p.second.rows,
            };
            p.second = Transform { rows, translation: place };
            return Ok(Ok(Held { placement: p, box_made_again: origin_keyed }));
        }
        let s = self.distance(sl)?.unwrap_or(p.saved_distance);
        let (place, facing) = match self.path_place(path, path_first, s) {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        // (bit 0x400: the facing orientation whatever the orientation track gives;
        // else the track's; where there is none the identity as 3.0 and the stored second
        // transform's rotation as 3.1 and 5.0 (observed: hsc-sw1); scales 1 and shears 0 where
        // there is no track as 3.0, the stored second transform's as 3.1 and 5.0)
        let turn = if faces {
            Some(facing)
        } else {
            self.orientation(sl[placement_slot::ORIENT])?
        };
        let turn_rows = match (turn, &stored) {
            (Some(q), _) => quaternion_rows(q),
            (None, Some((_, _, q))) => *q,
            (None, None) => IDENTITY_ROWS,
        };
        let (stored_scale, stored_shear) = match &stored {
            Some((d, l, _)) => (*d, *l),
            None => ([1.0; 3], [0.0; 3]),
        };
        let scale = self.triple(sl[placement_slot::SCALE])?.unwrap_or(stored_scale);
        let shear = self.triple(sl[placement_slot::SHEAR])?.unwrap_or(stored_shear);
        p.second = Transform { rows: product_rows(scale, shear, turn_rows), translation: place };
        if let Some(o) = self.place(sl[placement_slot::ORIGIN])? {
            p.first.translation = o;
        }
        Ok(Ok(Held { placement: p, box_made_again: true }))
    }

    /// The terrains' bounds: a lower bound track (HF_eClipMin) and an upper bound track
    /// (HF_eClipMax) with keys give their value at the opening time as the bound stored.
    fn terrains(&self, s: &mut Scene) -> Result<(), AnimError> {
        for k in 0..s.terrains.len() {
            let Some(sl) = self.tracks.terrains.get(k) else { continue };
            for (b, slot) in sl.iter().enumerate() {
                if let Some(v) = self.double(*slot)? {
                    s.terrains[k].part.stored_bounds[b] = v;
                }
            }
        }
        Ok(())
    }

    /// Every object's placement tracks (cameras aside: `view`), the torus's radii, and the
    /// worlds made again (linked objects).
    fn objects(&self, s: &mut Scene) -> Result<(), AnimError> {
        let mut open: Vec<Option<AnimError>> = vec![None; s.objects.len()];
        // The worlds as the document stored them: the structure keeps listing by them an object
        // that a path places without a key (A held object with no key, as 5.0).
        let stored_worlds: Vec<Option<[Transform; 3]>> = s
            .objects
            .iter()
            .map(|o| {
                let w = VERSIONS.map(|v| o.world_as(v));
                w.iter().all(Option::is_some).then(|| w.map(Option::unwrap))
            })
            .collect();
        let mut stale = vec![false; s.objects.len()];
        for o in s.objects.iter_mut() {
            o.listing_worlds = None;
        }
        for i in 0..s.objects.len() {
            let sl = self.tracks.object(i);
            let o = &mut s.objects[i];
            if let Body::Shape(Shape::Torus { tube_radius, ring_radius }) = &mut o.body {
                if let Some(v) = self.double(sl.torus[slots::TORUS_TUBE])? {
                    *tube_radius = v;
                }
                if let Some(v) = self.double(sl.torus[slots::TORUS_RING])? {
                    *ring_radius = v;
                }
            }
            if matches!(o.body, Body::Camera(_)) {
                continue; // a camera's tracks: the view's (below), where applies them
            }
            let Some(p) = o.placement else { continue };
            // (An object held to a path: a row of a path's list with bit
            // 0x800 of its first flag word)
            let linked_to = o.linked_to;
            let held = linked_to.and_then(|parent| match s.objects[parent].body {
                Body::Path(k) if p.first_flags & HELD_BIT != 0 => Some((parent, k)),
                _ => None,
            });
            if let Some((parent, k)) = held {
                let path_first = s.objects[parent].placement.map(|q| q.first);
                match self.held_placement(&p, &sl.placement, s.paths[k].part.path_as(self.version), path_first)? {
                    Ok(h) => {
                        s.objects[i].placement = Some(h.placement);
                        stale[i] = !h.box_made_again;
                    }
                    Err(e) => open[i] = Some(e),
                }
                continue;
            }
            match self.placement(&p, &sl.placement)? {
                Ok(q) => s.objects[i].placement = Some(q),
                Err(e) => open[i] = Some(e),
            }
        }
        // The worlds again: the root list's by, the linked ones from their parents'
        // opened chains . An Open object, and every object linked below it, has none.
        for o in s.objects.iter_mut() {
            let Some(p) = &o.placement else { continue };
            if o.linked_to.is_none() {
                o.world = Some(p.world());
            } else {
                o.world = None;
                o.linked_worlds = None;
                o.world_open = None;
            }
            if let Body::Tree(t) = o.body {
                s.trees[t].placement = *p;
                s.trees[t].world = p.world();
            }
        }
        linked_worlds(&mut s.objects, &mut s.lights, &mut s.trees);
        // : the object, and what is linked below
        // it (code only), keep the box of the place stored. Parents come before their children.
        for i in 0..s.objects.len() {
            if let Some(parent) = s.objects[i].linked_to
                && stale[parent]
            {
                stale[i] = true;
            }
            if stale[i] {
                s.objects[i].listing_worlds = stored_worlds[i];
            }
        }
        for i in 0..s.objects.len() {
            let inherited = s.objects[i].linked_to.and_then(|parent| open[parent].clone());
            if let Some(e) = open[i].clone().or(inherited) {
                let o = &mut s.objects[i];
                o.world = None;
                o.linked_worlds = None;
                o.world_open = Some(e.clone());
                open[i] = Some(e);
            }
        }
        for l in s.lights.iter_mut() {
            if let Light::Radial(r) = l
                && let Some(w) = s.objects[r.object].world_as(RenderAs::Bryce30)
            {
                r.position = w.translation;
            }
        }
        Ok(())
    }

    /// The lights' and the sun's tracks; a radial light's colour made again.,
    fn lights(&self, s: &mut Scene) -> Result<(), AnimError> {
        for i in 0..s.lights.len() {
            // , where the track has a key its value at the opening time
            // replaces the stored exponent in a render: kept beside the stored exponent, which
            // the reader still reports. (Opening)
            if let Light::Radial(r) = &s.lights[i]
                && matches!(r.beam, Some(BeamRecord::Spot { .. }))
            {
                let v = self.double(self.tracks.beam_falloff.get(i).copied().flatten())?;
                if let Light::Radial(r) = &mut s.lights[i] {
                    r.falloff_opened = v;
                }
            }
            let sl = self.tracks.light(i);
            let third_flags = match &s.lights[i] {
                Light::Radial(r) => s.objects[r.object].placement.map_or(0, |p| p.third_flags),
                Light::Sun(sun) => sun.placement.third_flags,
            };
            let colour = self.colour(sl[light_slot::COLOUR])?;
            let intensity = self.double(sl[light_slot::INTENSITY])?;
            let part = match &mut s.lights[i] {
                Light::Radial(r) => &mut r.part,
                Light::Sun(sun) => &mut sun.light,
            };
            if let Some(c) = colour {
                part.colour = c;
            }
            if let Some(v) = intensity {
                part.intensity = v;
            }
            let five = &mut part.five;
            if let Some(v) = self.double(sl[light_slot::RADIUS])? {
                five.radius = crate::scene_parts::gated_radius(third_flags, v, true);
            }
            if let Some(v) = self.double(sl[light_slot::SHADOW_AMBIENCE])? {
                five.shadow_ambience = v;
            }
            if let Some(v) = self.double(sl[light_slot::RANGE])? {
                five.range = v;
            }
            if let Some(v) = self.double(sl[light_slot::GRADIENT_RANGE])? {
                five.gradient[0] = v;
            }
            if let Some(v) = self.double(sl[light_slot::GRADIENT_STEP])? {
                five.gradient[1] = v;
            }
            if let Some(c) = self.colour(sl[light_slot::SHADOW_COLOUR])? {
                five.shadow_colour = c;
            }
            let part = *part;
            match &mut s.lights[i] {
                Light::Radial(r) => {
                    r.own_colour = part.colour;
                    r.intensity = part.intensity;
                    let material = s.objects[r.object]
                        .material
                        .or_else(|| crate::scene::unmaterialled_light_material(r.beam.is_some(), &s.objects[..r.object]));
                    // , with
                    // the opened material; a gel's and no material's as the reader gives them
                    r.colour = match (material, r.gel) {
                        (Some(m), None) => {
                            let diffuse = s.materials[m].part.colours[0].a;
                            std::array::from_fn(|k| part.colour[k] * diffuse[k])
                        }
                        _ => part.colour,
                    };
                }
                Light::Sun(sun) => {
                    sun.light_colour = part.colour;
                    sun.intensity = part.intensity;
                    let sl = self.tracks.sun;
                    if let Some(c) = self.colour(sl[slots::SUN_COLOUR])? {
                        sun.colour = c;
                        sun.sun.colour = c;
                    }
                    if let Some(c) = self.colour(sl[slots::SUN_GLOW])? {
                        sun.glow_colour = c;
                        sun.sun.glow_colour = c;
                    }
                    if let Some(d) = self.triple(sl[slots::SUN_DIRECTION])? {
                        sun.direction = d;
                        sun.sun.direction = d;
                    }
                }
            }
        }
        Ok(())
    }

    /// The sky's ambient colour and ambience tracks at the opening time: s times k as 5.0 in
    /// both forms of the parts, the saved time as 3.1 and 3.0 (the time of `self.opening`).
    /// So are the horizon colour, dome colour, plain sky colour, haze, haze colour and shadow
    /// intensity tracks (observed), and the sky colour track by the fallback.
    /// , (offsets 104, 116, 128, 140, 152,
    /// 180, 192)
    fn sky(&self, s: &mut Scene) -> Result<(), AnimError> {
        let sl = self.tracks.sky;
        let colours: [(usize, &mut Rgb); 5] = [
            (slots::SKY_AMBIENT_COLOUR, &mut s.sky.ambient_colour),
            (slots::SKY_HORIZON_COLOUR, &mut s.sky.horizon_colour),
            (slots::SKY_DOME_COLOUR, &mut s.sky.dome_colour),
            (slots::SKY_PLAIN_COLOUR, &mut s.sky.plain_sky_colour),
            (slots::SKY_HAZE_COLOUR, &mut s.sky.haze_colour),
        ];
        for (k, field) in colours {
            if let Some(c) = self.colour(sl[k])? {
                *field = c;
            }
        }
        if let Some(c) = self.colour(sl[slots::SKY_COLOUR])? {
            s.sky.sky_colour = c;
        }
        let numbers: [(usize, &mut f64); 3] = [
            (slots::SKY_AMBIENCE, &mut s.sky.ambience),
            (slots::SKY_HAZE, &mut s.sky.haze),
            (slots::SKY_SHADOW_INTENSITY, &mut s.sky.shadow_intensity),
        ];
        for (k, field) in numbers {
            if let Some(v) = self.double(sl[k])? {
                *field = v;
            }
        }
        // : where the sky holds any track, the cloud offsets and the
        // edge noise's two numbers are made anew
        if s.sky.tracks.iter().any(|&t| t != 0) {
            self.wind(s)?;
        }
        Ok(())
    }

    /// The wind: the offsets become the sums of the wind's speed along its direction over the
    /// steps from the tracks' first key to the time (or (0, 0) without both tracks, or with the
    /// time before them); alpha becomes the turbulence (its track at the time, else the record's)
    /// and zeta half the number of steps (0 for none)., (offset
    /// 244 + 4 x slot, slots 40 to 42),
    fn wind(&self, s: &mut Scene) -> Result<(), AnimError> {
        let sl = self.tracks.sky_wind;
        let time = self.opening.time;
        let turbulence = self.double(sl[2])?.unwrap_or(s.sky.wind[2]);
        let (mut offset, mut zeta) = ([0.0; 2], 0.0);
        // (The wind; read 2026-10-22 in 3.0's wind routine and
        // the same compares in 3.1's): the wind runs where both wind tracks are named, with or
        // without keys; a named track with no key has the first key's time 0 and leaves the
        // sky's stored direction (or speed) as it is.
        let named = |slot: Option<usize>| slot.map(|i| &self.tracks.tracks[i]);
        if let (Some(d), Some(v)) = (named(sl[0]), named(sl[1])) {
            let as_doubles = |t: &DocTrack| match &t.values {
                other if other.key_count() == 0 => Ok(None),
                TrackValues::Double(tr) => Ok(Some(self.with_map(t, tr))),
                _ => Err(refuse_unread_track()),
            };
            let (direction, speed) = (as_doubles(d)?, as_doubles(v)?);
            // the earlier of the first keys' times (the first key as stored, a key below 0 read
            // as a key at 0,; a track with no key: 0)
            let first = |t: &Option<Track<f64>>| t.as_ref().and_then(|t| t.keys.first()).map_or(0.0, |k| k.time.max(0.0));
            let t0 = first(&direction).min(first(&speed));
            // the original's test is "not (time >= t0)": a time that is not a number takes the
            // offsets (0, 0) as a time before the tracks does
            if time >= t0 {
                let length = s.settings.animation_length as f64;
                let t_end = if time < length { time } else { length };
                let mut dir_reader = direction.as_ref().map(TrackReader::new);
                let mut speed_reader = speed.as_ref().map(TrackReader::new);
                // the pass that applies every track at the time comes first (the readers
                // remember the key it stood at)
                if let Some(r) = dir_reader.as_mut() {
                    r.value_at(time)?;
                }
                if let Some(r) = speed_reader.as_mut() {
                    r.value_at(time)?;
                }
                let (stored_direction, stored_speed) = (s.sky.wind[0], s.sky.wind[1]);
                let sums = crate::sky::wind::wind_sums(self.version, t0, t_end, |tau| {
                    let direction = match dir_reader.as_mut() {
                        Some(r) => r.value_at(tau)?.unwrap_or(0.0),
                        None => stored_direction,
                    };
                    let speed = match speed_reader.as_mut() {
                        Some(r) => r.value_at(tau)?.unwrap_or(0.0),
                        None => stored_speed,
                    };
                    Ok((direction, speed))
                })
                .map_err(|e| match e {
                    crate::sky::wind::WindError::Value(e) => e,
                    crate::sky::wind::WindError::NoEnd => AnimError::Unsupported(
                        "a sky whose wind's steps never end (the animation's length or the time is infinite, or more than 50 million steps; SK-2.2)",
                    ),
                })?;
                (offset, zeta) = (sums.offset, sums.zeta);
            }
        }
        s.sky.cloud_offset = offset;
        s.sky.wind_edge = Some([turbulence, zeta]);
        Ok(())
    }

    /// The view camera's tracks, where applies them; its transform made again for this
    /// version. Where they are Open for it, the view is Open.,
    /// points 1 and 2,
    fn view(&self, s: &mut Scene) -> Result<(), AnimError> {
        let c = s.view.camera;
        let object = s.cameras[c].object;
        if s.objects[object].linked_to.is_none() {
            return self.own_view(s);
        }
        // A camera of a linked list: its own transform as makes it (the read view holds
        // it composed with the stored chain), its tracks applied, then the parent's opened
        // chain.
        let cam = &s.cameras[c];
        let perspective = cam.name.as_slice() == b"perspective";
        let inputs = s.opening_inputs();
        s.view.transforms =
            view_transforms(&cam.placement.first, &cam.placement.second, cam.part.angles, cam.part.roll, perspective, inputs);
        self.own_view(s)?;
        s.view.transforms = linked_view(&s.objects, &s.cameras, &s.view, inputs);
        Ok(())
    }

    /// The view camera's own transform with its tracks applied.,
    fn own_view(&self, s: &mut Scene) -> Result<(), AnimError> {
        let c = s.view.camera;
        let object = s.cameras[c].object;
        let osl = self.tracks.object(object).placement;
        let csl = self.tracks.camera(c);
        // : as 3.0 the cameras named
        // director and right held to a path are held as the one named perspective.
        let name = s.cameras[c].name.clone();
        let held_by = s.objects[object].linked_to.and_then(|p| match s.objects[p].body {
            Body::Path(k) if s.cameras[c].placement.first_flags & HELD_BIT != 0 => Some((p, k)),
            _ => None,
        });
        if let Some((p, k)) = held_by
            && matches!(name.as_slice(), b"director" | b"right")
            && self.opening.rule == OpeningRule::EveryTrack
        {
            return self.held_other_view(s, c, p, k, &osl);
        }
        let has_keys = osl.iter().chain(&csl).any(|&k| self.keyed(k).is_some());
        if !has_keys {
            return Ok(());
        }
        let stand_in = match s.view.transform(self.version) {
            ViewTransform::Open(v) => v.stand_in,
            t => t.given().expect("a given transform"),
        };
        let open = ViewTransforms::all(ViewTransform::Open(OpenView { stand_in }));
        // : only the first "perspective" has tracks
        // applied; the director's field of view track as 3.0 is not applied, nor any track of
        // another view camera (observed or the fallback).
        match self.opening.view_camera_tracks_applied(&name) {
            None | Some(Ok(false)) => return Ok(()),
            Some(Err(_)) => {
                s.view.transforms = open;
                return Ok(());
            }
            Some(Ok(true)) => {}
        }
        // A camera held to a path (a row of a path's list with bit 0x800) whose distance track
        // has a key: its place from the path, its rows from its angles or from the angles
        // that look along the path; no track of orientation, scale or shear changes them.
        let parent = s.objects[object].linked_to;
        let held_path = parent.and_then(|p| match s.objects[p].body {
            Body::Path(k) if s.cameras[c].placement.first_flags & HELD_BIT != 0 => Some((p, k)),
            _ => None,
        });
        let distance = match held_path {
            Some(_) if self.keyed(osl[placement_slot::PATH]).is_some() => self.distance(&osl)?,
            _ => None,
        };
        let mut held_at = None;
        if let (Some((p, k)), Some(d)) = (held_path, distance) {
            let path_first = s.objects[p].placement.map(|q| q.first);
            let refusal = if s.cameras[c].name.as_slice() != b"perspective" {
                Some("a camera other than \"perspective\" held to a path (AN-1.9: open)")
            } else {
                None
            };
            match (refusal, self.path_place(s.paths[k].part.path_as(self.version), path_first, d)) {
                (None, Ok(at)) => held_at = Some(at),
                (Some(_), _) | (None, Err(_)) => {
                    s.view.transforms = open;
                    return Ok(());
                }
            }
        } else {
            // a distance track on a camera without bit 0x800 is passed over (revised log
            // 18295: given only where the bit is set); with the bit and no path holding it, the
            // implementer's choice: Open here.
            let held_bit = s.cameras[c].placement.first_flags & HELD_BIT != 0;
            if held_bit && self.keyed(osl[placement_slot::PATH]).is_some() {
                s.view.transforms = open;
                return Ok(());
            }
        }
        let cam = &mut s.cameras[c];
        // (the track of place is given only where bit 0x800 is
        // clear)
        let takes_place = cam.placement.first_flags & HELD_BIT == 0;
        if held_at.is_none() {
            if let Some(t) = self.place(osl[placement_slot::POSITION])?.filter(|_| takes_place) {
                cam.placement.second.translation = t;
            }
            if let Some(t) = self.place(osl[placement_slot::ORIGIN])? {
                cam.placement.first.translation = t;
            }
        }
        let part = &mut cam.part;
        if let Some(a) = self.triple(csl[slots::CAM_ANGLES])? {
            part.angles = a;
        }
        if let Some((place, facing)) = held_at {
            cam.placement.second.translation = place;
            if cam.placement.first_flags & FACES_ALONG_BIT != 0 {
                // : f the third row of the facing orientation, p = arcsin(−f_y)
                // held to −1 … 1, y = atan2(f_x, f_z); the rows of Rx(p)·Ry(y) (the path's
                // roll dropped)
                (cam.part.angles[0], cam.part.angles[1]) = facing_angles(facing);
            }
        }
        let part = &mut cam.part;
        if let Some(r) = self.double(csl[slots::CAM_ROLL])? {
            part.roll = r;
        }
        let [pan_x, pan_y] = &mut part.pan;
        let [zoom_x, zoom_y] = &mut part.zoom;
        let numbers = [
            (slots::CAM_FOV, &mut part.field_of_view),
            (slots::CAM_PAN_X, pan_x),
            (slots::CAM_PAN_Y, pan_y),
            (slots::CAM_ZOOM_X, zoom_x),
            (slots::CAM_ZOOM_Y, zoom_y),
        ];
        for (k, field) in numbers {
            if let Some(v) = self.double(csl[k])? {
                *field = v;
            }
        }
        let cam = &s.cameras[c];
        s.objects[object].placement = Some(cam.placement);
        let perspective = cam.name.as_slice() == b"perspective";
        let inputs = s.opening_inputs();
        let t = view_transforms(&cam.placement.first, &cam.placement.second, cam.part.angles, cam.part.roll, perspective, inputs)
            .get(self.version);
        s.view.transforms = ViewTransforms::all(t);
        Ok(())
    }

    /// As 3.0, the view camera named director or right held to a path (`path`, its object
    /// `p`): held as the one named perspective (the translation q', the rows of its own two
    /// angles or, with bit 0x400, of the angles that look along the path), none of its other
    /// tracks applied ('s fallback for these view cameras). The director with a target
    /// keeps the rows of the turn made as the list is read , with the translation
    /// q'; where its distance track has no key, or it has bit 0x400 (Open), the fallback: the
    /// rows of the angles that look along the path. No key and no target: the stored second
    /// transform.
    fn held_other_view(&self, s: &mut Scene, c: usize, p: usize, path: usize, osl: &[Option<usize>; 7]) -> Result<(), AnimError> {
        let object = s.cameras[c].object;
        let distance = if self.keyed(osl[placement_slot::PATH]).is_some() { self.distance(osl)? } else { None };
        let director = s.cameras[c].name.as_slice() == b"director";
        let has_target = s.tracking.iter().any(|r| r.tracker == object);
        let faces = s.cameras[c].placement.first_flags & FACES_ALONG_BIT != 0;
        let path_first = s.objects[p].placement.map(|q| q.first);
        let stand_in = match s.view.transform(self.version) {
            ViewTransform::Open(v) => v.stand_in,
            t => t.given().expect("a given transform"),
        };
        let at = |d: f32| held_place(self.version, s.paths[path].part.path_as(self.version), path_first, d);
        let cam_saved = s.cameras[c].placement.saved_distance;
        // (translation, angles for the rows; None keeps the rows)
        let made: Option<(Option<[f64; 3]>, Option<(f64, f64)>)> = if director && has_target {
            if distance.is_some() && !faces {
                Some((Some(at(distance.unwrap_or(cam_saved))?.0), None))
            } else {
                // Open : the fallback, the rows of the angles that look along the path.
                let (place, facing) = match at(distance.unwrap_or(cam_saved)) {
                    Ok(v) => v,
                    Err(_) => {
                        s.view.transforms = ViewTransforms::all(ViewTransform::Open(OpenView { stand_in }));
                        return Ok(());
                    }
                };
                Some((distance.map(|_| place), Some(facing_angles(facing))))
            }
        } else {
            match distance {
                None => None,
                Some(d) => {
                    let (place, facing) = at(d)?;
                    let cam = &s.cameras[c];
                    let angles = if faces { facing_angles(facing) } else { (cam.part.angles[0], cam.part.angles[1]) };
                    Some((Some(place), Some(angles)))
                }
            }
        };
        let Some((place, angles)) = made else { return Ok(()) };
        let version = self.version;
        let cam = &mut s.cameras[c];
        if let Some(t) = place {
            cam.placement.second.translation = t;
        }
        if let Some((pitch, yaw)) = angles {
            (cam.part.angles[0], cam.part.angles[1]) = (pitch, yaw);
            cam.placement.second.rows = rows_from_angles(version, pitch, yaw);
        }
        let cam = &s.cameras[c];
        s.objects[object].placement = Some(cam.placement);
        let inputs = s.opening_inputs();
        let t = view_transforms(&cam.placement.first, &cam.placement.second, cam.part.angles, cam.part.roll, false, inputs).get(version);
        s.view.transforms = ViewTransforms::all(t);
        Ok(())
    }
}

/// The place q' an object or camera held to a path takes at distance `s`, and the facing
/// orientation there:'s place carried by the path's first transform, q' = q·M + t.
pub(crate) fn held_place(version: RenderAs, path: &Path, path_first: Option<Transform>, s: f32) -> Result<([f64; 3], [f64; 4]), AnimError> {
    let first = path_first.ok_or(AnimError::Unsupported("a path whose placement is not read (SCN-21)"))?;
    let at = path.sample(version, s);
    let (q, m, t) = (at.place, first.rows, first.translation);
    // (q' = q.M + t, each number as forms the translation: the three
    // products added in the order of the inner index, the translation added last)
    let moved = |c: usize| ((q[0] * m[0][c] + q[1] * m[1][c]) + q[2] * m[2][c]) + t[c];
    Ok(([moved(0), moved(1), moved(2)], at.facing))
}

/// The two angles that look along a path, from its facing orientation: f its third row,
/// p = arcsin(−f_y) held to −1 … 1, y = atan2(f_x, f_z) (the path's roll dropped).
pub(crate) fn facing_angles(facing: [f64; 4]) -> (f64, f64) {
    let f = second_rows([1.0; 3], [0.0; 3], facing)[2];
    ((-f[1]).clamp(-1.0, 1.0).asin(), f[0].atan2(f[2]))
}

/// diag(sc) · L · R(q): the second transform's matrix made from the tracks, with L the lower
/// triangular shear (rows (1, 0, 0), (a, 1, 0), (b, c, 1)) and R(q) the quaternion's rows as
/// writes them.
fn second_rows(sc: [f64; 3], sh: [f64; 3], q: [f64; 4]) -> [[f64; 3]; 3] {
    product_rows(sc, sh, quaternion_rows(q))
}

const IDENTITY_ROWS: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// R(q): the quaternion's rows as writes them.
fn quaternion_rows(q: [f64; 4]) -> [[f64; 3]; 3] {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ]
}

/// A stored matrix written D . L . Q by Gram-Schmidt on its rows in order (Q of determinant
/// +1, D's last sign following), as writes a camera's first transform: the scales, the
/// shears (a, b, c) and Q. `None` where a row has length 0 or a number is not finite.
/// (3.1 and 5.0: an absent track leaves its factor as stored; the last bits of a
/// stored matrix that is not exact: Open, this fallback),
fn stored_factors(m: &[[f64; 3]; 3]) -> Option<([f64; 3], [f64; 3], [[f64; 3]; 3])> {
    let dot = |a: [f64; 3], b: [f64; 3]| (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2];
    let d0 = dot(m[0], m[0]).sqrt();
    let q0 = m[0].map(|v| v / d0);
    let a10 = dot(m[1], q0);
    let r1: [f64; 3] = std::array::from_fn(|k| m[1][k] - a10 * q0[k]);
    let d1 = dot(r1, r1).sqrt();
    let q1 = r1.map(|v| v / d1);
    let a20 = dot(m[2], q0);
    let a21 = dot(m[2], q1);
    let r2: [f64; 3] = std::array::from_fn(|k| m[2][k] - a20 * q0[k] - a21 * q1[k]);
    let mut d2 = dot(r2, r2).sqrt();
    if ![d0, d1, d2].iter().all(|d| d.is_finite() && *d > 0.0) {
        return None;
    }
    let q2 = r2.map(|v| v / d2);
    let det = q0[0] * (q1[1] * q2[2] - q1[2] * q2[1]) - q0[1] * (q1[0] * q2[2] - q1[2] * q2[0]) + q0[2] * (q1[0] * q2[1] - q1[1] * q2[0]);
    let q2 = if det < 0.0 {
        d2 = -d2;
        q2.map(|v| -v)
    } else {
        q2
    };
    Some(([d0, d1, d2], [a10 / d1, a20 / d2, a21 / d2], [q0, q1, q2]))
}

/// diag(sc) . L . R: the product in the order writes.
fn product_rows(sc: [f64; 3], sh: [f64; 3], r: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let [a, b, c] = sh;
    let l = [[1.0, 0.0, 0.0], [a, 1.0, 0.0], [b, c, 1.0]];
    let m: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| sc[i] * l[i][j]));
    std::array::from_fn(|i| std::array::from_fn(|j| (m[i][0] * r[0][j] + m[i][1] * r[1][j]) + m[i][2] * r[2][j]))
}

