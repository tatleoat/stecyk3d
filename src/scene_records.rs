// SPDX-License-Identifier: MIT
//! The records of a scene document: settings, object lists, placements, the torus.
//!
//! (a tree's placement, version 2: PlacementPart::parse_bryce5), (the placement part's
//! version 2: its third flag word and track slots, PlacementPart::parse_either), revised
//! (the two depths of Bryce 5's part, DocSettings::depths)
//! (Records)

use std::fmt;

use crate::geom::Vec3;

/// The byte order of a record's numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
    Big,
    Little,
}

/// Why a record cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordError {
    WrongLength { record: &'static str, len: usize, expected: usize },
    ListFirstWord(u32),
    UnknownCodes { row: usize, kind: FourCc, class: FourCc },
    UnknownVersion { record: &'static str, version: u32 },
    MaterialCode(FourCc),
    GradientCode(FourCc),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RecordError::*;
        match self {
            WrongLength { record, len, expected } => {
                write!(f, "damaged document: the {record} is {len} bytes, not {expected}")
            }
            ListFirstWord(w) => write!(f, "damaged document: an object list begins with {w}, not 1"),
            UnknownCodes { row, kind, class } => {
                write!(f, "damaged document: object list row {row} has the unknown codes {kind}/{class}")
            }
            UnknownVersion { record, version } => {
                write!(f, "unsupported document: {record} version {version}")
            }
            MaterialCode(c) => write!(f, "damaged document: material code {c} is neither 0 nor base"),
            GradientCode(c) => write!(f, "damaged document: a light's gradient code {c} is neither 0 nor base"),
        }
    }
}

impl std::error::Error for RecordError {}

/// A four-character code. (Records)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc(pub [u8; 4]);

impl FourCc {
    /// The code read as a word in `order`, its value taken as big-endian characters.
    pub(crate) fn from_word(b: &[u8], order: ByteOrder) -> FourCc {
        FourCc(word(b, 0, order).to_be_bytes())
    }

    pub const fn new(code: &[u8; 4]) -> FourCc {
        FourCc(*code)
    }

    pub fn is_zero(self) -> bool {
        self.0 == [0; 4]
    }
}

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Debug for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FourCc({self})")
    }
}

pub(crate) fn word(b: &[u8], at: usize, order: ByteOrder) -> u32 {
    let w = [b[at], b[at + 1], b[at + 2], b[at + 3]];
    match order {
        ByteOrder::Big => u32::from_be_bytes(w),
        ByteOrder::Little => u32::from_le_bytes(w),
    }
}

pub(crate) fn single(b: &[u8], at: usize, order: ByteOrder) -> f32 {
    f32::from_bits(word(b, at, order))
}

pub(crate) fn double(b: &[u8], at: usize, order: ByteOrder) -> f64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(&b[at..at + 8]);
    f64::from_bits(match order {
        ByteOrder::Big => u64::from_be_bytes(w),
        ByteOrder::Little => u64::from_le_bytes(w),
    })
}

pub(crate) fn check_len(record: &'static str, b: &[u8], expected: usize) -> Result<(), RecordError> {
    if b.len() == expected { Ok(()) } else { Err(RecordError::WrongLength { record, len: b.len(), expected }) }
}

// ---------------------------------------------------------------------------------------
// The document settings
// ---------------------------------------------------------------------------------------

/// Length of the settings data.
pub const SETTINGS_LEN: usize = 416;

/// The part of the picture a render covers; bottom and right excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
}

/// Where a render stood when the document was saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub block: u32,
    pub mask: u32,
    pub column: u32,
    pub row: u32,
}

impl Progress {
    /// 16, 0, 0, 0 means "start over".
    pub fn is_start_over(&self) -> bool {
        *self == Progress { block: 16, mask: 0, column: 0, row: 0 }
    }
}

/// The document settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocSettings {
    pub picture_width: u32,
    pub picture_height: u32,
    pub region: Region,
    pub progress: Progress,
    pub switches: u32,
    pub animation_length: f32,
    pub frame_rate: f32,
    /// The saved time: the time the document stands at, binary32 at offset 68; a negative
    /// time counts as 0. It is the time opening applies tracks at (`opening`).
    pub current_time: f32,
    pub anti_aliasing: u32,
    pub spatial_option: u32,
    /// The lowest byte of the animation switch word.
    pub animation_switch: u8,
    /// The opening time factor, binary32 at offset 148: as 5.0, with the animation switch not
    /// 0, opening applies the tracks at the saved time times it.,
    pub opening_factor: f32,
    /// Bryce 5's part of the settings: the ten words at offsets 156 to 195, big-endian as
    /// the rest; zero in Bryce 3D documents.,; the words a
    /// render reads: points 4 and 5
    pub bryce5_words: [u32; 10],
}

