// SPDX-License-Identifier: MIT
//! Metaballs: the field of a metaball and of a set, the set a ray meets, the crossing of the
//! set's surface, what the walk does with it, and the material a hit shows.
//!
//! , (the set
//! and its order),, (the
//! material at a hit).
//! (Metaballs)

use super::structure::{Bounds, box_met, sphere_points};
use super::{Placement, Ray, cofactors, row_times};
use crate::geom::Vec3;
use crate::numerics::{rsqrt_as, sqrt_as};
use crate::version::RenderAs;

/// The version metaballs exist in.
const V: RenderAs = RenderAs::Bryce50;

/// Where the field reaches 1/2: the surface.
const SURFACE: f64 = 0.5;
/// The band around 1/2 inside which a halving point is the crossing. step 4
const BAND_LOW: f64 = 0.49999;
const BAND_HIGH: f64 = 0.50001;
/// The shift of the origin along the ray, over L. step 2
const SHIFT: f64 = 0.001;
/// The step is H times this (the double nearest 0.0125). step 3
const STEP: f64 = 0.0125;
/// The offset of the normal's central differences. step 6
const OFFSET: f64 = 0.0001;
/// A share must lead by more than this to take the lead.
const MARGIN: f64 = 0.001;
/// Bits of a material's flag word whose hit shows the leader's own material (the fuzzy
/// material, the mode of bit 0x4, the three volumes). [format]
const OWN_BITS: u32 = 0x2 | 0x4 | 0x100 | 0x200 | 0x400;
/// The flag word of the material made for a hit.
pub const MADE_FLAGS: u32 = 0x10000;

/// `a0·b0 + (a1·b1 + a2·b2)`: the grouping of every sum of three terms here (the rows decide
/// it: Accuracy). (Metaballs)
fn dot3(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + (a[1] * b[1] + a[2] * b[2])
}

/// The translation of the inverse map: t'_j = s_j·D_j·(1/det) with D_j the 3 x 3 determinant
/// of the rows other than row j and the translation (in that order), written out as the six
/// products of the rule of Sarrus, s = -, +, -. The `inverse` rows of metaballs-scenes.jsonl
/// decide it (76 of 76 balls; the 5.0 form t'_j = -((t·adj)_j · (1/det)) of `Placement`
/// differs in the last bit for two turned ones).,
/// (Metaballs: the inverse's translation)
fn inverse_translation(m: &[Vec3; 3], t: Vec3, inv: f64) -> Vec3 {
    let d3 = |p: Vec3, q: Vec3, r: Vec3| -> f64 {
        p[0] * q[1] * r[2] + p[1] * q[2] * r[0] + p[2] * q[0] * r[1]
            - p[2] * q[1] * r[0]
            - p[0] * q[2] * r[1]
            - p[1] * q[0] * r[2]
    };
    [-d3(m[1], m[2], t) * inv, d3(m[0], m[2], t) * inv, -d3(m[0], m[1], t) * inv]
}

/// The sum of three products in the arrangement the rows fix.
/// (Metaballs)
fn dot_xz_y(a: Vec3, b: Vec3) -> f64 {
    (a[0] * b[0] + a[2] * b[2]) + a[1] * b[1]
}
fn dot_xy_z(a: Vec3, b: Vec3) -> f64 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// One metaball of the scene.,
#[derive(Clone, Debug, PartialEq)]
pub struct Ball {
    pub placement: Placement,
    /// +1 or -1.
    pub negative: bool,
    /// The group of whose list it is a member; `None` for the root list.
    pub group: Option<usize>,
    /// Its box: the sphere's 25 x 24 points carried to the world, not widened.
    pub bounds: Bounds,
    /// The map's inverse (by cofactors) and its translation: a point or a ray of the world is
    /// carried to the metaball's space as o·M⁻¹ + t' and d·M⁻¹.
    inverse: [Vec3; 3],
    inverse_translation: Vec3,
}

