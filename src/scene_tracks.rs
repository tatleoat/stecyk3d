// SPDX-License-Identifier: MIT
//! The animation tracks of a document: the track resources' layout, the classes and the kinds
//! of their values, and which class each owner's track slot names.
//!
//! , (the placement's
//! seven slots), (the camera's seven, the sun's three,
//! the material's nineteen, the torus's two), (a place key's
//! settings),
//! (Bryce 5's documents: The tracks)

use std::fmt;

use crate::animation::{Key, MappingWords, OrientKey, PlaceKey, Repeat, TimeMapPoint, Track};
use crate::scene_parts::{Channel, Rgb, colour_channel, rgb, value_channel};
use crate::scene_records::{ByteOrder, double, single, word};
use crate::scene_textures::{RECORD_PART_LEN, ProceduralTexture, RecordForm};
use crate::textures::TextureSettings;

/// The header's length.
pub const HEADER_LEN: usize = 148;
/// Where a key's value starts.
const VALUE_AT: usize = 44;
/// The time map's head, and each point's bytes.
const MAP_HEAD: usize = 44;
const MAP_POINT: usize = 28;
/// The key size of an orientation track that lacks the half-turns word (older .br3
/// documents).
const ORIENT_KEY_SHORT: usize = 76;

/// The kind of a track's value, which sets its key's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Double,
    Single,
    Colour,
    Triple,
    Place,
    Orientation,
    ValueChannel,
    ColourChannel,
    /// A texture mapping's two words (Btx_eMapping).
    Words,
    /// A texture's record part (3dTxt_TxtData).
    Record,
}

impl TrackKind {
    /// The key's size in bytes.
    pub fn key_size(self) -> usize {
        match self {
            TrackKind::Double => 52,
            TrackKind::Single => 48,
            TrackKind::Colour => 56,
            TrackKind::Triple => 68,
            TrackKind::Place => 92,
            TrackKind::Orientation => 80,
            TrackKind::ValueChannel => 84,
            TrackKind::ColourChannel => 100,
            TrackKind::Words => 52,
            TrackKind::Record => 1132,
        }
    }
}

