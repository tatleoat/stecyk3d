// SPDX-License-Identifier: MIT
//! Decoders of the parts of a resource's data that `scene_records` does not cover: the
//! first part, the material part, the light and sun parts, the camera part, the sky part.
//! Each takes the part's bytes and the resource's byte order.
//!
//! (the sky part is in `scene_sky`, with
//! ; its names are re-exported here),,
//! (a light's
//! gradient resource),
//! (The scene reader)

use crate::geom::Vec3;
use crate::scene_records::{ByteOrder, FourCc, RecordError, check_len, double, single, word};

/// Three singles: red, green, blue.
pub type Rgb = [f32; 3];

pub(crate) fn rgb(b: &[u8], at: usize, o: ByteOrder) -> Rgb {
    [single(b, at, o), single(b, at + 4, o), single(b, at + 8, o)]
}

pub(crate) fn vec3(b: &[u8], at: usize, o: ByteOrder) -> Vec3 {
    [double(b, at, o), double(b, at + 8, o), double(b, at + 16, o)]
}

pub(crate) fn doubles<const N: usize>(b: &[u8], at: usize, o: ByteOrder) -> [f64; N] {
    std::array::from_fn(|k| double(b, at + 8 * k, o))
}

pub(crate) fn words<const N: usize>(b: &[u8], at: usize, o: ByteOrder) -> [u32; N] {
    std::array::from_fn(|k| word(b, at + 4 * k, o))
}

fn expect_version(record: &'static str, b: &[u8], o: ByteOrder, known: &[u32]) -> Result<u32, RecordError> {
    if b.len() < 4 {
        return Err(RecordError::WrongLength { record, len: b.len(), expected: 4 });
    }
    let version = word(b, 0, o);
    if known.contains(&version) { Ok(version) } else { Err(RecordError::UnknownVersion { record, version }) }
}

/// A reference by type code and identity; `None` where the code is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRef {
    pub code: FourCc,
    pub identity: u32,
}

pub(crate) fn reference(b: &[u8], code_at: usize, id_at: usize, o: ByteOrder) -> Option<TextureRef> {
    let code = FourCc::from_word(&b[code_at..code_at + 4], o);
    if code.is_zero() { None } else { Some(TextureRef { code, identity: word(b, id_at, o) }) }
}

// ---------------------------------------------------------------------------------------
// The first part and the byte order
// ---------------------------------------------------------------------------------------

/// Length of the first part.
pub const FIRST_PART_LEN: usize = 264;
/// Length of the name field.
const NAME_FIELD_LEN: usize = 256;

/// The byte order a resource's data states in its first four bytes.
/// (01 00 00 00: little-endian; anything else: big-endian)
pub fn order_of(data: &[u8]) -> ByteOrder {
    if data.len() >= 4 && data[..4] == [1, 0, 0, 0] { ByteOrder::Little } else { ByteOrder::Big }
}

/// The name in the first part: the bytes before the first zero (all 256 when there is
/// none: not refused, the implementer's choice of point 7).;
pub fn first_part_name(data: &[u8]) -> Result<Vec<u8>, RecordError> {
    if data.len() < FIRST_PART_LEN {
        return Err(RecordError::WrongLength { record: "first part", len: data.len(), expected: FIRST_PART_LEN });
    }
    Ok(data[8..8 + NAME_FIELD_LEN].iter().take_while(|&&c| c != 0).copied().collect())
}

// ---------------------------------------------------------------------------------------
// A material
// ---------------------------------------------------------------------------------------

/// Length of the material part.
pub const MATERIAL_PART_LEN: usize = 712;
/// The flag bit whose absence marks an older material.
pub const MATERIAL_CURRENT: u32 = 0x1_0000;
/// The bits that make a material a volume, each saying of which kind. (bits
/// 0x100, 0x200 and 0x400 [format])
pub const MATERIAL_VOLUME_BITS: u32 = 0x700;
/// The fuzzy material's bit.,
pub const MATERIAL_FUZZY: u32 = 0x2;
/// Value channels whose B and alpha an older material takes as 1.
const OLDER_VALUE_CHANNELS: usize = 7;

