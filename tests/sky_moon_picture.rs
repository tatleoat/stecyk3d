// SPDX-License-Identifier: MIT
// The moon's picture in the moon's disc: properties that follow from the entry's formulas,
// with pictures made up here (no expected value of the original: the picture is the user's).
// , (arle unpacked
// as rle2)

use std::sync::Arc;

use stecyk3d::scene_pictures::decode_picture_packed;
use stecyk3d::sky::colours::SunInputs;
use stecyk3d::sky::moon_image::MoonImage;
use stecyk3d::sky::sky_colours::{SkyColours, SkySettings};
use stecyk3d::version::RenderAs;

fn sky(version: RenderAs, switches: u32) -> SkyColours {
    let sun = SunInputs {
        direction: [0.0, 0.5, 0.866_025_403_784_438_6],
        colour: [1.0, 0.9, 0.8],
        glow_colour: [0.9, 0.8, 0.5],
        intensity: 1.0,
    };
    let mut discs = [0.0; 15];
    discs[0] = 3.5;
    discs[1] = 3.5;
    let settings = SkySettings {
        switches,
        sky_colour: [0.2, 0.4, 0.8],
        horizon_colour: [0.7, 0.8, 0.9],
        plain_colour: [0.3, 0.5, 0.9],
        haze: 0.0,
        moon: [0.0; 4],
        discs,
    };
    SkyColours::new(version, &sun, &settings).expect("a sky with atmosphere")
}

fn uniform(byte: u8) -> Arc<MoonImage> {
    Arc::new(MoonImage::new(256, 256, vec![[byte; 3]; 256 * 256]))
}

/// The ray along the moon's direction: the middle of the disc.
const MOON: [f64; 3] = [0.0, -0.5, -0.866_025_403_784_438_6];

// (step 4: t'k = (((pk − 28)·sl + fl)·gk)·B', so over a uniform picture the colour is
// linear in the byte with intercept fl·g·B' at 28 and slope sl·g·B': as 3.0, sl/fl = 0.55/148.3)
#[test]
fn the_disc_is_linear_in_the_picture_as_3_0() {
    let base = sky(RenderAs::Bryce30, 0x1 | 0x8 | 0x100 | 0x20000);
    let at = |p: u8| base.clone().with_moon_picture(uniform(p)).colour(MOON);
    let (low, high) = (at(28), at(255));
    for k in 0..3 {
        let ratio = f64::from(high[k] - low[k]) / (227.0 * f64::from(low[k]));
        assert!((ratio - 0.55 / 148.3).abs() < 2e-4, "channel {k}: {ratio}");
    }
}

// (step 4 as 3.1 and 5.0: fl = 150 + 40·ν, sl = (255 − fl)/194, so sl/fl lies between
// 65/194/190 and 105/194/150 whatever ν in [0, 1])
#[test]
fn the_disc_is_linear_in_the_picture_as_3_1() {
    let base = sky(RenderAs::Bryce31, 0x1 | 0x8 | 0x100 | 0x20000);
    let at = |p: u8| base.clone().with_moon_picture(uniform(p)).colour(MOON);
    let (low, high) = (at(28), at(255));
    for k in 0..3 {
        let ratio = f64::from(high[k] - low[k]) / (227.0 * f64::from(low[k]));
        assert!(ratio > 65.0 / 194.0 / 190.0 - 1e-4 && ratio < 105.0 / 194.0 / 150.0 + 1e-4, "channel {k}: {ratio}");
    }
}

#[test]
fn outside_the_disc_the_picture_changes_nothing() {
    let plain = sky(RenderAs::Bryce31, 0x1 | 0x8 | 0x100 | 0x20000);
    let with = plain.clone().with_moon_picture(uniform(200));
    for d in [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.5, 0.866_025_403_784_438_6]] {
        assert_eq!(plain.colour(d), with.colour(d));
    }
}

#[test]
fn arle_is_unpacked_as_rle2() {
    // A 2 by 2 picture: planes alpha, red, green less red, blue less red, each four bytes as one run.
    let mut header = vec![0u8; 24];
    header[0..8].copy_from_slice(b"imgeiv32");
    header[8..12].copy_from_slice(&4u32.to_be_bytes());
    header[12..16].copy_from_slice(&2u32.to_be_bytes());
    header[16..20].copy_from_slice(&8u32.to_be_bytes());
    header[20..24].copy_from_slice(&2u32.to_be_bytes());
    let stream = [4u8, 255, 4, 10, 4, 5, 4, 7];
    for code in [b"rle2", b"arle"] {
        let mut packed = vec![0u8; 64];
        packed[0..4].copy_from_slice(code);
        packed.extend_from_slice(&stream);
        let p = decode_picture_packed(&header, &packed, true).expect("decodes");
        assert_eq!(p.pixels()[0].0, [255, 10, 15, 17]);
    }
    let mut packed = vec![0u8; 64];
    packed[0..4].copy_from_slice(b"arle");
    packed.extend_from_slice(&stream);
    assert!(decode_picture_packed(&header, &packed, false).is_err(), "arle is refused where not allowed");
}
