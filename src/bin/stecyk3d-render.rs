// SPDX-License-Identifier: MIT
//! The render program: a scene file in, a picture out.
//!
//!     stecyk3d-render --substrate S --document F [--as 3.0|3.1|5.0] --picture OUT [--levels L]
//!         [--mode mask|distance|altitude]
//!     stecyk3d-render --substrate S --description D.json --levels L [--picture OUT] [--stream] [--scattered|--rows]
//!
//! `--stream` sends finished pixels to standard output as they are drawn (task T2, the live
//! picture): runs of `VSTA`, x, y, n as binary32-le words, then n pixels of RGB, the levels'
//! SHA-256 going to standard error. `--scattered` takes the pixels in a fixed random
//! permutation (the live picture).
//!
//!     stecyk3d-render --substrate S --serve
//!
//! `--serve` stays running: one JSON request per line on standard input, a framed answer on standard output
//! (see `serve` below;, The long-lived renderer). A newer request stops the
//! render going on. The one-shot forms above are unchanged.
//!
//! `--threads N` renders the first pass and the pass over edges on N threads (0 or absent: every
//! core; 1: the serial render); the picture is the same bytes .
//! A `--stream` or `--scattered` render uses every core too (`--threads`); a closed output pipe ends the process at once.
//!
//! `--mode` renders one of's render modes in place of the shaded picture .
//!
//! `--time T` renders the document as it stands at T seconds . `--frames FIRST-LAST`
//! or `--frames all` renders a range of frames one after another, each to the PNG file
//! NAME-NNNN.png , NAME given by `--series` or the document's own name without its
//! extension .
//!
//!     stecyk3d-render read (library FILE | library --substrate S --kept PATH |
//!         kept-libraries --substrate S --as V | document FILE [--as V] | relief SEED)
//!
//! writes what the Blender add-on reads as JSON (
//!
//! The first form renders a saved document; its levels' SHA-256 is what the case command's
//! `levels=` says for the same document and version (`cases::picture`'s path:
//! `Scene::read` and `render_lit` with the substrate's tables).
//!
//! The second form is the add-on's protocol: the description JSON the Blender add-on
//! writes, with `size` added, renders to `L` as width x height x 3 bytes, top row first.
//!
//! (The protocol with that future program),

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use stecyk3d::cases::picture;
use stecyk3d::description;
use stecyk3d::render::OpenChoices;
use stecyk3d::render::frames::{FrameSeries, frame_file_name, render_frame_lit, whole_animation};
use stecyk3d::render::stages::Picture;
use stecyk3d::render::{RenderMode, render_lit_streamed, render_mode};
use stecyk3d::scene::Scene;
use stecyk3d::scene_export;
use stecyk3d::scene_file::Flavour;
use stecyk3d::scene_library::{Library, LibraryKind};
use stecyk3d::substrate;
use stecyk3d::version::RenderAs;

struct Args {
    /// The option's file; none: BRYCE_SUBSTRATE, then the default place .
    substrate: Option<PathBuf>,
    document: Option<PathBuf>,
    description: Option<PathBuf>,
    version: Option<RenderAs>,
    picture: Option<PathBuf>,
    levels: Option<PathBuf>,
    /// Send finished pixels to standard output as they are drawn (task T2, the live
    /// picture): records of `MAGIC, x, y, n` (four binary32-le words) then n pixels of RGB,
    /// top row first. The levels' SHA-256 goes to standard error instead of the usual line.
    stream: bool,
    /// Take the pixels in a fixed random permutation (the add-on's "emerge from noise"
    /// default;, the live picture).
    scattered: bool,
    /// `--rows`: with `--scattered`, the pixels row by row from the top rather than shuffled.
    rows: bool,
    /// `--threads N`: worker threads of a document or description render; 0 or absent: every
    /// core, 1: the serial render. 
    threads: usize,
    /// A render mode of for a document render; `None`: the
    /// shaded picture. (Render modes)
    mode: Option<RenderMode>,
    /// A frame at this time in seconds in place of the document as it opens .
    time: Option<f64>,
    /// A range of frames : the first and the last frame numbers, or the whole
    /// animation (`None` inside). Written as's files.
    frames: Option<Option<(u64, u64)>>,
    /// The name and place of a range's files ; by default the document's own name
    /// without its extension, in its folder.
    series: Option<PathBuf>,
    /// `--serve`: the long-lived form.
    serve: bool,
}

