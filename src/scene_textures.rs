// SPDX-License-Identifier: MIT
//! The texture resources a document's references name: a procedural texture (class
//! C3dTexture) with its mapping part and record part, decoded into the settings of
//! `textures::settings`; a picture texture (class CPictTexture) with its mapping part and
//! its picture part (the identity of the picture,, which `scene` resolves). The first part and the resolving of references are `scene`'s.
//!
//! (the mapping part, the record part and its two forms, a noise word, a colour
//! word, the type codes brtx and pctx; the data of a CPictTexture; Errors)
//! (Textures)

use std::fmt;

use crate::geom::Vec3;
use crate::scene_parts::FIRST_PART_LEN;
use crate::scene_records::{ByteOrder, FourCc, double, single, word};
use crate::textures::settings::{
    ColourMap, ColourSpace, Component, Dimension, Filter, MAX_COMPONENTS, NoiseSettings, NoiseSpace, OutputSet, Phase,
    TextureSettings,
};

/// The class tags of the two kinds of texture resource. [format]
pub const PROCEDURAL_CLASS: &str = "C3dTexture";
pub const PICTURE_CLASS: &str = "CPictTexture";
/// The type codes of a texture reference. [format]
pub const PROCEDURAL_CODE: FourCc = FourCc::new(b"brtx");
pub const PICTURE_CODE: FourCc = FourCc::new(b"pctx");

/// Length of the mapping part.
pub const MAPPING_PART_LEN: usize = 236;
/// Length of the record part.
pub const RECORD_PART_LEN: usize = 1088;
/// A C3dTexture's data: the first part, the mapping part, the record part.
/// (1,588 bytes)
pub const PROCEDURAL_DATA_LEN: usize = FIRST_PART_LEN + MAPPING_PART_LEN + RECORD_PART_LEN;
/// The parts of a CPictTexture this reader reads (its own part after them is Open).
pub const PICTURE_READ_LEN: usize = FIRST_PART_LEN + MAPPING_PART_LEN;
/// Length of a CPictTexture's picture part.
pub const PICTURE_PART_LEN: usize = 136;
/// A CPictTexture's data: the first part, the mapping part, the picture part.
/// (636 bytes)
pub const PICTURE_DATA_LEN: usize = PICTURE_READ_LEN + PICTURE_PART_LEN;
/// The picture part's words.
const AT_PICTURE_VERSION: usize = 0;
const AT_PICTURE_IDENTITY: usize = 4;

/// The record part's offsets.
const AT_COUNT: usize = 4;
const AT_OUTPUTS: usize = 8;
const AT_BLEND: usize = 12;
const AT_PHASE: usize = 20;
const AT_FILTER: usize = 48;
const AT_COLOURS: usize = 64;
const AT_COMPONENTS: usize = 84;
const COMPONENT_LEN: usize = 272;
const AT_SAVED_RESULT: usize = 900;
const AT_TRACK: usize = 1084;
/// A component's offsets.
const AT_C_OUTPUTS: usize = 0;
const AT_C_NOISE: usize = 8;
const AT_C_ANGLES: usize = 12;
const AT_C_FREQUENCIES: usize = 20;
const AT_C_PHASE: usize = 32;
const AT_C_FILTER: usize = 60;
const AT_C_COLOURS: usize = 76;

/// The output bits.
const OUTPUT_VALUE: u32 = 0x1;
const OUTPUT_COLOUR: u32 = 0x2;
const OUTPUT_BUMP: u32 = 0x4;
/// The colour space words.
const SPACE_RGB: u32 = 0;
const SPACE_HLS: u32 = 0x40;

/// Which class a reference's type code names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureClass {
    /// brtx: C3dTexture.
    Procedural,
    /// pctx: CPictTexture.
    Picture,
}

impl TextureClass {
    /// `None` for a code other than brtx and pctx (0, none, is told apart by the caller).
    pub fn of_code(code: FourCc) -> Option<TextureClass> {
        match code {
            PROCEDURAL_CODE => Some(TextureClass::Procedural),
            PICTURE_CODE => Some(TextureClass::Picture),
            _ => None,
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            TextureClass::Procedural => PROCEDURAL_CLASS,
            TextureClass::Picture => PICTURE_CLASS,
        }
    }
}