/// The classes's table lays out, by tag, with their kind. [format]
const LAID_OUT: [(&str, TrackKind); 64] = [
    // , (the sky's
    // colours and single numbers: a colour, a double)
    ("Sky_eAmbientC", TrackKind::Colour),
    ("Sky_eAmbientInt", TrackKind::Double),
    ("Sky_eHorizontC", TrackKind::Colour),
    ("Sky_eDomeC", TrackKind::Colour),
    ("Sky_eDefaultC", TrackKind::Colour),
    ("Sky_eHazeAmount", TrackKind::Double),
    ("Sky_eHazeC", TrackKind::Colour),
    ("Sky_eShadowInt", TrackKind::Double),
    ("Sky_eBaseC", TrackKind::Colour),
    // (the sky's single numbers are doubles)
    ("Sky_eWindDir", TrackKind::Double),
    ("Sky_eWindSpeed", TrackKind::Double),
    ("Sky_eWindTurb", TrackKind::Double),
    ("Cam_eFOV", TrackKind::Double),
    ("Cam_ePanX", TrackKind::Double),
    ("Cam_ePanY", TrackKind::Double),
    ("Cam_eZoomX", TrackKind::Double),
    ("Cam_eZoomY", TrackKind::Double),
    ("Cam_eBank", TrackKind::Double),
    ("Light_Int", TrackKind::Double),
    ("Light_Rad", TrackKind::Double),
    ("Light_ShAmb", TrackKind::Double),
    ("Light_Rng", TrackKind::Double),
    ("Light_GrRng", TrackKind::Double),
    ("Light_GrDlt", TrackKind::Double),
    // : the terrain's HF_eClipMin and HF_eClipMax are the double
    // kind (52 bytes a key), their value the terrain's lower and upper bound (offsets 80
    // and 88); what opening does with them is not applied here
    ("HF_eClipMin", TrackKind::Double),
    ("HF_eClipMax", TrackKind::Double),
    ("Tor_eMajorRad", TrackKind::Double),
    ("Tor_eMinorRad", TrackKind::Double),
    ("ConL_eFallOff", TrackKind::Double),
    ("Mat_eRefract", TrackKind::Single),
    ("Mat_eHTEdge", TrackKind::Single),
    ("Mat_eHTSharp", TrackKind::Single),
    ("Mat_eHTQuality", TrackKind::Single),
    ("Obj_ePathT", TrackKind::Single),
    ("Light_C", TrackKind::Colour),
    ("Light_ShAmbCol", TrackKind::Colour),
    ("SunL_eDiscClr", TrackKind::Colour),
    ("SunL_eHaloClr", TrackKind::Colour),
    ("Mat_eVolC", TrackKind::Colour),
    ("Obj_eScale", TrackKind::Triple),
    ("Obj_eShear", TrackKind::Triple),
    ("Cam_eCraOrient", TrackKind::Triple),
    ("SunL_eSunVctr", TrackKind::Triple),
    ("Obj_ePosition", TrackKind::Place),
    ("Obj_eOrigin", TrackKind::Place),
    ("Obj_eOrient", TrackKind::Orientation),
    ("Mat_eDiffuse", TrackKind::ValueChannel),
    ("Mat_eAmbient", TrackKind::ValueChannel),
    ("Mat_eSpecular", TrackKind::ValueChannel),
    ("Mat_eTrans", TrackKind::ValueChannel),
    ("Mat_eReflect", TrackKind::ValueChannel),
    ("Mat_eBump", TrackKind::ValueChannel),
    ("Mat_eMetallic", TrackKind::ValueChannel),
    ("Mat_eHTDensity", TrackKind::ValueChannel),
    ("Mat_eDiffuseC", TrackKind::ColourChannel),
    ("Mat_eAmbientC", TrackKind::ColourChannel),
    ("Mat_eSpecularC", TrackKind::ColourChannel),
    ("Mat_eSpecExp", TrackKind::ColourChannel),
    ("Mat_eTransC", TrackKind::ColourChannel),
    ("Btx_eMapping", TrackKind::Words),
    ("Btx_eTrnScale", TrackKind::Triple),
    ("Btx_eTrnOffset", TrackKind::Triple),
    ("Btx_eTrnRot", TrackKind::Orientation),
    ("3dTxt_TxtData", TrackKind::Record),
];

/// The other classes found in documents whose keys' sizes gives as observed, their
/// values Open here: kept by count only.
const SIZES_ONLY: [(&str, usize); 3] = [
    ("HF_eGradShift", 48),
    ("Mat_eMatFlags", 48),
    ("Cam_eCorpOrient", 76),
];

/// The kind of a class this reader lays out.
pub fn kind_of(tag: &str) -> Option<TrackKind> {
    LAID_OUT.iter().find(|(t, _)| *t == tag).map(|e| e.1)
}

