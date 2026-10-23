// SPDX-License-Identifier: MIT
//! Making the substrate file and the folder of kept files from the user's own installs.
//!
//! A folder is an install when it holds a program file; the program, the maths library and
//! the menus file are recognised by size and SHA-256, every table is found by opening the
//! sealed row of its file (the file itself is the key) and checked against its length and
//! hash, and the kept files (moon, star catalogue, installed leaves, preset libraries) are
//! listed and copied beside the substrate.
//!
//! (Extracting from installs)

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cases::json::{self, Value};
use crate::sha256::{Digest, sha256, to_hex};
use crate::substrate::{self, FORMAT_VERSION, MAGIC, Substrate, TableName};

// ---------------------------------------------------------------------------------------
// The sealed data (spec:, copied to data/extract)
// ---------------------------------------------------------------------------------------

const BUILDS: &str = include_str!("../data/extract/builds.jsonl");
const TABLES: &str = include_str!("../data/extract/tables.jsonl");
const LOCATIONS: &str = include_str!("../data/extract/locations.jsonl");
const LEAVES: &str = include_str!("../data/extract/leaves.jsonl");
const KEPT_COUNTS: &str = include_str!("../data/extract/kept-counts.jsonl");

/// The 13 ASCII bytes the file's key begins with.
const KEY_PREFIX: &[u8] = b"SUB-12 places";
/// A piece of an opened row: offset, length, times, four bytes little-endian each.
const PIECE_LEN: usize = 12;
/// The longest entry name.
const MAX_ENTRY_NAME: usize = 255;

// ---------------------------------------------------------------------------------------
// Opening a sealed row
// ---------------------------------------------------------------------------------------

/// The key of a file: SHA-256 of " places" and every byte of the file.
pub fn file_key(file: &[u8]) -> Digest {
    let mut h = crate::sha256::Sha256::new();
    h.update(KEY_PREFIX);
    h.update(file);
    h.finish()
}

/// Block `n` of a table's keystream: SHA-256 of the key, the table's name, a zero byte and
/// `n` as four bytes little-endian.
pub fn keystream_block(key: &Digest, table: &str, n: u32) -> Digest {
    let mut data = Vec::with_capacity(32 + table.len() + 5);
    data.extend_from_slice(key);
    data.extend_from_slice(table.as_bytes());
    data.push(0);
    data.extend_from_slice(&n.to_le_bytes());
    sha256(&data)
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    text.as_bytes().chunks(2).map(|p| Some(digit(p[0])? << 4 | digit(p[1])?)).collect()
}

/// Why a table could not be taken from a recognised file (refusal 6 of).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowError {
    /// The sealed text is not hexadecimal.
    NotHex,
    /// The opened row is not 32 bytes and a whole number of pieces.
    BadShape(usize),
    /// A piece runs past the end of the file, or the pieces make more than the length.
    PieceOutside { offset: u32, length: u32, times: u32 },
    /// The pieces do not give the table's length.
    WrongLength { found: usize, expected: usize },
    /// The table's SHA-256 is not the opened row's.
    WrongHash,
}

impl std::fmt::Display for RowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RowError::NotHex => write!(f, "the sealed row is not hexadecimal"),
            RowError::BadShape(n) => write!(f, "the opened row is {n} bytes, not 32 and whole pieces"),
            RowError::PieceOutside { offset, length, times } => {
                write!(f, "a piece (offset {offset}, length {length}, {times} times) lies outside the file or the table")
            }
            RowError::WrongLength { found, expected } => {
                write!(f, "the pieces give {found} bytes, not the table's {expected}")
            }
            RowError::WrongHash => write!(f, "the table's SHA-256 is not the one the row holds"),
        }
    }
}

