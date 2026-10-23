// SPDX-License-Identifier: MIT
//! The media a ray's path is in, and the rest of a path's state that the surface rules carry
//! from a ray to the rays it sends: the solids the path holds and the meetings that decide the
//! medium beyond each, the count of total internal reflections (as 5.0) and the number of rays
//! the path has gone through.
//!
//! (The media, corrected at by : "A path holds a solid from
//! the moment a ray of it is sent through the solid's surface until a ray of the path is next
//! sent through that surface"; the medium beyond a held solid; The transmitted ray, step 4: the
//! count; The blended material: the rays the path has gone through), (The media, revised
//! : the shadow ray's stretches as rays of the path), point 4 (a turned ray's
//! medium and holds unchanged),;
//! revised at (where the look-back reaches a solid the path no
//! longer holds and no ray decides it, "the path holds no solid but S" is read for S, the solid
//! being left: `PathMedia::beyond`)
//! (Media along a path; The held solids and the meetings),
//! sky.md (The air over a tree of rays)
//! (the medium of a volume material's solid, F = 1; the medium
//! colour Φ'); (Volume materials: in the render)

use crate::geom::Vec3;
use crate::geometry::{Placement, Shape};
use crate::scene_parts::{MATERIAL_VOLUME_BITS, MaterialPart};
use crate::shading::OpenItem;
use crate::shading::media::MediumLaw;
use crate::shading::volume::{Extent, MediumHit, MediumTint, bits};
use crate::sky::air::AirPath;
use crate::version::RenderAs;

/// The flag bits of the medium colour Φ': 0x4000 and 0x8000.
pub const MEDIUM_TINT_BITS: u32 = bits::BLEND_DISTANCE | bits::BLEND_ALTITUDE;

/// The Light mode's bit 0x4 [format]; beside a volume bit it only makes the refraction value 1.
const LIGHT_MODE_BIT: u32 = 0x4;

/// A shape's own extent in y for bit 0x8000: −1 to 1 for the sphere, the cylinder, the cone and
/// the slab, −0.5 to 0.5 for the cube and the pyramid; the torus (and every other shape):
/// Open.,
pub fn extent(shape: &Shape) -> Extent {
    match &shape.plain() {
        Shape::Sphere | Shape::Cylinder | Shape::Cone | Shape::Slab => Extent::Y(-1.0, 1.0),
        Shape::Cube | Shape::Pyramid => Extent::Y(-0.5, 0.5),
        // : the torus from minus its tube
        // radius to its tube radius
        Shape::Torus { tube, .. } => Extent::Y(-*tube, *tube),
        _ => Extent::Open,
    }
}

/// The medium of a solid of material `part`: its law (Φ, or white for the flag word's bit 8;
/// η, whose factor F is 1 for a volume material, the value itself serving the ratio r) and, where the flag word has 0x4000 or
/// 0x8000 and not bit 8, the medium colour Φ' in place of Φ. (What a ray brings
/// back: the blended material's Φ white), (The weight a ray comes back with; The medium
/// colour)
pub fn solid_medium(part: &MaterialPart, shape: &Shape) -> (MediumLaw, Option<MediumTint>) {
    let white = part.flags & bits::WHITE_MEDIUM != 0;
    let volume = part.flags & MATERIAL_VOLUME_BITS != 0;
    // : F is 1 for a volume solid whatever its value (the law that does not
    // fade, as a fuzzy solid's), while r = η1 / η2 takes the material's own refraction value.
    let law = if volume {
        // : a volume material whose flag word also has
        // bit 0x4 has its refraction value count as 1 in the ratio as well as in the factor F
        let eta = if part.flags & LIGHT_MODE_BIT != 0 { 1.0 } else { part.refraction };
        MediumLaw::of_fuzzy_solid(part.volume_colour, eta, white)
    } else {
        MediumLaw::of_solid(part.volume_colour, part.refraction, white)
    };
    let tint = if white { None } else { MediumTint::of(part.flags, extent(shape)) };
    (law, tint)
}

/// The law with Φ' for a ray of the medium of the solid placed at `placement` that met `hit`
/// (its parameter t and point) or nothing.
pub fn tinted(
    version: RenderAs,
    law: MediumLaw,
    tint: Option<MediumTint>,
    placement: &Placement,
    hit: Option<(f64, Vec3)>,
) -> Result<MediumLaw, OpenItem> {
    let Some(tint) = tint else { return Ok(law) };
    let space = tint.group.map_or(*placement, |g| g.placement);
    let hit = hit.map(|(t, p)| MediumHit { t, own_y: space.point_to_local(p)[1] });
    Ok(MediumLaw { volume_colour: tint.colour(version, law.volume_colour, hit)?, ..law })
}