/// The colour channels, in order.
pub const COLOUR_CHANNELS: [&str; 5] = ["Diffuse", "Ambient", "Specular", "Specular Halo", "Transparent colour"];
/// The value channels, in order.
pub const VALUE_CHANNELS: [&str; 8] =
    ["Diffusion", "Ambience", "Specularity", "Transparency", "Reflection", "Bump Height", "Metallicity", "Volume density"];

/// One channel: three texture references (A, B, alpha), the channel's flag word, A, B and
/// alpha. `T` is `Rgb` for a colour channel, `f32` for a value channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Channel<T> {
    pub refs: [Option<TextureRef>; 3],
    pub flags: u32,
    pub a: T,
    pub b: T,
    pub alpha: f32,
}

impl<T> Channel<T> {
    pub fn has_texture_refs(&self) -> bool {
        self.refs.iter().any(Option::is_some)
    }
}

fn channel_refs(b: &[u8], at: usize, o: ByteOrder) -> [Option<TextureRef>; 3] {
    std::array::from_fn(|k| reference(b, at + 8 * k, at + 8 * k + 4, o))
}

/// Bytes of a colour channel and of a value channel.
pub const COLOUR_CHANNEL_LEN: usize = 56;
pub const VALUE_CHANNEL_LEN: usize = 40;

/// A colour channel at `at`: three references, the flag word, A, B, alpha.
pub(crate) fn colour_channel(b: &[u8], at: usize, o: ByteOrder) -> Channel<Rgb> {
    Channel {
        refs: channel_refs(b, at, o),
        flags: word(b, at + 24, o),
        a: rgb(b, at + 28, o),
        b: rgb(b, at + 40, o),
        alpha: single(b, at + 52, o),
    }
}

/// A value channel at `at`: three references, the flag word, A, B, alpha.
pub(crate) fn value_channel(b: &[u8], at: usize, o: ByteOrder) -> Channel<f32> {
    Channel {
        refs: channel_refs(b, at, o),
        flags: word(b, at + 24, o),
        a: single(b, at + 28, o),
        b: single(b, at + 32, o),
        alpha: single(b, at + 36, o),
    }
}

/// A material part (the name comes from the first part).
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialPart {
    /// The flag word as the reader leaves it (the older-material rule applied).
    pub flags: u32,
    /// Diffuse, Ambient, Specular, Specular Halo, Transparent colour.
    pub colours: [Channel<Rgb>; 5],
    /// Diffusion, Ambience, Specularity, Transparency, Reflection, Bump Height, Metallicity,
    /// Volume density.
    pub values: [Channel<f32>; 8],
    /// The Volume Color: the colour of the object's inside (offset 608).
    pub volume_colour: Rgb,
    /// The refraction value (offset 620).
    pub refraction: f32,
    /// Edge softness, fuzzy factor and quality (offsets 624, 628, 632), as the file holds
    /// them.
    pub volume_numbers: [f32; 3],
    pub tracks: [u32; 19],
}

impl MaterialPart {
    /// (the first word is not a version a reader
    /// checks; the part is read at its one size whatever it holds)
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<MaterialPart, RecordError> {
        check_len("material", b, MATERIAL_PART_LEN)?;
        let colours = std::array::from_fn(|k| colour_channel(b, 8 + COLOUR_CHANNEL_LEN * k, o));
        let mut values: [Channel<f32>; 8] = std::array::from_fn(|k| value_channel(b, 288 + VALUE_CHANNEL_LEN * k, o));
        let mut flags = word(b, 4, o);
        if flags & MATERIAL_CURRENT == 0 {
            flags |= MATERIAL_CURRENT;
            for ch in values.iter_mut().take(OLDER_VALUE_CHANNELS) {
                ch.b = 1.0;
                ch.alpha = 1.0;
            }
        }
        Ok(MaterialPart {
            flags,
            colours,
            values,
            volume_colour: rgb(b, 608, o),
            refraction: single(b, 620, o),
            volume_numbers: [single(b, 624, o), single(b, 628, o), single(b, 632, o)],
            tracks: words(b, 636, o),
        })
    }