/// Open a sealed row with its file and join its pieces; check the length and the hash.
pub fn open_table(file: &[u8], table: &str, sealed: &[String], expected_len: usize) -> Result<Vec<u8>, RowError> {
    let mut row = Vec::new();
    for part in sealed {
        row.extend(unhex(part).ok_or(RowError::NotHex)?);
    }
    let key = file_key(file);
    for (n, chunk) in row.chunks_mut(32).enumerate() {
        let block = keystream_block(&key, table, n as u32);
        for (b, k) in chunk.iter_mut().zip(block) {
            *b ^= k;
        }
    }
    if row.len() < 32 || (row.len() - 32) % PIECE_LEN != 0 {
        return Err(RowError::BadShape(row.len()));
    }
    let word = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let mut bytes = Vec::with_capacity(expected_len);
    for p in row[32..].chunks_exact(PIECE_LEN) {
        let (offset, length, times) = (word(&p[0..]), word(&p[4..]), word(&p[8..]));
        let outside = RowError::PieceOutside { offset, length, times };
        let end = u64::from(offset) + u64::from(length);
        if end > file.len() as u64 {
            return Err(outside);
        }
        if (bytes.len() as u64) + u64::from(length) * u64::from(times) > expected_len as u64 {
            return Err(outside);
        }
        for _ in 0..times {
            bytes.extend_from_slice(&file[offset as usize..end as usize]);
        }
    }
    if bytes.len() != expected_len {
        return Err(RowError::WrongLength { found: bytes.len(), expected: expected_len });
    }
    if sha256(&bytes)[..] != row[..32] {
        return Err(RowError::WrongHash);
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------------------
// The known files
// ---------------------------------------------------------------------------------------

/// A role of a known file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FileRole {
    Program,
    Maths,
    Menus,
    Moon,
    Stars,
    /// The terrain editor's resource file (terraineditor.dat).
    Editor,
}

impl FileRole {
    fn from_name(n: &str) -> Option<FileRole> {
        Some(match n {
            "program" => FileRole::Program,
            "maths" => FileRole::Maths,
            "menus" => FileRole::Menus,
            "moon" => FileRole::Moon,
            "stars" => FileRole::Stars,
            "editor" => FileRole::Editor,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
struct KnownFile {
    build: String,
    family: u8,
    role: FileRole,
    install_path: Option<String>,
    size: u64,
    sha256: String,
}

struct Location {
    file_sha256: String,
    table: String,
    sealed: Vec<String>,
}

/// The parsed data tables of the sealed vectors.
struct Known {
    files: Vec<KnownFile>,
    table_len: BTreeMap<String, usize>,
    locations: Vec<Location>,
    leaves: BTreeSet<String>,
    counts: BTreeMap<(String, String), (u64, u64)>,
}

fn lines(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| json::parse(l).expect("data/extract is JSON Lines"))
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn number(v: &Value, key: &str) -> u64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.parse().unwrap_or(0),
        _ => 0,
    }
}

fn known() -> Known {
    let files = lines(BUILDS)
        .map(|v| KnownFile {
            build: text(&v, "build"),
            family: text(&v, "family").parse().unwrap_or(0),
            role: FileRole::from_name(&text(&v, "role")).expect("a known role"),
            install_path: v.get("install_path").and_then(Value::as_str).map(str::to_owned),
            size: number(&v, "size"),
            sha256: text(&v, "sha256"),
        })
        .collect();
    let table_len = lines(TABLES).map(|v| (text(&v, "table"), number(&v, "bytes") as usize)).collect();
    let locations = lines(LOCATIONS)
        .map(|v| Location {
            file_sha256: text(&v, "file_sha256"),
            table: text(&v, "table"),
            sealed: v
                .get("sealed")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect(),
        })
        .collect();
    let leaves = lines(LEAVES).map(|v| text(&v, "sha256")).collect();
    let counts = lines(KEPT_COUNTS)
        .map(|v| ((text(&v, "build"), text(&v, "role")), (number(&v, "files"), number(&v, "total_bytes"))))
        .collect();
    Known { files, table_len, locations, leaves, counts }
}

// ---------------------------------------------------------------------------------------
// Looking in a folder
// ---------------------------------------------------------------------------------------

/// The child of `dir` whose name equals `name` without regard to letter case; of several,
/// the first by name.
fn child_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_lowercase();
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().to_lowercase() == want)
        .map(|e| e.path())
        .collect();
    found.sort();
    found.into_iter().next()
}

/// All children of `dir` named `name` without regard to letter case, sorted.
fn children_named(dir: &Path, name: &str) -> Vec<PathBuf> {
    let want = name.to_lowercase();
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().to_lowercase() == want)
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// A file of the given name (any case) that is a file, not a folder.
fn file_named(dir: &Path, name: &str) -> Option<PathBuf> {
    children_named(dir, name).into_iter().find(|p| p.is_file())
}

/// Resolve "a/b" below `dir`, each component without regard to letter case.
fn resolve(dir: &Path, path: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for comp in path.split('/') {
        cur = child_named(&cur, comp)?;
    }
    Some(cur)
}

const PROGRAM_THREE: &str = "Bryce 3D.exe";
const PROGRAM_FIVE: &str = "Bryce5.exe";
const MATHS_THREE: &str = "Axiom.dll";
const MATHS_FIVE: &str = "B5Axiom.dll";
/// The deepest a preset library may stand below the Presets folder.
const PRESET_DEPTH: usize = 4;

// ---------------------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------------------

/// A kept file to list and copy.
#[derive(Clone, Debug)]
pub struct KeptItem {
    /// The path in the substrate and in the folder beside it.
    pub entry: String,
    pub role: u8,
    pub from: PathBuf,
    pub bytes: Vec<u8>,
}

/// What a run would write, and what it reports.
#[derive(Debug, Default)]
pub struct Plan {
    pub tables: BTreeMap<TableName, (Vec<u8>, String)>,
    pub kept: Vec<KeptItem>,
    pub report: Vec<String>,
}

/// Every refusal found; nothing is written.
#[derive(Debug)]
pub struct Refused {
    pub messages: Vec<String>,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.messages.join("\n"))
    }
}