/// Bit 0x1000 of a group's first flag word: its own origin is part of the box the medium colour
/// takes its extent in y from.
const GROUP_OWN_ORIGIN: u32 = 0x1000;

/// The space bit 0x8000 of a member's material reads in, inside the combined group that is
/// geometry object `group` of `plan`: the group's map and its own extent in y (the box of its
/// members' boxes, with its origin under bit 0x1000, widened by version). `None` where the
/// object is not a combined group or has no such box. (The medium colour, inside a
/// combined group)
pub fn group_space(
    scene: &crate::scene::Scene,
    plan_geometry: &super::plan::GeometryBuild,
    group: usize,
    version: RenderAs,
) -> Option<crate::shading::volume::GroupSpace> {
    let Shape::Group(k) = plan_geometry.scene.objects[group].shape else { return None };
    let object = &scene.objects[plan_geometry.scene_object[group]];
    let world = object.world_as(version)?;
    let placement = Placement::new(world.rows, world.translation)?;
    let origin = object.placement.is_some_and(|p| p.first_flags & GROUP_OWN_ORIGIN != 0);
    let (y0, y1) = plan_geometry.scene.medium_extent_y(k, &placement, origin, version)?;
    Some(crate::shading::volume::GroupSpace { placement, y0, y1 })
}

/// What the media leave Open or the design guards, as refusals.
pub mod open {
    use crate::shading::OpenItem;

    /// (a path that holds more than `HELD_MAX` solids at once)
    pub const DEEP: OpenItem = OpenItem {
        entry: "SH-2.2",
        subject: "a path that holds more than four solids at once (design guard of the path's media)",
        question: "",
    };
    /// (a look-back past the meetings the path's media keep)
    pub const LOST: OpenItem = OpenItem {
        entry: "SH-2.2",
        subject: "the medium beyond a held solid decided by a meeting older than the sixteen the path keeps (design guard)",
        question: "",
    };
    /// (Open: the medium beyond a held solid along a shadow ray where no ray of the path
    /// decides it and the path holds another solid as well)
    pub const UNDECIDED: OpenItem = OpenItem {
        entry: "SH-2.2",
        subject: "the medium beyond a held solid where no ray of the path decides it and the path holds another solid as well",
        question: "",
    };
}

/// How many solids a path may hold at once. (a fixed set, so the state stays
/// plain `Copy` data in the batches)
pub const HELD_MAX: usize = 4;
/// How many meetings a path keeps. (The held solids and the meetings)
pub const MEETINGS_MAX: usize = 16;

/// How many terrains that enclose no volume a path can count crossings of (one parity bit
/// each; a terrain beyond the first of them is never held, a design guard).
/// (A terrain that encloses no volume: held by its crossings)
pub const TERRAINS_MAX: usize = 128;

/// No solid: outside every object (a stored medium).
const NONE: u32 = u32::MAX;

/// A ray of the path met a surface of `solid` while travelling in `medium` (`NONE`: outside
/// every object), a medium other than `solid`'s own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct Meeting {
    solid: u32,
    medium: u32,
}

/// The media of a path: the medium the ray travels in, the solids the path holds, and the
/// meetings of the path's rays with solids' surfaces from other media, oldest first.
/// (The held solids and the meetings)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathMedia {
    current: u32,
    held: [u32; HELD_MAX],
    /// Per held solid: the object whose material is its medium (the solid itself, or for a
    /// group the member that the hit which entered it named).
    held_material: [u32; HELD_MAX],
    held_len: u8,
    meetings: [Meeting; MEETINGS_MAX],
    meetings_len: u8,
    /// Whether older meetings were dropped (design guard).
    lost: bool,
    /// Per terrain that encloses no volume (by its slot): whether the path has sent an odd
    /// number of its rays through the terrain's surface, that is, holds it. (The
    /// media), (A terrain that encloses no volume)
    terrains_held: u128,
}

impl Default for PathMedia {
    fn default() -> Self {
        PathMedia::OUTSIDE
    }
}

impl PathMedia {
    /// Outside every object, holding nothing, no meeting.
    pub const OUTSIDE: PathMedia = PathMedia {
        current: NONE,
        held: [NONE; HELD_MAX],
        held_material: [NONE; HELD_MAX],
        held_len: 0,
        meetings: [Meeting { solid: NONE, medium: NONE }; MEETINGS_MAX],
        meetings_len: 0,
        lost: false,
        terrains_held: 0,
    };