/// Why a texture resource or a reference is refused. Every case is one leaves to
/// the implementer, refusal with a reason being acceptable (Errors); short data is's
/// and is `SceneError::ShortData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureRefusal {
    /// A type code other than 0, brtx and pctx.
    Code(FourCc),
    /// A picture texture whose picture identity names no C3dCImage.
    /// (Errors: this rule)
    NoSuchPicture { identity: u32 },
    /// No resource of the class the code names has the identity.
    NoSuchTexture { class: &'static str, identity: u32 },
    /// A record version other than 1 and 2.
    RecordVersion(u32),
    /// A count of components outside 1 to 3.
    Count(u32),
    /// A colour space word other than 0 and 0x40; `component` is `None` for the global map.
    ColourSpace { component: Option<usize>, word: u32 },
    /// An outputs word with bits other than 0x1, 0x2, 0x4 (gives no meaning to
    /// them); `component` is `None` for the texture's own outputs.
    /// (Textures: refused)
    OutputBits { component: Option<usize>, word: u32 },
}

impl fmt::Display for TextureRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TextureRefusal::*;
        let place = |c: &Option<usize>| match c {
            Some(k) => format!("component {}", k + 1),
            None => "the texture".to_string(),
        };
        match self {
            Code(c) => write!(f, "unsupported document: a texture reference has the type code {c}, neither brtx nor pctx"),
            NoSuchTexture { class, identity } => {
                write!(f, "damaged document: a texture reference names {class} {identity}, which the document lacks")
            }
            NoSuchPicture { identity } => {
                write!(f, "damaged document: a picture texture names the picture C3dCImage {identity}, which the document lacks")
            }
            RecordVersion(v) => write!(f, "unsupported document: a texture's record part of version {v}, not 1 or 2"),
            Count(n) => write!(f, "unsupported document: a texture with {n} components, not 1 to 3"),
            ColourSpace { component, word } => write!(
                f,
                "unsupported document: the colour space word of {} is {word:#x}, neither 0 nor 0x40",
                place(component)
            ),
            OutputBits { component, word } => write!(
                f,
                "unsupported document: the outputs word of {} is {word:#x}, with bits other than 0x1, 0x2 and 0x4",
                place(component)
            ),
        }
    }
}

impl std::error::Error for TextureRefusal {}

/// The mapping part:'s inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// The mapping mode's number (0 Object Space to 16 World Cubic, as stored).
    pub mode: u32,
    /// The switches, as stored (their meaning: Open).
    pub switches: u32,
    pub scale: Vec3,
    pub offset: Vec3,
    /// A quaternion: x, y, z, then w.
    pub turn: [f64; 4],
    /// Four animation track identities, 0 for none.
    pub tracks: [u32; 4],
}

impl Mapping {
    /// `b` holds the mapping part's 236 bytes; its version word is not used .
    pub fn read(b: &[u8], o: ByteOrder) -> Mapping {
        let d = |at| double(b, at, o);
        Mapping {
            mode: word(b, 4, o),
            switches: word(b, 8, o),
            scale: [d(12), d(20), d(28)],
            offset: [d(36), d(44), d(52)],
            turn: [d(60), d(68), d(76), d(84)],
            tracks: std::array::from_fn(|k| word(b, 92 + 4 * k, o)),
        }
    }
}

/// The record part's form: how its angles, frequencies and phase amounts are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordForm {
    /// Version 1: signed 32-bit whole numbers.
    Whole,
    /// Version 2: singles.
    Single,
}

/// The result as saved: a colour word and four doubles, as stored. Not part of the settings;
/// a texture holds it before any evaluation (the colour, and as 3.0 the gradient, reach
/// pictures; the value never), which `render::textured` reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedResult {
    pub colour: u32,
    pub numbers: [f64; 4],
}

impl SavedResult {
    /// The result as saved, from a record part's bytes (the same in either form: a word and four
    /// doubles as stored).,
    pub fn read(b: &[u8], o: ByteOrder) -> SavedResult {
        SavedResult {
            colour: word(b, AT_SAVED_RESULT, o),
            numbers: std::array::from_fn(|k| double(b, AT_SAVED_RESULT + 4 + 8 * k, o)),
        }
    }
}

