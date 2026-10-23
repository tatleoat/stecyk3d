// SPDX-License-Identifier: MIT
// The terrain editor's eleventh round: C9 to C11 (the shape tools, Relief Noise, Subplateaus),
// , rule 8
// (Square and Round Edges by an amount) and 5.0's Fractal menu with Rounded Dunes' frame.
// No vector holds these rules (the entries say so: code, with the observations they name); each test
// below states what the entry states and nothing else.
// C9, C10, C11, rule 8,

use stecyk3d::terrain::HeightGrid;
use stecyk3d::terrain_clip::{
    apply_filter_in, at_amount_in, dampen_at_amount_in, round_edges_at, square_edges_at, ClipRange, Pencil,
};
use stecyk3d::terrain_editor::{
    apply_filter, at_amount, default_graph, round_edges, square_edges, Bracket, HeightNoiseSource, GRAPH_LEN,
};
use stecyk3d::terrain_fractal::{FractalKind, FractalMenu, Switch, Tiling, KINDS};
use stecyk3d::terrain_shapes::{apply, drag, side_works, ShapeTool};

fn ramp(n: usize) -> HeightGrid {
    HeightGrid::new(n, (0..n * n).map(|i| ((i * 4) % 65536) as u16).collect()).unwrap()
}

fn noise(n: usize) -> HeightGrid {
    let mut s = HeightNoiseSource::new();
    HeightGrid::new(n, (0..n * n).map(|_| (s.next_number() * 2) as u16).collect()).unwrap()
}

fn differing(a: &HeightGrid, b: &HeightGrid) -> usize {
    a.heights().iter().zip(b.heights()).filter(|(x, y)| x != y).count()
}

// C9 (b), (c): Posterize with s = 16 |d|, 1 when d = 0; Mosaic with q = |d|.
#[test]
fn posterize_and_mosaic() {
    let g = noise(128);
    let mut s = HeightNoiseSource::new();
    // 30 pixels to the right as 3.0 gave s = 480 (the last reading's d = 30): cut down(v / 480) 480.
    let p = apply(&g, ShapeTool::Posterize, 30, 0, false, &mut s);
    for (a, b) in g.heights().iter().zip(p.heights()) {
        assert_eq!(*b as u32, (*a as u32 / 480) * 480);
    }
    // a click is s = 1: no change; the direction does not matter
    assert_eq!(differing(&apply(&g, ShapeTool::Posterize, 0, 0, false, &mut s), &g), 0);
    assert_eq!(differing(&apply(&g, ShapeTool::Posterize, -30, 0, false, &mut s), &p), 0);
    // Mosaic: each point takes the height at row q cut down(r / q), column q cut down(c / q)
    let m = apply(&g, ShapeTool::Mosaic, 4, 0, false, &mut s);
    for r in 0..128 {
        for c in 0..128 {
            assert_eq!(m.at(r, c), g.at(4 * (r / 4), 4 * (c / 4)));
        }
    }
    assert_eq!(differing(&apply(&g, ShapeTool::Mosaic, 0, 0, false, &mut s), &g), 0);
    assert_eq!(s.taken(), 0, "None of (b) to (f) takes numbers");
}

// C9 (a): Basic Noise takes n n numbers at the press and n n more at every move read; the heights kept
// are those of the last pass: 30 pixels in ten moves of 3 gave E = 480 and the numbers 163841 on.
#[test]
fn basic_noise_drag() {
    let g = ramp(128);
    let n2 = 128 * 128;
    let readings: Vec<(i64, i64)> = (1..=10).map(|k| (3 * k, 0)).collect();
    let mut s = HeightNoiseSource::new();
    let kept = drag(&g, ShapeTool::BasicNoise, &readings, false, &mut s);
    assert_eq!(s.taken(), 11 * n2 as u64, "the press and ten moves each take n n numbers");
    // the pass of the eleventh reading: numbers 10 n n + 1 on, v + 2 E r / 32767 + 0.5 - E, E = 16 * 30
    let mut again = HeightNoiseSource::new();
    again.skip(10 * n2 as u64);
    for (i, &v) in g.heights().iter().enumerate() {
        let r = again.next_number();
        let e = 480.0f64;
        let x = ((2.0 * e) * (r as f64 * (1.0 / 32767.0)) + 0.5) - e + v as f64;
        assert_eq!(kept.heights()[i] as i64, (x.trunc() as i64).clamp(0, 65535), "point {i}");
    }
    // a click: E = 0, no point changes, and n n numbers are taken
    let mut s = HeightNoiseSource::new();
    let click = apply(&g, ShapeTool::BasicNoise, 0, 0, false, &mut s);
    assert_eq!(differing(&click, &g), 0);
    assert_eq!(s.taken(), n2 as u64);
}