    /// The Edge Softness of the Materials Lab (offset 624).
    pub fn edge_softness(&self) -> f32 {
        self.volume_numbers[0]
    }

    /// The Fuzzy Factor of the Materials Lab (offset 628).
    pub fn fuzzy_factor(&self) -> f32 {
        self.volume_numbers[1]
    }

    /// The Quality/Speed of the Materials Lab (offset 632).
    pub fn quality(&self) -> f32 {
        self.volume_numbers[2]
    }

    /// Whether no channel has a texture reference.
    pub fn is_plain(&self) -> bool {
        !self.colours.iter().any(Channel::has_texture_refs) && !self.values.iter().any(Channel::has_texture_refs)
    }

    /// Each channel's switch word and whether it has a texture reference, in's channel
    /// order (the colours, then the values).
    pub fn channel_words(&self) -> Vec<(u32, bool)> {
        let colours = self.colours.iter().map(|c| (c.flags, c.has_texture_refs()));
        colours.chain(self.values.iter().map(|c| (c.flags, c.has_texture_refs()))).collect()
    }
}

// ---------------------------------------------------------------------------------------
// The light part and the sun part
// ---------------------------------------------------------------------------------------

/// Length of Bryce 3D's light part.
pub const LIGHT_PART_LEN: usize = 192;
/// Length of the sun part.
pub const SUN_PART_LEN: usize = 64;

/// A light's switch bits (its placement's second flag word).
pub mod switch {
    /// The light is on.
    pub const ON: u32 = 0x100;
    /// Rays meet the light's own object.
    pub const MEETS_OWN_OBJECT: u32 = 0x200;
    /// Its strength does not fall off with distance.
    pub const NO_FALL_OFF: u32 = 0x400;
    /// It casts no shadows.
    pub const NO_SHADOWS: u32 = 0x800;
    /// Without NO_FALL_OFF: falls off with the square of the distance (else with the
    /// distance).
    pub const SQUARE_FALL_OFF: u32 = 0x8000_0000;
}

/// Length of Bryce 5's light part, versions 2 and 3.
pub const LIGHT_PART_V3_LEN: usize = 204;
/// The light part's version in Bryce 5's documents.
pub const LIGHT_VERSION_FIVE: u32 = 3;
/// The version no document holds, read by the layout gives it.
pub const LIGHT_VERSION_TWO: u32 = 2;
/// The type code of a light's gradient reference.
const GRADIENT_CODE: FourCc = FourCc::new(b"base");

/// The light part's track slots (version 3; version 1's four are slots 0 to 3).
/// , revised (version 1's four)
pub mod light_slot {
    /// `Light_C`, the light's own colour.
    pub const COLOUR: usize = 0;
    /// `Light_Int`, the intensity.
    pub const INTENSITY: usize = 1;
    /// `Light_Rad`, the radius.
    pub const RADIUS: usize = 2;
    /// `Light_ShAmb`, the shadow ambience.
    pub const SHADOW_AMBIENCE: usize = 3;
    /// `Light_Rng`, the range.
    pub const RANGE: usize = 4;
    /// `Light_GrRng`, the gradient's range.
    pub const GRADIENT_RANGE: usize = 5;
    /// `Light_GrDlt`, the gradient's step.
    pub const GRADIENT_STEP: usize = 6;
    /// `Light_ShAmbCol`, the shadow colour.
    pub const SHADOW_COLOUR: usize = 7;
}

