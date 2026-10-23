// SPDX-License-Identifier: MIT
//! Kind `tree`: one tree grown from its settings, answered with its counts and the SHA-256 of
//! its text.
//!
//! (Kind `tree`: the case's fields, the result line, the tree's text);
//! (Refusals and bounds),

use super::{Outcome, Value};
use crate::numerics::Bryce5Rand;
use crate::sha256::{Sha256, to_hex};
use crate::substrate::Substrate;
use crate::trees::text::write_text;
use crate::trees::{self, GrowthInputs, LeafMesh, LeafRun};
use crate::version::RenderAs;

/// A tree case's inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeCase {
    pub settings: GrowthInputs,
    pub leaf_run: LeafRun,
    /// Whether the text holds the leaf's shape and normals.
    pub shape: bool,
}

/// What a tree case gives: its counts and the hash of its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeResult {
    pub pieces: usize,
    pub places: usize,
    pub leaves: usize,
    pub sha256_hex: String,
}

/// Compute a tree case. (Kind `tree`)
pub fn compute(v: &Value, version: RenderAs, substrate: Option<&Substrate>) -> Outcome {
    if version != RenderAs::Bryce50 {
        return Outcome::refused("a tree case is for version 5.0 only (TR-2: Bryce 5 only)");
    }
    let case = match parse_case(v) {
        Ok(c) => c,
        Err(e) => return Outcome::refused(format!("tree case not understood: {e}")),
    };
    let Some(substrate) = substrate else {
        return Outcome::refused("no substrate could be read: the tree needs noise-bits-five (TR-3 Errors)");
    };
    let Ok(noise) = substrate.noise_bits_five() else {
        return Outcome::refused("the substrate has no noise-bits-five: the tree cannot be grown (TR-3 Errors)");
    };
    let mesh = if case.shape {
        match (substrate.leaf_points(), substrate.leaf_uvs(), substrate.leaf_triangles()) {
            (Ok(points), Ok(uvs), Ok(triangles)) => Some(LeafMesh { points, uvs, triangles }),
            _ => {
                return Outcome::refused(
                    "the substrate lacks leaf-points, leaf-uvs or leaf-triangles, which the leaf's shape needs (TR-4 Errors)",
                );
            }
        }
    } else {
        None
    };
    match run(&case, noise, mesh) {
        Ok(r) => Outcome::Tree(r),
        Err(e) => Outcome::refused(e.to_string()),
    }
}

/// Grow the case's tree and hash its text. (Kind `tree`)
pub fn run(case: &TreeCase, noise_bits_five: &[u32; 64], mesh: Option<LeafMesh<'_>>) -> Result<TreeResult, trees::GrowthRefusal> {
    let tree = trees::grow(&case.settings, case.leaf_run, noise_bits_five)?;
    let shape = mesh.map(|m| m.triangles_with_normals());
    let mut hash = Sha256::new();
    write_text(&tree, shape.as_deref(), &mut hash);
    Ok(TreeResult {
        pieces: tree.pieces.len(),
        places: tree.places.len(),
        leaves: tree.leaves.len(),
        sha256_hex: to_hex(&hash.finish()),
    })
}

/// Read `tree`, `leaves-run` and `shape`. (Kind `tree`)
pub fn parse_case(v: &Value) -> Result<TreeCase, String> {
    let t = field(v, "tree")?;
    let w = |key: &str| whole(field(t, key)?).map_err(|e| format!("{key}: {e}"));
    let d = |key: &str| double(field(t, key)?).map_err(|e| format!("{key}: {e}"));
    let pair_w = |key: &str| -> Result<[u32; 2], String> {
        let [a, b] = list::<2>(field(t, key)?).map_err(|e| format!("{key}: {e}"))?;
        Ok([whole(a).map_err(|e| format!("{key}: {e}"))?, whole(b).map_err(|e| format!("{key}: {e}"))?])
    };
    let pair_d = |key: &str| -> Result<[f64; 2], String> {
        let [a, b] = list::<2>(field(t, key)?).map_err(|e| format!("{key}: {e}"))?;
        Ok([double(a).map_err(|e| format!("{key}: {e}"))?, double(b).map_err(|e| format!("{key}: {e}"))?])
    };
    let outline_list = list::<4>(field(t, "outline")?).map_err(|e| format!("outline: {e}"))?;
    let mut outline = [[0.0; 2]; 4];
    for (o, p) in outline.iter_mut().zip(outline_list) {
        let [x, y] = list::<2>(p).map_err(|e| format!("outline: {e}"))?;
        *o = [double(x).map_err(|e| format!("outline: {e}"))?, double(y).map_err(|e| format!("outline: {e}"))?];
    }
    let settings = GrowthInputs {
        deepest_level: w("deepest-level")?,
        trunk_branches: w("trunk-branches")?,
        branch_branches: w("branch-branches")?,
        trunk_heights: pair_w("trunk-heights")?,
        leaves_per_place: w("leaves")?,
        angles: pair_d("angles")?,
        trunk_thickness: d("trunk-thickness")?,
        least_thickness: d("least-thickness")?,
        shares: pair_d("shares")?,
        randomness: d("randomness")?,
        power: d("power")?,
        leaf_size: d("leaf-size")?,
        leaf_kind: w("leaf-kind")?,
        turn: d("turn")?,
        gravity: d("gravity")?,
        outline,
        seed: w("seed")?,
    };
    let r = field(v, "leaves-run")?;
    let sum = whole(field(r, "sum")?).map_err(|e| format!("leaves-run sum: {e}"))?;
    let pos = list::<4>(field(r, "positions")?).map_err(|e| format!("leaves-run positions: {e}"))?;
    let mut positions = [0u32; 4];
    for (k, (p, value)) in positions.iter_mut().zip(pos).enumerate() {
        *p = whole(value).map_err(|e| format!("leaves-run positions: {e}"))?;
        let period = Bryce5Rand::PERIODS[k];
        if *p >= period {
            return Err(format!("leaves-run position {} is {p}, not below {period} (TR-4)", k + 1));
        }
    }
    let shape = match field(v, "shape")? {
        Value::Bool(b) => *b,
        _ => return Err("shape: not true or false".into()),
    };
    Ok(TreeCase { settings, leaf_run: LeafRun { sum, positions }, shape })
}

