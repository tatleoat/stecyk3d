// SPDX-License-Identifier: MIT
//! The substrate file: parsing, validation, typed tables, kept-beside files, finding the
//! file and the tables each render needs.
//! (format version 4; the kept
//! files' layouts,, are in `substrate_kept`)

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::sha256::{Digest, sha256};

pub const MAGIC: &[u8; 8] = b"BRYSUBST";
pub const FORMAT_VERSION: u32 = 4;
pub const MIN_FILE_LEN: usize = 48;
const HEADER_LEN: usize = 16;
const HASH_LEN: usize = 32;
pub const DEFAULT_FILE_NAME: &str = "substrate.bin";
pub const ENV_VAR: &str = "BRYCE_SUBSTRATE";
/// The program's name for itself, the name of the folder of the default place.
/// (Freedom: a folder named after the program). (The default place)
pub const PROGRAM_NAME: &str = "stecyk3d";
/// The folder an earlier release used for the default place; read only when the new folder has no file.
const EARLIER_PROGRAM_NAME: &str = "vista";

/// The platforms whose folder for each user's application data names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    /// The platform this program was built for.
    pub fn current() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

/// The default place of the substrate file: a folder named after the program in the platform's
/// folder for each user's application data. On Windows the folder the APPDATA variable names; on
/// macOS ~/Library/Application Support; on other systems the folder XDG_DATA_HOME names, or
/// ~/.local/share where that variable is not set or is empty. `env` reads an environment
/// variable; an empty value counts as unset. `None` where the folder cannot be told (no APPDATA
/// on Windows, no home folder elsewhere).
pub fn default_folder_for(platform: Platform, env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let get = |k: &str| env(k).filter(|v| !v.is_empty());
    let base = match platform {
        Platform::Windows => PathBuf::from(get("APPDATA")?),
        Platform::MacOs => PathBuf::from(get("HOME")?).join("Library").join("Application Support"),
        Platform::Other => match get("XDG_DATA_HOME") {
            Some(x) => PathBuf::from(x),
            None => PathBuf::from(get("HOME")?).join(".local").join("share"),
        },
    };
    Some(base.join(PROGRAM_NAME))
}

/// The default place for this platform and this process's environment.
pub fn default_folder() -> Option<PathBuf> {
    default_folder_for(Platform::current(), |k| std::env::var_os(k))
}

const REMAKE: &str = "make the substrate again with the current extractor, from your own install";

// ---------------------------------------------------------------------------------------
// Table names and shapes
// ---------------------------------------------------------------------------------------

/// Element type of a numeric table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    I32,
    U32,
    U8,
    F32,
    F64,
}

impl Element {
    pub fn size(self) -> usize {
        match self {
            Element::U8 => 1,
            Element::I32 | Element::U32 | Element::F32 => 4,
            Element::F64 => 8,
        }
    }
}

/// The shape of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableShape {
    Numbers { element: Element, count: usize },
    Labels { count: usize },
}

/// The table names of 02-tables.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TableName {
    Perm,
    Waves,
    NoiseBitsThree,
    NoiseBitsFive,
    Dither,
    LeafPoints,
    LeafUvs,
    LeafTriangles,
    AaOffsets,
    Pattern,
    Pyramid,
    HazeFive,
    StarColours,
    NoiseNames,
    NoiseNamesFive,
    MethodNames,
    BlendNames,
    FilterNames,
    ColorModeNames,
    /// The terrain editor's Filtering presets, Bryce 3D family.
    TerrainFilterPresetsThree,
    /// The same for the Bryce 5 family.
    TerrainFilterPresetsFive,
    /// The terrain editor's default gradient, Bryce 3D family only: 258 colours.
    TerrainDefaultGradientThree,
}

impl TableName {
    pub const ALL: [TableName; 22] = [
        TableName::Perm,
        TableName::Waves,
        TableName::NoiseBitsThree,
        TableName::NoiseBitsFive,
        TableName::Dither,
        TableName::LeafPoints,
        TableName::LeafUvs,
        TableName::LeafTriangles,
        TableName::AaOffsets,
        TableName::Pattern,
        TableName::Pyramid,
        TableName::HazeFive,
        TableName::StarColours,
        TableName::NoiseNames,
        TableName::NoiseNamesFive,
        TableName::MethodNames,
        TableName::BlendNames,
        TableName::FilterNames,
        TableName::ColorModeNames,
        TableName::TerrainFilterPresetsThree,
        TableName::TerrainFilterPresetsFive,
        TableName::TerrainDefaultGradientThree,
    ];

    /// The entry name in the file.
    pub fn name(self) -> &'static str {
        match self {
            TableName::Perm => "perm",
            TableName::Waves => "waves",
            TableName::NoiseBitsThree => "noise-bits-three",
            TableName::NoiseBitsFive => "noise-bits-five",
            TableName::Dither => "dither",
            TableName::LeafPoints => "leaf-points",
            TableName::LeafUvs => "leaf-uvs",
            TableName::LeafTriangles => "leaf-triangles",
            TableName::AaOffsets => "aa-offsets",
            TableName::Pattern => "pattern",
            TableName::Pyramid => "pyramid",
            TableName::HazeFive => "haze-five",
            TableName::StarColours => "star-colours",
            TableName::NoiseNames => "noise-names",
            TableName::NoiseNamesFive => "noise-names-five",
            TableName::MethodNames => "method-names",
            TableName::BlendNames => "blend-names",
            TableName::FilterNames => "filter-names",
            TableName::ColorModeNames => "color-mode-names",
            TableName::TerrainFilterPresetsThree => "terrain-filter-presets-three",
            TableName::TerrainFilterPresetsFive => "terrain-filter-presets-five",
            TableName::TerrainDefaultGradientThree => "terrain-default-gradient-three",
        }
    }

    pub fn from_name(bytes: &[u8]) -> Option<TableName> {
        TableName::ALL.into_iter().find(|t| t.name().as_bytes() == bytes)
    }

    pub fn shape(self) -> TableShape {
        use Element::*;
        use TableShape::*;
        match self {
            TableName::Perm => Numbers { element: I32, count: 256 },
            TableName::Waves => Numbers { element: F64, count: 18 },
            TableName::NoiseBitsThree => Numbers { element: U32, count: 64 },
            TableName::NoiseBitsFive => Numbers { element: U32, count: 64 },
            TableName::Dither => Numbers { element: U8, count: 256 },
            TableName::LeafPoints => Numbers { element: F64, count: 60 },
            TableName::LeafUvs => Numbers { element: F32, count: 40 },
            TableName::LeafTriangles => Numbers { element: U32, count: 60 },
            TableName::AaOffsets => Numbers { element: F64, count: 14 },
            TableName::Pattern => Numbers { element: F64, count: 12 },
            TableName::Pyramid => Numbers { element: F64, count: 2 },
            TableName::HazeFive => Numbers { element: F64, count: 3 },
            TableName::StarColours => Numbers { element: U8, count: 105 },
            TableName::NoiseNames => Labels { count: 27 },
            TableName::NoiseNamesFive => Labels { count: 23 },
            TableName::MethodNames => Labels { count: 13 },
            TableName::BlendNames => Labels { count: 19 },
            TableName::FilterNames => Labels { count: 14 },
            TableName::ColorModeNames => Labels { count: 17 },
            TableName::TerrainFilterPresetsThree => Numbers { element: U8, count: 1152 },
            TableName::TerrainFilterPresetsFive => Numbers { element: U8, count: 1152 },
            TableName::TerrainDefaultGradientThree => Numbers { element: U8, count: 774 },
        }
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What length a table must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthRule {
    Bytes(usize),
    ZeroTerminatedLabels(usize),
}

impl fmt::Display for LengthRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LengthRule::Bytes(n) => write!(f, "{n} bytes"),
            LengthRule::ZeroTerminatedLabels(n) => write!(f, "{n} labels, each ending in a zero byte"),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------

/// Why a substrate file is refused, whole.,
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubstrateError {
    BadMagic,
    /// Version 1, 2 or 3: an earlier extractor's file. Variants
    OldVersion { version: u32 },
    UnknownVersion { version: u32 },
    TooShort { length: usize },
    EntryOverrun { entry: u32 },
    FileHashMismatch,
    UnknownKind { entry: u32, kind: u8 },
    /// A table's role byte not 0, or a kept file's not 1 to 4.
    BadRole { entry: u32, kind: u8, role: u8 },
    UnknownTable { name: Vec<u8> },
    DuplicateTable { name: TableName },
    DuplicateKeptFile { path: String },
    WrongTableLength { table: TableName, expected: LengthRule, found: usize },
    TableHashMismatch { table: TableName },
    TrailingBytes { count: usize },
    UnsafePath { path: Vec<u8>, reason: &'static str },
    TableDamaged { table: TableName, reason: String },
}

impl fmt::Display for SubstrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SubstrateError::*;
        match self {
            BadMagic => write!(f, "not a substrate file (it does not start with BRYSUBST)"),
            OldVersion { version } => write!(
                f,
                "substrate file of format version {version}, written by an earlier extractor; {REMAKE}"
            ),
            UnknownVersion { version } => {
                write!(f, "substrate file of unknown format version {version} (only {FORMAT_VERSION} is read)")
            }
            TooShort { length } => write!(f, "substrate file damaged: only {length} bytes"),
            EntryOverrun { entry } => {
                write!(f, "substrate file damaged: entry {entry} runs past the end")
            }
            FileHashMismatch => write!(f, "substrate file damaged: its final hash does not match"),
            UnknownKind { entry, kind } => {
                write!(f, "substrate file damaged: entry {entry} has unknown kind {kind}")
            }
            BadRole { entry, kind: 0, role } => {
                write!(f, "substrate file damaged: entry {entry} is a table with role {role} (a table's role is 0)")
            }
            BadRole { entry, role, .. } => write!(
                f,
                "substrate file damaged: entry {entry} is a kept file with role {role} (a kept file's role is 1 to 4)"
            ),
            UnknownTable { name } => write!(
                f,
                "substrate file damaged: unknown table {:?}",
                String::from_utf8_lossy(name)
            ),
            DuplicateTable { name } => write!(f, "substrate file damaged: table {name} occurs twice"),
            DuplicateKeptFile { path } => {
                write!(f, "substrate file damaged: kept file {path} listed twice")
            }
            WrongTableLength { table, expected, found } => write!(
                f,
                "substrate file damaged: table {table} is {found} bytes, expected {expected}"
            ),
            TableHashMismatch { table } => {
                write!(f, "substrate file damaged: table {table} does not match its hash")
            }
            TrailingBytes { count } => write!(
                f,
                "substrate file damaged: {count} bytes between the last entry and the final hash"
            ),
            UnsafePath { path, reason } => write!(
                f,
                "substrate file damaged: kept file path {:?} is not a safe relative path ({reason})",
                String::from_utf8_lossy(path)
            ),
            TableDamaged { table, reason } => {
                write!(f, "substrate file damaged: table {table} {reason}")
            }
        }?;
        match self {
            OldVersion { .. } => Ok(()),
            _ => write!(f, "; {REMAKE}"),
        }
    }
}