/// Every owner's track slots and the class each names. [format]
pub mod slots {
    /// The placement part's seven (slot 0 names no class).
    pub const PLACEMENT: [Option<&str>; 7] = [
        None,
        Some("Obj_ePathT"),
        Some("Obj_ePosition"),
        Some("Obj_eOrient"),
        Some("Obj_eScale"),
        Some("Obj_eShear"),
        Some("Obj_eOrigin"),
    ];
    /// The light part's eight (version 1's four are the first four). revised
    pub const LIGHT: [&str; 8] =
        ["Light_C", "Light_Int", "Light_Rad", "Light_ShAmb", "Light_Rng", "Light_GrRng", "Light_GrDlt", "Light_ShAmbCol"];
    /// The sun part's three.
    pub const SUN: [&str; 3] = ["SunL_eDiscClr", "SunL_eHaloClr", "SunL_eSunVctr"];
    /// The camera part's seven. Slot 1 may name `CAMERA_SLOT_1_OLDER` in older documents, or
    /// no resource at all.
    pub const CAMERA: [&str; 7] =
        ["Cam_eCraOrient", "Cam_eBank", "Cam_eFOV", "Cam_ePanX", "Cam_ePanY", "Cam_eZoomX", "Cam_eZoomY"];
    /// The class slot 1 of the camera part names in 33 older Bryce 3D documents.
    pub const CAMERA_SLOT_1_OLDER: &str = "Cam_eCorpOrient";
    /// The material part's nineteen.
    pub const MATERIAL: [&str; 19] = [
        "Mat_eDiffuseC",
        "Mat_eAmbientC",
        "Mat_eSpecularC",
        "Mat_eSpecExp",
        "Mat_eTransC",
        "Mat_eDiffuse",
        "Mat_eAmbient",
        "Mat_eSpecular",
        "Mat_eTrans",
        "Mat_eReflect",
        "Mat_eBump",
        "Mat_eMetallic",
        "Mat_eHTDensity",
        "Mat_eMatFlags",
        "Mat_eVolC",
        "Mat_eRefract",
        "Mat_eHTEdge",
        "Mat_eHTSharp",
        "Mat_eHTQuality",
    ];
    /// The spot light's falloff track (its beam record's offset 12). [format]
    pub const BEAM_FALLOFF: &str = "ConL_eFallOff";
    /// The torus part's two, at its offsets 20 and 24.
    pub const TORUS: [&str; 2] = ["Tor_eMinorRad", "Tor_eMajorRad"];
    /// The mapping part's four, at its offsets 92 to 104: mode and switches, scale, offset,
    /// turn.,
    pub const TEXTURE_MAPPING: [&str; 4] = ["Btx_eMapping", "Btx_eTrnScale", "Btx_eTrnOffset", "Btx_eTrnRot"];
    /// The record part's track, at its offset 1084.
    pub const TEXTURE_RECORD: &str = "3dTxt_TxtData";
    /// The terrain's track words 3 and 4: the lower and the upper bound (doubles).
    pub const TERRAIN_BOUNDS: [&str; 2] = ["HF_eClipMin", "HF_eClipMax"];

    /// The camera part's slots.
    pub const CAM_ANGLES: usize = 0;
    pub const CAM_ROLL: usize = 1;
    pub const CAM_FOV: usize = 2;
    pub const CAM_PAN_X: usize = 3;
    pub const CAM_PAN_Y: usize = 4;
    pub const CAM_ZOOM_X: usize = 5;
    pub const CAM_ZOOM_Y: usize = 6;
    /// The material part's slots beyond the thirteen channels (0 to 12).
    pub const MAT_FLAGS: usize = 13;
    pub const MAT_VOLUME_COLOUR: usize = 14;
    pub const MAT_REFRACTION: usize = 15;
    /// The three volume numbers at offsets 624, 628 and 632, in that order.
    pub const MAT_VOLUME_NUMBERS: usize = 16;
    /// The sun part's slots.
    pub const SUN_COLOUR: usize = 0;
    pub const SUN_GLOW: usize = 1;
    pub const SUN_DIRECTION: usize = 2;
    /// The sky's two tracks that opening applies, found by class among the sky part's track
    /// identities.,
    /// The nine applied (revised: the horizon colour, dome colour, plain sky
    /// colour, haze, haze colour and shadow intensity observed; the sky colour by the fallback).
    pub const SKY: [&str; 9] = [
        "Sky_eAmbientC",
        "Sky_eAmbientInt",
        "Sky_eHorizontC",
        "Sky_eDomeC",
        "Sky_eDefaultC",
        "Sky_eHazeAmount",
        "Sky_eHazeC",
        "Sky_eShadowInt",
        "Sky_eBaseC",
    ];
    pub const SKY_AMBIENT_COLOUR: usize = 0;
    pub const SKY_AMBIENCE: usize = 1;
    pub const SKY_HORIZON_COLOUR: usize = 2;
    pub const SKY_DOME_COLOUR: usize = 3;
    pub const SKY_PLAIN_COLOUR: usize = 4;
    pub const SKY_HAZE: usize = 5;
    pub const SKY_HAZE_COLOUR: usize = 6;
    pub const SKY_SHADOW_INTENSITY: usize = 7;
    pub const SKY_COLOUR: usize = 8;
    /// The wind's three tracks, slots 40 to 42 of the sky part's identities.
    pub const SKY_WIND: [&str; 3] = ["Sky_eWindDir", "Sky_eWindSpeed", "Sky_eWindTurb"];
    pub const SKY_WIND_FIRST_SLOT: usize = 40;
    /// The two classes whose track the wind replaces (the cloud offset): not read.
    pub const SKY_CLOUD_OFFSET: [&str; 2] = ["Sky_eCloudX", "Sky_eCloudZ"];
    /// The torus's slots.
    pub const TORUS_TUBE: usize = 0;
    pub const TORUS_RING: usize = 1;
}