    /// In the medium of the solid that holds the camera's eye, which the path holds; no ray
    /// has met its surface. (The medium a camera ray starts in; The media: the
    /// solids that hold the eye at the start are held)
    pub fn holding(solid: usize) -> PathMedia {
        let mut s = PathMedia::OUTSIDE;
        s.current = solid as u32;
        s.held[0] = solid as u32;
        s.held_material[0] = solid as u32;
        s.held_len = 1;
        s
    }

    /// Holding each (solid, object whose material is its medium) of `held`, in that order, in the
    /// medium of the last; no ray has met their surfaces. `None` where they are more than
    /// `HELD_MAX`. (The medium a camera ray starts in: every holder left is held by
    /// the path, the deciding one last)
    pub fn holding_all(held: &[(usize, usize)]) -> Option<PathMedia> {
        if held.len() > HELD_MAX {
            return None;
        }
        let mut s = PathMedia::OUTSIDE;
        for (k, &(solid, material)) in held.iter().enumerate() {
            s.held[k] = solid as u32;
            s.held_material[k] = material as u32;
            s.current = solid as u32;
        }
        s.held_len = held.len() as u8;
        Some(s)
    }

    /// The solid whose medium the ray travels in, `None` outside every object.
    pub fn current(&self) -> Option<usize> {
        (self.current != NONE).then_some(self.current as usize)
    }

    /// The object whose material is the medium the ray travels in, `None` outside every object:
    /// the solid itself, or for a group the member the entering hit named.
    pub fn current_material(&self) -> Option<usize> {
        if self.current == NONE {
            return None;
        }
        let at = self.held[..self.held_len as usize].iter().position(|&h| h == self.current);
        Some(at.map_or(self.current, |k| self.held_material[k]) as usize)
    }

    /// Whether the path holds the terrain of slot `slot` that encloses no volume: an odd number
    /// of its rays have gone through that terrain's surface, so its next crossing is a second,
    /// fourth,... one and r = 1 there.,
    pub fn holds_terrain(&self, slot: usize) -> bool {
        slot < TERRAINS_MAX && (self.terrains_held >> slot) & 1 == 1
    }

    /// The path after a ray of it is sent through the surface of the terrain of slot `slot`
    /// (it encloses no volume): the crossing counted for that terrain alone, whatever other
    /// shapes were crossed; a reflected ray is sent with the path as it was (the caller).
    pub fn crossed_terrain(&self, slot: usize) -> PathMedia {
        let mut s = *self;
        if slot < TERRAINS_MAX {
            s.terrains_held ^= 1u128 << slot;
        }
        s
    }

    /// Whether the path holds `solid`.
    pub fn holds(&self, solid: usize) -> bool {
        self.held[..self.held_len as usize].contains(&(solid as u32))
    }

    /// A ray of the path, travelling in the current medium, meets a surface of `solid` (a
    /// shape that encloses a volume): the meeting is kept where that medium is not `solid`'s
    /// own. Nothing the path holds changes. (The media: "of those that met a
    /// surface of S while travelling in a medium other than S's own")
    pub fn met(&self, solid: usize) -> PathMedia {
        let solid = solid as u32;
        let mut s = *self;
        if s.current == solid {
            return s;
        }
        if s.meetings_len as usize == MEETINGS_MAX {
            // (the oldest meeting dropped; a look-back past it refuses)
            s.meetings.copy_within(1.., 0);
            s.meetings_len -= 1;
            s.lost = true;
        }
        s.meetings[s.meetings_len as usize] = Meeting { solid, medium: s.current };
        s.meetings_len += 1;
        s
    }

    /// The media of a ray sent through a surface of `solid` (a shape that encloses a volume),
    /// after the meeting (`met`): entering it where the path does not hold it (its medium,
    /// held from now on); otherwise the path no longer holds it, and the ray travels in the
    /// medium beyond it.,
    pub fn through(&self, solid: usize) -> Result<PathMedia, OpenItem> {
        self.through_as(solid, solid)
    }