/// The seed of the fixed random permutation (scattered order).
const SCATTERED_SEED: u32 = 1;
/// The four bytes that open a stream record.
const RECORD_MAGIC: &[u8; 4] = b"VSTA";

/// The value that follows a flag; advances `i` onto the value itself.
fn take(args: &[String], i: &mut usize, what: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| format!("{what} needs a value"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        substrate: None,
        document: None,
        description: None,
        version: None,
        picture: None,
        levels: None,
        stream: false,
        scattered: false,
        rows: false,
        threads: 0,
        mode: None,
        time: None,
        frames: None,
        series: None,
        serve: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--substrate" => a.substrate = Some(PathBuf::from(take(args, &mut i, "--substrate")?)),
            "--document" => a.document = Some(PathBuf::from(take(args, &mut i, "--document")?)),
            "--description" => a.description = Some(PathBuf::from(take(args, &mut i, "--description")?)),
            "--picture" => a.picture = Some(PathBuf::from(take(args, &mut i, "--picture")?)),
            "--levels" => a.levels = Some(PathBuf::from(take(args, &mut i, "--levels")?)),
            "--time" => {
                let w = take(args, &mut i, "--time")?;
                a.time = Some(w.parse().map_err(|_| format!("--time {w:?} is not a number of seconds"))?);
            }
            "--frames" => {
                let w = take(args, &mut i, "--frames")?;
                a.frames = Some(if w == "all" {
                    None
                } else {
                    let (f, l) = w.split_once('-').ok_or_else(|| format!("--frames {w:?}: give all or FIRST-LAST"))?;
                    let n = |s: &str| s.parse::<u64>().map_err(|_| format!("--frames {w:?}: {s:?} is not a whole number from 0"));
                    Some((n(f)?, n(l)?))
                });
            }
            "--series" => a.series = Some(PathBuf::from(take(args, &mut i, "--series")?)),
            "--stream" => a.stream = true,
            "--serve" => a.serve = true,
            "--scattered" => a.scattered = true,
            "--rows" => {
                a.scattered = true;
                a.rows = true;
            }
            "--exact-random" => {
                let _ = DRAWS_FLAG.set("exact".to_owned());
            }
            "--independent-random" => {
                let _ = DRAWS_FLAG.set("independent".to_owned());
            }
            "--threads" => {
                let w = take(args, &mut i, "--threads")?;
                a.threads = w.parse().map_err(|_| format!("--threads needs a number, not {w}"))?;
            }
            "--mode" => {
                let w = take(args, &mut i, "--mode")?;
                // Several modes, comma-separated, are resolved as says (mask, altitude, distance).
                let parsed: Option<Vec<RenderMode>> = w.split(',').map(RenderMode::parse).collect();
                let on = parsed.ok_or_else(|| format!("--mode {w:?} is not one of mask, distance and altitude"))?;
                a.mode = RenderMode::choose(&on);
            }
            "--as" => {
                a.version = Some(match take(args, &mut i, "--as")?.as_str() {
                    "3.0" => RenderAs::Bryce30,
                    "3.1" => RenderAs::Bryce31,
                    "5.0" => RenderAs::Bryce50,
                    other => return Err(format!("--as {other:?} is not one of 3.0, 3.1 and 5.0")),
                });
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        i += 1;
    }
    if a.serve {
        return if a.document.is_some() || a.description.is_some() || a.stream || a.scattered {
            Err("--serve takes requests on standard input, with no document, description or stream flag".to_owned())
        } else {
            Ok(a)
        };
    }
    match (&a.document, &a.description) {
        (Some(_), None) => {}
        (None, Some(_)) => {
            if a.levels.is_none() && a.picture.is_none() {
                return Err("a description render needs --levels or --picture".to_owned());
            }
        }
        (Some(_), Some(_)) => return Err("give --document or --description, not both".to_owned()),
        (None, None) => return Err("give --document or --description".to_owned()),
    }
    if a.mode.is_some() && (a.description.is_some() || a.stream || a.scattered) {
        return Err("--mode is for a document render, without --stream or --scattered".to_owned());
    }
    let framed = a.time.is_some() || a.frames.is_some();
    if framed && (a.document.is_none() || a.mode.is_some() || a.stream || a.scattered) {
        return Err("--time and --frames are for a document's shaded picture, without --stream or --scattered".to_owned());
    }
    if a.time.is_some() && a.frames.is_some() {
        return Err("give --time or --frames, not both".to_owned());
    }
    if a.document.is_some() && a.version.is_none() {
        return Err("a document render needs --as (the version to render it as)".to_owned());
    }
    Ok(a)
}

fn write_picture(p: &Picture, args: &Args) -> Result<(), String> {
    if let Some(path) = &args.levels {
        std::fs::write(path, &p.levels).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    if let Some(path) = &args.picture {
        std::fs::write(path, p.png()).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// The `read` form: what the add-on reads, as JSON on standard output; a refusal's reason on
/// standard error with exit status 1. No substrate is needed but for the extractor's libraries.
fn read_form(args: &[String]) -> ExitCode {
    match read_json(args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn library_kind(path: &str) -> Result<LibraryKind, String> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".mat") {
        Ok(LibraryKind::Materials)
    } else if lower.ends_with(".brt") {
        Ok(LibraryKind::Textures)
    } else {
        Err(format!("{path}: not a library (its name ends neither in .mat nor in .brt)"))
    }
}

fn read_json(args: &[String]) -> Result<String, String> {
    let usage = "usage: stecyk3d-render read (library FILE | library --substrate S --kept PATH | \
                 kept-libraries --substrate S --as V | document FILE [--as V] | relief SEED)";
    let what = args.first().ok_or(usage)?;
    let rest = &args[1..];
    let flag = |name: &str| -> Option<String> {
        rest.iter().position(|a| a == name).and_then(|k| rest.get(k + 1)).cloned()
    };
    let load = |path: &str| substrate::load(Path::new(path)).map_err(|e| format!("the substrate cannot be read: {e}"));
    match what.as_str() {
        "library" => {
            let (label, bytes) = match (flag("--substrate"), flag("--kept")) {
                (Some(s), Some(kept)) => {
                    let sub = load(&s)?;
                    let bytes = sub.read_kept(&kept).map_err(|e| format!("{kept}: {e}"))?;
                    (kept, bytes)
                }
                _ => {
                    let file = rest.first().ok_or(usage)?;
                    let bytes = std::fs::read(file).map_err(|e| format!("{file}: cannot be read: {e}"))?;
                    (file.clone(), bytes)
                }
            };
            let kind = library_kind(&label)?;
            let lib = Library::read(&bytes, kind).map_err(|e| format!("{label}: {e}"))?;
            Ok(scene_export::library_json(&lib).write())
        }
        "kept-libraries" => {
            let sub = load(&flag("--substrate").ok_or(usage)?)?;
            let version = match flag("--as").as_deref() {
                Some("3.0") => RenderAs::Bryce30,
                Some("3.1") => RenderAs::Bryce31,
                Some("5.0") => RenderAs::Bryce50,
                _ => return Err(format!("--as must be one of 3.0, 3.1 and 5.0; {usage}")),
            };
            Ok(scene_export::kept_libraries_json(&sub, version).write())
        }
        "document" => {
            let file = rest.first().ok_or(usage)?;
            let path = Path::new(file);
            let bytes = std::fs::read(path).map_err(|e| format!("{file}: cannot be read: {e}"))?;
            let flavour = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("br5")) {
                Flavour::Bryce5
            } else {
                Flavour::Bryce3d
            };
            let scene = Scene::read(&bytes, flavour).map_err(|e| format!("{file}: {e}"))?;
            let version = match flag("--as").as_deref() {
                None => None,
                Some("3.0") => Some(RenderAs::Bryce30),
                Some("3.1") => Some(RenderAs::Bryce31),
                Some("5.0") => Some(RenderAs::Bryce50),
                Some(other) => return Err(format!("--as must be one of 3.0, 3.1 and 5.0, not {other}")),
            };
            Ok(scene_export::document_json_as(&scene, version).write())
        }
        "relief" => {
            let seed = rest.first().ok_or(usage)?;
            let seed: u32 = seed.parse().map_err(|_| format!("the seed {seed:?} is not a whole number from 0 to 4294967295"))?;
            Ok(scene_export::grid_json(&scene_export::relief(seed)).write())
        }
        _ => Err(usage.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "read") {
        return read_form(&args[1..]);
    }
    let args = match parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("usage: stecyk3d-render [--substrate S] (--document F --as V --picture OUT | --description D --levels L) [--levels L]");
            eprintln!("stecyk3d-render: {e}");
            return ExitCode::from(2);
        }
    };
    // : the file named on the command line, else the one BRYCE_SUBSTRATE names, else
    // substrate.bin at the default place
    let substrate = match substrate::find_and_load(args.substrate.as_deref(), substrate::default_folder().as_deref()) {
        Ok(s) => s,
        Err(e) => {
            // : no render without its tables.
            eprintln!("stecyk3d-render: the substrate cannot be read: {e}");
            return ExitCode::from(1);
        }
    };

    if args.serve {
        return serve(&substrate);
    }
    use std::io::Write;
    // The stream's records go out through a wide buffer: the add-on reads the pipe in short
    // steps, and pixels reach it as each batch fills the buffer.
    let mut stream = args.stream.then(|| std::io::BufWriter::with_capacity(1 << 16, std::io::stdout().lock()));
    let mut record = |x: u32, y: u32, levels: &[u8]| {
        if let Some(out) = &mut stream {
            // An empty run is the renderer's "a part is complete": send what is buffered now.
            // A write that fails is a viewport that closed the pipe (a cancel): stop at once, the
            // threads with the process, with nothing left to say.
            let sent = if levels.is_empty() {
                out.flush()
            } else {
                let n = (levels.len() / 3) as u32;
                out.write_all(RECORD_MAGIC)
                    .and_then(|()| {
                        out.write_all(&x.to_le_bytes())?;
                        out.write_all(&y.to_le_bytes())?;
                        out.write_all(&n.to_le_bytes())?;
                        out.write_all(levels)
                    })
            };
            if sent.is_err() {
                std::process::exit(0);
            }
        }
    };
    if let (Some(document), Some(range)) = (&args.document, args.frames) {
        return match render_range(document, args.version.unwrap(), &substrate, range, args.series.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("stecyk3d-render: {e}");
                ExitCode::from(1)
            }
        };
    }
    let threads = args.threads;
    let result = match (&args.document, &args.description) {
        (Some(document), None) if args.time.is_some() => {
            render_at_time(document, args.version.unwrap(), &substrate, args.time.unwrap())
        }
        (Some(document), None) => {
            render_document(document, args.version.unwrap(), &substrate, args.scattered, args.rows, args.mode, threads, &mut record)
        }
        (None, Some(path)) => render_description(path, &substrate, args.scattered, args.rows, threads, &mut record),
        _ => unreachable!("parse_args checked the pair"),
    };
    if let Some(out) = &mut stream {
        let _ = out.flush();
    }
    match result {
        Ok(picture) => {
            let sha = picture.levels_sha256_hex();
            if args.stream {
                eprintln!("levels {sha}"); // stdout holds only the records
            } else {
                println!("levels {sha}");
            }
            if let Err(e) = write_picture(&picture, &args) {
                eprintln!("stecyk3d-render: {e}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("stecyk3d-render: {e}");
            ExitCode::from(1)
        }
    }
}

fn read_document(path: &Path, version: RenderAs, substrate: &substrate::Substrate) -> Result<Scene, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let flavour = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("br5")) {
        Flavour::Bryce5
    } else {
        Flavour::Bryce3d
    };
    let scene = Scene::read(&bytes, flavour).map_err(|e| format!("{}: {e}", path.display()))?;
    // (no render without the tables and kept files it uses, refused before any
    // pixel is written; the tables and kept files are named in one message)
    substrate.check_render_needs(&picture::render_needs(&scene, version, path)).map_err(|e| e.to_string())?;
    Ok(scene)
}

/// A frame at a time, as the first render of a program just started.
fn render_at_time(path: &Path, version: RenderAs, substrate: &substrate::Substrate, t: f64) -> Result<Picture, String> {
    let scene = read_document(path, version, substrate)?;
    let (picture, _counts) =
        render_frame_lit(&scene, version, t, &picture::choices_from(substrate)).map_err(|e| e.to_string())?;
    Ok(picture)
}

/// A range of frames, one after another, each written to its own PNG file as it is done; a
/// file that cannot be written ends the run with failure after the files already written.
fn render_range(
    path: &Path,
    version: RenderAs,
    substrate: &substrate::Substrate,
    range: Option<(u64, u64)>,
    series: Option<&Path>,
) -> Result<(), String> {
    let scene = read_document(path, version, substrate)?;
    // : the whole animation is a movie's range (its frame numbers, the
    // first at its start time, and as 5.0 the whole picture); a range of frame numbers given
    // is this interface's own (frames first to last at k / f; a first above the last renders
    // nothing, the implementer's choice).
    let (frames, mut run): (Vec<(u64, Option<f64>)>, FrameSeries) = match range {
        Some((first, last)) => {
            ((first..=last).map(|k| (k, None)).collect(), FrameSeries::new(&scene, version, picture::choices_from(substrate)))
        }
        None => {
            let movie = whole_animation(&scene, version).map_err(|e| e.to_string())?;
            (
                movie.frames().map(|(k, t)| (k, Some(t))).collect(),
                FrameSeries::movie(&scene, version, picture::choices_from(substrate)),
            )
        }
    };
    let base = match series {
        Some(s) => s.to_path_buf(),
        None => path.with_extension(""),
    };
    let name = base.to_string_lossy().into_owned();
    for (k, time) in frames {
        let done = match time {
            Some(t) => run.frame(t),
            None => run.frame_number(k),
        };
        let (p, _) = done.map_err(|e| format!("frame {k}: {e}"))?;
        let file = frame_file_name(&name, k);
        std::fs::write(&file, p.png()).map_err(|e| format!("cannot write {file}: {e}"))?;
        println!("frame {k} levels {} {file}", p.levels_sha256_hex());
    }
    Ok(())
}

/// The draws a description render may make in the program's order before it starts over with places of its own.
const AUTO_DRAWS: u64 = 100_000_000;

/// `--exact-random` / `--independent-random`, or the environment's `STECYK3D_DRAWS` (`exact`, `independent`
/// or a number of draws): how a frame whose shading draws from the generator is shared out
/// over cores. The default for a document is the program's order; for a description (the add-on's
/// renders) the program's order while the frame has made fewer than `AUTO_DRAWS` draws, and above that
/// a place of its own in the generator's sequence for every pixel, as the old recomp did. A count of
/// draws, never a time: the same description gives the same picture on any machine and under any load.
static DRAWS_FLAG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn shared_draws(description: bool, asked: Option<&str>) -> stecyk3d::render::SharedDraws {
    use stecyk3d::render::SharedDraws;
    let setting = asked.map(str::to_owned).or_else(|| DRAWS_FLAG.get().cloned()).or_else(|| std::env::var("STECYK3D_DRAWS").ok());
    match setting.as_deref() {
        Some("exact") => SharedDraws::Exact,
        Some("independent") => SharedDraws::Independent,
        Some(ms) if ms.parse::<u64>().is_ok() => SharedDraws::Auto(ms.parse().unwrap_or(AUTO_DRAWS)),
        _ if description => SharedDraws::Auto(AUTO_DRAWS),
        _ => SharedDraws::Exact,
    }
}

fn render_document(
    path: &Path,
    version: RenderAs,
    substrate: &substrate::Substrate,
    scattered: bool,
    rows: bool,
    mode: Option<RenderMode>,
    threads: usize,
    emit: &mut dyn FnMut(u32, u32, &[u8]),
) -> Result<Picture, String> {
    let scene = read_document(path, version, substrate)?;
    let choices = OpenChoices {
        scattered: scattered.then_some(SCATTERED_SEED),
        scattered_rows: rows,
        threads: Some(threads),
        shared_draws: shared_draws(false, None),
        ..picture::choices_from(substrate)
    };
    if let Some(mode) = mode {
        let (picture, _counts) = render_mode(&scene, version, &choices, mode).map_err(|e| e.to_string())?;
        return Ok(picture);
    }
    let (picture, _counts) = render_lit_streamed(&scene, version, &choices, emit).map_err(|e| e.to_string())?;
    Ok(picture)
}

fn render_description(
    path: &Path,
    substrate: &substrate::Substrate,
    scattered: bool,
    rows: bool,
    threads: usize,
    emit: &mut dyn FnMut(u32, u32, &[u8]),
) -> Result<Picture, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // : a tree of the description is grown with the substrate's tables
    let described = description::describe_with(&text, &description::Tables::of(substrate))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let choices = described.choices(OpenChoices {
        scattered: scattered.then_some(SCATTERED_SEED),
        scattered_rows: rows,
        threads: Some(threads),
        shared_draws: shared_draws(true, None),
        ..picture::choices_from(substrate)
    });
    let (picture, _counts) =
        render_lit_streamed(&described.scene, described.version, &choices, emit).map_err(|e| e.to_string())?;
    Ok(picture)
}