impl std::error::Error for Refused {}

fn describe(path: &Path, bytes: &[u8]) -> String {
    format!("{} ({} bytes, SHA-256 {})", path.display(), bytes.len(), to_hex(&sha256(bytes)))
}

fn table_by_name(name: &str) -> Option<TableName> {
    TableName::from_name(name.as_bytes())
}

/// The Bryce version a build belongs to ("3.0", "3.1", "5.0"), the three the add-on's folder
/// fields stand for. (Folders by version)
pub fn version_of_build(build: &str) -> &'static str {
    if build.ends_with("3.0") {
        "3.0"
    } else if build.ends_with("3.1") {
        "3.1"
    } else {
        "5.0"
    }
}

/// How far below the folder given a program file is looked for: the folder itself, a wrapper
/// folder (`Corel/Bryce 3D/`), and one more level.
const SEARCH_DEPTH: usize = 2;

fn has_program(dir: &Path) -> bool {
    file_named(dir, PROGRAM_THREE).is_some() || file_named(dir, PROGRAM_FIVE).is_some()
}

/// The install folder in or below `dir`: `dir` itself when it holds a program file, else the
/// first folder (shallowest, then by name) within SEARCH_DEPTH levels that does.
/// (Folders by version)
pub fn locate_install(dir: &Path) -> Option<PathBuf> {
    let mut level = vec![dir.to_path_buf()];
    for depth in 0..=SEARCH_DEPTH {
        if let Some(found) = level.iter().find(|d| has_program(d)) {
            return Some(found.clone());
        }
        if depth == SEARCH_DEPTH {
            break;
        }
        let mut next = Vec::new();
        for d in &level {
            let mut subs: Vec<PathBuf> = fs::read_dir(d)
                .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default();
            subs.sort();
            next.append(&mut subs);
        }
        level = next;
    }
    None
}

/// Read the folders and decide what to write.
pub fn plan(folders: &[PathBuf]) -> Result<Plan, Refused> {
    let items: Vec<(Option<String>, PathBuf)> = folders.iter().map(|f| (None, f.clone())).collect();
    plan_expected(&items)
}

/// As `plan`, each folder optionally with the Bryce version ("3.0", "3.1", "5.0") its field stands
/// for: a folder holding another version is refused, naming both.
pub fn plan_expected(items: &[(Option<String>, PathBuf)]) -> Result<Plan, Refused> {
    let folders: Vec<PathBuf> = items.iter().map(|i| i.1.clone()).collect();
    let data = known();
    let mut plan = Plan::default();
    let mut errors = Vec::new();
    let mut seen_builds: BTreeMap<String, u32> = BTreeMap::new();
    for (expect, dir) in items {
        match plan_folder(&data, dir, expect.as_deref(), &mut plan, &mut seen_builds) {
            Ok(()) => {}
            Err(mut e) => errors.append(&mut e),
        }
    }
    if errors.is_empty() && folders.is_empty() {
        errors.push("no install folder given".to_owned());
    }
    if !errors.is_empty() {
        return Err(Refused { messages: errors });
    }
    Ok(plan)
}

