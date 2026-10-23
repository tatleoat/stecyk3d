// SPDX-License-Identifier: MIT
//! A bounding-volume tree over a description mesh's triangles.
//!
//! (Speed on mesh scenes). Not a spec entry: it changes how the
//! nearest triangle is found, never which one or its arithmetic. `intersect_mesh` keeps the
//! nearest `t` in the range and, on equal `t`, the earliest triangle; the tree visits the
//! boxes that can hold a candidate and keeps the same (t, index) minimum, with every
//! triangle still tested by the same Möller-Trumbore expressions. Boxes are grown by a margin
//! far above the rounding of that test, so a triangle the test would accept is never culled.

use super::Triangle;
use crate::geom::Vec3;

/// Triangles per leaf at most.
const LEAF: usize = 4;
/// Relative growth of a box (of its extent), and of its largest coordinate: the test's
/// rounding error is ~1e-16 of the coordinates times the ray's grazing amplification; this is
/// orders above that for any ray that is not parallel to the triangle to 1e-9.
const GROW_EXTENT: f64 = 1e-6;
const GROW_COORD: f64 = 1e-9;
/// Slack on the ray parameter when a box is compared with the range or the best so far.
const SLACK_T: f64 = 1e-6;

#[derive(Clone, Copy, Debug)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Inner node: the right child's index (the left is the next node). Leaf: first slot.
    a: u32,
    /// 0 for an inner node, else the number of triangles in the leaf.
    n: u32,
}

/// The tree: nodes in depth-first order, and the triangle numbers its leaves list.
#[derive(Debug, Default)]
pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

/// A ray prepared for box tests.
pub struct Probe {
    origin: Vec3,
    inv: Vec3,
    zero: [bool; 3],
}

impl Probe {
    /// `None` for a ray with a non-finite origin or direction (the caller scans every triangle).
    pub fn new(origin: Vec3, dir: Vec3) -> Option<Probe> {
        if !origin.iter().chain(dir.iter()).all(|x| x.is_finite()) {
            return None;
        }
        Some(Probe {
            origin,
            inv: [1.0 / dir[0], 1.0 / dir[1], 1.0 / dir[2]],
            zero: [dir[0] == 0.0, dir[1] == 0.0, dir[2] == 0.0],
        })
    }
}

impl Bvh {
    pub fn build(tris: &[Triangle]) -> Bvh {
        let n = tris.len();
        let mut items: Vec<(u32, Vec3)> = (0..n)
            .map(|i| {
                let c = tris[i].corners;
                (i as u32, [0, 1, 2].map(|k| (c[0][k] + c[1][k] + c[2][k]) / 3.0))
            })
            .collect();
        let mut bvh = Bvh { nodes: Vec::with_capacity(2 * n / LEAF + 2), order: Vec::with_capacity(n) };
        if n > 0 {
            bvh.node(tris, &mut items);
        }
        bvh
    }

    fn node(&mut self, tris: &[Triangle], items: &mut [(u32, Vec3)]) -> usize {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let (mut clo, mut chi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let mut big = 0.0f64;
        for &(i, c) in items.iter() {
            for p in tris[i as usize].corners {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                    big = big.max(p[k].abs());
                }
            }
            for k in 0..3 {
                clo[k] = clo[k].min(c[k]);
                chi[k] = chi[k].max(c[k]);
            }
        }
        for k in 0..3 {
            let g = GROW_EXTENT * (hi[k] - lo[k]) + GROW_COORD * big + 1e-12;
            lo[k] -= g;
            hi[k] += g;
        }
        let me = self.nodes.len();
        self.nodes.push(Node { lo, hi, a: 0, n: 0 });
        let axis = (0..3).max_by(|&x, &y| (chi[x] - clo[x]).total_cmp(&(chi[y] - clo[y]))).unwrap_or(0);
        if items.len() <= LEAF || !(chi[axis] > clo[axis]) {
            // few triangles, or all centroids equal (a stack of copies): a leaf (a long one in the
            // second case, still correct).
            self.nodes[me].a = self.order.len() as u32;
            self.nodes[me].n = items.len() as u32;
            self.order.extend(items.iter().map(|&(i, _)| i));
            return me;
        }
        let mid = items.len() / 2;
        items.select_nth_unstable_by(mid, |x, y| x.1[axis].total_cmp(&y.1[axis]));
        let (l, r) = items.split_at_mut(mid);
        self.node(tris, l);
        let right = self.node(tris, r);
        self.nodes[me].a = right as u32;
        me
    }