impl std::error::Error for SubstrateError {}

// ---------------------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------------------

/// A table's numbers, bit for bit as in the file.
#[derive(Clone, Debug, PartialEq)]
pub enum TableData {
    I32(Vec<i32>),
    U32(Vec<u32>),
    U8(Vec<u8>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    Labels(Vec<Vec<u8>>),
}

/// A kind-0 entry.
#[derive(Clone, Debug)]
pub struct Table {
    pub name: TableName,
    /// For the user only; never interpreted.
    pub source: Vec<u8>,
    pub hash: Digest,
    pub bytes: Vec<u8>,
    pub data: TableData,
}

/// The job of a kept file, from its entry's role byte (never from its path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeptRole {
    /// Role 1: the picture of the moon .
    MoonPicture,
    /// Role 2: Bryce 5's star catalogue .
    StarCatalogue,
    /// Role 3: one of the Tree Lab's installed leaves .
    InstalledLeaf,
    /// Role 4: a material or texture preset library (no render reads one).
    PresetLibrary,
}

impl KeptRole {
    /// The role byte's meaning for a kept file.
    pub fn from_byte(role: u8) -> Option<KeptRole> {
        match role {
            1 => Some(KeptRole::MoonPicture),
            2 => Some(KeptRole::StarCatalogue),
            3 => Some(KeptRole::InstalledLeaf),
            4 => Some(KeptRole::PresetLibrary),
            _ => None,
        }
    }

    /// The role byte.
    pub fn byte(self) -> u8 {
        match self {
            KeptRole::MoonPicture => 1,
            KeptRole::StarCatalogue => 2,
            KeptRole::InstalledLeaf => 3,
            KeptRole::PresetLibrary => 4,
        }
    }
}

impl fmt::Display for KeptRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            KeptRole::MoonPicture => "the moon's picture",
            KeptRole::StarCatalogue => "Bryce 5's star catalogue",
            KeptRole::InstalledLeaf => "an installed leaf of the Tree Lab",
            KeptRole::PresetLibrary => "a preset library",
        })
    }
}

/// A kind-1 entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptFile {
    pub path: String,
    pub role: KeptRole,
    pub source: Vec<u8>,
    pub hash: Digest,
    pub length: u32,
}

/// A table the substrate does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Missing(pub TableName);

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the substrate holds no table {}", self.0)
    }
}

impl std::error::Error for Missing {}

/// A validated substrate file.
#[derive(Clone, Debug, Default)]
pub struct Substrate {
    tables: BTreeMap<TableName, Table>,
    kept: Vec<KeptFile>,
    beside: Option<PathBuf>,
}

// ---------------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------------

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.end - self.pos < n {
            return None;
        }
        let s = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
}

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Check a kept file's path.
pub fn check_safe_path(path: &[u8]) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("empty");
    }
    if path.contains(&b'\\') {
        return Err("contains a backslash");
    }
    if path.contains(&b':') {
        return Err("contains a colon");
    }
    if path.contains(&0) {
        return Err("contains a zero byte");
    }
    for comp in path.split(|&b| b == b'/') {
        match comp {
            b"" => return Err("has an empty component"),
            b"." => return Err("has a '.' component"),
            b".." => return Err("has a '..' component"),
            _ => {}
        }
    }
    Ok(())
}

fn length_rule(shape: TableShape) -> LengthRule {
    match shape {
        TableShape::Numbers { element, count } => LengthRule::Bytes(element.size() * count),
        TableShape::Labels { count } => LengthRule::ZeroTerminatedLabels(count),
    }
}

fn length_ok(rule: LengthRule, bytes: &[u8]) -> bool {
    match rule {
        LengthRule::Bytes(n) => bytes.len() == n,
        LengthRule::ZeroTerminatedLabels(n) => {
            bytes.iter().filter(|&&b| b == 0).count() == n && bytes.last() == Some(&0)
        }
    }
}

fn decode(shape: TableShape, bytes: &[u8]) -> TableData {
    fn words<const N: usize>(bytes: &[u8]) -> impl Iterator<Item = [u8; N]> + '_ {
        bytes.chunks_exact(N).map(|c| c.try_into().expect("exact chunk"))
    }
    match shape {
        TableShape::Numbers { element, .. } => match element {
            Element::I32 => TableData::I32(words::<4>(bytes).map(i32::from_le_bytes).collect()),
            Element::U32 => TableData::U32(words::<4>(bytes).map(u32::from_le_bytes).collect()),
            Element::U8 => TableData::U8(bytes.to_vec()),
            Element::F32 => TableData::F32(
                words::<4>(bytes).map(|w| f32::from_bits(u32::from_le_bytes(w))).collect(),
            ),
            Element::F64 => TableData::F64(
                words::<8>(bytes).map(|w| f64::from_bits(u64::from_le_bytes(w))).collect(),
            ),
        },
        TableShape::Labels { .. } => {
            let mut labels: Vec<Vec<u8>> = bytes.split(|&b| b == 0).map(<[u8]>::to_vec).collect();
            labels.pop(); // the empty piece after the last zero byte
            TableData::Labels(labels)
        }
    }
}

