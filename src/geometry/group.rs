// SPDX-License-Identifier: MIT
//! Combined groups: which objects take part, and the list of crossings and the hit of a ray
//! on the one solid a group of members with roles is.
//!
//! : the bits a group
//! reads, which members take part and which groups combine, the solid and its list, the
//! material of a cut, groups in groups, planes and slabs, hidden groups and Bryce 5's rules;
//! (the members' lists, a mesh's and a terrain's included, released/
//! geometry/12-group-members.md),;
//! (Combined groups)

use super::mesh::mesh_list;
use super::structure::{Bounds, group_box_widened, points_box, shape_box};
use super::{Interior, LocalCrossing, Object, Range, Ray, Scene, Shape, ShapeContext, local_crossings};
use crate::geom::Vec3;
use crate::version::RenderAs;

/// Second flag word: positive, the member is solid, part of the union.
pub const POSITIVE: u32 = 0x8;
/// Second flag word: negative, the member is a cutter.
pub const NEGATIVE: u32 = 0x10;
/// Second flag word: intersecting, the member limits the union to its own space.
pub const INTERSECTING: u32 = 0x20;
/// Second flag word, on a negative member: its cut shows the positive member's material.
pub const KEEPS_MATERIAL: u32 = 0x80;
/// Second flag word: excluded, the member takes no part.
pub const EXCLUDED: u32 = 0x4;
/// Second flag word: hidden, the object is not drawn.
pub const HIDDEN: u32 = 0x1 | 0x40_0000;

/// A member's role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Positive,
    Intersecting,
    Negative,
}

impl Role {
    /// The role a word gives: `Ok(None)` with no role bit, `Err(TwoRoleBits)` with two or
    /// three (Open).
    pub fn of_word(word: u32) -> Result<Option<Role>, TwoRoleBits> {
        let roles = [(POSITIVE, Role::Positive), (INTERSECTING, Role::Intersecting), (NEGATIVE, Role::Negative)];
        let mut found = roles.iter().filter(|(bit, _)| word & bit != 0).map(|&(_, r)| r);
        match (found.next(), found.next()) {
            (None, _) => Ok(None),
            (Some(r), None) => Ok(Some(r)),
            _ => Err(TwoRoleBits),
        }
    }
}

/// A word with two or three of the role bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwoRoleBits;

/// What a member that takes part is, as its combination holds it.
/// (Combined groups, Types)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A geometry object (index into `Scene::objects`) drawn only through its group.
    Object(usize),
    /// An inner group: index into `Groups::combinations`.
    Group(usize),
    /// A hidden member: no crossings, and it still counts in its role; its geometry object (built,
    /// never asked) where it is not a group, for the box it adds to its group's walk box.
    Hidden(Option<usize>),
}

/// A member that takes part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub role: Role,
    /// Bit 0x80 (read on a negative member).
    pub keeps_material: bool,
    pub source: Source,
}

/// A group that combines: its members that take part, in the order of the group's rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Combination {
    pub members: Vec<Member>,
}

/// The scene's combinations, and the objects drawn only through them.
/// (Combined groups, Types)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    pub combinations: Vec<Combination>,
    /// Geometry objects that are members of a combination: never met as themselves. Sorted.
    through: Vec<usize>,
}

impl Groups {
    /// Adds a combination; `through` are its own `Source::Object` members' indices.
    pub fn push(&mut self, c: Combination) -> usize {
        for m in &c.members {
            if let Source::Object(i) | Source::Hidden(Some(i)) = m.source
                && let Err(at) = self.through.binary_search(&i)
            {
                self.through.insert(at, i);
            }
        }
        self.combinations.push(c);
        self.combinations.len() - 1
    }

    /// Whether geometry object `i` is drawn only through a group. ("a member that
    /// takes part is drawn only through its group")
    pub fn is_through(&self, i: usize) -> bool {
        self.through.binary_search(&i).is_ok()
    }
}

// ---------------------------------------------------------------------------------------
// Who takes part. (Which members take part, and which groups combine; Worked out
// when a render starts; Hidden groups, bit 0x4 and Bryce 5)
// ---------------------------------------------------------------------------------------

/// What kind of object a row is, as far as a group reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A sphere, cube, cylinder, cone, torus, plane, slab, square or disc .
    Shape,
    /// A group.
    Group,
    /// A path or a camera; as 5.0 a metaball or a tree: never takes part.
    Never,
    /// A kind that takes part as the shapes do but whose lists are out of scope here (a mesh,
    /// a terrain, a pyramid, a light's own shape; a metaball or a tree as 3.0 and 3.1).
    OutOfScope,
    /// A light: its own object takes part as the shapes do, by's lists and marks; its
    /// word also holds its switches , but 0x1 and 0x400000 hide it as any object
    /// (lights held by a group, revised) and, as 5.0, a hidden group above it hides it.
    /// . (Who takes part)
    Light,
}

