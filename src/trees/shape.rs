// SPDX-License-Identifier: MIT
//! The built-in leaf's shape and its normals, from the substrate's leaf tables.
//!
//! testing-interface.md (`shape` and `triangle` lines)
//! (Orders and forms: choice 7; stated by)

use super::arith::{Vec3, add, cross, dot, length, scale, sub};

/// The three leaf tables.
#[derive(Clone, Copy, Debug)]
pub struct LeafMesh<'a> {
    /// 20 points, x y z of each.
    pub points: &'a [f64; 60],
    /// 20 places in the leaf's picture, u v of each.
    pub uvs: &'a [f32; 40],
    /// 20 triangles, three point numbers each (every number 0 to 19).
    pub triangles: &'a [u32; 60],
}

/// One triangle as the `triangle` line gives it. (triangle)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeTriangle {
    pub numbers: [u32; 3],
    pub corners: [Vec3; 3],
    pub uvs: [[f32; 2]; 3],
    pub plane: Vec3,
    pub corner_normals: [Vec3; 3],
}

impl LeafMesh<'_> {
    fn point(&self, i: u32) -> Vec3 {
        let i = i as usize * 3;
        [self.points[i], self.points[i + 1], self.points[i + 2]]
    }

    fn uv(&self, i: u32) -> [f32; 2] {
        let i = i as usize * 2;
        [self.uvs[i], self.uvs[i + 1]]
    }

    fn triangle(&self, t: usize) -> [u32; 3] {
        [self.triangles[3 * t], self.triangles[3 * t + 1], self.triangles[3 * t + 2]]
    }

    /// The triangles with their plane and corner normals, in the table's order.
    pub fn triangles_with_normals(&self) -> Vec<ShapeTriangle> {
        let count = self.triangles.len() / 3;
        let numbers: Vec<[u32; 3]> = (0..count).map(|t| self.triangle(t)).collect();
        let planes: Vec<Vec3> = numbers.iter().map(|n| plane_normal(n.map(|i| self.point(i)))).collect();
        // The corners' normals so far, triangle by triangle, each triangle's three in order.
        let mut earlier: Vec<Vec3> = Vec::with_capacity(3 * count);
        (0..count)
            .map(|t| {
                let own = planes[t];
                let corner_normals = numbers[t].map(|p| {
                    // (a corner's mean: the own triangle's plane normal first, then
                    // every other triangle naming the point by its number whose dot with it is
                    // not below −1, in the table's order; the sum divided by the count)
                    let mut sum = own;
                    let mut n = 1u32;
                    for (u, nu) in numbers.iter().enumerate() {
                        if u != t && nu.contains(&p) && !(dot(planes[u], own) < -1.0) {
                            sum = add(sum, planes[u]);
                            n += 1;
                        }
                    }
                    let c = f64::from(n);
                    let mean = [sum[0] / c, sum[1] / c, sum[2] / c];
                    // (a corner's normal: the first earlier corner's normal within
                    // 1e-10 in each coordinate, else the mean)
                    let near = |e: &Vec3| (0..3).all(|k| (mean[k] - e[k]).abs() < NEAR);
                    let normal = earlier.iter().copied().find(near).unwrap_or(mean);
                    earlier.push(normal);
                    normal
                });
                ShapeTriangle {
                    numbers: numbers[t],
                    corners: numbers[t].map(|i| self.point(i)),
                    uvs: numbers[t].map(|i| self.uv(i)),
                    plane: own,
                    corner_normals,
                }
            })
            .collect()
    }
}

/// How near two corners' normals must be to be shared.
const NEAR: f64 = 1e-10;

/// (V1 − V0) × (V0 − V2), of unit length ((x² + y²) + z²), or 0 where that product is 0.
/// ; (Orders and forms, choice 7)
pub fn plane_normal(v: [Vec3; 3]) -> Vec3 {
    let c = cross(sub(v[1], v[0]), sub(v[0], v[2]));
    if c[0] == 0.0 && c[1] == 0.0 && c[2] == 0.0 {
        return [0.0; 3];
    }
    scale(1.0 / length(c), c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_normals() {
        // (V1 − V0) × (V0 − V2) for V0 = 0, V1 = x, V2 = y: x × (−y) = −z.
        let n = plane_normal([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        assert_eq!(n, [0.0, 0.0, -1.0]);
        assert_eq!(plane_normal([[1.0, 2.0, 3.0]; 3]), [0.0; 3]);
    }

    #[test]
    fn corner_means_are_not_unit() {
        // Two triangles sharing points 0 and 1, folded: the shared corners' normals are the
        // mean of two unit normals (not unit), the others their own triangle's.
        let mut points = [0.0f64; 60];
        let set = |p: &mut [f64; 60], i: usize, v: [f64; 3]| p[3 * i..3 * i + 3].copy_from_slice(&v);
        set(&mut points, 0, [0.0, 0.0, 0.0]);
        set(&mut points, 1, [1.0, 0.0, 0.0]);
        set(&mut points, 2, [0.0, 1.0, 0.0]);
        set(&mut points, 3, [0.0, 0.0, 1.0]);
        let mut triangles = [4u32; 60]; // the other 18 triangles name only points 4, 5, 6
        triangles[..6].copy_from_slice(&[0, 1, 2, 1, 0, 3]);
        for t in 2..20 {
            triangles[3 * t..3 * t + 3].copy_from_slice(&[4, 5, 6]);
        }
        let uvs = [0.0f32; 40];
        let mesh = LeafMesh { points: &points, uvs: &uvs, triangles: &triangles };
        let tri = mesh.triangles_with_normals();
        assert_eq!(tri.len(), 20);
        let a = tri[0].plane;
        let b = tri[1].plane;
        assert_eq!(a, [0.0, 0.0, -1.0]);
        assert_eq!(b, [0.0, -1.0, 0.0]);
        assert_eq!(tri[0].corner_normals[0], [0.0, -0.5, -0.5]);
        assert_eq!(tri[0].corner_normals[2], a);
        assert_eq!(tri[1].corner_normals[2], b);
    }
}
