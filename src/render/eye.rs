// SPDX-License-Identifier: MIT
//! A camera eye inside solids: which solids hold the eye, which of them decides each camera
//! ray's medium, the media the ray's path starts in, and the product of a camera sample's colour
//! with the weight the deciding solid's medium gives the camera ray.
//!
//! (The medium a camera ray starts in: Which solids hold the eye
//! (strict tests, the pyramid, the lights' own objects, combining groups by their members' roles,
//! the material not read); Which solid decides (each ray: a lone holder that is not a group
//! whatever the ray; otherwise the last crossing behind the ray's origin of each holder's list,
//! holders without one left out as 3.1 and 5.0, ordered by distance, the last deciding, every
//! holder left held); The colour; Scope: plain and textured materials), point 3 (a
//! group's member as the medium's material), (a terrain
//! as 5.0),, (a
//! metaball holds the eye within 1/2 of its centre)
//! (a camera ray that starts in a volume material's solid; the medium colour Φ' of
//! bits 0x4000 and 0x8000, inside a combined group the group's space)
//! (A camera eye inside a solid),
//! (Revision of: the eye's holders)
use std::collections::BTreeMap;

use super::media::{MEDIUM_TINT_BITS, PathMedia, group_space, solid_medium, tinted};
use super::plan::RenderPlan;
use crate::geom::Vec3;
use crate::geometry::group::{Role, Source};
use crate::geometry::light::LightShape;
use crate::geometry::{Interior, Range, Ray, Shape, ShapeContext};
use crate::sampling::batch::CameraRay;
use crate::scene::Scene;
use crate::scene_parts::{MATERIAL_CURRENT, MATERIAL_FUZZY};

/// The blended material's bit beside a fuzzy one's.,
const FUZZY_BLENDED_BIT: u32 = 0x8;
use crate::shading::OpenItem;
use crate::shading::media::MediumLaw;
use crate::shading::volume::{MediumTint, VolumeKind};
use crate::version::RenderAs;

/// What leaves Open about the eye, and the guards of this design, as refusals.
pub mod open {
    use crate::shading::OpenItem;

    const fn item(subject: &'static str) -> OpenItem {
        OpenItem { entry: "SH-2.2", subject, question: "" }
    }

    pub const UNDECIDED: OpenItem =
        item("whether a solid holds the camera eye (a torus with r or R not finite, or an eye not finite)");
    pub const HOLDER_MATERIAL: OpenItem = item(
        "a camera eye held by a solid whose material is neither plain (flag word 0x10000 with 0x4000 and 0x8000 only, textures or not) nor a volume material on a shape, or has none",
    );
    /// : as 3.0 the program has no rule for a holder whose list has
    /// no crossing behind the ray's origin.
    pub const NO_CROSSING_BEHIND: OpenItem =
        item("as 3.0, a holder of the eye whose list has no crossing behind the camera ray's origin");
    /// a metaball that holds the eye  makes the object standing for every set
    /// a holder whose material is the latest showing, which no render has before its first ray.
    pub const METABALL: OpenItem = OpenItem {
        entry: "GEO-21.6",
        subject: "a camera eye within 1/2 of a metaball's centre (the set's material before any hit)",
        question: "",
    };
    /// (a path holds at most `HELD_MAX` solids)
    pub const MANY: OpenItem = OpenItem {
        entry: "SH-2.2",
        subject: "a camera eye held by more solids than a path holds (design guard)",
        question: "",
    };
}

/// One solid that holds the eye: a geometry object, asked in this order. (Which
/// solids hold the eye)
#[derive(Clone, Copy, Debug, PartialEq)]
struct Holder {
    object: usize,
    /// A combining group's combination index.
    group: Option<usize>,
}

/// The medium of a material object (a holder, or a group's member): its law and medium colour.
type Medium = Result<(MediumLaw, Option<MediumTint>), OpenItem>;