// C9 (d): Raise Edges: c0 = r0 = h; a click and a drag to the left change nothing; a drag of h / 4 pixels or
// more to the right raises the outermost rows and columns to 65535.
#[test]
fn raise_edges() {
    let g = noise(128);
    let mut s = HeightNoiseSource::new();
    assert_eq!(differing(&apply(&g, ShapeTool::RaiseEdges, 0, 0, false, &mut s), &g), 0);
    assert_eq!(differing(&apply(&g, ShapeTool::RaiseEdges, -40, 0, false, &mut s), &g), 0);
    let up = apply(&g, ShapeTool::RaiseEdges, 16, 0, false, &mut s); // h / 4 = 16
    for i in 0..128 {
        // the outermost row and column 0 lie at distance 64 = h: w = 64 * 4 * 16 / 64 = 64 = h
        assert_eq!(up.at(0, i), 65535);
        assert_eq!(up.at(i, 0), 65535);
    }
    // the middle (distance 0): w = 0, unchanged
    assert_eq!(up.at(64, 64), g.at(64, 64));
}

// C9 (e), (f): the centred tools on a flat grid, checked with the entry's formulas written out.
#[test]
fn centred_tools() {
    let n = 128usize;
    let h = 64i64;
    let g = HeightGrid::new(n, vec![20000; n * n]).unwrap();
    let mut s = HeightNoiseSource::new();
    let (d, e) = (10i64, -6i64);
    let isqrt = |v: i64| (v as f64).sqrt().floor() as i64;
    let (c0, r0) = (h + d, h + e);
    let blob = apply(&g, ShapeTool::BlobMaker, d, e, false, &mut s);
    let bubble = apply(&g, ShapeTool::BubbleRidges, d, e, false, &mut s);
    let cross = apply(&g, ShapeTool::CrossRidges, d, e, false, &mut s);
    let cross2 = apply(&g, ShapeTool::CrossRidges2, d, e, false, &mut s);
    let big_r_cross = c0.min(n as i64 - c0).min(r0).min(n as i64 - r0);
    for r in 0..n as i64 {
        for c in 0..n as i64 {
            let (dc, dr) = ((c - c0).abs(), (r - r0).abs());
            let gap = 65535 - 20000i64;
            let a = isqrt(dr * dr + dc * dc);
            let rb = h - d;
            let ab = a.max(0).min(rb);
            assert_eq!(blob.at(r as usize, c as usize) as i64, (20000 + ((rb - ab) * gap).div_euclid(h)).clamp(0, 65535));
            let ap = a.max(0).min(h);
            assert_eq!(bubble.at(r as usize, c as usize) as i64, (20000 + ((h - ap) * gap).div_euclid(h)).clamp(0, 65535));
            let m = dc.min(dr).max(0).min(big_r_cross);
            assert_eq!(cross.at(r as usize, c as usize) as i64, (20000 + (gap * m).div_euclid(h)).clamp(0, 65535));
            assert_eq!(cross2.at(r as usize, c as usize) as i64, (20000 + ((big_r_cross - m) * gap).div_euclid(h)).clamp(0, 65535));
        }
    }
}