/// One object as the resolver reads it. (Who takes part)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The second flag word (offset 8).
    pub word: u32,
    pub kind: Kind,
    /// The group (a row index) whose list holds it; `None` in the root list or where the
    /// object it is linked to is not a group.
    pub parent: Option<usize>,
    /// The Solo mode's mark: bit 0x10 of the object's first flag word (any object). As 3.1
    /// and 5.0 the object is drawn as if its bit 0x1 were clear; as 3.0 it changes nothing.
    pub solo: bool,
}

/// Why a scene's groups are not drawn: an Open case of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupOpen {
    /// A member with two or three role bits and 0x4 clear: no longer raised (revised:
    /// it goes to the list of the member before it).
    TwoRoles(usize),
    /// A member whose lists are out of scope takes part in a group that combines.
    OutOfScope(usize),
}

/// The fate of each row. (Who takes part)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub hidden: Vec<bool>,
    pub takes_part: Vec<bool>,
    pub combines: Vec<bool>,
    pub roles: Vec<Option<Role>>,
    parents: Vec<Option<usize>>,
    kinds: Vec<Kind>,
    words: Vec<u32>,
}

/// Works out from the rows alone which members take part and which groups combine, and which
/// objects are hidden, as `version` does.
pub fn resolve(rows: &[Row], version: RenderAs) -> Result<Resolution, GroupOpen> {
    let n = rows.len();
    let five = version == RenderAs::Bryce50;
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, r) in rows.iter().enumerate() {
        if let Some(p) = r.parent {
            members[p].push(i);
        }
    }
    // : a member with two or three role bits goes
    // to the list of the member before it in the group's order (a first member: positive).
    let mut roles = vec![None; n];
    for group in &members {
        let mut before = Role::Positive;
        for &i in group {
            let r = &rows[i];
            if r.word & EXCLUDED != 0 {
                continue;
            }
            roles[i] = match Role::of_word(r.word) {
                Ok(role) => role,
                Err(TwoRoleBits) => Some(before),
            };
            if let Some(role) = roles[i] {
                before = role;
            }
        }
    }
    // combines(g) needs takes_part of its members, which for a group needs combines: memo.
    let mut combines: Vec<Option<bool>> = vec![None; n];
    fn combines_of(
        g: usize,
        rows: &[Row],
        members: &[Vec<usize>],
        roles: &[Option<Role>],
        memo: &mut Vec<Option<bool>>,
        depth: usize,
    ) -> bool {
        if let Some(c) = memo[g] {
            return c;
        }
        // A cycle of groups cannot come from a document's lists (every parent is an
        // earlier object); guard anyway.
        if depth > rows.len() {
            return false;
        }
        let c = members[g].iter().any(|&m| {
            roles[m] == Some(Role::Positive) && takes_part_of(m, rows, members, roles, memo, depth + 1)
        });
        memo[g] = Some(c);
        c
    }
    fn takes_part_of(
        m: usize,
        rows: &[Row],
        members: &[Vec<usize>],
        roles: &[Option<Role>],
        memo: &mut Vec<Option<bool>>,
        depth: usize,
    ) -> bool {
        if roles[m].is_none() {
            return false;
        }
        match rows[m].kind {
            Kind::Shape | Kind::OutOfScope | Kind::Light => true,
            Kind::Group => combines_of(m, rows, members, roles, memo, depth),
            Kind::Never => false,
        }
    }
    let takes_part: Vec<bool> = (0..n).map(|i| takes_part_of(i, rows, &members, &roles, &mut combines, 0)).collect();
    let combines: Vec<bool> = (0..n)
        .map(|g| rows[g].kind == Kind::Group && combines_of(g, rows, &members, &roles, &mut combines, 0))
        .collect();
    for i in 0..n {
        let in_combination = takes_part[i] && rows[i].parent.is_some_and(|p| combines[p]);
        if in_combination && rows[i].kind == Kind::OutOfScope {
            return Err(GroupOpen::OutOfScope(i));
        }
    }
    // (hidden: the object's own 0x1 or 0x400000; as 5.0 a hidden group hides
    // every member without 0x4 at any depth below it through groups)
    // : as 3.1 and 5.0 a marked object counts as one without
    // bit 0x1 (bit 0x400000 still hides it); as 3.0 the mark changes nothing.
    let own_hidden = |i: usize| {
        let word = rows[i].word;
        let word = if rows[i].solo && version != RenderAs::Bryce30 { word & !0x1 } else { word };
        // : on a group only bit 0x1 hides it; bit 0x400000
        // on a group changes nothing, in every version.
        if rows[i].kind == Kind::Group { word & 0x1 != 0 } else { word & HIDDEN != 0 }
    };
    let hidden = (0..n)
        .map(|i| {
            if own_hidden(i) {
                return true;
            }
            if !five || rows[i].word & EXCLUDED != 0 {
                return false;
            }
            let mut p = rows[i].parent;
            let mut steps = 0;
            while let Some(g) = p {
                if own_hidden(g) {
                    return true;
                }
                p = rows[g].parent;
                steps += 1;
                if steps > n {
                    break;
                }
            }
            false
        })
        .collect();
    Ok(Resolution {
        hidden,
        takes_part,
        combines,
        roles,
        parents: rows.iter().map(|r| r.parent).collect(),
        kinds: rows.iter().map(|r| r.kind).collect(),
        words: rows.iter().map(|r| r.word).collect(),
    })
}