    /// The slab test: the ray's parameter interval inside the box, `None` if it misses (or
    /// cannot reach `[near, far]` even with the slack).
    #[inline]
    fn enter(&self, nd: &Node, p: &Probe, near: f64, far: f64) -> Option<f64> {
        let mut t0 = f64::NEG_INFINITY;
        let mut t1 = f64::INFINITY;
        for k in 0..3 {
            if p.zero[k] {
                if p.origin[k] < nd.lo[k] || p.origin[k] > nd.hi[k] {
                    return None;
                }
            } else {
                let a = (nd.lo[k] - p.origin[k]) * p.inv[k];
                let b = (nd.hi[k] - p.origin[k]) * p.inv[k];
                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                t0 = t0.max(a);
                t1 = t1.min(b);
            }
        }
        if t0 > t1 {
            return None;
        }
        if t1 < near - SLACK_T * (near.abs() + t1.abs()) - 1e-12 {
            return None;
        }
        if far.is_finite() && t0 > far + SLACK_T * (far.abs() + t0.abs()) + 1e-12 {
            return None;
        }
        Some(t0)
    }

    /// Calls `test(i)` for the triangles of every leaf the ray can reach, in an order near to
    /// far. `test` returns the triangle's accepted `t`, if any; the nearest (and on equal `t`
    /// the lowest number) is returned with its number. `near`/`far` is the range.
    pub fn nearest(
        &self,
        p: &Probe,
        near: f64,
        far: f64,
        mut test: impl FnMut(usize) -> Option<f64>,
    ) -> Option<(usize, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(usize, f64)> = None;
        // (node, the ray parameter where the ray enters it): the entry is rechecked against the
        // best so far when the node is taken off.
        let mut stack = [(0u32, 0.0f64); 64];
        let mut sp = 0;
        let Some(t0) = self.enter(&self.nodes[0], p, near, far) else { return None };
        let mut cur = (0usize, t0);
        loop {
            let (ci, entry) = cur;
            let limit = match best {
                Some((_, t)) => t.min(far),
                None => far,
            };
            let nd = &self.nodes[ci];
            if !(limit.is_finite() && entry > limit + SLACK_T * (limit.abs() + entry.abs()) + 1e-12) {
                if nd.n == 0 {
                    let (l, r) = (ci + 1, nd.a as usize);
                    let tl = self.enter(&self.nodes[l], p, near, limit);
                    let tr = self.enter(&self.nodes[r], p, near, limit);
                    match (tl, tr) {
                        (Some(a), Some(b)) => {
                            if a <= b {
                                stack[sp] = (r as u32, b);
                                cur = (l, a);
                            } else {
                                stack[sp] = (l as u32, a);
                                cur = (r, b);
                            }
                            sp += 1;
                            continue;
                        }
                        (Some(a), None) => {
                            cur = (l, a);
                            continue;
                        }
                        (None, Some(b)) => {
                            cur = (r, b);
                            continue;
                        }
                        (None, None) => {}
                    }
                } else {
                    for s in nd.a..nd.a + nd.n {
                        let i = self.order[s as usize] as usize;
                        if let Some(t) = test(i) {
                            let better = match best {
                                None => true,
                                Some((bi, bt)) => t < bt || (t == bt && i < bi),
                            };
                            if better {
                                best = Some((i, t));
                            }
                        }
                    }
                }
            }
            if sp == 0 {
                return best;
            }
            sp -= 1;
            cur = (stack[sp].0 as usize, stack[sp].1);
        }
    }
}

/// A mesh's tree, built on the first ray that needs it and shared by the mesh's clones. Two
/// of them are always equal: the tree is a function of the triangles.
#[derive(Clone, Default)]
pub struct Accel(std::sync::Arc<std::sync::OnceLock<Bvh>>);

impl Accel {
    pub fn get(&self, tris: &[Triangle]) -> &Bvh {
        self.0.get_or_init(|| Bvh::build(tris))
    }
}

impl PartialEq for Accel {
    fn eq(&self, _: &Accel) -> bool {
        true
    }
}

impl std::fmt::Debug for Accel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Accel")
    }
}