fn plan_folder(
    data: &Known,
    given: &Path,
    expect: Option<&str>,
    plan: &mut Plan,
    seen: &mut BTreeMap<String, u32>,
) -> Result<(), Vec<String>> {
    let name = given.display().to_string();
    // (1) a folder that exists
    if !given.is_dir() {
        return Err(vec![format!("{name}: not a folder (it does not exist or is not a folder)")]);
    }
    // The install may stand in the folder or a little below it.
    let Some(dir) = locate_install(given) else {
        return Err(vec![format!(
            "{name}: no Bryce program file ({PROGRAM_THREE} or {PROGRAM_FIVE}) found in this folder or in the {SEARCH_DEPTH} levels of folders below it"
        )]);
    };
    let dir = dir.as_path();
    let name = if dir == given { name } else { format!("{name} (install found at {})", dir.display()) };
    // (2) and (3): the program file
    let three = file_named(dir, PROGRAM_THREE);
    let five = file_named(dir, PROGRAM_FIVE);
    let (program_path, family) = match (three, five) {
        (None, None) => return Err(vec![format!("{name}: no program file ({PROGRAM_THREE} or {PROGRAM_FIVE})")]),
        (Some(a), Some(b)) => {
            return Err(vec![format!(
                "{name}: both program files, {} and {}: not one install",
                a.display(),
                b.display()
            )]);
        }
        (Some(p), None) => (p, 3u8),
        (None, Some(p)) => (p, 5u8),
    };
    let program = fs::read(&program_path).map_err(|e| vec![format!("{}: cannot be read: {e}", program_path.display())])?;
    let program_hash = to_hex(&sha256(&program));
    // (4) a known program
    let Some(build) = data.files.iter().find(|k| {
        k.role == FileRole::Program && k.family == family && k.size == program.len() as u64 && k.sha256 == program_hash
    }) else {
        return Err(vec![format!("{}: unknown build", describe(&program_path, &program))]);
    };
    let build_name = build.build.clone();
    if let Some(want) = expect {
        let got = version_of_build(&build_name);
        if got != want {
            return Err(vec![format!(
                "{name}: this is {build_name} (Bryce {got}), not Bryce {want}: put it in the Bryce {got} folder"
            )]);
        }
    }
    let mut report = vec![format!("{name}: recognised as {build_name}")];
    // (5) its maths library
    let maths_name = if family == 3 { MATHS_THREE } else { MATHS_FIVE };
    let maths_known = data.files.iter().find(|k| k.role == FileRole::Maths && k.build == build_name).expect("a row");
    let maths = match file_named(dir, maths_name) {
        None => return Err(vec![format!("{name}: {build_name}: the maths library {maths_name} is missing")]),
        Some(p) => {
            let bytes = fs::read(&p).map_err(|e| vec![format!("{}: cannot be read: {e}", p.display())])?;
            if bytes.len() as u64 != maths_known.size || to_hex(&sha256(&bytes)) != maths_known.sha256 {
                return Err(vec![format!("{}: not the maths library known for {build_name}", describe(&p, &bytes))]);
            }
            bytes
        }
    };
    // The folder's name beside the substrate.
    let count = seen.entry(build_name.clone()).or_insert(0);
    *count += 1;
    let folder_name = if *count == 1 { build_name.clone() } else { format!("{build_name} ({count})") };

    let mut errors = Vec::new();
    // Tables of the program and the maths library (6).
    for (file_name, file_bytes) in [(&program_path, &program), (&PathBuf::from(maths_name), &maths)] {
        take_tables(data, file_bytes, file_name, &build_name, plan, &mut errors, &mut report);
    }
    // The menus file.
    let data_dir = child_named(dir, "Data");
    let menus_known = data.files.iter().find(|k| k.role == FileRole::Menus && k.build == build_name);
    let menus_path = menus_known.and_then(|k| data_dir.as_ref().and_then(|d| resolve(d, file_part(k))));
    match (menus_known, menus_path.filter(|p| p.is_file())) {
        (Some(k), Some(p)) => match fs::read(&p) {
            Ok(bytes) if bytes.len() as u64 == k.size && to_hex(&sha256(&bytes)) == k.sha256 => {
                take_tables(data, &bytes, &p, &build_name, plan, &mut errors, &mut report);
            }
            Ok(bytes) => report.push(format!(
                "{}: the menus file is not the known one; the six tables of names are left out",
                describe(&p, &bytes)
            )),
            Err(e) => report.push(format!("{}: cannot be read ({e}); the tables of names are left out", p.display())),
        },
        _ => report.push("the menus file is missing; the tables of names are left out".to_owned()),
    }
    // The terrain editor's resource file: its presets table ; missing or not the known
    // one is reported and the table left out, not refused.
    let editor_known = data.files.iter().find(|k| k.role == FileRole::Editor && k.build == build_name);
    let editor_path = editor_known.and_then(|k| data_dir.as_ref().and_then(|d| resolve(d, file_part(k))));
    match (editor_known, editor_path.filter(|p| p.is_file())) {
        (Some(k), Some(p)) => match fs::read(&p) {
            Ok(bytes) if bytes.len() as u64 == k.size && to_hex(&sha256(&bytes)) == k.sha256 => {
                take_tables(data, &bytes, &p, &build_name, plan, &mut errors, &mut report);
            }
            Ok(bytes) => report.push(format!(
                "{}: the terrain editor's resource file is not the known one; its presets table is left out",
                describe(&p, &bytes)
            )),
            Err(e) => report.push(format!("{}: cannot be read ({e}); the presets table is left out", p.display())),
        },
        _ => report.push("the terrain editor's resource file is missing; its presets table is left out".to_owned()),
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Kept files.
    let mut kept = Vec::new();
    let moon_known = data.files.iter().find(|k| k.role == FileRole::Moon && k.build == build_name);
    let stars_known = data.files.iter().find(|k| k.role == FileRole::Stars && k.build == build_name);
    if let Some(k) = moon_known {
        keep_known(dir, &folder_name, k, 1, "the moon", &mut kept, &mut report);
    }
    if let Some(k) = stars_known {
        keep_known(dir, &folder_name, k, 2, "the star catalogue", &mut kept, &mut report);
    }
    let presets = child_named(dir, "Presets");
    if family == 5 {
        let leaves_dir = presets.as_ref().and_then(|p| resolve(p, "Leaves/Installed"));
        let mut n = 0u64;
        let mut total = 0u64;
        if let Some(ld) = leaves_dir.filter(|d| d.is_dir()) {
            for f in sorted_files(&ld) {
                if lower_name(&f).ends_with(".lps") {
                    if let Some(bytes) = keep_file(dir, &folder_name, &f, 3, &mut kept, &mut report) {
                        n += 1;
                        total += bytes.len() as u64;
                        if !data.leaves.contains(&to_hex(&sha256(&bytes))) {
                            report.push(format!("{}: not a known leaf file", f.display()));
                        }
                    }
                }
            }
        }
        count_report(data, &build_name, "leaf", n, total, &mut report);
    }
    let (mut n, mut total) = (0u64, 0u64);
    if let Some(p) = presets.filter(|p| p.is_dir()) {
        let mut found = Vec::new();
        collect_presets(&p, 0, &mut found);
        for f in found {
            if let Some(bytes) = keep_file(dir, &folder_name, &f, 4, &mut kept, &mut report) {
                n += 1;
                total += bytes.len() as u64;
            }
        }
    }
    count_report(data, &build_name, "library", n, total, &mut report);
    report.push(format!("{name}: {} kept files taken under {folder_name}", kept.len()));
    plan.kept.append(&mut kept);
    plan.report.append(&mut report);
    Ok(())
}

/// The part of a known file's install path below the Data folder ("dteditors.dat").
fn file_part(k: &KnownFile) -> &str {
    let p = k.install_path.as_deref().unwrap_or_default();
    p.split_once('/').map_or(p, |(_, rest)| rest)
}

fn lower_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn sorted_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_file())).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Preset libraries: `.mat` and `.brt` files in the Presets folder or at most four levels
/// below it; links are not followed.
fn collect_presets(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let Ok(t) = e.file_type() else { continue };
        let path = e.path();
        if t.is_file() {
            let l = lower_name(&path);
            if l.ends_with(".mat") || l.ends_with(".brt") {
                out.push(path);
            }
        } else if t.is_dir() && depth < PRESET_DEPTH {
            collect_presets(&path, depth + 1, out);
        }
    }
}