fn check_constraints(name: TableName, data: &TableData) -> Result<(), String> {
    fn permutation(values: impl Iterator<Item = i64>) -> Result<(), String> {
        let mut seen = [false; 256];
        for v in values {
            if !(0..256).contains(&v) {
                return Err(format!("is not a permutation of 0 to 255 (holds {v})"));
            }
            if std::mem::replace(&mut seen[v as usize], true) {
                return Err(format!("is not a permutation of 0 to 255 ({v} occurs twice)"));
            }
        }
        Ok(())
    }
    match (name, data) {
        (TableName::Perm, TableData::I32(v)) => permutation(v.iter().map(|&x| x as i64)),
        (TableName::Dither, TableData::U8(v)) => permutation(v.iter().map(|&x| x as i64)),
        (TableName::LeafTriangles, TableData::U32(v)) => match v.iter().find(|&&x| x > 19) {
            Some(x) => Err(format!("names point {x}, outside 0 to 19")),
            None => Ok(()),
        },
        (_, TableData::F64(v)) => match v.iter().position(|x| !x.is_finite()) {
            Some(i) => Err(format!("has a number that is not finite (element {i})")),
            None => Ok(()),
        },
        (_, TableData::F32(v)) => match v.iter().position(|x| !x.is_finite()) {
            Some(i) => Err(format!("has a number that is not finite (element {i})")),
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

impl Substrate {
    /// Parse and validate a whole substrate file.
    pub fn parse(bytes: &[u8]) -> Result<Substrate, SubstrateError> {
        use SubstrateError::*;
        if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
            if bytes.len() < MAGIC.len() && MAGIC.starts_with(bytes) {
                return Err(TooShort { length: bytes.len() });
            }
            return Err(BadMagic);
        }
        if bytes.len() < 12 {
            return Err(TooShort { length: bytes.len() });
        }
        let version = le_u32(&bytes[8..12]);
        match version {
            FORMAT_VERSION => {}
            1..=3 => return Err(OldVersion { version }), // Variants (versions 1, 2, 3)
            _ => return Err(UnknownVersion { version }),
        }
        if bytes.len() < MIN_FILE_LEN {
            return Err(TooShort { length: bytes.len() });
        }
        let hash_start = bytes.len() - HASH_LEN;
        if sha256(&bytes[..hash_start])[..] != bytes[hash_start..] {
            return Err(FileHashMismatch);
        }
        let count = le_u32(&bytes[12..16]);

        let mut r = Reader { bytes, pos: HEADER_LEN, end: hash_start };
        let mut out = Substrate::default();
        for entry in 0..count {
            let overrun = || EntryOverrun { entry };
            let kind = r.take(1).ok_or_else(overrun)?[0];
            if kind > 1 {
                return Err(UnknownKind { entry, kind });
            }
            let role_byte = r.take(1).ok_or_else(overrun)?[0];
            let role = match kind {
                0 if role_byte == 0 => None,
                1 => Some(KeptRole::from_byte(role_byte).ok_or(BadRole { entry, kind, role: role_byte })?),
                _ => return Err(BadRole { entry, kind, role: role_byte }),
            };
            let name_len = r.take(1).ok_or_else(overrun)?[0] as usize;
            let name = r.take(name_len).ok_or_else(overrun)?;
            let src_len = u16::from_le_bytes(r.take(2).ok_or_else(overrun)?.try_into().unwrap());
            let source = r.take(src_len as usize).ok_or_else(overrun)?.to_vec();
            let hash: Digest = r.take(HASH_LEN).ok_or_else(overrun)?.try_into().unwrap();
            let length = le_u32(r.take(4).ok_or_else(overrun)?);
            if kind == 0 {
                let body = r.take(length as usize).ok_or_else(overrun)?;
                let table = TableName::from_name(name).ok_or_else(|| UnknownTable { name: name.to_vec() })?;
                if out.tables.contains_key(&table) {
                    return Err(DuplicateTable { name: table });
                }
                let shape = table.shape();
                let rule = length_rule(shape);
                if !length_ok(rule, body) {
                    return Err(WrongTableLength { table, expected: rule, found: body.len() });
                }
                if sha256(body) != hash {
                    return Err(TableHashMismatch { table });
                }
                let data = decode(shape, body);
                check_constraints(table, &data).map_err(|reason| TableDamaged { table, reason })?;
                out.tables.insert(table, Table { name: table, source, hash, bytes: body.to_vec(), data });
            } else {
                check_safe_path(name).map_err(|reason| UnsafePath { path: name.to_vec(), reason })?;
                let path = String::from_utf8(name.to_vec()).map_err(|_| UnsafePath {
                    path: name.to_vec(),
                    reason: "not UTF-8 text",
                })?;
                if out.kept.iter().any(|k| k.path == path) {
                    return Err(DuplicateKeptFile { path });
                }
                let role = role.expect("kind 1 has a role");
                out.kept.push(KeptFile { path, role, source, hash, length });
            }
        }
        if r.pos != hash_start {
            return Err(TrailingBytes { count: hash_start - r.pos });
        }
        Ok(out)
    }

    /// The tables held, in name order.
    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.values()
    }

    pub fn holds(&self, name: TableName) -> bool {
        self.tables.contains_key(&name)
    }

    pub fn table(&self, name: TableName) -> Result<&Table, Missing> {
        self.tables.get(&name).ok_or(Missing(name))
    }

    /// The kept-beside files listed.
    pub fn kept_files(&self) -> &[KeptFile] {
        &self.kept
    }

    /// The `.files` folder beside the file, when loaded from a path.
    pub fn beside_folder(&self) -> Option<&Path> {
        self.beside.as_deref()
    }

    pub fn set_beside_folder(&mut self, folder: Option<PathBuf>) {
        self.beside = folder;
    }
}

// ---------------------------------------------------------------------------------------
// Typed table access
// ---------------------------------------------------------------------------------------

macro_rules! accessor {
    ($fn:ident, $name:ident, $variant:ident, $t:ty, $n:literal) => {
        #[doc = concat!("spec: SUB-3 (", stringify!($name), ")")]
        pub fn $fn(&self) -> Result<&[$t; $n], Missing> {
            match &self.table(TableName::$name)?.data {
                TableData::$variant(v) => Ok(v.as_slice().try_into().expect("shape checked at parse")),
                _ => unreachable!("table decoded with its own shape"),
            }
        }
    };
}

impl Substrate {
    accessor!(perm, Perm, I32, i32, 256);
    accessor!(waves, Waves, F64, f64, 18);
    accessor!(noise_bits_three, NoiseBitsThree, U32, u32, 64);
    accessor!(noise_bits_five, NoiseBitsFive, U32, u32, 64);
    accessor!(dither, Dither, U8, u8, 256);
    accessor!(leaf_points, LeafPoints, F64, f64, 60);
    accessor!(leaf_uvs, LeafUvs, F32, f32, 40);
    accessor!(leaf_triangles, LeafTriangles, U32, u32, 60);
    accessor!(aa_offsets, AaOffsets, F64, f64, 14);
    accessor!(pattern, Pattern, F64, f64, 12);
    accessor!(pyramid, Pyramid, F64, f64, 2);
    accessor!(haze_five, HazeFive, F64, f64, 3);
    accessor!(star_colours, StarColours, U8, u8, 105);
    accessor!(terrain_filter_presets_three, TerrainFilterPresetsThree, U8, u8, 1152);
    accessor!(terrain_filter_presets_five, TerrainFilterPresetsFive, U8, u8, 1152);
    accessor!(terrain_default_gradient_three, TerrainDefaultGradientThree, U8, u8, 774);

    /// The six Waves centres as points (the file holds the six x, six y, six z).
    /// , Freedom
    pub fn waves_centres(&self) -> Result<[[f64; 3]; 6], Missing> {
        let w = self.waves()?;
        Ok(std::array::from_fn(|i| [w[i], w[6 + i], w[12 + i]]))
    }

    /// The labels of a table of names.
    pub fn labels(&self, name: TableName) -> Result<&[Vec<u8>], Missing> {
        match &self.table(name)?.data {
            TableData::Labels(v) => Ok(v),
            _ => Err(Missing(name)),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Kept-beside files
// ---------------------------------------------------------------------------------------

/// Why a kept file could not be read.
#[derive(Debug)]
pub enum KeptFileError {
    NotListed { path: String },
    NoFolder { path: String },
    Missing { path: PathBuf, error: std::io::Error },
    LengthDiffers { path: PathBuf, expected: u64, found: u64 },
    HashDiffers { path: PathBuf },
    /// The file does not have the layout its role gives it .
    Damaged { path: String, condition: String },
    /// No kept file of the role in the folders a render may take it from.
    NoneForRole { role: KeptRole, leaf: Option<String> },
}

impl fmt::Display for KeptFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeptFileError::NotListed { path } => {
                write!(f, "the substrate does not list the file {path}; {REMAKE}")
            }
            KeptFileError::NoFolder { path } => {
                write!(f, "no folder beside the substrate is known, to read {path}")
            }
            KeptFileError::Missing { path, error } => write!(
                f,
                "kept file {} cannot be read ({error}); {REMAKE}",
                path.display()
            ),
            KeptFileError::LengthDiffers { path, expected, found } => write!(
                f,
                "kept file {} is {found} bytes, the substrate says {expected}; {REMAKE}",
                path.display()
            ),
            KeptFileError::HashDiffers { path } => write!(
                f,
                "kept file {} does not match its hash in the substrate; {REMAKE}",
                path.display()
            ),
            KeptFileError::Damaged { path, condition } => {
                write!(f, "kept file {path} is damaged: {condition}; {REMAKE}")
            }
            KeptFileError::NoneForRole { role, leaf } => {
                write!(f, "the substrate keeps no file for {role}")?;
                if let Some(name) = leaf {
                    write!(f, " named {name:?}")?;
                }
                write!(f, "; {REMAKE}")
            }
        }
    }
}

impl std::error::Error for KeptFileError {}

/// The folder beside a substrate file: its extension replaced by `files`.
pub fn beside_folder_of(file: &Path) -> PathBuf {
    file.with_extension("files")
}

impl Substrate {
    /// Read a kept file from the folder beside this substrate.
    pub fn read_kept(&self, path: &str) -> Result<Vec<u8>, KeptFileError> {
        let folder = self
            .beside
            .as_deref()
            .ok_or_else(|| KeptFileError::NoFolder { path: path.to_string() })?;
        self.read_kept_from(folder, path)
    }

    /// Read a kept file from a given folder.
    pub fn read_kept_from(&self, folder: &Path, path: &str) -> Result<Vec<u8>, KeptFileError> {
        let entry = self
            .kept
            .iter()
            .find(|k| k.path == path)
            .ok_or_else(|| KeptFileError::NotListed { path: path.to_string() })?;
        let mut full = folder.to_path_buf();
        for comp in entry.path.split('/') {
            full.push(comp);
        }
        let expected = entry.length as u64;
        let meta = fs::metadata(&full).map_err(|error| KeptFileError::Missing { path: full.clone(), error })?;
        if !meta.is_file() {
            return Err(KeptFileError::Missing {
                path: full,
                error: std::io::Error::other("not a file"),
            });
        }
        if meta.len() != expected {
            return Err(KeptFileError::LengthDiffers { path: full, expected, found: meta.len() });
        }
        let bytes = fs::read(&full).map_err(|error| KeptFileError::Missing { path: full.clone(), error })?;
        if bytes.len() as u64 != expected {
            return Err(KeptFileError::LengthDiffers { path: full, expected, found: bytes.len() as u64 });
        }
        if sha256(&bytes) != entry.hash {
            return Err(KeptFileError::HashDiffers { path: full });
        }
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------------------
// Which kept file a render takes, by build ("Which kept file is which, by role")
// ---------------------------------------------------------------------------------------

/// The folder names of the builds the substrate names. [format]
pub const FOLDER_BRYCE_30: &str = "Bryce 3D 3.0"; // [format]
/// [format]
pub const FOLDER_BRYCE_31: &str = "Bryce 3D 3.1"; // [format]
/// [format]
pub const FOLDER_COREL_5: &str = "Corel Bryce 5.0"; // [format]
/// [format]
pub const FOLDER_DAZ_5: &str = "DAZ Bryce 5 (2006)"; // [format]
/// A folder whose name begins with this holds a build of Bryce 3D.
const BRYCE_3D_PREFIX: &str = "Bryce 3D";

/// The two families of builds a kept file's folder may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildFamily {
    /// Bryce 3D (3.0 or 3.1): a folder name beginning with "Bryce 3D".
    Bryce3D,
    /// Bryce 5: every other folder.
    Bryce5,
}

/// The family of the build a folder holds. (case-sensitive prefix: our choice,
pub fn folder_family(folder: &str) -> BuildFamily {
    if folder.starts_with(BRYCE_3D_PREFIX) { BuildFamily::Bryce3D } else { BuildFamily::Bryce5 }
}

/// The family a render takes its kept files from.
pub fn version_family(version: crate::version::RenderAs) -> BuildFamily {
    match version {
        crate::version::RenderAs::Bryce30 | crate::version::RenderAs::Bryce31 => BuildFamily::Bryce3D,
        crate::version::RenderAs::Bryce50 => BuildFamily::Bryce5,
    }
}

/// The named folders a render takes first, in order.
fn preferred_folders(version: crate::version::RenderAs) -> &'static [&'static str] {
    match version {
        crate::version::RenderAs::Bryce30 => &[FOLDER_BRYCE_30],
        crate::version::RenderAs::Bryce31 => &[FOLDER_BRYCE_31],
        crate::version::RenderAs::Bryce50 => &[FOLDER_COREL_5, FOLDER_DAZ_5],
    }
}

/// A kept file's path split into its build's folder and the path inside that build's
/// install; `None` for a path of one component, which names no build (our choice).
pub fn split_build(path: &str) -> Option<(&str, &str)> {
    path.split_once('/')
}

/// A file's last path component without a final ".lps" (in any letter case), for
/// comparing leaf names.;
/// (any mix of letter case in the extension)
fn leaf_key(name: &str) -> String {
    let last = name.rsplit('/').next().unwrap_or(name);
    let n = last.len();
    let stem = if n >= 4 && last.is_char_boundary(n - 4) && last[n - 4..].eq_ignore_ascii_case(".lps") {
        &last[..n - 4]
    } else {
        last
    };
    stem.chars().flat_map(char::to_lowercase).collect()
}

impl Substrate {
    /// The builds' folders a render as `version` takes kept files of `role` from, in
    /// order. The moon's picture and the preset libraries: the version's own named folders
    /// (3.0: Bryce 3D 3.0; 3.1: Bryce 3D 3.1; 5.0: Corel Bryce 5.0, then DAZ Bryce 5 (2006)),
    /// then the other folders of the same family in the order the substrate lists their
    /// files. The star catalogue and the installed leaves: the order of a render as 5.0,
    /// whatever the version. Only folders holding a listed file of the role appear.
    pub fn folder_order(&self, version: crate::version::RenderAs, role: KeptRole) -> Vec<&str> {
        let version = match role {
            KeptRole::StarCatalogue | KeptRole::InstalledLeaf => crate::version::RenderAs::Bryce50,
            KeptRole::MoonPicture | KeptRole::PresetLibrary => version,
        };
        let mut listed: Vec<&str> = Vec::new();
        for k in self.kept.iter().filter(|k| k.role == role) {
            if let Some((folder, _)) = split_build(&k.path)
                && !listed.contains(&folder)
            {
                listed.push(folder);
            }
        }
        let preferred = preferred_folders(version);
        let family = version_family(version);
        let mut order: Vec<&str> =
            preferred.iter().filter_map(|p| listed.iter().copied().find(|f| f == p)).collect();
        order.extend(
            listed.iter().copied().filter(|f| !preferred.contains(f) && folder_family(f) == family),
        );
        order
    }

    fn kept_in_folder<'a>(&'a self, folder: &'a str, role: KeptRole) -> impl Iterator<Item = &'a KeptFile> + 'a {
        self.kept
            .iter()
            .filter(move |k| k.role == role && split_build(&k.path).is_some_and(|(f, _)| f == folder))
    }

    /// The kept file of `role` a render as `version` takes: the first file of the role
    /// (in listing order: our choice, Freedom) in the first folder of
    /// `folder_order`. For the installed leaves use `installed_leaf`.
    pub fn kept_for_role(&self, version: crate::version::RenderAs, role: KeptRole) -> Option<&KeptFile> {
        self.folder_order(version, role)
            .into_iter()
            .find_map(|folder| self.kept_in_folder(folder, role).next())
    }

    /// The installed leaves: the files of role 3 of the first Bryce 5 folder (in the order
    /// of a render as 5.0) that keeps any, in listing order; the same for every version.
    pub fn installed_leaves(&self) -> Vec<&KeptFile> {
        let role = KeptRole::InstalledLeaf;
        let order = self.folder_order(crate::version::RenderAs::Bryce50, role);
        match order.first() {
            Some(folder) => self.kept_in_folder(folder, role).collect(),
            None => Vec::new(),
        }
    }

    /// The installed leaf a tree names: the first of `installed_leaves` whose last path
    /// component equals `name`, ignoring letter case, once a final ".lps" is taken off
    /// each. No leaf is taken from another folder.
    pub fn installed_leaf(&self, name: &str) -> Option<&KeptFile> {
        let key = leaf_key(name);
        self.installed_leaves().into_iter().find(|k| leaf_key(&k.path) == key)
    }
}