/// A procedural texture's record part, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct ProceduralTexture {
    pub form: RecordForm,
    /// 's inputs (1); the angles, frequencies and amounts are the stored numbers,
    /// exactly (each form's numbers are exact in a double).
    pub settings: TextureSettings,
    pub saved_result: SavedResult,
    /// The texture's own animation track, 0 for none.
    pub track: u32,
    /// The words two copies are compared in (Which stored words two copies are
    /// compared in); `None` for a texture built from a description, which has no record.
    pub copy_words: Option<CopyWords>,
    /// Where the record is of version 2: its settings as 3.0 reads them (every record as version 1,
    /// the bits of each single taken as a whole number; Variants). `None` otherwise.
    pub as_bryce30: Option<Box<TextureSettings>>,
}

/// The words of a record part in which two procedural copies are compared. (Which
/// stored words two copies are compared in). (Copies of a texture)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyWords {
    /// As 3.1 and 5.0 compare them: the numbers (angles, frequencies, phase amounts) as the
    /// singles the record's own version gives.
    pub stored: Vec<u32>,
    /// As 3.0 compares them: every record read as version 1.
    pub whole: Vec<u32>,
}

impl CopyWords {
    /// The words of `b` (the record part, 1,088 bytes) that decide, in order: the fields the
    /// layout names (but for the version word and the track), the words marks not used that
    /// lists, every colour word with its top byte, and the result as saved; the numbers as
    /// binary32 bits (a whole number as the nearest single, ties to even).
    fn of(b: &[u8], o: ByteOrder) -> CopyWords {
        let count = word(b, AT_COUNT, o);
        // the offsets of the numbers: the global phase's amount, angles and frequencies, and
        // each component's space and phase
        let mut numbers: Vec<usize> = (AT_PHASE + 4..AT_PHASE + 28).step_by(4).collect();
        let mut compared: Vec<usize> = (AT_COUNT..AT_COMPONENTS).step_by(4).collect();
        for k in 0..MAX_COMPONENTS {
            let at = AT_COMPONENTS + COMPONENT_LEN * k;
            // the word at offset 4 and those from 96 on are not used and do not decide
            compared.push(at);
            compared.extend((at + 8..at + 96).step_by(4));
            numbers.extend((at + AT_C_ANGLES..at + AT_C_ANGLES + 8).step_by(4));
            numbers.extend((at + AT_C_FREQUENCIES..at + AT_C_FREQUENCIES + 12).step_by(4));
            numbers.extend((at + AT_C_PHASE + 4..at + AT_C_PHASE + 28).step_by(4));
        }
        compared.extend((AT_SAVED_RESULT..AT_SAVED_RESULT + 36).step_by(4));
        // the six words at 1024 to 1047 where the count is 1, three where it is 2, none where 3
        match count {
            1 => compared.extend((1024..1048).step_by(4)),
            2 => compared.extend([1028, 1036, 1044]),
            _ => {}
        }
        compared.extend((1064..1084).step_by(4));
        let words = |whole_numbers: bool| -> Vec<u32> {
            compared
                .iter()
                .map(|&at| {
                    let w = word(b, at, o);
                    if whole_numbers && numbers.contains(&at) { (w as i32 as f32).to_bits() } else { w }
                })
                .collect()
        };
        CopyWords { stored: words(word(b, 0, o) <= 1), whole: words(true) }
    }
}

/// What a texture resource holds after its mapping.
#[derive(Clone, Debug, PartialEq)]
pub enum TextureBody {
    Procedural(Box<ProceduralTexture>),
    /// A picture texture's own part, the picture it names.
    Picture(PictureTexture),
}

/// A picture texture's picture part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureTexture {
    /// The part's version word, as stored (not used by a reader).
    pub version: u32,
    /// The identity of the C3dCImage it shows.
    pub picture: u32,
    /// That picture's index in `Scene::pictures`.
    pub index: usize,
}

/// The picture part's version word and picture identity; `b` holds its 136 bytes.
pub fn picture_part(b: &[u8], o: ByteOrder) -> (u32, u32) {
    (word(b, AT_PICTURE_VERSION, o), word(b, AT_PICTURE_IDENTITY, o))
}

/// A texture resource a reference names.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub class: TextureClass,
    pub identity: u32,
    pub name: Vec<u8>,
    pub mapping: Mapping,
    pub body: TextureBody,
}