    /// `through` for a solid whose medium has the material of object `material` (a group:
    /// the member the hit named, point 3; any other solid: itself).
    pub fn through_as(&self, solid: usize, material: usize) -> Result<PathMedia, OpenItem> {
        let id = solid as u32;
        let mut s = *self;
        let held = &s.held[..s.held_len as usize];
        match held.iter().position(|&h| h == id) {
            None => {
                if s.held_len as usize == HELD_MAX {
                    return Err(open::DEEP);
                }
                s.held[s.held_len as usize] = id;
                s.held_material[s.held_len as usize] = material as u32;
                s.held_len += 1;
                s.current = id;
            }
            Some(at) => {
                // The medium beyond is found while the path still holds the solid (the holds
                // before this ray is sent; decide it
                // render.md), then the solid is released.
                let beyond = s.beyond(id)?;
                s.held.copy_within(at + 1..s.held_len as usize, at);
                s.held_material.copy_within(at + 1..s.held_len as usize, at);
                s.held_len -= 1;
                s.held[s.held_len as usize] = NONE; // unused slots stay NONE, so equal paths compare equal
                s.held_material[s.held_len as usize] = NONE;
                s.current = beyond;
            }
        }
        Ok(s)
    }

    /// The medium beyond solid `left`, which the path holds, as the meetings decide it: the
    /// latest meeting of the solid from another medium; outside every object, a solid the
    /// path holds, or the medium beyond a solid it no longer holds, decided by the meetings
    /// before that one in the same way. Where none decides: outside if the path holds no
    /// solid but `left`, else the second-to-last held solid.
    fn beyond(&self, left: u32) -> Result<u32, OpenItem> {
        let (mut solid, mut before) = (left, self.meetings_len as usize);
        loop {
            let found = self.meetings[..before].iter().rposition(|m| m.solid == solid);
            let Some(j) = found else {
                if self.lost {
                    return Err(open::LOST);
                }
                // : where none decides, outside
                // every object if the path holds no solid but `left`; else the solid held just
                // before the one the path most recently came to hold: the second-to-last entry of
                // the list of held solids, in the order the path came to hold them
                let held = &self.held[..self.held_len as usize];
                // revised (beam-light-pictures): a light's own object is asked twice as 3.0 and
                // 3.1 (Which solids hold the eye), so the path may hold one solid twice:
                // it holds a solid besides the one it leaves where it holds two entries
                return if held.len() >= 2 {
                    Ok(held[held.len() - 2])
                } else {
                    Ok(NONE)
                };
            };
            let medium = self.meetings[j].medium;
            if medium == NONE || self.holds(medium as usize) {
                return Ok(medium);
            }
            (solid, before) = (medium, j);
        }
    }
}

/// A path's state, carried from a ray to the rays it sends. (Inputs: the medium
/// it travels in; as 5.0 its count of total internal reflections; the rays the path has gone
/// through, for the blended ray as 5.0)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathState {
    pub media: PathMedia,
    /// As 5.0: the count of total internal reflections in a row (0 for a camera ray).
    pub tir_count: u32,
    /// The rays of the path up to and including this one (1 for a camera ray).
    /// (the count the blended ray's limit of 100 reads as 5.0)
    pub rays: u32,
    /// The path the ray's air is measured along: its own from its origin, or the path of a
    /// ray that met a blended surface.
    /// (The air over a tree of rays)
    pub air: AirPath,
    /// What the object standing for every set of metaballs showed when this ray's own hit was
    /// found (the entry of the latest recorded hit then, that hit counted), `None` where no
    /// set was hit before. The weight of a ray that travels in that object reads its material
    /// as it was then.
    pub set_showing: Option<u32>,
}

impl PathState {
    /// A camera ray's state outside every object.
    pub const CAMERA: PathState = PathState { media: PathMedia::OUTSIDE, tir_count: 0, rays: 1, air: AirPath::Own, set_showing: None };
}