/// How a row is drawn. (Who takes part)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// Met as itself (for a group: not an object, its members drawn each by its own fate).
    Alone,
    /// A group drawn as one solid: a combination of the scene.
    Combined,
    /// A member taking part in a group that combines: drawn only through it.
    Through,
    /// Not met: hidden.
    Hidden,
}

impl Resolution {
    /// The fate of row `i`.
    pub fn fate(&self, i: usize) -> Fate {
        let through = self.takes_part[i] && self.parents[i].is_some_and(|p| self.combines[p]);
        if through {
            Fate::Through
        } else if self.hidden[i] {
            Fate::Hidden
        } else if self.kinds[i] == Kind::Group && self.combines[i] {
            Fate::Combined
        } else {
            Fate::Alone
        }
    }

    /// Builds the combination of group `g` (and of the groups inside it that take part) into
    /// `groups`; `object` gives the geometry object of a shape member (it is called once for
    /// each member that is drawn, in row order).
    pub fn build<E>(
        &self,
        g: usize,
        groups: &mut Groups,
        object: &mut impl FnMut(usize) -> Result<usize, E>,
    ) -> Result<usize, E> {
        let mut members = Vec::new();
        for m in (0..self.parents.len()).filter(|&m| self.parents[m] == Some(g) && self.takes_part[m]) {
            let role = self.roles[m].expect("a member that takes part has a role");
            // A hidden member that is not a group is built too (never asked), for its box; one
            // whose object cannot be built adds none (our fallback).
            let source = if self.hidden[m] {
                Source::Hidden(if self.kinds[m] == Kind::Group { None } else { object(m).ok() })
            } else if self.kinds[m] == Kind::Group {
                Source::Group(self.build(m, groups, object)?)
            } else {
                Source::Object(object(m)?)
            };
            members.push(Member { role, keeps_material: self.words[m] & KEEPS_MATERIAL != 0, source });
        }
        Ok(groups.push(Combination { members }))
    }
}

// ---------------------------------------------------------------------------------------
// The list.
// ---------------------------------------------------------------------------------------

/// One entry of a group's list: a crossing of a member (`surface`, a geometry object) in that
/// member's own space, the member whose material it shows (a geometry object), the index in
/// the combination's members of the member that gave it, and its place in that member's list
/// (counted from 1: odd enters).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupEntry {
    pub crossing: LocalCrossing,
    pub surface: usize,
    pub material: usize,
    /// The member a ray that goes into the solid through this crossing travels in the material
    /// of, as 5.0: the surface member itself, except at a cut of a negative member and at the
    /// kept surface of an intersecting member, where it is the member that made the last
    /// entering entry of the positives before the crossing (with or without bit 0x80).
    /// . (The medium of a cut)
    pub carried: usize,
    pub member: usize,
    pub place: usize,
}

impl GroupEntry {
    pub fn t(&self) -> f64 {
        self.crossing.hit.t
    }

    fn enters(&self) -> bool {
        self.place % 2 == 1
    }
}

/// Inserts `e` before the first entry of `seq` whose t is not below its own.
fn insert_before_not_below(seq: &mut Vec<GroupEntry>, e: GroupEntry) {
    let at = seq.iter().position(|x| !(x.t() < e.t())).unwrap_or(seq.len());
    seq.insert(at, e);
}

/// Adds the entries of `extra` to `r` one at a time from the nearest, each placed before every
/// entry already in the result whose t is not below its own: of two tied entries of `extra` the
/// later stands first, and both stand before the entries of `r` at that distance. Each output
/// item says whether it came from `extra`.
fn merge(r: Vec<GroupEntry>, extra: &[GroupEntry]) -> Vec<(GroupEntry, bool)> {
    let mut out: Vec<(GroupEntry, bool)> = r.into_iter().map(|y| (y, false)).collect();
    for &x in extra {
        let at = out.iter().position(|(y, _)| !(y.t() < x.t())).unwrap_or(out.len());
        out.insert(at, (x, true));
    }
    out
}

