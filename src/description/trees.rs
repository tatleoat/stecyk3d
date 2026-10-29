// SPDX-License-Identifier: MIT
//! A tree of the Blender description, grown from its settings into a stored tree.
//!
//! , (the
//! settings by the testing interface's tree case names, which the add-on writes),
//! ("A new tree":
//! the deepest level of branching 25)
//! (The reader takes trees, terrains, groups, ...)

use super::{DescriptionError, Rd, Tables, doubles_of, err, f64_of, member, u32_of};
use crate::cases::json::Value;
use crate::geom::Vec3;
use crate::scene_records::Transform;
use crate::scene_trees::{GrowthSettings, Leaf, LeafPlace, LeafShape, StoredTree, WoodPiece};
use crate::trees::{GrownTree, GrowthInputs, LeafRun, Placement};

/// "A new tree": the deepest level of branching a tree made new has. Growth reads the setting and
/// does not use it (Inputs), so a description that does not give it takes this.
const NEW_TREE_DEEPEST_LEVEL: u32 = 25;

fn setting<'a>(t: &'a Value, key: &str, tree: &str) -> Rd<&'a Value> {
    member(t, key).ok_or_else(|| DescriptionError(format!("tree {tree:?} has no setting {key:?}")))
}

fn word_of(t: &Value, key: &str, tree: &str) -> Rd<u32> {
    u32_of(setting(t, key, tree)?).map_err(|e| DescriptionError(format!("tree {tree:?}'s {key}: {e}")))
}

fn double_of(t: &Value, key: &str, tree: &str) -> Rd<f64> {
    let x = f64_of(setting(t, key, tree)?).map_err(|e| DescriptionError(format!("tree {tree:?}'s {key}: {e}")))?;
    if !x.is_finite() {
        return err(format!("tree {tree:?}'s {key} is not finite"));
    }
    Ok(x)
}

fn pair_of(t: &Value, key: &str, tree: &str) -> Rd<[f64; 2]> {
    let d = doubles_of(setting(t, key, tree)?, 2, &format!("tree {tree:?}'s {key}"))?;
    Ok([d[0], d[1]])
}

/// 's settings from the description's `tree` (the tree case's names: `seed`, `trunk-heights`,
/// `trunk-branches`, `branch-branches`, `angles`, `shares`, `leaves`, `leaf-kind`, `outline` with `O0` to `O3`,
/// `trunk-thickness`, `least-thickness`, `randomness`, `power`, `gravity`, `turn`, `leaf-size`, and
/// `deepest-level` where given).
/// , testing-interface.md (Kind `tree`)
pub(super) fn growth_inputs(t: &Value, tree: &str) -> Rd<GrowthInputs> {
    let heights = pair_of(t, "trunk-heights", tree)?;
    let whole = |x: f64, what: &str| -> Rd<u32> {
        if x >= 0.0 && x <= 4294967295.0 && x == x.trunc() {
            Ok(x as u32)
        } else {
            err(format!("tree {tree:?}'s {what} {x} is not a whole number"))
        }
    };
    let outline_of = setting(t, "outline", tree)?;
    let mut outline = [[0.0; 2]; 4];
    for (k, key) in ["O0", "O1", "O2", "O3"].iter().enumerate() {
        let d = doubles_of(
            member(outline_of, key).ok_or_else(|| DescriptionError(format!("tree {tree:?}'s outline has no {key}")))?,
            2,
            &format!("tree {tree:?}'s outline {key}"),
        )?;
        outline[k] = [d[0], d[1]];
    }
    Ok(GrowthInputs {
        deepest_level: match member(t, "deepest-level") {
            None => NEW_TREE_DEEPEST_LEVEL,
            Some(_) => word_of(t, "deepest-level", tree)?,
        },
        trunk_branches: word_of(t, "trunk-branches", tree)?,
        branch_branches: word_of(t, "branch-branches", tree)?,
        trunk_heights: [whole(heights[0], "trunk-heights")?, whole(heights[1], "trunk-heights")?],
        leaves_per_place: word_of(t, "leaves", tree)?,
        angles: pair_of(t, "angles", tree)?,
        trunk_thickness: double_of(t, "trunk-thickness", tree)?,
        least_thickness: double_of(t, "least-thickness", tree)?,
        shares: pair_of(t, "shares", tree)?,
        randomness: double_of(t, "randomness", tree)?,
        power: double_of(t, "power", tree)?,
        leaf_size: double_of(t, "leaf-size", tree)?,
        leaf_kind: word_of(t, "leaf-kind", tree)?,
        turn: double_of(t, "turn", tree)?,
        gravity: double_of(t, "gravity", tree)?,
        outline,
        seed: word_of(t, "seed", tree)?,
    })
}