/// Where Bryce 5's words of the settings start.
pub const BRYCE5_PART_AT: usize = 156;
/// Where the structure code of Bryce 5's part is.
pub const STRUCTURE_CODE_AT: usize = 176;
/// Where the ray depth of Bryce 5's part is. revised (offset 180)
const RAY_DEPTH_AT: usize = 180;
/// Where the reflection depth in glass of Bryce 5's part is. revised (offset 184)
const GLASS_DEPTH_AT: usize = 184;

impl DocSettings {
    pub fn parse(b: &[u8]) -> Result<DocSettings, RecordError> {
        check_len("document settings", b, SETTINGS_LEN)?;
        let o = ByteOrder::Big;
        let w = |at| word(b, at, o);
        let time = single(b, 68, o);
        Ok(DocSettings {
            picture_width: w(8),
            picture_height: w(12),
            region: Region { top: w(16), left: w(20), bottom: w(24), right: w(28) },
            progress: Progress { block: w(32), mask: w(36), column: w(40), row: w(44) },
            switches: w(56),
            animation_length: single(b, 60, o),
            frame_rate: single(b, 64, o),
            current_time: if time < 0.0 { 0.0 } else { time },
            anti_aliasing: w(80),
            spatial_option: w(84),
            animation_switch: b[95],
            opening_factor: single(b, 148, o),
            bryce5_words: std::array::from_fn(|k| w(BRYCE5_PART_AT + 4 * k)),
        })
    }

    /// The structure code in Bryce 5's part (offset 176), a four-character code: `bspT` or
    /// `grid` in every document with the part, any other taken as it stands. A render as
    /// 5.0 reads it where the part is present (its first word, offset 156, not 0); 3.0 and
    /// 3.1 do not. What a render does with it is Open .
    pub fn structure_code(&self) -> FourCc {
        FourCc(self.bryce5_words[(STRUCTURE_CODE_AT - BRYCE5_PART_AT) / 4].to_be_bytes())
    }

    /// The two depths of Bryce 5's part, where the part is present (its first word, offset
    /// 156, not 0): the ray depth (offset 180) and the limit of total internal reflections in
    /// a row (offset 184), the numbers a render as 5.0 takes . `None` without the
    /// part. revised (offsets 180 and 184; vector 50)
    pub fn depths(&self) -> Option<(u32, u32)> {
        let w = |at: usize| self.bryce5_words[(at - BRYCE5_PART_AT) / 4];
        (w(BRYCE5_PART_AT) != 0).then(|| (w(RAY_DEPTH_AT), w(GLASS_DEPTH_AT)))
    }

    /// What opening reads of the settings (the saved time, offset 68, and the animation
    /// switch, offset 92), with the form of the document's parts , which the
    /// settings do not hold.,
    pub fn opening_inputs(&self, form: crate::animation::PartsForm) -> crate::animation::OpeningInputs {
        crate::animation::OpeningInputs {
            saved_time: self.current_time,
            animation_switch: self.animation_switch,
            opening_factor: self.opening_factor,
            form,
            asked_time: None,
        }
    }

    /// What opening the document as `version` does with its tracks, by's one rule
    /// (the same the view's transform uses, `scene_view::view_transforms`). An error where
    /// leaves it Open (3.1 with the switch not 0; 5.0 with it not 0 unless the
    /// document is in Bryce 5's form and saved at 0, point 1).
    pub fn opening(
        &self,
        version: crate::version::RenderAs,
        form: crate::animation::PartsForm,
    ) -> Result<crate::animation::Opening, crate::animation::AnimError> {
        crate::animation::Opening::new(version, self.opening_inputs(form))
    }

    /// Whether the region is the whole picture.
    pub fn region_is_whole(&self) -> bool {
        self.region == Region { top: 0, left: 0, bottom: self.picture_height, right: self.picture_width }
    }
}

// ---------------------------------------------------------------------------------------
// The object lists
// ---------------------------------------------------------------------------------------