/// The union sweep of one role's sequence: the count c.
fn union_sweep(seq: Vec<GroupEntry>) -> Vec<GroupEntry> {
    // : a leaving entry met at count 0 makes the count wrap as an
    // unsigned number; that entry and the next entering one are dropped.
    let mut c: u32 = 0;
    let mut kept = Vec::with_capacity(seq.len());
    for e in seq {
        let keep = if e.enters() { c == 0 } else { c == 1 };
        if keep {
            kept.push(e);
        }
        c = if e.enters() { c.wrapping_add(1) } else { c.wrapping_sub(1) };
    }
    kept
}

/// `shape_box` remembered per thread: the box is a pure function of (shape, version, placement),
/// and a group asks for each member's box on every ray; carrying a sphere's 600 points each time
/// was a third of a grouped scene's render. A hit must match every input bit for bit.
fn memo_shape_box(shape: Shape, version: crate::version::RenderAs, rows: &[Vec3; 3], t: Vec3) -> Option<Bounds> {
    type Entry = (Shape, crate::version::RenderAs, [u64; 12], Option<Bounds>);
    thread_local! {
        static MEMO: std::cell::RefCell<Vec<Option<Entry>>> = std::cell::RefCell::new(vec![None; 1024]);
    }
    let mut key = [0u64; 12];
    for (i, v) in rows.iter().flatten().chain(t.iter()).enumerate() {
        key[i] = v.to_bits();
    }
    let h = key.iter().fold(0u64, |h, k| (h ^ k).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23));
    let slot = (h >> 40) as usize % 1024;
    let hit = MEMO.with(|m| match &m.borrow()[slot] {
        Some((s, v, k, b)) if *s == shape && *v == version && *k == key => Some(*b),
        _ => None,
    });
    if let Some(b) = hit {
        return b;
    }
    let b = shape_box(shape, version, rows, t);
    MEMO.with(|m| m.borrow_mut()[slot] = Some((shape, version, key, b)));
    b
}

impl Scene {
    /// The box a group tests a member's world ray against, not widened (code, the three
    /// programs):'s points of its shape carried to the world; a mesh's stored box
    /// carried by the matrix; a terrain's as `Relief::place_member` made it (by
    /// version); a disc's the 24 points (sin a, 0, cos a), a = 15 degrees k; a light's object the
    /// box of's points of its shape (the radial light the sphere's, the spot light the
    /// cone's, the parallel light the cube's corners). `None`: no box, the member always
    /// contributes (a plane, a slab, a light with bit 0x2000). (the member's box
    /// test),
    fn member_box(&self, o: &Object, version: crate::version::RenderAs) -> Option<Bounds> {
        let (rows, t) = (o.placement.rows(), o.placement.translation());
        match o.shape {
            Shape::StoredMesh(k) => {
                let b = self.stored_meshes.get(k)?.bounds;
                let mut out = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
                for c in 0..8 {
                    let p = [
                        if c & 4 == 0 { b[0] } else { b[3] },
                        if c & 2 == 0 { b[1] } else { b[4] },
                        if c & 1 == 0 { b[2] } else { b[5] },
                    ];
                    let w = o.placement.to_world(p);
                    for a in 0..3 {
                        out.min[a] = out.min[a].min(w[a]);
                        out.max[a] = out.max[a].max(w[a]);
                    }
                }
                Some(out)
            }
            Shape::Terrain(k) => {
                let (min, max) = self.terrains.get(k)?.member_world_box()?;
                Some(Bounds { min, max })
            }
            // : the cone's rim of moved to y = 0, the same arithmetic
            Shape::Disc => {
                static RIM: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
                let points = RIM.get_or_init(|| {
                    let step = |k: usize| k as f64 * std::f64::consts::PI / 12.0; // 15 degrees k
                    (0..24).map(|k| [step(k).sin(), 0.0, step(k).cos()]).collect()
                });
                Some(points_box(points, &rows, t))
            }
            // : a light with bit 0x2000 has no box; any other its shape's points
            Shape::Light(l) => {
                if l.endless() {
                    return None;
                }
                // : the light's own box points, as 3.0 with the point
                // below the object , as 3.1 and 5.0 without
                Some(points_box(&l.box_points(version), &rows, t))
            }
            Shape::Torus { .. } => shape_box(o.shape, version, &rows, t),
            shape => memo_shape_box(shape, version, &rows, t),
        }
    }

    /// The box a member adds to its group's walk box: as its box test takes it (`member_box`).
    /// `None`: no box (a plane, a slab, a light with bit 0x2000). gives a plane and a slab
    /// a box of +-10 as 3.1, but that is the plane's world box (scene bounds), not what the walk
    /// lists: the original's scene preparation sets a group's bit 0x40000000 when any member (at
    /// any depth) answers "no box test" (plane, slab), and the tree's add routine sends every
    /// object answering that to the list asked on every ray, in 3.0, 3.1 and 5.0 alike (question
    /// 0626, answered 2026-10-22; groups.jsonl plane-hole-013 as 3.1, a hit at x = -14, agrees).
    fn walk_member_box(&self, o: &Object, version: crate::version::RenderAs) -> Option<Bounds> {
        self.member_box(o, version)
    }

