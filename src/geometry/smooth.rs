// SPDX-License-Identifier: MIT
//! The Mesh Editor's Smooth and Unsmooth: how Bryce gives a mesh's corner normals from its plane
//! normals and a maximum angle, for a program that imports a model and smooths it as Bryce does.
//! The renderer takes the stored normals as stored and does not use this.
//!
//! ("Bryce's smoothing of a mesh's normals",: the rule
//! of one application; a further click and Unsmooth, code only)
//! (Smooth and Unsmooth)

use crate::geom::Vec3;
use crate::numerics::rsqrt_as;
use crate::scene_meshes::{CornerUse, MeshResource};
use crate::version::RenderAs;

use super::Placement;

/// Degrees to radians, the decimal the entry gives. (Smoothing: theta in degrees
/// times 0.01745329251994)
const DEGREES: f64 = 0.01745329251994;

/// Two corners' points are the same where each coordinate is within this, strictly (the meshes of
/// a group, as 5.0).
const SAME_POINT: f64 = 1e-10;

/// The excluded role's bit of an object's second flag word; a mesh with it takes no part in a
/// group's smoothing.,
pub const EXCLUDED_ROLE_BIT: u32 = 0x4; // [format]

/// A mesh given to Smooth with its object's map (read only where meshes are smoothed together).
pub struct SmoothInput<'a> {
    pub mesh: &'a mut MeshResource,
    /// The object's three rows and translation .
    pub rows: [Vec3; 3],
    pub translation: Vec3,
}

/// The triangle's plane normal made again from its corner points: the unit cross product of the
/// edge from point 0 to point 1 and the edge from point 0 to point 2; for a cross product that is
/// the zero vector, the plane normal by which meets nothing (the zero vector as 3.0 and 5.0,
/// not a number as 3.1).
fn made_plane(points: [Vec3; 3], version: RenderAs) -> Vec3 {
    let a: Vec3 = std::array::from_fn(|k| points[1][k] - points[0][k]);
    let b: Vec3 = std::array::from_fn(|k| points[2][k] - points[0][k]);
    let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    if c == [0.0; 3] {
        return if version == RenderAs::Bryce31 { [f64::NAN; 3] } else { [0.0; 3] };
    }
    let q = rsqrt_as(version, (c[1] * c[1] + c[2] * c[2]) + c[0] * c[0]);
    [c[0] * q, c[1] * q, c[2] * q]
}

/// N . U in the order of the version's program.
fn dot_as(version: RenderAs, n: Vec3, u: Vec3) -> f64 {
    match version {
        RenderAs::Bryce30 => (n[0] * u[0] + n[2] * u[2]) + n[1] * u[1],
        RenderAs::Bryce31 => (n[0] * u[0] + n[1] * u[1]) + n[2] * u[2],
        RenderAs::Bryce50 => (n[1] * u[1] + n[2] * u[2]) + n[0] * u[0],
    }
}

/// Every triangle's plane normal made again from its points, for each mesh.
fn remake_planes(meshes: &mut [SmoothInput], version: RenderAs) {
    for m in meshes.iter_mut() {
        let mesh = &mut *m.mesh;
        for t in 0..mesh.triangles.len() {
            let tri = mesh.triangles[t];
            if let Some(ps) = tri.points.iter().map(|&p| mesh.points.get(p as usize).copied()).collect::<Option<Vec<_>>>() {
                mesh.triangles[t].plane = made_plane([ps[0], ps[1], ps[2]], version);
            }
        }
    }
}

