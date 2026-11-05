// SPDX-License-Identifier: MIT
// A tree with no `world` is placed by its `matrix`, whose translation Blender keeps in the fourth column: a tree
// moved along x draws a different picture (before the fix the translation was read from the fourth row, always 0).
// (axes and units)

use stecyk3d::description::describe;
use stecyk3d::render::{OpenChoices, render_lit};

fn picture(x: f64) -> String {
    let textured = false;
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
    let s = 1.0;
    let l = 1.0;
    let t = -1.0; // the leaf spans -1 to 1 in the world either way
    let text = format!(
        r#"{{"size": [48, 36], "render": {{"version": "5.0", "units": 1.0, "antialias": 0, "preview": false, "switches": 959}}, "sky": {{}},
        "sun": {{"colour": [1, 1, 1], "glow_colour": [1, 1, 1], "direction": [0.3, 0.6, -0.7], "intensity": 1.0,
        "switches": 4195585}}, "five": {{"sky5": true, "sky5_flags2": 0}},
        "camera": {{"position": [0, 0, -8], "forward": [0, 0, 1], "up": [0, 1, 0], "type": "PERSP", "lens": 40.4,
        "sensor_width": 36, "projection": "perspective", "frame_angle": 0.8377578413281667, "pan": [0, 0], "pixel_aspect": 1}},
        "materials": {{"wood": {wood}, "leaf": {leaf}}}, "textures": [{texture}], "objects": [],
        "trees": [{{"name": "t", "matrix": [[{s}, 0, 0, {x}], [0, {s}, 0, 0], [0, 0, {s}, 0], [0, 0, 0, 1]],
        "material": "wood", "leaves_material": "leaf",
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
fn a_trees_matrix_translation_moves_it() {
    // Blender's matrix_world keeps the translation in the fourth column; a tree with no `world` reads `matrix`
    assert_ne!(picture(-2.0), picture(2.0), "the tree at x = -2 and at x = 2 are different pictures");
}