// C9: nothing is done at a side that is not a power of two from 8 to 1024 (5.0: up to 4096).
#[test]
fn sides_of_the_shape_tools() {
    assert!(side_works(8, false) && side_works(1024, false) && !side_works(2048, false));
    assert!(side_works(2048, true) && side_works(4096, true) && !side_works(4, true) && !side_works(100, true));
    let g = HeightGrid::new(4, (0..16).collect()).unwrap();
    let mut s = HeightNoiseSource::new();
    for t in [ShapeTool::BasicNoise, ShapeTool::RaiseEdges, ShapeTool::BlobMaker, ShapeTool::ReliefNoise] {
        assert_eq!(differing(&apply(&g, t, 30, 30, false, &mut s), &g), 0);
    }
    assert_eq!(s.taken(), 0);
}

// C10: Relief Noise: columns 1 to n - 2 become limited(cut down(sum of weight * height / 2048)); columns
// 0 and n - 1 keep their heights; no numbers.
#[test]
fn relief_noise() {
    let g = noise(64);
    let mut s = HeightNoiseSource::new();
    let out = apply(&g, ShapeTool::ReliefNoise, 0, 0, false, &mut s);
    assert_eq!(s.taken(), 0);
    let w = [[2330i64, 258, -1147], [924, -4474, 925], [1147, 258, 2330]];
    for r in 0..64usize {
        for c in 0..64usize {
            if c == 0 || c == 63 {
                assert_eq!(out.at(r, c), g.at(r, c));
                continue;
            }
            let mut sum = 0i64;
            for (k, rr) in [r.saturating_sub(1), r, (r + 1).min(63)].into_iter().enumerate() {
                for j in 0..3 {
                    sum += w[k][j] * g.at(rr, c + j - 1) as i64;
                }
            }
            assert_eq!(out.at(r, c) as i64, sum.div_euclid(2048).clamp(0, 65535));
        }
    }
}

// C11: Subplateaus: four passes of C5's step (d) as 3.0 and 3.1, (n - 2)^2 numbers each; as 5.0 two passes
// at sides up to 128, then two more for each doubling (four at 256), then S once.
#[test]
fn subplateaus_numbers() {
    let g = noise(128);
    let per_pass = 126u64 * 126;
    let mut s = HeightNoiseSource::new();
    apply(&g, ShapeTool::Subplateaus, 0, 0, false, &mut s);
    assert_eq!(s.taken(), 4 * per_pass);
    let mut s = HeightNoiseSource::new();
    apply(&g, ShapeTool::Subplateaus, 0, 0, true, &mut s);
    assert_eq!(s.taken(), 2 * per_pass, "5.0: one pair at 128");
    let g2 = noise(256);
    let mut s = HeightNoiseSource::new();
    apply(&g2, ShapeTool::Subplateaus, 0, 0, true, &mut s);
    assert_eq!(s.taken(), 4 * 254 * 254, "5.0: one pair more at 256");
    // on flat ground each pass lowers the inner points by 12 where the step writes (C5)
    let flat = HeightGrid::new(64, vec![30000; 64 * 64]).unwrap();
    let mut s = HeightNoiseSource::new();
    let out = apply(&flat, ShapeTool::Subplateaus, 0, 0, false, &mut s);
    assert!(out.heights().iter().all(|&v| v <= 30000));
}

// : the clipping range: lo = cut toward zero(lower 65535) with lower held at 0 or above; the OK step
// sets an end within 1e-10 of 0 to -0.00001 and of 1 to 1.00001.
#[test]
fn clipping_range_and_ok_step() {
    let full = Bracket { lower: -0.00001, upper: 1.00001 };
    assert_eq!(full.range(), ClipRange::FULL);
    // the measured bounds: upper 0.6109049417248369 gave 40035 and lower 0.38909505827516316 gave 25499
    assert_eq!(Bracket { lower: -0.00001, upper: 0.6109049417248369 }.range().hi, 40035);
    assert_eq!(Bracket { lower: 0.38909505827516316, upper: 1.00001 }.range().lo, 25499);
    let mut b = Bracket { lower: 0.0, upper: 1.0 };
    b.ok_step();
    assert_eq!((b.lower, b.upper), (-0.00001, 1.00001));
    // 3.1's middle drag ends at 0 and 1.00002: the lower end is set, the upper is kept
    let mut b = Bracket { lower: 0.0, upper: 1.00002 };
    b.ok_step();
    assert_eq!((b.lower, b.upper), (-0.00001, 1.00002));
    let mut b = Bracket { lower: 0.38909505827516316, upper: 0.6109049417248369 };
    b.ok_step();
    assert_eq!((b.lower, b.upper), (0.38909505827516316, 0.6109049417248369));
    // a drag brings the lower end to 0 by its limit: the OK saves -0.00001
    let mut b = Bracket { lower: 0.00004, upper: 1.00001 };
    b.drag(stecyk3d::terrain_editor::BracketPart::Lower, 100, 257);
    assert_eq!(b.lower, 0.0);
    b.ok_step();
    assert_eq!(b.lower, -0.00001);
}