/// A record track's keys read in each of the two forms.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRecords {
    /// The number of keys.
    pub keys: usize,
    /// Read in version 1's form: as 3.0 and 3.1.
    pub whole: Result<Track<TextureSettings>, String>,
    /// Read in version 2's form: as 5.0.
    pub single: Result<Track<TextureSettings>, String>,
    /// Each key's result as saved (record offsets 900 to 935), which a stand of the
    /// track gives the texture with the key's settings; the same in both forms.
    pub saved: Vec<crate::scene_textures::SavedResult>,
}

impl KeyRecords {
    /// The track as `version` reads its keys.
    ///
    /// The words are whole numbers as 3.0 and 3.1, and as 5.0 where the texture's own record is
    /// of version 1; as 5.0 where it is of version 2, binary32 numbers as they stand: whatever
    /// each key's own version word says (no program reads it).
    pub fn as_read_by(
        &self,
        version: crate::version::RenderAs,
        texture: RecordForm,
    ) -> Result<&Track<TextureSettings>, &str> {
        let way = match (version, texture) {
            (crate::version::RenderAs::Bryce50, RecordForm::Single) => &self.single,
            _ => &self.whole,
        };
        way.as_ref().map_err(String::as_str)
    }
}

/// A track's keys, by the kind of its class.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackValues {
    Double(Track<f64>),
    Single(Track<[f32; 1]>),
    Colour(Track<Rgb>),
    Triple(Track<[f64; 3]>),
    Place(Track<crate::animation::PlaceKey>),
    Orientation(Track<OrientKey>),
    ValueChannel(Track<Channel<f32>>),
    ColourChannel(Track<Channel<Rgb>>),
    /// A texture mapping's mode and switch words.
    Words(Track<MappingWords>),
    /// A texture's record, its keys read both ways's Variants give: in version 1's
    /// form (3.0, 3.1) and in version 2's (5.0), whatever each key's version word. A way that
    /// cannot read a key keeps the refusal, raised only by the version that reads it that way.
    Record(KeyRecords),
    /// A class whose keys this reader does not lay out: their number only (the
    /// implementer's choice).
    Unread { keys: usize, repeat: u32 },
}

impl TrackValues {
    /// The number of keys.
    pub fn key_count(&self) -> usize {
        match self {
            TrackValues::Double(t) => t.keys.len(),
            TrackValues::Single(t) => t.keys.len(),
            TrackValues::Colour(t) => t.keys.len(),
            TrackValues::Triple(t) => t.keys.len(),
            TrackValues::Place(t) => t.keys.len(),
            TrackValues::Orientation(t) => t.keys.len(),
            TrackValues::ValueChannel(t) => t.keys.len(),
            TrackValues::ColourChannel(t) => t.keys.len(),
            TrackValues::Words(t) => t.keys.len(),
            TrackValues::Record(t) => t.keys,
            TrackValues::Unread { keys, .. } => *keys,
        }
    }
}

/// One track resource, read.
#[derive(Clone, Debug, PartialEq)]
pub struct DocTrack {
    /// Its class tag.
    pub tag: String,
    pub identity: u32,
    pub values: TrackValues,
    /// The time map's points , `None` for none.
    pub map: Option<Vec<TimeMapPoint>>,
}

/// Why a track resource is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackRefusal {
    /// The header's count and the time map's size do not make the data's length.
    Length { len: usize, keys: u32, key_size: usize, map_size: u32 },
    /// The header's first word is not 1 (read in the byte order its mark gives).
    FirstWord(u32),
    /// The time map's first word is not 1 (read in the byte order of the map's own mark).
    MapFirstWord(u32),
    /// The time map's points do not fit its size (no document holds one).
    MapPoints { points: u32, map_size: u32 },
    /// A repeat word other than 0 to 3 (no document holds one).
    Repeat(u32),
    /// A key of a texture's record track whose record part is refused (version 2: Open).
    KeyRecord(String),
}