/// One click of Smooth on the meshes of `meshes` with the maximum angle `degrees`: (1) every
/// triangle's plane normal is made again from its points; (2) every corner's normal is the mean of
/// the plane normals of its triangle and of every other triangle that names the same point number
/// and whose plane normal has a dot product with its own not below cos(theta); (3) every flag word
/// has bit 0 set and every corner word bit 0 clear, the other bits kept. The normals each mesh holds
/// afterwards are one per corner, in the order of the triangles and their corners.
///
/// With `together` (Bryce 5.0, the meshes of one group without the excluded role) a corner also takes
/// the triangles of the other meshes whose point, carried to the world by the first mesh's map and
/// into the other mesh's space by its inverse, lies in the other mesh's stored box (ends included),
/// one of whose corner points lies strictly within 1e-10 of it in each coordinate, and whose plane
/// normal (as made in (1)) passes the same test; those are added after the mesh's own, mesh by mesh
/// in the order given and in each by the triangles' numbers, and counted in the mean. As 3.0 and
/// 3.1 each mesh is smoothed alone: pass `together` false.
pub fn smooth(meshes: &mut [SmoothInput], degrees: f64, version: RenderAs, together: bool) {
    remake_planes(meshes, version);
    let cos_theta = (degrees * DEGREES).cos();
    let together = together && version == RenderAs::Bryce50;
    let maps: Vec<Option<Placement>> =
        meshes.iter().map(|m| if together { Placement::new(m.rows, m.translation) } else { None }).collect();
    let planes: Vec<Vec<Vec3>> = meshes.iter().map(|m| m.mesh.triangles.iter().map(|t| t.plane).collect()).collect();
    let mut normals: Vec<Vec<Vec3>> = Vec::with_capacity(meshes.len());
    for (mi, m) in meshes.iter().enumerate() {
        let mesh = &*m.mesh;
        let mut out = Vec::with_capacity(mesh.triangles.len() * 3);
        for (f, tri) in mesh.triangles.iter().enumerate() {
            let n = planes[mi][f];
            for &v in &tri.points {
                let mut sum = n;
                let mut count = 1.0;
                let mut add = |u: Vec3| {
                    for k in 0..3 {
                        sum[k] += u[k];
                    }
                    count += 1.0;
                };
                for (g, other) in mesh.triangles.iter().enumerate() {
                    if g != f && other.points.contains(&v) && dot_as(version, n, planes[mi][g]) >= cos_theta {
                        add(planes[mi][g]);
                    }
                }
                if together && let (Some(here), Some(&point)) = (&maps[mi], mesh.points.get(v as usize)) {
                    let world = here.to_world(point);
                    for (oi, o) in meshes.iter().enumerate().filter(|&(oi, _)| oi != mi) {
                        let Some(there) = &maps[oi] else { continue };
                        let p = there.point_to_local(world);
                        let b = o.mesh.bounds;
                        if !(0..3).all(|a| p[a] >= b[a] && p[a] <= b[3 + a]) {
                            continue;
                        }
                        for (g, other) in o.mesh.triangles.iter().enumerate() {
                            let near = other.points.iter().any(|&q| {
                                o.mesh.points.get(q as usize).is_some_and(|c| (0..3).all(|a| (c[a] - p[a]).abs() < SAME_POINT))
                            });
                            if near && dot_as(version, n, planes[oi][g]) >= cos_theta {
                                add(planes[oi][g]);
                            }
                        }
                    }
                }
                out.push([sum[0] / count, sum[1] / count, sum[2] / count]);
            }
        }
        normals.push(out);
    }
    for (m, out) in meshes.iter_mut().zip(normals) {
        let mesh = &mut *m.mesh;
        for (f, tri) in mesh.triangles.iter_mut().enumerate() {
            tri.normals = Some([3 * f as u32, 3 * f as u32 + 1, 3 * f as u32 + 2]);
            tri.flag_word |= 1;
            for w in &mut tri.corner_words {
                *w &= !1;
            }
            tri.corner_use = CornerUse::of(tri.corner_words, tri.flag_word);
        }
        mesh.normals = out;
    }
}