// : a drag by an amount changes only points whose height lies from lo to hi; Apply of the
// default graph with the top end dragged down remaps only points of height at most 40035 (10008 of
// the ramp's 16384 points changed); (b), (c) are tested on the tools' own results below.
#[test]
fn tools_inside_the_range() {
    let n = 128;
    let g = HeightGrid::new(n, (0..n * n).map(|i| (4 * i) as u16).collect()).unwrap(); // the ramp
    let range = Bracket { lower: -0.00001, upper: 0.6109049417248369 }.range();
    assert_eq!(range.hi, 40035);
    let mut graph = default_graph();
    for (i, e) in graph.iter_mut().enumerate() {
        *e = (i * 100) as u16; // not the default: every point moves
    }
    let all = apply_filter(&g, &graph);
    let part = apply_filter_in(&g, &graph, range);
    for (i, &v) in g.heights().iter().enumerate() {
        if v as i64 <= 40035 {
            assert_eq!(part.heights()[i], all.heights()[i]);
        } else {
            assert_eq!(part.heights()[i], v);
        }
    }
    // an amount by the point formula inside the range only
    let click = stecyk3d::terrain_editor::invert(&g);
    let moved = at_amount(&g, &click, 27);
    let inside = at_amount_in(&g, &click, 27, range);
    for (i, &v) in g.heights().iter().enumerate() {
        let want = if v as i64 <= 40035 { moved.heights()[i] } else { v };
        assert_eq!(inside.heights()[i], want);
    }
    // Dampen's click (P = 64) is that blend at its click's amount: only points inside change
    let damp = dampen_at_amount_in(&g, 64, range);
    for (i, &v) in g.heights().iter().enumerate() {
        if v as i64 > 40035 {
            assert_eq!(damp.heights()[i], v);
        }
    }
    assert_eq!(
        differing(&dampen_at_amount_in(&g, 64, ClipRange::FULL), &stecyk3d::terrain_editor::dampen(&g)),
        0,
        "rule 2 is that blend at its click's amount"
    );
}

// : S leaves a value outside lo..hi alone in each of its two passes. With every height of the
// grid below lo, Subcontours' mask is not smoothed at all: the result is M + cut toward zero(40 (v - M) /
// 100) with M the zeroed copy of the heights (C2 without its smoothing). At the full range the mask is
// smoothed and the result differs. (C8 (c), the droplets' range, at the full range is C8's "h' > 0".)
#[test]
fn smoothing_reads_the_range() {
    let n = 64usize;
    let g = noise(n);
    let below = HeightGrid::new(n, g.heights().iter().map(|&v| v / 8).collect()).unwrap(); // up to 8191
    let at = |r: usize, c: usize| below.at(r, c) as i64;
    let mut m: Vec<i64> = below.heights().iter().map(|&v| v as i64).collect();
    for r in 0..n {
        for c in 1..n - 1 {
            if (at(r, c - 1) - at(r, c + 1)).abs() > 960 {
                m[r * n + c] = 0;
            }
        }
    }
    for r in 1..n - 1 {
        for c in 0..n {
            if (at(r - 1, c) - at(r + 1, c)).abs() > 960 {
                m[r * n + c] = 0;
            }
        }
    }
    let narrowed = ClipRange { lo: 20000, hi: 65535 };
    let got = stecyk3d::terrain_tools::subcontours_in(&below, false, narrowed);
    for i in 0..n * n {
        let v = below.heights()[i] as i64;
        assert_eq!(got.heights()[i] as i64, m[i] + 40 * (v - m[i]) / 100, "point {i}");
    }
    assert!(differing(&stecyk3d::terrain_tools::subcontours_in(&below, false, ClipRange::FULL), &got) > 0);
}