    /// The box by which the walk of lists combination `k`, before its widenings: the union
    /// of the boxes of the members that take part other than the negative ones, hidden or not
    /// (`walk_member_box`; a group inside: its own union widened as the walk widens it). `None`
    /// where a member adds no box: the group stays unlisted by a box and is asked first (our
    /// choice, as before). A hidden inner group adds nothing (its members are not built: our
    /// fallback). (How a combined group is asked in the walk of, revised log
    /// 18497)
    pub(crate) fn group_walk_box(&self, k: usize, version: crate::version::RenderAs) -> Option<Bounds> {
        let mut union: Option<Bounds> = None;
        for m in &self.groups.combinations[k].members {
            if m.role == Role::Negative {
                continue;
            }
            let b = match m.source {
                Source::Hidden(None) => continue,
                Source::Group(g) => group_box_widened(self.group_walk_box(g, version)?, version),
                Source::Object(i) | Source::Hidden(Some(i)) => self.walk_member_box(&self.objects[i], version)?,
            };
            union = Some(match union {
                None => b,
                Some(u) => Bounds {
                    min: std::array::from_fn(|a| u.min[a].min(b.min[a])),
                    max: std::array::from_fn(|a| u.max[a].max(b.max[a])),
                },
            });
        }
        union
    }

    /// The box an inner group is tested by as a member: its walk box, widened, or `None` where it
    /// holds, at any depth through groups that take part, a member with no box of its test (it
    /// always contributes).
    fn inner_group_box(&self, k: usize, version: crate::version::RenderAs) -> Option<Bounds> {
        let mut union: Option<Bounds> = None;
        for m in &self.groups.combinations[k].members {
            let b = match m.source {
                Source::Group(g) => {
                    let inner = self.inner_group_box(g, version)?;
                    (m.role != Role::Negative).then_some(inner)
                }
                Source::Object(i) | Source::Hidden(Some(i)) => {
                    let b = self.member_box(&self.objects[i], version)?;
                    (m.role != Role::Negative).then_some(b)
                }
                Source::Hidden(None) => None,
            };
            if let Some(b) = b {
                union = Some(match union {
                    None => b,
                    Some(u) => Bounds {
                        min: std::array::from_fn(|a| u.min[a].min(b.min[a])),
                        max: std::array::from_fn(|a| u.max[a].max(b.max[a])),
                    },
                });
            }
        }
        union.map(|b| group_box_widened(b, version))
    }

    /// 's list of a member in its own space: the shape's list; a document mesh's
    /// and a terrain's  as a group reads them. `world` is the ray as
    /// given in the world.
    pub(crate) fn member_crossings(&self, shape: Shape, local: &Ray, world: &Ray, ctx: &ShapeContext) -> Vec<LocalCrossing> {
        match shape {
            Shape::StoredMesh(k) => match self.stored_meshes.get(k) {
                Some(m) => mesh_list(m, local, m.solid_in_group, world.dir, ctx.version),
                None => Vec::new(),
            },
            Shape::Terrain(k) => self.terrains.get(k).map_or_else(Vec::new, |t| t.list(local)),
            other => local_crossings(other, local, ctx),
        }
    }

    /// The list of crossings of member `member` of combination `k` for a world ray, in the
    /// member's own space , before the group reads it: the
    /// function-level view of a member (the box test of the group is not applied). Empty for a
    /// member that is not an object.
    pub fn member_list(&self, k: usize, member: usize, ray: &Ray, ctx: &ShapeContext) -> Vec<LocalCrossing> {
        let Source::Object(i) = self.groups.combinations[k].members[member].source else { return Vec::new() };
        let o = &self.objects[i];
        let local = o.placement.to_local_as(ctx.version, ray);
        self.member_crossings(o.shape, &local, ray, ctx)
    }