fn count_report(data: &Known, build: &str, role: &str, n: u64, total: u64, report: &mut Vec<String>) {
    match data.counts.get(&(build.to_owned(), role.to_owned())) {
        Some(&(files, bytes)) if (files, bytes) != (n, total) => report.push(format!(
            "{build}: {n} {role} files of {total} bytes taken; the owner's install holds {files} of {bytes} bytes"
        )),
        Some(_) => {}
        None => report.push(format!("{build}: {n} {role} files of {total} bytes taken")),
    }
}

/// Take the tables of a recognised file at its sealed rows.
fn take_tables(
    data: &Known,
    file: &[u8],
    path: &Path,
    build: &str,
    plan: &mut Plan,
    errors: &mut Vec<String>,
    report: &mut Vec<String>,
) {
    let hash = to_hex(&sha256(file));
    let mut taken = Vec::new();
    for loc in data.locations.iter().filter(|l| l.file_sha256 == hash) {
        let Some(table) = table_by_name(&loc.table) else { continue };
        let expected = data.table_len.get(&loc.table).copied().unwrap_or(0);
        match open_table(file, &loc.table, &loc.sealed, expected) {
            Ok(bytes) => {
                if let Some((have, _)) = plan.tables.get(&table) {
                    if *have != bytes {
                        errors.push(format!(
                            "{}: table {} differs from the same table of an earlier folder",
                            describe(path, file),
                            loc.table
                        ));
                    }
                } else {
                    plan.tables.insert(table, (bytes, format!("{build}: {}", path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))));
                }
                taken.push(loc.table.clone());
            }
            Err(e) => errors.push(format!("{}: table {}: {e}", describe(path, file), loc.table)),
        }
    }
    report.push(format!("{}: tables taken: {}", path.display(), taken.join(", ")));
}

