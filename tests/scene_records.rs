// SPDX-License-Identifier: MIT
// Tests of the scene records on parts built to the layouts, and's
// example vector.
// (Records)

use stecyk3d::scene_records::*;

fn put(b: &mut [u8], at: usize, v: u32, order: ByteOrder) {
    let w = match order {
        ByteOrder::Big => v.to_be_bytes(),
        ByteOrder::Little => v.to_le_bytes(),
    };
    b[at..at + 4].copy_from_slice(&w);
}

fn put_f64(b: &mut [u8], at: usize, v: f64, order: ByteOrder) {
    let w = match order {
        ByteOrder::Big => v.to_bits().to_be_bytes(),
        ByteOrder::Little => v.to_bits().to_le_bytes(),
    };
    b[at..at + 8].copy_from_slice(&w);
}

fn code(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

#[test]
fn settings() {
    let o = ByteOrder::Big;
    let mut b = vec![0u8; SETTINGS_LEN];
    put(&mut b, 8, 640, o);
    put(&mut b, 12, 480, o);
    put(&mut b, 24, 480, o);
    put(&mut b, 28, 640, o);
    put(&mut b, 32, 16, o);
    put(&mut b, 68, (-2.5f32).to_bits(), o);
    put(&mut b, 80, 2, o);
    b[95] = 7;
    let s = DocSettings::parse(&b).unwrap();
    assert_eq!((s.picture_width, s.picture_height), (640, 480));
    assert!(s.region_is_whole());
    assert!(s.progress.is_start_over());
    assert_eq!(s.current_time, 0.0); // a negative time counts as 0
    assert_eq!(s.anti_aliasing, 2);
    assert_eq!(s.animation_switch, 7);
    assert!(matches!(DocSettings::parse(&b[..415]), Err(RecordError::WrongLength { .. })));
    // ,: opening as 3.0 applies tracks at the
    // saved time whatever the switch; 3.1 with the switch not 0 at the saved time; 5.0 at the
    // saved time times the opening time factor (the word at 148, here 0), in either form of the
    // parts (the sky's tracks too, no `sky` flag any more).
    use stecyk3d::animation::{OpeningRule, PartsForm};
    use stecyk3d::version::RenderAs;
    let (b3, b5) = (PartsForm::Bryce3d, PartsForm::Bryce5);
    assert_eq!(s.opening_factor, 0.0);
    // Saved at -2.5, which counts as 0.
    let op = s.opening(RenderAs::Bryce50, b5).unwrap();
    assert_eq!((op.rule, op.time), (OpeningRule::Marked { by_factor: true }, 0.0));
    let op = s.opening(RenderAs::Bryce50, b3).unwrap();
    assert_eq!((op.rule, op.time), (OpeningRule::Marked { by_factor: true }, 0.0));
    assert_eq!(s.opening(RenderAs::Bryce31, b3).unwrap().rule, OpeningRule::Marked { by_factor: false });
    // 3.1 in Bryce 5's form: as 3.1 in Bryce 3D's (the pictures of vectors 47 and 48).
    assert_eq!(s.opening(RenderAs::Bryce31, b5).unwrap().rule, OpeningRule::Marked { by_factor: false });
    put(&mut b, 68, 1.5f32.to_bits(), o);
    let s = DocSettings::parse(&b).unwrap();
    assert_eq!(s.current_time, 1.5);
    for form in [b3, b5] {
        let op = s.opening(RenderAs::Bryce30, form).unwrap();
        assert!(op.applies_tracks && op.time == 1.5);
        assert_eq!(s.opening(RenderAs::Bryce31, form).unwrap().time, 1.5);
        // k = 0: time 0 whatever the saved time, in either form.
        assert_eq!(s.opening(RenderAs::Bryce50, form).unwrap().time, 0.0);
    }
    for (k, want) in [(1.0f32, 1.5), (0.5, 0.75), (2.0, 3.0), (-1.0, 0.0), (1e-30, 1.5f32 as f64 * 1e-30f32 as f64)] {
        put(&mut b, 148, k.to_bits(), o);
        let s = DocSettings::parse(&b).unwrap();
        assert_eq!(s.opening_factor, k);
        for form in [b3, b5] {
            assert_eq!(s.opening(RenderAs::Bryce50, form).unwrap().time, want, "k = {k}");
            // 3.0 and 3.1 take no account of the factor.
            assert_eq!(s.opening(RenderAs::Bryce30, form).unwrap().time, 1.5);
            assert_eq!(s.opening(RenderAs::Bryce31, form).unwrap().time, 1.5);
        }
    }
    put(&mut b, 148, 0, o);
    b[95] = 0;
    let s = DocSettings::parse(&b).unwrap();
    for form in [b3, b5] {
        assert!(!s.opening(RenderAs::Bryce50, form).unwrap().applies_tracks);
    }
    // : the structure code, as a four-character code.
    put(&mut b, 176, u32::from_be_bytes(*b"grid"), o);
    assert_eq!(DocSettings::parse(&b).unwrap().structure_code().0, *b"grid");
}

fn list(order: ByteOrder, rows: &[(&[u8; 4], &[u8; 4], u32, u32)]) -> Vec<u8> {
    let mut b = vec![0u8; 12 + 16 * rows.len()];
    put(&mut b, 0, 1, order);
    if order == ByteOrder::Little {
        b[4..8].copy_from_slice(&[1, 0, 0, 0]);
    }
    put(&mut b, 8, rows.len() as u32, order);
    for (i, (k, c, id, link)) in rows.iter().enumerate() {
        let at = 12 + 16 * i;
        put(&mut b, at, code(k), order);
        put(&mut b, at + 4, code(c), order);
        put(&mut b, at + 8, *id, order);
        put(&mut b, at + 12, *link, order);
    }
    b
}

#[test]
fn object_lists_in_either_order() {
    for order in [ByteOrder::Big, ByteOrder::Little] {
        let b = list(order, &[(b"3dob", b"sphr", 5, 0), (b"lght", b"cone", 9, 12), (b"camr", b"base", 3, 0)]);
        let l = ObjectList::parse(&b).unwrap();
        assert_eq!(l.order, order);
        assert_eq!(l.rows.len(), 3);
        assert_eq!(l.rows[0].kind, ObjectKind::Sphere);
        assert_eq!(l.rows[1].kind, ObjectKind::ConeLight);
        assert_eq!(l.rows[1].linked_list, 12);
        assert_eq!(l.rows[2].kind.class_tag(), "C3dBaseCamera");
    }
}

#[test]
fn object_list_refusals() {
    let mut b = list(ByteOrder::Big, &[(b"3dob", b"sphr", 5, 0)]);
    assert!(matches!(ObjectList::parse(&b[..27]), Err(RecordError::WrongLength { .. })));
    put(&mut b, 0, 2, ByteOrder::Big);
    assert_eq!(ObjectList::parse(&b), Err(RecordError::ListFirstWord(2)));
    let b = list(ByteOrder::Big, &[(b"3dob", b"base", 5, 0)]);
    assert!(matches!(ObjectList::parse(&b), Err(RecordError::UnknownCodes { row: 0, .. })));
}

fn placement(order: ByteOrder, first: ([f64; 12],), second: ([f64; 12],)) -> Vec<u8> {
    let mut b = vec![0u8; PLACEMENT_LEN];
    put(&mut b, 0, 1, order);
    put(&mut b, 4, CAMERA_IN_USE, order);
    put(&mut b, 12, code(b"base"), order);
    put(&mut b, 16, 77, order);
    for k in 0..12 {
        put_f64(&mut b, 20 + 8 * k, first.0[k], order);
        put_f64(&mut b, 116 + 8 * k, second.0[k], order);
    }
    b
}

#[test]
fn placement_world_transform_vector() {
    let first = [4.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.5, 0.0, 0.0];
    let second = [0.8, 0.0, -0.6, 0.0, 1.0, 0.0, 0.6, 0.0, 0.8, -8.0, 2.0, 4.0];
    for order in [ByteOrder::Big, ByteOrder::Little] {
        let p = PlacementPart::parse(&placement(order, (first,), (second,)), order).unwrap();
        assert_eq!(p.material, Some(77));
        assert!(p.is_camera_in_use());
        let w = p.world();
        let got: Vec<f64> = w.rows.iter().flatten().chain(w.translation.iter()).copied().collect();
        let want = [3.2, 0.0, -2.4, 0.0, 2.0, 0.0, 1.7999999999999998, 0.0, 2.4000000000000004, -7.6, 2.0, 3.7];
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&got), bits(&want));
    }
}

#[test]
fn placement_refusals() {
    let o = ByteOrder::Big;
    let mut b = placement(o, ([0.0; 12],), ([0.0; 12],));
    put(&mut b, 12, code(b"xxxx"), o);
    assert!(matches!(PlacementPart::parse(&b, o), Err(RecordError::MaterialCode(_))));
    put(&mut b, 12, 0, o);
    assert_eq!(PlacementPart::parse(&b, o).unwrap().material, None);
    put(&mut b, 0, 2, o);
    assert!(matches!(PlacementPart::parse(&b, o), Err(RecordError::UnknownVersion { version: 2, .. })));
}

#[test]
fn torus_part() {
    let o = ByteOrder::Little;
    let mut b = vec![0u8; TORUS_PART_LEN];
    put(&mut b, 0, 1, o);
    put_f64(&mut b, 4, 0.25, o);
    put_f64(&mut b, 12, 1.0, o);
    let t = TorusPart::parse(&b, o).unwrap();
    assert_eq!((t.tube_radius, t.ring_radius), (0.25, 1.0));
}