impl Default for PathState {
    fn default() -> Self {
        PathState::CAMERA
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ray sent through a surface: the meeting, then the far side.
    fn cross(m: PathMedia, solid: usize) -> PathMedia {
        m.met(solid).through(solid).unwrap()
    }

    /// : entering a solid, its medium; leaving it, the medium beyond, which
    /// for solids one inside another is the one the path was in where it entered.
    #[test]
    fn enter_and_leave_nested() {
        let out = PathMedia::OUTSIDE;
        assert_eq!(out.current(), None);
        let a = cross(out, 3);
        assert_eq!(a.current(), Some(3));
        assert!(a.holds(3));
        let ab = cross(a, 5);
        assert_eq!(ab.current(), Some(5));
        let a2 = cross(ab, 5);
        assert_eq!(a2.current(), Some(3));
        assert!(!a2.holds(5));
        assert_eq!(cross(a2, 3).current(), None);
    }

    /// question 1, first bullet: a ray travelling in B's medium meets A's surface (A
    /// held): it stays in B's medium and A is no longer held.
    #[test]
    fn leaving_a_solid_while_in_another_stays() {
        let ab = cross(cross(PathMedia::OUTSIDE, 1), 2);
        let b = cross(ab, 1);
        assert_eq!(b.current(), Some(2));
        assert!(!b.holds(1) && b.holds(2));
        // Then leaving B: the latest meeting of B from another medium was in A's medium, and
        // A is no longer held: the medium beyond A, decided by the rays before: outside.
        assert_eq!(cross(b, 2).current(), None);
    }

    /// question 1, second bullet: leaving A in A's own medium after the path left the
    /// solid it was in before A: back past it to the medium before that solid's entry.
    #[test]
    fn leaving_goes_back_past_a_solid_left() {
        // Enter C, enter A (from C's medium, overlapping), leave C while in A (stays in A).
        let c = cross(PathMedia::OUTSIDE, 7);
        let ca = cross(c, 4);
        let a = cross(ca, 7);
        assert_eq!(a.current(), Some(4));
        // Leave A: met from C's medium, C no longer held: the medium beyond C: outside.
        assert_eq!(cross(a, 4).current(), None);
    }

    /// A reflected ray changes nothing a path holds; the meeting is kept.
    #[test]
    fn a_reflection_holds_nothing_new() {
        let m = PathMedia::OUTSIDE.met(2);
        assert_eq!(m.current(), None);
        assert!(!m.holds(2));
    }

    /// the eye in T, the path enters S from T's medium, leaves T while in S's medium
    /// then leaves S: the look-back reaches T, which no earlier ray met from another medium;
    /// the path holds no solid but S, so the medium beyond S is outside every object. With
    /// another solid held as well: Open.
    #[test]
    fn undecided_after_a_solid_left() {
        let t = PathMedia::holding(1);
        let ts = cross(t, 2); // enters S from T's medium
        let s = cross(ts, 1); // leaves T while in S's medium: stays in S
        assert_eq!(s.current(), Some(2));
        assert!(s.holds(2) && !s.holds(1));
        assert_eq!(cross(s, 2).current(), None);
        // with another solid held as well (S held, then 6 taken up after it), the
        // medium beyond S is S itself: the second-to-last of the held solids.
        let mut other = s;
        other.held[1] = 6;
        other.held_len = 2;
        assert_eq!(other.met(2).through(2).unwrap().current(), Some(2));
    }

    ///, the eye in a solid: no ray decides; outside where nothing else is held
    /// Open where another solid is.
    #[test]
    fn undecided() {
        let eye = PathMedia::holding(2);
        assert_eq!(eye.met(2).through(2).unwrap().current(), None);
        let eye_b = cross(eye, 6);
        // Leaving the eye's solid while in 6's medium: the meeting from 6's medium decides.
        assert_eq!(cross(eye_b, 2).current(), Some(6));
        // Leaving the eye's solid from its own medium while 6 is held as well, taken up after it:
        // the medium beyond is the solid held before the latest, which is 2 itself.
        let other = PathMedia::holding(2);
        let mut both = other;
        both.held[1] = 6;
        both.held_len = 2;
        assert_eq!(both.met(2).through(2).unwrap().current(), Some(2));
        // Where the solid left is the latest taken up, the one held before it.
        let mut before = PathMedia::holding(6);
        before.held[1] = 2;
        before.held_len = 2;
        before.current = 2;
        assert_eq!(before.met(2).through(2).unwrap().current(), Some(6));
    }

    /// The holds are read before the solid left is released, so the look-back can return it:
    /// the path then travels in its medium without holding it (
    /// decide it).
    #[test]
    fn a_look_back_can_return_the_solid_left() {
        let s = cross(PathMedia::OUTSIDE, 1);
        let st = cross(s, 2);
        let t = cross(st, 1); // in 2's medium: stays there
        assert_eq!(t.current(), Some(2));
        let ts = cross(t, 1); // enters 1 again from 2's medium
        let s2 = cross(ts, 2); // leaves 2 while in 1's medium: stays in 1
        assert_eq!(s2.current(), Some(1));
        let left = cross(s2, 1);
        assert_eq!(left.current(), Some(1));
        assert!(!left.holds(1) && !left.holds(2));
    }

    /// (the oldest meeting dropped; a look-back past it refuses)
    #[test]
    fn meetings_guard() {
        let mut m = cross(PathMedia::OUTSIDE, 1);
        for _ in 0..MEETINGS_MAX {
            m = m.met(9);
        }
        assert_eq!(m.met(1).through(1), Err(open::LOST));
    }
}