/// Keep a known moon or star file: reported when it is not the known one, still kept.
fn keep_known(
    dir: &Path,
    folder: &str,
    k: &KnownFile,
    role: u8,
    what: &str,
    kept: &mut Vec<KeptItem>,
    report: &mut Vec<String>,
) {
    let Some(path) = k.install_path.as_deref().and_then(|p| resolve(dir, p)).filter(|p| p.is_file()) else {
        report.push(format!("{what}: missing, left out"));
        return;
    };
    if let Some(bytes) = keep_file(dir, folder, &path, role, kept, report) {
        if bytes.len() as u64 != k.size || to_hex(&sha256(&bytes)) != k.sha256 {
            report.push(format!("{}: {what} is not the known file", describe(&path, &bytes)));
        }
    }
}

/// Read one kept file and list it under `folder`; `None` (reported) when it cannot be kept.
fn keep_file(
    install: &Path,
    folder: &str,
    path: &Path,
    role: u8,
    kept: &mut Vec<KeptItem>,
    report: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let rel = path.strip_prefix(install).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c.as_os_str().to_str() {
            Some(s) => parts.push(s.to_owned()),
            None => {
                report.push(format!("{}: a name that is not text, left out", path.display()));
                return None;
            }
        }
    }
    let entry = format!("{folder}/{}", parts.join("/"));
    if entry.len() > MAX_ENTRY_NAME || substrate::check_safe_path(entry.as_bytes()).is_err() {
        report.push(format!("{}: cannot be given a safe entry name, left out", path.display()));
        return None;
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            report.push(format!("{}: cannot be read ({e}), left out", path.display()));
            return None;
        }
    };
    if bytes.len() > u32::MAX as usize {
        report.push(format!("{}: too long, left out", path.display()));
        return None;
    }
    kept.push(KeptItem { entry, role, from: path.to_path_buf(), bytes: bytes.clone() });
    Some(bytes)
}

// ---------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------