/// What an object list row names: its kind and the resource class that holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Sphere,
    Cube,
    Cylinder,
    Cone,
    Plane,
    Torus,
    Slab,
    Square,
    Pyramid,
    HeightField,
    TriMesh,
    Group,
    Path,
    /// Bryce 5 only.
    Blob,
    /// Bryce 5 only.
    Tree,
    SphereLight,
    ConeLight,
    CubeLight,
    /// The cylindrical parallel light: lght+cyln, C3dCylLight. [format]
    CylinderLight,
    /// The square spot light: lght+pyrm, C3dPyrLightType. [format]
    PyramidLight,
    Camera,
}

/// The pairs of codes, and the class tag of the resource each names. [format]
const KINDS: [(&[u8; 4], &[u8; 4], &str, ObjectKind); 22] = [
    (b"3dob", b"sphr", "C3dSphere", ObjectKind::Sphere),
    (b"3dob", b"cube", "C3dCube", ObjectKind::Cube),
    (b"3dob", b"cylr", "C3dCylinder", ObjectKind::Cylinder),
    (b"3dob", b"cone", "C3dCone", ObjectKind::Cone),
    (b"3dob", b"plne", "C3dPlane", ObjectKind::Plane),
    (b"3dob", b"toru", "C3dTorusType", ObjectKind::Torus),
    (b"3dob", b"slab", "C3dSlab", ObjectKind::Slab),
    (b"3dob", b"sqre", "C3dSquare", ObjectKind::Square),
    (b"3dob", b"pyra", "C3dPyramid", ObjectKind::Pyramid),
    (b"3dob", b"hfld", "C3dHeightField", ObjectKind::HeightField),
    // : a Symmetrical Lattice's row, read as a row 3dob hfld is (the mirrored
    // switch is the word the part holds, whichever class code the row has)
    (b"3dob", b"symh", "C3dHeightField", ObjectKind::HeightField),
    (b"3dob", b"tmsh", "C3dTriMeshType2", ObjectKind::TriMesh),
    (b"3dob", b"grou", "C3dGroupType", ObjectKind::Group),
    (b"3dob", b"path", "C3dPath", ObjectKind::Path),
    (b"3dob", b"blob", "C3dBlob", ObjectKind::Blob),
    (b"3dob", b"tree", "C3dTree", ObjectKind::Tree),
    (b"lght", b"sphr", "C3dSphereLight", ObjectKind::SphereLight),
    (b"lght", b"cone", "C3dConeLight", ObjectKind::ConeLight),
    (b"lght", b"cube", "C3dCubeLight", ObjectKind::CubeLight),
    // (the tags and codes of the cylindrical parallel light and the
    // square spot light, which no document holds [format])
    (b"lght", b"cyln", "C3dCylLight", ObjectKind::CylinderLight),
    (b"lght", b"pyrm", "C3dPyrLightType", ObjectKind::PyramidLight),
    (b"camr", b"base", "C3dBaseCamera", ObjectKind::Camera),
];

impl ObjectKind {
    /// The kind a pair of codes names.
    pub fn from_codes(kind: FourCc, class: FourCc) -> Option<ObjectKind> {
        KINDS.iter().find(|(k, c, _, _)| **k == kind.0 && **c == class.0).map(|e| e.3)
    }

    /// The class tag of the resource that holds an object of this kind. [format]
    pub fn class_tag(self) -> &'static str {
        KINDS.iter().find(|e| e.3 == self).map(|e| e.2).expect("every kind is in the table")
    }
}

/// One row of an object list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectRow {
    pub kind: ObjectKind,
    /// The identity of the object's resource (of `kind.class_tag()`).
    pub identity: u32,
    /// The identity of the C3dObjList of the objects linked to this one; 0 for none.
    pub linked_list: u32,
}

/// An object list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectList {
    pub order: ByteOrder,
    pub rows: Vec<ObjectRow>,
}

/// Bytes before the rows, and per row.
const LIST_HEAD: usize = 12;
const LIST_ROW: usize = 16;