/// Unsmooth: (1) of a click (the plane normals made again), then every flag word has bit 0 clear and
/// every corner word bit 0 set, the other bits kept, so that the shading normal is the plane
/// normal; the corners' normals are left as they are (no longer read; a later Smooth makes them
/// anew).
pub fn unsmooth(mesh: &mut MeshResource, version: RenderAs) {
    remake_planes(&mut [SmoothInput { mesh: &mut *mesh, rows: [[0.0; 3]; 3], translation: [0.0; 3] }], version);
    for tri in &mut mesh.triangles {
        tri.flag_word &= !1;
        for w in &mut tri.corner_words {
            *w |= 1;
        }
        tri.corner_use = CornerUse::of(tri.corner_words, tri.flag_word);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_meshes::MeshTriangle;

    fn tri(points: [u32; 3], plane: Vec3) -> MeshTriangle {
        MeshTriangle {
            points,
            normals: None,
            coords: None,
            plane,
            corner_use: CornerUse::Neither,
            corner_words: [3; 3],
            flag_word: 0,
        }
    }

    /// Two triangles folded along the edge points 0 and 1 by 90 degrees: the floor in y = 0, the
    /// wall in z = 0 above it.
    fn fold() -> MeshResource {
        MeshResource {
            object: 0,
            version: 2,
            bounds: [-1.0; 6],
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            coords: Vec::new(),
            // the stored plane normals are the opposite ones: a click makes them again
            triangles: vec![tri([0, 2, 1], [0.0, -1.0, 0.0]), tri([0, 1, 3], [0.0, 0.0, 1.0])],
        }
    }

    fn run(mesh: &mut MeshResource, degrees: f64, version: RenderAs) {
        smooth(&mut [SmoothInput { mesh, rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], translation: [0.0; 3] }], degrees, version, false);
    }

    /// The plane normals are made again from the points, whatever was stored; at 85 degrees a fold
    /// of 90 stays sharp (each corner its own plane normal), at 100 the corners at the shared
    /// points 0 and 1 are the mean of the two plane normals and the others stay. The words are set
    /// so that the shading normal is the corners' mix.
    #[test]
    fn fold_stays_sharp_below_its_angle_and_blends_above() {
        for version in [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50] {
            let mut m = fold();
            run(&mut m, 85.0, version);
            // (0,2,1): edges (0,0,1) and (1,0,0): cross (0*0 - 1*0, 1*1 - 0*0, 0) = (0, 1, 0)
            assert_eq!(m.triangles[0].plane, [0.0, 1.0, 0.0]);
            assert_eq!(m.triangles[1].plane, [0.0, 0.0, 1.0]);
            assert!(m.normals[..3].iter().all(|n| *n == [0.0, 1.0, 0.0]));
            assert!(m.normals[3..].iter().all(|n| *n == [0.0, 0.0, 1.0]));
            assert!(m.triangles.iter().all(|t| t.flag_word & 1 == 1 && t.corner_words.iter().all(|w| w & 1 == 0)));
            let mut m = fold();
            run(&mut m, 100.0, version);
            // triangle 0's corners: points 0, 2, 1; points 0 and 1 are shared with triangle 1
            assert_eq!(m.normals[0], [0.0, 0.5, 0.5]);
            assert_eq!(m.normals[1], [0.0, 1.0, 0.0]);
            assert_eq!(m.normals[2], [0.0, 0.5, 0.5]);
        }
    }

    /// A triangle of three points on a line: no plane normal, by which nothing is met (the zero
    /// vector, not a number as 3.1).
    #[test]
    fn degenerate_triangle_has_no_plane_normal() {
        let mut m = fold();
        m.points[3] = [2.0, 0.0, 0.0];
        run(&mut m, 85.0, RenderAs::Bryce30);
        assert_eq!(m.triangles[1].plane, [0.0; 3]);
        run(&mut m, 85.0, RenderAs::Bryce31);
        assert!(m.triangles[1].plane.iter().all(|c| c.is_nan()));
    }

    /// Unsmooth: the plane normals made again, the words the other way, the normals kept.
    #[test]
    fn unsmooth_sets_the_words_the_other_way() {
        let mut m = fold();
        run(&mut m, 100.0, RenderAs::Bryce31);
        let kept = m.normals.clone();
        unsmooth(&mut m, RenderAs::Bryce31);
        assert_eq!(m.normals, kept);
        assert!(m.triangles.iter().all(|t| t.flag_word & 1 == 0 && t.corner_words.iter().all(|w| w & 1 == 1)));
    }
}
