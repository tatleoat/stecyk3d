// SPDX-License-Identifier: MIT
// The order of a metaball set's search under the two structures: the grid's add routine puts every
// metaball at the FRONT of its entry list (index 0), the tree's appends; each metaball's near list is then
// its overlapping neighbours in entry order (the same link loop serves both structures), so the
// depth-first search visits the neighbours in the list order under the tree and in the reverse of it under
// the grid.
// (Metaballs)

use stecyk3d::geometry::metaball::{Ball, Metaballs};
use stecyk3d::geometry::{Placement, Ray};

fn ball_at(x: f64, y: f64, z: f64) -> Ball {
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    Ball::new(Placement::new(id, [x, y, z]).expect("identity map"), false, None)
}

fn set(reversed: bool) -> Vec<usize> {
    // ball 0 at the origin; balls 1 and 2 each overlap ball 0 and each other
    let balls = Metaballs { balls: vec![ball_at(0.0, 0.0, 0.0), ball_at(1.0, 0.0, 0.0), ball_at(0.0, 1.0, 0.0)], reversed };
    let ray = Ray { origin: [-5.0, 0.5, 0.0], dir: [1.0, 0.0, 0.0] };
    balls.set(0, &ray, 0.0, f64::INFINITY)
}

#[test]
fn tree_searches_neighbours_in_list_order() {
    // preorder 0, 1, 2; the set is kept in the reverse of the order they were found
    assert_eq!(set(false), vec![2, 1, 0]);
}

#[test]
fn grid_searches_neighbours_in_reverse_list_order() {
    // preorder 0, 2, 1
    assert_eq!(set(true), vec![1, 2, 0]);
}