impl ObjectList {
    /// . Whether each identity names a resource is checked by the caller against
    /// the resource index , with `ObjectKind::class_tag`.
    pub fn parse(b: &[u8]) -> Result<ObjectList, RecordError> {
        if b.len() < LIST_HEAD {
            return Err(RecordError::WrongLength { record: "object list", len: b.len(), expected: LIST_HEAD });
        }
        let order = if b[4..8] == [1, 0, 0, 0] { ByteOrder::Little } else { ByteOrder::Big };
        let first = word(b, 0, order);
        if first != 1 {
            return Err(RecordError::ListFirstWord(first));
        }
        let count = word(b, 8, order) as usize;
        let expected = LIST_HEAD.saturating_add(count.saturating_mul(LIST_ROW));
        check_len("object list", b, expected)?;
        let rows = b[LIST_HEAD..]
            .chunks_exact(LIST_ROW)
            .enumerate()
            .map(|(i, r)| {
                let kind = FourCc::from_word(&r[0..4], order);
                let class = FourCc::from_word(&r[4..8], order);
                let k = ObjectKind::from_codes(kind, class).ok_or(RecordError::UnknownCodes { row: i, kind, class })?;
                Ok(ObjectRow { kind: k, identity: word(r, 8, order), linked_list: word(r, 12, order) })
            })
            .collect::<Result<_, _>>()?;
        Ok(ObjectList { order, rows })
    }
}

// ---------------------------------------------------------------------------------------
// An object's placement
// ---------------------------------------------------------------------------------------

/// Length of the placement part.
pub const PLACEMENT_LEN: usize = 376;
/// The version of Bryce 3D's placement part.
const PLACEMENT_VERSION: u32 = 1;
/// Length of Bryce 5's placement part.
pub const PLACEMENT_V2_LEN: usize = 1024;
/// The version of Bryce 5's placement part.
pub const PLACEMENT_V2_VERSION: u32 = 2;
/// Where the seven track identities are, in both versions. revised,
const PLACEMENT_TRACKS_AT: usize = 220;
/// Where the third flag word is., revised
const THIRD_FLAGS_AT: usize = 252;
/// Where an object held to a path keeps its saved distance, a single.
const SAVED_DISTANCE_AT: usize = 248;

/// The placement part's track slots.
pub mod placement_slot {
    /// Slot 1: `Obj_ePathT`.
    pub const PATH: usize = 1;
    /// Slot 2: `Obj_ePosition`, the second transform's translation.
    pub const POSITION: usize = 2;
    /// Slot 3: `Obj_eOrient`, a quaternion.
    pub const ORIENT: usize = 3;
    /// Slot 4: `Obj_eScale`, three scales.
    pub const SCALE: usize = 4;
    /// Slot 5: `Obj_eShear`, three numbers.
    pub const SHEAR: usize = 5;
    /// Slot 6: `Obj_eOrigin`, the first transform's translation.
    pub const ORIGIN: usize = 6;
}
/// Bit of the first flag word: the camera in use.
pub const CAMERA_IN_USE: u32 = 0x2000_0000;
/// The material code of an object with a material.
const MATERIAL_BASE: FourCc = FourCc::new(b"base");

/// A 3 x 3 matrix by rows and a translation: p goes to p·M + t.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rows: [Vec3; 3],
    pub translation: Vec3,
}

impl Transform {
    pub(crate) fn read(b: &[u8], at: usize, order: ByteOrder) -> Transform {
        let d = |k: usize| double(b, at + 8 * k, order);
        Transform {
            rows: [[d(0), d(1), d(2)], [d(3), d(4), d(5)], [d(6), d(7), d(8)]],
            translation: [d(9), d(10), d(11)],
        }
    }

    /// This transform followed by `then`: matrix F·S, translation t_F·S + t_S, each element
    /// (a·b + c·d) + e·f over the inner index in order, the translation of `then` added last.
    pub fn followed_by(&self, then: &Transform) -> Transform {
        let s = &then.rows;
        let row_times = |v: Vec3| -> Vec3 {
            let e = |j: usize| (v[0] * s[0][j] + v[1] * s[1][j]) + v[2] * s[2][j];
            [e(0), e(1), e(2)]
        };
        let t = row_times(self.translation);
        Transform {
            rows: [row_times(self.rows[0]), row_times(self.rows[1]), row_times(self.rows[2])],
            translation: [t[0] + then.translation[0], t[1] + then.translation[1], t[2] + then.translation[2]],
        }
    }
}

/// The placement part of an object, light or camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementPart {
    /// 1 (Bryce 3D's, 376 bytes) or 2 (Bryce 5's, 1,024 bytes).
    pub version: u32,
    pub first_flags: u32,
    pub second_flags: u32,
    /// The material's identity (class C3dBaseMaterial), or `None` for no material.
    pub material: Option<u32>,
    pub first: Transform,
    pub second: Transform,
    /// The word at offset 252: zero in version 1, the third flag word in version 2 (what its
    /// bits do: Open, kept as a number). revised
    pub third_flags: u32,
    /// The identities of the seven animation tracks (slots: `placement_slot`), 0 for none.
    /// revised (offset 220)
    pub tracks: [u32; 7],
    /// The single at offset 248: an object held to a path keeps its saved distance there
    /// (what a render takes from it:, nothing where a track has keys).
    pub saved_distance: f32,
}