/// The eye rule of a render, fixed before the first ray: the holders, and the media their
/// materials give. (A camera eye inside a solid)
#[derive(Clone, Debug)]
pub struct EyeMedium {
    holders: Vec<Holder>,
    media: BTreeMap<usize, Medium>,
    version: RenderAs,
    ctx: ShapeContext,
    plan_geometry: crate::geometry::Scene,
}

/// The decision for one camera ray: the solids its path holds (with the object whose material is
/// each one's medium), in order, the deciding one last.
#[derive(Clone, Debug, PartialEq)]
pub struct EyeDecision {
    pub held: Vec<(usize, usize)>,
}

impl EyeDecision {
    /// The solid that decides and the object whose material is its medium.
    pub fn deciding(&self) -> Option<(usize, usize)> {
        self.held.last().copied()
    }
}

/// Whether `p`, in a light's own space, lies in its endless solid (the lower limit dropped:
/// the cone and the cylinder below y = −1, the cube and the pyramid below y = −0.5), strictly.
fn in_endless(shape: LightShape, p: Vec3) -> Option<bool> {
    if !p.iter().all(|c| c.is_finite()) {
        return None;
    }
    let [x, y, z] = p;
    let xz = x * x + z * z;
    Some(match shape {
        LightShape::Sphere => xz + y * y < 1.0,
        LightShape::Cone => {
            let r = (1.0 - y) / 2.0;
            y < 1.0 && xz < r * r
        }
        LightShape::Cylinder => y < 1.0 && xz < 1.0,
        LightShape::Cube => y < 0.5 && x.abs() < 0.5 && z.abs() < 0.5,
        LightShape::Pyramid => {
            let half = (0.5 - y) / 2.0;
            y < 0.5 && x.abs() < half && z.abs() < half
        }
    })
}

