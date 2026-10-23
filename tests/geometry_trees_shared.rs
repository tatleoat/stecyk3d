// SPDX-License-Identifier: MIT
// : trees that show one stored tree share the marks of the items and the
// leaves' kept hits; the pieces' one kept crossing is the scene's; each tree object has its own missed
// mark and kept hit. Observed: two such trees at 0.07 and 0.03 apart, or in one place, draw their
// overlapping wood all in the later tree's material (the tree asked last answers a crossing the first
// found). The test: a tree that keeps a piece's crossing for a ray, and a second tree of the same stored
// tree, placed a little aside, which then answers that crossing as it stands (and, with nothing shared,
// meets its own wood at another distance).
// (marks)

use stecyk3d::geometry::tree::{ItemMarks, Orders, PieceCrossing, RayMarks, TreeModel, tree_map};
use stecyk3d::geometry::Ray;
use stecyk3d::scene_records::Transform;
use stecyk3d::scene_trees::WoodPiece;

const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[test]
fn trees_of_one_stored_tree_share_the_kept_crossing() {
    let piece = WoodPiece { parent: None, transform: Transform { rows: IDENTITY, translation: [0.0; 3] } };
    let model = TreeModel::new(&[], &[piece], &[]).expect("one piece").with_stored(0);
    assert_eq!(model.stored(), Some(0));
    let here = tree_map(IDENTITY, [0.0, 0.0, 0.0]).unwrap();
    let there = tree_map(IDENTITY, [0.05, 0.0, 0.0]).unwrap();
    let ray = Ray { origin: [0.0, 0.0, -6.0], dir: [0.0, 0.0, 1.0] };
    let o = Orders::VECTORS;

    // Alone, the second tree meets its own wood, at another distance than the first's.
    let mut alone = RayMarks::new();
    let own = model.ask(&there, &mut alone, &ray, 0.0, 100.0, 100.0, &o).hit.expect("its own wood");
    let mut first = RayMarks::new();
    let found = model.ask(&here, &mut first, &ray, 0.0, 100.0, 100.0, &o).hit.expect("the wood of the first tree");
    assert_ne!(own.t, found.t);

    // The first tree asked with a leaf bound below the crossing keeps it (the far end draws in)...
    let (mut items, mut crossing) = (ItemMarks::default(), PieceCrossing::default());
    let (mut a, mut b) = (RayMarks::new(), RayMarks::new());
    a.exchange(&mut items, &mut crossing);
    let kept = model.ask(&here, &mut a, &ray, 0.0, found.t / 2.0, 100.0, &o);
    a.exchange(&mut items, &mut crossing);
    assert!(kept.hit.is_none());
    assert_eq!(kept.far_end, found.t, "the crossing was kept for a later stretch");

    // ...and the second tree, asked next over the stretch that holds it, answers the first tree's crossing
    // as it stands (its items hold the kept mark, the pieces' crossing is the scene's).
    b.exchange(&mut items, &mut crossing);
    let second = model.ask(&there, &mut b, &ray, found.t / 2.0, 100.0, 100.0, &o);
    b.exchange(&mut items, &mut crossing);
    let hit = second.hit.expect("the shared kept crossing answers for the second tree");
    assert_eq!(hit.t, found.t);

    // Not shared (separate item marks and crossing): the second tree meets its own wood.
    let (mut items2, mut crossing2) = (ItemMarks::default(), PieceCrossing::default());
    let mut c = RayMarks::new();
    c.exchange(&mut items2, &mut crossing2);
    let unshared = model.ask(&there, &mut c, &ray, found.t / 2.0, 100.0, 100.0, &o).hit.expect("own wood");
    assert_eq!(unshared.t, own.t);
}

// : the tree asks each piece whether the point lies
// inside it, handing it the point itself; a piece holds the point where the point carried by the inverse
// of its map lies less than one half from its centre, and a piece whose map is singular holds every point
// (the eye with it); a tree with no singular piece holds only those balls.
#[test]
fn a_tree_holds_the_eye_by_its_pieces() {
    let ball = WoodPiece { parent: None, transform: Transform { rows: IDENTITY, translation: [1.0, 0.0, 0.0] } };
    let tree = TreeModel::new(&[], &[ball], &[]).expect("one piece");
    assert!(tree.holds_point([1.2, 0.0, 0.0]));
    assert!(!tree.holds_point([1.5, 0.0, 0.0]), "strictly less than one half");
    assert!(!tree.holds_point([0.0, 0.0, 0.0]));
    // a piece with a size of 0 on y: not drawn, holds every point
    let flat = WoodPiece {
        parent: None,
        transform: Transform { rows: [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]], translation: [0.0; 3] },
    };
    let singular = TreeModel::new(&[], &[flat], &[]).expect("one piece");
    assert!(singular.is_blank());
    assert!(singular.holds_point([123.0, -4.0, 5.0]));
    assert!(singular.holds_point([0.0, 0.0, 0.0]));
    // the stored tree with no pieces and no leaves draws nothing and holds nothing: no model
    assert!(TreeModel::new(&[], &[], &[]).is_err());
}