impl Texture {
    /// The procedural texture's record part, `None` for a picture texture.
    pub fn procedural(&self) -> Option<&ProceduralTexture> {
        match &self.body {
            TextureBody::Procedural(p) => Some(p),
            TextureBody::Picture(_) => None,
        }
    }

    /// The picture texture's own part, `None` for a procedural texture.
    pub fn picture(&self) -> Option<&PictureTexture> {
        match &self.body {
            TextureBody::Picture(p) => Some(p),
            TextureBody::Procedural(_) => None,
        }
    }
}

/// Reads one record part's words and numbers. Textures (one reader per form)
struct Record<'a> {
    b: &'a [u8],
    o: ByteOrder,
    form: RecordForm,
}

impl Record<'_> {
    fn word(&self, at: usize) -> u32 {
        word(self.b, at, self.o)
    }

    /// A word as a whole number of the settings (types, modes and the like are words,
    /// Limits).
    fn whole(&self, at: usize) -> i64 {
        i64::from(self.word(at))
    }

    /// "A number": a signed 32-bit whole number in version 1, a single in version 2.
    fn number(&self, at: usize) -> f64 {
        match self.form {
            // The textures take these numbers as singles (Inputs): a whole number is
            // read as the nearest single, ties to even (Which stored words two copies
            // are compared in; copy-words-pictures f-none, f-same).
            RecordForm::Whole => f64::from(self.word(at) as i32 as f32),
            RecordForm::Single => f64::from(single(self.b, at, self.o)),
        }
    }

    fn outputs(&self, at: usize, component: Option<usize>) -> Result<OutputSet, TextureRefusal> {
        let w = self.word(at);
        if w & !(OUTPUT_VALUE | OUTPUT_COLOUR | OUTPUT_BUMP) != 0 {
            return Err(TextureRefusal::OutputBits { component, word: w });
        }
        Ok(OutputSet { colour: w & OUTPUT_COLOUR != 0, value: w & OUTPUT_VALUE != 0, bump: w & OUTPUT_BUMP != 0 })
    }

    /// A noise word. (bits 0 and 1: dimension less one; bits 2 to 7: type;
    /// bits 8 to 31: ten times method plus octaves)
    fn noise(&self, at: usize) -> NoiseSettings {
        let w = self.word(at);
        let rest = i64::from(w >> 8);
        NoiseSettings {
            type_number: i64::from((w >> 2) & 0x3f),
            method: rest / 10,
            octaves: rest % 10,
            dimension: Dimension::from_number(i64::from(w & 0x3) + 1),
        }
    }

    /// Two angles, then three frequencies.
    fn space(&self, angles: usize, frequencies: usize) -> NoiseSpace {
        NoiseSpace {
            angles: [self.number(angles), self.number(angles + 4)],
            frequencies: std::array::from_fn(|k| self.number(frequencies + 4 * k)),
        }
    }

    /// Noise word, amount, two angles, three frequencies.
    fn phase(&self, at: usize) -> Phase {
        Phase { amount: self.number(at + 4), noise: self.noise(at), space: self.space(at + 8, at + 16) }
    }

    /// `number` (a word), `n` (a signed word), `a` and `b` (singles).
    fn filter(&self, at: usize) -> Filter {
        Filter {
            number: self.whole(at),
            n: i64::from(self.word(at + 4) as i32),
            a: single(self.b, at + 8, self.o),
            b: single(self.b, at + 12, self.o),
        }
    }

    /// A colour word: red × 65,536 + green × 256 + blue; the top byte not used.
    fn colour(&self, at: usize) -> [u8; 3] {
        let w = self.word(at);
        [(w >> 16) as u8, (w >> 8) as u8, w as u8]
    }

    /// Mode, colour space, three colours.
    fn colours(&self, at: usize, component: Option<usize>) -> Result<ColourMap, TextureRefusal> {
        let space = match self.word(at + 4) {
            SPACE_RGB => ColourSpace::Rgb,
            SPACE_HLS => ColourSpace::Hls,
            word => return Err(TextureRefusal::ColourSpace { component, word }),
        };
        Ok(ColourMap {
            mode: self.whole(at),
            space,
            a: self.colour(at + 8),
            b: self.colour(at + 12),
            c: self.colour(at + 16),
        })
    }

    /// Component `k` (from 0).
    fn component(&self, k: usize) -> Result<Component, TextureRefusal> {
        let at = AT_COMPONENTS + COMPONENT_LEN * k;
        Ok(Component {
            outputs: self.outputs(at + AT_C_OUTPUTS, Some(k))?,
            noise: self.noise(at + AT_C_NOISE),
            space: self.space(at + AT_C_ANGLES, at + AT_C_FREQUENCIES),
            phase: self.phase(at + AT_C_PHASE),
            filter: self.filter(at + AT_C_FILTER),
            colours: self.colours(at + AT_C_COLOURS, Some(k))?,
        })
    }
}