// ---------------------------------------------------------------------------------------
// Finding and loading the file
// ---------------------------------------------------------------------------------------

/// Where the chosen file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    CommandLine,
    Environment,
    DefaultPlace,
}

/// Why no substrate could be loaded.
#[derive(Debug)]
pub enum LocateError {
    /// No file named, no environment variable, and no default place known.
    NothingGiven,
    NotFound { path: PathBuf, from: Origin },
    Unreadable { path: PathBuf, error: std::io::Error },
    Refused { path: PathBuf, error: SubstrateError },
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MAKE: &str = "make it with the extractor, from your own install";
        match self {
            LocateError::NothingGiven => write!(
                f,
                "no substrate file: name one on the command line or in {ENV_VAR}; {MAKE}"
            ),
            LocateError::NotFound { path, from } => {
                let from = match from {
                    Origin::CommandLine => "named on the command line",
                    Origin::Environment => "named by BRYCE_SUBSTRATE",
                    Origin::DefaultPlace => "at the default place",
                };
                write!(f, "no substrate file at {} ({from}); {MAKE}", path.display())
            }
            LocateError::Unreadable { path, error } => {
                write!(f, "substrate file {} cannot be read: {error}", path.display())
            }
            LocateError::Refused { path, error } => {
                write!(f, "substrate file {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for LocateError {}

/// Choose the substrate file: the command line's, else `BRYCE_SUBSTRATE`'s, else
/// `substrate.bin` in the default folder. An empty environment value counts as unset.
/// The first place given is chosen even if no file is there (the run then ends).
pub fn locate(
    explicit: Option<&Path>,
    env: Option<&OsStr>,
    default_folder: Option<&Path>,
) -> Result<(PathBuf, Origin), LocateError> {
    let (path, from) = if let Some(p) = explicit {
        (p.to_path_buf(), Origin::CommandLine)
    } else if let Some(e) = env.filter(|e| !e.is_empty()) {
        (PathBuf::from(e), Origin::Environment)
    } else if let Some(d) = default_folder {
        (d.join(DEFAULT_FILE_NAME), Origin::DefaultPlace)
    } else {
        return Err(LocateError::NothingGiven);
    };
    if !path.is_file() {
        return Err(LocateError::NotFound { path, from });
    }
    Ok((path, from))
}

/// Read and validate a substrate file, remembering the folder beside it.
pub fn load(path: &Path) -> Result<Substrate, LocateError> {
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LocateError::NotFound { path: path.to_path_buf(), from: Origin::CommandLine }
        } else {
            LocateError::Unreadable { path: path.to_path_buf(), error }
        }
    })?;
    let mut s = Substrate::parse(&bytes)
        .map_err(|error| LocateError::Refused { path: path.to_path_buf(), error })?;
    s.beside = Some(beside_folder_of(path));
    Ok(s)
}

/// Locate (reading `BRYCE_SUBSTRATE` from the environment) and load.
pub fn find_and_load(
    explicit: Option<&Path>,
    default_folder: Option<&Path>,
) -> Result<Substrate, LocateError> {
    let env = std::env::var_os(ENV_VAR);
    match locate(explicit, env.as_deref(), default_folder) {
        Ok((path, _)) => load(&path),
        // Migration only: the earlier release kept its file in a folder named for the earlier program name.
        // locate stays as says; the old folder is tried only when the default place holds nothing.
        Err(LocateError::NotFound { from: Origin::DefaultPlace, path }) => {
            let earlier = path.parent().and_then(Path::parent).map(|base| base.join(EARLIER_PROGRAM_NAME).join(DEFAULT_FILE_NAME));
            match earlier {
                Some(e) if e.is_file() => load(&e),
                _ => Err(LocateError::NotFound { from: Origin::DefaultPlace, path }),
            }
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------------------
// Which renders need which tables
// ---------------------------------------------------------------------------------------

/// The version a scene renders as (the shared type).
pub use crate::version::RenderAs as RenderVersion;

/// Which Bryce supplies a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuppliedBy {
    /// Bryce 3D 3.0 or 3.1, or any Bryce 5.
    AnyBryce,
    /// Bryce 5.
    Bryce5,
}

const EVERY_RENDER: [TableName; 5] =
    [TableName::Perm, TableName::Waves, TableName::AaOffsets, TableName::Pattern, TableName::Pyramid];
const BRYCE5_RENDER: [TableName; 6] = [
    TableName::NoiseBitsFive,
    TableName::Dither,
    TableName::LeafPoints,
    TableName::LeafUvs,
    TableName::LeafTriangles,
    TableName::HazeFive,
];
const TREES_RENDER: [TableName; 4] =
    [TableName::NoiseBitsFive, TableName::LeafPoints, TableName::LeafUvs, TableName::LeafTriangles];

pub fn supplied_by(table: TableName) -> SuppliedBy {
    if EVERY_RENDER.contains(&table) { SuppliedBy::AnyBryce } else { SuppliedBy::Bryce5 }
}

/// The tables a render needs, in table order.
pub fn needed_tables(version: RenderVersion, bryce5_document: bool, has_trees: bool) -> Vec<TableName> {
    let mut need: Vec<TableName> = EVERY_RENDER.to_vec();
    if version == RenderVersion::Bryce50 || bryce5_document {
        need.extend(BRYCE5_RENDER);
    }
    if has_trees {
        need.extend(TREES_RENDER);
    }
    need.sort();
    need.dedup();
    need
}

/// A needed table that the substrate does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingTable {
    pub table: TableName,
    pub supplied_by: SuppliedBy,
}

/// The needed tables not held.
pub fn missing_tables(
    version: RenderVersion,
    bryce5_document: bool,
    has_trees: bool,
    held: impl Fn(TableName) -> bool,
) -> Vec<MissingTable> {
    needed_tables(version, bryce5_document, has_trees)
        .into_iter()
        .filter(|&t| !held(t))
        .map(|t| MissingTable { table: t, supplied_by: supplied_by(t) })
        .collect()
}

/// What a render needs from the substrate beyond its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderNeeds {
    pub version: RenderVersion,
    pub bryce5_document: bool,
    pub has_trees: bool,
    /// The sky shows the moon's picture and the scene carries no picture for it .
    pub moon_picture: bool,
    /// The sky takes its stars from the star catalogue .
    pub catalogue_stars: bool,
    /// The installed leaves the document's trees name .
    pub leaves: Vec<String>,
    /// The user named a folder of the install .
    pub install_folder_named: bool,
}

impl RenderNeeds {
    /// A render needing only tables.
    pub fn tables_only(version: RenderVersion, bryce5_document: bool, has_trees: bool) -> RenderNeeds {
        RenderNeeds {
            version,
            bryce5_document,
            has_trees,
            moon_picture: false,
            catalogue_stars: false,
            leaves: Vec::new(),
            install_folder_named: false,
        }
    }
}

/// The tables a render needs, in table order, with the sky's star-colours.
pub fn needed_tables_for(needs: &RenderNeeds) -> Vec<TableName> {
    let mut need = needed_tables(needs.version, needs.bryce5_document, needs.has_trees);
    if needs.version == RenderVersion::Bryce50 && needs.catalogue_stars {
        need.push(TableName::StarColours);
        need.sort();
        need.dedup();
    }
    need
}

/// The family of Bryce that supplies a kept file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeptSupplier {
    Bryce3D,
    Bryce5,
}

