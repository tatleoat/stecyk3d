// SPDX-License-Identifier: MIT
//! The sky part of a C3dBaseSky resource, in every version a reader knows: Bryce 3D's
//! versions 1 and 2  and Bryce 5's version 7 , and the values a version 2
//! sky has as 5.0 in the numbers it does not hold.
//!
//! (revised and)

use crate::geom::Vec3;
use crate::scene_parts::{Rgb, TextureRef, doubles, reference, rgb, vec3, words};
use crate::scene_records::{ByteOrder, RecordError, double, word};

/// Bryce 3D's versions.
pub const SKY_VERSION_ONE: u32 = 1;
pub const SKY_VERSION_TWO: u32 = 2;
/// Bryce 5's own sky part.
pub const SKY_VERSION_SEVEN: u32 = 7;

/// The bytes a version's fields need (from the start of the sky part), or `None` for a
/// version the reader does not know (3 to 6 and any other: refused).
pub fn sky_part_needed(version: u32) -> Option<usize> {
    match version {
        SKY_VERSION_ONE | SKY_VERSION_TWO => Some(612),
        3 => Some(612),
        4..=6 => Some(2_616),
        SKY_VERSION_SEVEN => Some(13_712),
        _ => None,
    }
}

/// The flag bit the reader sets from the view.
pub const SKY_AXIS_VIEW: u32 = 0x400;

/// The stars and comets of a Bryce 3D sky, which give `second_flags` 7 as 5.0.
const BRYCE3D_STARS: u32 = 0x8000;
/// The switch bit with which a version 2 sky taken over as 5.0 gathers its lit air in 1,000
/// steps (else 500). ("A Bryce 5 sky's own numbers": a Bryce 3D sky taken over
/// gives n = 500, or 1000 where its stored word has bit 0x1000000),
const AIR_STEPS_FINE_BIT: u32 = 0x100_0000;

/// The sky's basic settings, every field names, for each version.
#[derive(Clone, Debug, PartialEq)]
pub struct Sky {
    pub version: u32,
    /// The flag word; bit 0x400 is set by the reader from the view (`Scene::read`).
    pub flags: u32,
    pub cloud_height: f64,
    pub lower_cloud_colour: Rgb,
    pub upper_cloud_colour: Rgb,
    pub cloud_offset: [f64; 2],
    pub cloud_cover: f64,
    pub cloud_amplitude: f64,
    pub cloud_frequency: f64,
    /// Three doubles at offset 80, read by no render.,
    pub spare_80: [f64; 3],
    pub sky_colour: Rgb,
    pub horizon_colour: Rgb,
    pub dome_colour: Rgb,
    pub plain_sky_colour: Rgb,
    pub shadow_intensity: f64,
    pub ambient_colour: Rgb,
    pub ambience: f64,
    pub haze_colour: Rgb,
    pub haze: f64,
    pub fog_colour: Rgb,
    pub fog_height: f64,
    pub fog: f64,
    /// The two cloud layers' textures: the Stratus layer's, then the Cumulus layer's
    /// (resolved in `Scene::clouds`).
    pub cloud_textures: [Option<TextureRef>; 2],
    /// The animation tracks: 43 (versions 1 and 2) or 56 (version 7;'s 43 first).
    pub tracks: Vec<u32>,
    pub moon: [f64; 4],
    /// The fifteen discs and rings numbers: 0 r, 1 r', 2 and 3 the haze's and fog's share of
    /// the sun's colour, 4 κ, 5 w1, 6 w2, 7 to 14 the rainbow (Open).
    pub discs: [f64; 15],
    /// The amount of lit air.,
    pub lit_air: f64,
    /// The wind's direction, speed and turbulence (units Open).
    pub wind: [f64; 3],
    /// The edge noise's two numbers (alpha, zeta) where the sky's tracks were applied (opening):
    /// alpha the wind's turbulence, zeta half the number of the wind's steps. `None` where no
    /// track was applied.,
    pub wind_edge: Option<[f64; 2]>,
    /// The star field's seed, signed.,
    pub star_seed: i32,
    pub star_count: u32,
    pub comet_count: u32,
    /// The numbers only Bryce 5's sky holds: `Some` for version 7, `None` for 1 and 2 (for
    /// their values as 5.0: `Sky::as_five`).
    pub five: Option<SkyFive>,
    /* For a part of version 4 to 7: the same bytes read with the layout of version 2, which
    is how a render as 3.0 takes any version word but 1 (Errors). `None` otherwise.
    */
    pub as_bryce30: Option<Box<Sky>>,
    /* Whether, read as 3.0 at version 2's places, the part names a texture or a track the
    document does not hold, so that 3.0 does not open it. Set by `Scene::read` for versions
    3 to 7.*/
    pub bryce30_unopened: bool,
}

