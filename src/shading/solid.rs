// SPDX-License-Identifier: MIT
//! Which objects enclose a volume, and whose material a medium has: terrains, meshes and groups
//! as solids that hold a ray.
//!
//! ("Terrains, meshes and groups
//! as solids that hold a ray", points 1, 3 and 4), (Inputs: the shapes that enclose a
//! volume; for the mesh)
//! (Solid media)

use crate::version::RenderAs;

/// The kinds of object a ray's surface hit can name, as's point 1 sorts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidKind {
    Sphere,
    Cube,
    Cylinder,
    Cone,
    Torus,
    Slab,
    Pyramid,
    /// A mesh, closed or not.
    Mesh,
    /// A group, whatever its members.
    Group,
    /// A terrain with its closing-faces word.
    Terrain { closing_word: u32 },
    Plane,
    Square,
    Disc,
    Path,
    /// The own object of a light.
    Light(LightObject),
}

/// The kinds of light whose own object a ray can meet.,
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightObject {
    Radial,
    Spot,
    Parallel,
    SquareSpot,
    CylindricalParallel,
    /// The sun's object: encloses no volume in all three versions (no ray meets it, read as 3.0 only).
    Sun,
}

/// What leaves Open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidOpen {
    /// None arises since (a word with a bit above the six lowest is read by its six
    /// lowest bits); the type is kept for the callers.
    TerrainWord(u32),
}

/// The six lowest bits of a terrain's closing-faces word: its faces.
pub const TERRAIN_FACES: u32 = 0x3f;

/// Whether an object of `kind` encloses a volume, as `version` renders it.
pub fn encloses_volume(version: RenderAs, kind: SolidKind) -> Result<bool, SolidOpen> {
    Ok(match kind {
        SolidKind::Sphere
        | SolidKind::Cube
        | SolidKind::Cylinder
        | SolidKind::Cone
        | SolidKind::Torus
        | SolidKind::Slab
        | SolidKind::Pyramid
        | SolidKind::Mesh
        | SolidKind::Group => true,
        SolidKind::Plane | SolidKind::Square | SolidKind::Disc | SolidKind::Path => false,
        // : the own object of each of the five kinds of light
        // encloses a volume; the sun's does not, as 3.0, 3.1 and 5.0
        SolidKind::Light(LightObject::Sun) => false,
        SolidKind::Light(_) => true,
        SolidKind::Terrain { closing_word } => {
            // : a closing-faces word with a bit above the six lowest
            // is taken as the same word with only its six lowest bits, in every version
            let _ = version;
            closing_word & TERRAIN_FACES != 0
        }
    })
}

/// The material of the medium a ray travels in after it goes into a solid: the member that the
/// hit names where the hit is on a group, the object's own material otherwise.
pub fn medium_material(object_material: usize, member_materials: &[usize], hit_member: Option<usize>) -> usize {
    match hit_member {
        Some(k) => member_materials[k],
        None => object_material,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_words() {
        let t = |w| SolidKind::Terrain { closing_word: w };
        assert_eq!(encloses_volume(RenderAs::Bryce30, t(0)), Ok(false));
        assert_eq!(encloses_volume(RenderAs::Bryce31, t(0x20)), Ok(true));
        assert_eq!(encloses_volume(RenderAs::Bryce30, t(0x40)), Ok(false));
        // : as 5.0 bits 6 and above close nothing.
        assert_eq!(encloses_volume(RenderAs::Bryce50, t(0x40)), Ok(false));
        assert_eq!(encloses_volume(RenderAs::Bryce50, t(0x7f)), Ok(true));
        // : the six lowest bits in every version
        assert_eq!(encloses_volume(RenderAs::Bryce31, t(0x40)), Ok(false));
        assert_eq!(encloses_volume(RenderAs::Bryce31, t(0x41)), Ok(true));
    }
}