fn field<'a>(o: &'a Value, key: &str) -> Result<&'a Value, String> {
    o.get(key).ok_or_else(|| format!("no field {key:?}"))
}

fn list<const N: usize>(v: &Value) -> Result<&[Value; N], String> {
    let l = v.as_array().ok_or("not a list")?;
    l.try_into().map_err(|_| format!("{} items, not {N}", l.len()))
}

/// A whole number from 0 to 4,294,967,295 written as a JSON number.
fn whole(v: &Value) -> Result<u32, String> {
    match v {
        Value::Number(t) => t.parse::<u32>().map_err(|_| format!("{t:?} is not a whole number from 0 to 4294967295")),
        _ => Err("not a number".into()),
    }
}

/// A double as 16 hexadecimal digits of its bit pattern. (Case files)
fn double(v: &Value) -> Result<f64, String> {
    let s = v.as_str().ok_or("not a string")?;
    if s.len() != 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{s:?} is not 16 hexadecimal digits"));
    }
    u64::from_str_radix(s, 16).map(f64::from_bits).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cases::json;

    const ONE: &str = r#"{"case":"tree-001","kind":"tree","version":"5.0","tree":{"deepest-level":25,"trunk-branches":1,"branch-branches":2,"trunk-heights":[6,6],"leaves":1,"angles":["3fdf7503b81b64e0","3ff1f55475a31a4c"],"trunk-thickness":"3f9aa64c2f837b4a","least-thickness":"3f7885d31337eb29","shares":["3fd8be1eb4202d9d","3feb8efdc9c4da90"],"randomness":"3fed69552e2fbe34","power":"3ff8dd249e44fa05","leaf-size":"3fb172abef06b378","leaf-kind":3,"turn":"3ff921fb54442d18","gravity":"3fe50b32ce89656f","outline":[["0000000000000000","3fa2915379fa97e1"],["3fd6c258d5842b73","3fc1929670196d8f"],["3fdd84f4c6e6d9be","3fee1fcb0c026cc2"],["0000000000000000","3fee47fbacb42891"]],"seed":2184841607},"leaves-run":{"sum":3421969743,"positions":[24,38,29,13]},"shape":true}"#;

    #[test]
    fn parses_every_field() {
        let c = parse_case(&json::parse(ONE).unwrap()).unwrap();
        let s = c.settings;
        assert_eq!((s.deepest_level, s.trunk_branches, s.branch_branches), (25, 1, 2));
        assert_eq!(s.trunk_heights, [6, 6]);
        assert_eq!((s.leaves_per_place, s.leaf_kind, s.seed), (1, 3, 2_184_841_607));
        assert_eq!(s.turn.to_bits(), 0x3ff9_21fb_5444_2d18);
        assert_eq!(s.outline[3][1].to_bits(), 0x3fee_47fb_acb4_2891);
        assert_eq!(s.outline[0][0], 0.0);
        assert_eq!(c.leaf_run, LeafRun { sum: 3_421_969_743, positions: [24, 38, 29, 13] });
        assert!(c.shape);
    }

    #[test]
    fn positions_beyond_their_periods_refused() {
        let bad = ONE.replace("[24,38,29,13]", "[47,38,29,13]");
        assert!(parse_case(&json::parse(&bad).unwrap()).is_err());
        let bad = ONE.replace("[24,38,29,13]", "[24,38,29,61]");
        assert!(parse_case(&json::parse(&bad).unwrap()).is_err());
        let bad = ONE.replace("\"seed\":2184841607", "\"seed\":-1");
        assert!(parse_case(&json::parse(&bad).unwrap()).is_err());
    }

    #[test]
    fn refused_without_substrate_or_other_version() {
        let v = json::parse(ONE).unwrap();
        assert!(matches!(compute(&v, RenderAs::Bryce50, None), Outcome::Refused { .. }));
        assert!(matches!(compute(&v, RenderAs::Bryce31, None), Outcome::Refused { .. }));
    }
}
