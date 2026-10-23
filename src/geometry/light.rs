// SPDX-License-Identifier: MIT
//! The lights' own objects: which rays meet them, their shapes, the endless option, the marks of
//! their lists and the points of their boxes.
//!
//! ,, with the shapes'
//! own entries and, and (a light in a
//! combined group reads these lists and marks).

use super::crossings::LocalCrossing;
use super::{Ray, Shape, ShapeContext, local_crossings};
use crate::geom::Vec3;
use crate::version::RenderAs;
use std::f64::consts::PI;

/// The bit of a light's second flag word that makes its own object visible to rays.
pub const VISIBLE: u32 = 0x200;
/// The "Infinite light" choice: the endless option.
pub const ENDLESS: u32 = 0x2000;
/// The bits that hide any object: 0x1 and 0x400000., [format]
pub const HIDDEN: u32 = 0x1 | 0x40_0000;

/// The shape of a light's object: the radial light's sphere, the spot light's cone.
/// The parallel light's object as a primitive is Open ; as a member of a group it is
/// the cube of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightShape {
    Sphere,
    Cone,
    Cube,
    Cylinder,
    Pyramid,
}

/// A light's own object: its shape and its second flag word.
/// (Types)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightObject {
    pub shape: LightShape,
    pub flags: u32,
}

impl LightObject {
    /// Whether a ray meets the object: bit 0x200, neither hidden bit, and no shadow ray.
    pub fn met(self, shadow: bool) -> bool {
        self.flags & VISIBLE != 0 && self.flags & HIDDEN == 0 && !shadow
    }

    /// The endless option, bit 0x2000 alone, on a spot, parallel, cylindrical or square light:
    /// lists read it with or without 0x200; a hit also needs the light to be met; the radial
    /// light's sphere is not changed.
    pub fn endless(self) -> bool {
        self.shape != LightShape::Sphere && self.flags & ENDLESS != 0
    }

    /// The plain shape of the kind.
    pub fn plain(self) -> Shape {
        match self.shape {
            LightShape::Sphere => Shape::Sphere,
            LightShape::Cone => Shape::Cone,
            LightShape::Cube => Shape::Cube,
            LightShape::Cylinder => Shape::Cylinder,
            LightShape::Pyramid => Shape::Pyramid,
        }
    }

    /// Whether a crossing of the object's list shows: as 3.0 and 3.1 always; as 5.0 only for a
    /// light with bit 0x200 and a ray that is no shadow ray (the hidden bits are not tested).
    pub fn marks_show(self, ctx: &ShapeContext) -> bool {
        ctx.version != RenderAs::Bryce50 || (self.flags & VISIBLE != 0 && !ctx.shadow)
    }

    /// 's list of the object for a ray in its own space, with's marks. The endless
    /// cone's list is Open; the plain cone's list stands in for it.
    pub fn list(self, r: &Ray, ctx: &ShapeContext) -> Vec<LocalCrossing> {
        let list = if self.endless() {
            super::crossings::endless_local_crossings(self.plain(), r, ctx)
        } else {
            local_crossings(self.plain(), r, ctx)
        };
        if self.marks_show(ctx) {
            return list;
        }
        list.into_iter().map(|c| LocalCrossing::hidden(c.hit.t)).collect()
    }

    /// The points of the light's own space whose carried box the structure lists it by.
    pub fn box_points(self, version: RenderAs) -> Vec<Vec3> {
        match self.shape {
            LightShape::Sphere => super::structure::sphere_points(),
            LightShape::Cone => spot_points(version),
            LightShape::Cube => parallel_points(version),
            // :'s cylinder points (48) and, as 3.0 only (revised: 3.1 takes no such point, spec), (0, -2, 0)
            LightShape::Cylinder => {
                let mut v = super::structure::shape_points(Shape::Cylinder, version).expect("cylinder points");
                if version == RenderAs::Bryce30 {
                    v.push([0.0, -2.0, 0.0]);
                }
                v
            }
            // : the four corners, the apex and, as 3.0 only (revised: 3.1 takes no such point, spec), (0, -1, 0)
            LightShape::Pyramid => {
                let mut v = super::structure::shape_points(Shape::Pyramid, version).expect("pyramid points");
                if version == RenderAs::Bryce30 {
                    v.push([0.0, -1.0, 0.0]);
                }
                v
            }
        }
    }
}

/// The spot light's box points: the 24 rim points at y = -1, the apex, and as 3.0 only (revised: 3.1 takes no such point, spec)
/// the point (0, -2, 0).
pub fn spot_points(version: RenderAs) -> Vec<Vec3> {
    let step = |k: usize| k as f64 * PI / 12.0; // 15 degrees * k, in binary64
    let mut v: Vec<Vec3> = (0..24).map(|k| [step(k).sin(), -1.0, step(k).cos()]).collect();
    v.push([0.0, 1.0, 0.0]);
    if version == RenderAs::Bryce30 {
        v.push([0.0, -2.0, 0.0]);
    }
    v
}

/// The parallel light's box points: the eight corners (+-0.5 on each axis) and, as 3.0 and 3.1
/// only, the point (0, -1, 0).
pub fn parallel_points(version: RenderAs) -> Vec<Vec3> {
    let mut v = Vec::with_capacity(9);
    for x in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                v.push([x, y, z]);
            }
        }
    }
    if version == RenderAs::Bryce30 {
        v.push([0.0, -1.0, 0.0]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which rays meet a light's object.
    #[test]
    fn filter() {
        let l = |flags| LightObject { shape: LightShape::Sphere, flags };
        assert!(l(0x200).met(false));
        assert!(!l(0x200).met(true));
        assert!(!l(0x1100).met(false));
        assert!(!l(0x201).met(false) && !l(0x40_0200).met(false));
    }
}