impl Ball {
    /// Builds a metaball; its box is the sphere's points of carried by the map.
    pub fn new(placement: Placement, negative: bool, group: Option<usize>) -> Ball {
        let rows = placement.rows();
        let t = placement.translation();
        let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
        for p in sphere_points() {
            let c = |j: usize| p[0] * rows[0][j] + p[1] * rows[1][j] + p[2] * rows[2][j] + t[j];
            let w = [c(0), c(1), c(2)];
            for a in 0..3 {
                b.min[a] = b.min[a].min(w[a]);
                b.max[a] = b.max[a].max(w[a]);
            }
        }
        let (inverse, _, inv) = cofactors(&rows).expect("a placement has an inverse");
        let inverse_translation = inverse_translation(&rows, t, inv);
        Ball { placement, negative, group, bounds: b, inverse, inverse_translation }
    }

    /// A world point in the metaball's own space: p·M⁻¹ + t'.
    fn point_to_local(&self, p: Vec3) -> Vec3 {
        let q = row_times(p, &self.inverse);
        [q[0] + self.inverse_translation[0], q[1] + self.inverse_translation[1], q[2] + self.inverse_translation[2]]
    }

    /// The ray in the metaball's own space; the direction keeps its length as carried.
    pub fn ray_to_local(&self, r: &Ray) -> Ray {
        Ray { origin: self.point_to_local(r.origin), dir: row_times(r.dir, &self.inverse) }
    }

    fn sign(&self) -> f64 {
        if self.negative { -1.0 } else { 1.0 }
    }
}

/// The scene's metaballs in the order of's entries. (Metaballs)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metaballs {
    pub balls: Vec<Ball>,
    /// The code `grid`: the entries stand in the reverse of the list order, and so does the
    /// search's order of neighbours.
    pub reversed: bool,
}

/// The soft-object function.
pub fn w(u: f64) -> f64 {
    if u > 1.0 {
        return 0.0;
    }
    // 1 - u (22 - u (17 - 4u)) * (1/9), 1/9 the double nearest; Horner's form is the one
    // metaballs-scenes.jsonl's `field` rows give (the other arrangements of differ in
    // 74 to 150 of 456 values).
    1.0 - u * (22.0 - u * (17.0 - 4.0 * u)) * (1.0 / 9.0)
}

/// The field of a metaball at a world point: s·w(u), and +0 for either sign above u = 1.
pub fn field(ball: &Ball, p: Vec3) -> f64 {
    let q = ball.point_to_local(p);
    let u = dot3(q, q);
    if u > 1.0 { 0.0 } else { ball.sign() * w(u) }
}

/// A crossing of a set's surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// The distance from the shifted origin, which the walk compares. step 5
    pub t: f64,
    pub point: Vec3,
    /// Unit length, not turned toward the ray. step 6
    pub normal: Vec3,
    /// S at the point.
    pub sum: f64,
    /// The set, in its order (indices into `Metaballs::balls`).
    pub set: Vec<usize>,
}

/// The marks a metaball carries for one ray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    None,
    Done,
    Missed,
    Kept,
}

/// What a material made for a hit, or a leader's own, is built from: the numbers
/// that mixes. (Metaballs)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixInput {
    pub flags: u32,
    /// The A colours of Diffuse, Ambient, Specular, Specular Halo and Transparent.
    pub colours: [[f32; 3]; 5],
    /// The A values of Diffusion, Ambience, Specularity, Transparency, Reflection, Bump,
    /// Metallicity, Volume density.
    pub values: [f32; 8],
    pub volume_colour: [f32; 3],
    pub refraction: f32,
    /// Edge softness, fuzzy factor, quality.
    pub volume_numbers: [f32; 3],
}

/// The shares, the leader and the material of a hit.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// Each member's share, in the set's order.
    pub shares: Vec<f64>,
    /// The leading member (index into `Metaballs::balls`).
    pub leader: usize,
    /// The made material, or `None` where the leader's own shows.
    pub made: Option<MixInput>,
}