impl EyeMedium {
    /// The solids that hold the plan's eye, in the order asked, and their media.
    pub fn new(scene: &Scene, plan: &RenderPlan) -> Result<EyeMedium, OpenItem> {
        let eye = plan.camera.eye();
        let g = &plan.geometry;
        let version = plan.version;
        let five = version == RenderAs::Bryce50;
        let mut holders = Vec::new();
        for (i, o) in g.scene.objects.iter().enumerate() {
            // : a member that takes part is asked through its group.
            if g.scene.groups.is_through(i) {
                continue;
            }
            let held = match o.shape {
                Shape::Group(k) => Some(Self::group_holds(&g.scene, k, eye, five)?).filter(|&h| h).map(|_| Some(k)),
                // : as 5.0 by its own test, as 3.1 by the test with
                // the bounds -0.00001 n and 1.00001 n (`Relief::holds_eye`), as 3.0 no terrain
                // holds the eye (our own choice where the scene gives nothing)
                Shape::Terrain(_) if version == RenderAs::Bryce30 => None,
                // : a metaball holds the eye within 1/2 of its centre
                Shape::Metaball(_) => {
                    let p = o.placement.point_to_local(eye);
                    if (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() < 0.5 {
                        return Err(open::METABALL);
                    }
                    None
                }
                _ => Self::object_holds(&g.scene, i, eye)?.then_some(None),
            };
            let Some(group) = held else { continue };
            holders.push(Holder { object: i, group });
            // : as 3.0 and 3.1 a light's own object is asked once among the objects
            // and once among the lights: two holders, the same object.
            if (g.light[i] || g.unlisted.contains(&i)) && !five {
                holders.push(Holder { object: i, group });
            }
        }
        let mut media = BTreeMap::new();
        for h in &holders {
            let mut objects = vec![h.object];
            if let Some(k) = h.group {
                Self::members_of(&g.scene, k, &mut objects);
            }
            for m in objects {
                media.entry(m).or_insert_with(|| Self::medium_of(scene, plan, h, m));
            }
        }
        Ok(EyeMedium { holders, media, version, ctx: plan.shape_context, plan_geometry: g.scene.clone() })
    }

    /// Whether geometry object `i` (not a group) holds `eye`, strictly: a point on the surface
    /// is outside; a light's own object with the endless option without its lower limit.
    fn object_holds(scene: &crate::geometry::Scene, i: usize, eye: Vec3) -> Result<bool, OpenItem> {
        let o = &scene.objects[i];
        if let Shape::Light(l) = o.shape
            && l.endless()
        {
            return in_endless(l.shape, o.placement.point_to_local(eye)).ok_or(open::UNDECIDED);
        }
        Ok(scene.interior_of(i, eye).ok_or(open::UNDECIDED)? == Interior::Inside)
    }

    /// A combining group holds the eye where one of its positive members does, none of its
    /// negative ones, and one of its intersecting ones where it has any; a member group by this
    /// same test. A hidden member is never asked (it has no shape here).
    fn group_holds(scene: &crate::geometry::Scene, k: usize, eye: Vec3, five: bool) -> Result<bool, OpenItem> {
        let comb = &scene.groups.combinations[k];
        let mut inside = [false; 3];
        for m in &comb.members {
            let held = match m.source {
                Source::Hidden(_) => false,
                Source::Group(inner) => Self::group_holds(scene, inner, eye, five)?,
                Source::Object(i) => Self::object_holds(scene, i, eye)?,
            };
            if held {
                inside[m.role as usize] = true;
            }
        }
        let any_i = comb.members.iter().any(|m| m.role == Role::Intersecting);
        Ok(inside[Role::Positive as usize] && !inside[Role::Negative as usize] && (!any_i || inside[Role::Intersecting as usize]))
    }

    /// Every geometry object that is a member of combination `k`, however deep.
    fn members_of(scene: &crate::geometry::Scene, k: usize, out: &mut Vec<usize>) {
        for m in &scene.groups.combinations[k].members {
            match m.source {
                Source::Object(i) => out.push(i),
                Source::Group(inner) => Self::members_of(scene, inner, out),
                Source::Hidden(_) => {}
            }
        }
    }

    /// The medium of material object `m` (the holder, or a member of the holder group `h`).
    /// (a volume material's solid; the medium colour; inside a combined group the group's space)
    fn medium_of(scene: &Scene, plan: &RenderPlan, h: &Holder, m: usize) -> Medium {
        let g = &plan.geometry;
        let object = &scene.objects[g.scene_object[m]];
        let part = object.material.map(|k| &scene.materials[k].part).ok_or(open::HOLDER_MATERIAL)?;
        // (The medium of a fuzzy solid; Limits, observed: the camera with its eye
        // inside a fuzzy sphere or cube, fuzzy-more-pictures e-sphere, e-cube): a fuzzy material
        // (bit 0x2, with bit 0x8 or without, no tint bit) is a medium as a plain one's, its factor 1
        if part.flags & MATERIAL_FUZZY != 0 && part.flags & !(MATERIAL_CURRENT | MATERIAL_FUZZY | FUZZY_BLENDED_BIT) == 0 {
            let law = MediumLaw::of_fuzzy_solid(part.volume_colour, part.refraction, part.flags & FUZZY_BLENDED_BIT != 0);
            return Ok((law, None));
        }
        // (The media: bit 0x80 acts for a shadow ray only, "no medium at all"; it
        // changes nothing for a camera ray's medium); beam-light-pictures' eyv/eyh materials hold it
        let plain = part.flags & !(MEDIUM_TINT_BITS | crate::shading::volume::bits::NO_SHADOW_MEDIUM) == MATERIAL_CURRENT;
        let volume = !VolumeKind::kinds(part.flags).is_empty();
        if volume && !matches!(object.body, crate::scene::Body::Shape(_)) {
            return Err(crate::shading::volume::open::SHAPE);
        }
        if !plain && !volume {
            return Err(open::HOLDER_MATERIAL);
        }
        let (law, mut tint) = solid_medium(part, &g.scene.objects[m].shape);
        if let (Some(t), Some(_)) = (tint.as_mut(), h.group) {
            if t.altitude.is_some() {
                t.group = group_space(scene, g, h.object, plan.version);
            }
        }
        Ok((law, tint))
    }

    /// Whether any solid holds the eye.
    pub fn any(&self) -> bool {
        !self.holders.is_empty()
    }

    /// The last crossing behind the ray's origin of holder `h`'s list of the ray's whole line:
    /// its distance t (below 0) and the object whose material is the medium there (a group's
    /// member, point 3).,
    fn behind(&self, h: &Holder, ray: &Ray) -> Option<(f64, usize)> {
        let whole = Range { near: f64::NEG_INFINITY, far: f64::INFINITY };
        match h.group {
            Some(k) => {
                let list = self.plan_geometry.group_list(k, ray, whole, &self.ctx);
                let e = list.iter().filter(|e| e.t() < 0.0).last()?;
                let member = if self.version == RenderAs::Bryce50 { e.carried } else { e.surface };
                Some((e.t(), member))
            }
            None => {
                let o = &self.plan_geometry.objects[h.object];
                let local = o.placement.to_local_as(self.version, ray);
                let list = self.plan_geometry.member_crossings(o.shape, &local, ray, &self.ctx);
                let t = list.iter().map(|c| c.hit.t).filter(|&t| t < 0.0).last()?;
                Some((t, h.object))
            }
        }
    }

    /// The decision for one camera ray, `None` where no solid holds the eye or none is left.
    pub fn decide(&self, ray: &CameraRay) -> Result<Option<EyeDecision>, OpenItem> {
        match self.holders.as_slice() {
            [] => return Ok(None),
            // : one holder that is not a group decides whatever the ray
            [h] if h.group.is_none() => return Ok(Some(EyeDecision { held: vec![(h.object, h.object)] })),
            _ => {}
        }
        let line = Ray { origin: ray.origin, dir: ray.dir };
        let mut left: Vec<(f64, usize, usize)> = Vec::new();
        for h in &self.holders {
            match self.behind(h, &line) {
                Some((t, member)) => left.push((t, h.object, member)),
                None if self.version == RenderAs::Bryce30 => return Err(open::NO_CROSSING_BEHIND),
                None => {}
            }
        }
        // : each place from the first takes any later holder whose distance is
        // smaller, the two exchanged, in the order the holders were asked
        for i in 0..left.len() {
            for j in i + 1..left.len() {
                if left[j].0 < left[i].0 {
                    left.swap(i, j);
                }
            }
        }
        if left.is_empty() {
            return Ok(None);
        }
        Ok(Some(EyeDecision { held: left.into_iter().map(|(_, o, m)| (o, m)).collect() }))
    }

    /// The media a camera ray's path starts in. (Every holder left is held by the
    /// path, the deciding one last; none left: outside every object)
    pub fn start_media(&self, ray: &CameraRay) -> Result<PathMedia, OpenItem> {
        match self.decide(ray)? {
            None => Ok(PathMedia::OUTSIDE),
            Some(d) => PathMedia::holding_all(&d.held).ok_or(open::MANY),
        }
    }

    /// A camera sample's colour c as gives it: ⟨ck·Wk⟩, one binary32 product per channel,
    /// with W the weight the deciding solid's medium gives the camera ray from the weight
    /// (1, 1, 1) and its own hit's t and point (or its miss); c unchanged where no solid decides,
    /// or where the hit is a volume material's surface (`as_sent`: the ray comes back with the
    /// weight it was sent with).,
    pub fn apply(&self, ray: &CameraRay, hit: Option<(f64, Vec3)>, as_sent: bool, c: [f32; 3]) -> Result<[f32; 3], OpenItem> {
        let Some((solid, member)) = self.decide(ray)?.and_then(|d| d.deciding()) else {
            return Ok(c);
        };
        if as_sent {
            return Ok(c);
        }
        let (law, tint) = self.media.get(&member).copied().unwrap_or(Err(open::HOLDER_MATERIAL))?;
        let placement = &self.plan_geometry.objects[if tint.is_some_and(|t| t.group.is_some()) { solid } else { member }].placement;
        let law = tinted(self.version, law, tint, placement, hit)?;
        let w = law.weight_back(self.version, [1.0; 3], hit.map(|(t, _)| t));        Ok([c[0] * w[0], c[1] * w[1], c[2] * w[2]])
    }
}