// ---------------------------------------------------------------------------------------------------------------
// --serve: the long-lived renderer. The viewport sends a request for each rung of its ladder and each view change;
// a newer request stops the render going on, so a child need not be started (and the description written whole) for
// each. (The long-lived renderer)
//
// Requests: one JSON object a line on standard input:
//   {"id": 7, "description": "/dir/scene.json", "levels": "/dir/out.rgb", "order": "rows"|"scattered"|"plain",
//    "size": [w, h], "camera": {...}, "render": {...}, "threads": 0}
// or `{"cancel": true}`: stop the render going on, or a request sent before it and not yet begun, and start nothing
// (that request gets its VEND, status 2; the line itself is not answered).
// `description` is the whole description file as the one-shot form reads it; `size`, `camera` and `render`, where
// given, replace those members (the file is read and parsed again only when it changes, so a view change sends the
// camera alone). `id` is a whole number of the asker's.
//
// Answers on standard output, binary, in this order for each request:
//   "VRDY" u32 1                       once, at the start
//   "VBEG" u32 id                      the request's render begins
//   the stream's records ("VSTA", x, y, n and n RGB pixels), as `--stream` writes them
//   "VEND" u32 id, u32 status, u32 n, n bytes      status 0 finished (the text is the levels' SHA-256; the levels
//        file is written), 1 failed (the text is the reason), 2 cancelled (a newer request came; nothing written)
// Every request gets exactly one VBEG and one VEND. Standard input closing ends the process with 0.