impl Metaballs {
    /// The set of a ray asked about metaball `m0`.
    pub fn set(&self, m0: usize, ray: &Ray, lo: f64, far: f64) -> Vec<usize> {
        let n = self.balls.len();
        let group = self.balls[m0].group;
        let usable: Vec<bool> = (0..n)
            .map(|i| self.balls[i].group == group && box_met(&self.balls[i].bounds, ray, lo, far))
            .collect();
        if !usable[m0] {
            return Vec::new();
        }
        let overlap = |a: &Bounds, b: &Bounds| (0..3).all(|k| a.min[k] < b.max[k] && b.min[k] < a.max[k]);
        let mut seen = vec![false; n];
        let mut visit = Vec::new();
        // The search: depth first from m0, to the neighbours in the order of the entries.
        let mut stack = vec![(m0, 0usize)];
        seen[m0] = true;
        visit.push(m0);
        while let Some(&mut (m, ref mut next)) = stack.last_mut() {
            let mut found = None;
            while *next < n {
                let k = *next;
                *next += 1;
                let j = if self.reversed { n - 1 - k } else { k };
                if !seen[j] && usable[j] && overlap(&self.balls[m].bounds, &self.balls[j].bounds) {
                    found = Some(j);
                    break;
                }
            }
            match found {
                Some(j) => {
                    seen[j] = true;
                    visit.push(j);
                    stack.push((j, 0));
                }
                None => {
                    stack.pop();
                }
            }
        }
        visit.reverse();
        visit
    }

    /// The field of a set at a world point: the members' fields added in the set's order.
    pub fn sum(&self, set: &[usize], p: Vec3) -> f64 {
        let mut s = 0.0;
        for &m in set {
            s += field(&self.balls[m], p);
        }
        s
    }

    /// The crossing of a ray with a set's surface, or none.
    pub fn crossing(&self, set: &[usize], ray: &Ray) -> Option<Crossing> {
        // 1. The stretch.
        let mut a: Option<f64> = None;
        let mut b = f64::NEG_INFINITY;
        let mut h_short = f64::INFINITY;
        let mut l = f64::INFINITY;
        let mut any = false;
        for &m in set {
            let local = self.balls[m].ray_to_local(ray);
            let (o, d) = (local.origin, local.dir);
            let dd = dot_xz_y(d, d);
            let len = sqrt_as(V, dot_xy_z(d, d));
            let od = dot_xz_y(o, d);
            let c = -od / dd;
            let oo = dot_xz_y(o, o);
            // A member wholly behind an origin outside its reach takes no part.
            if c < 0.0 && oo > 1.0 {
                continue;
            }
            l = l.min(len);
            let lc = len * c;
            let under = (1.0 - oo) + lc * lc;
            if under < 0.0 {
                continue;
            }
            let h = sqrt_as(V, under) / len;
            let (t0, t1) = (c - h, c + h);
            any = true;
            a = Some(a.map_or(t0, |x| x.min(t0)));
            b = b.max(t1);
            h_short = h_short.min(2.0 * h);
        }
        if !any {
            return None;
        }
        let a = a.unwrap_or(0.0);
        let a = if a < 0.0 { 0.0 } else { a };
        // 2. The shifted origin.
        let k = SHIFT / l;
        let o2 = [ray.origin[0] + k * ray.dir[0], ray.origin[1] + k * ray.dir[1], ray.origin[2] + k * ray.dir[2]];
        let at = |t: f64| [o2[0] + t * ray.dir[0], o2[1] + t * ray.dir[1], o2[2] + t * ray.dir[2]];
        let s_at = |t: f64| self.sum(set, at(t));
        // 3. The march. The offsets are the step added to itself; sample k is at
        // (g + a) + s_(k-1), g + a rounded once and kept; after a sample on the start's side the
        // next is taken while s_k + a is below b. step 3
        let g = h_short * STEP;
        let ga = g + a;
        let start_side = s_at(a) > SURFACE;
        let mut offset = 0.0f64; // s_(k-1), the last sample on the start's side
        let first = loop {
            let t = ga + offset;
            if (s_at(t) > SURFACE) != start_side {
                break t;
            }
            offset += g;
            if !(offset + a < b) {
                return None;
            }
        };
        // 4. The halving: the first sample on the other side, then the points (s + h_j) + a with
        // h_j = g / 2^j, s growing by h_j where the point tested is on the start's side.
        let mut t = first;
        let mut h = g;
        let mut halving = false;
        let (t, sum) = loop {
            let s = s_at(t);
            if s > BAND_LOW && s < BAND_HIGH {
                break (t, s);
            }
            if halving && (s > SURFACE) == start_side {
                offset += h;
            }
            halving = true;
            h *= 0.5;
            t = (offset + h) + a;
        };
        // 6. The normal.
        let p = at(t);
        let diff = |axis: usize| {
            let mut lo = p;
            let mut hi = p;
            lo[axis] -= OFFSET;
            hi[axis] += OFFSET;
            self.sum(set, lo) - self.sum(set, hi)
        };
        let n = [diff(0), diff(1), diff(2)];
        let q = rsqrt_as(V, dot_xy_z(n, n));
        Some(Crossing { t, point: p, normal: [n[0] * q, n[1] * q, n[2] * q], sum, set: set.to_vec() })
    }