/// The bytes of the substrate file (format version 4) for a plan.
pub fn substrate_bytes(plan: &Plan) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(MAGIC);
    f.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    f.extend_from_slice(&((plan.tables.len() + plan.kept.len()) as u32).to_le_bytes());
    let mut entry = |kind: u8, role: u8, name: &[u8], source: &str, hash: Digest, len: usize, body: Option<&[u8]>| {
        f.push(kind);
        f.push(role);
        f.push(name.len() as u8);
        f.extend_from_slice(name);
        let source = &source.as_bytes()[..source.len().min(u16::MAX as usize)];
        f.extend_from_slice(&(source.len() as u16).to_le_bytes());
        f.extend_from_slice(source);
        f.extend_from_slice(&hash);
        f.extend_from_slice(&(len as u32).to_le_bytes());
        if let Some(b) = body {
            f.extend_from_slice(b);
        }
    };
    for (name, (bytes, source)) in &plan.tables {
        entry(0, 0, name.name().as_bytes(), source, sha256(bytes), bytes.len(), Some(bytes));
    }
    for k in &plan.kept {
        entry(1, k.role, k.entry.as_bytes(), &k.from.display().to_string(), sha256(&k.bytes), k.bytes.len(), None);
    }
    let h = sha256(&f);
    f.extend_from_slice(&h);
    f
}

/// Make the substrate file at `out` and the folder beside it from the install folders.
/// Nothing is written when any folder is refused.
pub fn extract(folders: &[PathBuf], out: &Path) -> Result<Vec<String>, Refused> {
    let items: Vec<(Option<String>, PathBuf)> = folders.iter().map(|f| (None, f.clone())).collect();
    extract_expected(&items, out)
}

