// SPDX-License-Identifier: MIT
// The one stand-in the trees' wood and the sets of metaballs share: the record of hand-overs.
// (One stand-in)

use stecyk3d::geometry::metaball::{MetaScene, Metaballs, Showing};

/// A taken crossing of the wood hands the shared stand-in that tree's wood material; a hit on the
/// wood names the same entry; a later tree's hand-over replaces what the stand-in holds.
#[test]
fn wood_hand_overs_are_the_latest_showing() {
    let ms = MetaScene::new(Metaballs::default(), vec![], vec![], 9);
    assert_eq!(ms.latest(), None);
    ms.hand_over_wood(3);
    assert_eq!(ms.latest(), Some(Showing::Wood(3)));
    let id = ms.record_wood_hit(3);
    assert_eq!(ms.showing(id), Showing::Wood(3));
    ms.hand_over_wood(5);
    assert_eq!(ms.latest(), Some(Showing::Wood(5)));
    assert_eq!(ms.showing(id), Showing::Wood(3), "a hit keeps the showing it was given");
}