    /// The list of combination `k` for the world ray, nearest first:'s R. `range` is lo
    /// and hi of the members' box test.
    pub fn group_list(&self, k: usize, ray: &Ray, range: Range, ctx: &ShapeContext) -> Vec<GroupEntry> {
        let comb = &self.groups.combinations[k];
        let mut seqs: [Vec<GroupEntry>; 3] = Default::default();
        for (index, m) in comb.members.iter().enumerate() {
            // The member's entries are placed straight into its role's sequence: no list of
            // its own is built (a Vec per member per ray was a tenth of a grouped scene's time).
            let seq = &mut seqs[m.role as usize];
            match m.source {
                Source::Hidden(_) => {}
                Source::Group(g) => {
                    if let Some(b) = self.inner_group_box(g, ctx.version)
                        && !member_box_met(&b, ray, range.near, range.far)
                    {
                        continue;
                    }
                    let list = self.group_list(g, ray, range, ctx);
                    if list.len() < 2 {
                        continue;
                    }
                    for (j, e) in list.into_iter().enumerate() {
                        insert_before_not_below(seq, GroupEntry { member: index, place: j + 1, ..e });
                    }
                }
                Source::Object(i) => {
                    let o = &self.objects[i];
                    if let Some(b) = self.member_box(o, ctx.version)
                        && !member_box_met(&b, ray, range.near, range.far)
                    {
                        continue;
                    }
                    let local = o.placement.to_local_as(ctx.version, ray);
                    let crossings = self.member_crossings(o.shape, &local, ray, ctx);
                    if crossings.len() < 2 {
                        continue;
                    }
                    for (j, crossing) in crossings.into_iter().enumerate() {
                        let e = GroupEntry { crossing, surface: i, material: i, carried: i, member: index, place: j + 1 };
                        insert_before_not_below(seq, e);
                    }
                }
            }
        }
        let [mut p, i, n] = seqs.map(union_sweep);
        // step 1
        if p.is_empty() {
            return Vec::new();
        }
        let has_i = comb.members.iter().any(|m| m.role == Role::Intersecting);
        let has_n = comb.members.iter().any(|m| m.role == Role::Negative);
        // p is read again only by steps 2 and 3, so it is moved into r when neither runs.
        let mut r = if has_i || has_n { p.clone() } else { std::mem::take(&mut p) };
        // step 2 (an intersecting member that takes part, crossed or not)
        if has_i {
            let (mut a, mut b) = (false, false);
            let mut kept = Vec::new();
            for (e, from_i) in merge(r, &i) {
                if from_i {
                    if a {
                        let mut kept_entry = e;
                        kept_entry.carried = carried_member(&p, &e);
                        kept.push(kept_entry);
                    }
                    b = e.enters();
                } else {
                    if b {
                        kept.push(e);
                    }
                    a = e.enters();
                }
            }
            r = kept;
        }
        // step 3 (a negative member that takes part, crossed or not)
        if has_n {
            let (mut a, mut b) = (false, false);
            let mut kept = Vec::new();
            for (e, from_n) in merge(r, &n) {
                if from_n {
                    if a {
                        let mut cut = e;
                        cut.carried = carried_member(&p, &cut);
                        if comb.members[cut.member].keeps_material {
                            cut.material = kept_material(&p, &cut);
                        }
                        kept.push(cut);
                    }
                    b = e.enters();
                } else {
                    if !b {
                        kept.push(e);
                    }
                    a = e.enters();
                }
            }
            r = kept;
        }
        r
    }

    /// The bounds in y of the box, in the space of the group placed by `group`, that holds the
    /// boxes of combination `k`'s members (negative and intersecting ones included, a group
    /// inside by its own members; each member's box carried to the world and from there
    /// into the group's space), with the group's own origin where `origin` is set, then widened
    /// at each end: as 3.0 by 1/100 of the extent, as 3.1 and 5.0 by 0.00001. `None` where no
    /// member has a box.
    pub fn medium_extent_y(
        &self,
        k: usize,
        group: &crate::geometry::Placement,
        origin: bool,
        version: crate::version::RenderAs,
    ) -> Option<(f64, f64)> {
        let mut points: Vec<Vec3> = Vec::new();
        self.collect_member_points(k, version, &mut points);
        if origin {
            points.push([0.0; 3]);
        } else if points.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for (n, p) in points.iter().enumerate() {
            // the group's own origin is a point of the group's space already
            let y = if origin && n == points.len() - 1 { 0.0 } else { group.point_to_local(*p)[1] };
            lo = lo.min(y);
            hi = hi.max(y);
        }
        if !(lo <= hi) {
            return None;
        }
        let d = match version {
            crate::version::RenderAs::Bryce30 => (hi - lo) * 0.01,
            _ => 0.00001,
        };
        Some((lo - d, hi + d))
    }

    /// The world points of the boxes of combination `k`'s members (the corners of a stored
    /// mesh's box, of a terrain's member box,'s points of a shape).
    fn collect_member_points(&self, k: usize, version: crate::version::RenderAs, out: &mut Vec<Vec3>) {
        for m in &self.groups.combinations[k].members {
            match m.source {
                Source::Hidden(_) => {}
                Source::Group(g) => self.collect_member_points(g, version, out),
                Source::Object(i) => {
                    let o = &self.objects[i];
                    match o.shape {
                        Shape::StoredMesh(_) | Shape::Terrain(_) => {
                            if let Some(b) = self.member_box(o, version) {
                                for c in 0..8 {
                                    out.push([
                                        if c & 4 == 0 { b.min[0] } else { b.max[0] },
                                        if c & 2 == 0 { b.min[1] } else { b.max[1] },
                                        if c & 1 == 0 { b.min[2] } else { b.max[2] },
                                    ]);
                                }
                            }
                        }
                        shape => {
                            if let Some(points) = super::structure::shape_points(shape, version) {
                                out.extend(points.into_iter().map(|p| o.placement.to_world(p)));
                            }
                        }
                    }
                }
            }
        }
    }