/// A kept file a render needs and the substrate does not provide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingKept {
    pub role: KeptRole,
    /// For an installed leaf, the name the tree gives.
    pub leaf: Option<String>,
    pub supplied_by: KeptSupplier,
}

/// A render refused because tables or kept files are missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderRefused {
    pub missing: Vec<MissingTable>,
    pub missing_kept: Vec<MissingKept>,
}

impl fmt::Display for RenderRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.missing.is_empty() {
            write!(f, "the substrate lacks tables this render needs:")?;
            for m in &self.missing {
                let who = match m.supplied_by {
                    SuppliedBy::AnyBryce => "Bryce 3D 3.0 or 3.1, or any Bryce 5",
                    SuppliedBy::Bryce5 => "Bryce 5",
                };
                write!(f, " {} (from an install of {who});", m.table)?;
            }
        }
        if !self.missing_kept.is_empty() {
            if !self.missing.is_empty() {
                write!(f, " ")?;
            }
            write!(f, "the substrate keeps no file this render needs:")?;
            for m in &self.missing_kept {
                let who = match m.supplied_by {
                    KeptSupplier::Bryce3D => "Bryce 3D",
                    KeptSupplier::Bryce5 => "Bryce 5",
                };
                match &m.leaf {
                    Some(name) => write!(f, " the installed leaf {name:?}")?,
                    None => write!(f, " {}", m.role)?,
                }
                write!(f, " (from {who}: make the substrate again with that Bryce's folder among the folders);")?;
            }
        }
        write!(f, " {REMAKE}")
    }
}