// Variants: as 5.0 Subcontours' mask is zeroed where the height of the right neighbour minus that
// of the left is more than t = cut toward zero(5.5 n) (704 at n = 128) or below 0, and likewise down
// minus up; the grid whose rows rise by 352 up to row 64 and by 353 after it (so heights above and below
// a point differ by 704 up to row 63, 705 at row 64, 706 from row 65) is the entry's own probe.
#[test]
fn subcontours_as_five() {
    use stecyk3d::terrain_tools::subcontours_in;
    let n = 128usize;
    let heights: Vec<u16> = (0..n * n)
        .map(|i| {
            let r = i / n;
            let rise = if r <= 64 { 352 * r } else { 352 * 64 + 353 * (r - 64) };
            (1000 + rise) as u16
        })
        .collect();
    let g = HeightGrid::new(n, heights).unwrap();
    let five = subcontours_in(&g, true, ClipRange::FULL);
    let three = subcontours_in(&g, false, ClipRange::FULL);
    // each row rises, so the rows are zeroed as 5.0 reads "below minus above more than t" at 705 and
    // 706 only; the two versions differ somewhere on this grid
    assert!(differing(&five, &three) > 0);
    // a falling row is zeroed as 5.0 (the signed test) and, in the same place, a rising one by less
    // than t is kept: a flat grid is unchanged in both
    let flat = HeightGrid::new(n, vec![30000; n * n]).unwrap();
    assert_eq!(differing(&subcontours_in(&flat, true, ClipRange::FULL), &flat), 0);
}

// rule 8: Square and Round Edges by an amount: a click (d = 0) is rule 7; the entry's two observed
// cases left every point 0 (Square d = 30, Round d = -60 as 5.0).
#[test]
fn edges_by_an_amount() {
    let g = noise(128);
    assert_eq!(differing(&square_edges_at(&g, 0), &square_edges(&g)), 0);
    assert_eq!(differing(&round_edges_at(&g, 0), &round_edges(&g)), 0);
    // Square with d = 30: k = max(0, min(h, cut down(m (h - 120) / h))) with h = 64: h - 4 d < 0 so k = 0
    assert!(square_edges_at(&g, 30).heights().iter().all(|&v| v == 0));
    // Round with d = -60: h + 2 d = -56 < 0 so k = 0
    assert!(round_edges_at(&g, -60).heights().iter().all(|&v| v == 0));
}

// : a stroke is a straight line from the previous place's column toward the new place's,
// each column from the lesser x up to but not including the greater; G = 512 (127 - y).
#[test]
fn pencil_strokes() {
    // as 5.0 the press sets nothing and the place before the first place read is the pressed place
    let mut g = default_graph();
    {
        let mut pencil = Pencil::press(&mut g, true, (10, 20));
        pencil.read((14, 20));
    }
    for c in 10..14 {
        assert_eq!(g[c], 512 * (127 - 20), "column {c}");
    }
    assert_eq!(g[14], default_graph()[14], "the column of the latest place is not set");
    assert_eq!(g[9], default_graph()[9]);
    // as 3.0 and 3.1 the press sets G of the pressed column at once and the place one right and one
    // below counts as the place before the first place read
    let mut g = default_graph();
    {
        let mut pencil = Pencil::press(&mut g, false, (10, 20));
        pencil.read((14, 20));
    }
    assert_eq!(g[10], 512 * (127 - 20));
    // p = (11, 21), q = (14, 20): columns 11, 12, 13 with s = 3 and f = 1, 2/3, 1/3; q.y < p.y:
    // y = q.y + (p.y - q.y) f, cut toward zero: 21, 20, 20
    assert_eq!(g[11], 512 * (127 - 21));
    assert_eq!(g[12], 512 * (127 - 20));
    assert_eq!(g[13], 512 * (127 - 20));
    // the same place again changes nothing; a place left of the previous one runs the line leftward
    let mut g = default_graph();
    {
        let mut pencil = Pencil::press(&mut g, true, (50, 100));
        pencil.read((50, 100));
        pencil.read((40, 100));
    }
    assert_eq!(g[50], default_graph()[50]);
    for c in 40..50 {
        assert_eq!(g[c], 512 * (127 - 100), "column {c}");
    }
    // the highest entry a stroke sets is 65024 and the lowest 0
    let mut g = [1u16; GRAPH_LEN];
    {
        let mut pencil = Pencil::press(&mut g, true, (0, 0));
        pencil.read((5, 0));
        pencil.read((5, 127));
        pencil.read((10, 127));
    }
    assert_eq!(g[0], 65024);
    assert_eq!(g[5], 0);
}