enum Message {
    Request(Box<Request>),
    /// `{"cancel": true}`: stop the render going on, start nothing (the viewport's view changed; its next request
    /// comes when the view has been still). The render stopped still gets its VEND, status 2; the line has no answer.
    Cancel,
    Bad(String),
    Quit,
}

struct Request {
    id: u32,
    description: PathBuf,
    levels: PathBuf,
    order: String,
    size: Option<stecyk3d::cases::json::Value>,
    camera: Option<stecyk3d::cases::json::Value>,
    render: Option<stecyk3d::cases::json::Value>,
    threads: usize,
    /// `draws` of a request: `exact`, `independent` or (absent, or `auto`) the renderer's own rule.
    draws: Option<String>,
}

fn parse_request(line: &str) -> Result<Request, String> {
    use stecyk3d::cases::json::{Value, parse};
    let v = parse(line).map_err(|e| format!("the request is not JSON: {e}"))?;
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| match v.get(key) {
        Some(Value::Number(n)) => n.parse::<u32>().ok(),
        _ => None,
    };
    Ok(Request {
        id: number("id").ok_or("the request has no whole-number id")?,
        description: PathBuf::from(text("description").ok_or("the request has no description")?),
        levels: PathBuf::from(text("levels").ok_or("the request has no levels")?),
        order: text("order").unwrap_or_else(|| "plain".to_owned()),
        size: v.get("size").cloned(),
        camera: v.get("camera").cloned(),
        render: v.get("render").cloned(),
        threads: number("threads").unwrap_or(0) as usize,
        draws: text("draws"),
    })
}