/// A tree grown from settings with a piece of wood whose map is singular (no thickness or no length) is refused, an
/// own refusal of this program: such a piece holds every point, the camera's eye among them (`TreeModel::holds_point`),
/// so the whole picture would be drawn as from inside the tree, black. The setting named is the one that makes every
/// piece's radius 0 (the roots' from trunk-thickness) or lets a radius fall to 0 (no least radius 3·least-thickness).
/// A tree a document stored is drawn as stored (`stored_tree_given`), singular pieces and all.
fn flat_wood_refused(grown: &GrownTree, inputs: &GrowthInputs, name: &str) -> Rd<()> {
    let singular = |p: &Placement| crate::geometry::tree::tree_map([p[0], p[1], p[2]], p[3]).is_none();
    let Some(k) = grown.pieces.iter().position(|p| singular(&p.placement)) else {
        return Ok(());
    };
    let why = if inputs.trunk_thickness == 0.0 {
        "its trunk-thickness is 0: give it one above 0".to_owned()
    } else if inputs.least_thickness == 0.0 {
        "its least-thickness is 0: give it one above 0".to_owned()
    } else {
        format!("piece {k} of its wood has no thickness or no length")
    };
    err(format!(
        "tree {name:?} is refused, {why} (a piece of wood with no thickness holds every point, the camera's too, \
         and would turn the whole picture black)"
    ))
}

fn transform_of(p: &Placement) -> Transform {
    // (A placement: x·A + y·B + z·C + T), (a transform as's: rows, then
    // the translation)
    Transform { rows: [p[0], p[1], p[2]], translation: p[3] }
}

/// The stored tree of a description's tree: grown from the seed (the leaves' run the seed starts),
/// its one leaf shape the built-in leaf of the substrate's tables, its settings those it was grown from.
/// A `leaf` (the user's own installed leaf) is refused: its shape is not in the description.
/// , Errors (the tables)
pub(super) fn stored_tree_of(t: &Value, name: &str, tables: &Tables<'_>) -> Rd<StoredTree> {
    if let Some(leaf) = member(t, "leaf").and_then(Value::as_str).filter(|l| !l.is_empty()) {
        return err(format!(
            "tree {name:?} takes the leaf {leaf:?} of the user's own: its shape is not in the description (TR-4)"
        ));
    }
    let inputs = growth_inputs(t, name)?;
    let Some(noise) = tables.noise_bits_five else {
        return err(format!(
            "tree {name:?} needs the substrate's noise-bits-five to be grown (TR-3 Errors): give the renderer a substrate"
        ));
    };
    let grown = crate::trees::grow(&inputs, LeafRun::of_seed(inputs.seed), noise)
        .map_err(|e| DescriptionError(format!("tree {name:?}: {e}")))?;
    flat_wood_refused(&grown, &inputs, name)?;
    let shapes = match tables.leaf {
        Some(mesh) => {
            let triangles = mesh.triangles_with_normals();
            let points: Vec<Vec3> = mesh.points.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
            let coords: Vec<[f32; 2]> = mesh.uvs.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
            // each triangle's three corners' normals in the order made, numbered in that order; its coordinates are
            // the table's, one to a point (the numbers of its points, normals and coordinates)
            let normals: Vec<Vec3> = triangles.iter().flat_map(|t| t.corner_normals).collect();
            let words: Vec<[u32; 9]> = triangles
                .iter()
                .enumerate()
                .map(|(k, t)| {
                    let n = 3 * k as u32;
                    [t.numbers[0], t.numbers[1], t.numbers[2], n, n + 1, n + 2, t.numbers[0], t.numbers[1], t.numbers[2]]
                })
                .collect();
            vec![LeafShape { points, normals, coords, triangles: words }]
        }
        None if grown.leaves.is_empty() => Vec::new(),
        None => {
            return err(format!(
                "tree {name:?} has leaves, which need the substrate's leaf-points, leaf-uvs and leaf-triangles (TR-4 Errors)"
            ));
        }
    };
    let settings = GrowthSettings {
        max_depth: inputs.deepest_level,
        trunk_branches_max: inputs.trunk_branches,
        branch_branches: inputs.branch_branches,
        segments: inputs.trunk_heights,
        leaves_per_place: inputs.leaves_per_place,
        angle_at_thickest: inputs.angles[0],
        angle_at_thinnest: inputs.angles[1],
        trunk_thickness: inputs.trunk_thickness,
        min_thickness: inputs.least_thickness,
        radius_share: inputs.shares,
        randomness: inputs.randomness,
        spread_power: inputs.power,
        leaf_size: inputs.leaf_size,
        leaf_arrangement: inputs.leaf_kind,
        turn_per_height: inputs.turn,
        gravity: inputs.gravity,
        seed: inputs.seed,
        outline: inputs.outline.map(|[across, up]| [across, up, 0.0]),
    };
    Ok(StoredTree {
        shapes,
        settings,
        pieces: grown.pieces.iter().map(|p| WoodPiece { parent: p.from, transform: transform_of(&p.placement) }).collect(),
        leaves: grown.leaves.iter().map(|l| Leaf { shape: 0, transform: transform_of(&l.placement) }).collect(),
        places: grown
            .places
            .iter()
            .map(|p| LeafPlace { direction: p.direction, point: p.point, piece: p.piece.unwrap_or(0) })
            .collect(),
    })
}