// : the menu. The tab starts with Bryce Classic, the three switches on and no
// tiling choice; the switches and tiling choices are greyed with Bryce Classic; choosing a switch turns it
// over; choosing a tiling choice other than the current one makes it current and turns the three switches
// off; choosing the current one again leaves none.
#[test]
fn fractal_menu() {
    assert_eq!(KINDS.len(), 27);
    assert_eq!(KINDS[0], FractalKind::BryceClassic);
    assert_eq!(KINDS[3], FractalKind::RoundedDunes);
    assert_eq!(KINDS[25], FractalKind::PlanetX);
    assert_eq!(KINDS[26], FractalKind::MudCracks);
    let mut m = FractalMenu::new();
    assert!(m.random_extent && m.random_position && m.random_character && m.tiling.is_none());
    m.choose_switch(Switch::RandomExtent);
    m.choose_tiling(Tiling::North);
    assert!(m.random_extent && m.tiling.is_none(), "greyed with Bryce Classic: nothing happens");
    m.choose_kind(FractalKind::RoundedDunes);
    m.choose_switch(Switch::RandomExtent);
    assert!(!m.random_extent && m.random_position);
    m.choose_switch(Switch::RandomExtent);
    assert!(m.random_extent);
    m.choose_tiling(Tiling::North);
    assert_eq!(m.tiling, Some(Tiling::North));
    assert!(!m.random_extent && !m.random_position && !m.random_character);
    m.choose_tiling(Tiling::North);
    assert_eq!(m.tiling, None);
    m.choose_tiling(Tiling::East);
    m.choose_tiling(Tiling::West);
    assert_eq!(m.tiling, Some(Tiling::West));
}

// : the five items' forms; "Blend, Pict Blend and
// Add coincide at s = 128 in both versions"; the control moves half a unit a pixel, to the left raising s
// (148 after one move of 40 pixels to the left with the button held), limited to 0 to 256.
#[test]
fn pictures_result_as_three() {
    use stecyk3d::terrain_pictures::{control_after_reading, result_value, ResultMode};
    for a in [0i64, 1, 77, 128, 254, 255] {
        for b in [0i64, 3, 100, 200, 255] {
            let blend = result_value(ResultMode::Blend, a, b, 128);
            assert_eq!(blend, result_value(ResultMode::PictBlend, a, b, 128));
            assert_eq!(blend, result_value(ResultMode::Add, a, b, 128), "a {a} b {b}");
            // the forms written out at s = 148
            let s = 148;
            assert_eq!(result_value(ResultMode::Blend, a, b, s), b + (a - b).wrapping_mul(s).div_euclid(256));
            assert_eq!(result_value(ResultMode::Minimum, a, b, s), (b * (256 - s) + a.min(b) * s) / 256);
            assert_eq!(result_value(ResultMode::Subtract, a, b, s), ((b * s - a * (256 - s)) / 256).clamp(0, 255));
            assert_eq!(result_value(ResultMode::Add, a, b, s), ((a * (256 - s) + b * s) / 256).clamp(0, 255));
        }
    }
    assert_eq!(control_after_reading(128.0, 40), 148.0);
    assert_eq!(control_after_reading(128.0, -40), 108.0);
    assert_eq!(control_after_reading(250.0, 40), 256.0);
    assert_eq!(control_after_reading(3.0, -40), 0.0);
}