fn is_cancel(line: &str) -> bool {
    use stecyk3d::cases::json::{Value, parse};
    line.contains("\"cancel\"") && matches!(parse(line).ok().and_then(|v| v.get("cancel").cloned()), Some(Value::Bool(true)))
}

fn put_end(out: &mut impl std::io::Write, id: u32, status: u32, text: &str) -> std::io::Result<()> {
    out.write_all(b"VEND")?;
    out.write_all(&id.to_le_bytes())?;
    out.write_all(&status.to_le_bytes())?;
    out.write_all(&(text.len() as u32).to_le_bytes())?;
    out.write_all(text.as_bytes())?;
    out.flush()
}

/// The requests' channel, counting what the worker takes (see `serve`).
struct Inbox {
    rx: std::sync::mpsc::Receiver<Message>,
    counts: std::sync::Arc<std::sync::Mutex<(u64, u64)>>,
}

impl Inbox {
    fn taken(&self, got: Option<Message>) -> Option<Message> {
        if got.is_some() {
            self.counts.lock().expect("the counts").1 += 1;
        }
        got
    }
    fn recv(&self) -> Result<Message, ()> {
        self.taken(self.rx.recv().ok()).ok_or(())
    }
    fn try_recv(&self) -> Result<Message, ()> {
        self.taken(self.rx.try_recv().ok()).ok_or(())
    }
    /// Ends the stop once every message sent has been taken.
    fn clear_if_none_waits(&self) {
        let c = self.counts.lock().expect("the counts");
        if c.1 >= c.0 {
            stecyk3d::render::parallel::set_cancel(false);
        }
    }
}