impl PlacementPart {
    /// The placement part of either version, told by its first word: 376 bytes for version
    /// 1, 1,024 for version 2 (`b` must hold at least the version's length; the rest is not
    /// read). Another version is refused.,
    pub fn parse_either(b: &[u8], order: ByteOrder) -> Result<PlacementPart, RecordError> {
        if b.len() < 4 {
            return Err(RecordError::WrongLength { record: "placement", len: b.len(), expected: PLACEMENT_LEN });
        }
        let version = word(b, 0, order);
        let len = Self::len_of_version(version).ok_or(RecordError::UnknownVersion { record: "placement", version })?;
        if b.len() < len {
            return Err(RecordError::WrongLength { record: "placement", len: b.len(), expected: len });
        }
        if version == PLACEMENT_VERSION { Self::parse(&b[..len], order) } else { Self::parse_bryce5(&b[..len], order) }
    }

    /// The length of the placement part a version word gives, `None` for a version no
    /// reader knows.
    pub fn len_of_version(version: u32) -> Option<usize> {
        match version {
            PLACEMENT_VERSION => Some(PLACEMENT_LEN),
            PLACEMENT_V2_VERSION => Some(PLACEMENT_V2_LEN),
            _ => None,
        }
    }

    pub fn parse(b: &[u8], order: ByteOrder) -> Result<PlacementPart, RecordError> {
        check_len("placement", b, PLACEMENT_LEN)?;
        let version = word(b, 0, order);
        if version != PLACEMENT_VERSION {
            return Err(RecordError::UnknownVersion { record: "placement", version });
        }
        Self::fields(b, order)
    }

    /// Bryce 5's placement part, version 2: 1,024 bytes whose first 376 are laid out as
    /// version 1's; the rest is not read.
    pub fn parse_bryce5(b: &[u8], order: ByteOrder) -> Result<PlacementPart, RecordError> {
        check_len("placement", b, PLACEMENT_V2_LEN)?;
        let version = word(b, 0, order);
        if version != PLACEMENT_V2_VERSION {
            return Err(RecordError::UnknownVersion { record: "placement", version });
        }
        Self::fields(b, order)
    }

    /// The fields of version 1's layout, which version 2 shares.
    fn fields(b: &[u8], order: ByteOrder) -> Result<PlacementPart, RecordError> {
        let code = FourCc::from_word(&b[12..16], order);
        let material = if code.is_zero() {
            None
        } else if code == MATERIAL_BASE {
            Some(word(b, 16, order))
        } else {
            return Err(RecordError::MaterialCode(code));
        };
        Ok(PlacementPart {
            version: word(b, 0, order),
            first_flags: word(b, 4, order),
            second_flags: word(b, 8, order),
            material,
            first: Transform::read(b, 20, order),
            second: Transform::read(b, 116, order),
            third_flags: word(b, THIRD_FLAGS_AT, order),
            tracks: std::array::from_fn(|k| word(b, PLACEMENT_TRACKS_AT + 4 * k, order)),
            saved_distance: single(b, SAVED_DISTANCE_AT, order),
        })
    }

    /// The object's own transform, from its own space to the world.
    pub fn world(&self) -> Transform {
        self.first.followed_by(&self.second)
    }

    pub fn is_camera_in_use(&self) -> bool {
        self.first_flags & CAMERA_IN_USE != 0
    }
}

// ---------------------------------------------------------------------------------------
// The primitive shapes
// ---------------------------------------------------------------------------------------

/// Length of the torus's own part.
pub const TORUS_PART_LEN: usize = 28;

/// The torus's part after its placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorusPart {
    pub tube_radius: f64,
    pub ring_radius: f64,
    /// At 20 the tube radius's track (class Tor_eMinorRad), at 24 the ring radius's
    /// (Tor_eMajorRad); 0 for none.
    pub tracks: [u32; 2],
}

impl TorusPart {
    pub fn parse(b: &[u8], order: ByteOrder) -> Result<TorusPart, RecordError> {
        check_len("torus", b, TORUS_PART_LEN)?;
        // : the first word is not a version a reader checks.
        Ok(TorusPart {
            tube_radius: double(b, 4, order),
            ring_radius: double(b, 12, order),
            tracks: [word(b, 20, order), word(b, 24, order)],
        })
    }
}