/// The numbers Bryce 5's sky part added.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyFive {
    pub haze_shift_colour: Rgb,
    pub haze_offset: f64,
    pub haze_clip: f64,
    pub haze_thickness: f64,
    pub fog_offset: f64,
    pub fog_clip: f64,
    /// `None` only as 5.0 for a version 2 sky whose switch word has bit 0x1000000 (Open).
    pub air_steps: Option<u32>,
    pub star_intensity: f64,
    pub comet_intensity: f64,
    pub cloud_detail_z: f64,
    pub cloud_detail: f64,
    pub second_flags: u32,
    pub star_orientation: Vec3,
    pub sun_glow_weight: f64,
    pub moon_glow_weight: f64,
    pub sun_disc_weight: f64,
    pub moon_disc_weight: f64,
}

impl SkyFive {
    /// The values a version 2 sky part has as 5.0 in the numbers it does not hold, with the
    /// substrate's `haze-five` numbers (offset, thickness, fog offset) and the stored switch
    /// word.
    pub fn of_bryce3d(flags: u32, haze_five: [f64; 3]) -> SkyFive {
        let [haze_offset, haze_thickness, fog_offset] = haze_five;
        SkyFive {
            haze_shift_colour: [0.0, 0.05, 0.25],
            haze_offset,
            haze_clip: f64::INFINITY,
            haze_thickness,
            fog_offset,
            fog_clip: f64::INFINITY,
            air_steps: Some(if flags & AIR_STEPS_FINE_BIT != 0 { 1000 } else { 500 }),
            star_intensity: 1.0,
            comet_intensity: 1.0,
            cloud_detail_z: 0.5,
            cloud_detail: 0.5,
            second_flags: if flags & BRYCE3D_STARS != 0 { 7 } else { 1 },
            star_orientation: [0.0, 1.0, 0.0],
            sun_glow_weight: 1.0,
            moon_glow_weight: 1.0,
            sun_disc_weight: 1.0,
            moon_disc_weight: 1.0,
        }
    }
}

impl Sky {
    /// Decode the sky part. `b` is the data after the first part; bytes past the version's
    /// fields are ignored. A version other than 1, 2 or 7 is refused (versions 3 to 6: no
    /// document holds them), and so is data shorter than the version's fields need.
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<Sky, RecordError> {
        if b.len() < 4 {
            return Err(RecordError::WrongLength { record: "sky", len: b.len(), expected: 4 });
        }
        let version = word(b, 0, o);
        let Some(needed) = sky_part_needed(version) else {
            return Err(RecordError::UnknownVersion { record: "sky", version });
        };
        if b.len() < needed {
            return Err(RecordError::WrongLength { record: "sky", len: b.len(), expected: needed });
        }
        let mut sky = Self::decode(b, o, version);
        if (4..=SKY_VERSION_SEVEN).contains(&version) {
            // : as 3.0 the layout of version 2, whatever the version word
            sky.as_bryce30 = Some(Box::new(Self::decode(b, o, SKY_VERSION_TWO)));
        }
        Ok(sky)
    }