impl fmt::Display for TrackRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackRefusal::Length { len, keys, key_size, map_size } => write!(
                f,
                "damaged document: a track of {len} bytes with {keys} keys of {key_size} bytes and a time map of {map_size} bytes"
            ),
            TrackRefusal::FirstWord(w) => write!(f, "damaged document: a track's first word is {w}, not 1"),
            TrackRefusal::MapFirstWord(w) => write!(f, "damaged document: a track's time map begins with {w}, not 1"),
            TrackRefusal::MapPoints { points, map_size } => {
                write!(f, "damaged document: a time map of {points} points in {map_size} bytes")
            }
            TrackRefusal::Repeat(r) => write!(f, "damaged document: a track's repeat setting is {r}, not 0 to 3"),
            TrackRefusal::KeyRecord(why) => write!(f, "{why} (a key of a texture's own track)"),
        }
    }
}

impl std::error::Error for TrackRefusal {}

impl DocTrack {
    /// Reads a track resource's data (it has no first part).
    pub fn read(tag: &str, identity: u32, b: &[u8]) -> Result<DocTrack, TrackRefusal> {
        let short = |keys, key_size, map_size| TrackRefusal::Length { len: b.len(), keys, key_size, map_size };
        if b.len() < HEADER_LEN {
            return Err(short(0, 0, 0));
        }
        let o = if b[4..8] == [1, 0, 0, 0] { ByteOrder::Little } else { ByteOrder::Big };
        // (Errors, -10-22: the readers of 3.0, 3.1 and 5.0 test the first
        // word, read in the byte order the mark gives, and make no other test of the head)
        let first = word(b, 0, o);
        if first != 1 {
            return Err(TrackRefusal::FirstWord(first));
        }
        let keys = word(b, 8, o);
        let map_size = word(b, 16, o);
        let repeat_word = word(b, 20, o);
        let kind = kind_of(tag);
        let fits = |size: usize| {
            (keys as usize).checked_mul(size).and_then(|k| k.checked_add(HEADER_LEN + map_size as usize)) == Some(b.len())
        };
        let key_size = match kind {
            Some(TrackKind::Orientation) if !fits(TrackKind::Orientation.key_size()) && fits(ORIENT_KEY_SHORT) => {
                ORIENT_KEY_SHORT
            }
            Some(k) => k.key_size(),
            None => match SIZES_ONLY.iter().find(|(t, _)| *t == tag) {
                Some(&(_, size)) => size,
                // A class no entry sizes: only the header and the map are checked (design).
                None => {
                    if b.len() < HEADER_LEN + map_size as usize {
                        return Err(short(keys, 0, map_size));
                    }
                    return Ok(DocTrack {
                        tag: tag.to_string(),
                        identity,
                        values: TrackValues::Unread { keys: keys as usize, repeat: repeat_word },
                        map: None,
                    });
                }
            },
        };
        if !fits(key_size) {
            return Err(short(keys, key_size, map_size));
        }
        let keys_end = HEADER_LEN + keys as usize * key_size;
        let map = if map_size == 0 {
            None
        } else {
            // : the time map has an order mark of its own,
            // at its offset 4, read as the header's mark is (01 00 00 00: little-endian, anything
            // else big-endian), whatever the record's mark; a map in a little-endian record is
            // read by its own mark.
            let map = &b[keys_end..];
            let map_order = if map.len() >= 8 && map[4..8] == [1, 0, 0, 0] { ByteOrder::Little } else { ByteOrder::Big };
            Some(read_map(map, map_order, map_size)?)
        };
        let Some(kind) = kind else {
            return Ok(DocTrack {
                tag: tag.to_string(),
                identity,
                values: TrackValues::Unread { keys: keys as usize, repeat: repeat_word },
                map,
            });
        };
        let repeat = Repeat::from_setting(repeat_word).ok_or(TrackRefusal::Repeat(repeat_word))?;
        let at = |k: usize| HEADER_LEN + k * key_size;
        let n = keys as usize;
        fn track<V>(n: usize, repeat: Repeat, key: impl Fn(usize) -> Key<V>) -> Track<V> {
            Track::new((0..n).map(key).collect(), repeat)
        }
        let time = |k: usize| {
            let t = double(b, at(k), o);
            if t < 0.0 { 0.0 } else { t }
        };
        let v = |k: usize| at(k) + VALUE_AT;
        let values = match kind {
            TrackKind::Double => TrackValues::Double(track(n, repeat, |k| Key { time: time(k), value: double(b, v(k), o) })),
            TrackKind::Single => {
                TrackValues::Single(track(n, repeat, |k| Key { time: time(k), value: [single(b, v(k), o)] }))
            }
            TrackKind::Colour => TrackValues::Colour(track(n, repeat, |k| Key { time: time(k), value: rgb(b, v(k), o) })),
            TrackKind::Triple => TrackValues::Triple(track(n, repeat, |k| Key { time: time(k), value: triple(b, v(k), o) })),
            TrackKind::Place => TrackValues::Place(track(n, repeat, |k| {
                // (Interface: the binary32 words at bytes 68, 72 and
                // 76 of the 92-byte key are T, C, B in that order; 80, 84, 88 are not read)
                let tcb = [single(b, at(k) + 68, o), single(b, at(k) + 72, o), single(b, at(k) + 76, o)];
                Key { time: time(k), value: PlaceKey::from_passing(triple(b, v(k), o), word(b, at(k) + 8, o), tcb) }
            })),
            TrackKind::Orientation => TrackValues::Orientation(track(n, repeat, |k| {
                let q = [double(b, v(k), o), double(b, v(k) + 8, o), double(b, v(k) + 16, o), double(b, v(k) + 24, o)];
                // ; a 76-byte key
                // has none: 0 (the tracks)
                let half_turns = if key_size == ORIENT_KEY_SHORT { 0.0 } else { single(b, v(k) + 32, o) };
                Key { time: time(k), value: OrientKey { orientation: q, half_turns } }
            })),
            TrackKind::ValueChannel => {
                TrackValues::ValueChannel(track(n, repeat, |k| Key { time: time(k), value: value_channel(b, v(k), o) }))
            }
            TrackKind::ColourChannel => {
                TrackValues::ColourChannel(track(n, repeat, |k| Key { time: time(k), value: colour_channel(b, v(k), o) }))
            }
            TrackKind::Words => TrackValues::Words(track(n, repeat, |k| Key {
                time: time(k),
                value: MappingWords { mode: word(b, v(k), o), switches: word(b, v(k) + 4, o) },
            })),
            // (Variants: each key read in version 1's form as 3.0
            // and 3.1 and in version 2's as 5.0, whatever its version word)
            TrackKind::Record => {
                let read = |form: RecordForm| -> Result<Track<TextureSettings>, String> {
                    let mut keys = Vec::with_capacity(n);
                    for k in 0..n {
                        let record = &b[v(k)..v(k) + RECORD_PART_LEN];
                        // (Variants: a key of version 1's form, whole
                        // numbers, is read alike by the three programs; version 2's form as 5.0
                        // reads it stays Open)
                        let form = if word(record, 0, o) <= 1 { RecordForm::Whole } else { form };
                        let p = ProceduralTexture::read_key_as(record, o, form).map_err(|e| e.to_string())?;
                        keys.push(Key { time: time(k), value: p.settings });
                    }
                    Ok(Track::new(keys, repeat))
                };
                let (whole, single) = (read(RecordForm::Whole), read(RecordForm::Single));
                if let (Err(why), Err(_)) = (&whole, &single) {
                    return Err(TrackRefusal::KeyRecord(why.clone()));
                }
                let saved = (0..n)
                    .map(|k| crate::scene_textures::SavedResult::read(&b[v(k)..v(k) + RECORD_PART_LEN], o))
                    .collect();
                TrackValues::Record(KeyRecords { keys: n, whole, single, saved })
            }
        };
        Ok(DocTrack { tag: tag.to_string(), identity, values, map })
    }
}