fn serve(substrate: &substrate::Substrate) -> ExitCode {
    use std::io::{BufRead, Write};
    use std::sync::mpsc;
    use stecyk3d::cases::json::Value;
    use stecyk3d::render::parallel::{set_cancel, set_serving};
    set_serving(true);
    let (tx, rx) = mpsc::channel::<Message>();
    // Messages sent and taken, under one lock with the stop flag: the flag is set exactly when a message waits that the
    // worker has not taken, and cleared only when none does. (Set after the send without this, a message taken the
    // moment it was sent had the stop set on its own render, and nothing newer to follow it.)
    let counts = std::sync::Arc::new(std::sync::Mutex::new((0u64, 0u64)));
    let sent = counts.clone();
    std::thread::spawn(move || {
        let send = |message: Message| {
            let mut c = sent.lock().expect("the counts");
            c.0 += 1;
            let _ = tx.send(message);
            if c.1 < c.0 {
                set_cancel(true);
            }
        };
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            send(if is_cancel(&line) {
                Message::Cancel
            } else {
                match parse_request(&line) {
                    Ok(r) => Message::Request(Box::new(r)),
                    Err(e) => Message::Bad(e),
                }
            });
        }
        send(Message::Quit);
    });
    let rx = Inbox { rx, counts };
    let mut out = std::io::BufWriter::with_capacity(1 << 16, std::io::stdout().lock());
    let ok = out.write_all(b"VRDY").and_then(|()| out.write_all(&1u32.to_le_bytes())).and_then(|()| out.flush());
    if ok.is_err() {
        return ExitCode::SUCCESS;
    }
    let mut cached: Option<((PathBuf, u64, std::time::SystemTime), Value)> = None;
    let mut next: Option<Message> = rx.recv().ok();
    'serving: loop {
        // The newest request waiting; the ones it passed are cancelled without a start.
        let mut request = match next.take() {
            Some(Message::Request(r)) => r,
            Some(Message::Bad(e)) => {
                let sent = put_end(&mut out, 0, 1, &e);
                if sent.is_err() {
                    return ExitCode::SUCCESS;
                }
                next = rx.recv().ok();
                continue;
            }
            Some(Message::Cancel) => {
                next = rx.recv().ok();
                continue;
            }
            Some(Message::Quit) | None => return ExitCode::SUCCESS,
        };
        loop {
            while let Ok(newer) = rx.try_recv() {
                match newer {
                    Message::Request(r) => {
                        let old = std::mem::replace(&mut request, r);
                        let _ = out.write_all(b"VBEG").and_then(|()| out.write_all(&old.id.to_le_bytes()));
                        let _ = put_end(&mut out, old.id, 2, "");
                    }
                    Message::Bad(e) => {
                        let _ = put_end(&mut out, 0, 1, &e);
                    }
                    Message::Cancel => {
                        // a cancel sent after the request held here, before it began: it ends unrendered too
                        let _ = out.write_all(b"VBEG").and_then(|()| out.write_all(&request.id.to_le_bytes()));
                        let _ = put_end(&mut out, request.id, 2, "");
                        next = rx.recv().ok();
                        continue 'serving;
                    }
                    Message::Quit => return ExitCode::SUCCESS,
                }
            }
            rx.clear_if_none_waits();
            if let Ok(newer) = rx.try_recv() {
                // one came after the flag was cleared (the lock says none had been sent): take it and look again
                match newer {
                    Message::Request(r) => {
                        let old = std::mem::replace(&mut request, r);
                        let _ = out.write_all(b"VBEG").and_then(|()| out.write_all(&old.id.to_le_bytes()));
                        let _ = put_end(&mut out, old.id, 2, "");
                        continue;
                    }
                    Message::Bad(e) => {
                        let _ = put_end(&mut out, 0, 1, &e);
                    }
                    Message::Cancel => {
                        // a cancel sent after the request held here, before it began: it ends unrendered too
                        let _ = out.write_all(b"VBEG").and_then(|()| out.write_all(&request.id.to_le_bytes()));
                        let _ = put_end(&mut out, request.id, 2, "");
                        next = rx.recv().ok();
                        continue 'serving;
                    }
                    Message::Quit => return ExitCode::SUCCESS,
                }
            }
            break;
        }
        let id = request.id;
        if out.write_all(b"VBEG").and_then(|()| out.write_all(&id.to_le_bytes())).is_err() {
            return ExitCode::SUCCESS;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            serve_one(&request, substrate, &mut cached, &mut out)
        }))
        .unwrap_or_else(|_| Err("the render failed (a panic)".to_owned()));
        let sent = match result {
            Ok(sha) => put_end(&mut out, id, 0, &sha),
            Err(_) if stecyk3d::render::parallel::cancelled() => put_end(&mut out, id, 2, ""),
            Err(e) => put_end(&mut out, id, 1, &e),
        };
        if sent.is_err() {
            return ExitCode::SUCCESS;
        }
        next = rx.try_recv().ok().or_else(|| rx.recv().ok());
    }
}

