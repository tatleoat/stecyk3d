// SPDX-License-Identifier: MIT
// A leaf's texture, mapped in object space, is read at the point in the TREE's space, and over the leaf shape's own
// box : the same leaf drawn twice at the same place in the world, once from a tree sized 2
// and once from a tree sized 1 whose leaf is made twice as large, has the same silhouette (plain leaves: the same
// picture) and a different texture (the tree-space points differ by the factor). Before the fix the map was read over
// a box of no numbers and at the world point, so the two textured pictures were the same.
// (Legacy properties: kept trees and their leaves)

use stecyk3d::description::describe;
use stecyk3d::render::{OpenChoices, render_lit};

fn picture(tree_size: f64, leaf_size: f64, textured: bool) -> String {
    let channel = if textured {
        r#""diffuse_color": {"switches": 268435456, "a": [1, 1, 1], "b": [0, 0, 0], "alpha": 0, "a_tex": "leaf:0"}"#
    } else {
        r#""diffuse_color": {"switches": 0, "a": [0.2, 0.8, 0.3]}"#
    };
    let material = |channel: &str| {
        format!(
            r#"{{"flags": 65552, "volume_colour": [1, 1, 1], "refraction": 1.33, "volume_numbers": [0, 1, 0.75], "channels": {{
            {channel}, "diffusion": {{"switches": 0, "a": 1}}, "ambience": {{"switches": 0, "a": 0.2}},
            "specularity": {{"switches": 0, "a": 0}}, "transparency": {{"switches": 0, "a": 0}},
            "reflection": {{"switches": 0, "a": 0}}}}}}"#
        )
    };
    let texture = r#"{"texture": {"outputs": 2, "blend": [0, 0], "phase": {"amount": 0, "noise": {"type": 0}},
        "filter": {"number": 0, "n": 0, "a": 0, "b": 0},
        "colours": {"mode": 0, "space": 0, "a": "000000", "b": "ff66ff", "c": "ffffff"},
        "components": [{"outputs": 2, "noise": {"type": 2, "method": 0, "octaves": 1, "dimension": 2},
        "angles": [0, 0], "frequencies": [12, 12, 12], "phase": {"amount": 0, "noise": {"type": 0}},
        "filter": {"number": 0}, "colours": {"mode": 4, "space": 0, "a": "1c8600", "b": "cfe500", "c": "58fa02"}}]},
        "mapping": {"mode": 0, "switches": 0, "scale": [1, 1, 1], "offset": [0, 0, 0], "turn": [0, 0, 0, 1]},
        "name": "leaf:0", "kind": "procedural"}"#;
    let s = tree_size;
    let l = leaf_size;
    let t = -1.0 / tree_size; // the leaf spans -1 to 1 in the world either way
    let text = format!(
        r#"{{"size": [48, 36], "render": {{"version": "5.0", "units": 1.0, "antialias": 0, "preview": false, "switches": 959}}, "sky": {{}},
        "sun": {{"colour": [1, 1, 1], "glow_colour": [1, 1, 1], "direction": [0.3, 0.6, -0.7], "intensity": 1.0,
        "switches": 4195585}}, "five": {{"sky5": true, "sky5_flags2": 0}},
        "camera": {{"position": [0, 0, -8], "forward": [0, 0, 1], "up": [0, 1, 0], "type": "PERSP", "lens": 40.4,
        "sensor_width": 36, "projection": "perspective", "frame_angle": 0.8377578413281667, "pan": [0, 0], "pixel_aspect": 1}},
        "materials": {{"wood": {wood}, "leaf": {leaf}}}, "textures": [{texture}], "objects": [],
        "trees": [{{"name": "t", "matrix": [[{s}, 0, 0, 0], [0, {s}, 0, 0], [0, 0, {s}, 0], [0, 0, 0, 1]],
        "world": [{s}, 0, 0, 0, {s}, 0, 0, 0, {s}, 0, 0, 0], "material": "wood", "leaves_material": "leaf",
        "stored": {{"pieces": [{{"parent": -1, "matrix": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, -40, 0]}}],
        "leaves": [{{"shape": 0, "matrix": [{l}, 0, 0, 0, {l}, 0, 0, 0, {l}, {t}, {t}, 0]}}],
        "shapes": [{{"vertices": [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]], "normals": [[0, 0, -1]],
        "uvs": [[0, 0], [1, 0], [0, 1], [1, 1]],
        "triangles": [{{"v": [0, 1, 2], "n": [0, 0, 0], "uv": [0, 1, 2]}}, {{"v": [1, 3, 2], "n": [0, 0, 0], "uv": [1, 3, 2]}}]}}]}}}}]}}"#,
        wood = material(r#""diffuse_color": {"switches": 0, "a": [0.5, 0.3, 0.2]}"#),
        leaf = material(channel),
    );
    let d = describe(&text).unwrap_or_else(|e| panic!("the description is read: {}", e.0));
    let choices = d.choices(OpenChoices { no_preview: true, haze_five: Some([0.5, 0.5, 0.11]), ..OpenChoices::default() });
    let (picture, _) = render_lit(&d.scene, d.version, &choices).expect("the picture is drawn");
    picture.levels_sha256_hex()
}

#[test]
fn a_leafs_texture_is_read_in_the_trees_space() {
    // the same leaf at the same place of the world, by a tree twice the size or a leaf twice the size
    assert_eq!(picture(2.0, 1.0, false), picture(1.0, 2.0, false), "the plain leaves are the same picture");
    assert_ne!(picture(2.0, 1.0, true), picture(1.0, 2.0, true), "the textures are read at the tree's own points");
}
