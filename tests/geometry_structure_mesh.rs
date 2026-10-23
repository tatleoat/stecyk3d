// SPDX-License-Identifier: MIT
// and rule M, with the consequence for ties
// the entry states ("stated for checking"): where a mesh and a plane, a slab, a cube, a square, a
// cylinder, a cone or another mesh meet a ray at exactly the same distance, the mesh is reported
// if the walk asks it in the leaf where it first asks either of the two; a plane or slab is asked
// first, so the mesh beats it; of two meshes the one asked later wins. No released vector holds
// such a tie, so these are the entry's own sentences, not vectors.
// (Meshes in the structure)

use stecyk3d::geometry::structure::{Layout, Listed, Structure};
use stecyk3d::geometry::{Face, Object, Placement, Range, Ray, Scene, Shape, ShapeContext, StoredMesh};
use stecyk3d::version::RenderAs;

/// A one-triangle mesh lying in the plane z = z0, its box the points' bounds moved out by a
/// hundredth of the extent (and a flat axis by 0.001).
fn sheet(z0: f64) -> StoredMesh {
    StoredMesh {
        points: vec![[-1.0, -1.0, z0], [1.0, -1.0, z0], [0.0, 1.0, z0]],
        normals: vec![],
        coords: vec![],
        faces: vec![Face {
            points: [0, 1, 2],
            normals: [0; 3],
            coords: [0; 3],
            plane: [0.0, 0.0, -1.0],
            words: [0; 3],
            flags: 0,
        }],
        bounds: [-1.02, -1.02, z0 - 0.001, 1.02, 1.02, z0 + 0.001],
        solid_in_group: false,
        fast: None,
    }
}

fn scene_of(shapes: &[Shape], meshes: Vec<StoredMesh>, version: RenderAs, option: u32) -> Scene {
    let objects: Vec<Object> = shapes.iter().map(|&shape| Object { shape, placement: Placement::IDENTITY }).collect();
    let listed: Vec<Listed> = (0..objects.len()).map(Listed::Object).collect();
    let mut scene = Scene { objects, stored_meshes: meshes, ..Scene::default() };
    scene.structure = Some(Structure::build(
        &scene,
        &listed,
        ([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]),
        version,
        Layout::Tree,
        option,
    ));
    scene
}

fn first(scene: &Scene, version: RenderAs) -> Option<usize> {
    let ctx = ShapeContext { version, pyramid_normal: [0.5, 0.5], shadow: false };
    let ray = Ray { origin: [0.0, 0.0, -5.0], dir: [0.0, 0.0, 1.0] };
    scene.intersect(&ray, Range { near: 0.0, far: f64::INFINITY }, &ctx).map(|h| h.object)
}

const VERSIONS: [RenderAs; 3] = [RenderAs::Bryce30, RenderAs::Bryce31, RenderAs::Bryce50];

#[test]
fn a_mesh_beats_a_shape_at_the_same_distance() {
    // The cube's near face and the sheet are both at z = -0.5: distance 4.5.
    for version in VERSIONS {
        for option in [0, 1, 2] {
            for shapes in [[Shape::Cube, Shape::StoredMesh(0)], [Shape::StoredMesh(0), Shape::Cube]] {
                let scene = scene_of(&shapes, vec![sheet(-0.5)], version, option);
                let mesh = if shapes[0] == Shape::Cube { 1 } else { 0 };
                assert_eq!(first(&scene, version), Some(mesh), "{version:?} option {option} {shapes:?}");
            }
        }
    }
}

#[test]
fn a_mesh_beats_a_plane_or_slab_at_the_same_distance() {
    // The plane y = 0 is met by no ray along z; use a slab: its near face is at z = -0.5.
    for version in VERSIONS {
        let scene = scene_of(&[Shape::Slab, Shape::StoredMesh(0)], vec![sheet(-0.5)], version, 1);
        assert_eq!(first(&scene, version), Some(1), "{version:?}");
    }
}

#[test]
fn of_two_meshes_the_one_asked_later_wins() {
    for version in VERSIONS {
        let scene = scene_of(&[Shape::StoredMesh(0), Shape::StoredMesh(1)], vec![sheet(-0.5), sheet(-0.5)], version, 0);
        assert_eq!(first(&scene, version), Some(1), "{version:?}");
    }
}

#[test]
fn a_nearer_mesh_is_reported_and_a_nearer_shape_too() {
    for version in VERSIONS {
        let scene = scene_of(&[Shape::Cube, Shape::StoredMesh(0)], vec![sheet(0.25)], version, 1);
        assert_eq!(first(&scene, version), Some(0), "{version:?}: the cube's face is nearer");
        let scene = scene_of(&[Shape::Cube, Shape::StoredMesh(0)], vec![sheet(-2.0)], version, 1);
        assert_eq!(first(&scene, version), Some(1), "{version:?}: the sheet is nearer");
    }
}