/// What Bryce 5 reads of a light beyond Bryce 3D's fields: from a version 3 part as
/// stored, from a version 1 part as says a render as 5.0 takes it (radius 0, the
/// shadow ambience the double at 32, the rest 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightFive {
    /// The radius ρ a render as 5.0 takes, or `None` where leaves it Open: the double
    /// at 24 where the light object's third flag word has bit 0x2000, else 0
    /// (`gated_radius`, applied by `with_third_flags`; the part alone is read as with the word
    /// 0). An applied `Light_Rad` track gives it, through the same gate.
    pub radius: Option<f64>,
    pub shadow_ambience: f64,
    pub range: f64,
    /// The gradient's range and step.
    pub gradient: [f64; 2],
    pub shadow_colour: Rgb,
    /// The C3dBaseGradient resource named at offset 120 (its identity), `None` for 0, 0.
    pub gradient_resource: Option<u32>,
}

/// A light part: Bryce 3D's version 1 (192 bytes) or Bryce 5's version 3 (204 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightPart {
    /// 1 or 3.
    pub version: u32,
    pub colour: Rgb,
    pub intensity: f64,
    /// The two doubles at offset 24, as stored (in version 3 the radius and the shadow
    /// ambience; in version 1 not needed by a render as 3.0 or 3.1).
    pub open_24: [f64; 2],
    /// The eight track identities (version 3, offset 128); for version 1 its four (offset 48)
    /// in slots 0 to 3 and 0 in the rest. Slots: `light_slot`. revised
    pub tracks: [u32; 8],
    /// Bryce 5's numbers, as 5.0 reads them.
    pub five: LightFive,
}

impl LightPart {
    /// The light part's length by version, `None` for a version no reader knows; version 2's
    /// layout is Open, and it is refused by `parse`.
    pub fn len_of_version(version: u32) -> Option<usize> {
        match version {
            1 => Some(LIGHT_PART_LEN),
            LIGHT_VERSION_TWO | LIGHT_VERSION_FIVE => Some(LIGHT_PART_V3_LEN),
            _ => None,
        }
    }

    /// (version 1 or 3; another version refuses the document; version 2,
    /// whose layout is Open, is refused as allows; a gradient type code neither 0
    /// nor `base` is refused). `b` is the part's bytes, of its version's length.
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<LightPart, RecordError> {
        let version = expect_version("light", b, o, &[1, LIGHT_VERSION_TWO, LIGHT_VERSION_FIVE])?;
        let open_24 = doubles(b, 24, o);
        if version == 1 {
            check_len("light", b, LIGHT_PART_LEN)?;
            let four: [u32; 4] = words(b, 48, o);
            return Ok(LightPart {
                version,
                colour: rgb(b, 4, o),
                intensity: double(b, 16, o),
                open_24,
                tracks: std::array::from_fn(|k| if k < 4 { four[k] } else { 0 }),
                // (a light of version 1 read as 5.0: radius 0, shadow ambience the
                // double at 32, the other fields 0)
                five: LightFive {
                    radius: Some(0.0),
                    shadow_ambience: open_24[1],
                    range: 0.0,
                    gradient: [0.0; 2],
                    shadow_colour: [0.0; 3],
                    gradient_resource: None,
                },
            });
        }
        check_len("light", b, LIGHT_PART_V3_LEN)?;
        if version == LIGHT_VERSION_TWO {
            // (version 2: the words to offset 111 as version 3's except that the
            // gradient numbers and the shadow colour are absent, taken as 0; a reference at
            // 112 not used; the identities of the first five tracks at 120 to 139)
            let five: [u32; 5] = words(b, 120, o);
            return Ok(LightPart {
                version,
                colour: rgb(b, 4, o),
                intensity: double(b, 16, o),
                open_24,
                tracks: std::array::from_fn(|k| if k < 5 { five[k] } else { 0 }),
                five: LightFive {
                    radius: gated_radius(0, open_24[0], false),
                    shadow_ambience: open_24[1],
                    range: double(b, 40, o),
                    gradient: [0.0; 2],
                    shadow_colour: [0.0; 3],
                    gradient_resource: None,
                },
            });
        }
        let code =FourCc::from_word(&b[120..124], o);
        let gradient_resource = if code.is_zero() {
            None
        } else if code == GRADIENT_CODE {
            Some(word(b, 124, o))
        } else {
            return Err(RecordError::GradientCode(code));
        };
        Ok(LightPart {
            version,
            colour: rgb(b, 4, o),
            intensity: double(b, 16, o),
            open_24,
            tracks: words(b, 128, o),
            five: LightFive {
                radius: gated_radius(0, open_24[0], false),
                shadow_ambience: open_24[1],
                range: double(b, 40, o),
                gradient: doubles(b, 48, o),
                shadow_colour: rgb(b, 64, o),
                gradient_resource,
            },
        })
    }