fn triple(b: &[u8], at: usize, o: ByteOrder) -> [f64; 3] {
    [double(b, at, o), double(b, at + 8, o), double(b, at + 16, o)]
}

/// The time map after the last key.,
fn read_map(b: &[u8], o: ByteOrder, map_size: u32) -> Result<Vec<TimeMapPoint>, TrackRefusal> {
    let size = map_size as usize;
    if size < MAP_HEAD {
        return Err(TrackRefusal::MapPoints { points: 0, map_size });
    }
    let first = word(b, 0, o);
    if first != 1 {
        return Err(TrackRefusal::MapFirstWord(first));
    }
    let points = word(b, 8, o);
    let needed = (points as usize).checked_mul(MAP_POINT).and_then(|p| p.checked_add(MAP_HEAD));
    if needed.is_none_or(|n| n > size) {
        return Err(TrackRefusal::MapPoints { points, map_size });
    }
    Ok((0..points as usize)
        .map(|k| {
            let at = MAP_HEAD + MAP_POINT * k;
            TimeMapPoint { part: single(b, at, o), value: single(b, at + 4, o), flags: word(b, at + 8, o) }
        })
        .collect())
}

/// The tracks every owner's slots name, bound by index into `tracks`; `None` for a slot whose
/// identity is 0 (or names nothing, where allows it). A scene the reader did not make
/// (a description) holds no tracks: every list empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneTracks {
    /// Every track resource bound, once each, in the order first named.
    pub tracks: Vec<DocTrack>,
    /// Per `Scene::objects`: the placement's seven slots, and a torus's two.
    pub objects: Vec<ObjectTrackSlots>,
    /// Per `Scene::lights` (the sun last): the light part's eight slots.
    pub lights: Vec<[Option<usize>; 8]>,
    /// Per `Scene::lights`: a spot light's falloff track , `None` for none and for
    /// every other light.
    pub beam_falloff: Vec<Option<usize>>,
    /// The sun part's three slots.
    pub sun: [Option<usize>; 3],
    /// The sky's tracks that opening applies (`slots::SKY`).
    pub sky: [Option<usize>; 9],
    /// The sky part's track identities other than 0 that name none of those: the wind and
    /// drift tracks among them (Open).,
    pub sky_other_tracks: usize,
    /// The sky's wind tracks: direction, speed and turbulence (slots 40 to 42 of the sky part's
    /// identities).,
    pub sky_wind: [Option<usize>; 3],
    /// Per `Scene::cameras`: the camera part's seven slots.
    pub cameras: Vec<[Option<usize>; 7]>,
    /// Per `Scene::materials`: the material part's nineteen slots.
    pub materials: Vec<[Option<usize>; 19]>,
    /// Per `Scene::textures`: the mapping's four tracks (mode and switches, scale, offset,
    /// turn) and a procedural texture's record track.
    pub textures: Vec<[Option<usize>; 5]>,
    /// Per `Scene::terrains`: the track words of the lower bound (HF_eClipMin) and the upper
    /// bound (HF_eClipMax).,
    pub terrains: Vec<[Option<usize>; 2]>,
}