    // The fields of `b` (long enough for `version`) as the layout of `version` places them.
    fn decode(b: &[u8], o: ByteOrder, version: u32) -> Sky {
        let mut sky = Sky {
            version,
            // : the same places from 4 to 199
            flags: word(b, 4, o),
            cloud_height: double(b, 8, o),
            lower_cloud_colour: rgb(b, 16, o),
            upper_cloud_colour: rgb(b, 28, o),
            cloud_offset: doubles(b, 40, o),
            cloud_cover: double(b, 56, o),
            cloud_amplitude: double(b, 64, o),
            cloud_frequency: double(b, 72, o),
            spare_80: doubles(b, 80, o),
            sky_colour: rgb(b, 104, o),
            horizon_colour: rgb(b, 116, o),
            dome_colour: rgb(b, 128, o),
            plain_sky_colour: rgb(b, 140, o),
            shadow_intensity: double(b, 152, o),
            ambient_colour: rgb(b, 160, o),
            ambience: double(b, 172, o),
            haze_colour: rgb(b, 180, o),
            haze: double(b, 192, o),
            fog_colour: [0.0; 3],
            fog_height: 0.0,
            fog: 0.0,
            cloud_textures: [None, None],
            tracks: Vec::new(),
            moon: [0.0; 4],
            discs: [0.0; 15],
            lit_air: 0.0,
            wind: [0.0; 3],
            wind_edge: None,
            star_seed: 0,
            star_count: 0,
            comet_count: 0,
            five: None,
            as_bryce30: None,
            bryce30_unopened: false,
        };
        if version == SKY_VERSION_SEVEN {
            sky.read_seven(b, o);
        } else if (4..=6).contains(&version) {
            sky.read_four_to_six(b, o);
        } else {
            sky.read_bryce3d(b, o);
        }
        sky
    }

    /// Versions 1 and 2 from offset 200.
    fn read_bryce3d(&mut self, b: &[u8], o: ByteOrder) {
        self.fog_colour = rgb(b, 200, o);
        self.fog_height = double(b, 212, o);
        self.fog = double(b, 220, o);
        self.cloud_textures = [reference(b, 228, 236, o), reference(b, 232, 240, o)];
        self.tracks = words::<43>(b, 244, o).to_vec();
        self.moon = doubles(b, 416, o);
        self.discs = doubles(b, 448, o);
        self.lit_air = double(b, 568, o);
        self.wind = doubles(b, 576, o);
        self.read_star_field(b, 600, o);
    }

    /// Version 7 from offset 200.
    fn read_seven(&mut self, b: &[u8], o: ByteOrder) {
        self.fog_colour = rgb(b, 236, o);
        self.fog_height = double(b, 248, o);
        self.fog = double(b, 256, o);
        self.cloud_textures = [reference(b, 280, 288, o), reference(b, 284, 292, o)];
        self.tracks = words::<56>(b, 296, o).to_vec();
        self.moon = doubles(b, 520, o);
        self.discs = doubles(b, 552, o);
        self.lit_air = double(b, 672, o);
        self.wind = doubles(b, 684, o);
        self.read_star_field(b, 708, o);
        self.five = Some(SkyFive {
            haze_shift_colour: rgb(b, 200, o),
            haze_offset: double(b, 212, o),
            haze_clip: double(b, 220, o),
            haze_thickness: double(b, 228, o),
            fog_offset: double(b, 264, o),
            fog_clip: double(b, 272, o),
            air_steps: Some(word(b, 680, o)),
            star_intensity: double(b, 720, o),
            comet_intensity: double(b, 728, o),
            cloud_detail_z: double(b, 13_636, o),
            cloud_detail: double(b, 13_644, o),
            second_flags: word(b, 13_652, o),
            star_orientation: vec3(b, 13_656, o),
            sun_glow_weight: double(b, 13_680, o),
            moon_glow_weight: double(b, 13_688, o),
            sun_disc_weight: double(b, 13_696, o),
            moon_disc_weight: double(b, 13_704, o),
        });
    }