impl std::error::Error for RenderRefused {}

impl Substrate {
    /// Refuse a render whose tables are not all held.
    pub fn check_render(
        &self,
        version: RenderVersion,
        bryce5_document: bool,
        has_trees: bool,
    ) -> Result<(), RenderRefused> {
        self.check_render_needs(&RenderNeeds::tables_only(version, bryce5_document, has_trees))
    }

    /// The kept files a render needs and cannot take.,
    pub fn missing_kept(&self, needs: &RenderNeeds) -> Vec<MissingKept> {
        let mut out = Vec::new();
        if needs.install_folder_named {
            return out;
        }
        if needs.moon_picture && self.kept_for_role(needs.version, KeptRole::MoonPicture).is_none() {
            let supplied_by = match version_family(needs.version) {
                BuildFamily::Bryce3D => KeptSupplier::Bryce3D,
                BuildFamily::Bryce5 => KeptSupplier::Bryce5,
            };
            out.push(MissingKept { role: KeptRole::MoonPicture, leaf: None, supplied_by });
        }
        if needs.version == RenderVersion::Bryce50
            && needs.catalogue_stars
            && self.kept_for_role(needs.version, KeptRole::StarCatalogue).is_none()
        {
            out.push(MissingKept { role: KeptRole::StarCatalogue, leaf: None, supplied_by: KeptSupplier::Bryce5 });
        }
        for name in &needs.leaves {
            if self.installed_leaf(name).is_none() && !out.iter().any(|m| m.leaf.as_deref() == Some(name.as_str())) {
                out.push(MissingKept {
                    role: KeptRole::InstalledLeaf,
                    leaf: Some(name.clone()),
                    supplied_by: KeptSupplier::Bryce5,
                });
            }
        }
        out
    }

    /// Refuse a render whose tables or kept files are not all held; both lists are
    /// given in one refusal (Freedom).
    pub fn check_render_needs(&self, needs: &RenderNeeds) -> Result<(), RenderRefused> {
        let missing: Vec<MissingTable> = needed_tables_for(needs)
            .into_iter()
            .filter(|&t| !self.holds(t))
            .map(|t| MissingTable { table: t, supplied_by: supplied_by(t) })
            .collect();
        let missing_kept = self.missing_kept(needs);
        if missing.is_empty() && missing_kept.is_empty() {
            Ok(())
        } else {
            Err(RenderRefused { missing, missing_kept })
        }
    }

    /// Read the kept file of `role` that a render as `version` takes.
    fn read_role(&self, version: RenderVersion, role: KeptRole) -> Result<(String, Vec<u8>), KeptFileError> {
        let k = self
            .kept_for_role(version, role)
            .ok_or(KeptFileError::NoneForRole { role, leaf: None })?;
        Ok((k.path.clone(), self.read_kept(&k.path)?))
    }

    /// The star catalogue's records (Bryce 5's file whatever the version).
    pub fn read_star_catalogue(&self) -> Result<Vec<crate::substrate_kept::StarRecord>, KeptFileError> {
        let (_, bytes) = self.read_role(RenderVersion::Bryce50, KeptRole::StarCatalogue)?;
        Ok(crate::substrate_kept::parse_star_catalogue(&bytes))
    }

    /// The shape of the installed leaf a tree names.
    pub fn read_installed_leaf(&self, name: &str) -> Result<crate::substrate_kept::LeafShape, KeptFileError> {
        let k = self.installed_leaf(name).ok_or_else(|| KeptFileError::NoneForRole {
            role: KeptRole::InstalledLeaf,
            leaf: Some(name.to_owned()),
        })?;
        let bytes = self.read_kept(&k.path)?;
        crate::substrate_kept::parse_leaf(&bytes)
            .map_err(|e| KeptFileError::Damaged { path: k.path.clone(), condition: e.to_string() })
    }