/// An object's slots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectTrackSlots {
    pub placement: [Option<usize>; 7],
    pub torus: [Option<usize>; 2],
}

impl SceneTracks {
    /// The track a slot names, if it has keys.
    pub fn keyed(&self, slot: Option<usize>) -> Option<&DocTrack> {
        slot.map(|i| &self.tracks[i]).filter(|t| t.values.key_count() > 0)
    }

    /// Whether any bound track has a key.
    pub fn any_keys(&self) -> bool {
        self.tracks.iter().any(|t| t.values.key_count() > 0)
    }

    /// An object's slots (none for an object beyond the list, as in a description).
    pub fn object(&self, i: usize) -> ObjectTrackSlots {
        self.objects.get(i).copied().unwrap_or_default()
    }

    /// A light's slots.
    pub fn light(&self, i: usize) -> [Option<usize>; 8] {
        self.lights.get(i).copied().unwrap_or_default()
    }

    /// A camera's slots.
    pub fn camera(&self, i: usize) -> [Option<usize>; 7] {
        self.cameras.get(i).copied().unwrap_or_default()
    }

    /// A texture's five slots (none for a scene without them, as in a description).
    pub fn texture(&self, i: usize) -> [Option<usize>; 5] {
        self.textures.get(i).copied().unwrap_or_default()
    }

    /// A material's slots.
    pub fn material(&self, i: usize) -> [Option<usize>; 19] {
        self.materials.get(i).copied().unwrap_or([None; 19])
    }
}
