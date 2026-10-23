// SPDX-License-Identifier: MIT
//! The description the add-on's own functions make for every kind the renderer draws from documents is read by
//! the description reader (roadmap item 4).
//!
//! (The add-on sends what the reader takes); the fixture is made by
//! blender_addon/tests/addon_kinds.py, which a Python test keeps equal to what the add-on makes.

use stecyk3d::description::describe;
use stecyk3d::scene::{Body, Light};
use stecyk3d::scene_beams::BeamRecord;
use stecyk3d::scene_records::ObjectKind;

const FIXTURE: &str = include_str!("../blender_addon/tests/fixtures/addon-kinds.json");

#[test]
fn the_add_ons_kinds_are_read() {
    // trees are grown with the substrate's tables, which no test here holds: the tree is read apart, below
    let without_tree = FIXTURE.replacen("\"trees\": [", "\"trees\": [], \"unused_trees\": [", 1);
    let d = describe(&without_tree).expect("the add-on's description is read");
    let scene = &d.scene;

    // lights: pyramid, cylinder, and a spot with a gradient
    let beams: Vec<_> = scene
        .lights
        .iter()
        .filter_map(|l| match l {
            Light::Radial(r) => Some(r),
            Light::Sun(_) => None,
        })
        .collect();
    assert_eq!(beams.len(), 3);
    assert!(matches!(beams[0].beam, Some(BeamRecord::Square { .. })));
    assert!(matches!(beams[1].beam, Some(BeamRecord::Cylinder { .. })));
    assert!(beams[2].gradient.is_some() && beams[0].gradient.is_none());
    let gradient = beams[2].gradient.as_ref().unwrap();
    assert_eq!((gradient.colour_keys.len(), gradient.value_keys.len()), (2, 1));
    assert!(gradient.colour_keys[0].place < gradient.colour_keys[1].place, "sorted by place");

    // a group with its members, a terrain, and a sphere with the named material
    let by_name = |n: &str| scene.objects.iter().position(|o| o.name == n.as_bytes()).expect(n);
    let g = by_name("G");
    assert_eq!(scene.objects[g].kind, ObjectKind::Group);
    assert_eq!(scene.objects[by_name("A")].linked_to, Some(g));
    assert_eq!(scene.objects[by_name("B")].linked_to, Some(g));
    assert_eq!(scene.objects[by_name("B")].placement.unwrap().second_flags, 0x90);
    assert_eq!(scene.objects[by_name("A")].placement.unwrap().second_flags, 0x8);
    let t = by_name("T");
    assert!(matches!(scene.objects[t].body, Body::Terrain(0)));
    assert_eq!(scene.terrains[0].part.grid.side(), 16);
    assert!(scene.terrains[0].part.mirrored);
    let a_material = scene.objects[by_name("A")].material.expect("material");
    assert_eq!(scene.materials[a_material].name, b"Wood");

    // the picture and its texture
    assert_eq!(scene.pictures.len(), 1);
    assert_eq!(scene.textures.len(), 1);
}

#[test]
fn the_add_ons_tree_is_read_or_refused_for_its_missing_tables() {
    let e = describe(FIXTURE).err().expect("a tree needs the substrate's tables");
    assert!(e.0.contains("noise-bits-five"), "{}", e.0);
}