/// One request: the description as the file says with the request's own members, rendered, its levels written.
fn serve_one(
    r: &Request,
    substrate: &substrate::Substrate,
    cached: &mut Option<((PathBuf, u64, std::time::SystemTime), stecyk3d::cases::json::Value)>,
    out: &mut std::io::BufWriter<std::io::StdoutLock<'_>>,
) -> Result<String, String> {
    use std::io::Write;
    use stecyk3d::cases::json::Value;
    let meta = std::fs::metadata(&r.description).map_err(|e| format!("cannot read {}: {e}", r.description.display()))?;
    let stamp = (r.description.clone(), meta.len(), meta.modified().map_err(|e| e.to_string())?);
    if cached.as_ref().is_none_or(|(k, _)| *k != stamp) {
        *cached = None;
        let text = std::fs::read_to_string(&r.description)
            .map_err(|e| format!("cannot read {}: {e}", r.description.display()))?;
        let v = stecyk3d::cases::json::parse(&text)
            .map_err(|e| format!("{}: the description is not JSON: {e}", r.description.display()))?;
        *cached = Some((stamp, v));
    }
    let Some((_, Value::Object(members))) = cached.as_mut() else {
        return Err(format!("{}: the description is not an object", r.description.display()));
    };
    for (key, value) in [("size", &r.size), ("camera", &r.camera), ("render", &r.render)] {
        if let Some(v) = value {
            members.insert(key.to_owned(), v.clone());
        }
    }
    let value = &cached.as_ref().expect("just made").1;
    let described = description::describe_value(value, &description::Tables::of(substrate))
        .map_err(|e| format!("{}: {e}", r.description.display()))?;
    let (scattered, rows) = match r.order.as_str() {
        "rows" => (Some(SCATTERED_SEED), true),
        "scattered" => (Some(SCATTERED_SEED), false),
        _ => (None, false),
    };
    let choices = described.choices(OpenChoices {
        scattered,
        scattered_rows: rows,
        threads: Some(r.threads),
        // The Rendered viewport is a live preview: unless the user chose Exact, every pixel draws from a place of its
        // own from the start (no exact try first), as the recomp always did.
        shared_draws: match r.draws.as_deref() {
            None | Some("auto") => stecyk3d::render::SharedDraws::Independent,
            other => shared_draws(true, other),
        },
        ..picture::choices_from(substrate)
    });
    let mut emit = |x: u32, y: u32, levels: &[u8]| {
        let sent = if levels.is_empty() {
            out.flush()
        } else {
            let n = (levels.len() / 3) as u32;
            out.write_all(RECORD_MAGIC).and_then(|()| {
                out.write_all(&x.to_le_bytes())?;
                out.write_all(&y.to_le_bytes())?;
                out.write_all(&n.to_le_bytes())?;
                out.write_all(levels)
            })
        };
        if sent.is_err() {
            std::process::exit(0); // Blender has gone
        }
    };
    let (picture, _counts) =
        render_lit_streamed(&described.scene, described.version, &choices, &mut emit).map_err(|e| e.to_string())?;
    std::fs::write(&r.levels, &picture.levels).map_err(|e| format!("cannot write {}: {e}", r.levels.display()))?;
    Ok(picture.levels_sha256_hex())
}