/// A tree of a Bryce document, already grown: the pieces, leaves and leaf shapes the scene file stored, given in the
/// description's `stored` (the add-on sends what a file of the earlier add-on keeps for a tree it holds only as a proxy
/// mesh). The growth settings and the leaf places are not in it, and nothing that draws a tree reads them (a
/// stored tree is drawn as stored), so they stay empty.
/// (Legacy properties, stored trees)
pub(super) fn stored_tree_given(t: &Value, name: &str) -> Rd<StoredTree> {
    let list = |key: &str| -> Rd<&[Value]> {
        member(t, key)
            .and_then(Value::as_array)
            .ok_or_else(|| DescriptionError(format!("tree {name:?}'s stored tree has no list {key:?}")))
    };
    let transform = |v: &Value, what: &str| -> Rd<Transform> {
        let m = doubles_of(
            member(v, "matrix").ok_or_else(|| DescriptionError(format!("tree {name:?}'s {what} has no matrix")))?,
            12,
            &format!("tree {name:?}'s {what} matrix"),
        )?;
        Ok(Transform { rows: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]], translation: [m[9], m[10], m[11]] })
    };
    let words = |v: &Value, key: &str| -> Rd<[u32; 3]> {
        let a = member(v, key)
            .and_then(Value::as_array)
            .filter(|a| a.len() == 3)
            .ok_or_else(|| DescriptionError(format!("tree {name:?}'s triangle needs three numbers in {key:?}")))?;
        Ok([u32_of(&a[0])?, u32_of(&a[1])?, u32_of(&a[2])?])
    };
    let mut shapes = Vec::new();
    for shape in list("shapes")? {
        let mut out = LeafShape { points: Vec::new(), normals: Vec::new(), coords: Vec::new(), triangles: Vec::new() };
        for (key, into) in [("vertices", 0), ("normals", 1)] {
            for p in member(shape, key).and_then(Value::as_array).unwrap_or(&[]) {
                let d = doubles_of(p, 3, &format!("tree {name:?}'s leaf {key}"))?;
                [&mut out.points, &mut out.normals][into].push([d[0], d[1], d[2]]);
            }
        }
        for c in member(shape, "uvs").and_then(Value::as_array).unwrap_or(&[]) {
            let d = doubles_of(c, 2, &format!("tree {name:?}'s leaf coordinates"))?;
            out.coords.push([d[0] as f32, d[1] as f32]);
        }
        for tri in member(shape, "triangles").and_then(Value::as_array).unwrap_or(&[]) {
            let (v, n, uv) = (words(tri, "v")?, words(tri, "n")?, words(tri, "uv")?);
            out.triangles.push([v[0], v[1], v[2], n[0], n[1], n[2], uv[0], uv[1], uv[2]]);
        }
        shapes.push(out);
    }
    let mut pieces = Vec::new();
    for p in list("pieces")? {
        let parent = f64_of(member(p, "parent").ok_or_else(|| DescriptionError(format!("tree {name:?}'s piece has no parent")))?)?;
        pieces.push(WoodPiece { parent: if parent < 0.0 { None } else { Some(parent as u32) }, transform: transform(p, "piece")? });
    }
    let mut leaves = Vec::new();
    for l in list("leaves")? {
        let shape = member(l, "shape").map(u32_of).transpose()?.unwrap_or(0);
        if shape as usize >= shapes.len() {
            return err(format!("tree {name:?}'s leaf names the shape {shape}, of {}", shapes.len()));
        }
        leaves.push(Leaf { shape, transform: transform(l, "leaf")? });
    }
    let settings = GrowthSettings {
        max_depth: 0,
        trunk_branches_max: 0,
        branch_branches: 0,
        segments: [0, 0],
        leaves_per_place: 0,
        angle_at_thickest: 0.0,
        angle_at_thinnest: 0.0,
        trunk_thickness: 0.0,
        min_thickness: 0.0,
        radius_share: [0.0, 0.0],
        randomness: 0.0,
        spread_power: 0.0,
        leaf_size: 0.0,
        leaf_arrangement: 0,
        turn_per_height: 0.0,
        gravity: 0.0,
        seed: 0,
        outline: [[0.0; 3]; 4],
    };
    Ok(StoredTree { shapes, settings, pieces, leaves, places: Vec::new() })
}