    /// The shares, the leader and the material of a hit at `p` on `set`. `own` gives each
    /// ball's material numbers.
    pub fn shown(&self, set: &[usize], p: Vec3, material: impl Fn(usize) -> MixInput) -> Shown {
        let fields: Vec<f64> = set.iter().map(|&m| field(&self.balls[m], p)).collect();
        // A negative member's share is twice its field taken positive.
        let mut t = 0.0;
        for (&m, &f) in set.iter().zip(&fields) {
            if self.balls[m].negative {
                t = -2.0 * f;
            }
        }
        let mut shares: Vec<f64> = set
            .iter()
            .zip(&fields)
            .map(|(&m, &f)| {
                if self.balls[m].negative {
                    -2.0 * f
                } else if f - t > 0.0 {
                    f - t
                } else {
                    0.0
                }
            })
            .collect();
        let mut total = 0.0;
        for s in &shares {
            total += s;
        }
        let inv = 1.0 / total;
        for s in &mut shares {
            *s *= inv;
        }
        let mut leader = set[0];
        let mut leading = 0.0;
        for (&m, &s) in set.iter().zip(&shares) {
            if s > leading + MARGIN {
                leader = m;
                leading = s;
            }
        }
        let lead = material(leader);
        if lead.flags & OWN_BITS != 0 {
            return Shown { shares, leader, made: None };
        }
        let members: Vec<(f64, MixInput)> = set.iter().zip(&shares).map(|(&m, &s)| (s, material(m))).collect();
        Shown { shares, leader, made: Some(mix(&members)) }
    }
}

/// A material made from members' materials by their shares: colours added in singles, the
/// numbers in doubles and cut to a single at the end.
pub fn mix(members: &[(f64, MixInput)]) -> MixInput {
    let colour = |pick: &dyn Fn(&MixInput) -> [f32; 3]| -> [f32; 3] {
        let mut acc = [0.0f32; 3];
        for (share, m) in members {
            let c = pick(m);
            for k in 0..3 {
                let prod = c[k] as f64 * *share;
                // The rows decide (README-w22.md): the product is cut to a single
                // before it is added for the first two components, not for the third.
                if k < 2 {
                    acc[k] += prod as f32;
                } else {
                    acc[k] = (acc[k] as f64 + prod) as f32;
                }
            }
        }
        acc
    };
    let number = |pick: &dyn Fn(&MixInput) -> f32| -> f32 {
        let mut acc = 0.0f64;
        for (share, m) in members {
            acc += pick(m) as f64 * *share;
        }
        acc as f32
    };
    MixInput {
        flags: MADE_FLAGS,
        colours: std::array::from_fn(|c| colour(&|m| m.colours[c])),
        values: std::array::from_fn(|v| number(&|m| m.values[v])),
        volume_colour: colour(&|m| m.volume_colour),
        refraction: number(&|m| m.refraction),
        volume_numbers: std::array::from_fn(|v| number(&|m| m.volume_numbers[v])),
    }
}

/// What a hit on a set shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Showing {
    /// The material made for the hit.
    Made(MixInput),
    /// The leader's own material, as it is (index into `Metaballs::balls`).
    Own(usize),
    /// The wood material of the tree whose geometry object this is: the stand-in the trees and
    /// the sets share  was handed it. (One stand-in)
    Wood(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct MetaState {
    /// What the recorded hits of the current camera sample showed (and the latest showing
    /// from before it); a hit names its entry.
    shown: Vec<Showing>,
    /// The entry of the latest recorded hit on any set.
    latest: Option<u32>,
    /// The set of the crossing last found as a hit or kept, for any ray.
    last_set: Vec<usize>,
    /// Which of the two things that cross from one camera sample to the next this run of samples read before it
    /// wrote them, and which it wrote (the many-core render's marks). (Metaball state)
    marks: MetaMarks,
}

/// What a run of camera samples left of the metaballs' record for the next one: the showing the standing object
/// holds, and the set whose depth is read. Everything else in the record is made again at each camera sample.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaCarry {
    pub latest: Option<Showing>,
    pub last_set: Vec<usize>,
}