    /// The hit of combination `k`: the first entry that shows with lo < t < hi.
    pub fn group_hit(&self, k: usize, ray: &Ray, range: Range, ctx: &ShapeContext) -> Option<GroupEntry> {
        self.group_list(k, ray, range, ctx).into_iter().find(|e| e.crossing.shows && range.holds(e.t()))
    }

    /// Where the world point `p` lies against combination `k`'s solid, from its members' own
    /// tests: on the surface of any member that gives crossings, on the surface; inside where
    /// it is inside a positive member, inside an intersecting one (if any takes part) and
    /// inside no negative one. (The eye inside a group)
    pub fn group_interior(&self, k: usize, p: Vec3) -> Option<Interior> {
        let comb = &self.groups.combinations[k];
        let mut inside = [false; 3];
        for m in &comb.members {
            let at = match m.source {
                Source::Hidden(_) => Interior::Outside,
                Source::Group(g) => self.group_interior(g, p)?,
                Source::Object(i) => self.interior_of(i, p)?,
            };
            match at {
                Interior::OnSurface => return Some(Interior::OnSurface),
                Interior::Inside => inside[m.role as usize] = true,
                Interior::Outside => {}
            }
        }
        let any_i = comb.members.iter().any(|m| m.role == Role::Intersecting);
        let held = inside[Role::Positive as usize] && (!any_i || inside[Role::Intersecting as usize]) && !inside[Role::Negative as usize];
        Some(if held { Interior::Inside } else { Interior::Outside })
    }
}

/// The member a ray going in through a kept cut or a kept intersecting surface travels in the
/// material of as 5.0: that of the last entering entry of U_P before the crossing (its own
/// carried member, for an entry of an inner group); the crossing's own where there is none.
fn carried_member(u_p: &[GroupEntry], at: &GroupEntry) -> usize {
    u_p.iter().take_while(|e| e.t() < at.t()).filter(|e| e.enters()).last().map_or(at.carried, |e| e.carried)
}

/// A group's box test of a member, in every version: on each axis the distances (m - o) and
/// (M - o) each times the quotient 1/d (rounded once), or where d is zero of either sign times
/// 16,776,704.001; the greater is the far one; a far one not above 0 misses; the box is met where
/// near <= far, near <= hi and lo <= far (near the greatest of the near ones, far the least of
/// the far ones).
fn member_box_met(b: &Bounds, ray: &Ray, lo: f64, hi: f64) -> bool {
    const ZERO_DIRECTION_FACTOR: f64 = 16776704.001;
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for a in 0..3 {
        let (o, d) = (ray.origin[a], ray.dir[a]);
        let k = if d == 0.0 { ZERO_DIRECTION_FACTOR } else { 1.0 / d };
        let (t1, t2) = ((b.min[a] - o) * k, (b.max[a] - o) * k);
        let (n, f) = if t1 > t2 { (t2, t1) } else { (t1, t2) };
        if !(f > 0.0) {
            return false;
        }
        near = near.max(n);
        far = far.min(f);
    }
    near <= far && near <= hi && lo <= far
}