    /// The part with its placement's third flag word taken into account: the radius a render
    /// as 5.0 takes is the stored double at 24 where the word has bit 0x2000, else 0
    /// (`gated_radius`)., (the third
    /// flag word),
    pub fn with_third_flags(mut self, third_flags: u32) -> LightPart {
        self.five.radius = gated_radius(third_flags, self.open_24[0], false);
        self
    }
}

/// The bit of an object's third flag word that lets a light's radius count.
/// (Inputs),
pub const RADIUS_COUNTS: u32 = 0x2000; // [format]
/// The radius ρ a render as 5.0 takes from a light's number `r` (the stored double, or, with
/// `from_track`, the value its `Light_Rad` track gives where the tracks are applied) and its
/// object's third flag word; `None` where leaves it Open:
/// - with bit 0x2000 the radius counts whatever the word's other bits and whatever the number
/// (below 0 or not a number included: case 3; observed soft-17, soft-18, soft-21 to
///   soft-25);
/// - without the bit the stored radius is 0, but a radius a track gives counts whatever the
///   word holds: the gate acts on the stored radius as the light part is read, and the track's
///   value is bound afterwards with no test (observed: light-range-track-pictures tg-track2-*).
///
pub fn gated_radius(third_flags: u32, r: f64, from_track: bool) -> Option<f64> {
    if third_flags & RADIUS_COUNTS != 0 || from_track {
        return Some(r);
    }
    Some(0.0)
}

/// Length of a gradient's record before its keys.
pub const GRADIENT_HEAD_LEN: usize = 52;
/// Length of a colour key.
pub const GRADIENT_COLOUR_KEY_LEN: usize = 28;
/// Length of a value key.
pub const GRADIENT_VALUE_KEY_LEN: usize = 20;
/// A colour key of a light's gradient: its colour, place and midpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientColourKey {
    pub colour: Rgb,
    pub place: f32,
    pub midpoint: f32,
}

/// A value key (the manual's transparency node) of a light's gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientValueKey {
    pub value: f32,
    pub place: f32,
    pub midpoint: f32,
}

/// A light's gradient: the record of a C3dBaseGradient resource after its first part.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientPart {
    pub colour_switches: u32,
    pub value_switches: u32,
    /// In the order stored.
    pub colour_keys: Vec<GradientColourKey>,
    /// In the order stored.
    pub value_keys: Vec<GradientValueKey>,
}