/// Whether a run of samples read (`asked`: before anything of its own had written it) or wrote each part of
/// [`MetaCarry`]. Only a part that was asked can show what an earlier pixel left; only a part that was written
/// leaves otherwise than it found it. The same two marks the old recomp keeps (`bryce_rt::scene::Left`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetaMarks {
    pub asked_latest: bool,
    pub asked_set: bool,
    pub wrote_latest: bool,
    pub wrote_set: bool,
}

/// The record of what the hits showed, shared by the clones of a `MetaScene` (behind a lock, so
/// that a scene may sit in structures read from several threads).
#[derive(Clone, Debug, Default)]
struct SharedMetaState(std::sync::Arc<std::sync::Mutex<MetaState>>);

impl SharedMetaState {
    fn lock(&self) -> std::sync::MutexGuard<'_, MetaState> {
        self.0.lock().unwrap()
    }
}
impl PartialEq for SharedMetaState {
    fn eq(&self, o: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &o.0) || *self.lock() == *o.lock()
    }
}

/// The scene's metaballs with what the render needs of them: the geometry object of each, the
/// object that stands for every set, and the record of what the hits showed.
/// . (Metaballs)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaScene {
    pub set: Metaballs,
    /// The geometry object (`Shape::Metaball`) of each ball.
    pub object_of: Vec<usize>,
    /// The numbers of each ball's material .
    pub materials: Vec<MixInput>,
    /// The geometry object that stands for every set (`Shape::MetaSet`).
    pub stand: usize,
    state: SharedMetaState,
}

impl MetaScene {
    pub fn new(set: Metaballs, object_of: Vec<usize>, materials: Vec<MixInput>, stand: usize) -> MetaScene {
        MetaScene { set, object_of, materials, stand, state: Default::default() }
    }

    /// Records a hit on the set of `c` (a crossing the walk answers as a hit): what it shows
    /// becomes what the standing object holds. Returns the hit's own entry.
    pub fn record_hit(&self, c: &Crossing) -> u32 {
        let shown = self.set.shown(&c.set, c.point, |m| self.materials[m]);
        let showing = match shown.made {
            Some(m) => Showing::Made(m),
            None => Showing::Own(shown.leader),
        };
        let mut st = self.state.lock();
        let id = Self::push(&mut st, showing);
        st.last_set = c.set.clone();
        st.marks.wrote_set = true;
        id
    }

    /// Names `showing` in the record (the latest entry again where it shows the same) and makes
    /// it what the standing object holds.
    fn push(st: &mut MetaState, showing: Showing) -> u32 {
        // A hit that shows what the latest entry shows names that entry.
        let id = match st.shown.last() {
            Some(s) if *s == showing => st.shown.len() - 1,
            _ => {
                st.shown.push(showing);
                st.shown.len() - 1
            }
        } as u32;
        st.latest = Some(id);
        st.marks.wrote_latest = true;
        id
    }

    /// A piece's crossing of the tree whose object is `tree` was taken : the standing
    /// object is handed that tree's wood material, whatever the tree answers. A crossing of the
    /// wood leaves the set of the depth as it was.
    pub fn hand_over_wood(&self, tree: usize) {
        Self::push(&mut self.state.lock(), Showing::Wood(tree));
    }

    /// A hit on the wood of the tree whose object is `tree` was recorded: the standing object
    /// holds its wood material; returns the hit's own entry.
    pub fn record_wood_hit(&self, tree: usize) -> u32 {
        Self::push(&mut self.state.lock(), Showing::Wood(tree))
    }

    /// The entry of the latest recorded hit on any set.
    pub fn latest_id(&self) -> Option<u32> {
        let mut st = self.state.lock();
        st.marks.asked_latest |= !st.marks.wrote_latest;
        st.latest
    }

    /// A camera sample begins: no hit of an earlier sample is asked about any more, so the
    /// record keeps the latest showing alone (what the standing object holds), as entry 0.
    /// (The record of showings)
    pub fn new_sample(&self) {
        let mut st = self.state.lock();
        let keep = st.latest.map(|i| st.shown[i as usize]);
        st.shown.clear();
        st.latest = keep.map(|s| {
            st.shown.push(s);
            0
        });
    }