/// As `extract`, each folder optionally with the Bryce version it must hold.
pub fn extract_expected(items: &[(Option<String>, PathBuf)], out: &Path) -> Result<Vec<String>, Refused> {
    let plan = plan_expected(items)?;
    let bytes = substrate_bytes(&plan);
    // The file must be one the reader accepts ; the tables carry their constraints.
    if let Err(e) = Substrate::parse(&bytes) {
        return Err(Refused { messages: vec![format!("the substrate made is not accepted by its reader: {e}")] });
    }
    let beside = substrate::beside_folder_of(out);
    let io = |what: &Path, e: std::io::Error| Refused { messages: vec![format!("{}: cannot be written: {e}", what.display())] };
    for k in &plan.kept {
        let mut dest = beside.clone();
        dest.extend(k.entry.split('/'));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        fs::write(&dest, &k.bytes).map_err(|e| io(&dest, e))?;
    }
    fs::write(out, &bytes).map_err(|e| io(out, e))?;
    Ok(plan.report)
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_install_is_found_below_a_wrapper_folder_in_any_case() {
        let root = std::env::temp_dir().join(format!("stecyk3d-locate-{}", std::process::id()));
        let inner = root.join("Corel").join("Bryce 3D");
        fs::create_dir_all(&inner).unwrap();
        assert_eq!(locate_install(&root), None);
        fs::write(inner.join("BRYCE 3D.EXE"), b"x").unwrap();
        assert_eq!(locate_install(&root), Some(inner.clone()));
        assert_eq!(locate_install(&inner), Some(inner.clone()));
        // three levels down is too far
        let deep = root.join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Bryce5.exe"), b"x").unwrap();
        assert_eq!(locate_install(&root.join("a")), None);
        // a folder with none says so, saying it looked below
        let refused = plan_expected(&[(Some("3.0".into()), root.join("a"))]).unwrap_err();
        assert!(refused.messages[0].contains("no Bryce program file"), "{refused}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn builds_belong_to_their_versions() {
        assert_eq!(version_of_build("Bryce 3D 3.0"), "3.0");
        assert_eq!(version_of_build("Bryce 3D 3.1"), "3.1");
        assert_eq!(version_of_build("Corel Bryce 5.0"), "5.0");
        assert_eq!(version_of_build("DAZ Bryce 5 (2006)"), "5.0");
    }

    use super::*;

    #[test]
    fn key_and_keystream_of_the_example() {
        let key = file_key(b"an example file.");
        assert_eq!(to_hex(&key), "99508fb263630a1bbabfbdd3ca01ec4d11b598a7210657bef380a177d50b226c");
        assert_eq!(
            to_hex(&keystream_block(&key, "example", 0)),
            "81550bdfcceb8aad12a8444c9e70f2407d62aed349c1ea5a26636aeb08d57ad2"
        );
        assert_eq!(
            to_hex(&keystream_block(&key, "example", 1)),
            "7d8ecab16a3ecfb535e2d4cb11e9bd934184d6f88aa4d3be7b20a5d9ef97b1f7"
        );
    }

    /// Seal a row of our own with the same rule, then open it.
    fn seal(file: &[u8], table: &str, row: &[u8]) -> Vec<String> {
        let key = file_key(file);
        let mut sealed = row.to_vec();
        for (n, chunk) in sealed.chunks_mut(32).enumerate() {
            let block = keystream_block(&key, table, n as u32);
            for (b, k) in chunk.iter_mut().zip(block) {
                *b ^= k;
            }
        }
        sealed.chunks(32).map(to_hex_bytes).collect()
    }

    fn to_hex_bytes(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn row(table: &[u8], pieces: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut r = sha256(table).to_vec();
        for &(o, l, t) in pieces {
            for w in [o, l, t] {
                r.extend_from_slice(&w.to_le_bytes());
            }
        }
        r
    }

    #[test]
    fn a_sealed_row_opens_and_joins() {
        let file = b"0123456789abcdef";
        let want = b"897012389".to_vec(); // "89", "7", "0123", "89": two pieces name the same bytes
        let sealed = seal(file, "t", &row(&want, &[(8, 2, 1), (7, 1, 1), (0, 4, 1), (8, 2, 1)]));
        assert_eq!(open_table(file, "t", &sealed, 9), Ok(want));
        let rep = b"898989".to_vec(); // one piece, three times over
        let sealed = seal(file, "t", &row(&rep, &[(8, 2, 3)]));
        assert_eq!(open_table(file, "t", &sealed, 6), Ok(rep));
    }

    #[test]
    fn a_damaged_row_is_refused() {
        let file = b"0123456789abcdef";
        let want = b"0123".to_vec();
        let good = row(&want, &[(0, 4, 1)]);
        let sealed = seal(file, "t", &good);
        assert_eq!(open_table(file, "t", &sealed, 4), Ok(want.clone()));
        assert_eq!(open_table(file, "t", &sealed, 5), Err(RowError::WrongLength { found: 4, expected: 5 }));
        assert!(open_table(file, "u", &sealed, 4).is_err(), "another table's name");
        let moved = seal(file, "t", &row(&want, &[(1, 4, 1)]));
        assert_eq!(open_table(file, "t", &moved, 4), Err(RowError::WrongHash));
        let past = seal(file, "t", &row(&want, &[(14, 4, 1)]));
        assert!(matches!(open_table(file, "t", &past, 4), Err(RowError::PieceOutside { .. })));
        let short = seal(file, "t", &good[..40]);
        assert_eq!(open_table(file, "t", &short, 4), Err(RowError::BadShape(40)));
        assert_eq!(open_table(b"0123456789abcdeg", "t", &sealed, 4).is_err(), true, "another file");
    }

    #[test]
    fn the_data_is_whole() {
        let d = known();
        assert_eq!(d.files.len(), 22);
        assert_eq!(d.table_len.len(), 22);
        assert_eq!(d.locations.len(), 64);
        let editor_rows = d.locations.iter().filter(|l| l.table.starts_with("terrain-")).count();
        assert_eq!(editor_rows, 6);
        assert!(d.table_len.iter().filter(|(k, _)| k.starts_with("terrain-filter-presets-")).all(|(_, &n)| n == 1152));
        assert_eq!(d.leaves.len(), 61);
        let menus_rows = d
            .locations
            .iter()
            .filter(|l| {
                matches!(
                    l.table.as_str(),
                    "noise-names" | "noise-names-five" | "method-names" | "blend-names" | "filter-names" | "color-mode-names"
                )
            })
            .count();
        assert_eq!(menus_rows, 22);
        for l in &d.locations {
            assert!(d.table_len.contains_key(&l.table), "{}", l.table);
            assert!(table_by_name(&l.table).is_some());
        }
    }
}
