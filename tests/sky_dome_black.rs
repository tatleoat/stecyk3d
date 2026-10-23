// SPDX-License-Identifier: MIT
// A black dome colour at the levels of 8 bits ("The dome light and a black
// dome colour"): the dome light is black when for each channel the whole part of 255 Uk, cut toward
// zero, is a multiple of 256. The values below are the ones the answer's pictures decided, with the
// update's (point 7): no bound, the exact product, not-a-number and infinite channels black.
// , to 4 and 7
// (Sixth release)

use stecyk3d::sky::colours::dome_colour_black;

const ONE_LEVEL: f32 = f32::from_bits(0x3b80_8081); // the single nearest 1/255
const JUST_BELOW: f32 = f32::from_bits(0x3b80_8080);

#[test]
fn black_at_eight_bits() {
    for u in [
        [0.0, 0.0, 0.0],
        [0.0039, 0.0, 0.0],
        [0.0039, 0.0039, 0.0039],
        [JUST_BELOW, 0.0, 0.0],
        [1.005, 0.0, 0.0],
        [0.0, 1.005, 0.0],
        [-0.0039, 0.0, 0.0],
    ] {
        assert!(dome_colour_black(u), "{u:?} is black (whole parts 0 or 256)");
    }
}

#[test]
fn not_black_when_one_channel_counts() {
    for u in [
        [ONE_LEVEL, 0.0, 0.0],
        [0.0, ONE_LEVEL, 0.0],
        [0.0, 0.0, ONE_LEVEL],
        [-0.01, 0.0, 0.0],
        [0.01, -0.01, 0.0],
        [1.0, 0.0, 0.0],
        [1.01, 0.0, 0.0],
    ] {
        assert!(!dome_colour_black(u), "{u:?} is not black");
    }
}

// The update of (point 7).

/// A single whose 255 times has the whole part w, searched near w / 255 (the product is exact).
fn with_whole(w: f64) -> f32 {
    let base = (w / 255.0) as f32;
    let sign: i64 = if base < 0.0 { -1 } else { 1 };
    for d in -4i64..=4 {
        let c = f32::from_bits((i64::from(base.to_bits()) + d * sign) as u32);
        if (f64::from(c) * 255.0).trunc() == w {
            return c;
        }
    }
    panic!("no single for whole part {w}")
}

#[test]
fn update_black_values() {
    // 512, 768, 2^20, 4,294,967,040 (above 2^31), -256 and -512 in each channel alone, and
    // (512, 768, -256) at once.
    for w in [512.0, 768.0, 1_048_576.0, 4_294_967_040.0, -256.0, -512.0] {
        for ch in 0..3 {
            let mut u = [0.0f32; 3];
            u[ch] = with_whole(w);
            assert!(dome_colour_black(u), "whole part {w} in channel {ch}");
        }
    }
    assert!(dome_colour_black([with_whole(512.0), with_whole(768.0), with_whole(-256.0)]));
}

#[test]
fn update_not_black_values() {
    // 2,550,000,000 is 128 more than a multiple of 256 (above 2^31); 4,294,967,550 is above 2^32.
    for w in [511.0, 513.0, -257.0, 2_550_000_000.0, 4_294_967_550.0] {
        assert!(!dome_colour_black([with_whole(w), 0.0, 0.0]), "whole part {w}");
    }
}

#[test]
fn update_special_values() {
    assert!(dome_colour_black([f32::NAN, 0.0, 0.0]));
    assert!(dome_colour_black([f32::INFINITY, 0.0, 0.0]));
    assert!(dome_colour_black([1.0e17, 0.0, 0.0])); // a multiple of 2^33
    assert!(!dome_colour_black([f32::NAN, 1.0, 0.0])); // one channel that counts is enough
}

#[test]
fn update_exact_product() {
    // Red the single with bits 43810101: 255 U = 65,791.99..., whole part 65,791; a product
    // rounded to a single would be 65,792 = 257 * 256 and give black. The dome light is on.
    assert!(!dome_colour_black([f32::from_bits(0x4381_0101), 0.0, 0.0]));
}