    /// The moon's picture a render as `version` takes, still packed.
    pub fn read_moon_picture(&self, version: RenderVersion) -> Result<crate::substrate_kept::MoonPicture, KeptFileError> {
        let (path, bytes) = self.read_role(version, KeptRole::MoonPicture)?;
        crate::substrate_kept::find_moon_picture(&bytes)
            .map_err(|e| KeptFileError::Damaged { path, condition: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a version-4 file from entries: (kind, role, name, table bytes or kept hash and
    /// length). test helper; synthetic data.
    pub(crate) fn build_v(
        version: u32,
        entries: &[(u8, u8, &[u8], Option<&[u8]>, Option<(Digest, u32)>)],
    ) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(MAGIC);
        f.extend_from_slice(&version.to_le_bytes());
        f.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (kind, role, name, body, kept) in entries {
            f.push(*kind);
            f.push(*role);
            f.push(name.len() as u8);
            f.extend_from_slice(name);
            f.extend_from_slice(&4u16.to_le_bytes());
            f.extend_from_slice(b"test");
            match (body, kept) {
                (Some(b), _) => {
                    f.extend_from_slice(&sha256(b));
                    f.extend_from_slice(&(b.len() as u32).to_le_bytes());
                    f.extend_from_slice(b);
                }
                (None, Some((h, len))) => {
                    f.extend_from_slice(h);
                    f.extend_from_slice(&len.to_le_bytes());
                }
                _ => unreachable!(),
            }
        }
        let h = sha256(&f);
        f.extend_from_slice(&h);
        f
    }

    /// A version-4 file of tables (kind 0, role 0) and kept files.
    pub(crate) fn build(entries: &[(u8, &[u8], Option<&[u8]>, Option<(Digest, u32)>)]) -> Vec<u8> {
        let with_roles: Vec<_> = entries
            .iter()
            .map(|&(kind, name, body, kept)| (kind, if kind == 0 { 0 } else { 4 }, name, body, kept))
            .collect();
        build_v(FORMAT_VERSION, &with_roles)
    }

    /// A substrate listing kept files with these roles at these paths, in this order.
    fn with_kept(files: &[(u8, &str)]) -> Substrate {
        let kept = files
            .iter()
            .map(|&(role, p)| KeptFile {
                path: p.to_string(),
                role: KeptRole::from_byte(role).unwrap(),
                source: Vec::new(),
                hash: [0; 32],
                length: 0,
            })
            .collect();
        Substrate { kept, ..Substrate::default() }
    }

    #[test]
    fn folder_order_by_version_and_role() {
        use crate::version::RenderAs::*;
        let s = with_kept(&[
            (1, "Extra 5/a"),
            (1, "DAZ Bryce 5 (2006)/a"),
            (1, "Bryce 3D 3.1/a"),
            (1, "Bryce 3D Other/a"),
            (1, "Corel Bryce 5.0/a"),
            (1, "Bryce 3D 3.0/a"),
            (1, "Other/a"),
            (1, "loose"),
            (2, "Bryce 3D 3.0/s"),
            (2, "Other/s"),
        ]);
        let moon = KeptRole::MoonPicture;
        assert_eq!(s.folder_order(Bryce30, moon), ["Bryce 3D 3.0", "Bryce 3D 3.1", "Bryce 3D Other"]);
        assert_eq!(s.folder_order(Bryce31, moon), ["Bryce 3D 3.1", "Bryce 3D Other", "Bryce 3D 3.0"]);
        assert_eq!(s.folder_order(Bryce50, moon), ["Corel Bryce 5.0", "DAZ Bryce 5 (2006)", "Extra 5", "Other"]);
        // The catalogue is Bryce 5's, whatever the version.
        assert_eq!(s.folder_order(Bryce30, KeptRole::StarCatalogue), ["Other"]);
        assert_eq!(s.kept_for_role(Bryce31, KeptRole::StarCatalogue).unwrap().path, "Other/s");
    }

    #[test]
    fn kept_for_role_by_role_byte() {
        use crate::version::RenderAs::*;
        let s = with_kept(&[
            (1, "Mine/moon.pic"),
            (1, "DAZ Bryce 5 (2006)/moon.pic"),
            (1, "Bryce 3D 3.1/moon.pic"),
            (4, "Corel Bryce 5.0/moon.pic"),
        ]);
        let moon = KeptRole::MoonPicture;
        assert_eq!(s.kept_for_role(Bryce50, moon).unwrap().path, "DAZ Bryce 5 (2006)/moon.pic");
        assert_eq!(s.kept_for_role(Bryce30, moon).unwrap().path, "Bryce 3D 3.1/moon.pic");
        assert_eq!(s.kept_for_role(Bryce31, moon).unwrap().path, "Bryce 3D 3.1/moon.pic");
        assert!(s.kept_for_role(Bryce50, KeptRole::StarCatalogue).is_none());
    }

    #[test]
    fn installed_leaves_from_one_folder() {
        let s = with_kept(&[
            (3, "Bryce 3D 3.0/L/Fir.lps"),
            (3, "DAZ Bryce 5 (2006)/L/Oak.LPS"),
            (3, "DAZ Bryce 5 (2006)/More/elm"),
            (3, "Extra/L/Pine.lps"),
            (4, "Corel Bryce 5.0/L/Ash.lps"),
        ]);
        let names: Vec<&str> = s.installed_leaves().iter().map(|k| k.path.as_str()).collect();
        assert_eq!(names, ["DAZ Bryce 5 (2006)/L/Oak.LPS", "DAZ Bryce 5 (2006)/More/elm"]);
        assert_eq!(s.installed_leaf("oak").unwrap().path, "DAZ Bryce 5 (2006)/L/Oak.LPS");
        assert_eq!(s.installed_leaf("OAK.lps").unwrap().path, "DAZ Bryce 5 (2006)/L/Oak.LPS");
        assert_eq!(s.installed_leaf("Elm.lps").unwrap().path, "DAZ Bryce 5 (2006)/More/elm");
        assert!(s.installed_leaf("pine").is_none());
        assert!(s.installed_leaf("fir").is_none());
        assert!(s.installed_leaf("ash").is_none());
    }

    #[test]
    fn role_bytes_checked() {
        let kept = Some(([0u8; 32], 0u32));
        for role in 1..=4 {
            assert!(Substrate::parse(&build_v(4, &[(1, role, b"A/x", None, kept)])).is_ok());
        }
        for role in [0, 5, 255] {
            assert!(matches!(
                Substrate::parse(&build_v(4, &[(1, role, b"A/x", None, kept)])),
                Err(SubstrateError::BadRole { kind: 1, .. })
            ));
        }
        let body: Vec<u8> = vec![7; 105];
        assert!(Substrate::parse(&build_v(4, &[(0, 0, b"star-colours", Some(&body), None)])).is_ok());
        assert!(matches!(
            Substrate::parse(&build_v(4, &[(0, 1, b"star-colours", Some(&body), None)])),
            Err(SubstrateError::BadRole { kind: 0, role: 1, .. })
        ));
        assert_eq!(
            Substrate::parse(&build_v(3, &[])).unwrap_err(),
            SubstrateError::OldVersion { version: 3 }
        );
    }

    #[test]
    fn render_needs_kept_files() {
        use crate::version::RenderAs::*;
        let s = with_kept(&[(1, "Bryce 3D 3.0/m"), (3, "Corel Bryce 5.0/L/Oak.lps")]);
        let mut needs = RenderNeeds::tables_only(Bryce50, false, false);
        needs.moon_picture = true;
        needs.catalogue_stars = true;
        needs.leaves = vec!["oak".into(), "elm".into()];
        let missing = s.missing_kept(&needs);
        let roles: Vec<_> = missing.iter().map(|m| (m.role, m.leaf.clone(), m.supplied_by)).collect();
        assert_eq!(
            roles,
            [
                (KeptRole::MoonPicture, None, KeptSupplier::Bryce5),
                (KeptRole::StarCatalogue, None, KeptSupplier::Bryce5),
                (KeptRole::InstalledLeaf, Some("elm".to_string()), KeptSupplier::Bryce5),
            ]
        );
        assert!(needed_tables_for(&needs).contains(&TableName::StarColours));
        let err = s.check_render_needs(&needs).unwrap_err();
        assert!(err.missing.iter().any(|m| m.table == TableName::StarColours));
        needs.install_folder_named = true;
        assert!(s.missing_kept(&needs).is_empty());
        // As 3.0: the moon comes from Bryce 3D, the catalogue and star-colours are not needed.
        let mut needs = RenderNeeds::tables_only(Bryce30, false, false);
        needs.moon_picture = true;
        needs.catalogue_stars = true;
        assert!(s.missing_kept(&needs).is_empty());
        assert!(!needed_tables_for(&needs).contains(&TableName::StarColours));
        let s = with_kept(&[]);
        assert_eq!(s.missing_kept(&needs)[0].supplied_by, KeptSupplier::Bryce3D);
    }

    fn perm_bytes(values: impl Iterator<Item = i32>) -> Vec<u8> {
        values.flat_map(|v| v.to_le_bytes()).collect()
    }

    fn f64_bytes(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn perm_accepted_and_read_bit_for_bit() {
        let body = perm_bytes((0..256).rev());
        let s = Substrate::parse(&build(&[(0, b"perm", Some(&body), None)])).unwrap();
        assert_eq!(s.perm().unwrap()[0], 255);
        assert_eq!(s.perm().unwrap()[255], 0);
        assert_eq!(s.waves(), Err(Missing(TableName::Waves)));
    }

    #[test]
    fn perm_not_a_permutation_refused() {
        let body = perm_bytes((0..256).map(|i| if i == 5 { 4 } else { i }));
        let err = Substrate::parse(&build(&[(0, b"perm", Some(&body), None)])).unwrap_err();
        assert!(matches!(err, SubstrateError::TableDamaged { table: TableName::Perm, .. }));
    }

    #[test]
    fn waves_regrouped_and_nonfinite_refused() {
        let vals: Vec<f64> = (0..18).map(|i| i as f64 + 0.1).collect();
        let s = Substrate::parse(&build(&[(0, b"waves", Some(&f64_bytes(&vals)), None)])).unwrap();
        assert_eq!(s.waves_centres().unwrap()[1], [1.1, 7.1, 13.1]);
        let mut bad = vals.clone();
        bad[3] = f64::NAN;
        let err = Substrate::parse(&build(&[(0, b"waves", Some(&f64_bytes(&bad)), None)])).unwrap_err();
        assert!(matches!(err, SubstrateError::TableDamaged { table: TableName::Waves, .. }));
    }

    #[test]
    fn leaf_triangles_range() {
        let ok: Vec<u8> = (0..60u32).flat_map(|i| (i % 20).to_le_bytes()).collect();
        assert!(Substrate::parse(&build(&[(0, b"leaf-triangles", Some(&ok), None)])).is_ok());
        let bad: Vec<u8> = (0..60u32).flat_map(|i| (if i == 7 { 20 } else { 0u32 }).to_le_bytes()).collect();
        assert!(matches!(
            Substrate::parse(&build(&[(0, b"leaf-triangles", Some(&bad), None)])),
            Err(SubstrateError::TableDamaged { .. })
        ));
    }

    #[test]
    fn labels_counted_by_zero_bytes() {
        let mut body = Vec::new();
        for i in 0..13 {
            body.extend_from_slice(format!("label{i}").as_bytes());
            body.push(0);
        }
        let s = Substrate::parse(&build(&[(0, b"method-names", Some(&body), None)])).unwrap();
        assert_eq!(s.labels(TableName::MethodNames).unwrap().len(), 13);
        assert_eq!(s.labels(TableName::MethodNames).unwrap()[2], b"label2");
        body.pop();
        assert!(matches!(
            Substrate::parse(&build(&[(0, b"method-names", Some(&body), None)])),
            Err(SubstrateError::WrongTableLength { .. })
        ));
    }

    #[test]
    fn table_hash_checked() {
        let body = perm_bytes(0..256);
        let mut f = build(&[(0, b"perm", Some(&body), None)]);
        // Change the entry hash and recompute the file hash.
        let at = 16 + 1 + 1 + 1 + 4 + 2 + 4; // header, kind, role, name length, "perm", source
        f[at] ^= 1;
        let n = f.len() - 32;
        let h = sha256(&f[..n]);
        f[n..].copy_from_slice(&h);
        assert_eq!(
            Substrate::parse(&f).unwrap_err(),
            SubstrateError::TableHashMismatch { table: TableName::Perm }
        );
    }

    #[test]
    fn duplicate_table_refused() {
        let body = perm_bytes(0..256);
        let f = build(&[(0, b"perm", Some(&body), None), (0, b"perm", Some(&body), None)]);
        assert_eq!(
            Substrate::parse(&f).unwrap_err(),
            SubstrateError::DuplicateTable { name: TableName::Perm }
        );
    }

    #[test]
    fn safe_paths() {
        for ok in [&b"a"[..], b"Data/example.dat", b"a/b/c..d", b".hidden"] {
            assert!(check_safe_path(ok).is_ok(), "{:?}", ok);
        }
        for bad in [&b""[..], b"/a", b"a/", b"a//b", b"./a", b"a/../b", b"..", b"a\\b", b"c:x", b"a\0b"] {
            assert!(check_safe_path(bad).is_err(), "{:?}", bad);
        }
    }

    #[test]
    fn needs_per_render() {
        use RenderVersion::*;
        assert_eq!(needed_tables(Bryce30, false, false).len(), 5);
        assert_eq!(needed_tables(Bryce31, true, false).len(), 11);
        assert_eq!(needed_tables(Bryce50, false, false).len(), 11);
        let trees = needed_tables(Bryce30, false, true);
        assert_eq!(trees.len(), 9);
        assert!(!trees.contains(&TableName::Dither));
        assert!(!trees.contains(&TableName::HazeFive));
        assert_eq!(needed_tables(Bryce50, true, true).len(), 11);
        for v in [Bryce30, Bryce31, Bryce50] {
            for b5 in [false, true] {
                for t in [false, true] {
                    let n = needed_tables(v, b5, t);
                    assert!(!n.contains(&TableName::NoiseBitsThree));
                    assert!(!n.contains(&TableName::NoiseNames));
                }
            }
        }
    }

    #[test]
    fn check_render_names_missing() {
        let body = perm_bytes(0..256);
        let s = Substrate::parse(&build(&[(0, b"perm", Some(&body), None)])).unwrap();
        let err = s.check_render(RenderVersion::Bryce30, false, false).unwrap_err();
        let names: Vec<_> = err.missing.iter().map(|m| m.table).collect();
        assert_eq!(names, vec![TableName::Waves, TableName::AaOffsets, TableName::Pattern, TableName::Pyramid]);
        assert!(err.missing.iter().all(|m| m.supplied_by == SuppliedBy::AnyBryce));
        let err5 = s.check_render(RenderVersion::Bryce50, false, false).unwrap_err();
        assert_eq!(err5.missing.len(), 10);
        let dither = err5.missing.iter().find(|m| m.table == TableName::Dither).unwrap();
        assert_eq!(dither.supplied_by, SuppliedBy::Bryce5);
    }

    #[test]
    fn locate_order() {
        // A real substrate file, if the test was given one (BRYCE_SUBSTRATE_TEST_FILE); else only the other checks run.
        let given = std::env::var_os("BRYCE_SUBSTRATE_TEST_FILE");
        let here = Path::new(given.as_deref().unwrap_or(OsStr::new("/nonexistent/none.bin")));
        let other = OsStr::new("/nonexistent/x.bin");
        // Command line first, even with the variable set.
        if here.is_file() {
            assert_eq!(locate(Some(here), Some(other), None).unwrap().1, Origin::CommandLine);
            assert_eq!(locate(None, Some(here.as_os_str()), None).unwrap().1, Origin::Environment);
        }
        // The chosen place is used, not passed over.
        assert!(matches!(
            locate(None, Some(other), Some(Path::new("/"))),
            Err(LocateError::NotFound { from: Origin::Environment, .. })
        ));
        assert!(matches!(
            locate(None, Some(OsStr::new("")), Some(Path::new("/nonexistent"))),
            Err(LocateError::NotFound { from: Origin::DefaultPlace, .. })
        ));
        assert!(matches!(locate(None, None, None), Err(LocateError::NothingGiven)));
    }

    /// Migration: a default place with no file falls back to the earlier release's folder, and only then.
    #[test]
    fn earlier_folder_fallback() {
        let base = std::env::temp_dir().join(format!("stecyk3d-fallback-{}", std::process::id()));
        let (new, old) = (base.join(PROGRAM_NAME), base.join(EARLIER_PROGRAM_NAME));
        fs::create_dir_all(&new).unwrap();
        fs::create_dir_all(&old).unwrap();
        let find = || find_and_load(None, Some(&new));
        // find_and_load reads BRYCE_SUBSTRATE, which would win over both folders: skip when it is set
        if std::env::var_os(ENV_VAR).is_none() {
            assert!(matches!(find(), Err(LocateError::NotFound { from: Origin::DefaultPlace, .. })));
            fs::write(old.join(DEFAULT_FILE_NAME), b"not a substrate").unwrap();
            // the earlier file is the one read (and refused here, being no substrate)
            match find() {
                Err(LocateError::Refused { path, .. }) => assert_eq!(path, old.join(DEFAULT_FILE_NAME)),
                _ => panic!("the earlier folder was not tried"),
            }
            // a file in the new folder wins
            fs::write(new.join(DEFAULT_FILE_NAME), b"not a substrate either").unwrap();
            match find() {
                Err(LocateError::Refused { path, .. }) => assert_eq!(path, new.join(DEFAULT_FILE_NAME)),
                _ => panic!("the new folder was not preferred"),
            }
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn default_place() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| std::ffi::OsString::from(v))
        };
        let name = PROGRAM_NAME;
        assert_eq!(
            default_folder_for(Platform::Windows, env(&[("APPDATA", "C:\\Users\\u\\AppData\\Roaming")])),
            Some(PathBuf::from("C:\\Users\\u\\AppData\\Roaming").join(name))
        );
        assert_eq!(default_folder_for(Platform::Windows, env(&[("HOME", "/h")])), None);
        assert_eq!(
            default_folder_for(Platform::MacOs, env(&[("HOME", "/Users/u")])),
            Some(PathBuf::from("/Users/u/Library/Application Support").join(name))
        );
        assert_eq!(
            default_folder_for(Platform::Other, env(&[("XDG_DATA_HOME", "/x"), ("HOME", "/h")])),
            Some(PathBuf::from("/x").join(name))
        );
        // not set, or empty: ~/.local/share
        for pairs in [&[("HOME", "/h")][..], &[("XDG_DATA_HOME", ""), ("HOME", "/h")][..]] {
            let pairs: &'static [(&'static str, &'static str)] = Box::leak(pairs.to_vec().into_boxed_slice());
            assert_eq!(default_folder_for(Platform::Other, env(pairs)), Some(PathBuf::from("/h/.local/share").join(name)));
        }
        assert_eq!(default_folder_for(Platform::Other, env(&[])), None);
    }

    #[test]
    fn beside_folder_name() {
        assert_eq!(beside_folder_of(Path::new("/x/substrate.bin")), PathBuf::from("/x/substrate.files"));
    }
}