/// The material of a kept cut of a negative member with bit 0x80: that of the last entering
/// entry of U_P (as before steps 2 and 3) that comes before the cut, the cut going before U_P's
/// entries of its own distance; the cutter's own where there is none.
fn kept_material(u_p: &[GroupEntry], cut: &GroupEntry) -> usize {
    u_p.iter().take_while(|e| e.t() < cut.t()).filter(|e| e.enters()).last().map_or(cut.material, |e| e.material)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{LocalHit, Part};

    fn entry(t: f64, place: usize, surface: usize) -> GroupEntry {
        let hit = LocalHit::new(t, [0.0; 3], [0.0; 3], Part::Curved);
        GroupEntry {
            crossing: LocalCrossing { hit, shows: true, unit_order: crate::geometry::crossings::SumOrder::YzFirst },
            surface,
            material: surface,
            carried: surface,
            member: surface,
            place,
        }
    }

    /// Of two entries at one distance, the one added later goes first.
    #[test]
    fn later_first_at_one_distance() {
        let mut seq = vec![entry(1.0, 1, 0), entry(2.0, 2, 0)];
        insert_before_not_below(&mut seq, entry(2.0, 1, 1));
        assert_eq!(seq.iter().map(|e| e.surface).collect::<Vec<_>>(), [0, 1, 0]);
    }

    /// Two overlapping members of one role: the boundary of the union.
    #[test]
    fn union_of_two() {
        let mut seq = Vec::new();
        for e in [entry(1.0, 1, 0), entry(3.0, 2, 0)] {
            insert_before_not_below(&mut seq, e);
        }
        for e in [entry(2.0, 1, 1), entry(4.0, 2, 1)] {
            insert_before_not_below(&mut seq, e);
        }
        let kept = union_sweep(seq);
        assert_eq!(kept.iter().map(|e| e.t()).collect::<Vec<_>>(), [1.0, 4.0]);
    }

    /// The role bits, one at a time; two are Open.
    #[test]
    fn roles_of_words() {
        assert_eq!(Role::of_word(0x8), Ok(Some(Role::Positive)));
        assert_eq!(Role::of_word(0x10 | 0x80), Ok(Some(Role::Negative)));
        assert_eq!(Role::of_word(0x20), Ok(Some(Role::Intersecting)));
        assert_eq!(Role::of_word(0x4), Ok(None));
        assert!(Role::of_word(0x18).is_err());
    }

    /// A group of negative members only does not combine: they are met as themselves.
    #[test]
    fn negatives_alone_do_not_combine() {
        let rows = [
            Row { word: 0x10, kind: Kind::Shape, parent: Some(2), solo: false },
            Row { word: 0x20, kind: Kind::Shape, parent: Some(2), solo: false },
            Row { word: 0, kind: Kind::Group, parent: None, solo: false },
        ];
        let r = resolve(&rows, RenderAs::Bryce31).unwrap();
        assert_eq!([r.fate(0), r.fate(1), r.fate(2)], [Fate::Alone, Fate::Alone, Fate::Alone]);
    }

    /// As 5.0 a hidden group hides its members without 0x4; as 3.x only what takes part.
    #[test]
    fn hidden_group_by_version() {
        let rows = [
            Row { word: 0x8, kind: Kind::Shape, parent: Some(3), solo: false },
            Row { word: 0, kind: Kind::Shape, parent: Some(3), solo: false },
            Row { word: 0x4, kind: Kind::Shape, parent: Some(3), solo: false },
            Row { word: 0x1, kind: Kind::Group, parent: None, solo: false },
        ];
        let r = resolve(&rows, RenderAs::Bryce30).unwrap();
        assert_eq!([r.fate(0), r.fate(1), r.fate(2), r.fate(3)], [Fate::Through, Fate::Alone, Fate::Alone, Fate::Hidden]);
        let r = resolve(&rows, RenderAs::Bryce50).unwrap();
        assert_eq!([r.fate(0), r.fate(1), r.fate(2), r.fate(3)], [Fate::Through, Fate::Hidden, Fate::Alone, Fate::Hidden]);
    }

    /// The Solo mode's mark: as 3.1 and 5.0 an object counts as one without bit 0x1, as 3.0 it
    /// changes nothing; a hidden group with the mark combines and hides none of its members; a
    /// marked member of an unmarked hidden group is hidden as 5.0; bit 0x400000 still hides.
    #[test]
    fn solo_mark_by_version() {
        let marked_group = |word: u32| {
            [
                Row { word: 0x8, kind: Kind::Shape, parent: Some(2), solo: false },
                Row { word: 0, kind: Kind::Shape, parent: Some(2), solo: false },
                Row { word: word, kind: Kind::Group, parent: None, solo: true },
            ]
        };
        let rows = marked_group(0x1);
        let r = resolve(&rows, RenderAs::Bryce30).unwrap();
        assert_eq!([r.fate(0), r.fate(1), r.fate(2)], [Fate::Through, Fate::Alone, Fate::Hidden]);
        for v in [RenderAs::Bryce31, RenderAs::Bryce50] {
            let r = resolve(&rows, v).unwrap();
            assert_eq!([r.fate(0), r.fate(1), r.fate(2)], [Fate::Through, Fate::Alone, Fate::Combined], "{v:?}");
        }
        // bit 0x400000 on a group changes nothing (revised), marked or not
        let rows = marked_group(0x40_0000);
        let r = resolve(&rows, RenderAs::Bryce31).unwrap();
        assert_eq!(r.fate(2), Fate::Combined);
        // a hidden member with the mark counts in its role as one that is not hidden
        let rows = [
            Row { word: 0x8 | 0x1, kind: Kind::Shape, parent: Some(1), solo: true },
            Row { word: 0, kind: Kind::Group, parent: None, solo: false },
        ];
        assert!(!resolve(&rows, RenderAs::Bryce31).unwrap().hidden[0]);
        assert!(resolve(&rows, RenderAs::Bryce30).unwrap().hidden[0]);
        // a marked member of an unmarked hidden group: hidden as 5.0, as any member
        let rows = [
            Row { word: 0, kind: Kind::Shape, parent: Some(1), solo: true },
            Row { word: 0x1, kind: Kind::Group, parent: None, solo: false },
        ];
        assert!(resolve(&rows, RenderAs::Bryce50).unwrap().hidden[0]);
        assert!(!resolve(&rows, RenderAs::Bryce31).unwrap().hidden[0]);
    }
}