impl ProceduralTexture {
    /// `b` holds the record part's 1,088 bytes, in the resource's byte order. The same
    /// reading serves every version the document is read as: a record of version 2 as 3.0
    /// or 3.1 is the implementer's choice (Variants), and it is read as 5.0 reads it.
    pub fn read(b: &[u8], o: ByteOrder) -> Result<ProceduralTexture, TextureRefusal> {
        Self::read_versioned(b, o, false)
    }

    /// A key of a texture's own record track holds a record part; its version word 0 is read
    /// as version 1's form. (A texture's own track: a reader takes a key's record
    /// part in version 1's form when its version word is 0 or 1)
    pub fn read_key(b: &[u8], o: ByteOrder) -> Result<ProceduralTexture, TextureRefusal> {
        Self::read_versioned(b, o, true)
    }

    /// A key's record part read in the form `form` whatever its version word: as 3.0 and 3.1
    /// always version 1's (whole numbers turned into binary32), as 5.0 always version 2's
    /// (every word as it stands, read as binary32).
    pub fn read_key_as(b: &[u8], o: ByteOrder, form: RecordForm) -> Result<ProceduralTexture, TextureRefusal> {
        Self::read_form(b, o, form)
    }

    fn read_versioned(b: &[u8], o: ByteOrder, zero_is_one: bool) -> Result<ProceduralTexture, TextureRefusal> {
        let form = match word(b, 0, o) {
            0 if zero_is_one => RecordForm::Whole,
            1 => RecordForm::Whole,
            2 => RecordForm::Single,
            v => return Err(TextureRefusal::RecordVersion(v)),
        };
        Self::read_form(b, o, form)
    }

    fn read_form(b: &[u8], o: ByteOrder, form: RecordForm) -> Result<ProceduralTexture, TextureRefusal> {
        let settings = Self::read_form_settings(b, o, form)?;
        let saved_result = SavedResult {
            colour: word(b, AT_SAVED_RESULT, o),
            numbers: std::array::from_fn(|k| double(b, AT_SAVED_RESULT + 4 + 8 * k, o)),
        };
        // As 3.0 a record of version 2 is read as version 1 (Variants).
        let as_bryce30 = match form {
            RecordForm::Single => Self::read_form_settings(b, o, RecordForm::Whole).ok().map(Box::new),
            RecordForm::Whole => None,
        };
        Ok(ProceduralTexture {
            form,
            settings,
            saved_result,
            track: word(b, AT_TRACK, o),
            copy_words: Some(CopyWords::of(b, o)),
            as_bryce30,
        })
    }

    /// The settings of a record part read in the form `form`.
    fn read_form_settings(b: &[u8], o: ByteOrder, form: RecordForm) -> Result<TextureSettings, TextureRefusal> {
        let r = Record { b, o, form };
        let count = r.word(AT_COUNT);
        if count == 0 || count as usize > MAX_COMPONENTS {
            return Err(TextureRefusal::Count(count));
        }
        // Components beyond the count are not part of the settings and are not read.
        let components = (0..count as usize).map(|k| r.component(k)).collect::<Result<Vec<_>, _>>()?;
        Ok(TextureSettings {
            outputs: r.outputs(AT_OUTPUTS, None)?,
            components,
            blend: [r.whole(AT_BLEND), r.whole(AT_BLEND + 4)],
            phase: r.phase(AT_PHASE),
            filter: r.filter(AT_FILTER),
            colours: r.colours(AT_COLOURS, None)?,
        })
    }
}