// Variants (5.0, slot B filled from the slot menu, writers w144 and w177): Black gives b = 0 and the
// first mode at the tab's own position cut down(a / 2); Gray 50% gives b = 32767 and the first mode
// cut down(a / 2) + 16383; the fifth mode is cut toward zero(a b (u c) + a w) with c = 1 / 65535 (a for a
// white slot B).
#[test]
fn pictures_slot_fills_as_five() {
    use stecyk3d::terrain_editor::{pictures_blend_v50, PicturesMode};
    use stecyk3d::terrain_pictures::SlotFill;
    assert_eq!(
        [SlotFill::Black, SlotFill::Gray25, SlotFill::Gray50, SlotFill::Gray75, SlotFill::White].map(SlotFill::height),
        [0, 16383, 32767, 49151, 65535]
    );
    let a = noise(64);
    let fill = |f: SlotFill| HeightGrid::new(64, vec![f.height(); 64 * 64]).unwrap();
    let black = pictures_blend_v50(&a, &fill(SlotFill::Black), 128.0, PicturesMode::First);
    let gray = pictures_blend_v50(&a, &fill(SlotFill::Gray50), 128.0, PicturesMode::First);
    for (i, &v) in a.heights().iter().enumerate() {
        assert_eq!(black.heights()[i], v / 2);
        assert_eq!(gray.heights()[i], v / 2 + 16383);
    }
    let white = fill(SlotFill::White);
    let fifth = pictures_blend_v50(&a, &white, 148.0, PicturesMode::Fifth);
    assert_eq!(differing(&fifth, &a), 0, "with b white the fifth item gives a itself at s = 148");
}

// : the frame of a click, with a made-up noise table (the original's table
// is the substrate's; no height here is claimed to be the original's): kinds not described are refused,
// Bryce Classic is C6, a point outside the clipping range keeps its height, and a click is repeatable
// from the same generator state.
#[test]
fn fractal_click_frame() {
    use stecyk3d::numerics::Bryce5Rand;
    use stecyk3d::terrain_fractal::{click, FractalRefusal, FractalState};
    use stecyk3d::textures::five::FiveSets;
    let mut bits = [0u32; 64];
    for (i, b) in bits.iter_mut().enumerate() {
        *b = (i as u32 + 1).wrapping_mul(2654435761) ^ 0x9e3779b9;
    }
    let sets = FiveSets::new(&bits);
    let g = ramp(16);
    let mut menu = FractalMenu::new();
    let mut state = FractalState::new();
    let mut rng = Bryce5Rand::new(1, &bits);
    assert_eq!(
        click(&g, &menu, &mut state, &mut rng, ClipRange::FULL, None, &bits, &sets).unwrap_err(),
        FractalRefusal::Classic
    );
    menu.choose_kind(FractalKind::Lava);
    assert_eq!(
        click(&g, &menu, &mut state, &mut rng, ClipRange::FULL, None, &bits, &sets).unwrap_err(),
        FractalRefusal::Undescribed(FractalKind::Lava)
    );
    menu.choose_kind(FractalKind::RoundedDunes);
    let mut a = Bryce5Rand::new(7, &bits);
    let mut b = Bryce5Rand::new(7, &bits);
    let first = click(&g, &menu, &mut FractalState::new(), &mut a, ClipRange::FULL, None, &bits, &sets).unwrap();
    let again = click(&g, &menu, &mut FractalState::new(), &mut b, ClipRange::FULL, None, &bits, &sets).unwrap();
    assert_eq!(differing(&first, &again), 0);
    assert!(differing(&first, &g) > 0);
    // a narrowed range: a point outside lo..hi keeps its height
    let range = ClipRange { lo: 100, hi: 400 };
    let mut c = Bryce5Rand::new(7, &bits);
    let part = click(&g, &menu, &mut FractalState::new(), &mut c, range, None, &bits, &sets).unwrap();
    for (i, &v) in g.heights().iter().enumerate() {
        if !range.holds(v as i64) {
            assert_eq!(part.heights()[i], v, "point {i}");
        } else {
            assert_eq!(part.heights()[i], first.heights()[i], "point {i}");
        }
    }
}