    /// A crossing was kept for a ray: its set is the one the depth reads.
    pub fn record_kept(&self, c: &Crossing) {
        let mut st = self.state.lock();
        st.last_set = c.set.clone();
        st.marks.wrote_set = true;
    }

    /// What recorded hit `id` showed.
    pub fn showing(&self, id: u32) -> Showing {
        let mut st = self.state.lock();
        st.marks.asked_latest |= !st.marks.wrote_latest;
        st.shown[id as usize]
    }

    /// What the latest recorded hit on any set showed: the standing object's material now.
    pub fn latest(&self) -> Option<Showing> {
        let mut st = self.state.lock();
        st.marks.asked_latest |= !st.marks.wrote_latest;
        st.latest.map(|i| st.shown[i as usize])
    }

    /// The sum of the fields of the set last found as a hit or kept, at a world point: how deep
    /// the standing object is there .
    pub fn depth_at(&self, p: Vec3) -> f64 {
        let mut st = self.state.lock();
        st.marks.asked_set |= !st.marks.wrote_set;
        self.set.sum(&st.last_set, p)
    }

    /// A copy with a record of its own (the many-core render's workers each follow their own; the clones
    /// of a scene otherwise share one).
    pub fn forked(&self) -> MetaScene {
        let mut s = self.clone();
        s.state = SharedMetaState(std::sync::Arc::new(std::sync::Mutex::new(self.state.lock().clone())));
        s
    }

    /// What the record carries from one camera sample to the next, as it stands.
    pub fn carry(&self) -> MetaCarry {
        let st = self.state.lock();
        MetaCarry { latest: st.latest.map(|i| st.shown[i as usize]), last_set: st.last_set.clone() }
    }

    /// Make `from` the record's state (as a camera sample's start leaves it) and clear the marks.
    pub fn load(&self, from: &MetaCarry) {
        let mut st = self.state.lock();
        st.shown.clear();
        st.latest = from.latest.map(|s| {
            st.shown.push(s);
            0
        });
        st.last_set = from.last_set.clone();
        st.marks = MetaMarks::default();
    }

    /// What was read and written since the last `load`.
    pub fn marks(&self) -> MetaMarks {
        self.state.lock().marks
    }
}

/// The marks and the kept crossing of one ray.
#[derive(Clone, Debug)]
pub struct MetaWalk {
    pub marks: Vec<Mark>,
    /// The one crossing the walk keeps for the ray (the last kept).
    pub kept: Option<Crossing>,
    /// The balls that were marked missed by the last `ask`, for the caller's own lists.
    pub newly_missed: Vec<usize>,
}

impl MetaWalk {
    pub fn new(balls: usize) -> MetaWalk {
        MetaWalk { marks: vec![Mark::None; balls], kept: None, newly_missed: Vec::new() }
    }

    /// Metaball `m0` asked about the ray with loL = `lo`, BL = `bl`; `f` is the walk's F, which
    /// becomes t where a crossing is kept. A hit is returned; the caller makes B and F equal to
    /// its t.
    pub fn ask(&mut self, set_of: &Metaballs, m0: usize, ray: &Ray, lo: f64, bl: f64, f: &mut f64) -> Option<Crossing> {
        self.newly_missed.clear();
        match self.marks[m0] {
            Mark::Done | Mark::Missed => return None,
            Mark::Kept => {
                let k = self.kept.clone()?;
                if lo <= k.t && k.t <= bl {
                    for &m in &k.set {
                        self.marks[m] = Mark::Done;
                    }
                    return Some(k);
                }
                return None;
            }
            Mark::None => {}
        }
        let set = set_of.set(m0, ray, lo, *f);
        if set.is_empty() {
            return None;
        }
        let missed = |w: &mut MetaWalk, set: &[usize]| {
            for &m in set {
                w.marks[m] = Mark::Missed;
                w.newly_missed.push(m);
            }
        };
        let Some(c) = set_of.crossing(&set, ray) else {
            missed(self, &set);
            return None;
        };
        let t = c.t;
        if t < bl {
            if lo < t {
                for &m in &set {
                    self.marks[m] = Mark::Done;
                }
                return Some(c);
            }
            missed(self, &set);
            None
        } else if t <= *f {
            for &m in &set {
                self.marks[m] = Mark::Kept;
            }
            *f = t;
            self.kept = Some(c);
            None
        } else {
            missed(self, &set);
            None
        }
    }
}