    /// Versions 4 to 6 (3,064 bytes): version 7's fields up to byte 295 at the same places, 51
    /// track identities, the rest 20 bytes lower, the tail at 2,596. Version 4 takes no value from
    /// the bytes 200 to 235, 264 to 279, 660 and 700 to 715; versions 4 and 5 turn bit 0x8000 of
    /// the flags into bits 0x2 and 0x4 of the second word.
    fn read_four_to_six(&mut self, b: &[u8], o: ByteOrder) {
        self.fog_colour = rgb(b, 236, o);
        self.fog_height = double(b, 248, o);
        self.fog = double(b, 256, o);
        self.cloud_textures = [reference(b, 280, 288, o), reference(b, 284, 292, o)];
        self.tracks = (0..51).map(|k| word(b, 296 + 4 * k, o)).collect();
        self.moon = doubles(b, 500, o);
        self.discs = doubles(b, 532, o);
        self.lit_air = double(b, 652, o);
        self.wind = doubles(b, 664, o);
        self.read_star_field(b, 688, o);
        let mut second_flags = word(b, 2_612, o);
        if self.version <= 5 && self.flags & BRYCE3D_STARS != 0 {
            self.flags &= !BRYCE3D_STARS;
            second_flags |= 0x6;
        }
        let base = SkyFive::of_bryce3d(0, [0.0; 3]);
        let mut five = SkyFive {
            cloud_detail_z: double(b, 2_596, o),
            cloud_detail: double(b, 2_604, o),
            second_flags,
            ..base
        };
        if self.version != 4 {
            five.haze_shift_colour = rgb(b, 200, o);
            five.haze_offset = double(b, 212, o);
            five.haze_clip = double(b, 220, o);
            five.haze_thickness = double(b, 228, o);
            five.fog_offset = double(b, 264, o);
            five.fog_clip = double(b, 272, o);
            five.air_steps = Some(word(b, 660, o));
            five.star_intensity = double(b, 700, o);
            five.comet_intensity = double(b, 708, o);
        }
        self.five = Some(five);
    }

    /// The star field's three words: the seed signed, the stars, the comets.
    fn read_star_field(&mut self, b: &[u8], at: usize, o: ByteOrder) {
        let [seed, stars, comets] = words::<3>(b, at, o);
        self.star_seed = seed as i32;
        self.star_count = stars;
        self.comet_count = comets;
    }

    /// The flag word with bit 0x80 as 3.0 and 3.1 leave it when they open the document: the
    /// bit stays clear if stored clear; if set it stays only when one of H, F, G is not 0 (each
    /// formed in binary64 and cut toward zero from the stored haze, fog and fog height).
    pub fn flags_opened_by_bryce3d(&self) -> u32 {
        const AIR: u32 = 0x80;
        if self.flags & AIR == 0 {
            return self.flags;
        }
        let mut h = ((self.haze * 0.2) * 100.0) as i64;
        if h == 0 && self.haze > 0.0 {
            h = 1;
        }
        let f = ((self.fog * 100.0) + 0.5) as i64;
        let g = (((self.fog_height * 20.0) * 0.01).sqrt() * 100.0) as i64;
        if h == 0 && f == 0 && g == 0 {
            self.flags & !AIR
        } else {
            self.flags
        }
    }

    /// The form a render as 5.0 draws the part in: version 3 as version 2, versions 4 to 6 as
    /// version 7 (same values; the original opens documents of those versions and draws them so).
    pub fn form(&self) -> u32 {
        match self.version {
            3 => SKY_VERSION_TWO,
            4..=6 => SKY_VERSION_SEVEN,
            v => v,
        }
    }

    /// The scene's shadow setting s as a render as 5.0 takes it: the field `shadows` where the
    /// sky's switch word as 5.0 has bit 0x80 (a part of version 1, 2 or 3 is taken over with
    /// the bit set; versions 4 to 7 keep the stored word's bit), else 1 (no shadow asked).
    pub fn shadow_setting_as_five(&self) -> f64 {
        const AIR_WORD: u32 = 0x80;
        let has_bit = match self.version {
            4..=7 => self.flags & AIR_WORD != 0,
            _ => true,
        };
        if has_bit { self.shadow_intensity } else { 1.0 }
    }

    /// The numbers of Bryce 5's sky as a render as 5.0 reads them: version 7's own, or for
    /// version 2 the values it does not hold (with `haze-five`'s three numbers). `None` for
    /// version 1 (Open: no document holds one).
    pub fn as_five(&self, haze_five: [f64; 3]) -> Option<SkyFive> {
        match self.version {
            SKY_VERSION_SEVEN | 5 | 6 => self.five,
            4 => self.five.map(|f| {
                let b = SkyFive::of_bryce3d(self.flags, haze_five);
                SkyFive { cloud_detail_z: f.cloud_detail_z, cloud_detail: f.cloud_detail, second_flags: f.second_flags, ..b }
            }),
            SKY_VERSION_TWO | 3 => Some(SkyFive::of_bryce3d(self.flags, haze_five)),
            _ => None,
        }
    }
}