impl GradientPart {
    /// . `b` is the data after the first part. Refused: data shorter than the
    /// counts need (Errors); a version word other than 1 (the implementer's choice,
    /// ). Data longer than the record needs: ignored (the implementer's
    /// choice, as's other parts).
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<GradientPart, RecordError> {
        // : the version word is read and never
        // compared (as 5.0 reads it): any value is read as version 1
        if b.len() < GRADIENT_HEAD_LEN {
            return Err(RecordError::WrongLength { record: "gradient", len: b.len(), expected: GRADIENT_HEAD_LEN });
        }
        let (nc, nv) = (word(b, 12, o) as usize, word(b, 16, o) as usize);
        let needed = nc
            .checked_mul(GRADIENT_COLOUR_KEY_LEN)
            .and_then(|c| nv.checked_mul(GRADIENT_VALUE_KEY_LEN).and_then(|v| c.checked_add(v)))
            .and_then(|k| k.checked_add(GRADIENT_HEAD_LEN))
            .unwrap_or(usize::MAX);
        if b.len() < needed {
            return Err(RecordError::WrongLength { record: "gradient", len: b.len(), expected: needed });
        }
        let colour_keys = (0..nc)
            .map(|k| {
                let at = GRADIENT_HEAD_LEN + GRADIENT_COLOUR_KEY_LEN * k;
                GradientColourKey { colour: rgb(b, at, o), place: single(b, at + 12, o), midpoint: single(b, at + 16, o) }
            })
            .collect();
        let values_at = GRADIENT_HEAD_LEN + GRADIENT_COLOUR_KEY_LEN * nc;
        let value_keys = (0..nv)
            .map(|k| {
                let at = values_at + GRADIENT_VALUE_KEY_LEN * k;
                GradientValueKey { value: single(b, at, o), place: single(b, at + 4, o), midpoint: single(b, at + 8, o) }
            })
            .collect();
        Ok(GradientPart { colour_switches: word(b, 4, o), value_switches: word(b, 8, o), colour_keys, value_keys })
    }
}

/// The sun part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunPart {
    pub colour: Rgb,
    pub glow_colour: Rgb,
    /// The direction toward the sun, as stored.
    pub direction: Vec3,
    pub tracks: [u32; 3],
}

impl SunPart {
    /// (the first word is not a version a reader
    /// checks; the part is read at its one size whatever it holds)
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<SunPart, RecordError> {
        check_len("sun", b, SUN_PART_LEN)?;
        Ok(SunPart {
            colour: rgb(b, 4, o),
            glow_colour: rgb(b, 16, o),
            direction: vec3(b, 28, o),
            tracks: words(b, 52, o),
        })
    }
}

// ---------------------------------------------------------------------------------------
// The camera part
// ---------------------------------------------------------------------------------------

/// Length of the camera part.
pub const CAMERA_PART_LEN: usize = 168;

/// The camera part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPart {
    /// Two angles and a third number (kept as read; nothing a render computes takes it).
    pub angles: [f64; 3],
    pub roll: f64,
    pub field_of_view: f64,
    pub scale: f64,
    pub aspect: f64,
    pub pan: [f64; 2],
    pub zoom: [f64; 2],
    pub tracks: [u32; 7],
    /// The word at offset 36 (in the part's unused bytes): 1 to 6 marks a saved view whose
    /// name begins with "dot" (with bit 0x80000 of its projection word).
    pub saved_view_word: u32,
}

impl CameraPart {
    pub fn parse(b: &[u8], o: ByteOrder) -> Result<CameraPart, RecordError> {
        check_len("camera", b, CAMERA_PART_LEN)?;
        let p: [f64; 7] = doubles(b, 84, o);
        Ok(CameraPart {
            angles: doubles(b, 4, o),
            roll: double(b, 76, o),
            field_of_view: p[0],
            scale: p[1],
            aspect: p[2],
            pan: [p[3], p[4]],
            zoom: [p[5], p[6]],
            tracks: words(b, 140, o),
            saved_view_word: word(b, 36, o),
        })
    }
}

// ---------------------------------------------------------------------------------------
// The sky part
// ---------------------------------------------------------------------------------------

/// The sky part, with Bryce 5's version 7 , is decoded in `scene_sky`.
pub use crate::scene_sky::{SKY_AXIS_VIEW, Sky, SkyFive, sky_part_needed};

